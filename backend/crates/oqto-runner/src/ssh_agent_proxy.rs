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

use anyhow::Result;
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

/// Locate the agent to proxy for.
///
/// The runner inherits `SSH_AUTH_SOCK` only if it was started from a session
/// that had one, which is not true for a service. Fall back to the well-known
/// per-user socket so an agent started later is still found.
fn resolve_agent_socket(auth_sock: Option<&str>, runtime_dir: Option<&str>) -> Option<PathBuf> {
    if let Some(value) = auth_sock.filter(|value| !value.is_empty()) {
        let path = PathBuf::from(value);
        if path.exists() {
            return Some(path);
        }
    }

    let candidate = PathBuf::from(runtime_dir?).join("ssh-agent.socket");
    candidate.exists().then_some(candidate)
}

fn upstream_agent_socket() -> Option<PathBuf> {
    resolve_agent_socket(
        std::env::var("SSH_AUTH_SOCK").ok().as_deref(),
        std::env::var("XDG_RUNTIME_DIR").ok().as_deref(),
    )
}

/// Start a proxy for one session.
///
/// Returns `Ok(None)` when the work directory did not enable it, or when no
/// agent is reachable. A missing agent means the session simply has no keys,
/// which is less privileged than running without the grant -- refusing to
/// start the session would deny service without protecting anything.
pub fn spawn(config: &SshProxyConfig, session_socket_dir: &Path) -> Result<Option<SshAgentProxy>> {
    if !config.enabled {
        return Ok(None);
    }

    let Some(upstream) = upstream_agent_socket() else {
        warn!(
            "SSH keys are granted to this work directory but no ssh-agent was found \
             (SSH_AUTH_SOCK unset and no $XDG_RUNTIME_DIR/ssh-agent.socket). \
             Starting the session without SSH access."
        );
        return Ok(None);
    };

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

    let child = match command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(error) => {
            warn!(
                "Could not start {} ({error}). Starting the session without SSH access.",
                binary.display()
            );
            return Ok(None);
        }
    };

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
        "SSH agent proxy did not create {} in time. \
         Starting the session without SSH access.",
        socket.display()
    );
    drop(SshAgentProxy {
        child,
        socket,
        known_hosts,
    });
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefers_the_inherited_agent_socket() {
        let dir = tempfile::tempdir().expect("tempdir");
        let sock = dir.path().join("agent.sock");
        std::fs::write(&sock, b"").expect("create");

        let resolved = resolve_agent_socket(Some(sock.to_str().unwrap()), None);

        assert_eq!(resolved.as_deref(), Some(sock.as_path()));
    }

    /// A runner started as a service inherits no SSH_AUTH_SOCK, so an agent
    /// started later must still be found at the well-known per-user path.
    #[test]
    fn falls_back_to_the_runtime_dir_socket() {
        let dir = tempfile::tempdir().expect("tempdir");
        let sock = dir.path().join("ssh-agent.socket");
        std::fs::write(&sock, b"").expect("create");

        let resolved = resolve_agent_socket(None, dir.path().to_str());

        assert_eq!(resolved.as_deref(), Some(sock.as_path()));
    }

    #[test]
    fn ignores_a_stale_socket_path() {
        let dir = tempfile::tempdir().expect("tempdir");

        let resolved = resolve_agent_socket(Some("/nonexistent/agent.sock"), dir.path().to_str());

        assert_eq!(resolved, None, "a path that does not exist is not an agent");
    }

    #[test]
    fn reports_no_agent_when_nothing_is_available() {
        assert_eq!(resolve_agent_socket(None, None), None);
    }
}
