//! Invite code models.

use serde::{Deserialize, Serialize};
use sqlx::FromRow;

/// Invite code for user registration.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct InviteCode {
    /// Unique identifier.
    pub id: String,
    /// The actual invite code string.
    pub code: String,
    /// User ID of who created this code.
    pub created_by: String,
    /// User ID of last person who used this code (for tracking).
    pub used_by: Option<String>,
    /// Remaining uses (0 = exhausted).
    pub uses_remaining: i32,
    /// Maximum total uses.
    pub max_uses: i32,
    /// Optional expiration timestamp.
    pub expires_at: Option<String>,
    /// Creation timestamp.
    pub created_at: String,
    /// Last usage timestamp.
    pub last_used_at: Option<String>,
    /// Optional admin note.
    pub note: Option<String>,
}

impl InviteCode {
    /// Check if this code is still valid (not expired and has uses remaining).
    pub fn is_valid(&self) -> bool {
        if self.uses_remaining <= 0 {
            return false;
        }

        if let Some(expires_at) = &self.expires_at {
            // Parse and compare with current time
            if let Ok(expiry) = chrono::DateTime::parse_from_rfc3339(expires_at)
                && expiry < chrono::Utc::now()
            {
                return false;
            }
            // Also try SQLite datetime format
            if let Ok(expiry) =
                chrono::NaiveDateTime::parse_from_str(expires_at, "%Y-%m-%d %H:%M:%S")
            {
                let expiry_utc = expiry.and_utc();
                if expiry_utc < chrono::Utc::now() {
                    return false;
                }
            }
        }

        true
    }

    /// Check if this code is exhausted (no uses remaining).
    pub fn is_exhausted(&self) -> bool {
        self.uses_remaining <= 0
    }

    /// Check if this code is expired.
    pub fn is_expired(&self) -> bool {
        if let Some(expires_at) = &self.expires_at {
            if let Ok(expiry) = chrono::DateTime::parse_from_rfc3339(expires_at) {
                return expiry < chrono::Utc::now();
            }
            if let Ok(expiry) =
                chrono::NaiveDateTime::parse_from_str(expires_at, "%Y-%m-%d %H:%M:%S")
            {
                let expiry_utc = expiry.and_utc();
                return expiry_utc < chrono::Utc::now();
            }
        }
        false
    }
}

/// Request to create a new invite code.
#[derive(Debug, Clone, Deserialize)]
pub struct CreateInviteCodeRequest {
    /// Custom code (optional, will be generated if not provided).
    pub code: Option<String>,
    /// Number of times this code can be used.
    #[serde(default = "default_uses")]
    pub max_uses: i32,
    /// Optional expiration duration in seconds from now.
    pub expires_in_secs: Option<i64>,
    /// Optional admin note.
    pub note: Option<String>,
}

fn default_uses() -> i32 {
    1
}

/// Request to create multiple invite codes at once.
#[derive(Debug, Clone, Deserialize)]
pub struct BatchCreateInviteCodesRequest {
    /// Number of codes to generate.
    pub count: u32,
    /// Number of uses per code.
    #[serde(default = "default_uses")]
    pub uses_per_code: i32,
    /// Optional expiration duration in seconds from now.
    pub expires_in_secs: Option<i64>,
    /// Optional prefix for generated codes.
    pub prefix: Option<String>,
    /// Optional admin note (applied to all codes).
    pub note: Option<String>,
}

/// Query parameters for listing invite codes.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct InviteCodeListQuery {
    /// Filter by creator.
    pub created_by: Option<String>,
    /// Filter by validity (true = still usable, false = exhausted/expired).
    pub valid: Option<bool>,
    /// Limit results.
    pub limit: Option<i64>,
    /// Offset for pagination.
    pub offset: Option<i64>,
}

/// Summary of invite code for API response.
#[derive(Debug, Clone, Serialize)]
pub struct InviteCodeSummary {
    pub id: String,
    pub code: String,
    pub created_by: String,
    pub uses_remaining: i32,
    pub max_uses: i32,
    pub expires_at: Option<String>,
    pub created_at: String,
    pub is_valid: bool,
    pub note: Option<String>,
}

impl From<InviteCode> for InviteCodeSummary {
    fn from(code: InviteCode) -> Self {
        let is_valid = code.is_valid();
        Self {
            id: code.id,
            code: code.code,
            created_by: code.created_by,
            uses_remaining: code.uses_remaining,
            max_uses: code.max_uses,
            expires_at: code.expires_at,
            created_at: code.created_at,
            is_valid,
            note: code.note,
        }
    }
}
