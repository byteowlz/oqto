//! Admission regressions use disposable stores/sockets; never a production runner.
use super::*;
use crate::{
    api::AppState,
    auth::{AuthConfig, AuthState},
    db::Database,
    session::{SessionRepository, SessionService, SessionServiceConfig},
    shared_workspace::{SharedWorkspaceRepository, SharedWorkspaceService},
    user::{UserRepository, UserService},
};
use oqto_placement::{
    JsonPlacementStore, PlacementId, PlacementKind, PlacementRecord, PlacementStore,
};

async fn state(root: &std::path::Path) -> (AppState, String) {
    let db = Database::in_memory().await.unwrap();
    let pool = db.pool().clone();
    let users = UserService::new(UserRepository::new(pool.clone()));
    let owner = users
        .create_user(crate::user::CreateUserRequest {
            username: "owner".into(),
            email: "owner@example.test".into(),
            password: None,
            display_name: None,
            role: None,
            external_id: None,
        })
        .await
        .unwrap();
    let local = crate::local::LocalRuntimeConfig {
        workspace_dir: root.display().to_string(),
        single_user: true,
        ..Default::default()
    };
    let sessions = SessionService::with_runner(
        SessionRepository::new(pool.clone()),
        RunnerClient::new(root.join("unused.sock")),
        crate::local::LocalRuntime::new(local.clone()),
        SessionServiceConfig {
            local_config: Some(local),
            single_user: true,
            ..Default::default()
        },
    );
    let mut state = AppState::new(
        sessions,
        users,
        crate::invite::InviteCodeRepository::new(pool.clone()),
        crate::api_keys::ApiKeyRepository::new(pool.clone()),
        AuthState::new(AuthConfig::default()),
        Default::default(),
        Default::default(),
        Default::default(),
        crate::session_target::SessionTargetRepository::new(pool.clone()),
        1024,
    );
    let repo = SharedWorkspaceRepository::new(pool);
    repo.create(
        "sw_mac",
        "Mac fixture",
        "mac-fixture",
        "fixture",
        "/remote/mac",
        &owner.id,
        None,
        "folder",
        "#ffffff",
    )
    .await
    .unwrap();
    repo.add_member(
        "sw_mac",
        &owner.id,
        serde_json::from_value(serde_json::json!("owner")).unwrap(),
        None,
    )
    .await
    .unwrap();
    state.shared_workspaces = Some(Arc::new(SharedWorkspaceService::new(repo)));
    let store = Arc::new(
        JsonPlacementStore::open(root.join("placements.json"))
            .await
            .unwrap(),
    );
    store
        .put(PlacementRecord {
            id: PlacementId("mac-fixture".into()),
            workspace_id: "sw_mac".into(),
            account_id: owner.id.clone(),
            kind: PlacementKind::LocalProcess,
            runner_endpoint: oqto_runner::transport::RunnerEndpointConfig::Unix {
                path: root.join("offline-mac.sock"),
            },
            runtime_name: "fixture".into(),
            spec: None,
        })
        .await
        .unwrap();
    // Even a regressed personal fallback is trapped by a disposable endpoint,
    // never RunnerClient::for_user's ambient default socket.
    for account in [owner.id.as_str(), "ungranted-account"] {
        store
            .put(PlacementRecord {
                id: PlacementId(format!("personal-{account}")),
                workspace_id: account.into(),
                account_id: account.into(),
                kind: PlacementKind::LocalProcess,
                runner_endpoint: oqto_runner::transport::RunnerEndpointConfig::Unix {
                    path: root.join("personal-spy.sock"),
                },
                runtime_name: "fixture".into(),
                spec: None,
            })
            .await
            .unwrap();
    }
    state.placement_store = Some(store);
    (state, owner.id)
}

fn connection() -> Arc<Mutex<WsConnectionState>> {
    let (event_tx, _) = mpsc::unbounded_channel();
    Arc::new(Mutex::new(WsConnectionState {
        subscribed_sessions: HashSet::new(),
        event_tx,
        pi_subscriptions: HashSet::new(),
        pi_forwarders: HashMap::new(),
        response_watchdogs: HashMap::new(),
        pi_session_meta: HashMap::new(),
        terminal_sessions: HashMap::new(),
        file_watchers: HashMap::new(),
        terminal_allowed: false,
    }))
}

#[tokio::test]
async fn denied_or_offline_workspace_never_falls_back_to_personal_execution() {
    let root = tempfile::tempdir().unwrap();
    let (state, owner) = state(root.path()).await;
    let personal_path = root.path().join("personal-spy.sock");
    let personal_listener = tokio::net::UnixListener::bind(&personal_path).unwrap();
    let personal = RunnerClient::new(personal_path);
    for user in ["ungranted-account", owner.as_str()] {
        let command = serde_json::from_value(serde_json::json!({
            "id": "request", "session_id": "platform-admission-fixture", "cmd": "session.create",
            "config": { "harness": "pi", "cwd": "/remote/mac/project" }
        }))
        .unwrap();
        let conn = connection();
        let response = tokio::time::timeout(
            Duration::from_secs(5),
            agent::handle_agent_command(command, user, &state, Some(&personal), Arc::clone(&conn)),
        )
        .await
        .unwrap()
        .unwrap();
        let encoded = serde_json::to_string(&response).unwrap();
        assert!(encoded.contains("no execution started"), "{encoded}");
        assert!(
            !encoded.contains("offline-mac.sock"),
            "transport details leaked"
        );
        assert!(conn.lock().await.pi_session_meta.is_empty());
        assert!(
            state
                .session_targets
                .get("platform-admission-fixture")
                .await
                .unwrap()
                .is_none()
        );
    }
    assert!(
        tokio::time::timeout(Duration::from_millis(50), personal_listener.accept())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn cached_metadata_cannot_bypass_revocation_or_rebind_a_session() {
    let root = tempfile::tempdir().unwrap();
    let (state, owner) = state(root.path()).await;
    state
        .session_targets
        .upsert(&SessionTargetRecord {
            session_id: "bound-session".into(),
            owner_user_id: None,
            scope: SessionTargetScope::SharedWorkspace,
            workspace_id: Some("sw_mac".into()),
            workspace_path: Some("/remote/mac/project".into()),
        })
        .await
        .unwrap();
    let spy_path = root.path().join("personal-spy.sock");
    let spy = tokio::net::UnixListener::bind(&spy_path).unwrap();
    let mac_spy = tokio::net::UnixListener::bind(root.path().join("offline-mac.sock")).unwrap();
    let personal = RunnerClient::new(spy_path);
    let target = ExecutionTarget::SharedWorkspace {
        workspace_id: "sw_mac".into(),
    };
    assert!(
        tokio::time::timeout(
            Duration::from_secs(1),
            crate::runner::router::resolve_service_target(
                &state,
                "ungranted-account",
                &target,
                8080
            )
        )
        .await
        .unwrap()
        .is_err()
    );
    for (user, payload, expected) in [
        (
            "ungranted-account",
            serde_json::json!({"cmd":"get_state"}),
            "access denied",
        ),
        (
            owner.as_str(),
            serde_json::json!({"cmd":"session.create", "config":{"harness":"pi", "cwd":"/different/workspace"}}),
            "cannot be changed",
        ),
    ] {
        let conn = connection();
        conn.lock().await.pi_session_meta.insert(
            "bound-session".into(),
            PiSessionMeta {
                scope: None,
                cwd: Some("/forged/local/path".into()),
            },
        );
        let mut value = payload;
        value["session_id"] = "bound-session".into();
        let response = agent::handle_agent_command(
            serde_json::from_value(value).unwrap(),
            user,
            &state,
            Some(&personal),
            conn,
        )
        .await
        .unwrap();
        assert!(serde_json::to_string(&response).unwrap().contains(expected));
        assert_eq!(
            state
                .session_targets
                .get("bound-session")
                .await
                .unwrap()
                .unwrap()
                .workspace_id
                .as_deref(),
            Some("sw_mac")
        );
    }
    assert!(
        tokio::time::timeout(Duration::from_millis(50), spy.accept())
            .await
            .is_err()
    );
    assert!(
        tokio::time::timeout(Duration::from_millis(50), mac_spy.accept())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn authorized_workspace_resolves_only_its_own_endpoint() {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    let root = tempfile::tempdir().unwrap();
    let (state, owner) = state(root.path()).await;
    let path = root.path().join("offline-mac.sock");
    let listener = tokio::net::UnixListener::bind(&path).unwrap();
    let peer = tokio::spawn(async move {
        for (expected, response) in [
            ("ping", serde_json::json!({"type":"pong"})),
            (
                "get_capabilities",
                serde_json::json!({"type":"runner_capabilities", "protocol_version":1, "harnesses":["pi"], "features":{"command_discovery":true,"model_discovery":true,"fork":true,"extension_ui":true}}),
            ),
        ] {
            let (stream, _) = listener.accept().await.unwrap();
            let mut stream = BufReader::new(stream);
            let mut request = String::new();
            stream.read_line(&mut request).await.unwrap();
            assert!(request.contains(expected));
            let frame = oqto_runner::wire::encode_json_frame(&response).unwrap();
            stream.get_mut().write_all(&frame).await.unwrap();
        }
    });
    let (client, target) = runner_client_for_path(&state, &owner, Some("/remote/mac/project"))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(client.unix_socket_path(), Some(path.as_path()));
    assert_eq!(
        target,
        ExecutionTarget::SharedWorkspace {
            workspace_id: "sw_mac".into()
        }
    );
    peer.await.unwrap();
}

#[tokio::test]
async fn absent_workspace_is_distinct_from_failed_admission() {
    let root = tempfile::tempdir().unwrap();
    let (state, owner) = state(root.path()).await;
    assert!(
        runner_client_for_path(&state, &owner, None)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        runner_client_for_path(&state, "ungranted-account", Some("/remote/mac"))
            .await
            .is_err()
    );
    assert!(
        runner_client_for_path(&state, &owner, Some("/remote/mac"))
            .await
            .is_err()
    );
}
