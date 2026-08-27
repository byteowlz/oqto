use crate::enforcer::{CompiledPolicy, EgressEnforcer};
use crate::policy::{Protocol, Verdict, WorkspaceEgressPolicy};
use crate::tier::EgressTier;
use anyhow::{Context, Result};
use serde::Serialize;

/// Where the enforcer listens for this workspace, as seen from the placement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProxyEndpoint {
    pub host: String,
    /// The proxy port as seen from inside the placement, backed by an endpoint
    /// bridge to [`ProxyRuntime::tunnel_listen_port`] on the host.
    pub port: u16,
    /// PEM path inside the placement for the enforcer's TLS-interception CA.
    pub ca_bundle_path: Option<String>,
}

impl ProxyEndpoint {
    pub fn url(&self) -> String {
        format!("http://{}:{}", self.host, self.port)
    }
}

/// Host-side material the proxy process needs, distinct from the placement's
/// view of the endpoint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProxyRuntime {
    /// Loopback port the proxy binds on the host. Never a privileged port:
    /// the proxy runs unprivileged.
    pub http_listen_port: u16,
    pub https_listen_port: u16,
    /// The CONNECT/SOCKS5 listener. Clients using HTTP_PROXY/HTTPS_PROXY must
    /// target this port: http_listen handles only direct forward-proxy HTTP,
    /// and a CONNECT arriving there is misinterpreted as a plain request.
    pub tunnel_listen_port: u16,
    pub ca_cert_path: String,
    pub ca_key_path: String,
}

#[derive(Debug, Serialize)]
struct IronProxyConfig {
    dns: Dns,
    proxy: Proxy,
    tls: Tls,
    transforms: Vec<Transform>,
}

#[derive(Debug, Serialize)]
struct Dns {
    /// Clients reach the proxy through explicit proxy environment, so the
    /// built-in DNS interceptor stays off and needs no privileged port.
    enabled: bool,
}

#[derive(Debug, Serialize)]
struct Proxy {
    http_listen: String,
    https_listen: String,
    tunnel_listen: String,
    upstream_deny_cidrs: Vec<String>,
}

#[derive(Debug, Serialize)]
struct Tls {
    mode: &'static str,
    ca_cert: String,
    ca_key: String,
}

#[derive(Debug, Serialize)]
struct Transform {
    name: &'static str,
    config: AllowlistConfig,
}

#[derive(Debug, Serialize)]
struct AllowlistConfig {
    domains: Vec<String>,
}

/// Compiles [`WorkspaceEgressPolicy`] into iron-proxy YAML.
#[derive(Debug, Clone)]
pub struct IronProxyEnforcer {
    endpoint: ProxyEndpoint,
    runtime: ProxyRuntime,
}

impl IronProxyEnforcer {
    pub fn new(endpoint: ProxyEndpoint, runtime: ProxyRuntime) -> Self {
        Self { endpoint, runtime }
    }

    pub fn endpoint(&self) -> &ProxyEndpoint {
        &self.endpoint
    }

    pub fn runtime(&self) -> &ProxyRuntime {
        &self.runtime
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

    /// Hosts carrying a non-default port are emitted as `host:port` so the
    /// allowlist does not silently widen to every port on that host.
    fn domains_with_ports(policy: &WorkspaceEgressPolicy) -> Vec<String> {
        let mut domains = Self::domains(policy);
        if policy.verdict == Verdict::Allowlist {
            for rule in &policy.destinations {
                for port in &rule.ports {
                    if *port != Protocol::Http.default_port()
                        && *port != Protocol::Https.default_port()
                    {
                        domains.push(format!("{}:{}", rule.host, port));
                    }
                }
            }
        }
        domains.sort();
        domains.dedup();
        domains
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
            dns: Dns { enabled: false },
            proxy: Proxy {
                http_listen: format!(":{}", self.runtime.http_listen_port),
                https_listen: format!(":{}", self.runtime.https_listen_port),
                tunnel_listen: format!(":{}", self.runtime.tunnel_listen_port),
                upstream_deny_cidrs: policy.deny_cidrs(),
            },
            tls: Tls {
                mode: "mitm",
                ca_cert: self.runtime.ca_cert_path.clone(),
                ca_key: self.runtime.ca_key_path.clone(),
            },
            transforms: vec![Transform {
                name: "allowlist",
                config: AllowlistConfig {
                    domains: Self::domains_with_ports(policy),
                },
            }],
        };

        let yaml = serde_yaml::to_string(&config)
            .with_context(|| format!("compiling egress policy for {}", policy.workspace_id))?;

        // Clients live inside the placement, where the only reachable proxy
        // address is the bridged endpoint port; the supervisor forwards that
        // to the proxy's tunnel listener on the host.
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
        IronProxyEnforcer::new(
            ProxyEndpoint {
                host: "127.0.0.1".to_string(),
                port: 18080,
                ca_bundle_path: Some("/etc/oqto/egress-ca.pem".to_string()),
            },
            ProxyRuntime {
                http_listen_port: 18080,
                https_listen_port: 18443,
                tunnel_listen_port: 18082,
                ca_cert_path: "/var/lib/oqto/egress/ca.crt".to_string(),
                ca_key_path: "/var/lib/oqto/egress/ca.key".to_string(),
            },
        )
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
    fn open_attributable_compiles_to_a_wildcard() {
        let mut policy = WorkspaceEgressPolicy::denied("ws-1");
        policy.verdict = Verdict::OpenAttributable;
        let compiled = enforcer().compile(&policy).expect("compile");
        assert!(compiled.config.contains('*'));
    }

    #[test]
    fn client_env_points_at_the_endpoint_and_ca() {
        let compiled = enforcer()
            .compile(&WorkspaceEgressPolicy::denied("ws-1"))
            .expect("compile");
        let env: std::collections::HashMap<_, _> = compiled.client_env.into_iter().collect();
        assert_eq!(
            env["HTTPS_PROXY"], "http://127.0.0.1:18080",
            "clients target the bridged endpoint port inside the placement"
        );
        assert_eq!(env["NODE_EXTRA_CA_CERTS"], "/etc/oqto/egress-ca.pem");
        assert_eq!(env["CARGO_HTTP_CAINFO"], "/etc/oqto/egress-ca.pem");
    }

    #[test]
    fn a_non_default_port_is_bound_to_its_host_not_opened_globally() {
        let mut rule = DestinationRule::https("registry.internal");
        rule.ports = vec![8443, 443, 8443];
        let policy = WorkspaceEgressPolicy::denied("ws-1").with_destinations(vec![rule]);
        let compiled = enforcer().compile(&policy).expect("compile");
        assert!(compiled.config.contains("registry.internal:8443"));
        assert_eq!(compiled.config.matches("registry.internal:8443").count(), 1);
    }

    #[test]
    fn the_proxy_binds_unprivileged_ports_and_leaves_dns_off() {
        let compiled = enforcer()
            .compile(&WorkspaceEgressPolicy::denied("ws-1"))
            .expect("compile");
        assert!(compiled.config.contains("http_listen: :18080"));
        assert!(compiled.config.contains("https_listen: :18443"));
        assert!(compiled.config.contains("tunnel_listen: :18082"));
        assert!(compiled.config.contains("enabled: false"));
    }

    #[test]
    fn the_proxy_alone_never_claims_isolation() {
        assert_eq!(enforcer().max_tier(), EgressTier::Transparent);
    }
}
