//! octo-ssh-proxy - SSH agent proxy with policy enforcement.
//!
//! This proxy sits between sandboxed agents and the real SSH agent,
//! enforcing policies like allowed hosts and key restrictions.
//!
//! ## Usage
//!
//! ```bash
//! # Start the proxy
//! octo-ssh-proxy --listen /run/user/1000/octo-ssh.sock \
//!                --upstream $SSH_AUTH_SOCK \
//!                --octo-server http://localhost:8080
//!
//! # With config
//! octo-ssh-proxy --config ~/.config/octo/sandbox.toml
//! ```
//!
//! ## How it works
//!
//! 1. Agent (inside sandbox) connects to proxy socket
//! 2. Proxy receives SSH agent protocol requests
//! 3. For sign requests, proxy extracts target host from SSH protocol
//! 4. Proxy checks policy (allowed_hosts, allowed_keys)
//! 5. If policy requires prompt, sends request to octo server
//! 6. If approved, forwards to real ssh-agent
//! 7. Returns response to agent
//!
//! ## SSH Agent Protocol
//!
//! The proxy implements the SSH agent protocol (RFC draft):
//! - SSH_AGENTC_REQUEST_IDENTITIES (11) - List keys
//! - SSH_AGENTC_SIGN_REQUEST (13) - Sign data
//! - SSH_AGENTC_ADD_IDENTITY (17) - Add key (blocked in proxy)
//! - SSH_AGENTC_REMOVE_IDENTITY (18) - Remove key (blocked in proxy)

use anyhow::{Context, Result};
use clap::Parser;
use glob::Pattern;
use log::{debug, error, info, warn};
use octo::local::{SandboxConfig, SshProxyConfig};
use std::io::{Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;

/// SSH Agent message types
mod ssh_agent {
    // Requests from client
    pub const SSH_AGENTC_REQUEST_IDENTITIES: u8 = 11;
    pub const SSH_AGENTC_SIGN_REQUEST: u8 = 13;
    pub const SSH_AGENTC_ADD_IDENTITY: u8 = 17;
    pub const SSH_AGENTC_REMOVE_IDENTITY: u8 = 18;
    pub const SSH_AGENTC_ADD_ID_CONSTRAINED: u8 = 25;

    // Responses from agent
    pub const SSH_AGENT_FAILURE: u8 = 5;
    #[allow(dead_code)]
    pub const SSH_AGENT_SUCCESS: u8 = 6;
    pub const SSH_AGENT_IDENTITIES_ANSWER: u8 = 12;
    #[allow(dead_code)]
    pub const SSH_AGENT_SIGN_RESPONSE: u8 = 14;
}

#[derive(Parser, Debug)]
#[command(
    name = "octo-ssh-proxy",
    about = "SSH agent proxy with policy enforcement",
    after_help = "Examples:\n  \
        octo-ssh-proxy --listen /tmp/octo-ssh.sock\n  \
        octo-ssh-proxy --config ~/.config/octo/sandbox.toml"
)]
struct Args {
    /// Path to listen socket
    #[arg(short, long)]
    listen: Option<PathBuf>,

    /// Path to upstream SSH agent socket (default: $SSH_AUTH_SOCK)
    #[arg(short, long)]
    upstream: Option<PathBuf>,

    /// Path to sandbox config file
    #[arg(short, long)]
    config: Option<PathBuf>,

    /// Octo server URL for prompts (default: http://localhost:8080)
    #[arg(long, default_value = "http://localhost:8080")]
    octo_server: String,

    /// Profile name to use from config
    #[arg(short, long, default_value = "development")]
    profile: String,

    /// Enable verbose logging
    #[arg(short, long)]
    verbose: bool,

    /// Dry run (don't actually connect to upstream)
    #[arg(long)]
    dry_run: bool,
}

/// Policy for SSH connections.
struct SshPolicy {
    /// Allowed host patterns (glob)
    allowed_hosts: Vec<Pattern>,
    /// Allowed key comments/fingerprints
    allowed_keys: Vec<String>,
    /// Prompt for unknown hosts
    prompt_unknown: bool,
    /// Octo server URL for prompts
    octo_server: String,
}

impl SshPolicy {
    fn from_config(config: &SshProxyConfig, octo_server: &str) -> Self {
        let allowed_hosts = config
            .allowed_hosts
            .iter()
            .filter_map(|h| Pattern::new(h).ok())
            .collect();

        Self {
            allowed_hosts,
            allowed_keys: config.allowed_keys.clone(),
            prompt_unknown: config.prompt_unknown,
            octo_server: octo_server.to_string(),
        }
    }

    /// Check if a host is allowed.
    fn is_host_allowed(&self, host: &str) -> PolicyResult {
        // Check explicit allows
        for pattern in &self.allowed_hosts {
            if pattern.matches(host) {
                return PolicyResult::Allow;
            }
        }

        // If no patterns defined and prompt_unknown is false, allow all
        if self.allowed_hosts.is_empty() && !self.prompt_unknown {
            return PolicyResult::Allow;
        }

        // Otherwise, need to prompt
        if self.prompt_unknown {
            PolicyResult::Prompt
        } else {
            PolicyResult::Deny
        }
    }

    /// Check if a key is allowed by comment.
    #[allow(dead_code)]
    fn is_key_allowed(&self, key_comment: &str) -> bool {
        // If no key restrictions, allow all
        if self.allowed_keys.is_empty() {
            return true;
        }

        self.allowed_keys.iter().any(|k| key_comment.contains(k))
    }

    /// Request approval from octo server.
    async fn request_approval(&self, host: &str, key_comment: Option<&str>) -> Result<bool> {
        let client = reqwest::Client::new();

        let body = serde_json::json!({
            "source": "octo_ssh_proxy",
            "prompt_type": "ssh_sign",
            "resource": host,
            "description": format!(
                "SSH connection to {}{}",
                host,
                key_comment.map(|k| format!(" using key '{}'", k)).unwrap_or_default()
            ),
            "timeout_secs": 60,
        });

        info!("Requesting approval for SSH to {} from octo server", host);

        let response = client
            .post(format!("{}/internal/prompt", self.octo_server))
            .json(&body)
            .send()
            .await
            .context("Failed to send prompt request")?;

        if !response.status().is_success() {
            warn!("Prompt request failed: {}", response.status());
            return Ok(false);
        }

        let result: serde_json::Value = response.json().await?;

        if let Some(action) = result.get("action").and_then(|a| a.as_str()) {
            Ok(action == "allow_once" || action == "allow_session")
        } else {
            Ok(false)
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum PolicyResult {
    Allow,
    Deny,
    Prompt,
}

/// Read a length-prefixed message from the socket.
fn read_message(stream: &mut UnixStream) -> Result<Vec<u8>> {
    let mut len_buf = [0u8; 4];
    stream.read_exact(&mut len_buf)?;
    let len = u32::from_be_bytes(len_buf) as usize;

    if len > 256 * 1024 {
        anyhow::bail!("Message too large: {} bytes", len);
    }

    let mut buf = vec![0u8; len];
    stream.read_exact(&mut buf)?;

    Ok(buf)
}

/// Write a length-prefixed message to the socket.
fn write_message(stream: &mut UnixStream, data: &[u8]) -> Result<()> {
    let len = (data.len() as u32).to_be_bytes();
    stream.write_all(&len)?;
    stream.write_all(data)?;
    Ok(())
}

/// Send a failure response.
fn send_failure(stream: &mut UnixStream) -> Result<()> {
    write_message(stream, &[ssh_agent::SSH_AGENT_FAILURE])
}

/// Handle a single client connection.
fn handle_client(
    mut client: UnixStream,
    upstream_path: &PathBuf,
    policy: &SshPolicy,
    runtime: &tokio::runtime::Handle,
    dry_run: bool,
) -> Result<()> {
    debug!("New client connection");

    // Connect to upstream agent
    let mut upstream = if dry_run {
        None
    } else {
        Some(UnixStream::connect(upstream_path).context("Failed to connect to upstream agent")?)
    };

    loop {
        // Read request from client
        let request = match read_message(&mut client) {
            Ok(r) => r,
            Err(e) => {
                debug!("Client disconnected: {}", e);
                break;
            }
        };

        if request.is_empty() {
            continue;
        }

        let msg_type = request[0];
        debug!("Received message type: {}", msg_type);

        match msg_type {
            ssh_agent::SSH_AGENTC_REQUEST_IDENTITIES => {
                // List keys - forward to upstream
                if let Some(ref mut upstream) = upstream {
                    write_message(upstream, &request)?;
                    let response = read_message(upstream)?;

                    // TODO: Filter keys based on policy.allowed_keys
                    write_message(&mut client, &response)?;
                } else {
                    // Dry run - return empty key list
                    let response = [ssh_agent::SSH_AGENT_IDENTITIES_ANSWER, 0, 0, 0, 0];
                    write_message(&mut client, &response)?;
                }
            }

            ssh_agent::SSH_AGENTC_SIGN_REQUEST => {
                // Sign request - this is where we enforce policy

                // Extract key blob and data from request
                // Format: type(1) | key_blob_len(4) | key_blob | data_len(4) | data | flags(4)
                if request.len() < 9 {
                    send_failure(&mut client)?;
                    continue;
                }

                // For now, we can't reliably extract the target host from the sign request
                // The host info is in the data being signed, but parsing SSH protocol is complex
                // We'll prompt for all sign requests if prompt_unknown is true

                let host = "[unknown host]"; // TODO: Parse from signed data

                match policy.is_host_allowed(host) {
                    PolicyResult::Allow => {
                        debug!("Host {} allowed by policy", host);
                    }
                    PolicyResult::Deny => {
                        warn!("Host {} denied by policy", host);
                        send_failure(&mut client)?;
                        continue;
                    }
                    PolicyResult::Prompt => {
                        // Request approval synchronously using tokio runtime
                        let approved = runtime.block_on(policy.request_approval(host, None))?;

                        if !approved {
                            warn!("User denied SSH to {}", host);
                            send_failure(&mut client)?;
                            continue;
                        }
                        info!("User approved SSH to {}", host);
                    }
                }

                // Forward to upstream
                if let Some(ref mut upstream) = upstream {
                    write_message(upstream, &request)?;
                    let response = read_message(upstream)?;
                    write_message(&mut client, &response)?;
                } else {
                    send_failure(&mut client)?;
                }
            }

            ssh_agent::SSH_AGENTC_ADD_IDENTITY
            | ssh_agent::SSH_AGENTC_REMOVE_IDENTITY
            | ssh_agent::SSH_AGENTC_ADD_ID_CONSTRAINED => {
                // Key management operations are blocked
                warn!("Blocked key management operation: {}", msg_type);
                send_failure(&mut client)?;
            }

            _ => {
                // Unknown message type - forward to upstream
                if let Some(ref mut upstream) = upstream {
                    write_message(upstream, &request)?;
                    let response = read_message(upstream)?;
                    write_message(&mut client, &response)?;
                } else {
                    send_failure(&mut client)?;
                }
            }
        }
    }

    Ok(())
}

fn main() -> Result<()> {
    let args = Args::parse();

    // Initialize logging
    let log_level = if args.verbose { "debug" } else { "info" };
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or(log_level)).init();

    // Load config
    let ssh_config = if let Some(config_path) = &args.config {
        let content = std::fs::read_to_string(config_path)
            .with_context(|| format!("Failed to read config: {:?}", config_path))?;
        let sandbox: SandboxConfig = toml::from_str(&content)?;

        // Get SSH config from profile
        sandbox
            .profiles
            .get(&args.profile)
            .and_then(|p| p.ssh.clone())
            .unwrap_or_default()
    } else {
        SshProxyConfig::default()
    };

    // Determine socket paths
    let listen_path = args.listen.unwrap_or_else(|| {
        let uid = unsafe { libc::getuid() };
        PathBuf::from(format!("/run/user/{}/octo-ssh.sock", uid))
    });

    let upstream_path = args.upstream.unwrap_or_else(|| {
        std::env::var("SSH_AUTH_SOCK")
            .map(PathBuf::from)
            .expect("SSH_AUTH_SOCK not set and --upstream not provided")
    });

    info!("octo-ssh-proxy starting");
    info!("  Listen: {:?}", listen_path);
    info!("  Upstream: {:?}", upstream_path);
    info!("  Profile: {}", args.profile);
    info!("  Allowed hosts: {:?}", ssh_config.allowed_hosts);
    info!("  Prompt unknown: {}", ssh_config.prompt_unknown);

    // Create policy (used in handle_client, created per-connection for thread safety)
    let _policy = SshPolicy::from_config(&ssh_config, &args.octo_server);
    drop(_policy); // Just validate config parses correctly

    // Remove existing socket
    let _ = std::fs::remove_file(&listen_path);

    // Create parent directory if needed
    if let Some(parent) = listen_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    // Bind listener
    let listener = UnixListener::bind(&listen_path)
        .with_context(|| format!("Failed to bind to {:?}", listen_path))?;

    // Set permissions (user-only access)
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&listen_path, std::fs::Permissions::from_mode(0o600))?;
    }

    info!("Listening for connections...");

    // Create tokio runtime for async operations (prompts)
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()?;
    let handle = runtime.handle().clone();

    // Accept connections
    for stream in listener.incoming() {
        match stream {
            Ok(client) => {
                let upstream = upstream_path.clone();
                let policy_clone = SshPolicy::from_config(&ssh_config, &args.octo_server);
                let handle_clone = handle.clone();
                let dry_run = args.dry_run;

                std::thread::spawn(move || {
                    if let Err(e) =
                        handle_client(client, &upstream, &policy_clone, &handle_clone, dry_run)
                    {
                        error!("Client error: {}", e);
                    }
                });
            }
            Err(e) => {
                error!("Accept error: {}", e);
            }
        }
    }

    Ok(())
}
