#![cfg(target_os = "linux")]

//! LISTEN_FDS socket activation: the runner must serve on an inherited fd 3
//! and shut down gracefully on SIGTERM.

use anyhow::{Context, Result};
use std::os::fd::AsRawFd;
use std::time::{Duration, Instant};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn runner_serves_on_inherited_fd_and_stops_on_sigterm() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let socket_path = temp.path().join("activated.sock");
    let listener = std::os::unix::net::UnixListener::bind(&socket_path)?;
    let fd = listener.as_raw_fd();

    // LISTEN_PID must equal the runner's own pid, which is unknowable before
    // spawn (and Command builds its exec envp explicitly, so pre_exec setenv
    // is discarded). `exec` from a shell preserves the pid: export $$ first.
    let mut command = std::process::Command::new("bash");
    command
        .arg("-c")
        .arg("export LISTEN_PID=$$; exec \"$0\"")
        .arg(env!("CARGO_BIN_EXE_oqto-runner"))
        .env("HOME", temp.path())
        .env("XDG_STATE_HOME", temp.path().join("state"))
        .env("LISTEN_FDS", "1")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    // SAFETY: dup2 in pre_exec only duplicates the listener onto fd 3
    // (close-on-exec cleared by dup2), mirroring systemd's fd passing.
    unsafe {
        use std::os::unix::process::CommandExt;
        command.pre_exec(move || {
            if libc::dup2(fd, 3) < 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let mut child = command.spawn().context("spawning oqto-runner")?;

    async {
        // The runner must accept connections on the activated socket.
        let client = oqto_runner::client::RunnerClient::new(socket_path.clone());
        tokio::time::timeout(Duration::from_secs(15), async {
            loop {
                if child.try_wait()?.is_some() {
                    anyhow::bail!("runner exited before becoming ready");
                }
                if client.ensure_ready_with_recovery().await.is_ok() {
                    break Ok::<_, anyhow::Error>(());
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        })
        .await??;

        // SIGTERM must produce a prompt, clean exit.
        // SAFETY: plain kill(2) on our own child.
        unsafe { libc::kill(child.id() as i32, libc::SIGTERM) };
        let started = Instant::now();
        let status = tokio::time::timeout(
            Duration::from_secs(10),
            tokio::task::spawn_blocking(move || child.wait()),
        )
        .await
        .context("runner did not exit within 10s of SIGTERM")???;
        anyhow::ensure!(status.success(), "runner exited unclean: {status}");
        anyhow::ensure!(
            started.elapsed() < Duration::from_secs(8),
            "shutdown took {:?}",
            started.elapsed()
        );

        // The activation manager owns the socket file: it must still exist.
        anyhow::ensure!(
            socket_path.exists(),
            "runner removed the activation-owned socket file"
        );
        Ok::<_, anyhow::Error>(())
    }
    .await
}
