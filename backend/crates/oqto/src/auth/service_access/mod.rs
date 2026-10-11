//! One-service pilot. Account auth and Workspace membership remain authoritative.
//! Workload credentials never become CurrentUser or authorize other Oqto routes.
mod repository;
mod routes;

pub use repository::PilotRepository;
pub use routes::pilot_router;

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};

use super::AuthState;

pub const SERVICE_ID: &str = "design-studio";
pub const RESOURCE_ID: &str = "catalog";
pub const WORKLOAD_ID: &str = "design-review";
pub const MAX_TTL_SECONDS: i64 = 3600;

/// Opt-in config; absence means no routes. Never carries upstream addresses.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PilotConfig {
    pub audience: String,
    pub workspace_id: String,
}

impl PilotConfig {
    pub fn validate(&self) -> Result<()> {
        let url = reqwest::Url::parse(&self.audience).context("pilot audience URL")?;
        ensure!(
            url.scheme() == "https"
                && url.host_str().is_some()
                && url.username().is_empty()
                && url.password().is_none()
                && url.path() == "/"
                && url.query().is_none()
                && url.fragment().is_none()
                && url.origin().ascii_serialization() == self.audience,
            "pilot audience must be one canonical HTTPS origin"
        );
        ensure!(
            valid_id(&self.workspace_id),
            "pilot Workspace ID is invalid"
        );
        Ok(())
    }
}

#[derive(Clone)]
pub struct PilotState {
    pub(crate) repository: PilotRepository,
    pub(crate) auth: AuthState,
    pub(crate) config: PilotConfig,
}

impl PilotState {
    pub fn new(repository: PilotRepository, auth: AuthState, config: PilotConfig) -> Result<Self> {
        config.validate()?;
        ensure!(
            !auth.is_dev_mode(),
            "service-access pilot refuses development auth bypass"
        );
        Ok(Self {
            repository,
            auth,
            config,
        })
    }
}

/// Exact request scope is checked, not inferred from forwarding/identity headers.
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AccessScope {
    pub service_id: String,
    pub workspace_id: String,
    pub resource_id: String,
    pub audience: String,
}

impl AccessScope {
    pub(crate) fn matches(&self, config: &PilotConfig) -> bool {
        self.service_id == SERVICE_ID
            && self.resource_id == RESOURCE_ID
            && self.workspace_id == config.workspace_id
            && self.audience == config.audience
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct GrantRequest {
    /// Caller-chosen stable ID. Duplicate IDs conflict; never blindly retry issuance.
    pub id: String,
    pub workload_id: String,
    pub scope: AccessScope,
    pub expires_at: i64,
    /// Expected current Workspace owner, not a remotely asserted acting identity.
    pub owner_account_id: String,
}

#[derive(Debug, Deserialize, Serialize, sqlx::FromRow)]
pub struct Grant {
    pub id: String,
    pub workload_id: String,
    pub service_id: String,
    pub workspace_id: String,
    pub resource_id: String,
    pub audience: String,
    pub expires_at: i64,
    pub revoked_at: Option<i64>,
    pub issued_by: String,
    pub created_at: i64,
}

#[derive(Serialize)]
pub struct IssuedGrant {
    pub grant: Grant,
    /// Shown once; stored only as a SHA-256 hash. Never included in inspect/audit.
    pub credential: String,
}

pub(crate) fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}
