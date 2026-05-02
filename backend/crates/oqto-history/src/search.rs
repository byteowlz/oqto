use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A single search hit returned by hstry.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct HstrySearchHit {
    pub message_id: String,
    pub conversation_id: String,
    pub message_idx: i32,
    pub role: String,
    pub content: String,
    pub snippet: String,
    pub created_at: Option<DateTime<Utc>>,
    pub conv_created_at: DateTime<Utc>,
    pub conv_updated_at: Option<DateTime<Utc>>,
    pub score: f32,
    pub source_id: String,
    pub external_id: Option<String>,
    pub title: Option<String>,
    pub workspace: Option<String>,
    pub source_adapter: String,
    pub source_path: Option<String>,
    #[serde(default)]
    pub host: Option<String>,
}
