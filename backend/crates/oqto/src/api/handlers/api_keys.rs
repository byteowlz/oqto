//! API key management handlers.

use axum::{
    Json,
    extract::{Path, State},
};
use serde::Serialize;
use tracing::instrument;

use crate::api::error::{ApiError, ApiResult};
use crate::api::state::AppState;
use crate::api_keys::{
    ApiKeyCreateRequest, ApiKeyCreateResponse, ApiKeyListItem, generate_api_key, hash_api_key,
    normalize_expires_at,
};
use crate::auth::CurrentUser;

const OMNI_KEY_NAME: &str = "omni-vanilla";

#[derive(Debug, Serialize)]
pub struct ApiKeyListResponse {
    pub keys: Vec<ApiKeyListItem>,
}

/// List API keys for the current user.
#[instrument(skip(state))]
pub async fn list_api_keys(
    State(state): State<AppState>,
    user: CurrentUser,
) -> ApiResult<Json<ApiKeyListResponse>> {
    let keys = state
        .api_keys
        .list_for_user(user.id())
        .await
        .map_err(|e| ApiError::internal(format!("Failed to list api keys: {e}")))?;
    Ok(Json(ApiKeyListResponse { keys }))
}

/// Create a new API key for the current user.
#[instrument(skip(state))]
pub async fn create_api_key(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(request): Json<ApiKeyCreateRequest>,
) -> ApiResult<Json<ApiKeyCreateResponse>> {
    let name = request.name.trim();
    if name.is_empty() {
        return Err(ApiError::bad_request("API key name is required"));
    }

    let expires_at = match request.expires_at.as_deref() {
        Some(value) => Some(
            normalize_expires_at(value)
                .map_err(|e| ApiError::bad_request(format!("Invalid expires_at: {e}")))?,
        ),
        None => None,
    };

    let scopes = request.scopes.unwrap_or_default();

    if name == OMNI_KEY_NAME {
        // Revoke existing keys with the same name so omni links stay stable.
        state
            .api_keys
            .revoke_by_name(user.id(), name)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to revoke old keys: {e}")))?;
    }

    let (api_key, key_prefix) = generate_api_key();
    let key_hash = hash_api_key(&api_key);

    let key = state
        .api_keys
        .create_key(user.id(), name, &key_prefix, &key_hash, scopes, expires_at)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to create api key: {e}")))?;

    Ok(Json(ApiKeyCreateResponse { api_key, key }))
}

/// Revoke an API key by id.
#[instrument(skip(state))]
pub async fn revoke_api_key(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(key_id): Path<String>,
) -> ApiResult<()> {
    let revoked = state
        .api_keys
        .revoke_key(user.id(), &key_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to revoke api key: {e}")))?;

    if !revoked {
        return Err(ApiError::not_found("API key not found"));
    }

    Ok(())
}

/// Delete an API key by id.
#[instrument(skip(state))]
pub async fn delete_api_key(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(key_id): Path<String>,
) -> ApiResult<()> {
    let deleted = state
        .api_keys
        .delete_key(user.id(), &key_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to delete api key: {e}")))?;

    if !deleted {
        return Err(ApiError::not_found("API key not found"));
    }

    Ok(())
}
