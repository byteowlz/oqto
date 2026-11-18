//! The cache key of the contract's caching rules: one host, one Session, one `store`.

use serde_json::json;

use crate::canonical;

/// Which cache a client uses: one per host and Session. The cached record names its `store`;
/// a different `store` in `attached` means the cache is invalid and must be dropped.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CacheKey {
    /// Names the host, for example its WebSocket URL.
    pub host: String,
    /// The Session id.
    pub session: String,
}

impl CacheKey {
    /// A key for `host` and `session`.
    pub fn new(host: impl Into<String>, session: impl Into<String>) -> Self {
        Self {
            host: host.into(),
            session: session.into(),
        }
    }

    /// A stable file name for this key that reveals neither host nor Session.
    pub fn file_stem(&self) -> String {
        canonical::sha256(&json!([self.host, self.session]))
    }
}

/// Whether a cache recorded under `cached` may be kept for a host that now names `current`.
pub fn store_matches(cached: Option<&str>, current: &str) -> bool {
    cached.is_none_or(|store| store == current)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stem_depends_on_host_and_session() {
        let a = CacheKey::new("ws://127.0.0.1:1/session", "ses_a");
        let b = CacheKey::new("ws://127.0.0.1:1/session", "ses_b");
        assert_ne!(a.file_stem(), b.file_stem());
        assert_eq!(a.file_stem().len(), 64);
        assert!(store_matches(None, "st_1"));
        assert!(!store_matches(Some("st_0"), "st_1"));
    }
}
