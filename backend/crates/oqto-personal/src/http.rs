//! Enrollment-only HTTPS adapter for composition into the Runner. This does not
//! supervise processes or expose grant issuance, sessions, Files or terminals.
use crate::PairingAuthority;
use anyhow::Result;
use axum::{
    Json, Router,
    body::Bytes,
    extract::{DefaultBodyLimit, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::post,
};
use hyper::server::conn::http1;
use hyper_util::rt::{TokioIo, TokioTimer};
use oqto_personal_wire::{ENROLL_PATH, EnrollmentReply, MAX_ENROLLMENT_BYTES};
use std::{future::Future, sync::Arc, time::Duration};
use tokio::{net::TcpListener, sync::Semaphore, task::JoinSet};
use tokio_rustls::{TlsAcceptor, rustls::ServerConfig};
use tower::ServiceExt;

pub fn enrollment_router(authority: Arc<PairingAuthority>) -> Router {
    Router::new()
        .route(ENROLL_PATH, post(enroll))
        .layer(DefaultBodyLimit::max(MAX_ENROLLMENT_BYTES))
        .with_state(authority)
}
async fn enroll(State(authority): State<Arc<PairingAuthority>>, body: Bytes) -> Response {
    match authority.enroll_json(&body).await {
        Ok(device_id) => Json(EnrollmentReply {
            version: 1,
            host_id: authority.host_id().into(),
            device_id,
        })
        .into_response(),
        Err(_) => (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"error":"enrollment_refused"})),
        )
            .into_response(),
    }
}

/// Bounded enrollment-only listener. Use the SAME configured leaf DER to issue
/// QR pins. Caller owns listener/config/shutdown; no background service is installed.
/// Connection deadlines here are not a long-lived session/PTY stream lifecycle.
pub async fn serve_enrollment_tls(
    listener: TcpListener,
    config: Arc<ServerConfig>,
    authority: Arc<PairingAuthority>,
    shutdown: impl Future<Output = ()>,
) -> Result<()> {
    let mut config = (*config).clone();
    config.alpn_protocols = vec![b"http/1.1".to_vec()];
    config.max_early_data_size = 0;
    config.send_tls13_tickets = 0;
    let acceptor = TlsAcceptor::from(Arc::new(config));
    let router = enrollment_router(authority);
    let slots = Arc::new(Semaphore::new(64));
    let mut connections = JoinSet::new();
    tokio::pin!(shutdown);
    loop {
        tokio::select! {
            _ = &mut shutdown => break,
            Some(_) = connections.join_next(), if !connections.is_empty() => {},
            accepted = listener.accept() => {
                let (stream, _) = accepted?;
                let Ok(slot) = Arc::clone(&slots).try_acquire_owned() else { continue; };
                let acceptor = acceptor.clone(); let router = router.clone();
                connections.spawn(async move {
                    let _slot = slot;
                    let Ok(Ok(tls)) = tokio::time::timeout(Duration::from_secs(5), acceptor.accept(stream)).await else { return; };
                    let service = hyper::service::service_fn(move |request| router.clone().oneshot(request));
                    let mut builder = http1::Builder::new();
                    builder.timer(TokioTimer::new()).header_read_timeout(Duration::from_secs(5));
                    let _ = tokio::time::timeout(Duration::from_secs(30), builder.serve_connection(TokioIo::new(tls), service)).await;
                });
            }
        }
    }
    connections.abort_all();
    while connections.join_next().await.is_some() {}
    Ok(())
}
