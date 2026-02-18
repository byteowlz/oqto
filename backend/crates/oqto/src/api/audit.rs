//! Audit middleware for HTTP requests.

use axum::body::Body;
use axum::extract::State;
use axum::http::Request;
use axum::middleware::Next;
use axum::response::Response;
use std::time::Instant;

use crate::auth::CurrentUser;

use super::state::AppState;

pub async fn audit_middleware(
    State(state): State<AppState>,
    req: Request<Body>,
    next: Next,
) -> Response {
    let start = Instant::now();
    let method = req.method().to_string();
    let path = req.uri().path().to_string();
    let user_id = req
        .extensions()
        .get::<CurrentUser>()
        .map(|user| user.id().to_string());

    let response = next.run(req).await;

    if let (Some(logger), Some(user_id)) = (state.audit_logger.as_ref(), user_id) {
        let status = response.status().as_u16();
        let duration_ms = start.elapsed().as_millis();
        logger
            .log_http(&user_id, &method, &path, status, duration_ms)
            .await;
    }

    response
}
