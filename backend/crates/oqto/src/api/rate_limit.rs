//! Per-IP rate limiting for unauthenticated auth endpoints.
//!
//! Guards `/auth/login`, `/auth/register`, and `/auth/dev-login` against
//! credential brute-force. Uses an in-memory sliding window keyed by client IP.
//! Behind the deploy Caddy reverse proxy the peer socket is always loopback, so
//! the client IP is taken from the last `X-Forwarded-For` hop (the entry Caddy
//! appends is the untrusted client and cannot be spoofed past the trusted proxy).

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
}

impl AuthRateLimiter {
    /// Allow `max_requests` per `window` per IP.
    pub fn new(max_requests: usize, window: Duration) -> Self {
        Self {
            inner: Arc::new(Inner {
                hits: DashMap::new(),
                max_requests,
                window,
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

/// Extract the client IP, preferring the reverse-proxy-supplied forwarded chain.
fn client_ip(headers: &HeaderMap, peer: Option<SocketAddr>) -> Option<IpAddr> {
    if let Some(forwarded) = headers.get("x-forwarded-for").and_then(|v| v.to_str().ok()) {
        // With a single trusted proxy hop, the rightmost entry is the address the
        // proxy observed for the immediate client; earlier entries are spoofable.
        if let Some(ip) = forwarded
            .split(',')
            .rev()
            .filter_map(|p| p.trim().parse::<IpAddr>().ok())
            .next()
        {
            return Some(ip);
        }
    }

    if let Some(real_ip) = headers
        .get("x-real-ip")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.trim().parse::<IpAddr>().ok())
    {
        return Some(real_ip);
    }

    peer.map(|addr| addr.ip())
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

    let Some(ip) = client_ip(req.headers(), peer) else {
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

    #[test]
    fn allows_up_to_limit_then_blocks() {
        let limiter = AuthRateLimiter::new(3, Duration::from_secs(60));
        let ip: IpAddr = "203.0.113.7".parse().unwrap();

        assert!(limiter.check(ip).is_ok());
        assert!(limiter.check(ip).is_ok());
        assert!(limiter.check(ip).is_ok());
        assert!(limiter.check(ip).is_err());
    }

    #[test]
    fn separate_ips_have_separate_budgets() {
        let limiter = AuthRateLimiter::new(1, Duration::from_secs(60));
        let a: IpAddr = "203.0.113.1".parse().unwrap();
        let b: IpAddr = "203.0.113.2".parse().unwrap();

        assert!(limiter.check(a).is_ok());
        assert!(limiter.check(a).is_err());
        assert!(limiter.check(b).is_ok());
    }

    #[test]
    fn forwarded_for_takes_last_hop() {
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", "1.1.1.1, 203.0.113.9".parse().unwrap());
        let peer: SocketAddr = "127.0.0.1:5000".parse().unwrap();

        let ip = client_ip(&headers, Some(peer)).unwrap();
        assert_eq!(ip, "203.0.113.9".parse::<IpAddr>().unwrap());
    }

    #[test]
    fn falls_back_to_peer_without_headers() {
        let headers = HeaderMap::new();
        let peer: SocketAddr = "198.51.100.4:5000".parse().unwrap();

        let ip = client_ip(&headers, Some(peer)).unwrap();
        assert_eq!(ip, "198.51.100.4".parse::<IpAddr>().unwrap());
    }
}
