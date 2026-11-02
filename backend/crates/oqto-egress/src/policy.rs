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

/// Whether an address is a *public internet* destination.
///
/// This is the guard for backend-side fetches of user-supplied URLs (feed
/// proxy, webhooks, provider probes). Loopback, private, link-local,
/// metadata, carrier-grade NAT, multicast and reserved ranges are not public:
/// reaching them from the backend bypasses the deployment's network position
/// and, for `169.254.169.254`, exfiltrates cloud credentials.
///
/// Callers must check every address a hostname resolves to, and re-check after
/// each redirect, because a name can resolve to a public address once and a
/// private one later (DNS rebinding).
pub fn is_public_destination(ip: std::net::IpAddr) -> bool {
    use std::net::IpAddr;

    match ip {
        IpAddr::V4(v4) => {
            !(v4.is_unspecified()
                || v4.is_loopback()
                || v4.is_private()
                || v4.is_link_local()
                || v4.is_broadcast()
                || v4.is_multicast()
                || v4.is_documentation()
                // Carrier-grade NAT / tailnet range: backend-position reachability.
                || (v4.octets()[0] == 100 && (64..128).contains(&v4.octets()[1]))
                // 198.18.0.0/15 benchmarking.
                || (v4.octets()[0] == 198 && (18..20).contains(&v4.octets()[1]))
                // 192.0.0.0/24 IETF protocol assignments.
                || (v4.octets()[0] == 192 && v4.octets()[1] == 0 && v4.octets()[2] == 0)
                // 240.0.0.0/4 reserved, and 0.0.0.0/8 "this network".
                || v4.octets()[0] >= 240
                || v4.octets()[0] == 0)
        }
        IpAddr::V6(v6) => {
            // IPv4-mapped/compatible forms must be judged as the v4 address.
            if let Some(mapped) = v6.to_ipv4_mapped() {
                return is_public_destination(IpAddr::V4(mapped));
            }
            !(v6.is_unspecified()
                || v6.is_loopback()
                || v6.is_multicast()
                // fc00::/7 unique local.
                || (v6.segments()[0] & 0xfe00) == 0xfc00
                // fe80::/10 link local.
                || (v6.segments()[0] & 0xffc0) == 0xfe80
                // 64:ff9b::/96 NAT64 maps arbitrary v4; treat as non-public.
                || (v6.segments()[0] == 0x0064
                    && v6.segments()[1] == 0xff9b
                    && v6.segments()[2..6].iter().all(|s| *s == 0)))
        }
    }
}

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

    #[test]
    fn public_destination_classification() {
        use std::net::IpAddr;
        let forbidden = [
            "127.0.0.1",
            "127.9.9.9",
            "0.0.0.0",
            "10.1.2.3",
            "172.16.0.1",
            "192.168.1.1",
            "169.254.169.254",
            "169.254.1.1",
            "100.64.0.12",
            "198.18.0.1",
            "224.0.0.1",
            "240.0.0.1",
            "192.0.0.8",
            "::1",
            "::",
            "fc00::1",
            "fd00::1",
            "fe80::1",
            "ff02::1",
            "::ffff:127.0.0.1",
            "64:ff9b::7f00:1",
        ];
        for ip in forbidden {
            let addr: IpAddr = ip.parse().expect("ip");
            assert!(
                !super::is_public_destination(addr),
                "{ip} must not be treated as a public destination"
            );
        }

        for ip in ["1.1.1.1", "93.184.216.34", "2606:2800:220:1::1"] {
            let addr: IpAddr = ip.parse().expect("ip");
            assert!(
                super::is_public_destination(addr),
                "{ip} must be treated as public"
            );
        }
    }
}
