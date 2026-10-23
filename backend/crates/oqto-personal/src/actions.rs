//! Authenticated semantic dispatch. The Runner implements the backend; this
//! layer cannot spawn, access granted files, or infer execution from pairing.
use crate::{Access, AuthorizedAction, PairingAuthority};
use axum::{
    Json, Router,
    body::Bytes,
    extract::{DefaultBodyLimit, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::post,
};
use oqto_personal_wire::{
    SignedRequest,
    actions::{ACTION_PATH, HostAction, MAX_ACTION_BYTES},
};
use serde_json::Value;
use std::sync::Arc;

#[derive(Clone, Copy, Debug)]
pub enum ActionFailure {
    Denied,
    StaleBaseline,
    NotFound,
    Unavailable,
    OutcomeUnknown,
    Invalid,
}
impl ActionFailure {
    fn response(self) -> Response {
        let (status, code) = match self {
            Self::Denied => (StatusCode::FORBIDDEN, "action_denied"),
            Self::StaleBaseline => (StatusCode::CONFLICT, "stale_baseline"),
            Self::NotFound => (StatusCode::NOT_FOUND, "not_found"),
            Self::Unavailable => (StatusCode::SERVICE_UNAVAILABLE, "unavailable"),
            Self::OutcomeUnknown => (StatusCode::CONFLICT, "outcome_unknown_do_not_retry"),
            Self::Invalid => (StatusCode::BAD_REQUEST, "invalid_action"),
        };
        (status, Json(serde_json::json!({"error":code}))).into_response()
    }
}

#[async_trait::async_trait]
pub trait ActionBackend: Send + Sync {
    /// Own the admission guard through bounded effects; never retain it across
    /// idle attachment/terminal lifetimes. Streams need their own revocation fence.
    async fn dispatch(
        &self,
        action: HostAction,
        authorization: AuthorizedAction,
    ) -> Result<Value, ActionFailure>;
}
#[derive(Clone)]
struct Service {
    authority: Arc<PairingAuthority>,
    backend: Arc<dyn ActionBackend>,
}

pub fn action_router(authority: Arc<PairingAuthority>, backend: Arc<dyn ActionBackend>) -> Router {
    Router::new()
        .route(ACTION_PATH, post(dispatch))
        .layer(DefaultBodyLimit::max(MAX_ACTION_BYTES))
        .with_state(Service { authority, backend })
}
fn access(action: &HostAction) -> Access<'_> {
    match action {
        HostAction::Capabilities => Access::Inventory,
        HostAction::FilesList { root_id, .. } | HostAction::FileRead { root_id, .. } => {
            Access::FileRead(root_id)
        }
        HostAction::FileWrite { root_id, .. }
        | HostAction::FileRemove { root_id, .. }
        | HostAction::DirectoryCreate { root_id, .. } => Access::FileWrite(root_id),
        HostAction::TerminalsList
        | HostAction::TerminalOpen { .. }
        | HostAction::TerminalRead { .. }
        | HostAction::TerminalWrite { .. }
        | HostAction::TerminalResize { .. }
        | HostAction::TerminalClose { .. } => Access::FullUserTerminal,
        HostAction::SessionsList
        | HostAction::ProjectIcons { .. }
        | HostAction::SessionHistory { .. }
        | HostAction::SessionAttach { .. }
        | HostAction::SessionRead { .. }
        | HostAction::SessionDetach { .. } => Access::SessionObserve,
        HostAction::SessionRequestControl { .. }
        | HostAction::SessionReleaseControl { .. }
        | HostAction::SessionInput { .. } => Access::SessionControl,
    }
}
async fn dispatch(State(service): State<Service>, bytes: Bytes) -> Response {
    let Ok(request) = serde_json::from_slice::<SignedRequest>(&bytes) else {
        return ActionFailure::Invalid.response();
    };
    let Ok(device) = service.authority.authenticate(&request).await else {
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"error":"authentication_refused"})),
        )
            .into_response();
    };
    let Ok(action) = serde_json::from_str::<HostAction>(&request.payload) else {
        return ActionFailure::Invalid.response();
    };
    let Ok(authorization) = service
        .authority
        .require_current(&device, access(&action))
        .await
    else {
        return ActionFailure::Denied.response();
    };
    match service.backend.dispatch(action, authorization).await {
        Ok(value) => Json(value).into_response(),
        Err(error) => error.response(),
    }
}
