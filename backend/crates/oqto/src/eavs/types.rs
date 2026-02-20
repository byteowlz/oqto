//! EAVS API types.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// Request to create a new virtual API key.
#[derive(Debug, Clone, Serialize)]
pub struct CreateKeyRequest {
    /// Human-readable name for the key.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,

    /// When the key expires.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<DateTime<Utc>>,

    /// Key permissions and limits.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permissions: Option<KeyPermissions>,

    /// Arbitrary metadata.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,

    /// OAuth user binding -- requests with this key resolve OAuth tokens for this user.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub oauth_user: Option<String>,
}

impl CreateKeyRequest {
    /// Create a new key request with just a name.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: Some(name.into()),
            expires_at: None,
            permissions: None,
            metadata: None,
            oauth_user: None,
        }
    }

    /// Set the expiration time.
    pub fn expires_at(mut self, expires_at: DateTime<Utc>) -> Self {
        self.expires_at = Some(expires_at);
        self
    }

    /// Set permissions.
    pub fn permissions(mut self, permissions: KeyPermissions) -> Self {
        self.permissions = Some(permissions);
        self
    }

    /// Set metadata.
    pub fn metadata(mut self, metadata: serde_json::Value) -> Self {
        self.metadata = Some(metadata);
        self
    }

    /// Set OAuth user binding.
    pub fn oauth_user(mut self, user: impl Into<String>) -> Self {
        self.oauth_user = Some(user.into());
        self
    }
}

/// Key permissions and limits.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct KeyPermissions {
    /// Allowed model patterns (glob syntax).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allowed_models: Option<HashSet<String>>,

    /// Blocked model patterns.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blocked_models: Option<HashSet<String>>,

    /// Allowed providers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allowed_providers: Option<HashSet<String>>,

    /// Maximum requests per minute.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rpm_limit: Option<u32>,

    /// Maximum tokens per minute.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tpm_limit: Option<u32>,

    /// Maximum requests per day.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rpd_limit: Option<u32>,

    /// Maximum budget in USD.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_budget_usd: Option<f64>,

    /// Budget reset window.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub budget_window: Option<BudgetWindow>,
}

impl KeyPermissions {
    /// Create permissions with a budget limit.
    pub fn with_budget(budget_usd: f64) -> Self {
        Self {
            max_budget_usd: Some(budget_usd),
            ..Default::default()
        }
    }

    /// Set rate limit (requests per minute).
    pub fn rpm(mut self, limit: u32) -> Self {
        self.rpm_limit = Some(limit);
        self
    }

    /// Set budget window.
    pub fn budget_window(mut self, window: BudgetWindow) -> Self {
        self.budget_window = Some(window);
        self
    }
}

/// Budget reset window options.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BudgetWindow {
    /// Never resets - lifetime budget.
    Total,
    /// Resets daily.
    Daily,
    /// Resets weekly.
    Weekly,
    /// Resets monthly.
    Monthly,
}

/// Response after creating a key.
#[derive(Debug, Clone, Deserialize)]
pub struct CreateKeyResponse {
    /// The actual key value (only returned once!).
    pub key: String,

    /// Human-readable key ID (e.g., "cold-lamp").
    pub key_id: String,

    /// Hash of the key for lookups.
    pub key_hash: String,

    /// Human-readable name.
    pub name: Option<String>,

    /// When the key was created.
    pub created_at: DateTime<Utc>,

    /// When the key expires.
    pub expires_at: Option<DateTime<Utc>>,

    /// Permissions summary.
    pub permissions: KeyPermissions,
}

/// Key info (without the actual key value).
#[derive(Debug, Clone, Deserialize)]
pub struct KeyInfo {
    /// Human-readable key ID.
    pub key_id: String,

    /// Hash of the key.
    pub key_hash: String,

    /// Human-readable name.
    pub name: Option<String>,

    /// When the key was created.
    pub created_at: DateTime<Utc>,

    /// When the key expires.
    pub expires_at: Option<DateTime<Utc>>,

    /// Whether the key is disabled.
    pub disabled: bool,

    /// Permissions.
    pub permissions: KeyPermissions,

    /// Usage stats.
    pub usage: KeyUsage,
}

/// Usage tracking for a key.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct KeyUsage {
    /// Total requests made.
    pub total_requests: u64,

    /// Total input tokens consumed.
    pub total_input_tokens: u64,

    /// Total output tokens consumed.
    pub total_output_tokens: u64,

    /// Total spend in USD.
    pub total_spend_usd: f64,

    /// Spend in current budget window.
    pub window_spend_usd: f64,

    /// When the current budget window started.
    pub window_start: Option<DateTime<Utc>>,

    /// Last request timestamp.
    pub last_request_at: Option<DateTime<Utc>>,
}

/// Usage record from history.
#[derive(Debug, Clone, Deserialize)]
pub struct UsageRecord {
    /// Request ID.
    pub id: i64,

    /// Key hash.
    pub key_hash: String,

    /// When the request was made.
    pub timestamp: DateTime<Utc>,

    /// Model used.
    pub model: String,

    /// Provider used.
    pub provider: Option<String>,

    /// Input tokens.
    pub input_tokens: i64,

    /// Output tokens.
    pub output_tokens: i64,

    /// Cost in USD.
    pub cost_usd: f64,

    /// Whether the request succeeded.
    pub success: bool,

    /// Error message if failed.
    pub error: Option<String>,
}

/// Provider detail from /providers/detail endpoint.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ProviderDetail {
    /// Provider name as configured in eavs
    pub name: String,
    /// Provider type (e.g., "openai", "anthropic")
    #[serde(rename = "type")]
    pub type_: String,
    /// Pi-compatible API type for models.json
    pub pi_api: Option<String>,
    /// Whether this provider uses OAuth
    pub oauth: bool,
    /// Whether the provider has a resolved API key
    pub has_api_key: bool,
    /// Custom headers the provider requires (e.g., Azure `api-key`).
    /// Values are placeholders ("EAVS_API_KEY"), not actual secrets.
    #[serde(default)]
    pub headers: std::collections::HashMap<String, String>,
    /// API version string (Azure providers)
    #[serde(default)]
    pub api_version: Option<String>,
    /// Model list (shortlist or full catalog)
    pub models: Vec<ProviderModel>,
}

/// Model entry from eavs provider detail.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ProviderModel {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub reasoning: bool,
    #[serde(default)]
    pub input: Vec<String>,
    #[serde(default)]
    pub context_window: u64,
    #[serde(default)]
    pub max_tokens: u64,
    #[serde(default)]
    pub cost: ProviderModelCost,
    /// Compatibility flags for Pi (e.g., supportsDeveloperRole)
    #[serde(default, skip_serializing_if = "std::collections::HashMap::is_empty")]
    pub compat: std::collections::HashMap<String, serde_json::Value>,
}

/// Cost per million tokens.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct ProviderModelCost {
    #[serde(default)]
    pub input: f64,
    #[serde(default)]
    pub output: f64,
    #[serde(default)]
    pub cache_read: f64,
}

/// Error response from EAVS API.
#[derive(Debug, Clone, Deserialize)]
pub struct ApiErrorResponse {
    pub error: String,
    pub code: String,
}
