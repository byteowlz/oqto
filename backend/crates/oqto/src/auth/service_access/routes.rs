use axum::{
    Json, Router,
    extract::{
        Path, Query, State,
        rejection::{JsonRejection, QueryRejection},
    },
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use chrono::Utc;
use rand::RngCore;
use serde::Serialize;

use super::repository::AuditRecord;
use super::{
    AccessScope, Grant, GrantRequest, IssuedGrant, MAX_TTL_SECONDS, PilotState, RESOURCE_ID,
    SERVICE_ID, WORKLOAD_ID, valid_id,
};

#[derive(Clone)]
struct RouteState {
    pilot: PilotState,
    local_admin: bool,
}

/// `local_admin` may only come from Oqto's existing trusted admin-socket router.
/// The public HTTP router always sets it false, regardless of JWT role.
pub fn pilot_router(pilot: PilotState, local_admin: bool) -> Router {
    Router::new()
        .route("/service-access/v1/check", get(check))
        .route("/service-access/v1/discovery", get(discovery))
        .route("/service-access/v1/grants", get(inspect).post(issue))
        .route("/service-access/v1/grants/{id}/revoke", post(revoke))
        .method_not_allowed_fallback(|| async {
            Failure(StatusCode::METHOD_NOT_ALLOWED, "operation_unsupported")
        })
        .with_state(RouteState { pilot, local_admin })
        .layer(axum::extract::DefaultBodyLimit::max(8192))
        .layer(tower_http::set_header::SetResponseHeaderLayer::overriding(
            header::CACHE_CONTROL,
            axum::http::HeaderValue::from_static("no-store"),
        ))
}

#[derive(Serialize)]
struct FailureBody {
    schema: &'static str,
    code: &'static str,
}

struct Failure(StatusCode, &'static str);
impl IntoResponse for Failure {
    fn into_response(self) -> Response {
        (
            self.0,
            Json(FailureBody {
                schema: "oqto.service-access.v1",
                code: self.1,
            }),
        )
            .into_response()
    }
}
fn denied(code: &'static str) -> Failure {
    Failure(StatusCode::FORBIDDEN, code)
}
fn unavailable(error: anyhow::Error) -> Failure {
    tracing::error!(error = %error, "pilot authority unavailable");
    Failure(StatusCode::SERVICE_UNAVAILABLE, "authority_unavailable")
}

struct Decision {
    actor: Option<String>,
    grant: Option<String>,
    reason: &'static str,
    allowed: bool,
}
impl Decision {
    fn deny(reason: &'static str) -> Self {
        Self {
            actor: None,
            grant: None,
            reason,
            allowed: false,
        }
    }
}

fn bearer(headers: &HeaderMap) -> Option<&str> {
    // Ambiguous duplicate headers are rejected, never interpreted differently by proxies.
    // Native Studio Authorization/Cookie is a separate authority and is ignored.
    const EDGE_AUTH: &str = "x-oqto-service-authorization";
    if headers.get_all(EDGE_AUTH).iter().count() != 1 {
        return None;
    }
    let mut parts = headers.get(EDGE_AUTH)?.to_str().ok()?.split_whitespace();
    if !parts.next()?.eq_ignore_ascii_case("bearer") {
        return None;
    }
    let token = parts.next()?;
    if parts.next().is_some() {
        return None;
    }
    Some(token)
}

async fn human(state: &PilotState, token: &str, now: i64) -> Result<Option<String>, Failure> {
    let Ok(claims) = state.auth.validate_token(token) else {
        return Ok(None);
    };
    // Existing human sessions are audience-less. Do not infer future OIDC assurance.
    if claims.iss.as_deref() != Some("workspace-backend")
        || claims.aud.is_some()
        || claims.exp <= now
        || claims.nbf.is_some_and(|nbf| nbf > now)
        || claims.effective_role() == crate::auth::Role::Service
    {
        return Ok(None);
    }
    let allowed = state
        .repository
        .human_has_access(&claims.sub, &state.config.workspace_id)
        .await
        .map_err(unavailable)?;
    Ok(allowed.then_some(claims.sub))
}

async fn decide(
    state: &PilotState,
    headers: &HeaderMap,
    scope: &AccessScope,
    now: i64,
) -> Result<Decision, Failure> {
    if !scope.matches(&state.config) {
        return Ok(Decision::deny("scope_denied"));
    }
    let Some(token) = bearer(headers) else {
        return Ok(Decision::deny("authentication_required"));
    };
    if !token.starts_with("oqto_sa_") {
        return Ok(match human(state, token, now).await? {
            Some(actor) => Decision {
                actor: Some(actor),
                grant: None,
                reason: "human_member",
                allowed: true,
            },
            None => Decision::deny("human_denied"),
        });
    }
    let Some(grant) = state
        .repository
        .workload_grant(token)
        .await
        .map_err(unavailable)?
    else {
        return Ok(Decision::deny("credential_invalid"));
    };
    let reason = workload_reason(state, &grant, now).await?;
    Ok(Decision {
        actor: Some(grant.workload_id),
        grant: Some(grant.id),
        reason,
        allowed: reason == "workload_granted",
    })
}

async fn workload_reason(
    state: &PilotState,
    grant: &Grant,
    now: i64,
) -> Result<&'static str, Failure> {
    if grant.revoked_at.is_some() {
        return Ok("grant_revoked");
    }
    if grant.expires_at <= now {
        return Ok("grant_expired");
    }
    if grant.service_id != SERVICE_ID
        || grant.resource_id != RESOURCE_ID
        || grant.workload_id != WORKLOAD_ID
        || grant.workspace_id != state.config.workspace_id
        || grant.audience != state.config.audience
    {
        return Ok("scope_denied");
    }
    if !state
        .repository
        .issuer_has_authority(&grant.issued_by, &grant.workspace_id)
        .await
        .map_err(unavailable)?
    {
        return Ok("issuer_authority_lost");
    }
    Ok("workload_granted")
}

async fn authorize(
    state: &PilotState,
    headers: &HeaderMap,
    scope: &AccessScope,
    operation: &str,
) -> Result<(bool, String), Failure> {
    let now = Utc::now().timestamp();
    let decision = decide(state, headers, scope, now).await?;
    let event = state
        .repository
        .audit(AuditRecord {
            operation,
            actor: decision.actor.as_deref(),
            grant: decision.grant.as_deref(),
            decision: if decision.allowed { "allow" } else { "deny" },
            reason: decision.reason,
            occurred_at: now,
        })
        .await
        .map_err(unavailable)?;
    if operation == "check" && !decision.allowed {
        return Err(denied(decision.reason));
    }
    Ok((decision.allowed, event))
}

#[derive(Serialize)]
struct AccessResult {
    schema: &'static str,
    allowed: bool,
    event_id: String,
}
async fn check(
    State(state): State<RouteState>,
    headers: HeaderMap,
    query: Result<Query<AccessScope>, QueryRejection>,
) -> Result<Json<AccessResult>, Failure> {
    let Query(scope) = query.map_err(|_| Failure(StatusCode::BAD_REQUEST, "invalid_scope"))?;
    // No stream/upgrade authority is claimed. Caddy must overwrite original method.
    let method = headers
        .get("x-forwarded-method")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("GET");
    if !matches!(method, "GET" | "HEAD")
        || headers.contains_key(header::UPGRADE)
        || headers
            .get(header::ACCEPT)
            .and_then(|v| v.to_str().ok())
            .is_some_and(|v| v.contains("text/event-stream"))
    {
        state
            .pilot
            .repository
            .audit_denial("check", "operation_unsupported")
            .await
            .map_err(unavailable)?;
        return Err(denied("operation_unsupported"));
    }
    if headers.get_all(header::AUTHORIZATION).iter().count() > 1 {
        state
            .pilot
            .repository
            .audit_denial("check", "ambiguous_credentials")
            .await
            .map_err(unavailable)?;
        return Err(denied("ambiguous_credentials"));
    }
    // A native-looking candidate still must not be an Oqto workload credential.
    // The gateway retains only Studio's pinned native syntax before this check.
    // Syntax alone is not proof of provenance or native Studio authorization.
    if let Some(native) = headers
        .get(header::AUTHORIZATION)
        .and_then(|h| h.to_str().ok())
        .and_then(|h| h.strip_prefix("Bearer "))
        && state
            .pilot
            .repository
            .workload_grant(native)
            .await
            .map_err(unavailable)?
            .is_some()
    {
        state
            .pilot
            .repository
            .audit_denial("check", "credential_overlap")
            .await
            .map_err(unavailable)?;
        return Err(denied("credential_overlap"));
    }
    let (allowed, event_id) = authorize(&state.pilot, &headers, &scope, "check").await?;
    Ok(Json(AccessResult {
        schema: "oqto.service-access.v1",
        allowed,
        event_id,
    }))
}

#[derive(Serialize)]
struct ServiceEntry {
    service_id: &'static str,
    resource_id: &'static str,
    audience: String,
}
async fn discovery(
    State(state): State<RouteState>,
    headers: HeaderMap,
    query: Result<Query<AccessScope>, QueryRejection>,
) -> Result<Json<Vec<ServiceEntry>>, Failure> {
    let Query(scope) = query.map_err(|_| Failure(StatusCode::BAD_REQUEST, "invalid_scope"))?;
    let (allowed, _) = authorize(&state.pilot, &headers, &scope, "discovery").await?;
    Ok(Json(if allowed {
        vec![ServiceEntry {
            service_id: SERVICE_ID,
            resource_id: RESOURCE_ID,
            audience: state.pilot.config.audience,
        }]
    } else {
        vec![]
    }))
}

async fn inspect(
    State(state): State<RouteState>,
    headers: HeaderMap,
) -> Result<Json<Vec<Grant>>, Failure> {
    if !state.local_admin {
        let token = bearer(&headers).ok_or_else(|| denied("authentication_required"))?;
        if human(&state.pilot, token, Utc::now().timestamp())
            .await?
            .is_none()
        {
            return Err(denied("human_denied"));
        }
    }
    Ok(Json(
        state
            .pilot
            .repository
            .inspect(&state.pilot.config.workspace_id)
            .await
            .map_err(unavailable)?,
    ))
}

async fn require_local_admin(state: &RouteState, operation: &str) -> Result<(), Failure> {
    if state.local_admin {
        return Ok(());
    }
    state
        .pilot
        .repository
        .audit_denial(operation, "local_admin_required")
        .await
        .map_err(unavailable)?;
    Err(denied("local_admin_required"))
}

async fn issue(
    State(state): State<RouteState>,
    body: Result<Json<GrantRequest>, JsonRejection>,
) -> Result<Json<IssuedGrant>, Failure> {
    require_local_admin(&state, "grant").await?;
    let Json(request) = body.map_err(|_| Failure(StatusCode::BAD_REQUEST, "invalid_request"))?;
    let now = Utc::now().timestamp();
    if !request.scope.matches(&state.pilot.config)
        || request.workload_id != WORKLOAD_ID
        || !valid_id(&request.id)
        || !valid_id(&request.owner_account_id)
        || request.expires_at <= now
        || request.expires_at > now + MAX_TTL_SECONDS
    {
        state
            .pilot
            .repository
            .audit_denial("grant", "invalid_grant")
            .await
            .map_err(unavailable)?;
        return Err(Failure(StatusCode::BAD_REQUEST, "invalid_grant"));
    }
    let mut bytes = [0u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    let credential = format!("oqto_sa_{}", hex::encode(bytes));
    let grant = state
        .pilot
        .repository
        .issue(&request, &credential, now)
        .await
        .map_err(unavailable)?
        .ok_or(Failure(
            StatusCode::CONFLICT,
            "grant_conflict_or_owner_changed",
        ))?;
    Ok(Json(IssuedGrant { grant, credential }))
}

async fn revoke(
    State(state): State<RouteState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, Failure> {
    require_local_admin(&state, "revoke").await?;
    if !valid_id(&id) {
        return Err(Failure(StatusCode::BAD_REQUEST, "invalid_grant_id"));
    }
    let found = state
        .pilot
        .repository
        .revoke(
            &id,
            &state.pilot.config.workspace_id,
            Utc::now().timestamp(),
        )
        .await
        .map_err(unavailable)?;
    if !found {
        return Err(Failure(StatusCode::NOT_FOUND, "grant_not_found"));
    }
    Ok(Json(
        serde_json::json!({ "schema": "oqto.service-access.v1", "revoked": true, "grant_id": id }),
    ))
}
