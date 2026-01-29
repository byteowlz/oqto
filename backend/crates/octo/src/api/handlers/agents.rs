//! Agent management handlers.

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use serde::Deserialize;
use tracing::{info, instrument};

use crate::agent::{
    AgentExecRequest, AgentExecResponse, AgentInfo, CreateAgentRequest, CreateAgentResponse,
    StartAgentRequest, StartAgentResponse, StopAgentResponse,
};
use crate::auth::CurrentUser;

use crate::api::error::{ApiError, ApiResult};
use crate::api::state::AppState;

#[derive(Debug, Deserialize)]
pub struct AgentListQuery {
    #[serde(default)]
    pub include_context: bool,
}

/// List all agents for a session (running + available directories).
#[instrument(skip(state))]
pub async fn list_agents(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(session_id): Path<String>,
    Query(query): Query<AgentListQuery>,
) -> ApiResult<Json<Vec<AgentInfo>>> {
    // Ensure the requested session belongs to this user.
    let _ = state
        .sessions
        .for_user(user.id())
        .get_session(&session_id)
        .await?
        .ok_or_else(|| ApiError::not_found(format!("Session {} not found", session_id)))?;

    let opencode_session = state
        .sessions
        .for_user(user.id())
        .get_or_create_opencode_session()
        .await?;
    let agents = state
        .agents
        .list_agents(&opencode_session.id, query.include_context)
        .await?;
    info!(
        requested_session_id = %session_id,
        opencode_session_id = %opencode_session.id,
        count = agents.len(),
        "Listed agents"
    );
    Ok(Json(agents))
}

/// Get a specific agent.
#[instrument(skip(state))]
pub async fn get_agent(
    State(state): State<AppState>,
    user: CurrentUser,
    Path((session_id, agent_id)): Path<(String, String)>,
    Query(query): Query<AgentListQuery>,
) -> ApiResult<Json<AgentInfo>> {
    let _ = state
        .sessions
        .for_user(user.id())
        .get_session(&session_id)
        .await?
        .ok_or_else(|| ApiError::not_found(format!("Session {} not found", session_id)))?;

    let opencode_session = state
        .sessions
        .for_user(user.id())
        .get_or_create_opencode_session()
        .await?;
    state
        .agents
        .get_agent(&opencode_session.id, &agent_id, query.include_context)
        .await?
        .map(Json)
        .ok_or_else(|| ApiError::not_found(format!("Agent {} not found", agent_id)))
}

/// Start an agent in a subdirectory.
#[instrument(skip(state, request), fields(directory = ?request.directory))]
pub async fn start_agent(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(session_id): Path<String>,
    Json(request): Json<StartAgentRequest>,
) -> ApiResult<(StatusCode, Json<StartAgentResponse>)> {
    let _ = state
        .sessions
        .for_user(user.id())
        .get_session(&session_id)
        .await?
        .ok_or_else(|| ApiError::not_found(format!("Session {} not found", session_id)))?;

    let opencode_session = state
        .sessions
        .for_user(user.id())
        .get_or_create_opencode_session()
        .await?;
    let response = state
        .agents
        .start_agent(&opencode_session.id, &request.directory)
        .await?;
    info!(
        requested_session_id = %session_id,
        opencode_session_id = %opencode_session.id,
        agent_id = %response.id,
        port = response.port,
        "Started agent"
    );
    Ok((StatusCode::CREATED, Json(response)))
}

/// Stop an agent.
#[instrument(skip(state))]
pub async fn stop_agent(
    State(state): State<AppState>,
    user: CurrentUser,
    Path((session_id, agent_id)): Path<(String, String)>,
) -> ApiResult<Json<StopAgentResponse>> {
    let _ = state
        .sessions
        .for_user(user.id())
        .get_session(&session_id)
        .await?
        .ok_or_else(|| ApiError::not_found(format!("Session {} not found", session_id)))?;

    let opencode_session = state
        .sessions
        .for_user(user.id())
        .get_or_create_opencode_session()
        .await?;
    let response = state
        .agents
        .stop_agent(&opencode_session.id, &agent_id)
        .await?;
    info!(
        requested_session_id = %session_id,
        opencode_session_id = %opencode_session.id,
        agent_id = %agent_id,
        stopped = response.stopped,
        "Stopped agent"
    );
    Ok(Json(response))
}

/// Rediscover agents after control plane restart.
#[instrument(skip(state))]
pub async fn rediscover_agents(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(session_id): Path<String>,
) -> ApiResult<StatusCode> {
    let _ = state
        .sessions
        .for_user(user.id())
        .get_session(&session_id)
        .await?
        .ok_or_else(|| ApiError::not_found(format!("Session {} not found", session_id)))?;

    let opencode_session = state
        .sessions
        .for_user(user.id())
        .get_or_create_opencode_session()
        .await?;
    state.agents.rediscover_agents(&opencode_session.id).await?;
    info!(
        requested_session_id = %session_id,
        opencode_session_id = %opencode_session.id,
        "Rediscovered agents"
    );
    Ok(StatusCode::NO_CONTENT)
}

/// Create a new agent directory with AGENTS.md file.
#[instrument(skip(state, request), fields(name = ?request.name))]
pub async fn create_agent(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(session_id): Path<String>,
    Json(request): Json<CreateAgentRequest>,
) -> ApiResult<(StatusCode, Json<CreateAgentResponse>)> {
    let _ = state
        .sessions
        .for_user(user.id())
        .get_session(&session_id)
        .await?
        .ok_or_else(|| ApiError::not_found(format!("Session {} not found", session_id)))?;

    let opencode_session = state
        .sessions
        .for_user(user.id())
        .get_or_create_opencode_session()
        .await?;
    let response = state
        .agents
        .create_agent(
            &opencode_session.id,
            &request.name,
            &request.description,
            request.scaffold.as_ref(),
        )
        .await?;
    info!(
        requested_session_id = %session_id,
        opencode_session_id = %opencode_session.id,
        agent_id = %response.id,
        directory = %response.directory,
        "Created agent"
    );
    Ok((StatusCode::CREATED, Json(response)))
}

/// Execute a command in a session workspace.
#[instrument(skip(state, request), fields(command = %request.command))]
pub async fn exec_agent_command(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(session_id): Path<String>,
    Json(request): Json<AgentExecRequest>,
) -> ApiResult<Json<AgentExecResponse>> {
    let _ = state
        .sessions
        .for_user(user.id())
        .get_session(&session_id)
        .await?
        .ok_or_else(|| ApiError::not_found(format!("Session {} not found", session_id)))?;

    let opencode_session = state
        .sessions
        .for_user(user.id())
        .get_or_create_opencode_session()
        .await?;
    let response = state
        .agents
        .exec_command(&opencode_session.id, request)
        .await?;
    Ok(Json(response))
}
