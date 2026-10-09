use crate::policy::{DestinationRule, Protocol, Verdict, WorkspaceEgressPolicy};
use serde::Deserialize;
use std::path::{Path, PathBuf};
use tracing::warn;

/// The `[egress]` table of a work directory's `.oqto/config.toml`.
#[derive(Debug, Clone, Deserialize, Default, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct EgressConfig {
    #[serde(default)]
    pub verdict: Verdict,
    /// Hostnames, optionally `*.suffix`, reachable over HTTPS.
    #[serde(default)]
    pub allow: Vec<String>,
    /// Destinations needing a protocol or port beyond HTTPS on 443.
    #[serde(default)]
    pub destinations: Vec<DestinationEntry>,
    #[serde(default)]
    pub deny_cidrs: Vec<String>,
    /// Provider-side fetching on this workspace's behalf. Denied by default.
    #[serde(default)]
    pub delegated_fetch: bool,
}

#[derive(Debug, Clone, Deserialize, Default, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DestinationEntry {
    pub host: String,
    #[serde(default)]
    pub protocols: Vec<Protocol>,
    #[serde(default)]
    pub ports: Vec<u16>,
}

#[derive(Debug, Deserialize, Default)]
struct WorkDirConfig {
    #[serde(default)]
    egress: Option<EgressConfig>,
}

impl EgressConfig {
    pub fn into_policy(self, workspace_id: impl Into<String>) -> WorkspaceEgressPolicy {
        let mut destinations: Vec<DestinationRule> =
            self.allow.into_iter().map(DestinationRule::https).collect();
        destinations.extend(self.destinations.into_iter().map(|entry| DestinationRule {
            protocols: if entry.protocols.is_empty() {
                vec![Protocol::Https]
            } else {
                entry.protocols
            },
            host: entry.host,
            ports: entry.ports,
        }));

        WorkspaceEgressPolicy {
            workspace_id: workspace_id.into(),
            verdict: self.verdict,
            destinations,
            extra_deny_cidrs: self.deny_cidrs,
            delegated_fetch: self.delegated_fetch,
        }
    }
}

pub fn config_path(workspace: &Path) -> PathBuf {
    workspace.join(".oqto").join("config.toml")
}

/// Loads the egress policy for a work directory.
///
/// A missing, unreadable, or invalid config yields a policy that permits
/// nothing, so a broken file narrows access instead of widening it. An unknown
/// key is an error rather than a silently ignored intention.
pub fn load_policy(workspace: &Path, workspace_id: impl Into<String>) -> WorkspaceEgressPolicy {
    let workspace_id = workspace_id.into();
    let path = config_path(workspace);
    if !path.exists() {
        return WorkspaceEgressPolicy::denied(workspace_id);
    }

    let raw = match std::fs::read_to_string(&path) {
        Ok(raw) => raw,
        Err(err) => {
            warn!(
                "egress: cannot read {}: {err}. Denying all egress.",
                path.display()
            );
            return WorkspaceEgressPolicy::denied(workspace_id);
        }
    };

    match toml::from_str::<WorkDirConfig>(&raw) {
        Ok(cfg) => cfg
            .egress
            .unwrap_or_default()
            .into_policy(workspace_id.clone()),
        Err(err) => {
            warn!(
                "egress: cannot parse {}: {err}. Denying all egress.",
                path.display()
            );
            WorkspaceEgressPolicy::denied(workspace_id)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn workspace_with(config: &str) -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("tempdir");
        fs::create_dir_all(dir.path().join(".oqto")).expect("mkdir");
        fs::write(config_path(dir.path()), config).expect("write");
        dir
    }

    #[test]
    fn missing_config_denies_everything() {
        let dir = tempfile::tempdir().expect("tempdir");
        let policy = load_policy(dir.path(), "ws-1");
        assert!(!policy.permits("github.com", Protocol::Https, 443));
        assert!(!policy.delegated_fetch);
    }

    #[test]
    fn invalid_toml_denies_everything() {
        let dir = workspace_with("[egress]\nallow = \"not-a-list\"\n");
        let policy = load_policy(dir.path(), "ws-1");
        assert!(!policy.permits("github.com", Protocol::Https, 443));
    }

    #[test]
    fn an_unknown_key_denies_rather_than_being_ignored() {
        let dir = workspace_with("[egress]\nallow_all_hosts = true\n");
        let policy = load_policy(dir.path(), "ws-1");
        assert!(!policy.permits("github.com", Protocol::Https, 443));
    }

    #[test]
    fn allow_list_compiles_to_https_destinations() {
        let dir = workspace_with("[egress]\nallow = [\"github.com\", \"*.crates.io\"]\n");
        let policy = load_policy(dir.path(), "ws-1");
        assert!(policy.permits("github.com", Protocol::Https, 443));
        assert!(policy.permits("static.crates.io", Protocol::Https, 443));
        assert!(!policy.permits("evil.example", Protocol::Https, 443));
    }

    #[test]
    fn destinations_carry_explicit_protocols_and_ports() {
        let dir = workspace_with(
            "[egress]\n[[egress.destinations]]\nhost = \"registry.internal\"\nprotocols = [\"http\"]\nports = [8080]\n",
        );
        let policy = load_policy(dir.path(), "ws-1");
        assert!(policy.permits("registry.internal", Protocol::Http, 8080));
        assert!(!policy.permits("registry.internal", Protocol::Https, 443));
    }

    #[test]
    fn config_without_an_egress_table_denies_everything() {
        let dir = workspace_with("[models]\nmode = \"global\"\n");
        let policy = load_policy(dir.path(), "ws-1");
        assert!(!policy.permits("github.com", Protocol::Https, 443));
    }

    #[test]
    fn delegated_fetch_round_trips_and_stays_opt_in() {
        let dir = workspace_with("[egress]\ndelegated_fetch = true\n");
        assert!(load_policy(dir.path(), "ws-1").delegated_fetch);

        let dir = workspace_with("[egress]\nallow = [\"github.com\"]\n");
        assert!(!load_policy(dir.path(), "ws-1").delegated_fetch);
    }

    #[test]
    fn mandatory_deny_cidrs_extend_rather_than_replace_configured_ones() {
        let dir = workspace_with("[egress]\ndeny_cidrs = [\"10.0.0.0/8\"]\n");
        let cidrs = load_policy(dir.path(), "ws-1").deny_cidrs();
        assert!(cidrs.contains(&"10.0.0.0/8".to_string()));
        assert!(cidrs.contains(&"169.254.169.254/32".to_string()));
    }
}
