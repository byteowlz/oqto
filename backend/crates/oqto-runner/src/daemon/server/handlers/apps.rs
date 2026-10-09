use std::path::{Component, Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use tokio::io::AsyncWriteExt;

use super::super::*;

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
                .map_err(|error| format!("invalid pinned App manifest: {error}"))?;
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
        .map_err(|error| format!("invalid pinned operations table: {error}"))?;
        let operation = table
            .resolve(&[req.operation_id])
            .map_err(|error| format!("operation is not pinned: {error}"))?
            .into_iter()
            .next()
            .ok_or_else(|| "operation is not pinned".to_owned())?;
        let validated_input = operation
            .validate_input(&req.input)
            .map_err(|error| format!("invalid operation input: {error}"))?;
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
        let mut command = tokio::process::Command::new(&executable);
        command
            .args(&operation.args)
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
        let mut child = command
            .spawn()
            .map_err(|_| "failed to start pinned operation".to_owned())?;
        if let Some(bytes) = input {
            let mut stdin = child
                .stdin
                .take()
                .ok_or_else(|| "operation stdin unavailable".to_owned())?;
            stdin
                .write_all(&bytes)
                .await
                .map_err(|_| "failed to write operation input".to_owned())?;
        }
        let output = tokio::time::timeout(
            Duration::from_secs(operation.timeout_seconds),
            child.wait_with_output(),
        )
        .await
        .map_err(|_| "operation timed out".to_owned())?
        .map_err(|_| "operation process failed".to_owned())?;
        if output.stdout.len() > MAX_OUTPUT_BYTES || output.stderr.len() > MAX_OUTPUT_BYTES {
            return Err("operation output exceeds the runner limit".to_owned());
        }
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        if !output.status.success() {
            return Ok(AppOperationResultResponse {
                success: false,
                code: "operation_failed".to_owned(),
                message: if stderr.is_empty() {
                    "operation failed".to_owned()
                } else {
                    stderr
                },
                output: serde_json::Value::Null,
            });
        }
        let stdout = String::from_utf8(output.stdout)
            .map_err(|_| "operation output is not UTF-8".to_owned())?;
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

fn safe_operation_path(path: &Path) -> bool {
    !path.is_absolute()
        && path
            .components()
            .all(|part| matches!(part, Component::Normal(_)))
        && path.starts_with("operations")
        && path.components().count() > 1
}
