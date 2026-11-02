use super::*;
use oqto_placement::{PlacementId, PlacementRecord, PlacementStore};
use oqto_runner::{protocol::*, transport::RunnerEndpointConfig};
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};

struct FixturePlacement {
    path: std::path::PathBuf,
    calls: Arc<AtomicUsize>,
}
#[async_trait::async_trait]
impl PlacementStore for FixturePlacement {
    async fn put(&self, _: PlacementRecord) -> anyhow::Result<()> {
        Ok(())
    }
    async fn get(&self, _: &PlacementId) -> anyhow::Result<Option<PlacementRecord>> {
        Ok(None)
    }
    async fn resolve_workspace(&self, _: &str) -> anyhow::Result<Option<RunnerEndpointConfig>> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(Some(RunnerEndpointConfig::Unix {
            path: self.path.clone(),
        }))
    }
    async fn find_workspace(&self, _: &str) -> anyhow::Result<Option<PlacementRecord>> {
        Ok(None)
    }
    async fn list(&self) -> anyhow::Result<Vec<PlacementRecord>> {
        Ok(Vec::new())
    }
    async fn remove(&self, _: &PlacementId) -> anyhow::Result<()> {
        Ok(())
    }
}
fn user(id: &str) -> CurrentUser {
    CurrentUser {
        claims: serde_json::from_value(serde_json::json!({"sub":id,"exp":i64::MAX}))
            .unwrap_or_else(|error| panic!("fixture claims: {error}")),
    }
}
fn state(
    pool: sqlx::SqlitePool,
    targets: crate::session_target::SessionTargetRepository,
    path: &std::path::Path,
    calls: Arc<AtomicUsize>,
) -> AppState {
    let runtime = Arc::new(crate::container::ContainerRuntime::with_binary(
        crate::container::RuntimeType::Podman,
        "/nonexistent-test-runtime",
    ));
    let sessions = crate::session::SessionService::new(
        crate::session::SessionRepository::new(pool.clone()),
        runtime,
        crate::session::SessionServiceConfig::default(),
    );
    AppState::new(
        sessions,
        crate::user::UserService::new(crate::user::UserRepository::new(pool.clone())),
        crate::invite::InviteCodeRepository::new(pool.clone()),
        crate::api_keys::ApiKeyRepository::new(pool),
        crate::auth::AuthState::new(crate::auth::AuthConfig {
            dev_mode: false,
            jwt_secret: Some("isolated-synthetic-media-test-secret".into()),
            ..Default::default()
        }),
        crate::api::VoiceState::default(),
        crate::api::SessionUiState::default(),
        crate::api::TemplatesState::default(),
        targets,
        1024 * 1024,
    )
    .with_linux_users(crate::local::LinuxUsersConfig {
        enabled: true,
        prefix: "media_fixture_".into(),
        ..Default::default()
    })
    .with_runner_socket_pattern(Some(path.to_string_lossy().into_owned()))
    .with_placement_store(Arc::new(FixturePlacement {
        path: path.into(),
        calls,
    }))
}
async fn fixture_runner(
    path: &std::path::Path,
    media: serde_json::Value,
    revoked: Option<(
        crate::shared_workspace::SharedWorkspaceRepository,
        Arc<AtomicBool>,
    )>,
) -> anyhow::Result<tokio::task::JoinHandle<()>> {
    let listener = tokio::net::UnixListener::bind(path)?;
    Ok(tokio::spawn(async move {
        loop {
            let Ok((stream, _)) = listener.accept().await else {
                break;
            };
            let mut stream = tokio::io::BufReader::new(stream);
            let Ok(Some(request)) = oqto_runner::wire::read_json_frame(&mut stream).await else {
                continue;
            };
            let request: RunnerRequest = request;
            let response = match request {
                RunnerRequest::Ping => RunnerResponse::Pong,
                RunnerRequest::GetCapabilities => {
                    RunnerResponse::RunnerCapabilities(RunnerCapabilitiesResponse {
                        protocol_version: RUNNER_WIRE_VERSION,
                        harnesses: vec!["pi".into()],
                        features: RunnerFeatureFlags {
                            command_discovery: false,
                            model_discovery: false,
                            fork: false,
                            extension_ui: false,
                        },
                    })
                }
                RunnerRequest::GetWorkspaceSessionMedia(request) => {
                    assert!(
                        matches!(&request.workspace_scope,oqto_protocol::projection::MediaWorkspaceScope::Within {root} if root==std::path::Path::new("/synthetic/shared")),
                        "controller must propagate the Shared Workspace ceiling, not full-principal scope"
                    );
                    if let Some((repo, flag)) = &revoked
                        && flag.swap(false, Ordering::SeqCst)
                    {
                        repo.remove_member("shared", "reader")
                            .await
                            .unwrap_or_else(|error| panic!("fixture revoke: {error}"));
                    }
                    RunnerResponse::WorkspaceSessionMedia(WorkspaceSessionMediaResponse {
                        session_id: request.session_id,
                        workspace_path: Some("/synthetic/shared".into()),
                        media: media.clone(),
                    })
                }
                _ => panic!("unexpected fixture request"),
            };
            let _ = oqto_runner::wire::write_json_frame(stream.get_mut(), &response).await;
        }
    }))
}
fn payload() -> anyhow::Result<(Vec<u8>, serde_json::Value)> {
    let bytes=STANDARD.decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+j7ioAAAAASUVORK5CYII=")?;
    let hash = hex::encode(Sha256::digest(&bytes));
    let media = serde_json::json!({"source_sha256":hash,"sha256":hash,"variant":"full","mime_type":"image/png","width":1,"height":1,"byte_size":bytes.len(),"data_base64":STANDARD.encode(&bytes)});
    Ok((bytes, media))
}

#[tokio::test]
async fn foreign_user_is_refused_before_any_runner_resolution() -> anyhow::Result<()> {
    let isolated = tempfile::tempdir()?;
    let db = crate::db::Database::in_memory().await?;
    let pool = db.pool().clone();
    let targets = crate::session_target::SessionTargetRepository::new(pool.clone());
    targets
        .upsert(&crate::session_target::SessionTargetRecord {
            session_id: "public-private".into(),
            owner_user_id: Some("owner".into()),
            scope: crate::session_target::SessionTargetScope::Personal,
            workspace_id: None,
            workspace_path: Some("/synthetic/private".into()),
        })
        .await?;
    let calls = Arc::new(AtomicUsize::new(0));
    let state = state(
        pool,
        targets,
        &isolated.path().join("never-contact.sock"),
        calls.clone(),
    );
    let result = get_session_media(
        State(state),
        user("reader"),
        Path(("public-private".into(), "a".repeat(64))),
        Query(MediaQuery {
            variant: WorkspaceMediaVariant::Thumb,
            shared_workspace_id: None,
        }),
    )
    .await;
    assert!(
        matches!(result,Err(ApiError::Forbidden(ref message)) if message=="session does not belong to this user")
    );
    assert_eq!(
        calls.load(Ordering::SeqCst),
        0,
        "foreign user must not reach even a cached placement endpoint"
    );
    Ok(())
}

#[tokio::test]
async fn remote_session_metadata_is_not_an_execution_or_media_grant() -> anyhow::Result<()> {
    let isolated = tempfile::tempdir()?;
    let db = crate::db::Database::in_memory().await?;
    let pool = db.pool().clone();
    let targets = crate::session_target::SessionTargetRepository::new(pool.clone());
    targets
        .upsert(&crate::session_target::SessionTargetRecord {
            session_id: "public-remote".into(),
            owner_user_id: Some("owner".into()),
            scope: crate::session_target::SessionTargetScope::RemoteMachine,
            workspace_id: Some("ungranted-machine".into()),
            workspace_path: Some("/synthetic/remote".into()),
        })
        .await?;
    let calls = Arc::new(AtomicUsize::new(0));
    let state = state(
        pool,
        targets,
        &isolated.path().join("never-contact.sock"),
        calls.clone(),
    );
    assert!(matches!(
        get_session_media(
            State(state),
            user("owner"),
            Path(("public-remote".into(), "a".repeat(64))),
            Query(MediaQuery {
                variant: WorkspaceMediaVariant::Thumb,
                shared_workspace_id: None
            })
        )
        .await,
        Err(ApiError::Forbidden(_))
    ));
    assert_eq!(
        calls.load(Ordering::SeqCst),
        0,
        "metadata/placement cannot substitute for a destination grant"
    );
    Ok(())
}

#[tokio::test]
async fn shared_viewer_reads_original_but_cannot_drive_and_revocation_withholds_bytes()
-> anyhow::Result<()> {
    let isolated = tempfile::tempdir()?;
    let db = crate::db::Database::in_memory().await?;
    let pool = db.pool().clone();
    let users = crate::user::UserRepository::new(pool.clone());
    for id in ["owner", "reader", "outsider"] {
        users
            .create_with_id(
                id,
                crate::user::CreateUserRequest {
                    username: id.into(),
                    email: format!("{id}@fixture.invalid"),
                    password: None,
                    display_name: None,
                    role: None,
                    external_id: None,
                },
            )
            .await?;
    }
    let repo = crate::shared_workspace::SharedWorkspaceRepository::new(pool.clone());
    repo.create(
        "shared",
        "Fixture",
        "fixture",
        "nonexistent_media_fixture",
        "/synthetic/shared",
        "owner",
        None,
        "folder",
        "#000000",
    )
    .await?;
    repo.add_member(
        "shared",
        "owner",
        serde_json::from_value(serde_json::json!("owner"))?,
        None,
    )
    .await?;
    repo.add_member(
        "shared",
        "reader",
        serde_json::from_value(serde_json::json!("viewer"))?,
        Some("owner"),
    )
    .await?;
    let targets = crate::session_target::SessionTargetRepository::new(pool.clone());
    targets
        .upsert(&crate::session_target::SessionTargetRecord {
            session_id: "public-shared".into(),
            owner_user_id: None,
            scope: crate::session_target::SessionTargetScope::SharedWorkspace,
            workspace_id: Some("shared".into()),
            workspace_path: Some("/synthetic/shared".into()),
        })
        .await?;
    let calls = Arc::new(AtomicUsize::new(0));
    let socket = isolated.path().join("runner.sock");
    let state = state(pool, targets, &socket, calls.clone()).with_shared_workspaces(
        crate::shared_workspace::SharedWorkspaceService::new(repo.clone()),
    );
    let (original, media) = payload()?;
    let hash = media["source_sha256"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("hash"))?
        .to_owned();
    let revoke = Arc::new(AtomicBool::new(false));
    let server = fixture_runner(&socket, media, Some((repo, revoke.clone()))).await?;
    let request = || {
        Query(MediaQuery {
            variant: WorkspaceMediaVariant::Full,
            shared_workspace_id: None,
        })
    };
    let response = get_session_media(
        State(state.clone()),
        user("reader"),
        Path(("public-shared".into(), hash.clone())),
        request(),
    )
    .await?;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers()[header::CACHE_CONTROL],
        "private, no-cache"
    );
    assert_eq!(
        axum::body::to_bytes(response.into_body(), 8 * 1024 * 1024)
            .await?
            .as_ref(),
        original.as_slice()
    );
    let before = calls.load(Ordering::SeqCst);
    let target = ExecutionTarget::SharedWorkspace {
        workspace_id: "shared".into(),
    };
    assert!(
        resolve_runner_for_target(&state, "reader", &target)
            .await
            .is_err(),
        "read resolution must not weaken prompt authorization"
    );
    assert_eq!(calls.load(Ordering::SeqCst), before);
    assert!(
        get_session_media(
            State(state.clone()),
            user("outsider"),
            Path(("public-shared".into(), hash.clone())),
            request()
        )
        .await
        .is_err()
    );
    assert_eq!(
        calls.load(Ordering::SeqCst),
        before,
        "nonmember must be denied before placement resolution"
    );
    revoke.store(true, Ordering::SeqCst);
    assert!(
        matches!(
            get_session_media(
                State(state),
                user("reader"),
                Path(("public-shared".into(), hash)),
                request()
            )
            .await,
            Err(ApiError::Forbidden(_))
        ),
        "revocation during media read must withhold response bytes"
    );
    server.abort();
    Ok(())
}
