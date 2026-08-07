//! oqto-ssh-proxy - SSH agent proxy with policy enforcement.
//!
//! This proxy sits between sandboxed agents and the real SSH agent,
//! enforcing policies like allowed hosts and key restrictions.
//!
//! ## Usage
//!
//! ```bash
//! # Start the proxy
//! oqto-ssh-proxy --listen /run/user/1000/oqto-ssh.sock \
//!                --upstream $SSH_AUTH_SOCK \
//!                --oqto-server http://localhost:8080
//!
//! # With config
//! oqto-ssh-proxy --config ~/.config/oqto/sandbox.toml
//! ```
//!
//! ## How it works
//!
//! 1. Agent (inside sandbox) connects to proxy socket
//! 2. Proxy receives SSH agent protocol requests
//! 3. For sign requests, proxy extracts target host from SSH protocol
//! 4. Proxy checks policy (allowed_hosts, allowed_keys)
//! 5. If policy requires prompt, sends request to oqto server
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
use base64::Engine as _;
use clap::Parser;
use glob::Pattern;
use log::{debug, error, info, warn};
use oqto_sandbox::{SandboxConfig, SshProxyConfig};
use rustix::process::getuid;
use sha2::{Digest, Sha256};
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
    name = "oqto-ssh-proxy",
    about = "SSH agent proxy with policy enforcement",
    after_help = "Examples:\n  \
        oqto-ssh-proxy --listen /tmp/oqto-ssh.sock\n  \
        oqto-ssh-proxy --config ~/.config/oqto/sandbox.toml"
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

    /// Oqto server URL for prompts (default: http://localhost:8080)
    #[arg(long, default_value = "http://localhost:8080")]
    oqto_server: String,

    /// Profile name to use from config
    #[arg(short, long, default_value = "development")]
    profile: String,

    /// Enable verbose logging
    #[arg(short, long)]
    verbose: bool,

    /// Dry run (don't actually connect to upstream)
    #[arg(long)]
    dry_run: bool,

    /// Grant use of a key. Accepts a key path, a `SHA256:` fingerprint, a
    /// comment substring, `ca:<path|fingerprint>` for any certificate signed by
    /// that CA, or `principal:<name>`. Repeatable. Overrides `allowed_keys`
    /// from config when present.
    #[arg(long = "allow-key")]
    allow_key: Vec<String>,

    /// Never prompt: keys outside the grant are refused outright.
    #[arg(long)]
    no_prompt: bool,
}

/// SSH agent wire helpers: length-prefixed fields inside a message body.
fn read_u32(buf: &[u8], at: usize) -> Option<u32> {
    let end = at.checked_add(4)?;
    let bytes: [u8; 4] = buf.get(at..end)?.try_into().ok()?;
    Some(u32::from_be_bytes(bytes))
}

/// Read a `u32`-length-prefixed field, returning it and the offset past it.
fn read_field(buf: &[u8], at: usize) -> Option<(&[u8], usize)> {
    let len = read_u32(buf, at)? as usize;
    let start = at.checked_add(4)?;
    let end = start.checked_add(len)?;
    Some((buf.get(start..end)?, end))
}

fn write_field(out: &mut Vec<u8>, field: &[u8]) {
    out.extend_from_slice(&(field.len() as u32).to_be_bytes());
    out.extend_from_slice(field);
}

/// Resolve a grant written as a key path to its fingerprint.
///
/// Key comments rarely match the filename people think in (`~/.ssh/forgejo`
/// commonly carries a `user@host` comment), and the agent protocol exposes
/// only the comment. The proxy runs outside the sandbox, so it can read the
/// public key and translate the path into the fingerprint the wire uses.
/// Returns `None` when the grant is not a readable key path.
fn fingerprint_from_key_path(grant: &str) -> Option<String> {
    let expanded = if let Some(rest) = grant.strip_prefix("~/") {
        PathBuf::from(std::env::var("HOME").ok()?).join(rest)
    } else if grant.starts_with('/') {
        PathBuf::from(grant)
    } else {
        return None;
    };

    let public_key = if expanded.extension().is_some_and(|ext| ext == "pub") {
        expanded
    } else {
        expanded.with_extension("pub")
    };

    let contents = std::fs::read_to_string(&public_key).ok()?;
    let encoded = contents.split_whitespace().nth(1)?;
    let blob = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .ok()?;
    Some(key_fingerprint(&blob))
}

/// OpenSSH-style key fingerprint (`SHA256:` + unpadded base64 of the digest).
fn key_fingerprint(blob: &[u8]) -> String {
    let digest = Sha256::digest(blob);
    format!(
        "SHA256:{}",
        base64::engine::general_purpose::STANDARD_NO_PAD.encode(digest)
    )
}

/// What an OpenSSH certificate says about the key it wraps.
///
/// A certificate is a distinct blob from the key inside it, so its own digest
/// changes every time the CA issues a new one. Grants must therefore name
/// something stable: the key being certified, the CA that signed it, or a
/// principal it carries.
struct CertificateIdentity {
    /// Fingerprint of the certified public key (what `ssh-keygen -l` reports).
    key_fingerprint: Option<String>,
    /// Fingerprint of the signing CA's public key.
    ca_fingerprint: Option<String>,
    key_id: String,
    principals: Vec<String>,
    valid_before: u64,
}

fn now_seconds() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0)
}

fn read_u64(buf: &[u8], at: usize) -> Option<u64> {
    let end = at.checked_add(8)?;
    let bytes: [u8; 8] = buf.get(at..end)?.try_into().ok()?;
    Some(u64::from_be_bytes(bytes))
}

/// Rebuild the plain public key blob embedded in a certificate.
///
/// The certified key's fields sit inline after the nonce, in the same order and
/// encoding a plain key blob uses, so the plain blob is the base algorithm name
/// followed by those fields verbatim.
fn certified_key_blob(base_algorithm: &str, body: &[u8], after_nonce: usize) -> Option<Vec<u8>> {
    let field_count = match base_algorithm {
        "ssh-ed25519" => 1,                           // public key
        "ssh-rsa" | "ssh-dss" => 2,                   // e, n
        name if name.starts_with("ecdsa-sha2-") => 2, // curve, point
        _ => return None,
    };

    let mut blob = Vec::new();
    write_field(&mut blob, base_algorithm.as_bytes());

    let mut offset = after_nonce;
    for _ in 0..field_count {
        let (field, next) = read_field(body, offset)?;
        write_field(&mut blob, field);
        offset = next;
    }

    Some(blob)
}

/// Parse an OpenSSH certificate blob. Returns `None` for plain keys.
fn parse_certificate(blob: &[u8]) -> Option<CertificateIdentity> {
    let (algorithm, offset) = read_field(blob, 0)?;
    let algorithm = std::str::from_utf8(algorithm).ok()?;
    let base_algorithm = algorithm.strip_suffix("-cert-v01@openssh.com")?;

    let (_nonce, offset) = read_field(blob, offset)?;
    let certified_fingerprint =
        certified_key_blob(base_algorithm, blob, offset).map(|blob| key_fingerprint(&blob));

    // Skip the certified key's own fields to reach the certificate body.
    let mut cursor = offset;
    let field_count = match base_algorithm {
        "ssh-ed25519" => 1,
        "ssh-rsa" | "ssh-dss" => 2,
        name if name.starts_with("ecdsa-sha2-") => 2,
        _ => return None,
    };
    for _ in 0..field_count {
        let (_, next) = read_field(blob, cursor)?;
        cursor = next;
    }

    let _serial = read_u64(blob, cursor)?;
    let cursor = cursor + 8;
    let _cert_type = read_u32(blob, cursor)?;
    let cursor = cursor + 4;

    let (key_id, cursor) = read_field(blob, cursor)?;
    let (principals_blob, cursor) = read_field(blob, cursor)?;

    let mut principals = Vec::new();
    let mut principal_offset = 0;
    while principal_offset < principals_blob.len() {
        let (principal, next) = read_field(principals_blob, principal_offset)?;
        principals.push(String::from_utf8_lossy(principal).into_owned());
        principal_offset = next;
    }

    let _valid_after = read_u64(blob, cursor)?;
    let valid_before = read_u64(blob, cursor + 8)?;
    let cursor = cursor + 16;

    let (_critical_options, cursor) = read_field(blob, cursor)?;
    let (_extensions, cursor) = read_field(blob, cursor)?;
    let (_reserved, cursor) = read_field(blob, cursor)?;
    let (signature_key, _) = read_field(blob, cursor)?;

    Some(CertificateIdentity {
        key_fingerprint: certified_fingerprint,
        ca_fingerprint: Some(key_fingerprint(signature_key)),
        key_id: String::from_utf8_lossy(key_id).into_owned(),
        principals,
        valid_before,
    })
}

/// Policy for SSH connections.
struct SshPolicy {
    /// Allowed host patterns (glob)
    allowed_hosts: Vec<Pattern>,
    /// Allowed key comments/fingerprints
    allowed_keys: Vec<String>,
    /// Prompt for unknown hosts
    prompt_unknown: bool,
    /// Oqto server URL for prompts
    oqto_server: String,
}

impl SshPolicy {
    fn from_config(config: &SshProxyConfig, oqto_server: &str) -> Self {
        let allowed_hosts = config
            .allowed_hosts
            .iter()
            .filter_map(|h| Pattern::new(h).ok())
            .collect();

        Self {
            allowed_hosts,
            allowed_keys: config.allowed_keys.clone(),
            prompt_unknown: config.prompt_unknown,
            oqto_server: oqto_server.to_string(),
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

    /// Check whether an identity may be used.
    ///
    /// Grants match a plain key by fingerprint or comment substring. For
    /// certificates they may instead name the certified key, the signing CA
    /// (`ca:<fingerprint>`), or a principal (`principal:<name>`), none of which
    /// change when the CA issues a fresh certificate.
    ///
    /// An empty grant list means no key restriction (host policy still applies).
    fn is_identity_allowed(&self, key_comment: &str, blob: &[u8]) -> bool {
        if self.allowed_keys.is_empty() {
            return true;
        }

        let fingerprint = key_fingerprint(blob);
        let certificate = parse_certificate(blob);

        if let Some(certificate) = certificate.as_ref()
            && certificate.valid_before < now_seconds()
        {
            warn!(
                "Certificate '{}' expired at {}; refusing it",
                certificate.key_id, certificate.valid_before
            );
            return false;
        }

        self.allowed_keys.iter().any(|grant| {
            if let Some(ca) = grant.strip_prefix("ca:") {
                return certificate
                    .as_ref()
                    .and_then(|certificate| certificate.ca_fingerprint.as_deref())
                    .is_some_and(|ca_fingerprint| ca_fingerprint == ca);
            }
            if let Some(principal) = grant.strip_prefix("principal:") {
                return certificate.as_ref().is_some_and(|certificate| {
                    certificate.principals.iter().any(|p| p == principal)
                });
            }

            if fingerprint == *grant || key_comment.contains(grant) {
                return true;
            }

            certificate.as_ref().is_some_and(|certificate| {
                certificate.key_fingerprint.as_deref() == Some(grant.as_str())
                    || certificate.key_id.contains(grant)
            })
        })
    }

    /// Drop identities the workspace was not granted from an
    /// `SSH_AGENT_IDENTITIES_ANSWER`, so the client never offers a key it
    /// cannot use. Returns the rewritten message.
    fn filter_identities(&self, response: &[u8]) -> Result<Vec<u8>> {
        if self.allowed_keys.is_empty() {
            return Ok(response.to_vec());
        }
        if response.first() != Some(&ssh_agent::SSH_AGENT_IDENTITIES_ANSWER) {
            return Ok(response.to_vec());
        }

        let count = read_u32(response, 1).context("identities answer truncated")?;
        let mut offset = 5;
        let mut kept: Vec<u8> = Vec::new();
        let mut kept_count = 0u32;

        for _ in 0..count {
            let (blob, next) = read_field(response, offset).context("identity blob truncated")?;
            let (comment, next) =
                read_field(response, next).context("identity comment truncated")?;
            offset = next;

            let comment = String::from_utf8_lossy(comment);
            if self.is_identity_allowed(&comment, blob) {
                write_field(&mut kept, blob);
                write_field(&mut kept, comment.as_bytes());
                kept_count += 1;
            } else {
                debug!(
                    "Withholding ungranted key '{}' ({})",
                    comment,
                    key_fingerprint(blob)
                );
            }
        }

        let mut out = Vec::with_capacity(kept.len() + 5);
        out.push(ssh_agent::SSH_AGENT_IDENTITIES_ANSWER);
        out.extend_from_slice(&kept_count.to_be_bytes());
        out.extend_from_slice(&kept);
        Ok(out)
    }

    /// Request approval from oqto server.
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

        info!("Requesting approval for SSH to {} from oqto server", host);

        let response = client
            .post(format!("{}/internal/prompt", self.oqto_server))
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

/// Ask upstream for its identities and resolve the grant list to concrete
/// fingerprints.
///
/// A sign request names only the key blob, so comment-based grants
/// (`allowed_keys = ["forgejo"]`) must be resolved to fingerprints before any
/// signature can be authorised.
fn resolve_granted_fingerprints(
    upstream: &mut UnixStream,
    policy: &SshPolicy,
) -> Result<std::collections::HashSet<String>> {
    let mut granted = std::collections::HashSet::new();
    if policy.allowed_keys.is_empty() {
        return Ok(granted);
    }

    write_message(upstream, &[ssh_agent::SSH_AGENTC_REQUEST_IDENTITIES])?;
    let response = read_message(upstream)?;
    if response.first() != Some(&ssh_agent::SSH_AGENT_IDENTITIES_ANSWER) {
        return Ok(granted);
    }

    let count = read_u32(&response, 1).context("identities answer truncated")?;
    let mut offset = 5;
    for _ in 0..count {
        let (blob, next) = read_field(&response, offset).context("identity blob truncated")?;
        let (comment, next) = read_field(&response, next).context("identity comment truncated")?;
        offset = next;

        let comment = String::from_utf8_lossy(comment);
        if policy.is_identity_allowed(&comment, blob) {
            // Keyed on the blob's own digest: a sign request names the
            // certificate, not the key inside it.
            let fingerprint = key_fingerprint(blob);
            debug!("Granted identity '{}' ({})", comment, fingerprint);
            granted.insert(fingerprint);
        }
    }

    Ok(granted)
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

    let granted = match upstream.as_mut() {
        Some(upstream) => resolve_granted_fingerprints(upstream, policy)?,
        None => std::collections::HashSet::new(),
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
                // List keys - forward to upstream, then withhold ungranted keys
                if let Some(ref mut upstream) = upstream {
                    write_message(upstream, &request)?;
                    let response = read_message(upstream)?;
                    let filtered = policy.filter_identities(&response)?;
                    write_message(&mut client, &filtered)?;
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

                // The sign request names the key, not the destination: SSH puts
                // the session id and username in the signed blob, never the
                // hostname. Key identity is therefore the enforceable grant
                // here; per-destination scoping needs the connection path.
                let (key_blob, _) = match read_field(&request, 1) {
                    Some(parsed) => parsed,
                    None => {
                        warn!("Malformed sign request: truncated key blob");
                        send_failure(&mut client)?;
                        continue;
                    }
                };
                let fingerprint = key_fingerprint(key_blob);

                if !policy.allowed_keys.is_empty() && !granted.contains(&fingerprint) {
                    warn!("Refusing sign request for ungranted key {}", fingerprint);
                    send_failure(&mut client)?;
                    continue;
                }
                debug!("Signing with granted key {}", fingerprint);

                let host = "[unknown host]";

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
    let mut ssh_config = if let Some(config_path) = &args.config {
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

    // CLI grants override config: the runner passes the workspace's grant set.
    if !args.allow_key.is_empty() {
        ssh_config.allowed_keys = args.allow_key.clone();
    }
    // Translate path-shaped grants into fingerprints while ~/.ssh is readable.
    ssh_config.allowed_keys = ssh_config
        .allowed_keys
        .iter()
        .map(|grant| {
            let (prefix, path) = match grant.strip_prefix("ca:") {
                Some(path) => ("ca:", path),
                None => ("", grant.as_str()),
            };
            match fingerprint_from_key_path(path) {
                Some(fingerprint) => {
                    let resolved = format!("{}{}", prefix, fingerprint);
                    info!("Grant '{}' resolved to {}", grant, resolved);
                    resolved
                }
                None => grant.clone(),
            }
        })
        .collect();
    if args.no_prompt {
        ssh_config.prompt_unknown = false;
    }

    // Determine socket paths
    let listen_path = args.listen.unwrap_or_else(|| {
        let uid = getuid().as_raw();
        PathBuf::from(format!("/run/user/{}/oqto-ssh.sock", uid))
    });

    let upstream_path = if let Some(upstream) = args.upstream {
        upstream
    } else {
        std::env::var("SSH_AUTH_SOCK")
            .map(PathBuf::from)
            .context("SSH_AUTH_SOCK not set and --upstream not provided")?
    };

    info!("oqto-ssh-proxy starting");
    info!("  Listen: {:?}", listen_path);
    info!("  Upstream: {:?}", upstream_path);
    info!("  Profile: {}", args.profile);
    info!("  Allowed hosts: {:?}", ssh_config.allowed_hosts);
    info!("  Granted keys: {:?}", ssh_config.allowed_keys);
    info!("  Prompt unknown: {}", ssh_config.prompt_unknown);

    // Create policy (used in handle_client, created per-connection for thread safety)
    let _policy = SshPolicy::from_config(&ssh_config, &args.oqto_server);
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
                let policy_clone = SshPolicy::from_config(&ssh_config, &args.oqto_server);
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

#[cfg(test)]
mod tests {
    use super::*;

    fn identities_answer(entries: &[(&[u8], &str)]) -> Vec<u8> {
        let mut out = vec![ssh_agent::SSH_AGENT_IDENTITIES_ANSWER];
        out.extend_from_slice(&(entries.len() as u32).to_be_bytes());
        for (blob, comment) in entries {
            write_field(&mut out, blob);
            write_field(&mut out, comment.as_bytes());
        }
        out
    }

    fn policy(allowed_keys: &[&str]) -> SshPolicy {
        SshPolicy {
            allowed_hosts: vec![],
            allowed_keys: allowed_keys.iter().map(|k| k.to_string()).collect(),
            prompt_unknown: false,
            oqto_server: String::new(),
        }
    }

    #[test]
    fn identities_outside_the_grant_are_withheld() {
        let response =
            identities_answer(&[(b"blob-forgejo", "forgejo"), (b"blob-github", "github")]);

        let filtered = policy(&["forgejo"]).filter_identities(&response).unwrap();

        let count = read_u32(&filtered, 1).unwrap();
        assert_eq!(count, 1, "only the granted key should be offered");
        let (blob, next) = read_field(&filtered, 5).unwrap();
        assert_eq!(blob, b"blob-forgejo");
        let (comment, _) = read_field(&filtered, next).unwrap();
        assert_eq!(comment, b"forgejo");
    }

    #[test]
    fn an_empty_grant_list_withholds_nothing() {
        let response = identities_answer(&[(b"blob-a", "a"), (b"blob-b", "b")]);

        let filtered = policy(&[]).filter_identities(&response).unwrap();

        assert_eq!(filtered, response);
    }

    #[test]
    fn grants_match_by_fingerprint_as_well_as_comment() {
        let fingerprint = key_fingerprint(b"blob-forgejo");
        let response = identities_answer(&[(b"blob-forgejo", "unrelated-comment")]);

        let filtered = policy(&[&fingerprint])
            .filter_identities(&response)
            .unwrap();

        assert_eq!(read_u32(&filtered, 1).unwrap(), 1);
    }

    #[test]
    fn fingerprints_are_openssh_formatted() {
        // ssh-keygen prints unpadded base64 of the SHA256 digest.
        let fingerprint = key_fingerprint(b"some-key-blob");
        assert!(fingerprint.starts_with("SHA256:"));
        assert!(!fingerprint.ends_with('='));
    }

    /// Real artifacts from `ssh-keygen`: an ed25519 key, a CA, and a
    /// certificate for that key signed by that CA (principals forgejo-ro, git).
    fn decode(name: &str) -> Vec<u8> {
        let encoded = match name {
            "cert" => include_str!("testdata/cert.b64"),
            "user" => include_str!("testdata/user.b64"),
            "ca" => include_str!("testdata/ca.b64"),
            "cert-expired" => include_str!("testdata/cert-expired.b64"),
            other => panic!("unknown fixture {other}"),
        };
        base64::engine::general_purpose::STANDARD
            .decode(encoded.trim())
            .expect("fixture decodes")
    }

    #[test]
    fn a_certificate_reports_the_key_and_ca_it_binds() {
        let certificate = parse_certificate(&decode("cert")).expect("parses as a certificate");

        assert_eq!(
            certificate.key_fingerprint.as_deref(),
            Some(key_fingerprint(&decode("user")).as_str()),
            "the certified key is what ssh-keygen -l reports"
        );
        assert_eq!(
            certificate.ca_fingerprint.as_deref(),
            Some(key_fingerprint(&decode("ca")).as_str())
        );
        assert_eq!(certificate.key_id, "tommy@oqto");
        assert_eq!(certificate.principals, vec!["forgejo-ro", "git"]);
    }

    #[test]
    fn plain_keys_are_not_certificates() {
        assert!(parse_certificate(&decode("user")).is_none());
    }

    #[test]
    fn a_certificate_is_granted_by_its_ca() {
        let ca_grant = format!("ca:{}", key_fingerprint(&decode("ca")));

        assert!(policy(&[&ca_grant]).is_identity_allowed("user-key", &decode("cert")));
        // The CA grant must not leak to a plain key that no CA vouched for.
        assert!(!policy(&[&ca_grant]).is_identity_allowed("user-key", &decode("user")));
    }

    #[test]
    fn a_certificate_is_granted_by_principal() {
        assert!(policy(&["principal:forgejo-ro"]).is_identity_allowed("user-key", &decode("cert")));
        assert!(
            !policy(&["principal:production"]).is_identity_allowed("user-key", &decode("cert"))
        );
    }

    #[test]
    fn a_certificate_is_granted_by_the_key_it_certifies() {
        // Grants written against the key survive certificate reissue, whose
        // own digest changes every time.
        let key_grant = key_fingerprint(&decode("user"));

        assert!(policy(&[&key_grant]).is_identity_allowed("user-key", &decode("cert")));
    }

    #[test]
    fn an_unrelated_grant_does_not_match_a_certificate() {
        let unrelated = key_fingerprint(b"some-other-key");

        assert!(!policy(&[&unrelated]).is_identity_allowed("", &decode("cert")));
    }

    #[test]
    fn an_expired_certificate_is_refused_even_when_its_ca_is_granted() {
        let ca_grant = format!("ca:{}", key_fingerprint(&decode("ca")));

        assert!(!policy(&[&ca_grant]).is_identity_allowed("user-key", &decode("cert-expired")));
    }
}
