use serde::{Deserialize, Serialize};

/// What a workspace is allowed to reach, independent of which enforcer runs it.
///
/// ADR-0035 separates three things that are easy to conflate: what the policy
/// permits (this type), which enforcement tier actually binds it
/// ([`crate::tier::EgressTier`]), and who attests the claim.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    /// Only listed destinations are reachable.
    #[default]
    Allowlist,
    /// Anything reachable, but every flow is attributed in the audit stream.
    OpenAttributable,
    /// Anything reachable, attribution not guaranteed.
    Open,
}

/// Protocols an enforcer may be asked to carry.
///
/// One enforcer never sees all traffic: an HTTP proxy closes raw TCP, so SSH
/// and other protocols route to their own capability endpoints or are denied.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Protocol {
    Http,
    Https,
}

impl Protocol {
    pub fn default_port(self) -> u16 {
        match self {
            Self::Http => 80,
            Self::Https => 443,
        }
    }
}

/// A destination the workspace may reach.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DestinationRule {
    /// Hostname, or `*.suffix` for a subdomain wildcard.
    pub host: String,
    #[serde(default = "DestinationRule::default_protocols")]
    pub protocols: Vec<Protocol>,
    /// Ports beyond the protocol defaults. Empty means defaults only.
    #[serde(default)]
    pub ports: Vec<u16>,
}

impl DestinationRule {
    fn default_protocols() -> Vec<Protocol> {
        vec![Protocol::Https]
    }

    pub fn https(host: impl Into<String>) -> Self {
        Self {
            host: host.into(),
            protocols: Self::default_protocols(),
            ports: Vec::new(),
        }
    }

    /// Wildcards match subdomains only, never the bare parent and never a
    /// suffix that merely ends with the same characters.
    pub fn matches_host(&self, candidate: &str) -> bool {
        let candidate = candidate.trim_end_matches('.').to_ascii_lowercase();
        let pattern = self.host.trim_end_matches('.').to_ascii_lowercase();
        match pattern.strip_prefix("*.") {
            Some(suffix) => candidate
                .strip_suffix(suffix)
                .is_some_and(|head| head.ends_with('.') && head.len() > 1),
            None => candidate == pattern,
        }
    }

    pub fn allows(&self, protocol: Protocol, port: u16) -> bool {
        self.protocols.contains(&protocol)
            && (port == protocol.default_port() || self.ports.contains(&port))
    }
}

/// Addresses no workspace may reach regardless of verdict, closing SSRF and
/// DNS-rebinding paths to cloud metadata and host-local services.
pub const MANDATORY_DENY_CIDRS: &[&str] = &[
    "169.254.169.254/32",
    "fd00:ec2::254/128",
    "fd20:ce::254/128",
    "127.0.0.0/8",
    "::1/128",
    "169.254.0.0/16",
    "fe80::/10",
];

/// The canonical policy for one workspace.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceEgressPolicy {
    pub workspace_id: String,
    pub verdict: Verdict,
    #[serde(default)]
    pub destinations: Vec<DestinationRule>,
    /// Deny CIDRs beyond [`MANDATORY_DENY_CIDRS`], which are always applied.
    #[serde(default)]
    pub extra_deny_cidrs: Vec<String>,
    /// Provider-side fetching (`file_url`, `image_url`, server-side web tools)
    /// is a second hop no network enforcer can observe, so it is a separate
    /// permission carried on the session key rather than a destination rule.
    #[serde(default)]
    pub delegated_fetch: bool,
}

impl WorkspaceEgressPolicy {
    /// A policy that permits nothing. Used whenever configuration is absent,
    /// unreadable, or invalid, so failure narrows access instead of widening it.
    pub fn denied(workspace_id: impl Into<String>) -> Self {
        Self {
            workspace_id: workspace_id.into(),
            verdict: Verdict::Allowlist,
            destinations: Vec::new(),
            extra_deny_cidrs: Vec::new(),
            delegated_fetch: false,
        }
    }

    pub fn with_destinations(mut self, destinations: Vec<DestinationRule>) -> Self {
        self.destinations = destinations;
        self
    }

    pub fn deny_cidrs(&self) -> Vec<String> {
        MANDATORY_DENY_CIDRS
            .iter()
            .map(|cidr| (*cidr).to_string())
            .chain(self.extra_deny_cidrs.iter().cloned())
            .collect()
    }

    /// Whether the policy text permits this destination. This is the policy's
    /// own answer; only a bound enforcer makes it true of the running workload.
    pub fn permits(&self, host: &str, protocol: Protocol, port: u16) -> bool {
        match self.verdict {
            Verdict::Open | Verdict::OpenAttributable => true,
            Verdict::Allowlist => self
                .destinations
                .iter()
                .any(|rule| rule.matches_host(host) && rule.allows(protocol, port)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn absent_configuration_permits_nothing() {
        let policy = WorkspaceEgressPolicy::denied("ws-1");
        assert!(!policy.permits("example.com", Protocol::Https, 443));
    }

    #[test]
    fn wildcard_matches_subdomains_but_not_parent_or_lookalike() {
        let rule = DestinationRule::https("*.github.com");
        assert!(rule.matches_host("api.github.com"));
        assert!(rule.matches_host("a.b.github.com"));
        assert!(!rule.matches_host("github.com"));
        assert!(!rule.matches_host("evilgithub.com"));
        assert!(!rule.matches_host("github.com.evil.net"));
    }

    #[test]
    fn host_matching_ignores_case_and_trailing_dot() {
        let rule = DestinationRule::https("Example.COM");
        assert!(rule.matches_host("example.com."));
    }

    #[test]
    fn ports_outside_the_protocol_default_need_an_explicit_rule() {
        let mut rule = DestinationRule::https("registry.internal");
        assert!(rule.allows(Protocol::Https, 443));
        assert!(!rule.allows(Protocol::Https, 8443));
        rule.ports.push(8443);
        assert!(rule.allows(Protocol::Https, 8443));
    }

    #[test]
    fn protocol_not_listed_is_denied() {
        let rule = DestinationRule::https("example.com");
        assert!(!rule.allows(Protocol::Http, 80));
    }

    #[test]
    fn metadata_and_loopback_are_always_denied() {
        let policy = WorkspaceEgressPolicy::denied("ws-1");
        let cidrs = policy.deny_cidrs();
        for required in ["169.254.169.254/32", "127.0.0.0/8", "::1/128"] {
            assert!(cidrs.contains(&required.to_string()), "missing {required}");
        }
    }

    #[test]
    fn open_verdicts_permit_unlisted_hosts() {
        let mut policy = WorkspaceEgressPolicy::denied("ws-1");
        policy.verdict = Verdict::OpenAttributable;
        assert!(policy.permits("anything.example", Protocol::Https, 443));
    }

    #[test]
    fn delegated_fetch_defaults_to_denied() {
        assert!(!WorkspaceEgressPolicy::denied("ws-1").delegated_fetch);
    }
}
