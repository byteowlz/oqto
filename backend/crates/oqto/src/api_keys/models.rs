use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKeyListItem {
    pub id: String,
    pub name: String,
    pub key_prefix: String,
    pub scopes: Vec<String>,
    pub last_used_at: Option<String>,
    pub expires_at: Option<String>,
    pub created_at: String,
    pub revoked_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKeyCreateRequest {
    pub name: String,
    #[serde(default)]
    pub scopes: Option<Vec<String>>,
    pub expires_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKeyCreateResponse {
    pub api_key: String,
    #[serde(flatten)]
    pub key: ApiKeyListItem,
}

#[derive(Debug, Clone)]
pub struct ApiKeyAuthUser {
    pub key_id: String,
    pub user_id: String,
    pub email: String,
    pub display_name: String,
    pub role: String,
    pub expires_at: Option<String>,
}
