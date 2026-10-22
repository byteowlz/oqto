//! Opt-in production JWT/ws_mux + real Runner/Pi acceptance fixture.
#[cfg(test)]
use super::*;
use crate::{
    api::AppState,
    auth::{AuthConfig, AuthState},
    db::Database,
    session::{SessionRepository, SessionService, SessionServiceConfig},
    shared_workspace::{SharedWorkspaceRepository, SharedWorkspaceService},
    user::{UserRepository, UserService},
};
use anyhow::{Context, Result, ensure};
use futures::{SinkExt, StreamExt};
use oqto_placement::{
    JsonPlacementStore, PlacementId, PlacementKind, PlacementRecord, PlacementStore,
};
use oqto_runner::{
    daemon::{
        config::RunnerUserConfig,
        server::{Runner, SessionBinaries},
    },
    pi_manager::{PiManagerConfig, PiSessionManager},
};
use serde_json::{Value, json};
use std::{
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
};
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, tungstenite::Message};

type Socket = WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>;

struct Client {
    socket: Socket,
    events: Vec<Value>,
}
impl Client {
    async fn connect(url: &str, token: &str) -> Result<Self> {
        let (socket, _) = tokio_tungstenite::connect_async(format!("{url}?token={token}")).await?;
        Ok(Self {
            socket,
            events: vec![],
        })
    }
    async fn command(&mut self, session: &str, cmd: &str, fields: Value) -> Result<Value> {
        let id = uuid::Uuid::new_v4().to_string();
        let mut request = json!({"channel":"agent","session_id":session,"id":id,"cmd":cmd});
        request
            .as_object_mut()
            .context("request object")?
            .extend(fields.as_object().context("fields object")?.clone());
        self.socket
            .send(Message::Text(request.to_string().into()))
            .await?;
        tokio::time::timeout(Duration::from_secs(20), async {
            loop {
                let msg = self.socket.next().await.context("socket closed")??;
                if let Message::Text(text) = msg {
                    let event: Value = serde_json::from_str(&text)?;
                    let matched = event["channel"] == "agent"
                        && event["event"] == "response"
                        && event["id"] == id;
                    self.events.push(event.clone());
                    if matched {
                        ensure!(
                            event["session_id"] == session && event["cmd"] == cmd,
                            "response identity changed: {event}"
                        );
                        return Ok(event);
                    }
                }
            }
        })
        .await
        .context("canonical response timeout")?
    }
    async fn control(&mut self, session: &str, op: &str, args: Value) -> Result<Value> {
        self.command(
            session,
            "extension_control",
            json!({"operation":op,"arguments":args}),
        )
        .await
    }
    async fn ok(&mut self, session: &str, op: &str, args: Value) -> Result<Value> {
        let event = self.control(session, op, args).await?;
        ensure!(
            event["success"] == true && event["data"]["reply"]["ok"] == true,
            "control failed: {event}"
        );
        ensure!(
            event["data"]["operation"] == op && event["data"]["binding_id"].as_str().is_some(),
            "missing trusted identity: {event}"
        );
        Ok(event["data"].clone())
    }
}

async fn fixture_state(root: &Path, socket: &Path) -> Result<(AppState, Vec<String>, Vec<String>)> {
    let db = Database::new(&root.join("accounts-sessions.sqlite")).await?;
    let pool = db.pool().clone();
    let users = UserService::new(UserRepository::new(pool.clone()));
    let auth = AuthState::new(AuthConfig {
        dev_mode: false,
        jwt_secret: Some("synthetic-private-selection-fixture-secret-not-a-real-credential".into()),
        allowed_origins: vec!["http://127.0.0.1:3000".into()],
        ..Default::default()
    });
    let mut accounts = vec![];
    let mut tokens = vec![];
    for name in ["fixture-a", "fixture-b"] {
        let account = users
            .create_user(crate::user::CreateUserRequest {
                username: name.into(),
                email: format!("{name}@example.test"),
                password: None,
                display_name: None,
                role: None,
                external_id: None,
            })
            .await?;
        tokens.push(auth.generate_token(&account.id, &account.email, name, "user")?);
        accounts.push(account.id);
    }
    let local = crate::local::LocalRuntimeConfig {
        workspace_dir: root.display().to_string(),
        single_user: true,
        ..Default::default()
    };
    let sessions = SessionService::with_runner(
        SessionRepository::new(pool.clone()),
        RunnerClient::new(socket),
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
        auth,
        Default::default(),
        Default::default(),
        Default::default(),
        crate::session_target::SessionTargetRepository::new(pool.clone()),
        1024,
    );
    // A private socket pattern prevents even connection-level fallback from touching /run/user.
    let linux = crate::local::LinuxUsersConfig {
        enabled: true,
        ..Default::default()
    };
    state.linux_users = Some(linux);
    state.runner_socket_pattern = Some(socket.display().to_string());
    let store = Arc::new(JsonPlacementStore::open(root.join("placements.json")).await?);
    for account in &accounts {
        store
            .put(PlacementRecord {
                id: PlacementId(format!("private-{account}")),
                workspace_id: account.clone(),
                account_id: account.clone(),
                kind: PlacementKind::LocalProcess,
                runner_endpoint: oqto_runner::transport::RunnerEndpointConfig::Unix {
                    path: socket.into(),
                },
                runtime_name: "selection-fixture".into(),
                spec: None,
            })
            .await?;
    }
    let shared = SharedWorkspaceRepository::new(pool);
    shared
        .create(
            "private-shared",
            "Private shared",
            "fixture-shared",
            "fixture",
            &root.join("shared").display().to_string(),
            &accounts[0],
            None,
            "folder",
            "#ffffff",
        )
        .await?;
    shared
        .add_member(
            "private-shared",
            &accounts[0],
            serde_json::from_value(json!("owner"))?,
            None,
        )
        .await?;
    store
        .put(PlacementRecord {
            id: PlacementId("private-shared-placement".into()),
            workspace_id: "private-shared".into(),
            account_id: accounts[0].clone(),
            kind: PlacementKind::LocalProcess,
            runner_endpoint: oqto_runner::transport::RunnerEndpointConfig::Unix {
                path: socket.into(),
            },
            runtime_name: "selection-fixture".into(),
            spec: None,
        })
        .await?;
    state.shared_workspaces = Some(Arc::new(SharedWorkspaceService::new(shared)));
    state.placement_store = Some(store);
    Ok((state, accounts, tokens))
}

fn private_runtime(root: &Path) -> Result<Arc<PiSessionManager>> {
    let binary = PathBuf::from(std::env::var("OQTO_SELECTION_PI_BINARY")?);
    let extensions = PathBuf::from(std::env::var("OQTO_SELECTION_EXTENSION_ROOT")?);
    for path in [root, &binary, &extensions] {
        ensure!(
            !path.to_string_lossy().contains('\''),
            "unsupported fixture path"
        );
    }
    let version = std::process::Command::new(&binary)
        .arg("--version")
        .output()?;
    ensure!(
        version.status.success() && String::from_utf8(version.stdout)?.trim() == "1.0.0",
        "exact Pi 1 required"
    );
    let wrapper = root.join("pi-private");
    std::fs::write(
        &wrapper,
        format!(
            "#!/bin/sh\nprintf '%s\\n' \"$$\" >> '{}/child-pids'\nexec '{}' --offline -ne -ns -np -nc -na --no-tools --no-session --provider fixture --model fixture -e '{}/pi-selection/index.ts' --selection-state-dir '{}/reviews' -e '{}/pi-capabilities/index.ts' \"$@\"\n",
            root.display(),
            binary.display(),
            extensions.display(),
            root.display(),
            extensions.display()
        ),
    )?;
    std::fs::set_permissions(&wrapper, std::fs::Permissions::from_mode(0o700))?;
    let agent = PathBuf::from(std::env::var("PI_CODING_AGENT_DIR")?);
    std::fs::write(
        agent.join("models.json"),
        serde_json::to_vec(
            &json!({"providers":{"fixture":{"baseUrl":"http://127.0.0.1:9/v1","api":"openai-completions","apiKey":"synthetic","models":[{"id":"fixture","input":["text"],"contextWindow":4096,"maxTokens":256}]}}}),
        )?,
    )?;
    Ok(PiSessionManager::new(PiManagerConfig {
        pi_binary: wrapper,
        default_cwd: root.join("workdir"),
        idle_timeout_secs: 7200,
        cleanup_interval_secs: 60,
        sandbox_config: None,
        runner_id: "private-auth-selection".into(),
        model_cache_dir: None,
        extra_deny_read: vec![],
    }))
}

#[tokio::test]
#[ignore = "requires whole-process private HOME; use run-auth-selection-proof.sh"]
async fn authenticated_real_selection_gateway() -> Result<()> {
    let root = PathBuf::from(std::env::var("OQTO_SELECTION_PROOF_ROOT")?).canonicalize()?;
    ensure!(
        root.starts_with("/tmp") && root != Path::new("/tmp"),
        "private root required"
    );
    let home = PathBuf::from(std::env::var("HOME")?).canonicalize()?;
    ensure!(
        home == root.join("home"),
        "private whole-process HOME required"
    );
    for key in [
        "XDG_STATE_HOME",
        "XDG_CONFIG_HOME",
        "XDG_DATA_HOME",
        "XDG_CACHE_HOME",
        "PI_CODING_AGENT_DIR",
        "XDG_RUNTIME_DIR",
    ] {
        ensure!(
            Path::new(&std::env::var(key)?).starts_with(&home),
            "{key} must be private"
        );
    }
    for dir in ["workdir", "workdir-b", "shared", "browser-sockets"] {
        std::fs::create_dir_all(root.join(dir))?;
    }
    let manager = private_runtime(&root)?;
    let socket = root.join("runner.sock");
    let runner = Arc::new(Runner::new(
        None,
        SessionBinaries {
            fileserver: "/bin/false".into(),
            ttyd: "/bin/false".into(),
        },
        RunnerUserConfig {
            credentials: Default::default(),
            provider_login: None,
            history_read: None,
            fileserver_binary: "/bin/false".into(),
            ttyd_binary: "/bin/false".into(),
            pi_binary: root.join("pi-private").display().to_string(),
            personal_source: None,
            runner_id: "private-auth-selection".into(),
            workspace_dir: root.clone(),
            pi_sessions_dir: home.join(".pi/agent/sessions"),
            memories_dir: root.join("memories"),
            single_user: true,
            linux_users_enabled: false,
            terminal_enabled: false,
        },
        manager.clone(),
        None,
    ));
    let running = runner.clone();
    let runner_task = tokio::spawn(async move { running.run(&socket).await });
    let socket = root.join("runner.sock");
    let runner_client = RunnerClient::new(&socket);
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if socket.exists() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await?;
    runner_client.ensure_ready_with_recovery().await?;
    let tls = root.join("tls");
    let tls_listener = oqto_runner::tls::TcpTlsRunnerListener::bind(
        "127.0.0.1:0".parse()?,
        oqto_runner::tls::server_config(
            &tls.join("ca.pem"),
            &tls.join("server.pem"),
            &tls.join("server.key"),
        )?,
    )
    .await?;
    let tls_address = tls_listener.local_addr();
    let running = runner.clone();
    let tls_task = tokio::spawn(async move {
        running
            .run_transport(
                &tls_listener,
                oqto_runner::daemon::server::ConnectionAccess::RemoteInventory,
            )
            .await
    });
    let (mut state, accounts, tokens) = fixture_state(&root, &socket).await?;
    let principal_output = std::process::Command::new("/usr/bin/id")
        .arg("-un")
        .output()?;
    ensure!(
        principal_output.status.success(),
        "cannot resolve actual fixture principal"
    );
    let principal = String::from_utf8(principal_output.stdout)?
        .trim()
        .to_owned();
    let endpoint = oqto_runner::transport::RunnerEndpointConfig::TcpTls {
        address: tls_address,
        server_name: "fixture-server".into(),
        ca: tls.join("ca.pem"),
        certificate: tls.join("client.pem"),
        key: tls.join("client.key"),
    };
    let targets = vec![
        crate::runner::targets::RunnerTargetConfig {
            id: "private-granted-machine".into(),
            label: "Disposable loopback runner".into(),
            account_ids: vec![accounts[0].clone()],
            endpoint: endpoint.clone(),
            provider_login: false,
            history_read: false,
            execution: Some(crate::runner::targets::RemoteExecutionGrant {
                principal: principal.clone(),
                roots: vec![root.join("remote")],
            }),
        },
        crate::runner::targets::RunnerTargetConfig {
            id: "ungranted-machine".into(),
            label: "Reachable inventory without execution".into(),
            account_ids: vec![accounts[0].clone()],
            endpoint: endpoint.clone(),
            provider_login: false,
            history_read: false,
            execution: None,
        },
    ];
    std::fs::write(
        root.join("runner-targets.json"),
        serde_json::to_vec_pretty(&targets)?,
    )?;
    state.runner_targets = Arc::new(crate::runner::targets::RunnerTargets::new(targets)?);
    let inventory_client = RunnerClient::from_endpoint(&endpoint)?;
    inventory_client.ensure_ready_with_recovery().await?;
    ensure!(
        state
            .runner_targets
            .execution_grant(&accounts[0], "ungranted-machine")
            .is_err(),
        "inventory acquired execution grant"
    );
    std::fs::create_dir_all(root.join("remote"))?;
    let session = format!("oqto-a-{}", uuid::Uuid::new_v4());
    let other = format!("oqto-b-{}", uuid::Uuid::new_v4());
    let shared_session = format!("oqto-shared-{}", uuid::Uuid::new_v4());
    let no_grant = format!("oqto-no-grant-{}", uuid::Uuid::new_v4());
    let remote_session = format!("oqto-remote-{}", uuid::Uuid::new_v4());
    let outside_root = format!("oqto-outside-root-{}", uuid::Uuid::new_v4());
    for (id, owner, scope, workspace, dir) in [
        (
            &session,
            Some(accounts[0].clone()),
            SessionTargetScope::Personal,
            None,
            "workdir",
        ),
        (
            &other,
            Some(accounts[1].clone()),
            SessionTargetScope::Personal,
            None,
            "workdir-b",
        ),
        (
            &shared_session,
            None,
            SessionTargetScope::SharedWorkspace,
            Some("private-shared".into()),
            "shared",
        ),
        (
            &remote_session,
            Some(accounts[0].clone()),
            SessionTargetScope::RemoteMachine,
            Some("private-granted-machine".into()),
            "remote",
        ),
        (
            &outside_root,
            Some(accounts[0].clone()),
            SessionTargetScope::RemoteMachine,
            Some("private-granted-machine".into()),
            "workdir",
        ),
        (
            &no_grant,
            Some(accounts[0].clone()),
            SessionTargetScope::RemoteMachine,
            Some("ungranted-machine".into()),
            "workdir",
        ),
    ] {
        state
            .session_targets
            .upsert(&SessionTargetRecord {
                session_id: id.clone(),
                owner_user_id: owner,
                scope,
                workspace_id: workspace,
                workspace_path: Some(root.join(dir).display().to_string()),
            })
            .await?;
    }
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let address = listener.local_addr()?;
    let url = format!("ws://{address}/api/ws/mux");
    let app = axum::Router::new().nest(
        "/api",
        crate::api::create_router_with_config(state.clone(), 1),
    );
    let server_task = tokio::spawn(async move { axum::serve(listener, app).await });
    let contract = json!({"url":url,"http_origin":format!("http://{address}"),"root":root,"accounts":accounts,"tokens":tokens,"session_id":session,"other_session_id":other,"shared_session_id":shared_session,"no_grant_session_id":no_grant,"workdir":root.join("workdir"),"other_workdir":root.join("workdir-b"),"remote_session_id":remote_session,"remote_workdir":root.join("remote"),"actual_principal":principal,"runner_tls_address":tls_address.to_string(),"status":"startup"});
    std::fs::write(
        root.join("contract.json"),
        serde_json::to_vec_pretty(&contract)?,
    )?;
    std::fs::write(
        "/tmp/oqto-selection-auth-gateway-last-root.txt",
        root.display().to_string(),
    )?;
    ensure!(
        tokio_tungstenite::connect_async(&url).await.is_err(),
        "unauthenticated upgrade allowed"
    );
    ensure!(
        tokio_tungstenite::connect_async(format!("{url}?token=invalid-synthetic-token"))
            .await
            .is_err(),
        "invalid JWT allowed"
    );
    let mut a = Client::connect(&url, &tokens[0]).await?;
    let mut b = Client::connect(&url, &tokens[1]).await?;
    for (client, sid, dir) in [(&mut a, &session, "workdir"), (&mut b, &other, "workdir-b")] {
        let created = client
            .command(
                sid,
                "session.create",
                json!({"config":{"harness":"pi","cwd":root.join(dir)}}),
            )
            .await?;
        ensure!(created["success"] == true, "attach failed: {created}");
        let state = client.command(sid, "get_state", json!({})).await?;
        ensure!(state["success"] == true, "state failed: {state}");
    }
    for (account_b, sid) in [
        (true, session.as_str()),
        (false, other.as_str()),
        (true, shared_session.as_str()),
        (false, no_grant.as_str()),
        (false, outside_root.as_str()),
        (true, remote_session.as_str()),
        (false, "_system"),
        (false, "pending-fixture"),
        (false, "native-only-id"),
    ] {
        let client = if account_b { &mut b } else { &mut a };
        let denied = client
            .control(sid, "selection_get", json!({"id":"cannot-read"}))
            .await?;
        ensure!(
            denied["success"] == false,
            "unauthorized control admitted: {denied}"
        );
    }
    for (sid, dir) in [(&shared_session, "shared"), (&remote_session, "remote")] {
        let created = a
            .command(
                sid,
                "session.create",
                json!({"config":{"harness":"pi","cwd":root.join(dir)}}),
            )
            .await?;
        ensure!(
            created["success"] == true,
            "authorized mapped attach failed: {created}"
        );
        let state = a.command(sid, "get_state", json!({})).await?;
        ensure!(
            state["success"] == true,
            "mapped native state failed: {state}"
        );
    }
    let shared_binding = a.ok(&shared_session, "presentation_bind", json!({"clientKind":"oqto-web","capabilities":["selection.questions.v1"],"leaseMs":60000})).await?;
    let remote_binding = a.ok(&remote_session, "presentation_bind", json!({"clientKind":"oqto-web","capabilities":["selection.questions.v1"],"leaseMs":60000})).await?;
    let bound = a.ok(&session, "presentation_bind", json!({"clientKind":"oqto-web","capabilities":["selection.questions.v1","selection.review.v1"],"leaseMs":60000})).await?;
    let binding = bound["binding_id"].as_str().context("binding")?.to_owned();
    ensure!(
        shared_binding["binding_id"] != binding
            && remote_binding["binding_id"] != binding
            && shared_binding["binding_id"] != remote_binding["binding_id"],
        "same connection reused binding across Sessions"
    );
    let native = bound["native_session_id"]
        .as_str()
        .context("native id")?
        .to_owned();
    ensure!(
        native != session && manager.session_external_id(&session).await == native,
        "real native identity mismatch"
    );
    let denied_native = a
        .control(&native, "selection_get", json!({"id":"cannot-read"}))
        .await?;
    ensure!(
        denied_native["success"] == false,
        "native id accepted as public authority"
    );
    let membership_db = Database::new(&root.join("accounts-sessions.sqlite")).await?;
    let memberships = SharedWorkspaceRepository::new(membership_db.pool().clone());
    memberships
        .add_member(
            "private-shared",
            &accounts[1],
            serde_json::from_value(json!("member"))?,
            None,
        )
        .await?;
    let member_binding = b.ok(&shared_session, "presentation_bind", json!({"clientKind":"oqto-web","capabilities":["selection.questions.v1"],"leaseMs":60000})).await?;
    ensure!(
        member_binding["binding_id"] != shared_binding["binding_id"],
        "Accounts shared connection identity"
    );
    memberships
        .remove_member("private-shared", &accounts[1])
        .await?;
    let revoked = b
        .control(&shared_session, "presentation_list", json!({}))
        .await?;
    ensure!(
        revoked["success"] == false,
        "cached binding survived membership revocation"
    );
    let mut a_second = Client::connect(&url, &tokens[0]).await?;
    let attached = a_second
        .command(
            &session,
            "session.create",
            json!({"config":{"harness":"pi","cwd":root.join("workdir")}}),
        )
        .await?;
    ensure!(
        attached["success"] == true,
        "second subscriber failed: {attached}"
    );
    let second = a_second.ok(&session, "presentation_bind", json!({"clientKind":"oqto-web","capabilities":["selection.questions.v1"],"leaseMs":60000})).await?;
    ensure!(second["binding_id"] != binding, "connection binding reused");
    let b_bound = b.ok(&other, "presentation_bind", json!({"clientKind":"oqto-web","capabilities":["selection.questions.v1"],"leaseMs":60000})).await?;
    ensure!(
        b_bound["binding_id"] != binding && b_bound["native_session_id"] != native,
        "Account/Session isolation collapsed"
    );
    let created = a.ok(&session, "selection_create", json!({"presentation":"host","spec":{"version":1,"mode":"questions","title":"Authenticated selection","questions":[{"id":"choice","header":"Choice","title":"Choose","kind":"single","required":true,"options":[{"id":"yes","label":"Yes"},{"id":"no","label":"No"}]}]}})).await?;
    let review = created["reply"]["result"]["record"]["id"]
        .as_str()
        .context("created review id")?
        .to_owned();
    a.ok(&session, "selection_ack", json!({"id":review}))
        .await?;
    let answers = json!({"choice":{"answered":true,"selectedIds":["yes"]}});
    let saved = a
        .ok(
            &session,
            "selection_save",
            json!({"id":review,"revision":0,"answers":answers}),
        )
        .await?;
    ensure!(
        saved["binding_id"] == binding,
        "same connection/session binding changed"
    );
    let stale = a
        .control(
            &session,
            "selection_close",
            json!({"id":review,"revision":0}),
        )
        .await?;
    ensure!(
        stale["success"] == true
            && stale["data"]["reply"]["ok"] == false
            && stale["data"]["reply"]["error"] == "Revision conflict",
        "stale CAS not rejected: {stale}"
    );
    let submitted = a
        .ok(
            &session,
            "selection_submit",
            json!({"id":review,"revision":1,"answers":answers}),
        )
        .await?;
    let terminal = a
        .ok(&session, "selection_get", json!({"id":review}))
        .await?;
    ensure!(
        terminal["reply"]["result"]["record"]["revision"] == 2
            && terminal["reply"]["result"]["record"]["state"] == "submitted",
        "terminal record missing: {terminal}"
    );
    let second_read = a_second
        .ok(&session, "selection_get", json!({"id":review}))
        .await?;
    ensure!(
        second_read["reply"]["result"] == terminal["reply"]["result"],
        "authorized same Session read diverged"
    );
    ensure!(
        a_second
            .events
            .iter()
            .any(|event| event["session_id"] == session
                && event["event"] == "status"
                && event["key"] == "pi-selection:present/v1"),
        "canonical Status not fanned out to authorized subscriber"
    );
    let foreign_review = b
        .control(&other, "selection_get", json!({"id":review}))
        .await?;
    ensure!(
        foreign_review["success"] == true && foreign_review["data"]["reply"]["ok"] == false,
        "review escaped native scope: {foreign_review}"
    );
    a.socket.close(None).await?;
    let mut reconnect = Client::connect(&url, &tokens[0]).await?;
    let rebound = reconnect.ok(&session, "presentation_bind", json!({"clientKind":"oqto-web","capabilities":["selection.questions.v1"],"leaseMs":60000})).await?;
    ensure!(
        rebound["binding_id"] != binding && rebound["binding_id"] != second["binding_id"],
        "reconnect adopted old binding"
    );
    let reloaded = reconnect
        .ok(&session, "selection_get", json!({"id":review}))
        .await?;
    ensure!(
        reloaded["reply"]["result"] == terminal["reply"]["result"],
        "durable reload changed answer"
    );
    let presents: Vec<_> = a
        .events
        .iter()
        .filter(|event| {
            event["event"] == "status"
                && event["key"] == "pi-selection:present/v1"
                && event["text"].is_string()
        })
        .collect();
    ensure!(
        !presents.is_empty(),
        "actual canonical Status presentation missing: {:?}",
        a.events
    );
    for event in &presents {
        ensure!(
            event["session_id"] == session,
            "Status public routing changed"
        );
        let payload: Value =
            serde_json::from_str(event["text"].as_str().context("Status JSON missing")?)?;
        ensure!(
            payload["review"]["id"] == review
                && payload["review"]["scopeId"] == native
                && payload["bindingIds"] == json!([binding]),
            "Status presentation escaped originating binding: {payload}"
        );
    }
    ensure!(
        !b.events
            .iter()
            .any(|event| event["event"] == "status" && event["session_id"] == session),
        "foreign Account received private Status"
    );
    let native_messages = manager.get_messages(&session).await?;
    let native_stats = serde_json::to_value(manager.get_session_stats(&session).await?)?;
    ensure!(
        native_messages["messages"] == json!([])
            && native_stats["totalMessages"] == 0
            && native_stats["toolCalls"] == 0
            && native_stats["toolResults"] == 0,
        "native Chat/model mutation"
    );
    for event in a
        .events
        .iter()
        .chain(b.events.iter())
        .chain(a_second.events.iter())
        .chain(reconnect.events.iter())
    {
        let kind = event["event"].as_str().unwrap_or("");
        ensure!(
            !kind.starts_with("stream.") && !kind.starts_with("tool.") && kind != "agent.working",
            "control created model/tool events: {event}"
        );
    }
    let ledger = oqto_history::oqto_log::store::read_session_stats(
        &home,
        &root.join("workdir").display().to_string(),
        &session,
    )
    .await?;
    ensure!(
        ledger.turns == 0 && ledger.messages == 0,
        "Chat ledger mutated"
    );
    let evidence = json!({"contract":contract,"binding":bound,"shared_binding":shared_binding,"remote_binding":remote_binding,"member_binding":member_binding,"membership_revoked":revoked,"native_id_denial":denied_native,"second_binding":second,"other_binding":b_bound,"create":created,"save":saved,"stale_cas":stale,"submit":submitted,"terminal":terminal,"foreign_review_denial":foreign_review,"reconnect":rebound,"reload":reloaded,"account_a_events":a.events,"account_b_events":b.events,"second_connection_events":a_second.events,"reconnect_events":reconnect.events,"native_messages":native_messages,"native_stats":native_stats,"ledger":{"turns":ledger.turns,"messages":ledger.messages}});
    std::fs::write(
        root.join("evidence.json"),
        serde_json::to_vec_pretty(&evidence)?,
    )?;
    let mut ready_contract = contract.clone();
    ready_contract["status"] = json!("ready");
    ready_contract["stop_file"] = json!(root.join("stop"));
    std::fs::write(
        root.join("contract.json"),
        serde_json::to_vec_pretty(&ready_contract)?,
    )?;
    std::fs::write(
        root.join("ready"),
        b"authenticated backend acceptance passed; browser pending\n",
    )?;
    println!(
        "Authenticated gateway ready at {url}; private contract {}",
        root.join("contract.json").display()
    );
    if let Ok(seconds) = std::env::var("OQTO_SELECTION_GATEWAY_HOLD_SECS") {
        let seconds: u64 = seconds.parse()?;
        ensure!(seconds <= 7200, "hold must be bounded to at most two hours");
        let deadline = tokio::time::Instant::now() + Duration::from_secs(seconds);
        while tokio::time::Instant::now() < deadline && !root.join("stop").exists() {
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
    }
    std::fs::remove_file(root.join("ready"))?;
    reconnect.socket.close(None).await?;
    a_second.socket.close(None).await?;
    b.socket.close(None).await?;
    manager.shutdown().await;
    server_task.abort();
    runner_task.abort();
    tls_task.abort();
    std::fs::write(
        root.join("stopped"),
        b"Pi manager shut down; listener tasks aborted\n",
    )?;
    Ok(())
}
