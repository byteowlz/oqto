//! Synthetic real-HTTP proof; no real Accounts, credentials, Studio or ACL changes.
use anyhow::{Context, Result, ensure};
use axum::{Router, http::HeaderMap, routing::get};
use chrono::Utc;
use oqto::{
    auth::{
        AuthConfig, AuthState,
        service_access::{
            AccessScope, GrantRequest, PilotConfig, PilotRepository, PilotState, pilot_router,
        },
    },
    db::Database,
    shared_workspace::SharedWorkspaceRepository,
    user::{CreateUserRequest, UserRepository},
};
use reqwest::{Client, StatusCode};
use serde_json::{Value, json};
use std::{
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::Duration,
};
use tempfile::TempDir;
use tokio::{
    net::{TcpListener, UnixListener},
    task::JoinHandle,
};

const AUDIENCE: &str = "https://design-studio.example.test";
const EDGE_HEADER: &str = "x-oqto-service-authorization";
const NATIVE_TOKEN: &str = "0123456789012345678901234567890123456789012";

fn native_syntax(token: &str) -> bool {
    token.len() == 43
        && token
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

struct Fixture {
    root: TempDir,
    pool: sqlx::SqlitePool,
    state: PilotState,
    auth: AuthState,
    token: String,
    outsider: String,
    base: String,
    admin: String,
    tasks: Vec<JoinHandle<()>>,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        for task in &self.tasks {
            task.abort();
        }
    }
}

async fn serve(router: Router) -> Result<(String, JoinHandle<()>)> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let base = format!("http://{}", listener.local_addr()?);
    let task = tokio::spawn(async move {
        axum::serve(listener, router)
            .await
            .expect("fixture HTTP server");
    });
    Ok((base, task))
}
impl Fixture {
    async fn new() -> Result<Self> {
        let root = tempfile::tempdir()?;
        let db = Database::new(&root.path().join("pilot.sqlite")).await?;
        let pool = db.pool().clone();
        let users = UserRepository::new(pool.clone());
        for id in ["fixture-owner", "fixture-outsider"] {
            users
                .create_with_id(
                    id,
                    CreateUserRequest {
                        username: id.into(),
                        email: format!("{id}@example.test"),
                        password: None,
                        display_name: None,
                        role: None,
                        external_id: None,
                    },
                )
                .await?;
        }
        let workspaces = SharedWorkspaceRepository::new(pool.clone());
        workspaces
            .create(
                "fixture-workspace",
                "Fixture",
                "fixture",
                "fixture",
                "/synthetic",
                "fixture-owner",
                None,
                "folder",
                "blue",
            )
            .await?;
        // This is the same membership authority used by the product, not pilot-specific roles.
        sqlx::query("INSERT INTO shared_workspace_members (shared_workspace_id, user_id, role) VALUES ('fixture-workspace', 'fixture-owner', 'owner')").execute(&pool).await?;
        let auth = AuthState::new(AuthConfig {
            jwt_secret: Some("synthetic-test-secret-not-a-production-credential-32".into()),
            ..Default::default()
        });
        let token = auth.generate_token("fixture-owner", "owner@example.test", "Owner", "user")?;
        let outsider = auth.generate_token(
            "fixture-outsider",
            "outsider@example.test",
            "Outsider",
            "admin",
        )?;
        let state = PilotState::new(
            PilotRepository::new(pool.clone()),
            auth.clone(),
            PilotConfig {
                audience: AUDIENCE.into(),
                workspace_id: "fixture-workspace".into(),
            },
        )?;
        let (base, public_task) = serve(pilot_router(state.clone(), false)).await?;
        let (admin, admin_task) = serve(pilot_router(state.clone(), true)).await?;
        // Trusted-channel injection fixture ONLY: production uses the protected Unix admin socket.
        Ok(Self {
            root,
            pool,
            state,
            auth,
            token,
            outsider,
            base,
            admin,
            tasks: vec![public_task, admin_task],
        })
    }
    fn scope(&self) -> AccessScope {
        AccessScope {
            service_id: "design-studio".into(),
            workspace_id: "fixture-workspace".into(),
            resource_id: "catalog".into(),
            audience: AUDIENCE.into(),
        }
    }
    fn grant(&self, id: &str, expires_at: i64) -> GrantRequest {
        GrantRequest {
            id: id.into(),
            workload_id: "design-review".into(),
            scope: self.scope(),
            expires_at,
            owner_account_id: "fixture-owner".into(),
        }
    }
    async fn issue(&self, client: &Client, id: &str) -> Result<String> {
        let response = client
            .post(format!("{}/service-access/v1/grants", self.admin))
            .json(&self.grant(id, Utc::now().timestamp() + 300))
            .send()
            .await?;
        ensure!(
            response.status() == StatusCode::OK,
            "fixture issuance failed"
        );
        let value: Value = response.json().await?;
        Ok(value["credential"]
            .as_str()
            .context("issued credential")?
            .into())
    }
    async fn check(
        &self,
        client: &Client,
        token: &str,
        scope: &AccessScope,
    ) -> Result<reqwest::Response> {
        Ok(client
            .get(format!("{}/service-access/v1/check", self.base))
            .query(scope)
            .header(EDGE_HEADER, format!("Bearer {token}"))
            .send()
            .await?)
    }
}

#[tokio::test]
async fn service_access_http_authority_lifecycle() -> Result<()> {
    let fixture = Fixture::new().await?;
    let client = Client::builder().timeout(Duration::from_secs(5)).build()?;
    assert_eq!(
        fixture
            .check(&client, &fixture.token, &fixture.scope())
            .await?
            .status(),
        StatusCode::OK
    );
    assert_eq!(
        fixture
            .check(&client, &fixture.outsider, &fixture.scope())
            .await?
            .status(),
        StatusCode::FORBIDDEN
    );
    let issue_url = format!("{}/service-access/v1/grants", fixture.base);
    // Even a verified coarse-admin JWT cannot mutate via the public router.
    assert_eq!(
        client
            .post(&issue_url)
            .header(EDGE_HEADER, format!("Bearer {}", fixture.outsider))
            .json(&fixture.grant("remote", Utc::now().timestamp() + 60))
            .send()
            .await?
            .status(),
        StatusCode::FORBIDDEN
    );
    for expires_at in [Utc::now().timestamp() - 1, Utc::now().timestamp() + 4000] {
        let response = client
            .post(format!("{}/service-access/v1/grants", fixture.admin))
            .json(&fixture.grant("invalid-expiry", expires_at))
            .send()
            .await?;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert_eq!(response.json::<Value>().await?["code"], "invalid_grant");
    }
    let invalid_scope = client
        .get(format!("{}/service-access/v1/check", fixture.base))
        .send()
        .await?;
    assert_eq!(invalid_scope.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        invalid_scope.json::<Value>().await?["code"],
        "invalid_scope"
    );
    let unsupported = client.patch(&issue_url).send().await?;
    assert_eq!(unsupported.status(), StatusCode::METHOD_NOT_ALLOWED);
    assert_eq!(
        unsupported.json::<Value>().await?["code"],
        "operation_unsupported"
    );
    let workload = fixture.issue(&client, "fixture-grant").await?;
    assert_eq!(
        fixture
            .check(&client, &workload, &fixture.scope())
            .await?
            .status(),
        StatusCode::OK
    );
    // An unrelated workload on the same machine has no credential; asserted identity never helps.
    assert_eq!(
        client
            .get(format!("{}/service-access/v1/check", fixture.base))
            .query(&fixture.scope())
            .header(EDGE_HEADER, "Bearer oqto_sa_unrelated-workload")
            .header("X-Oqto-Workload", "design-review")
            .header("X-Workspace-ID", "fixture-workspace")
            .send()
            .await?
            .status(),
        StatusCode::FORBIDDEN
    );
    for scope in [
        AccessScope {
            service_id: "immich".into(),
            ..fixture.scope()
        },
        AccessScope {
            audience: "https://other.example.test".into(),
            ..fixture.scope()
        },
        AccessScope {
            workspace_id: "unrelated".into(),
            ..fixture.scope()
        },
        AccessScope {
            resource_id: "feedback".into(),
            ..fixture.scope()
        },
    ] {
        assert_eq!(
            fixture.check(&client, &workload, &scope).await?.status(),
            StatusCode::FORBIDDEN
        );
    }
    for kind in ["expired", "future", "audience", "signature"] {
        let mut claims = fixture.auth.validate_token(&fixture.token)?;
        let now = Utc::now().timestamp();
        if kind == "expired" {
            claims.exp = now - 1;
        }
        if kind == "future" {
            claims.nbf = Some(now + 300);
        }
        if kind == "audience" {
            claims.aud = Some(vec!["https://wrong.example.test".into()]);
        }
        let secret = if kind == "signature" {
            "wrong-signing-secret"
        } else {
            "synthetic-test-secret-not-a-production-credential-32"
        };
        let token = jsonwebtoken::encode(
            &jsonwebtoken::Header::default(),
            &claims,
            &jsonwebtoken::EncodingKey::from_secret(secret.as_bytes()),
        )?;
        assert_eq!(
            fixture
                .check(&client, &token, &fixture.scope())
                .await?
                .status(),
            StatusCode::FORBIDDEN,
            "{kind}"
        );
    }
    assert_eq!(
        fixture
            .check(&client, "forged.jwt.claims", &fixture.scope())
            .await?
            .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        fixture
            .check(&client, "dev:fixture-owner", &fixture.scope())
            .await?
            .status(),
        StatusCode::FORBIDDEN
    );
    for (token, expected) in [(&workload, 1), (&fixture.outsider, 0)] {
        let entries: Vec<Value> = client
            .get(format!("{}/service-access/v1/discovery", fixture.base))
            .query(&fixture.scope())
            .header(EDGE_HEADER, format!("Bearer {token}"))
            .send()
            .await?
            .json()
            .await?;
        assert_eq!(entries.len(), expected);
    }
    let inspected = client
        .get(format!("{}/service-access/v1/grants", fixture.base))
        .header(EDGE_HEADER, format!("Bearer {}", fixture.token))
        .send()
        .await?
        .text()
        .await?;
    assert!(!inspected.contains(&workload));
    // Singleton, wrong baseline and duplicate ID are refused, without issuing additional authority.
    assert_eq!(
        client
            .post(format!("{}/service-access/v1/grants", fixture.admin))
            .json(&fixture.grant("duplicate", Utc::now().timestamp() + 60))
            .send()
            .await?
            .status(),
        StatusCode::CONFLICT
    );
    let revoke = format!(
        "{}/service-access/v1/grants/fixture-grant/revoke",
        fixture.admin
    );
    for _ in 0..2 {
        assert_eq!(client.post(&revoke).send().await?.status(), StatusCode::OK);
    }
    let response = fixture.check(&client, &workload, &fixture.scope()).await?;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert_eq!(response.json::<Value>().await?["code"], "grant_revoked");
    let second = fixture.issue(&client, "fixture-expiry").await?;
    // Safe fixture-only clock advance in durable store; real authority always uses server time.
    sqlx::query("UPDATE service_access_grants SET expires_at = ? WHERE id = 'fixture-expiry'")
        .bind(Utc::now().timestamp() - 1)
        .execute(&fixture.pool)
        .await?;
    assert_eq!(
        fixture
            .check(&client, &second, &fixture.scope())
            .await?
            .json::<Value>()
            .await?["code"],
        "grant_expired"
    );
    let third = fixture.issue(&client, "fixture-membership").await?;
    sqlx::query(
        "UPDATE shared_workspaces SET owner_id = 'fixture-outsider' WHERE id = 'fixture-workspace'",
    )
    .execute(&fixture.pool)
    .await?;
    assert_eq!(
        fixture
            .check(&client, &third, &fixture.scope())
            .await?
            .json::<Value>()
            .await?["code"],
        "issuer_authority_lost"
    );
    sqlx::query(
        "UPDATE shared_workspaces SET owner_id = 'fixture-owner' WHERE id = 'fixture-workspace'",
    )
    .execute(&fixture.pool)
    .await?;
    sqlx::query("DELETE FROM shared_workspace_members WHERE user_id = 'fixture-owner'")
        .execute(&fixture.pool)
        .await?;
    assert_eq!(
        fixture
            .check(&client, &third, &fixture.scope())
            .await?
            .json::<Value>()
            .await?["code"],
        "issuer_authority_lost"
    );
    assert_eq!(
        fixture
            .check(&client, &fixture.token, &fixture.scope())
            .await?
            .status(),
        StatusCode::FORBIDDEN
    );
    let audit: Vec<String> = sqlx::query_scalar("SELECT reason FROM service_access_audit")
        .fetch_all(&fixture.pool)
        .await?;
    assert!(audit.iter().any(|r| r == "credential_invalid"));
    assert!(audit.iter().all(|r| !r.contains(&workload)));
    // Restart/reconnect uses the same durable authority; no cached credential resurrection.
    let reopened = Database::new(&fixture.root.path().join("pilot.sqlite")).await?;
    let restored = PilotState::new(
        PilotRepository::new(reopened.pool().clone()),
        fixture.auth.clone(),
        PilotConfig {
            audience: AUDIENCE.into(),
            workspace_id: "fixture-workspace".into(),
        },
    )?;
    let (restarted, task) = serve(pilot_router(restored, false)).await?;
    assert_eq!(
        client
            .get(format!("{restarted}/service-access/v1/check"))
            .query(&fixture.scope())
            .header(EDGE_HEADER, format!("Bearer {workload}"))
            .send()
            .await?
            .status(),
        StatusCode::FORBIDDEN
    );
    task.abort();
    fixture.pool.close().await;
    assert_eq!(
        fixture
            .check(&client, &workload, &fixture.scope())
            .await?
            .status(),
        StatusCode::SERVICE_UNAVAILABLE
    );
    Ok(())
}

#[tokio::test]
async fn service_access_real_oqto_router_preserves_auth_boundaries() -> Result<()> {
    use oqto::{
        api::{AppState, create_admin_router_with_config, create_router_with_config},
        auth::CurrentUser,
        session::{SessionRepository, SessionService, SessionServiceConfig},
        user::UserService,
    };
    let mut fixture = Fixture::new().await?;
    let local = oqto::local::LocalRuntimeConfig {
        workspace_dir: fixture.root.path().display().to_string(),
        single_user: true,
        ..Default::default()
    };
    let sessions = SessionService::with_runner(
        SessionRepository::new(fixture.pool.clone()),
        oqto_runner::client::RunnerClient::new(
            fixture.root.path().join("never-connected-runner.sock"),
        ),
        oqto::local::LocalRuntime::new(local.clone()),
        SessionServiceConfig {
            local_config: Some(local),
            single_user: true,
            ..Default::default()
        },
    );
    let mut app_state = AppState::new(
        sessions,
        UserService::new(UserRepository::new(fixture.pool.clone())),
        oqto::invite::InviteCodeRepository::new(fixture.pool.clone()),
        oqto::api_keys::ApiKeyRepository::new(fixture.pool.clone()),
        fixture.auth.clone(),
        Default::default(),
        Default::default(),
        Default::default(),
        oqto::session_target::SessionTargetRepository::new(fixture.pool.clone()),
        1024,
    );
    let client = Client::new();
    let (disabled, task) = serve(create_router_with_config(app_state.clone(), 1)).await?;
    fixture.tasks.push(task);
    assert_eq!(
        client
            .get(format!("{disabled}/service-access/v1/grants"))
            .send()
            .await?
            .status(),
        StatusCode::NOT_FOUND
    );
    app_state.service_access_pilot = Some(fixture.state.clone());
    let (public, task) = serve(create_router_with_config(app_state.clone(), 1)).await?;
    fixture.tasks.push(task);
    let (admin, task) = serve(create_admin_router_with_config(
        app_state,
        1,
        CurrentUser {
            claims: fixture.auth.validate_token(&fixture.outsider)?,
        },
    ))
    .await?;
    fixture.tasks.push(task);
    let request = fixture.grant("real-router-grant", Utc::now().timestamp() + 300);
    assert_eq!(
        client
            .post(format!("{public}/service-access/v1/grants"))
            .header("Authorization", format!("Bearer {}", fixture.outsider))
            .header(EDGE_HEADER, format!("Bearer {}", fixture.outsider))
            .json(&request)
            .send()
            .await?
            .status(),
        StatusCode::FORBIDDEN
    );
    let response = client
        .post(format!("{admin}/service-access/v1/grants"))
        .json(&request)
        .send()
        .await?
        .error_for_status()?;
    let value: Value = response.json().await?;
    let credential = value["credential"]
        .as_str()
        .context("real router credential")?;
    assert_eq!(
        client
            .get(format!("{public}/service-access/v1/check"))
            .query(&fixture.scope())
            .header(EDGE_HEADER, format!("Bearer {}", fixture.token))
            .send()
            .await?
            .status(),
        StatusCode::OK
    );
    assert_eq!(
        client
            .get(format!("{public}/service-access/v1/check"))
            .query(&fixture.scope())
            .header(EDGE_HEADER, format!("Bearer {credential}"))
            .send()
            .await?
            .status(),
        StatusCode::OK
    );
    // Opaque pilot credentials cannot enter general Account auth/other API routes.
    assert_eq!(
        client
            .post(format!("{public}/auth/change-password"))
            .header("Authorization", format!("Bearer {credential}"))
            .json(&json!({}))
            .send()
            .await?
            .status(),
        StatusCode::UNAUTHORIZED
    );
    Ok(())
}

#[tokio::test]
async fn service_access_concurrent_issuance_is_singleton() -> Result<()> {
    let fixture = Fixture::new().await?;
    let client = Client::new();
    let url = format!("{}/service-access/v1/grants", fixture.admin);
    let left = client
        .post(&url)
        .json(&fixture.grant("concurrent-left", Utc::now().timestamp() + 300))
        .send();
    let right = client
        .post(&url)
        .json(&fixture.grant("concurrent-right", Utc::now().timestamp() + 300))
        .send();
    let (left, right) = tokio::join!(left, right);
    let mut statuses = [left?.status().as_u16(), right?.status().as_u16()];
    statuses.sort();
    assert_eq!(statuses, [200, 409]);
    let inspected: Vec<Value> = client.get(&url).send().await?.json().await?;
    assert_eq!(inspected.len(), 1);
    Ok(())
}

#[tokio::test]
async fn service_access_cli_uses_same_authority() -> Result<()> {
    use tokio::io::AsyncWriteExt;
    let mut fixture = Fixture::new().await?;
    let client = Client::new();
    let socket = fixture.root.path().join("admin.sock");
    let listener = UnixListener::bind(&socket)?;
    let app = Router::new().nest("/api", pilot_router(fixture.state.clone(), true));
    fixture.tasks.push(tokio::spawn(async move {
        axum::serve(listener, app)
            .await
            .expect("fixture CLI admin server");
    }));
    let script =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../scripts/dev/service-access-pilot.sh");
    let invoke = |args: &[&str]| {
        let mut cmd = tokio::process::Command::new("bash");
        cmd.arg(&script).arg(&socket).args(args);
        cmd
    };
    let invalid = invoke(&["grant"]).output().await?;
    assert_eq!(invalid.status.code(), Some(2));
    assert_eq!(
        serde_json::from_slice::<Value>(&invalid.stdout)?["code"],
        "cli_usage"
    );
    assert!(
        client
            .get(format!("{}/service-access/v1/grants", fixture.admin))
            .send()
            .await?
            .json::<Vec<Value>>()
            .await?
            .is_empty()
    );
    let mut command = invoke(&["grant", "--apply"]);
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()?;
    child
        .stdin
        .take()
        .context("CLI stdin")?
        .write_all(&serde_json::to_vec(
            &fixture.grant("cli-grant", Utc::now().timestamp() + 300),
        )?)
        .await?;
    let output = child.wait_with_output().await?;
    ensure!(output.status.success(), "CLI grant failed");
    let credential: String = serde_json::from_slice::<Value>(&output.stdout)?["credential"]
        .as_str()
        .context("CLI credential")?
        .into();
    assert_eq!(
        fixture
            .check(&client, &credential, &fixture.scope())
            .await?
            .status(),
        StatusCode::OK
    );
    let inspected = invoke(&["inspect"]).output().await?;
    assert!(inspected.status.success());
    let cli: Value = serde_json::from_slice(&inspected.stdout)?;
    let human: Value = client
        .get(format!("{}/service-access/v1/grants", fixture.base))
        .header(EDGE_HEADER, format!("Bearer {}", fixture.token))
        .send()
        .await?
        .json()
        .await?;
    assert_eq!(cli, human);
    assert!(!String::from_utf8(inspected.stdout)?.contains(&credential));
    let revoked = invoke(&["revoke", "cli-grant", "--apply"]).output().await?;
    assert!(revoked.status.success());
    assert_eq!(
        fixture
            .check(&client, &credential, &fixture.scope())
            .await?
            .json::<Value>()
            .await?["code"],
        "grant_revoked"
    );
    Ok(())
}

#[test]
fn service_access_config_fails_closed() {
    for audience in [
        "http://example.test",
        "https://example.test/path",
        "https://u:p@example.test",
        "https://example.test/?x=1",
        "https://example.test/#x",
        "https://example.test/",
    ] {
        assert!(
            PilotConfig {
                audience: audience.into(),
                workspace_id: "fixture-workspace".into()
            }
            .validate()
            .is_err()
        );
    }
    assert!(serde_json::from_value::<PilotConfig>(json!({ "audience": AUDIENCE, "workspace_id": "fixture-workspace", "upstream": "http://malicious" })).is_err());
}

struct Caddy(Child);
impl Drop for Caddy {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
fn example_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../docs/examples/service-access-pilot.Caddyfile")
}

/// Inspect real runtime errors, never print captured logs/credentials on failure.
async fn assert_redacted_runtime_errors(
    path: &Path,
    minimum: usize,
    secrets: &[&str],
) -> Result<usize> {
    for _ in 0..100 {
        let logs = std::fs::read_to_string(path)?;
        let errors: Vec<Value> = logs
            .lines()
            .filter_map(|line| serde_json::from_str::<Value>(line).ok())
            .filter(|record| {
                record["level"] == "error"
                    && record["logger"] == "http.log.error"
                    && record["status"] == 502
            })
            .collect();
        if errors.len() >= minimum {
            for secret in secrets {
                ensure!(!secret.is_empty(), "empty synthetic log marker");
                ensure!(
                    !logs.contains(secret),
                    "runtime log leaked a synthetic credential/query marker"
                );
            }
            for error in &errors {
                ensure!(
                    error["msg"].as_str().is_some_and(|msg| !msg.is_empty()),
                    "runtime error lacked a cause"
                );
                ensure!(
                    error["request"].get("headers").is_none()
                        && error["request"].get("uri").is_none(),
                    "runtime request fields were not redacted"
                );
            }
            return Ok(errors.len());
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    anyhow::bail!("expected real HTTP 502 runtime errors were not emitted")
}

/// Requires `caddy` on PATH. Explicitly opt-in: cargo test --test service_access_pilot -- --ignored.
#[tokio::test]
#[ignore = "requires real Caddy executable; synthetic local listeners only"]
async fn service_access_caddy_real_http() -> Result<()> {
    let mut fixture = Fixture::new().await?;
    let client = Client::builder().timeout(Duration::from_secs(5)).build()?;
    // Graceful fixture teardown closes pooled idle connections as well as listeners.
    let auth_listener = TcpListener::bind("127.0.0.1:0").await?;
    let auth_address = auth_listener.local_addr()?;
    let (auth_stop, auth_stopped) = tokio::sync::oneshot::channel::<()>();
    let auth_app = pilot_router(fixture.state.clone(), false);
    let auth_task = fixture.tasks.len();
    fixture.tasks.push(tokio::spawn(async move {
        axum::serve(auth_listener, auth_app)
            .with_graceful_shutdown(async {
                let _ = auth_stopped.await;
            })
            .await
            .expect("fixture Caddy auth server");
    }));
    let socket = fixture.root.path().join("studio.sock");
    let listener = UnixListener::bind(&socket)?;
    let (studio_stop, studio_stopped) = tokio::sync::oneshot::channel::<()>();
    let upstream = Router::new().route(
        "/{*path}",
        get(|headers: HeaderMap| async move {
            assert!(!headers.contains_key(EDGE_HEADER));
            assert!(!headers.contains_key("x-oqto-workload"));
            // Native Studio feedback token must survive unchanged, but never gets logged.
            if let Some(value) = headers.get("authorization") {
                assert_eq!(
                    value.to_str().ok(),
                    Some(format!("Bearer {NATIVE_TOKEN}").as_str())
                );
            }
            assert!(!headers.contains_key("cookie"));
            assert!(!headers.contains_key("x-api-key"));
            // Prove presence/omission without reflecting any credential in the response.
            let native_auth = if headers.contains_key("authorization") {
                "present"
            } else {
                "absent"
            };
            ([("X-Fixture-Native-Auth", native_auth)], "synthetic studio")
        }),
    );
    let studio_task = fixture.tasks.len();
    fixture.tasks.push(tokio::spawn(async move {
        axum::serve(listener, upstream)
            .with_graceful_shutdown(async {
                let _ = studio_stopped.await;
            })
            .await
            .expect("fixture Unix HTTP upstream");
    }));
    let reserved = TcpListener::bind("127.0.0.1:0").await?;
    let port = reserved.local_addr()?.port();
    drop(reserved);
    let log_path = fixture.root.path().join("caddy.log");
    let log = std::fs::File::create(&log_path)?;
    let mut caddy = Caddy(
        Command::new("caddy")
            .args(["run", "--config"])
            .arg(example_path())
            .args(["--adapter", "caddyfile"])
            .env("PILOT_GATEWAY_PORT", port.to_string())
            .env("PILOT_AUTH_UPSTREAM", auth_address.to_string())
            .env("PILOT_STUDIO_SOCKET", &socket)
            .stdout(Stdio::null())
            .stderr(log)
            .spawn()
            .context("run real Caddy")?,
    );
    let url = format!("http://127.0.0.1:{port}/api/feedback");
    for attempt in 0..100 {
        if client.get(&url).send().await.is_ok() {
            break;
        }
        ensure!(
            caddy.0.try_wait()?.is_none(),
            "Caddy exited; inspect fixture log"
        );
        ensure!(attempt < 99, "Caddy did not start");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let request = |host: &str, token: &str| {
        client
            .get(&url)
            .header("Host", host)
            .header(EDGE_HEADER, format!("Bearer {token}"))
            .header("Authorization", format!("Bearer {NATIVE_TOKEN}"))
            .header(
                "Cookie",
                "studio_login=synthetic-native-cookie; auth_token=synthetic-oqto-cookie",
            )
            .header("X-API-Key", "octo_sk_synthetic")
            .header("X-Oqto-Workload", "design-review")
            .header("X-Dev-User", "fixture-owner")
            .header("X-Forwarded-For", "100.64.0.1")
    };
    let workload = fixture.issue(&client, "gateway-grant").await?;
    // All current Oqto issuance formats are disjoint from native Studio syntax.
    let (api_key, _) = oqto::api_keys::generate_api_key();
    assert!(native_syntax(NATIVE_TOKEN));
    for token in [&fixture.token, &api_key, &workload] {
        assert!(!native_syntax(token));
        let mut alternate = request("design-studio.example.test", &workload).build()?;
        alternate
            .headers_mut()
            .insert("Authorization", format!("Bearer {token}").parse()?);
        let response = client.execute(alternate).await?;
        assert_eq!(response.status(), StatusCode::OK); // Credential omitted upstream.
        assert_eq!(
            response
                .headers()
                .get("x-fixture-native-auth")
                .and_then(|h| h.to_str().ok()),
            Some("absent")
        );
    }
    assert_eq!(
        request("design-studio.example.test", &workload)
            .header("Authorization", format!("Bearer {}", fixture.token))
            .send()
            .await?
            .status(),
        StatusCode::FORBIDDEN
    ); // Mixed native/Oqto headers.
    // A native-looking candidate is NOT an edge workload credential.
    assert_eq!(
        fixture
            .check(&client, NATIVE_TOKEN, &fixture.scope())
            .await?
            .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        request("design-studio.example.test", NATIVE_TOKEN)
            .send()
            .await?
            .status(),
        StatusCode::FORBIDDEN
    );
    for token in [&fixture.token, &workload] {
        let response = request("design-studio.example.test", token).send().await?;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response
                .headers()
                .get("x-fixture-native-auth")
                .and_then(|h| h.to_str().ok()),
            Some("present")
        );
        assert_eq!(response.text().await?, "synthetic studio");
    }
    assert_eq!(
        request("immich.example.test", &workload)
            .send()
            .await?
            .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        request("unknown.example.test", &workload)
            .send()
            .await?
            .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        request("design-studio.example.test", "oqto_sa_unrelated")
            .send()
            .await?
            .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        request("design-studio.example.test", "forged.claims")
            .send()
            .await?
            .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        request("design-studio.example.test", &workload)
            .header("Accept", "text/event-stream")
            .send()
            .await?
            .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        client
            .post(&url)
            .header("Host", "design-studio.example.test")
            .header(EDGE_HEADER, format!("Bearer {workload}"))
            .header("X-Forwarded-Method", "GET")
            .send()
            .await?
            .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        request("design-studio.example.test", &workload)
            .header("Connection", "Upgrade")
            .header("Upgrade", "websocket")
            .send()
            .await?
            .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        client
            .get(&url)
            .header("Host", "design-studio.example.test")
            .header("X-Dev-User", "fixture-owner")
            .header("X-Oqto-Workload", "design-review")
            .header("Cookie", format!("auth_token={}", fixture.token))
            .header("Authorization", format!("Bearer {NATIVE_TOKEN}"))
            .send()
            .await?
            .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        request("design-studio.example.test", &workload)
            .query(&[("token", "synthetic-url-credential")])
            .send()
            .await?
            .status(),
        StatusCode::BAD_REQUEST
    );
    // No TCP Studio upstream exists in this fixture: only the private Unix path is configured.
    // This proves the gateway fixture shape, NOT real host ACL or same-UID isolation.
    let direct_client = Client::builder().unix_socket(socket.clone()).build()?;
    // Same-UID administrative break-glass can deliberately reach the Unix upstream.
    // This is a tested LIMITATION, not a claim of per-agent mount/uid isolation.
    assert_eq!(
        direct_client
            .get("http://localhost/api/feedback")
            .header("Authorization", format!("Bearer {NATIVE_TOKEN}"))
            .send()
            .await?
            .status(),
        StatusCode::OK
    );
    client
        .post(format!(
            "{}/service-access/v1/grants/gateway-grant/revoke",
            fixture.admin
        ))
        .send()
        .await?
        .error_for_status()?;
    assert_eq!(
        request("design-studio.example.test", &workload)
            .send()
            .await?
            .status(),
        StatusCode::FORBIDDEN
    );
    // Studio outage: real auth still allows this human, but no upstream exists.
    studio_stop
        .send(())
        .map_err(|_| anyhow::anyhow!("fixture Studio already stopped"))?;
    tokio::time::timeout(Duration::from_secs(5), &mut fixture.tasks[studio_task]).await??;
    std::fs::remove_file(&socket)?;
    const QUERY_MARKER: &str = "synthetic-private-runtime-query-marker";
    const COOKIE_MARKER: &str = "synthetic-private-runtime-cookie-marker";
    let secret_markers = [
        &fixture.token[..],
        &workload[..],
        &api_key[..],
        NATIVE_TOKEN,
        "synthetic-native-cookie",
        "synthetic-oqto-cookie",
        QUERY_MARKER,
        COOKIE_MARKER,
    ];
    let failure_request = |edge: &str| {
        request("design-studio.example.test", edge)
            .query(&[("review", QUERY_MARKER)])
            .header("X-Regression-JWT", &fixture.token)
            .header("X-Regression-Workload", &workload)
            .header("X-Regression-API-Key", &api_key)
            .header("Cookie", format!("private_fixture={COOKIE_MARKER}"))
    };
    assert_eq!(
        failure_request(&fixture.token).send().await?.status(),
        StatusCode::BAD_GATEWAY
    );
    let studio_errors = assert_redacted_runtime_errors(&log_path, 1, &secret_markers).await?;
    // Simulate a future/imported workload credential colliding with native syntax.
    // The real issuance route cannot create this shape. Known collisions still deny.
    use sha2::{Digest, Sha256};
    sqlx::query("UPDATE service_access_grants SET credential_hash = ? WHERE id = 'gateway-grant'")
        .bind(hex::encode(Sha256::digest(NATIVE_TOKEN.as_bytes())))
        .execute(&fixture.pool)
        .await?;
    assert_eq!(
        request("design-studio.example.test", &fixture.token)
            .send()
            .await?
            .status(),
        StatusCode::FORBIDDEN
    );
    // Auth outage: close real listener/pooled connections, then emit genuine errors.
    auth_stop
        .send(())
        .map_err(|_| anyhow::anyhow!("fixture auth already stopped"))?;
    tokio::time::timeout(Duration::from_secs(5), &mut fixture.tasks[auth_task]).await??;
    for edge in [&fixture.token, &workload] {
        assert_eq!(
            failure_request(edge).send().await?.status(),
            StatusCode::BAD_GATEWAY
        );
    }
    let auth_errors =
        assert_redacted_runtime_errors(&log_path, studio_errors + 2, &secret_markers).await?;
    // Reject every credential query BEFORE auth, even when auth is unavailable.
    // Status and unchanged error count prove none reached the failing auth hop.
    for name in ["token", "api_key"] {
        assert_eq!(
            request("design-studio.example.test", &fixture.token)
                .query(&[(name, QUERY_MARKER)])
                .send()
                .await?
                .status(),
            StatusCode::BAD_REQUEST
        );
    }
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(
        assert_redacted_runtime_errors(&log_path, auth_errors, &secret_markers).await?,
        auth_errors
    );
    assert_eq!(
        request("immich.example.test", &fixture.token)
            .send()
            .await?
            .status(),
        StatusCode::FORBIDDEN
    );
    Ok(())
}
