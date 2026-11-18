//! Explicit, fail-closed foreground configuration. No ambient service discovery.
use crate::server::{Limits, Registration};
use crate::store::Policy;
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ServiceConfig {
    #[serde(default)]
    pub enabled: bool,
    pub database: PathBuf,
    pub socket_directory: PathBuf,
    #[serde(default)]
    pub policy: Policy,
    #[serde(default)]
    pub limits: Limits,
    #[serde(default)]
    pub identities: Vec<Registration>,
}
impl ServiceConfig {
    pub fn load(path: &Path) -> Result<Self> {
        let built = config::Config::builder()
            .add_source(
                config::File::from(path)
                    .format(config::FileFormat::Toml)
                    .required(true),
            )
            .build()
            .context("loading explicit post service config")?;
        let config: Self = built
            .try_deserialize()
            .context("decoding post service config")?;
        config.validate()?;
        Ok(config)
    }
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.database.is_absolute() && self.socket_directory.is_absolute(),
            "database and socket_directory must be absolute"
        );
        self.policy.validate()?;
        self.limits.validate()?;
        let mut names = std::collections::HashSet::new();
        for identity in &self.identities {
            identity.validate()?;
            ensure!(
                names.insert(&identity.name),
                "duplicate identity socket name"
            );
        }
        Ok(())
    }
    pub fn schema() -> Result<String> {
        let mut value = serde_json::to_value(schemars::schema_for!(Self))?;
        value["version"] = serde_json::json!("1.0.0");
        value["stability"] = serde_json::json!("draft");
        Ok(serde_json::to_string_pretty(&value)?)
    }
}

/// Write an owner-reviewed candidate to a NEW file. Never overwrite a user's
/// original config or silently discard unknown entries (load rejects them).
pub fn add_identity(config: &ServiceConfig, identity: Registration, output: &Path) -> Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let mut candidate = config.clone();
    candidate.identities.push(identity);
    candidate.validate()?;
    let bytes = toml::to_string_pretty(&candidate)?;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(output)
        .context("creating new registry candidate (must not exist)")?;
    file.write_all(bytes.as_bytes())
        .context("writing registry candidate")?;
    file.sync_all().context("syncing registry candidate")
}
