use axum::{Json, extract::Path, extract::State};
use serde::{Deserialize, Serialize};

use crate::api::error::{ApiError, ApiResult};
use crate::api::state::AppState;
use crate::auth::CurrentUser;
use crate::eavs::{CreateKeyRequest, EavsClient, OAuthLoginResponse, OAuthPollResponse};

use super::admin::sync_eavs_models_json_with_key;

#[derive(Debug, Serialize)]
pub struct OAuthProviderInfo {
    pub id: String,
    pub name: String,
    pub connected: bool,
}

#[derive(Debug, Serialize)]
pub struct OAuthProvidersResponse {
    pub enabled: bool,
    pub providers: Vec<OAuthProviderInfo>,
}

#[derive(Debug, Deserialize)]
pub struct OAuthCallbackRequest {
    pub code: String,
    pub state: String,
}

#[derive(Debug, Deserialize)]
pub struct OAuthPollRequest {
    pub device_code: String,
}

fn normalize_provider(value: &str) -> String {
    value.trim().to_lowercase()
}

fn provider_display_name(provider: &str) -> String {
    match provider {
        "anthropic" => "Anthropic (Claude)".to_string(),
        "openai-codex" => "OpenAI (ChatGPT/Codex)".to_string(),
        "github-copilot" => "GitHub Copilot".to_string(),
        "google-gemini-cli" => "Google Gemini".to_string(),
        "google-antigravity" => "Google Antigravity".to_string(),
        other => other.to_string(),
    }
}

fn is_oauth_enabled(state: &AppState) -> bool {
    state.eavs_oauth_enabled && !state.eavs_oauth_providers.is_empty()
}

fn is_provider_allowed(state: &AppState, provider: &str) -> bool {
    if !is_oauth_enabled(state) {
        return false;
    }
    let normalized = normalize_provider(provider);
    state
        .eavs_oauth_providers
        .iter()
        .any(|p| normalize_provider(p) == normalized)
}

fn require_oauth_enabled(state: &AppState) -> ApiResult<()> {
    if !is_oauth_enabled(state) {
        return Err(ApiError::forbidden(
            "OAuth login is disabled by the administrator.",
        ));
    }
    Ok(())
}

fn eavs_client(state: &AppState) -> ApiResult<&EavsClient> {
    state
        .eavs_client
        .as_ref()
        .map(|client| client.as_ref())
        .ok_or_else(|| ApiError::service_unavailable("EAVS client is not configured."))
}

fn require_redirect_uri(state: &AppState, provider: &str) -> ApiResult<()> {
    if provider == "openai-codex" && state.eavs_oauth_redirect_uri.is_none() {
        return Err(ApiError::bad_request(
            "OAuth redirect_uri is required for OpenAI Codex.",
        ));
    }
    Ok(())
}

async fn sync_models_with_key(state: &AppState, key: &str, user_id: &str) -> ApiResult<()> {
    let linux_users = state
        .linux_users
        .as_ref()
        .ok_or_else(|| ApiError::service_unavailable("Linux user isolation is not configured."))?;
    let linux_username = linux_users.linux_username(user_id);
    sync_eavs_models_json_with_key(eavs_client(state)?, linux_users, &linux_username, key)
        .await
        .map_err(ApiError::from_anyhow)
}

async fn create_oauth_key(state: &AppState, user_id: &str) -> ApiResult<()> {
    let key_req = CreateKeyRequest::new(format!("oqto-user-{}-oauth", user_id))
        .oauth_user(user_id.to_string());
    let key_resp = eavs_client(state)?
        .create_key(key_req)
        .await
        .map_err(|e| ApiError::bad_gateway(format!("Failed to create oauth key: {e}")))?;
    sync_models_with_key(state, &key_resp.key, user_id).await?;
    Ok(())
}

async fn create_default_key(state: &AppState, user_id: &str) -> ApiResult<()> {
    let key_req = CreateKeyRequest::new(format!("oqto-user-{}", user_id));
    let key_resp = eavs_client(state)?
        .create_key(key_req)
        .await
        .map_err(|e| ApiError::bad_gateway(format!("Failed to create default key: {e}")))?;
    sync_models_with_key(state, &key_resp.key, user_id).await?;
    Ok(())
}

pub async fn oauth_providers(
    State(state): State<AppState>,
    CurrentUser { claims: user }: CurrentUser,
) -> ApiResult<Json<OAuthProvidersResponse>> {
    if !is_oauth_enabled(&state) {
        return Ok(Json(OAuthProvidersResponse {
            enabled: false,
            providers: Vec::new(),
        }));
    }

    let eavs = eavs_client(&state)?;
    let available = eavs
        .providers_detail()
        .await
        .map_err(|e| ApiError::bad_gateway(format!("Failed to query providers: {e}")))?;
    let connected = eavs
        .oauth_status(&user.sub)
        .await
        .map_err(|e| ApiError::bad_gateway(format!("Failed to query oauth status: {e}")))?;

    let mut providers = Vec::new();
    for provider in available.into_iter().filter(|p| p.oauth) {
        if !is_provider_allowed(&state, &provider.name) {
            continue;
        }
        let connected = connected
            .iter()
            .any(|p| normalize_provider(p) == normalize_provider(&provider.name));
        providers.push(OAuthProviderInfo {
            id: provider.name.clone(),
            name: provider_display_name(&provider.name),
            connected,
        });
    }

    Ok(Json(OAuthProvidersResponse {
        enabled: true,
        providers,
    }))
}

pub async fn oauth_login(
    State(state): State<AppState>,
    CurrentUser { claims: user }: CurrentUser,
    Path(provider): Path<String>,
) -> ApiResult<Json<OAuthLoginResponse>> {
    require_oauth_enabled(&state)?;
    if !is_provider_allowed(&state, &provider) {
        return Err(ApiError::forbidden("OAuth provider is not allowed."));
    }
    require_redirect_uri(&state, &provider)?;

    let response = eavs_client(&state)?
        .oauth_login(
            &provider,
            &user.sub,
            state.eavs_oauth_redirect_uri.as_deref(),
        )
        .await
        .map_err(|e| ApiError::bad_gateway(format!("OAuth login failed: {e}")))?;

    Ok(Json(response))
}

pub async fn oauth_callback(
    State(state): State<AppState>,
    CurrentUser { claims: user }: CurrentUser,
    Json(payload): Json<OAuthCallbackRequest>,
) -> ApiResult<Json<crate::eavs::OAuthStatusResponse>> {
    require_oauth_enabled(&state)?;

    let response = eavs_client(&state)?
        .oauth_callback(
            &payload.code,
            &payload.state,
            state.eavs_oauth_redirect_uri.as_deref(),
        )
        .await
        .map_err(|e| ApiError::bad_gateway(format!("OAuth callback failed: {e}")))?;

    create_oauth_key(&state, &user.sub).await?;

    Ok(Json(response))
}

pub async fn oauth_poll(
    State(state): State<AppState>,
    CurrentUser { claims: user }: CurrentUser,
    Path(provider): Path<String>,
    Json(payload): Json<OAuthPollRequest>,
) -> ApiResult<Json<OAuthPollResponse>> {
    require_oauth_enabled(&state)?;
    if !is_provider_allowed(&state, &provider) {
        return Err(ApiError::forbidden("OAuth provider is not allowed."));
    }

    let response = eavs_client(&state)?
        .oauth_poll(&provider, &user.sub, &payload.device_code)
        .await
        .map_err(|e| ApiError::bad_gateway(format!("OAuth poll failed: {e}")))?;

    if response.status == "stored" {
        create_oauth_key(&state, &user.sub).await?;
    }

    Ok(Json(response))
}

pub async fn oauth_delete(
    State(state): State<AppState>,
    CurrentUser { claims: user }: CurrentUser,
    Path(provider): Path<String>,
) -> ApiResult<()> {
    require_oauth_enabled(&state)?;
    if !is_provider_allowed(&state, &provider) {
        return Err(ApiError::forbidden("OAuth provider is not allowed."));
    }

    let deleted = eavs_client(&state)?
        .oauth_delete(&user.sub, &provider)
        .await
        .map_err(|e| ApiError::bad_gateway(format!("OAuth delete failed: {e}")))?;

    if deleted {
        create_default_key(&state, &user.sub).await?;
    }

    Ok(())
}
