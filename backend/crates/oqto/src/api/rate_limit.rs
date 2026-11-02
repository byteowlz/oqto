//! Per-IP rate limiting for unauthenticated auth endpoints.
//!
//! Guards `/auth/login`, `/auth/register`, and `/auth/dev-login` against
//! credential brute-force. Uses an in-memory sliding window keyed by client IP.
//!
//! Forwarded headers are only honoured when the transport peer is a configured
//! trusted proxy (`[auth] trusted_proxies`). Without that check a directly
//! reachable backend lets any client choose its own bucket by sending
//! `X-Forwarded-For`, which defeats the limiter entirely.

use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::{
    body::Body,
    extract::{ConnectInfo, State},
    http::{Request, StatusCode, header::HeaderMap},
    middleware::Next,
    response::{IntoResponse, Response},
};
use dashmap::DashMap;

/// Sliding-window rate limiter keyed by client IP.
#[derive(Clone)]
pub struct AuthRateLimiter {
    inner: Arc<Inner>,
}

struct Inner {
    hits: DashMap<IpAddr, Vec<Instant>>,
    max_requests: usize,
    window: Duration,
    trusted_proxies: Vec<IpAddr>,
}

impl AuthRateLimiter {
    /// Allow `max_requests` per `window` per IP.
    ///
    /// `trusted_proxies` are peer addresses whose forwarded headers may be
    /// honoured; empty means trust nothing and key on the transport peer.
    pub fn new(max_requests: usize, window: Duration, trusted_proxies: Vec<IpAddr>) -> Self {
        Self {
            inner: Arc::new(Inner {
                hits: DashMap::new(),
                max_requests,
                window,
                trusted_proxies,
            }),
        }
    }

    /// Record a hit for `ip`. Returns `Err(retry_after)` when the limit is exceeded.
    fn check(&self, ip: IpAddr) -> Result<(), Duration> {
        let now = Instant::now();
        let window = self.inner.window;
        let max = self.inner.max_requests;

        let mut entry = self.inner.hits.entry(ip).or_default();
        entry.retain(|t| now.duration_since(*t) < window);

        if entry.len() >= max {
            let oldest = entry.first().copied().unwrap_or(now);
            let retry_after = window.saturating_sub(now.duration_since(oldest));
            return Err(retry_after);
        }

        entry.push(now);
        Ok(())
    }
}

/// Extract the client IP.
///
/// Forwarded headers are attacker-controlled text. They are only consulted when
/// the transport peer is a configured trusted proxy; otherwise the peer address
/// is the only evidence we have.
fn client_ip(
    headers: &HeaderMap,
    peer: Option<SocketAddr>,
    trusted_proxies: &[IpAddr],
) -> Option<IpAddr> {
    let peer_ip = peer.map(|addr| addr.ip());
    let peer_is_trusted = peer_ip.is_some_and(|ip| trusted_proxies.contains(&ip));

    if peer_is_trusted {
        if let Some(forwarded) = headers.get("x-forwarded-for").and_then(|v| v.to_str().ok())
            && let Some(ip) = forwarded
                .split(',')
                .rev()
                .filter_map(|p| p.trim().parse::<IpAddr>().ok())
                .next()
        {
            return Some(ip);
        }

        if let Some(real_ip) = headers
            .get("x-real-ip")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.trim().parse::<IpAddr>().ok())
        {
            return Some(real_ip);
        }
    }

    peer_ip
}

/// Axum middleware enforcing the per-IP auth rate limit.
pub async fn rate_limit_middleware(
    State(limiter): State<AuthRateLimiter>,
    req: Request<Body>,
    next: Next,
) -> Response {
    let peer = req
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|ci| ci.0);

    let Some(ip) = client_ip(req.headers(), peer, &limiter.inner.trusted_proxies) else {
        // Fail closed only on a genuinely unknown source is undesirable for
        // availability; without an IP we cannot bucket, so allow through.
        return next.run(req).await;
    };

    if let Err(retry_after) = limiter.check(ip) {
        let secs = retry_after.as_secs().max(1);
        return (
            StatusCode::TOO_MANY_REQUESTS,
            [("Retry-After", secs.to_string())],
            "Too many attempts. Please wait before retrying.",
        )
            .into_response();
    }

    next.run(req).await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn limiter(max: usize) -> AuthRateLimiter {
        AuthRateLimiter::new(max, Duration::from_secs(60), Vec::new())
    }

    #[test]
    fn allows_up_to_limit_then_blocks() {
        let limiter = limiter(3);
        let ip: IpAddr = "203.0.113.7".parse().unwrap();

        assert!(limiter.check(ip).is_ok());
        assert!(limiter.check(ip).is_ok());
        assert!(limiter.check(ip).is_ok());
        assert!(limiter.check(ip).is_err());
    }

    #[test]
    fn separate_ips_have_separate_budgets() {
        let limiter = limiter(1);
        let a: IpAddr = "203.0.113.1".parse().unwrap();
        let b: IpAddr = "203.0.113.2".parse().unwrap();

        assert!(limiter.check(a).is_ok());
        assert!(limiter.check(a).is_err());
        assert!(limiter.check(b).is_ok());
    }

    #[test]
    fn trusted_proxy_forwarded_for_takes_last_hop() {
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", "1.1.1.1, 203.0.113.9".parse().unwrap());
        let peer: SocketAddr = "127.0.0.1:5000".parse().unwrap();
        let trusted: Vec<IpAddr> = vec!["127.0.0.1".parse().unwrap()];

        let ip = client_ip(&headers, Some(peer), &trusted).unwrap();
        assert_eq!(ip, "203.0.113.9".parse::<IpAddr>().unwrap());
    }

    #[test]
    fn untrusted_peer_cannot_spoof_forwarded_for() {
        // Directly connected client: the header must be ignored entirely.
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", "198.51.100.77".parse().unwrap());
        headers.insert("x-real-ip", "198.51.100.78".parse().unwrap());
        let peer: SocketAddr = "203.0.113.50:5000".parse().unwrap();

        let ip = client_ip(&headers, Some(peer), &[]).unwrap();
        assert_eq!(ip, "203.0.113.50".parse::<IpAddr>().unwrap());
    }

    #[test]
    fn untrusted_peer_rotating_headers_shares_one_bucket() {
        let limiter = limiter(2);
        let peer: SocketAddr = "203.0.113.50:5000".parse().unwrap();
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", "10.0.0.1".parse().unwrap());

        let key = |h: &HeaderMap| client_ip(h, Some(peer), &limiter.inner.trusted_proxies).unwrap();

        assert!(limiter.check(key(&headers)).is_ok());
        headers.insert("x-forwarded-for", "10.0.0.2".parse().unwrap());
        assert!(limiter.check(key(&headers)).is_ok());
        headers.insert("x-forwarded-for", "10.0.0.3".parse().unwrap());
        assert!(
            limiter.check(key(&headers)).is_err(),
            "rotating a spoofed header must not mint fresh buckets"
        );
    }

    #[test]
    fn falls_back_to_peer_without_headers() {
        let headers = HeaderMap::new();
        let peer: SocketAddr = "198.51.100.4:5000".parse().unwrap();

        let ip = client_ip(&headers, Some(peer), &[]).unwrap();
        assert_eq!(ip, "198.51.100.4".parse::<IpAddr>().unwrap());
    }
}
