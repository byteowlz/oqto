//! octo-usermgr: Privileged user management daemon for Octo.
//!
//! Runs as a systemd service (as root) listening on a unix socket.
//! The main octo service (unprivileged) sends JSON requests over the socket.
//! This provides OS-level privilege separation: even if the octo process is
//! compromised, it cannot modify /etc/passwd or /home directly -- only through
//! this daemon which strictly validates all inputs.
//!
//! Protocol: newline-delimited JSON over unix socket.
//!   Request:  {"cmd": "create-user", "args": {"username": "octo_foo", "uid": 2000, ...}}
//!   Response: {"ok": true} or {"ok": false, "error": "message"}
//!
//! Security invariants:
//! - Usernames must start with "octo_" prefix
//! - UIDs must be in 2000-60000 range
//! - Group must be "octo"
//! - Paths restricted to /run/octo/ and /home/octo_*
//! - Shell must be in allowlist
//! - GECOS must start with "Octo platform user "

use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixListener;
use std::process::Command;

const SOCKET_PATH: &str = "/run/octo/usermgr.sock";
const DEFAULT_PREFIX: &str = "octo_";
const DEFAULT_GROUP: &str = "octo";
const UID_MIN: u32 = 2000;
const UID_MAX: u32 = 60000;

// --- Protocol types ---

#[derive(Deserialize)]
struct Request {
    cmd: String,
    #[serde(default)]
    args: serde_json::Value,
}

#[derive(Serialize)]
struct Response {
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<serde_json::Value>,
}

impl Response {
    fn success() -> Self {
        Self {
            ok: true,
            error: None,
            data: None,
        }
    }

    fn error(msg: impl Into<String>) -> Self {
        Self {
            ok: false,
            error: Some(msg.into()),
            data: None,
        }
    }
}

fn main() {
    eprintln!("octo-usermgr: starting (pid {})", std::process::id());

    // Remove stale socket
    let _ = std::fs::remove_file(SOCKET_PATH);

    // Ensure parent directory exists
    if let Some(parent) = std::path::Path::new(SOCKET_PATH).parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    let listener = match UnixListener::bind(SOCKET_PATH) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("octo-usermgr: failed to bind {SOCKET_PATH}: {e}");
            std::process::exit(1);
        }
    };

    // Set socket permissions: only octo group can connect
    set_socket_permissions();

    eprintln!("octo-usermgr: listening on {SOCKET_PATH}");

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => handle_connection(stream),
            Err(e) => eprintln!("octo-usermgr: accept error: {e}"),
        }
    }
}

fn set_socket_permissions() {
    // Socket owned by octo:root with mode 0600.
    // Only the octo service user can connect -- NOT octo_* platform users
    // (who share the octo group but are different UIDs).
    use std::os::unix::fs::PermissionsExt;
    let _ = std::fs::set_permissions(SOCKET_PATH, std::fs::Permissions::from_mode(0o600));
    if let Some(uid) = get_user_uid("octo") {
        let _ = run_cmd("/usr/bin/chown", &[&format!("{uid}:0"), SOCKET_PATH]);
    }
}

fn get_user_uid(name: &str) -> Option<u32> {
    let output = Command::new("/usr/bin/id")
        .args(["-u", name])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8_lossy(&output.stdout).trim().parse().ok()
}

fn handle_connection(stream: std::os::unix::net::UnixStream) {
    let reader = BufReader::new(&stream);
    let mut writer = &stream;

    for line in reader.lines() {
        let line = match line {
            Ok(l) => l,
            Err(e) => {
                eprintln!("octo-usermgr: read error: {e}");
                return;
            }
        };

        if line.trim().is_empty() {
            continue;
        }

        let response = match serde_json::from_str::<Request>(&line) {
            Ok(req) => dispatch(&req),
            Err(e) => Response::error(format!("invalid request: {e}")),
        };

        let mut resp_json = serde_json::to_string(&response).unwrap_or_else(|_| {
            r#"{"ok":false,"error":"serialization failed"}"#.to_string()
        });
        resp_json.push('\n');

        if let Err(e) = writer.write_all(resp_json.as_bytes()) {
            eprintln!("octo-usermgr: write error: {e}");
            return;
        }
        let _ = writer.flush();
    }
}

fn dispatch(req: &Request) -> Response {
    match req.cmd.as_str() {
        "create-group" => cmd_create_group(&req.args),
        "create-user" => cmd_create_user(&req.args),
        "delete-user" => cmd_delete_user(&req.args),
        "mkdir" => cmd_mkdir(&req.args),
        "chown" => cmd_chown(&req.args),
        "chmod" => cmd_chmod(&req.args),
        "enable-linger" => cmd_enable_linger(&req.args),
        "start-user-service" => cmd_start_user_service(&req.args),
        "ping" => Response::success(),
        other => Response::error(format!("unknown command: {other}")),
    }
}

// --- Validation helpers ---

fn validate_username(name: &str) -> Result<(), String> {
    if !name.starts_with(DEFAULT_PREFIX) {
        return Err(format!("username must start with '{DEFAULT_PREFIX}' prefix"));
    }
    if name.len() > 32 {
        return Err("username too long (max 32)".into());
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
    {
        return Err("username contains invalid characters".into());
    }
    Ok(())
}

fn validate_group(group: &str) -> Result<(), String> {
    if group != DEFAULT_GROUP {
        return Err(format!("group must be '{DEFAULT_GROUP}'"));
    }
    Ok(())
}

fn validate_uid(uid: u32) -> Result<(), String> {
    if uid < UID_MIN || uid > UID_MAX {
        return Err(format!("UID {uid} out of allowed range ({UID_MIN}-{UID_MAX})"));
    }
    Ok(())
}

fn validate_shell(shell: &str) -> Result<(), String> {
    match shell {
        "/bin/bash" | "/bin/sh" | "/usr/bin/bash" | "/usr/bin/sh" | "/bin/false"
        | "/usr/sbin/nologin" => Ok(()),
        _ => Err(format!("shell '{shell}' not in allowlist")),
    }
}

fn validate_path(path: &str, allowed_prefixes: &[&str]) -> Result<(), String> {
    if path.contains("..") {
        return Err("path contains '..' (path traversal)".into());
    }
    if path.contains("//") {
        return Err("path contains '//'".into());
    }
    if !path.starts_with('/') {
        return Err("path must be absolute".into());
    }
    for prefix in allowed_prefixes {
        if path.starts_with(prefix) {
            return Ok(());
        }
    }
    Err(format!("path '{path}' not in allowed directories"))
}

fn validate_gecos(gecos: &str) -> Result<(), String> {
    if !gecos.starts_with("Octo platform user ") {
        return Err("GECOS must start with 'Octo platform user '".into());
    }
    if gecos
        .chars()
        .any(|c| matches!(c, '\n' | '\r' | ':' | '\0'))
    {
        return Err("GECOS contains invalid characters".into());
    }
    if gecos.len() > 256 {
        return Err("GECOS too long".into());
    }
    Ok(())
}

fn run_cmd(cmd: &str, args: &[&str]) -> Result<String, String> {
    let output = Command::new(cmd)
        .args(args)
        .output()
        .map_err(|e| format!("failed to execute {cmd}: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "{cmd} failed (exit {}): {}",
            output.status.code().unwrap_or(-1),
            stderr.trim()
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

// --- Command handlers ---

fn cmd_create_group(args: &serde_json::Value) -> Response {
    let group = match args.get("group").and_then(|v| v.as_str()) {
        Some(g) => g,
        None => return Response::error("missing 'group' argument"),
    };

    if let Err(e) = validate_group(group) {
        return Response::error(e);
    }

    // Check if group already exists
    if let Ok(status) = Command::new("/usr/bin/getent")
        .args(["group", group])
        .status()
    {
        if status.success() {
            return Response::success();
        }
    }

    match run_cmd("/usr/sbin/groupadd", &[group]) {
        Ok(_) => Response::success(),
        Err(e) => Response::error(e),
    }
}

fn cmd_create_user(args: &serde_json::Value) -> Response {
    let username = match args.get("username").and_then(|v| v.as_str()) {
        Some(u) => u,
        None => return Response::error("missing 'username'"),
    };
    let uid = match args.get("uid").and_then(|v| v.as_u64()) {
        Some(u) => u as u32,
        None => return Response::error("missing 'uid'"),
    };
    let group = match args.get("group").and_then(|v| v.as_str()) {
        Some(g) => g,
        None => return Response::error("missing 'group'"),
    };
    let shell = match args.get("shell").and_then(|v| v.as_str()) {
        Some(s) => s,
        None => return Response::error("missing 'shell'"),
    };
    let gecos = match args.get("gecos").and_then(|v| v.as_str()) {
        Some(g) => g,
        None => return Response::error("missing 'gecos'"),
    };
    let create_home = args
        .get("create_home")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);

    if let Err(e) = validate_username(username) {
        return Response::error(e);
    }
    if let Err(e) = validate_uid(uid) {
        return Response::error(e);
    }
    if let Err(e) = validate_group(group) {
        return Response::error(e);
    }
    if let Err(e) = validate_shell(shell) {
        return Response::error(e);
    }
    if let Err(e) = validate_gecos(gecos) {
        return Response::error(e);
    }

    let uid_str = uid.to_string();
    let home_flag = if create_home { "-m" } else { "-M" };

    match run_cmd(
        "/usr/sbin/useradd",
        &[
            "-u", &uid_str, "-g", group, "-s", shell, home_flag, "-c", gecos, username,
        ],
    ) {
        Ok(_) => Response::success(),
        Err(e) => Response::error(e),
    }
}

fn cmd_delete_user(args: &serde_json::Value) -> Response {
    let username = match args.get("username").and_then(|v| v.as_str()) {
        Some(u) => u,
        None => return Response::error("missing 'username'"),
    };

    if let Err(e) = validate_username(username) {
        return Response::error(e);
    }

    match run_cmd("/usr/sbin/userdel", &[username]) {
        Ok(_) => Response::success(),
        Err(e) => Response::error(e),
    }
}

fn cmd_mkdir(args: &serde_json::Value) -> Response {
    let path = match args.get("path").and_then(|v| v.as_str()) {
        Some(p) => p,
        None => return Response::error("missing 'path'"),
    };

    if let Err(e) = validate_path(path, &["/run/octo/runner-sockets/", "/home/octo_"]) {
        return Response::error(e);
    }

    match run_cmd("/bin/mkdir", &["-p", path]) {
        Ok(_) => Response::success(),
        Err(e) => Response::error(e),
    }
}

fn cmd_chown(args: &serde_json::Value) -> Response {
    let owner = match args.get("owner").and_then(|v| v.as_str()) {
        Some(o) => o,
        None => return Response::error("missing 'owner' (format: user:group)"),
    };
    let path = match args.get("path").and_then(|v| v.as_str()) {
        Some(p) => p,
        None => return Response::error("missing 'path'"),
    };
    let recursive = args
        .get("recursive")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    // Validate owner format
    let parts: Vec<&str> = owner.split(':').collect();
    if parts.len() != 2 {
        return Response::error("owner must be in user:group format");
    }
    if let Err(e) = validate_username(parts[0]) {
        return Response::error(e);
    }
    if let Err(e) = validate_group(parts[1]) {
        return Response::error(e);
    }

    if let Err(e) = validate_path(path, &["/run/octo/runner-sockets/", "/home/octo_"]) {
        return Response::error(e);
    }

    let result = if recursive {
        run_cmd("/usr/bin/chown", &["-R", owner, path])
    } else {
        run_cmd("/usr/bin/chown", &[owner, path])
    };

    match result {
        Ok(_) => Response::success(),
        Err(e) => Response::error(e),
    }
}

fn cmd_chmod(args: &serde_json::Value) -> Response {
    let mode = match args.get("mode").and_then(|v| v.as_str()) {
        Some(m) => m,
        None => return Response::error("missing 'mode'"),
    };
    let path = match args.get("path").and_then(|v| v.as_str()) {
        Some(p) => p,
        None => return Response::error("missing 'path'"),
    };

    match mode {
        "700" | "750" | "755" | "770" | "2770" => {}
        _ => return Response::error(format!("mode '{mode}' not in allowlist")),
    }

    if let Err(e) = validate_path(path, &["/run/octo/runner-sockets/", "/home/octo_"]) {
        return Response::error(e);
    }

    match run_cmd("/usr/bin/chmod", &[mode, path]) {
        Ok(_) => Response::success(),
        Err(e) => Response::error(e),
    }
}

fn cmd_enable_linger(args: &serde_json::Value) -> Response {
    let username = match args.get("username").and_then(|v| v.as_str()) {
        Some(u) => u,
        None => return Response::error("missing 'username'"),
    };

    if let Err(e) = validate_username(username) {
        return Response::error(e);
    }

    match run_cmd("/usr/bin/loginctl", &["enable-linger", username]) {
        Ok(_) => Response::success(),
        Err(e) => Response::error(e),
    }
}

fn cmd_start_user_service(args: &serde_json::Value) -> Response {
    let uid = match args.get("uid").and_then(|v| v.as_u64()) {
        Some(u) => u as u32,
        None => return Response::error("missing 'uid'"),
    };

    if let Err(e) = validate_uid(uid) {
        return Response::error(e);
    }

    let service = format!("user@{uid}.service");
    match run_cmd("/usr/bin/systemctl", &["start", &service]) {
        Ok(_) => Response::success(),
        Err(e) => Response::error(e),
    }
}
