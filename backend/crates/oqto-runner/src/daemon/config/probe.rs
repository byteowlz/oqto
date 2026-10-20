//! Bounded native RPC qualification. Process survival is not readiness.
use anyhow::{Context, Result};
use rustix::fs::{OFlags, fcntl_getfl, fcntl_setfl};
use std::io::{Read, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const REQUEST_ID: &str = "oqto_startup_probe";

pub(super) fn qualify(path: &Path, workdir: Option<&Path>) -> Result<()> {
    let mut command = Command::new(path);
    command
        .args(["--mode", "rpc", "--no-session"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(workdir) = workdir {
        command.current_dir(workdir);
    }
    let mut child = command
        .spawn()
        .with_context(|| format!("starting Pi RPC probe: {}", path.display()))?;
    let result = (|| {
        let mut stdout = child.stdout.take().context("Pi probe stdout unavailable")?;
        fcntl_setfl(&stdout, fcntl_getfl(&stdout)? | OFlags::NONBLOCK)?;
        let mut stdin = child.stdin.take().context("Pi probe stdin unavailable")?;
        writeln!(stdin, "{{\"id\":\"{REQUEST_ID}\",\"type\":\"get_state\"}}")?;
        stdin.flush()?;
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut buffered = Vec::new();
        let mut chunk = [0; 4096];
        loop {
            match stdout.read(&mut chunk) {
                Ok(0) => anyhow::bail!("Pi exited before a successful get_state response"),
                Ok(count) => {
                    buffered.extend_from_slice(&chunk[..count]);
                    anyhow::ensure!(buffered.len() <= 65536, "Pi startup output exceeded 64KiB");
                    while let Some(end) = buffered.iter().position(|byte| *byte == b'\n') {
                        let line: Vec<u8> = buffered.drain(..=end).collect();
                        let Ok(value) = serde_json::from_slice::<serde_json::Value>(&line) else {
                            continue;
                        };
                        if value.get("id").and_then(|id| id.as_str()) == Some(REQUEST_ID) {
                            anyhow::ensure!(
                                value.get("type").and_then(|v| v.as_str()) == Some("response")
                                    && value.get("success").and_then(|v| v.as_bool()) == Some(true),
                                "Pi rejected startup get_state"
                            );
                            let state: oqto_pi::PiState = serde_json::from_value(
                                value
                                    .get("data")
                                    .cloned()
                                    .context("Pi startup get_state missing data")?,
                            )
                            .context("Pi startup get_state invalid state")?;
                            anyhow::ensure!(
                                state.session_id.is_some_and(|id| !id.trim().is_empty()),
                                "Pi startup get_state missing native identity"
                            );
                            anyhow::ensure!(
                                child.try_wait()?.is_none(),
                                "Pi exited during startup handshake"
                            );
                            return Ok(());
                        }
                    }
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {}
                Err(error) => return Err(error).context("reading Pi startup response"),
            }
            anyhow::ensure!(
                child.try_wait()?.is_none(),
                "Pi exited before startup handshake"
            );
            anyhow::ensure!(
                Instant::now() < deadline,
                "Pi startup get_state timed out after 5s"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    })();
    // Inherit the runner's group so recovery_child can clean the whole owned
    // recovery tree on cancellation. Release launchers exec their native runtime.
    // Never signal the inherited group here (it may include an adopted runner).
    let _ = child.kill();
    let _ = child.wait();
    // Do not forward arbitrary stderr: wrappers/extensions may print credentials.
    // The actual-session startup path provides bounded extension diagnostics.
    result.with_context(|| {
        format!(
            "Pi qualification failed for {}; no fallback is permitted",
            path.display()
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn fake_pi(script: &str) -> Result<(tempfile::TempDir, std::path::PathBuf)> {
        let temp = tempfile::tempdir()?;
        let path = temp.path().join("pi");
        std::fs::write(&path, format!("#!/bin/sh\n{script}\n"))?;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700))?;
        Ok((temp, path))
    }

    #[test]
    fn early_exit_is_not_ready_and_stderr_secrets_are_not_returned() -> Result<()> {
        let (_temp, path) = fake_pi("echo 'Bearer sensitive-value' >&2; exit 1")?;
        let error = qualify(&path, None).err().context("exit was accepted")?;
        let diagnostic = format!("{error:#}");
        assert!(diagnostic.contains("no fallback"));
        assert!(!diagnostic.contains("sensitive-value"));
        Ok(())
    }

    #[test]
    fn process_survival_and_unsolicited_events_are_not_ready() -> Result<()> {
        let (_temp, path) = fake_pi("echo '{\"type\":\"agent_start\"}'; exec sleep 30")?;
        let start = Instant::now();
        assert!(qualify(&path, None).is_err());
        assert!(start.elapsed() < Duration::from_secs(7));
        Ok(())
    }

    #[test]
    fn correlated_native_state_is_ready() -> Result<()> {
        let state = serde_json::json!({"type":"response","id":REQUEST_ID,"success":true,"data":{
            "model":null,"thinkingLevel":"off","isStreaming":false,"isCompacting":false,
            "steeringMode":"all","followUpMode":"all","sessionId":"native-probe",
            "autoCompactionEnabled":true,"messageCount":0,"pendingMessageCount":0}});
        let (_temp, path) = fake_pi(&format!(
            "read -r request; printf '%s\\n' '{state}'; exec sleep 30"
        ))?;
        qualify(&path, None)
    }
}
