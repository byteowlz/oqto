//! Cancellation-safe ownership of a recovery child until readiness adoption.
use anyhow::{Context, Result};
use std::ffi::OsStr;
use std::path::Path;
use tokio::process::Child;

pub(crate) struct RecoveryChild(Option<Child>);

impl RecoveryChild {
    pub(crate) fn spawn(binary: &OsStr, socket: &Path) -> Result<Self> {
        crate::runtime_guard::ensure_launch_identity()?;
        crate::runtime_guard::ensure_socket_identity(socket)?;
        let child = tokio::process::Command::new(binary)
            .process_group(0)
            .arg("--socket")
            .arg(socket)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .context("spawning oqto-runner for single-user recovery")?;
        Ok(Self(Some(child)))
    }

    /// Successful readiness transfers the daemon to durable service ownership.
    /// Backend/task exit must not kill an adopted runner hosting agent sessions.
    fn into_adopted(mut self) -> Option<Child> {
        self.0.take()
    }

    pub(crate) fn adopt(self) {
        if let Some(mut child) = self.into_adopted() {
            tokio::spawn(async move {
                let _ = child.wait().await;
            });
        }
    }

    fn kill_unadopted_group(child: &mut Child) -> Result<()> {
        if let Some(pid) = child.id() {
            // SAFETY: spawn assigned this unadopted child its own PGID=PID.
            // Its retained Child handle reserves the PID until reaping. No
            // adopted daemon or caller process group can reach this method.
            let result = unsafe { libc::kill(-(pid as libc::pid_t), libc::SIGKILL) };
            if result != 0 {
                let error = std::io::Error::last_os_error();
                if error.raw_os_error() != Some(libc::ESRCH) {
                    return Err(error).context("kill unadopted recovery process group");
                }
            }
        }
        child.start_kill().context("kill failed recovery child")
    }

    pub(crate) async fn stop(&mut self) -> Result<()> {
        if let Some(child) = self.0.as_mut() {
            Self::kill_unadopted_group(child)?;
            tokio::time::timeout(std::time::Duration::from_secs(3), child.wait())
                .await
                .context("reap failed recovery child timed out")?
                .context("reap failed recovery child")?;
        }
        self.0.take();
        Ok(())
    }
}

impl Drop for RecoveryChild {
    fn drop(&mut self) {
        if let Some(mut child) = self.0.take() {
            // Only an unadopted child owned by this attempt can reach Drop.
            if let Err(error) = Self::kill_unadopted_group(&mut child) {
                tracing::warn!(%error, "cleanup cancelled recovery child");
            }
            if let Ok(runtime) = tokio::runtime::Handle::try_current() {
                runtime.spawn(async move {
                    let _ = child.wait().await;
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn fixture() -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().expect("fixture");
        let binary = dir.path().join("runner");
        std::fs::write(
            &binary,
            "#!/bin/sh\nprintf '%s\\n' \"$@\" > \"$0.args\"\nexec sleep 60\n",
        )
        .expect("script");
        std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o700)).expect("mode");
        (dir, binary)
    }

    async fn wait_started(binary: &Path) {
        for _ in 0..100 {
            if binary.with_extension("args").exists() {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        panic!("fixture child did not start");
    }

    #[tokio::test]
    async fn explicit_socket_and_failure_cleanup_reap_owned_child() {
        let (dir, binary) = fixture();
        let socket = dir.path().join("exact.sock");
        let mut child = RecoveryChild::spawn(binary.as_os_str(), &socket).expect("child");
        let pid = child.0.as_ref().expect("owned").id().expect("pid");
        wait_started(&binary).await;
        assert_eq!(
            std::fs::read_to_string(binary.with_extension("args")).expect("args"),
            format!("--socket\n{}\n", socket.display())
        );
        child.stop().await.expect("cleanup");
        assert!(child.0.is_none());
        assert!(!Path::new(&format!("/proc/{pid}")).exists());
    }

    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn cancellation_kills_unready_child() {
        let (dir, binary) = fixture();
        let child = RecoveryChild::spawn(binary.as_os_str(), &dir.path().join("runner.sock"))
            .expect("child");
        let pid = child.0.as_ref().expect("owned").id().expect("pid");
        wait_started(&binary).await;
        drop(child);
        for _ in 0..100 {
            if !Path::new(&format!("/proc/{pid}")).exists() {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        panic!("cancelled recovery leaked its child");
    }

    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn failed_recovery_stops_its_startup_helper_group_too() {
        let (dir, binary) = fixture();
        std::fs::write(
            &binary,
            "#!/bin/sh\nsleep 60 &\necho $! > \"$0.helper\"\nwait\n",
        )
        .expect("helper fixture");
        let mut child = RecoveryChild::spawn(binary.as_os_str(), &dir.path().join("runner.sock"))
            .expect("child");
        let marker = binary.with_extension("helper");
        for _ in 0..100 {
            if marker.exists()
                && !std::fs::read_to_string(&marker)
                    .expect("marker")
                    .trim()
                    .is_empty()
            {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        let helper = std::fs::read_to_string(marker).expect("helper PID");
        child.stop().await.expect("owned group cleanup");
        // An orphan zombie may await init reaping, but no helper may run.
        for _ in 0..100 {
            match std::fs::read_to_string(format!("/proc/{}/status", helper.trim())) {
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
                Ok(status)
                    if status
                        .lines()
                        .any(|line| line.starts_with("State:") && line.contains('Z')) =>
                {
                    return;
                }
                _ => tokio::time::sleep(std::time::Duration::from_millis(10)).await,
            }
        }
        panic!("failed recovery left its startup helper running");
    }

    #[tokio::test]
    async fn ownership_transfer_does_not_kill_adopted_child() {
        let (dir, binary) = fixture();
        let child = RecoveryChild::spawn(binary.as_os_str(), &dir.path().join("runner.sock"))
            .expect("child");
        wait_started(&binary).await;
        // Exercise the actual seam used by adopt, not a copied take.
        let mut durable = child.into_adopted().expect("transfer");
        assert!(durable.try_wait().expect("alive check").is_none());
        durable.kill().await.expect("fixture cleanup");
    }
}
