//! Human credential administration over authenticated host ingress only.
//! Never register these handlers on an App Bridge or agent capability endpoint.

use std::net::{IpAddr, SocketAddr};

use axum::extract::{ConnectInfo, DefaultBodyLimit, State, rejection::JsonRejection};
use axum::http::{HeaderMap, HeaderValue, header};
use axum::routing::{get, post};
use axum::{Json, Router};
use oqto_protocol::credentials::{
    CredentialCommand, CredentialManageRequest, CredentialManageResult,
};
use tower_http::set_header::SetResponseHeaderLayer;

use crate::api::{ApiError, ApiResult, AppState};
use crate::auth::CurrentUser;
use crate::runner::router::{ExecutionTarget, resolve_runner_for_target};

pub fn credential_routes() -> Router<AppState> {
    Router::new()
        .route("/credentials", get(status))
        .route("/credentials/manage", post(manage))
        .layer(DefaultBodyLimit::max(512 * 1024))
        .layer(SetResponseHeaderLayer::overriding(
            header::CACHE_CONTROL,
            HeaderValue::from_static("no-store"),
        ))
}

async fn status(
    State(state): State<AppState>,
    user: CurrentUser,
) -> ApiResult<Json<CredentialManageResult>> {
    require_real_auth(&state)?;
    dispatch(&state, user.id(), CredentialCommand::Status)
        .await
        .map(Json)
}

async fn manage(
    State(state): State<AppState>,
    user: CurrentUser,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    body: Result<Json<CredentialCommand>, JsonRejection>,
) -> ApiResult<Json<CredentialManageResult>> {
    require_real_auth(&state)?;
    validate_submission_channel(Some(peer), &headers, state.auth.allowed_origins())?;
    // JsonRejection diagnostics can contain user-supplied field/variant names.
    // Do not return or log them on a credential-bearing route.
    let Json(command) =
        body.map_err(|_| ApiError::bad_request("Invalid credential management request"))?;
    dispatch(&state, user.id(), command).await.map(Json)
}

pub(super) fn require_real_auth(state: &AppState) -> ApiResult<()> {
    if state.auth.is_dev_mode() {
        return Err(ApiError::forbidden(
            "Credential administration requires authenticated mode, not development authentication bypass",
        ));
    }
    Ok(())
}

async fn dispatch(
    state: &AppState,
    account_id: &str,
    command: CredentialCommand,
) -> ApiResult<CredentialManageResult> {
    // Personal Account storage only. No caller-selected account, workspace,
    // Principal, machine, provider path or runner endpoint exists in the body.
    let runner = resolve_runner_for_target(state, account_id, &ExecutionTarget::Personal)
        .await
        .map_err(|_| ApiError::service_unavailable("Personal credential runner unavailable"))?
        .ok_or_else(|| ApiError::service_unavailable("Personal credential runner unavailable"))?;
    runner
        .manage_credentials(CredentialManageRequest {
            account_id: account_id.to_owned(),
            command,
        })
        .await
        .map_err(|_| ApiError::service_unavailable("Credential management unavailable"))
}

/// The HTTP backend itself is plain HTTP: keep raw-secret ingress local, or
/// behind an operator-owned TLS reverse proxy connecting over loopback.
/// Forwarded headers are deliberately not authentication evidence. Browser
/// requests require an exact configured trusted HTTPS/loopback Origin; CLI
/// requests require explicit header auth (and the CLI verifies its TLS URL).
pub(super) fn validate_submission_channel(
    peer: Option<SocketAddr>,
    headers: &HeaderMap,
    allowed_origins: &[String],
) -> ApiResult<()> {
    if !peer.is_some_and(|addr| addr.ip().is_loopback()) {
        return Err(ApiError::forbidden(
            "Credential submission requires loopback ingress or a trusted local TLS proxy",
        ));
    }
    if let Some(origin) = headers.get(header::ORIGIN) {
        let origin = origin
            .to_str()
            .map_err(|_| ApiError::forbidden("Credential submission Origin rejected"))?;
        if !allowed_origins.iter().any(|allowed| allowed == origin) || !protected_origin(origin) {
            return Err(ApiError::forbidden("Credential submission Origin rejected"));
        }
    } else {
        let bearer = headers
            .get(header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.strip_prefix("Bearer "))
            .is_some_and(|value| !value.is_empty());
        let api_key = headers
            .get("x-api-key")
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| !value.is_empty());
        if !bearer && !api_key {
            return Err(ApiError::forbidden(
                "Origin-free credential submission requires explicit header authentication",
            ));
        }
    }
    Ok(())
}

fn protected_origin(origin: &str) -> bool {
    let Ok(url) = reqwest::Url::parse(origin) else {
        return false;
    };
    if !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || url.path() != "/"
    {
        return false;
    }
    match url.scheme() {
        "https" => url.host_str().is_some(),
        "http" => url.host_str().is_some_and(|host| {
            host == "localhost"
                || host
                    .trim_matches(['[', ']'])
                    .parse::<IpAddr>()
                    .is_ok_and(|ip| ip.is_loopback())
        }),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn local() -> Option<SocketAddr> {
        Some(SocketAddr::from(([127, 0, 0, 1], 8080)))
    }

    #[test]
    fn configured_browser_origin_must_be_protected_and_exact() {
        for origin in [
            "https://oqto.example",
            "http://localhost:3000",
            "http://127.0.0.1:3000",
            "http://[::1]:3000",
        ] {
            let mut headers = HeaderMap::new();
            headers.insert(header::ORIGIN, HeaderValue::from_str(origin).unwrap());
            assert!(validate_submission_channel(local(), &headers, &[origin.to_owned()]).is_ok());
            assert!(validate_submission_channel(local(), &headers, &[]).is_err());
        }
        for origin in [
            "null",
            "http://10.0.0.10:3000",
            "http://localhost.evil",
            "https://user:pass@oqto.example",
            "https://oqto.example/path",
            "https://oqto.example?token=synthetic",
        ] {
            let mut headers = HeaderMap::new();
            headers.insert(header::ORIGIN, HeaderValue::from_str(origin).unwrap());
            assert!(validate_submission_channel(local(), &headers, &[origin.to_owned()]).is_err());
        }
    }

    #[test]
    fn missing_remote_peer_and_cookie_only_cli_cannot_submit() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::COOKIE,
            HeaderValue::from_static("auth_token=synthetic-only"),
        );
        assert!(validate_submission_channel(local(), &headers, &[]).is_err());
        headers.insert(
            header::AUTHORIZATION,
            HeaderValue::from_static("Bearer synthetic-only"),
        );
        assert!(validate_submission_channel(local(), &headers, &[]).is_ok());
        assert!(validate_submission_channel(None, &headers, &[]).is_err());
        assert!(
            validate_submission_channel(
                Some(SocketAddr::from(([10, 0, 0, 2], 8080))),
                &headers,
                &[]
            )
            .is_err()
        );
        // Spoofed proxy headers do not upgrade a remote peer to trusted ingress.
        headers.insert("x-forwarded-proto", HeaderValue::from_static("https"));
        assert!(
            validate_submission_channel(
                Some(SocketAddr::from(([10, 0, 0, 2], 8080))),
                &headers,
                &[]
            )
            .is_err()
        );
    }
}
