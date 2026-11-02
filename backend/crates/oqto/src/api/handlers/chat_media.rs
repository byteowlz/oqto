//! Cookie-authenticated media for the same backend-resolved Session target as
//! durable chat history. No filesystem paths or raw Runner endpoints from clients.
#[cfg(test)]
#[path = "chat_media_tests.rs"]
mod tests;
use super::*;
use axum::{
    body::Body,
    http::header,
    response::{IntoResponse, Response},
};
use base64::{Engine, engine::general_purpose::STANDARD};
use oqto_runner::protocol::{GetWorkspaceSessionMediaRequest, WorkspaceMediaVariant};
use sha2::{Digest, Sha256};
#[derive(Deserialize)]
pub struct MediaQuery {
    #[serde(default = "thumb")]
    pub variant: WorkspaceMediaVariant,
    pub shared_workspace_id: Option<String>,
}
fn thumb() -> WorkspaceMediaVariant {
    WorkspaceMediaVariant::Thumb
}
pub async fn get_session_media(
    State(state): State<AppState>,
    user: CurrentUser,
    Path((session_id, sha256)): Path<(String, String)>,
    Query(query): Query<MediaQuery>,
) -> ApiResult<Response> {
    if session_id.len() > 256
        || sha256.len() != 64
        || !sha256
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(ApiError::bad_request("invalid Session media reference"));
    }
    let target = resolve_session_target(
        &state,
        user.id(),
        &session_id,
        query.shared_workspace_id.as_deref(),
        is_multi_user_mode(&state),
    )
    .await?;
    let runner = resolve_runner_for_read_target(&state, user.id(), &target)
        .await
        .map_err(|_| ApiError::forbidden("Session media target unavailable"))?
        .ok_or_else(|| ApiError::not_found("Session media unavailable"))?;
    let workspace_scope = match &target {
        ExecutionTarget::Personal => oqto_protocol::projection::MediaWorkspaceScope::Owned,
        ExecutionTarget::RemoteMachine { workspace_path, .. } => {
            oqto_protocol::projection::MediaWorkspaceScope::Exact {
                path: workspace_path.into(),
            }
        }
        ExecutionTarget::SharedWorkspace { workspace_id } => {
            let service = state
                .shared_workspaces
                .as_ref()
                .ok_or_else(|| ApiError::forbidden("Session media Workspace unavailable"))?;
            let (workspace, _) = service
                .get(workspace_id, user.id())
                .await
                .map_err(|_| ApiError::forbidden("Session media access revoked"))?
                .ok_or_else(|| ApiError::forbidden("Session media access revoked"))?;
            oqto_protocol::projection::MediaWorkspaceScope::Within {
                root: workspace.path.into(),
            }
        }
    };
    let response = runner
        .get_workspace_session_media(GetWorkspaceSessionMediaRequest {
            session_id: session_id.clone(),
            workspace_scope,
            sha256: sha256.clone(),
            variant: query.variant,
        })
        .await
        .map_err(|_| ApiError::not_found("Session image unavailable"))?;
    match &target {
        ExecutionTarget::RemoteMachine { workspace_path, .. } => {
            if response.workspace_path.as_deref().map(std::path::Path::new)
                != Some(std::path::Path::new(workspace_path))
            {
                return Err(ApiError::forbidden(
                    "Session media Workspace binding mismatch",
                ));
            }
        }
        ExecutionTarget::SharedWorkspace { workspace_id } => {
            let service = state
                .shared_workspaces
                .as_ref()
                .ok_or_else(|| ApiError::forbidden("Session media Workspace unavailable"))?;
            let (workspace, _) = service
                .get(workspace_id, user.id())
                .await
                .map_err(|_| ApiError::forbidden("Session media access revoked"))?
                .ok_or_else(|| ApiError::forbidden("Session media access revoked"))?;
            let path = response
                .workspace_path
                .as_deref()
                .map(std::path::Path::new)
                .ok_or_else(|| {
                    ApiError::forbidden("Session media Workspace binding unavailable")
                })?;
            if !path.is_absolute()
                || path
                    .components()
                    .any(|component| matches!(component, std::path::Component::ParentDir))
                || !path.starts_with(&workspace.path)
            {
                return Err(ApiError::forbidden(
                    "Session media outside the authorized Workspace",
                ));
            }
        }
        ExecutionTarget::Personal => {}
    }
    let media = response.media;
    if response.session_id != session_id || media["source_sha256"] != sha256 {
        return Err(ApiError::internal("Session media identity mismatch"));
    }
    let full = matches!(query.variant, WorkspaceMediaVariant::Full);
    let limit: usize = if full { 8 * 1024 * 1024 } else { 128 * 1024 };
    let data = media["data_base64"]
        .as_str()
        .ok_or_else(|| ApiError::internal("invalid media response"))?;
    if data.len() > limit.div_ceil(3) * 4 {
        return Err(ApiError::internal("Session media exceeds bound"));
    }
    let bytes = STANDARD
        .decode(data)
        .map_err(|_| ApiError::internal("invalid media encoding"))?;
    if bytes.len() > limit
        || media["byte_size"].as_u64() != Some(bytes.len() as u64)
        || media["sha256"] != hex::encode(Sha256::digest(&bytes))
        || (full && media["sha256"] != sha256)
    {
        return Err(ApiError::internal("Session media integrity mismatch"));
    }
    let mime = media["mime_type"]
        .as_str()
        .ok_or_else(|| ApiError::internal("invalid media MIME"))?;
    if !matches!(
        mime,
        "image/jpeg" | "image/png" | "image/gif" | "image/webp"
    ) || (!full && mime != "image/jpeg")
    {
        return Err(ApiError::internal("unsupported media MIME"));
    }
    // Recheck target ownership/membership after the potentially queued read.
    let current = resolve_session_target(
        &state,
        user.id(),
        &session_id,
        query.shared_workspace_id.as_deref(),
        is_multi_user_mode(&state),
    )
    .await?;
    if current != target
        || resolve_runner_for_read_target(&state, user.id(), &current)
            .await
            .map_err(|_| ApiError::forbidden("Session media access revoked"))?
            .is_none()
    {
        return Err(ApiError::forbidden("Session media access revoked"));
    }
    Ok((
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, mime.to_owned()),
            (header::CONTENT_LENGTH, bytes.len().to_string()),
            (header::CACHE_CONTROL, "private, no-cache".into()),
            (
                axum::http::HeaderName::from_static("cross-origin-resource-policy"),
                "same-origin".into(),
            ),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff".into()),
            (
                header::CONTENT_SECURITY_POLICY,
                "default-src 'none'; img-src 'self' data:".into(),
            ),
        ],
        Body::from(bytes),
    )
        .into_response())
}
