use crate::{
    PlacementCapabilityReport, PlacementHealth, PlacementId, PlacementKind, PlacementRecord,
    PlacementSpec, PlacementSupervisor, ProbeEvidence, runtime_name,
};
use anyhow::{Context, Result};
use async_trait::async_trait;
use oqto_runner::client::RunnerClient;
use oqto_runner::transport::RunnerEndpointConfig;
use std::ffi::OsString;
use std::process::ExitStatus;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::process::Command;

pub struct CommandOutput {
    pub status: ExitStatus,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

#[async_trait]
pub trait CommandRunner: Send + Sync {
    async fn run(&self, program: &str, args: &[OsString]) -> Result<CommandOutput>;
}

pub struct TokioCommandRunner;

#[async_trait]
impl CommandRunner for TokioCommandRunner {
    async fn run(&self, program: &str, args: &[OsString]) -> Result<CommandOutput> {
        let output = Command::new(program)
            .args(args)
            .output()
            .await
            .with_context(|| format!("executing {program}"))?;
        Ok(CommandOutput {
            status: output.status,
            stdout: output.stdout,
            stderr: output.stderr,
        })
    }
}

pub struct PodmanSupervisor<R = TokioCommandRunner> {
    command: R,
    podman_binary: String,
}

impl PodmanSupervisor<TokioCommandRunner> {
    pub fn new() -> Self {
        Self {
            command: TokioCommandRunner,
            podman_binary: "podman".to_string(),
        }
    }
}

impl Default for PodmanSupervisor<TokioCommandRunner> {
    fn default() -> Self {
        Self::new()
    }
}

impl<R> PodmanSupervisor<R> {
    pub fn with_command_runner(command: R) -> Self {
        Self {
            command,
            podman_binary: "podman".to_string(),
        }
    }

    fn pod_create_args(spec: &PlacementSpec, pod_name: &str) -> Vec<OsString> {
        let mut args = vec![
            "pod".into(),
            "create".into(),
            "--replace".into(),
            "--name".into(),
            pod_name.into(),
            "--userns=keep-id".into(),
            "--label".into(),
            format!("oqto.workspace={}", spec.workspace_id).into(),
            "--label".into(),
            format!("oqto.account={}", spec.account_id).into(),
            "--label".into(),
            "oqto.placement=rootless-podman".into(),
        ];
        if let RunnerEndpointConfig::TcpTls { address, .. } = &spec.runner_endpoint {
            args.extend(["--publish".into(), format!("{}:7443", address).into()]);
        }
        args
    }

    fn create_args(spec: &PlacementSpec, name: &str, pod_name: &str) -> Result<Vec<OsString>> {
        let mut args = vec![
            "run".into(),
            "--detach".into(),
            "--replace".into(),
            "--name".into(),
            name.into(),
            "--pod".into(),
            pod_name.into(),
            "--security-opt=no-new-privileges".into(),
            "--cap-drop=all".into(),
            "--cap-add=chown,dac_override,setuid,setgid".into(),
            "--label".into(),
            format!("oqto.workspace={}", spec.workspace_id).into(),
            "--label".into(),
            format!("oqto.account={}", spec.account_id).into(),
            "--label".into(),
            "oqto.placement=rootless-podman".into(),
            "--volume".into(),
            format!("{}:/workspace:Z", spec.workspace_dir.display()).into(),
            // Preserve the canonical workspace path carried by session metadata.
            // This avoids transport-specific cwd rewriting and keeps Pi JSONL paths stable.
            "--volume".into(),
            format!(
                "{}:{}:Z",
                spec.workspace_dir.display(),
                spec.workspace_dir.display()
            )
            .into(),
            "--volume".into(),
            format!("{}:/home/oqto:Z", spec.state_dir.display()).into(),
        ];
        if let Some(cpu) = &spec.cpu_limit {
            args.extend(["--cpus".into(), cpu.into()]);
        }
        if let Some(memory) = &spec.memory_limit {
            args.extend(["--memory".into(), memory.into()]);
        }
        for (key, value) in &spec.environment {
            args.extend(["--env".into(), format!("{key}={value}").into()]);
        }

        let runner_args: Vec<OsString> = match &spec.runner_endpoint {
            RunnerEndpointConfig::Unix { path } => {
                let parent = path.parent().ok_or_else(|| {
                    anyhow::anyhow!("Unix runner endpoint must have a parent directory")
                })?;
                args.extend([
                    "--volume".into(),
                    format!("{}:/run/oqto:Z", parent.display()).into(),
                ]);
                vec![
                    "oqto-runner".into(),
                    "--socket".into(),
                    "/run/oqto/runner.sock".into(),
                ]
            }
            RunnerEndpointConfig::TcpTls { .. } => {
                let server = spec.server_tls.as_ref().ok_or_else(|| {
                    anyhow::anyhow!("TCP/TLS runner placement requires server TLS material")
                })?;
                args.extend([
                    "--volume".into(),
                    format!("{}:/run/oqto/ca.pem:ro,Z", server.client_ca.display()).into(),
                    "--volume".into(),
                    format!("{}:/run/oqto/runner.pem:ro,Z", server.certificate.display()).into(),
                    "--volume".into(),
                    format!("{}:/run/oqto/runner-key.pem:ro,Z", server.key.display()).into(),
                ]);
                vec![
                    "oqto-runner".into(),
                    "--listen-tls".into(),
                    "0.0.0.0:7443".into(),
                    "--tls-client-ca".into(),
                    "/run/oqto/ca.pem".into(),
                    "--tls-cert".into(),
                    "/run/oqto/runner.pem".into(),
                    "--tls-key".into(),
                    "/run/oqto/runner-key.pem".into(),
                ]
            }
        };
        args.push(spec.image.clone().into());
        args.extend(runner_args);
        Ok(args)
    }

    /// Read-only host-side discovery. A rootless `podman info` result is only
    /// detection: the actual container launch, sandbox, and operator policy
    /// remain separate, unverified gates. Callers must not turn this report
    /// into an authorization without independently filling those gates.
    pub async fn probe_read_only(&self) -> PlacementCapabilityReport
    where
        R: CommandRunner,
    {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_millis() as u64);
        self.probe_read_only_at(now, now.saturating_add(30_000))
            .await
    }

    async fn probe_read_only_at(
        &self,
        observed_at_unix_ms: u64,
        expires_at_unix_ms: u64,
    ) -> PlacementCapabilityReport
    where
        R: CommandRunner,
    {
        let rootless = match self
            .checked(&[
                "info".into(),
                "--format".into(),
                "{{.Host.Security.Rootless}}".into(),
            ])
            .await
        {
            Ok(output) => match std::str::from_utf8(&output.stdout).map(str::trim) {
                Ok("true") => ProbeEvidence::Verified,
                Ok("false") => ProbeEvidence::Denied("Podman engine is rootful".into()),
                _ => ProbeEvidence::Unverified("Podman rootless state is unknown".into()),
            },
            Err(_) => ProbeEvidence::Unverified("Podman info probe failed".into()),
        };
        PlacementCapabilityReport {
            backend: PlacementKind::RootlessPodman,
            source: "host-side-podman-supervisor".into(),
            observed_at_unix_ms,
            expires_at_unix_ms,
            effective_rootless: rootless,
            container_launch: ProbeEvidence::Unverified("no launch canary was run".into()),
            runner_sandbox: ProbeEvidence::Unverified("runner sandbox was not attested".into()),
            operator_policy: ProbeEvidence::Unverified("operator policy was not checked".into()),
        }
    }

    /// Rootless Podman is the isolation owner for this placement. Probe the
    /// effective engine before creating host directories or a pod; a binary on
    /// PATH (or a rootful Docker-compatible socket) proves nothing about it.
    async fn require_rootless(&self) -> Result<()>
    where
        R: CommandRunner,
    {
        match self.probe_read_only().await.effective_rootless {
            ProbeEvidence::Verified => Ok(()),
            ProbeEvidence::Denied(reason) | ProbeEvidence::Unverified(reason) => {
                anyhow::bail!("rootless Podman required for workspace placement: {reason}")
            }
        }
    }

    async fn checked(&self, args: &[OsString]) -> Result<CommandOutput>
    where
        R: CommandRunner,
    {
        let output = self.command.run(&self.podman_binary, args).await?;
        if !output.status.success() {
            anyhow::bail!(
                "podman command failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }
        Ok(output)
    }
}

#[async_trait]
impl<R> PlacementSupervisor for PodmanSupervisor<R>
where
    R: CommandRunner,
{
    async fn start(&self, spec: &PlacementSpec) -> Result<PlacementRecord> {
        spec.validate()?;
        self.require_rootless().await?;
        tokio::fs::create_dir_all(&spec.workspace_dir).await?;
        tokio::fs::create_dir_all(&spec.state_dir).await?;
        let runtime_name = runtime_name(&spec.workspace_id);
        let pod_name = format!("{runtime_name}-pod");
        self.checked(&Self::pod_create_args(spec, &pod_name))
            .await?;
        if let Err(error) = self
            .checked(&Self::create_args(spec, &runtime_name, &pod_name)?)
            .await
        {
            let _ = self
                .command
                .run(
                    &self.podman_binary,
                    &["pod".into(), "rm".into(), "--force".into(), pod_name.into()],
                )
                .await;
            return Err(error);
        }
        Ok(PlacementRecord {
            id: PlacementId(uuid::Uuid::new_v4().to_string()),
            workspace_id: spec.workspace_id.clone(),
            account_id: spec.account_id.clone(),
            kind: PlacementKind::RootlessPodman,
            runner_endpoint: spec.runner_endpoint.clone(),
            runtime_name,
        })
    }

    async fn stop(&self, placement: &PlacementRecord) -> Result<()> {
        self.checked(&[
            "pod".into(),
            "rm".into(),
            "--force".into(),
            format!("{}-pod", placement.runtime_name).into(),
        ])
        .await?;
        Ok(())
    }

    async fn health(&self, placement: &PlacementRecord) -> Result<PlacementHealth> {
        let output = self
            .checked(&[
                "inspect".into(),
                "--format".into(),
                "{{.State.Running}}".into(),
                placement.runtime_name.clone().into(),
            ])
            .await?;
        if String::from_utf8_lossy(&output.stdout).trim() != "true" {
            return Ok(PlacementHealth::Stopped);
        }
        let client = RunnerClient::from_endpoint(&placement.runner_endpoint)?;
        match client.ensure_ready_with_recovery().await {
            Ok(()) => Ok(PlacementHealth::Ready),
            Err(error) => Ok(PlacementHealth::Unhealthy {
                detail: error.to_string(),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::process::ExitStatusExt;
    use std::sync::Mutex;

    struct RecordingRunner {
        calls: Mutex<Vec<Vec<OsString>>>,
    }

    #[async_trait]
    impl CommandRunner for RecordingRunner {
        async fn run(&self, _program: &str, args: &[OsString]) -> Result<CommandOutput> {
            self.calls.lock().unwrap().push(args.to_vec());
            Ok(CommandOutput {
                status: ExitStatus::from_raw(0),
                stdout: Vec::new(),
                stderr: Vec::new(),
            })
        }
    }

    #[test]
    fn podman_args_are_labelled_rootless_and_do_not_use_a_shell() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let spec = PlacementSpec {
            workspace_id: "workspace-A".to_string(),
            account_id: "account-A".to_string(),
            image: "localhost/oqto-workspace:test".to_string(),
            workspace_dir: temp.path().join("workspace"),
            state_dir: temp.path().join("state"),
            runner_endpoint: RunnerEndpointConfig::Unix {
                path: temp.path().join("runtime/runner.sock"),
            },
            server_tls: None,
            environment: Default::default(),
            cpu_limit: Some("2".to_string()),
            memory_limit: Some("2g".to_string()),
        };
        let args =
            PodmanSupervisor::<RecordingRunner>::create_args(&spec, "oqto-ws-a", "oqto-ws-a-pod")?;
        let rendered = args
            .iter()
            .map(|arg| arg.to_string_lossy())
            .collect::<Vec<_>>()
            .join(" ");
        assert!(rendered.contains("--pod oqto-ws-a-pod"));
        let pod_args = PodmanSupervisor::<RecordingRunner>::pod_create_args(&spec, "oqto-ws-a-pod");
        assert!(pod_args.contains(&OsString::from("--userns=keep-id")));
        assert!(rendered.contains("oqto.workspace=workspace-A"));
        assert!(rendered.contains("oqto.placement=rootless-podman"));
        assert!(rendered.contains("--security-opt=no-new-privileges"));
        assert!(!rendered.contains("sh -c"));
        Ok(())
    }

    struct RootlessProbeRunner {
        reply: Vec<u8>,
        calls: Mutex<Vec<Vec<OsString>>>,
    }

    #[async_trait]
    impl CommandRunner for RootlessProbeRunner {
        async fn run(&self, _program: &str, args: &[OsString]) -> Result<CommandOutput> {
            self.calls.lock().unwrap().push(args.to_vec());
            Ok(CommandOutput {
                status: ExitStatus::from_raw(0),
                stdout: self.reply.clone(),
                stderr: Vec::new(),
            })
        }
    }

    #[tokio::test]
    async fn rootful_or_unverified_podman_is_rejected_before_host_mutation() -> Result<()> {
        for reply in ["false\n", "\n", "unknown\n"] {
            let temp = tempfile::tempdir()?;
            let spec = PlacementSpec {
                workspace_id: "workspace".to_string(),
                account_id: "account".to_string(),
                image: "localhost/oqto:test".to_string(),
                workspace_dir: temp.path().join("workspace"),
                state_dir: temp.path().join("state"),
                runner_endpoint: RunnerEndpointConfig::Unix {
                    path: temp.path().join("runtime/runner.sock"),
                },
                server_tls: None,
                environment: Default::default(),
                cpu_limit: None,
                memory_limit: None,
            };
            let supervisor = PodmanSupervisor::with_command_runner(RootlessProbeRunner {
                reply: reply.as_bytes().to_vec(),
                calls: Mutex::new(Vec::new()),
            });
            assert!(supervisor.start(&spec).await.is_err());
            assert!(!spec.workspace_dir.exists());
            assert!(!spec.state_dir.exists());
            let calls = supervisor.command.calls.lock().unwrap();
            assert_eq!(calls.len(), 1, "no create/run command after denied probe");
            assert_eq!(calls[0][0], "info");
        }
        Ok(())
    }

    #[tokio::test]
    async fn read_only_probe_reports_rootless_without_claiming_placement_availability() {
        for (reply, expected) in [
            ("true\n", ProbeEvidence::Verified),
            (
                "false\n",
                ProbeEvidence::Denied("Podman engine is rootful".into()),
            ),
            (
                "unknown\n",
                ProbeEvidence::Unverified("Podman rootless state is unknown".into()),
            ),
        ] {
            let supervisor = PodmanSupervisor::with_command_runner(RootlessProbeRunner {
                reply: reply.as_bytes().to_vec(),
                calls: Mutex::new(Vec::new()),
            });
            let report = supervisor.probe_read_only_at(100, 200).await;
            assert_eq!(report.effective_rootless, expected);
            assert!(!report.evaluate_at(150).available);
            assert!(report.container_launch != ProbeEvidence::Verified);
            assert!(report.operator_policy != ProbeEvidence::Verified);
            let calls = supervisor.command.calls.lock().unwrap();
            assert_eq!(calls.len(), 1);
            assert_eq!(calls[0][0], "info");
        }
    }

    struct UnavailableEngine;

    #[async_trait]
    impl CommandRunner for UnavailableEngine {
        async fn run(&self, _program: &str, _args: &[OsString]) -> Result<CommandOutput> {
            anyhow::bail!("socket path and private host diagnostics must not be advertised")
        }
    }

    #[tokio::test]
    async fn failed_engine_probe_is_unverified_and_cannot_mutate_host() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let spec = PlacementSpec {
            workspace_id: "workspace".to_string(),
            account_id: "account".to_string(),
            image: "localhost/oqto:test".to_string(),
            workspace_dir: temp.path().join("workspace"),
            state_dir: temp.path().join("state"),
            runner_endpoint: RunnerEndpointConfig::Unix {
                path: temp.path().join("runtime/runner.sock"),
            },
            server_tls: None,
            environment: Default::default(),
            cpu_limit: None,
            memory_limit: None,
        };
        let supervisor = PodmanSupervisor::with_command_runner(UnavailableEngine);
        let report = supervisor.probe_read_only_at(100, 200).await;
        assert_eq!(
            report.effective_rootless,
            ProbeEvidence::Unverified("Podman info probe failed".into())
        );
        assert!(!report.evaluate_at(150).available);
        let serialized = serde_json::to_string(&report)?;
        assert!(!serialized.contains("private host diagnostics"));
        assert!(supervisor.start(&spec).await.is_err());
        assert!(!spec.workspace_dir.exists());
        assert!(!spec.state_dir.exists());
        Ok(())
    }

    #[tokio::test]
    async fn rootless_probe_precedes_pod_creation() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let spec = PlacementSpec {
            workspace_id: "workspace".to_string(),
            account_id: "account".to_string(),
            image: "localhost/oqto:test".to_string(),
            workspace_dir: temp.path().join("workspace"),
            state_dir: temp.path().join("state"),
            runner_endpoint: RunnerEndpointConfig::Unix {
                path: temp.path().join("runtime/runner.sock"),
            },
            server_tls: None,
            environment: Default::default(),
            cpu_limit: None,
            memory_limit: None,
        };
        let supervisor = PodmanSupervisor::with_command_runner(RootlessProbeRunner {
            reply: b"true\n".to_vec(),
            calls: Mutex::new(Vec::new()),
        });
        supervisor.start(&spec).await?;
        let calls = supervisor.command.calls.lock().unwrap();
        assert_eq!(calls.len(), 3);
        assert_eq!(calls[0][0], "info");
        assert_eq!(calls[1][0], "pod");
        assert_eq!(calls[2][0], "run");
        Ok(())
    }
}
