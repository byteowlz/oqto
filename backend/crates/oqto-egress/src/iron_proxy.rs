use crate::enforcer::{CompiledPolicy, EgressEnforcer};
use crate::policy::{Protocol, Verdict, WorkspaceEgressPolicy};
use crate::tier::EgressTier;
use anyhow::{Context, Result};
use serde::Serialize;

/// Where the enforcer listens for this workspace, as seen from the placement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProxyEndpoint {
    pub host: String,
    pub port: u16,
    /// PEM path inside the placement for the enforcer's TLS-interception CA.
    pub ca_bundle_path: Option<String>,
}

impl ProxyEndpoint {
    pub fn url(&self) -> String {
        format!("http://{}:{}", self.host, self.port)
    }
}

#[derive(Debug, Serialize)]
struct IronProxyConfig {
    allowlist: Allowlist,
    audit: Audit,
}

#[derive(Debug, Serialize)]
struct Allowlist {
    domains: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    ports: Vec<u16>,
    deny_cidrs: Vec<String>,
}

#[derive(Debug, Serialize)]
struct Audit {
    format: &'static str,
    include_request_headers: bool,
}

/// Compiles [`WorkspaceEgressPolicy`] into iron-proxy YAML.
#[derive(Debug, Clone)]
pub struct IronProxyEnforcer {
    endpoint: ProxyEndpoint,
}

impl IronProxyEnforcer {
    pub fn new(endpoint: ProxyEndpoint) -> Self {
        Self { endpoint }
    }

    pub fn endpoint(&self) -> &ProxyEndpoint {
        &self.endpoint
    }

    fn domains(policy: &WorkspaceEgressPolicy) -> Vec<String> {
        match policy.verdict {
            // iron-proxy still audits every flow under a total wildcard, which
            // is what separates `open_attributable` from `open`.
            Verdict::Open | Verdict::OpenAttributable => vec!["*".to_string()],
            Verdict::Allowlist => policy
                .destinations
                .iter()
                .map(|rule| rule.host.clone())
                .collect(),
        }
    }

    fn non_default_ports(policy: &WorkspaceEgressPolicy) -> Vec<u16> {
        let mut ports: Vec<u16> = policy
            .destinations
            .iter()
            .flat_map(|rule| rule.ports.iter().copied())
            .filter(|port| {
                *port != Protocol::Http.default_port() && *port != Protocol::Https.default_port()
            })
            .collect();
        ports.sort_unstable();
        ports.dedup();
        ports
    }
}

impl EgressEnforcer for IronProxyEnforcer {
    fn name(&self) -> &'static str {
        "iron-proxy"
    }

    fn max_tier(&self) -> EgressTier {
        // The proxy itself only sees cooperative clients. Placing it behind a
        // network=none placement is what raises the binding to isolated, and
        // that is the placement's property, not the proxy's.
        EgressTier::Transparent
    }

    fn compile(&self, policy: &WorkspaceEgressPolicy) -> Result<CompiledPolicy> {
        let config = IronProxyConfig {
            allowlist: Allowlist {
                domains: Self::domains(policy),
                ports: Self::non_default_ports(policy),
                deny_cidrs: policy.deny_cidrs(),
            },
            audit: Audit {
                format: "json",
                include_request_headers: false,
            },
        };

        let yaml = serde_yaml::to_string(&config)
            .with_context(|| format!("compiling egress policy for {}", policy.workspace_id))?;

        let url = self.endpoint.url();
        let mut client_env = vec![
            ("HTTP_PROXY".to_string(), url.clone()),
            ("HTTPS_PROXY".to_string(), url.clone()),
            ("http_proxy".to_string(), url.clone()),
            ("https_proxy".to_string(), url),
            ("NO_PROXY".to_string(), "localhost,127.0.0.1".to_string()),
        ];
        if let Some(ca) = &self.endpoint.ca_bundle_path {
            // Tools disagree on which variable they read, so set the ones the
            // workspace image's toolchains actually consult.
            for var in [
                "SSL_CERT_FILE",
                "REQUESTS_CA_BUNDLE",
                "NODE_EXTRA_CA_CERTS",
                "CARGO_HTTP_CAINFO",
                "GIT_SSL_CAINFO",
            ] {
                client_env.push((var.to_string(), ca.clone()));
            }
        }

        Ok(CompiledPolicy {
            enforcer: self.name(),
            config: yaml,
            client_env,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::policy::DestinationRule;

    fn enforcer() -> IronProxyEnforcer {
        IronProxyEnforcer::new(ProxyEndpoint {
            host: "127.0.0.1".to_string(),
            port: 18080,
            ca_bundle_path: Some("/etc/oqto/egress-ca.pem".to_string()),
        })
    }

    #[test]
    fn denied_policy_compiles_to_an_empty_allowlist() {
        let compiled = enforcer()
            .compile(&WorkspaceEgressPolicy::denied("ws-1"))
            .expect("compile");
        assert!(compiled.config.contains("domains: []"));
    }

    #[test]
    fn allowlist_carries_only_listed_hosts() {
        let policy = WorkspaceEgressPolicy::denied("ws-1").with_destinations(vec![
            DestinationRule::https("github.com"),
            DestinationRule::https("*.crates.io"),
        ]);
        let compiled = enforcer().compile(&policy).expect("compile");
        assert!(compiled.config.contains("github.com"));
        assert!(compiled.config.contains("*.crates.io"));
        assert!(!compiled.config.contains("'*'"));
    }

    #[test]
    fn mandatory_deny_cidrs_survive_compilation() {
        let compiled = enforcer()
            .compile(&WorkspaceEgressPolicy::denied("ws-1"))
            .expect("compile");
        assert!(compiled.config.contains("169.254.169.254/32"));
        assert!(compiled.config.contains("127.0.0.0/8"));
    }

    #[test]
    fn open_attributable_compiles_to_a_wildcard_that_still_audits() {
        let mut policy = WorkspaceEgressPolicy::denied("ws-1");
        policy.verdict = Verdict::OpenAttributable;
        let compiled = enforcer().compile(&policy).expect("compile");
        assert!(compiled.config.contains('*'));
        assert!(compiled.config.contains("format: json"));
    }

    #[test]
    fn client_env_points_at_the_endpoint_and_ca() {
        let compiled = enforcer()
            .compile(&WorkspaceEgressPolicy::denied("ws-1"))
            .expect("compile");
        let env: std::collections::HashMap<_, _> = compiled.client_env.into_iter().collect();
        assert_eq!(env["HTTPS_PROXY"], "http://127.0.0.1:18080");
        assert_eq!(env["NODE_EXTRA_CA_CERTS"], "/etc/oqto/egress-ca.pem");
        assert_eq!(env["CARGO_HTTP_CAINFO"], "/etc/oqto/egress-ca.pem");
    }

    #[test]
    fn extra_ports_are_declared_once_and_sorted() {
        let mut rule = DestinationRule::https("registry.internal");
        rule.ports = vec![8443, 443, 8443];
        let policy = WorkspaceEgressPolicy::denied("ws-1").with_destinations(vec![rule]);
        let compiled = enforcer().compile(&policy).expect("compile");
        assert_eq!(compiled.config.matches("8443").count(), 1);
    }

    #[test]
    fn the_proxy_alone_never_claims_isolation() {
        assert_eq!(enforcer().max_tier(), EgressTier::Transparent);
    }
}
