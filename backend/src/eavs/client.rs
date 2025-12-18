//! EAVS HTTP client.

use reqwest::{Client, StatusCode};
use std::time::Duration;

use super::error::{EavsError, EavsResult};
use super::types::*;

/// Client for communicating with EAVS API.
#[derive(Debug, Clone)]
pub struct EavsClient {
    /// HTTP client.
    client: Client,
    /// Base URL for EAVS (e.g., "http://localhost:41823").
    base_url: String,
    /// Master key for admin operations.
    master_key: String,
}

impl EavsClient {
    /// Create a new EAVS client.
    pub fn new(base_url: impl Into<String>, master_key: impl Into<String>) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .expect("Failed to create HTTP client");

        Self {
            client,
            base_url: base_url.into(),
            master_key: master_key.into(),
        }
    }

    /// Check if EAVS is healthy.
    pub async fn health_check(&self) -> EavsResult<bool> {
        let url = format!("{}/health", self.base_url);
        let response =
            self.client
                .get(&url)
                .send()
                .await
                .map_err(|e| EavsError::ConnectionFailed {
                    url: url.clone(),
                    message: e.to_string(),
                })?;

        Ok(response.status().is_success())
    }

    /// Create a new virtual API key.
    pub async fn create_key(&self, request: CreateKeyRequest) -> EavsResult<CreateKeyResponse> {
        let url = format!("{}/admin/keys", self.base_url);
        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.master_key))
            .json(&request)
            .send()
            .await?;

        self.handle_response(response).await
    }

    /// Get information about a key.
    pub async fn get_key(&self, key_id_or_hash: &str) -> EavsResult<KeyInfo> {
        let url = format!("{}/admin/keys/{}", self.base_url, key_id_or_hash);
        let response = self
            .client
            .get(&url)
            .header("Authorization", format!("Bearer {}", self.master_key))
            .send()
            .await?;

        self.handle_response(response).await
    }

    /// List all keys.
    pub async fn list_keys(&self) -> EavsResult<Vec<KeyInfo>> {
        let url = format!("{}/admin/keys", self.base_url);
        let response = self
            .client
            .get(&url)
            .header("Authorization", format!("Bearer {}", self.master_key))
            .send()
            .await?;

        self.handle_response(response).await
    }

    /// Disable (revoke) a key.
    pub async fn revoke_key(&self, key_id_or_hash: &str) -> EavsResult<()> {
        let url = format!("{}/admin/keys/{}", self.base_url, key_id_or_hash);
        let response = self
            .client
            .delete(&url)
            .header("Authorization", format!("Bearer {}", self.master_key))
            .send()
            .await?;

        match response.status() {
            StatusCode::NO_CONTENT => Ok(()),
            StatusCode::NOT_FOUND => Err(EavsError::KeyNotFound(key_id_or_hash.to_string())),
            StatusCode::UNAUTHORIZED => Err(EavsError::Unauthorized),
            StatusCode::SERVICE_UNAVAILABLE => Err(EavsError::KeysDisabled),
            _ => {
                let error: ApiErrorResponse = response.json().await.map_err(|e| {
                    EavsError::ParseError(format!("Failed to parse error response: {}", e))
                })?;
                Err(EavsError::ApiError {
                    message: error.error,
                    code: error.code,
                })
            }
        }
    }

    /// Get usage history for a key.
    pub async fn get_usage(&self, key_id_or_hash: &str) -> EavsResult<Vec<UsageRecord>> {
        let url = format!("{}/admin/keys/{}/usage", self.base_url, key_id_or_hash);
        let response = self
            .client
            .get(&url)
            .header("Authorization", format!("Bearer {}", self.master_key))
            .send()
            .await?;

        self.handle_response(response).await
    }

    /// Handle response and parse JSON or error.
    async fn handle_response<T: serde::de::DeserializeOwned>(
        &self,
        response: reqwest::Response,
    ) -> EavsResult<T> {
        let status = response.status();

        if status.is_success() {
            response
                .json()
                .await
                .map_err(|e| EavsError::ParseError(format!("Failed to parse response: {}", e)))
        } else {
            match status {
                StatusCode::UNAUTHORIZED => Err(EavsError::Unauthorized),
                StatusCode::NOT_FOUND => Err(EavsError::KeyNotFound("unknown".to_string())),
                StatusCode::SERVICE_UNAVAILABLE => Err(EavsError::KeysDisabled),
                _ => {
                    let error: ApiErrorResponse = response.json().await.map_err(|e| {
                        EavsError::ParseError(format!("Failed to parse error response: {}", e))
                    })?;
                    Err(EavsError::ApiError {
                        message: error.error,
                        code: error.code,
                    })
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_creation() {
        let client = EavsClient::new("http://localhost:41823", "test-master-key");
        assert_eq!(client.base_url, "http://localhost:41823");
    }

    #[test]
    fn test_create_key_request() {
        let request = CreateKeyRequest::new("test-session")
            .permissions(KeyPermissions::with_budget(10.0).rpm(60))
            .metadata(serde_json::json!({"session_id": "abc123"}));

        assert_eq!(request.name, Some("test-session".to_string()));
        assert!(request.permissions.is_some());
        let perms = request.permissions.unwrap();
        assert_eq!(perms.max_budget_usd, Some(10.0));
        assert_eq!(perms.rpm_limit, Some(60));
    }
}
