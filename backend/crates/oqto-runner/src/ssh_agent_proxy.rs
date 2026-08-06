//! Per-session SSH agent proxy.
//!
//! Sandboxed sessions cannot read `~/.ssh` (the default profiles deny it), so
//! SSH-authenticated work such as `git pull` has no key material to use. The
//! proxy resolves this without ever placing a key inside the sandbox: the
//! user's own agent keeps the keys, the session gets an `SSH_AUTH_SOCK`
//! pointing at a proxy socket, and the proxy forwards only signature requests
//! for keys the work directory was granted.
//!
//! The socket lives in the session socket directory, which is already bound
//! into the sandbox, so the same wiring carries over to placements that mount
//! their endpoint directory the same way.

use anyhow::{Context, Result};
use log::{debug, info, warn};
use oqto_sandbox::SshProxyConfig;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

/// A running proxy, torn down when the session ends.
pub struct SshAgentProxy {
    child: Child,
    socket: PathBuf,
    known_hosts: Option<PathBuf>,
}

impl SshAgentProxy {
    /// Path the session should use as `SSH_AUTH_SOCK`.
    pub fn socket_path(&self) -> &Path {
        &self.socket
    }

    /// `GIT_SSH_COMMAND` giving the session the host keys it needs.
    ///
    /// Profiles deny `~/.ssh`, which also hides `known_hosts`, and SSH aborts
    /// on an unverified host before it ever reaches the agent. Host keys are
    /// public, so a copy is placed beside the socket and pointed at here.
    pub fn git_ssh_command(&self) -> Option<String> {
        self.known_hosts
            .as_ref()
            .map(|known_hosts| format!("ssh -o UserKnownHostsFile={}", known_hosts.display()))
    }
}

/// Copy the user's `known_hosts` next to the socket, inside the sandbox.
fn materialize_known_hosts(session_socket_dir: &Path) -> Option<PathBuf> {
    let source = dirs::home_dir()?.join(".ssh").join("known_hosts");
    let destination = session_socket_dir.join("known_hosts");
    match std::fs::copy(&source, &destination) {
        Ok(_) => Some(destination),
        Err(err) => {
            warn!(
                "Could not provide known_hosts from {}: {}. \
                 SSH will refuse unverified hosts.",
                source.display(),
                err
            );
            None
        }
    }
}

impl Drop for SshAgentProxy {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_file(&self.socket);
        debug!("SSH agent proxy stopped ({})", self.socket.display());
    }
}

/// Locate `oqto-ssh-proxy` next to the running binary, else on `PATH`.
fn proxy_binary() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.join("oqto-ssh-proxy")))
        .filter(|path| path.exists())
        .unwrap_or_else(|| PathBuf::from("oqto-ssh-proxy"))
}

/// Start a proxy for one session.
///
/// Returns `Ok(None)` when the work directory did not enable it. Fails when it
/// was enabled but cannot be honoured: a session that expects SSH must not
/// start believing it has an agent when it does not.
pub fn spawn(config: &SshProxyConfig, session_socket_dir: &Path) -> Result<Option<SshAgentProxy>> {
    if !config.enabled {
        return Ok(None);
    }

    let upstream = std::env::var("SSH_AUTH_SOCK")
        .ok()
        .filter(|value| !value.is_empty())
        .context(
            "SSH agent proxy enabled but SSH_AUTH_SOCK is unset: \
             start an ssh-agent holding the granted keys, or disable [ssh] for this work directory",
        )?;

    let socket = session_socket_dir.join("ssh-agent.sock");
    let _ = std::fs::remove_file(&socket);

    let binary = proxy_binary();
    let mut command = Command::new(&binary);
    command
        .arg("--listen")
        .arg(&socket)
        .arg("--upstream")
        .arg(&upstream);

    for key in &config.allowed_keys {
        command.arg("--allow-key").arg(key);
    }
    if !config.prompt_unknown {
        command.arg("--no-prompt");
    }

    let child = command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("Failed to spawn {}", binary.display()))?;

    let known_hosts = materialize_known_hosts(session_socket_dir);

    // The client fails outright if the socket is not there when it connects.
    for _ in 0..50 {
        if socket.exists() {
            info!(
                "SSH agent proxy ready ({}), granted keys: {:?}",
                socket.display(),
                config.allowed_keys
            );
            return Ok(Some(SshAgentProxy {
                child,
                socket,
                known_hosts,
            }));
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }

    warn!(
        "SSH agent proxy did not create {} in time",
        socket.display()
    );
    drop(SshAgentProxy {
        child,
        socket,
        known_hosts,
    });
    anyhow::bail!("SSH agent proxy failed to start")
}
