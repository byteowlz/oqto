use std::path::{Component, Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};

use super::super::*;

#[cfg(test)]
#[path = "apps_tests.rs"]
mod tests;

const MAX_OUTPUT_BYTES: usize = 1_048_576;
const MAX_INPUT_BYTES: usize = 65_536;
const MAX_TIMEOUT_SECONDS: u64 = 300;

pub(crate) async fn handle_request(runner: &Runner, req: RunnerRequest) -> RunnerResponse {
    match req {
        RunnerRequest::RunAppOperation(request) => runner.run_app_operation(request).await,
        _ => error_response(ErrorCode::InvalidRequest, "Invalid App operation request"),
    }
}

impl Runner {
    async fn run_app_operation(&self, req: RunAppOperationRequest) -> RunnerResponse {
        match self.run_app_operation_inner(req).await {
            Ok(response) => RunnerResponse::AppOperationResult(response),
            Err(message) => RunnerResponse::AppOperationResult(AppOperationResultResponse {
                success: false,
                code: "operation_rejected".to_owned(),
                message,
                output: serde_json::Value::Null,
            }),
        }
    }

    async fn run_app_operation_inner(
        &self,
        req: RunAppOperationRequest,
    ) -> Result<AppOperationResultResponse, String> {
        if req.content_digest.len() != 64
            || !req
                .content_digest
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
        {
            return Err("invalid Definition digest".to_owned());
        }
        let data_home = std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share"))
            })
            .ok_or_else(|| "runner data directory is unavailable".to_owned())?;
        let definition = data_home
            .join("oqto/app-artifacts/definitions")
            .join(&req.content_digest);
        let definition = tokio::fs::canonicalize(&definition)
            .await
            .map_err(|_| "immutable App Definition is unavailable".to_owned())?;
        let manifest_bytes = tokio::fs::read(definition.join("oqto-app.toml"))
            .await
            .map_err(|_| "pinned App manifest is unavailable".to_owned())?;
        let manifest =
            oqto_apps::parse_manifest(&format!("{}.oqtoapp", req.app_id), &manifest_bytes, 65_536)
                .map_err(|_| "invalid pinned App manifest".to_owned())?;
        let operations_request = manifest
            .operations_request()
            .ok_or_else(|| "pinned App has no operations capability".to_owned())?;
        if !operations_request.ids.contains(&req.operation_id) {
            return Err("operation is not requested by the pinned App manifest".to_owned());
        }
        let table_bytes = tokio::fs::read(definition.join(&operations_request.table))
            .await
            .map_err(|_| "pinned operations table is unavailable".to_owned())?;
        let table = oqto_apps::parse_operations_table(
            &table_bytes,
            2 * 1024 * 1024,
            &operations_request.table,
        )
        .map_err(|_| "invalid pinned operations table".to_owned())?;
        let operation = table
            .resolve(&[req.operation_id])
            .map_err(|_| "operation is not pinned".to_owned())?
            .into_iter()
            .next()
            .ok_or_else(|| "operation is not pinned".to_owned())?;
        let validated_input = operation
            .validate_input(&req.input)
            .map_err(|_| "invalid operation input".to_owned())?;
        if operation.timeout_seconds == 0 || operation.timeout_seconds > MAX_TIMEOUT_SECONDS {
            return Err("operation timeout is outside the runner limit".to_owned());
        }
        if !safe_operation_path(&operation.executable) {
            return Err("operation executable is not a safe package-relative path".to_owned());
        }
        let executable = definition.join(&operation.executable);
        let executable = tokio::fs::canonicalize(&executable)
            .await
            .map_err(|_| "pinned operation executable is unavailable".to_owned())?;
        if !executable.starts_with(&definition) {
            return Err("operation executable escapes its immutable Definition".to_owned());
        }
        let metadata = tokio::fs::symlink_metadata(&executable)
            .await
            .map_err(|_| "cannot inspect operation executable".to_owned())?;
        if !metadata.is_file() {
            return Err("operation executable is not a regular file".to_owned());
        }

        let workspace_root = tokio::fs::canonicalize(&self.user_config.workspace_dir)
            .await
            .map_err(|_| "runner workspace root is unavailable".to_owned())?;
        let work_directory = tokio::fs::canonicalize(&req.work_directory)
            .await
            .map_err(|_| "bound work directory is unavailable".to_owned())?;
        if !work_directory.starts_with(&workspace_root) {
            return Err("bound work directory is outside the runner workspace root".to_owned());
        }

        let input = match operation.stdin {
            oqto_apps::OperationStdin::Json => Some(
                serde_json::to_vec(&validated_input)
                    .map_err(|_| "operation input is not JSON".to_owned())?,
            ),
            oqto_apps::OperationStdin::None => None,
        };
        if input
            .as_ref()
            .is_some_and(|bytes| bytes.len() > MAX_INPUT_BYTES)
        {
            return Err("operation input exceeds the runner limit".to_owned());
        }

        let home =
            std::env::var_os("HOME").ok_or_else(|| "runner HOME is unavailable".to_owned())?;
        let home_path = PathBuf::from(&home);
        let managed_path = std::env::join_paths([
            PathBuf::from("/usr/local/bin"),
            PathBuf::from("/usr/bin"),
            PathBuf::from("/bin"),
            home_path.join(".local/bin"),
            home_path.join("go/bin"),
        ])
        .map_err(|_| "cannot construct runner operation PATH".to_owned())?;
        let config = operation_sandbox(
            self.sandbox_config.as_ref(),
            &home_path,
            &data_home,
            &definition,
            &work_directory,
        )?;
        let sandbox_command = oqto_sandbox::build_sandbox_command(
            &config,
            &work_directory,
            &executable,
            &operation.args,
            oqto_sandbox::SandboxStdin::Redirected,
        )
        .map_err(|_| "operation sandbox is unavailable or unsupported".to_owned())?;
        let mut command = tokio::process::Command::from(sandbox_command);
        command
            .current_dir(&work_directory)
            .kill_on_drop(true)
            .env_clear()
            .env("PATH", managed_path)
            .env("HOME", home);
        if let Some(config_home) = std::env::var_os("XDG_CONFIG_HOME") {
            command.env("XDG_CONFIG_HOME", config_home);
        }
        command
            .stdin(if input.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        oqto_sandbox::configure_bwrap_pre_exec(
            command.as_std_mut(),
            &config,
            &work_directory,
            None,
        )
        .map_err(|_| "operation sandbox setup failed".to_owned())?;
        #[cfg(unix)]
        command.process_group(0);
        let mut child = command
            .spawn()
            .map_err(|_| "failed to start pinned operation".to_owned())?;
        let (status, stdout) = collect_operation(
            &mut child,
            input,
            Duration::from_secs(operation.timeout_seconds),
        )
        .await?;
        if !status.success() {
            return Ok(AppOperationResultResponse {
                success: false,
                code: "operation_failed".to_owned(),
                message: "operation failed".to_owned(),
                output: serde_json::Value::Null,
            });
        }
        let stdout =
            String::from_utf8(stdout).map_err(|_| "operation output is not UTF-8".to_owned())?;
        let value = match operation.stdout {
            oqto_apps::OperationStdout::Json => serde_json::from_str(stdout.trim())
                .map_err(|_| "operation returned invalid JSON".to_owned())?,
            oqto_apps::OperationStdout::Lines => serde_json::Value::Array(
                stdout
                    .lines()
                    .map(|line| serde_json::Value::String(line.to_owned()))
                    .collect(),
            ),
        };
        Ok(AppOperationResultResponse {
            success: true,
            code: "ok".to_owned(),
            message: String::new(),
            output: value,
        })
    }
}

// No file-grant facts travel in this request. Do not merge agent-authored
// workspace config, caches, scoped paths, or ambient harness grants. Until the
// canonical request carries authorized resource grants, only package + stdio
// operations are supported; the bound directory is an empty, non-writable view.
fn operation_sandbox(
    configured: Option<&oqto_sandbox::SandboxConfig>,
    home: &Path,
    data: &Path,
    definition: &Path,
    workdir: &Path,
) -> Result<oqto_sandbox::SandboxConfig, String> {
    let mut config = configured
        .filter(|config| config.enabled)
        .cloned()
        .ok_or_else(|| "App operations require an enabled sandbox".to_owned())?;
    if config
        .network
        .as_ref()
        .is_some_and(|network| matches!(network.mode, oqto_sandbox::NetworkMode::Proxy))
    {
        return Err("App operation proxy mediation is unsupported".to_owned());
    }
    // The shared Linux builder mounts private /tmp after extra binds and
    // rebinds /tmp workdirs afterwards. That ordering cannot enforce this
    // stdio-only policy; reject rather than accidentally expose bound files.
    if definition.starts_with("/tmp") || workdir.starts_with("/tmp") {
        return Err("App operations do not support Definition/workdir under /tmp".to_owned());
    }
    let restrictive = oqto_sandbox::SandboxConfig::from_profile("strict");
    config.read_policy = restrictive.read_policy;
    config.home_access = restrictive.home_access;
    config.allow_read.clear();
    config.allow_write.clear();
    config.extra_rw_bind.clear();
    config.extra_ro_bind = vec![definition.to_string_lossy().into_owned()];
    config.scoped_paths.clear();
    config.overlay_enabled = false;
    config.workspace_cache_enabled = false;
    config.ssh = None;
    config.network = Some(oqto_sandbox::NetworkConfig {
        mode: oqto_sandbox::NetworkMode::Isolated,
        ..Default::default()
    });
    config.isolate_network = true;
    #[cfg(target_os = "linux")]
    {
        config.isolate_pid = true;
    }
    let config_home = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".config"));
    let runtime = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp"));
    let private = [
        config_home,
        home.join(".kyz"),
        data.join("kyz"),
        data.join("oqto/credentials"),
        data.join("oqto/runner"),
        runtime.join("kyz"),
        runtime.join("oqto"),
        runtime.join("oqto-runner.sock"),
        runtime,
        PathBuf::from("/run/oqto"),
        workdir.to_path_buf(),
    ];
    if private.iter().any(|path| definition.starts_with(path)) {
        return Err("Definition overlaps a private operation path".to_owned());
    }
    for path in private {
        let path = path.to_string_lossy().into_owned();
        config.deny_read.push(path.clone());
        config.deny_write.push(path);
    }
    Ok(config)
}

async fn bounded_output(mut stream: impl AsyncRead + Unpin) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    let mut buffer = [0_u8; 8192];
    loop {
        let count = stream
            .read(&mut buffer)
            .await
            .map_err(|_| "operation output collection failed".to_owned())?;
        if count == 0 {
            return Ok(bytes);
        }
        if count > MAX_OUTPUT_BYTES.saturating_sub(bytes.len()) {
            return Err("operation output exceeds the runner limit".to_owned());
        }
        bytes.extend_from_slice(&buffer[..count]);
    }
}

async fn collect_operation(
    child: &mut tokio::process::Child,
    input: Option<Vec<u8>>,
    deadline: Duration,
) -> Result<(std::process::ExitStatus, Vec<u8>), String> {
    let result = tokio::time::timeout(deadline, async {
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| "operation stdout unavailable".to_owned())?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| "operation stderr unavailable".to_owned())?;
        let stdin = child.stdin.take();
        let write = async move {
            if let Some(bytes) = input {
                let mut stdin = stdin.ok_or_else(|| "operation stdin unavailable".to_owned())?;
                stdin
                    .write_all(&bytes)
                    .await
                    .map_err(|_| "failed to write operation input".to_owned())?;
            }
            Ok::<(), String>(())
        };
        let wait = async {
            child
                .wait()
                .await
                .map_err(|_| "operation process failed".to_owned())
        };
        let (_, stdout, _, status) =
            tokio::try_join!(write, bounded_output(stdout), bounded_output(stderr), wait)?;
        Ok((status, stdout))
    })
    .await
    .unwrap_or_else(|_| Err("operation timed out".to_owned()));
    if result.is_err() {
        #[cfg(unix)]
        if let Some(pid) = child.id() {
            // SAFETY: the command starts a fresh process group identified by
            // its child pid; never signal the runner's own process group.
            unsafe {
                libc::kill(-(pid as i32), libc::SIGKILL);
            }
        }
        let _ = child.kill().await;
        let _ = child.wait().await;
    }
    result
}

fn safe_operation_path(path: &Path) -> bool {
    !path.is_absolute()
        && path
            .components()
            .all(|part| matches!(part, Component::Normal(_)))
        && path.starts_with("operations")
        && path.components().count() > 1
}
