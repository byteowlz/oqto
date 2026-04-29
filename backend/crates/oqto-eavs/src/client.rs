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
    pub fn new(base_url: impl Into<String>, master_key: impl Into<String>) -> EavsResult<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|e| EavsError::ClientBuild(e.to_string()))?;

        Ok(Self {
            client,
            base_url: base_url.into(),
            master_key: master_key.into(),
        })
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

    /// Get detailed provider info including model lists.
    ///
    /// Returns providers with their type, Pi API mapping, and model list
    /// (config shortlist if set, otherwise full models.dev catalog).
    pub async fn providers_detail(&self) -> EavsResult<Vec<ProviderDetail>> {
        let url = format!("{}/providers/detail", self.base_url);
        let response = self.client.get(&url).send().await?;
        self.handle_response(response).await
    }

    /// Start an OAuth login flow for a provider.
    pub async fn oauth_login(
        &self,
        provider: &str,
        user_id: &str,
        redirect_uri: Option<&str>,
    ) -> EavsResult<OAuthLoginResponse> {
        #[derive(serde::Serialize)]
        struct OAuthLoginRequest<'a> {
            user_id: &'a str,
            #[serde(skip_serializing_if = "Option::is_none")]
            redirect_uri: Option<&'a str>,
        }

        let url = format!("{}/auth/login/{}", self.base_url, provider);
        let response = self
            .client
            .post(&url)
            .json(&OAuthLoginRequest {
                user_id,
                redirect_uri,
            })
            .send()
            .await?;
        self.handle_response(response).await
    }

    /// Complete OAuth flow with a callback code + state.
    pub async fn oauth_callback(
        &self,
        code: &str,
        state: &str,
        redirect_uri: Option<&str>,
    ) -> EavsResult<OAuthStatusResponse> {
        #[derive(serde::Serialize)]
        struct OAuthCallbackRequest<'a> {
            code: &'a str,
            state: &'a str,
            #[serde(skip_serializing_if = "Option::is_none")]
            redirect_uri: Option<&'a str>,
        }

        let url = format!("{}/auth/callback", self.base_url);
        let response = self
            .client
            .post(&url)
            .json(&OAuthCallbackRequest {
                code,
                state,
                redirect_uri,
            })
            .send()
            .await?;
        self.handle_response(response).await
    }

    /// Poll device-code OAuth flow (GitHub Copilot).
    pub async fn oauth_poll(
        &self,
        provider: &str,
        user_id: &str,
        device_code: &str,
    ) -> EavsResult<OAuthPollResponse> {
        #[derive(serde::Serialize)]
        struct OAuthPollRequest<'a> {
            user_id: &'a str,
            device_code: &'a str,
        }

        let url = format!("{}/auth/poll/{}", self.base_url, provider);
        let response = self
            .client
            .post(&url)
            .json(&OAuthPollRequest {
                user_id,
                device_code,
            })
            .send()
            .await?;
        self.handle_response(response).await
    }

    /// Get OAuth providers connected for a user.
    pub async fn oauth_status(&self, user_id: &str) -> EavsResult<Vec<String>> {
        let url = format!("{}/auth/status/{}", self.base_url, user_id);
        let response = self.client.get(&url).send().await?;
        self.handle_response(response).await
    }

    /// Delete OAuth credentials for a user/provider.
    pub async fn oauth_delete(&self, user_id: &str, provider: &str) -> EavsResult<bool> {
        let url = format!("{}/auth/{}/{}", self.base_url, user_id, provider);
        let response = self.client.delete(&url).send().await?;
        match response.status() {
            StatusCode::NO_CONTENT => Ok(true),
            StatusCode::NOT_FOUND => Ok(false),
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

    /// Get the base URL (for constructing provider-prefixed URLs).
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Get the master key for admin API calls.
    pub fn master_key(&self) -> &str {
        &self.master_key
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
        let client = EavsClient::new("http://localhost:41823", "test-master-key").unwrap();
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
