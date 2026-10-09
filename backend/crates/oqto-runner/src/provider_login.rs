//! Private provider-auth worker supervision. No credentials are logged or returned.
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn private_wire_debug_is_redacted_and_unknown_fields_rejected() {
        let request = ProviderLoginRequest {
            account_id: "alice".into(),
            operation: ProviderLoginOperation::Answer {
                attempt: "a".into(),
                prompt: "p".into(),
                value: "private-answer".into(),
            },
        };
        assert!(!format!("{request:?}").contains("private-answer"));
        assert!(
            !format!(
                "{:?}",
                ProviderLoginResponse {
                    data: serde_json::json!({"url":"private-url"})
                }
            )
            .contains("private-url")
        );
        assert!(
            serde_json::from_value::<ProviderLoginOperation>(
                serde_json::json!({"command":"providers", "authPath":"/other"})
            )
            .is_err()
        );
    }

    #[test]
    fn catalog_and_attempt_responses_round_trip_through_tagged_runner_wire() {
        for data in [
            serde_json::json!([]),
            serde_json::json!({"state":"running"}),
        ] {
            let response = crate::protocol::RunnerResponse::ProviderLogin(ProviderLoginResponse {
                data: data.clone(),
            });
            let wire = serde_json::to_value(response).unwrap();
            assert_eq!(wire["type"], "provider_login");
            assert_eq!(wire["data"], data);
            let response: crate::protocol::RunnerResponse = serde_json::from_value(wire).unwrap();
            assert!(matches!(
                response,
                crate::protocol::RunnerResponse::ProviderLogin(_)
            ));
        }
    }

    #[tokio::test]
    async fn missing_grant_and_wrong_account_fail_before_worker_spawn() {
        let manager = ProviderLoginManager::default();
        let request = ProviderLoginRequest {
            account_id: "bob".into(),
            operation: ProviderLoginOperation::Providers {},
        };
        assert!(
            manager
                .request(None, None, "/missing/pi", "mac", request.clone())
                .await
                .is_err()
        );
        let config = ProviderLoginConfig {
            account_id: "alice".into(),
            node: "/missing/node".into(),
            worker: "/missing/worker".into(),
        };
        assert_eq!(
            manager
                .request(Some(&config), None, "/missing/pi", "mac", request)
                .await
                .unwrap_err()
                .to_string(),
            "provider login denied"
        );
        assert!(manager.worker.lock().await.is_none());
    }

    #[tokio::test]
    async fn watchdog_kills_expired_worker_but_not_a_different_generation() {
        let mut child = Command::new("/bin/cat")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .unwrap();
        let generation = uuid::Uuid::new_v4();
        let worker = Worker {
            generation,
            deadline: Some(std::time::Instant::now()),
            input: child.stdin.take().unwrap(),
            output: BufReader::new(child.stdout.take().unwrap()),
            _egress: oqto_sandbox::EgressGuard::inert(),
            child,
        };
        let manager = ProviderLoginManager::default();
        *manager.worker.lock().await = Some(worker);
        manager.watchdog(uuid::Uuid::new_v4());
        tokio::time::sleep(Duration::from_millis(1100)).await;
        assert!(manager.worker.lock().await.is_some());
        manager.watchdog(generation);
        tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                if manager.worker.lock().await.is_none() {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .unwrap();
    }
}

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{path::PathBuf, process::Stdio, time::Duration};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::{Child, ChildStdin, ChildStdout, Command},
    sync::Mutex,
};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderLoginConfig {
    pub account_id: String,
    pub node: PathBuf,
    pub worker: PathBuf,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(tag = "command", rename_all = "snake_case", deny_unknown_fields)]
pub enum ProviderLoginOperation {
    Providers {},
    Start {
        provider: String,
        method: String,
    },
    Status {
        attempt: String,
    },
    Answer {
        attempt: String,
        prompt: String,
        value: String,
    },
    Cancel {
        attempt: String,
    },
    Commit {
        attempt: String,
        nonce: String,
    },
}
impl std::fmt::Debug for ProviderLoginOperation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ProviderLoginOperation([private])")
    }
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderLoginRequest {
    pub account_id: String,
    pub operation: ProviderLoginOperation,
}

#[derive(Clone, Deserialize, Serialize)]
pub struct ProviderLoginResponse {
    pub data: serde_json::Value,
}
impl std::fmt::Debug for ProviderLoginResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ProviderLoginResponse([private])")
    }
}

struct Worker {
    generation: uuid::Uuid,
    deadline: Option<std::time::Instant>,
    child: Child,
    input: ChildStdin,
    output: BufReader<ChildStdout>,
    _egress: oqto_sandbox::EgressGuard,
}
#[derive(Default)]
pub struct ProviderLoginManager {
    worker: std::sync::Arc<Mutex<Option<Worker>>>,
}
impl ProviderLoginManager {
    pub async fn request(
        &self,
        config: Option<&ProviderLoginConfig>,
        sandbox: Option<&oqto_sandbox::SandboxConfig>,
        pi: &str,
        machine: &str,
        request: ProviderLoginRequest,
    ) -> Result<serde_json::Value> {
        let config = config.context("provider login is disabled")?;
        ensure!(
            request.account_id == config.account_id,
            "provider login denied"
        );
        ensure!(
            config.node.is_absolute() && config.worker.is_absolute(),
            "provider login requires managed absolute paths"
        );
        let mut slot = self.worker.lock().await;
        // Never silently restart an in-flight attempt against a different process.
        if let Some(worker) = slot.as_mut()
            && worker.child.try_wait()?.is_some()
        {
            *slot = None;
        }
        if slot.is_none() {
            ensure!(
                matches!(
                    request.operation,
                    ProviderLoginOperation::Providers {} | ProviderLoginOperation::Start { .. }
                ),
                "provider login attempt unavailable"
            );
            let sandbox = sandbox
                .filter(|config| config.enabled)
                .context("provider login requires sandbox isolation")?;
            let worker = Self::spawn(config, sandbox, pi, machine).await?;
            self.watchdog(worker.generation);
            *slot = Some(worker);
        }
        let worker = slot.as_mut().context("provider login unavailable")?;
        let is_start = matches!(request.operation, ProviderLoginOperation::Start { .. });
        let exchange = async {
            let id = uuid::Uuid::new_v4().to_string();
            let mut frame = serde_json::to_value(request.operation)?;
            frame["id"] = id.clone().into();
            let mut bytes = serde_json::to_vec(&frame)?;
            ensure!(bytes.len() < 16_000, "provider login input too large");
            bytes.push(b'\n');
            worker.input.write_all(&bytes).await?;
            worker.input.flush().await?;
            let mut response = Vec::new();
            loop {
                let available = worker.output.fill_buf().await?;
                ensure!(!available.is_empty(), "provider login worker closed");
                let count = available
                    .iter()
                    .position(|b| *b == b'\n')
                    .map_or(available.len(), |n| n + 1);
                ensure!(
                    response.len() + count <= 1_048_576,
                    "provider login response too large"
                );
                let done = available[count - 1] == b'\n';
                response.extend_from_slice(&available[..count]);
                worker.output.consume(count);
                if done {
                    break;
                }
            }
            let value: serde_json::Value = serde_json::from_slice(&response)?;
            ensure!(
                value["id"].as_str() == Some(id.as_str()),
                "provider login response mismatch"
            );
            ensure!(value["ok"].is_boolean(), "provider login response invalid");
            Ok::<_, anyhow::Error>(value)
        };
        match tokio::time::timeout(Duration::from_secs(10), exchange).await {
            Ok(Ok(value)) => {
                ensure!(
                    value["ok"].as_bool() == Some(true),
                    "provider login operation rejected"
                );
                let data = &value["data"];
                if is_start {
                    worker.deadline = Some(std::time::Instant::now() + Duration::from_secs(610));
                }
                match data["state"].as_str() {
                    Some("cancelling") => {
                        let deadline = std::time::Instant::now() + Duration::from_secs(5);
                        worker.deadline =
                            Some(worker.deadline.map_or(deadline, |old| old.min(deadline)));
                    }
                    Some(
                        "saved" | "saved_refresh_required" | "failed" | "cancelled" | "expired",
                    ) => worker.deadline = None,
                    _ => {}
                }
                Ok(data.clone())
            }
            result => {
                // Errors may leave framing uncertain. Killing also closes callback listeners.
                if let Some(mut worker) = slot.take() {
                    let _ = worker.child.kill().await;
                }
                match result {
                    Ok(Err(error)) => Err(error),
                    _ => anyhow::bail!("provider login worker timed out"),
                }
            }
        }
    }
    fn watchdog(&self, generation: uuid::Uuid) {
        let weak = std::sync::Arc::downgrade(&self.worker);
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(1)).await;
                let Some(store) = weak.upgrade() else { return };
                let mut slot = store.lock().await;
                let Some(worker) = slot.as_ref() else { return };
                if worker.generation != generation {
                    return;
                }
                if worker
                    .deadline
                    .is_some_and(|deadline| std::time::Instant::now() >= deadline)
                {
                    if let Some(mut worker) = slot.take() {
                        let _ = worker.child.kill().await;
                    }
                    return;
                }
            }
        });
    }

    async fn spawn(
        config: &ProviderLoginConfig,
        sandbox: &oqto_sandbox::SandboxConfig,
        pi: &str,
        machine: &str,
    ) -> Result<Worker> {
        let binary = tokio::fs::canonicalize(pi)
            .await
            .context("managed Pi path unavailable")?;
        let mut sdk = None;
        for parent in binary.ancestors().skip(1).take(4) {
            let package = parent.join("package.json");
            let Ok(bytes) = tokio::fs::read(package).await else {
                continue;
            };
            let package: serde_json::Value = serde_json::from_slice(&bytes)?;
            if matches!(
                package["name"].as_str(),
                Some("@earendil-works/pi-coding-agent" | "@mariozechner/pi-coding-agent")
            ) {
                sdk = Some(parent.join("dist/index.js"));
                break;
            }
        }
        let sdk = sdk.context("managed Pi SDK unavailable; native login required")?;
        let agent_dir = match std::env::var_os("PI_CODING_AGENT_DIR") {
            Some(path) => PathBuf::from(path),
            None => PathBuf::from(std::env::var_os("HOME").context("runner HOME unavailable")?)
                .join(".pi/agent"),
        };
        ensure!(
            agent_dir.is_absolute(),
            "managed agent directory must be absolute"
        );
        let args = vec![
            config.worker.to_string_lossy().into_owned(),
            "--sdk".into(),
            sdk.to_string_lossy().into_owned(),
            "--agent-dir".into(),
            agent_dir.to_string_lossy().into_owned(),
            "--machine".into(),
            machine.into(),
            "--account".into(),
            config.account_id.clone(),
        ];
        let mut policy = sandbox.clone();
        if let Some(socket) = crate::ssh_agent_proxy::upstream_agent_socket() {
            policy.deny_read.push(socket.to_string_lossy().into_owned());
        }
        let command = oqto_sandbox::build_sandbox_command(
            &policy,
            &agent_dir,
            &config.node,
            &args,
            oqto_sandbox::SandboxStdin::Redirected,
        )?;
        let egress = policy.prepare_egress()?;
        let mut command = Command::from(command);
        oqto_sandbox::configure_bwrap_pre_exec(
            command.as_std_mut(),
            &policy,
            &agent_dir,
            egress.plan(),
        )?;
        let mut child = command
            .env_remove("SSH_AUTH_SOCK")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .context("starting private provider login worker")?;
        let input = child
            .stdin
            .take()
            .context("provider login input unavailable")?;
        let output = BufReader::new(
            child
                .stdout
                .take()
                .context("provider login output unavailable")?,
        );
        Ok(Worker {
            generation: uuid::Uuid::new_v4(),
            deadline: None,
            child,
            input,
            output,
            _egress: egress,
        })
    }
}
