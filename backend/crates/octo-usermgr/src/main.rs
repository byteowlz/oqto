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

use octo_usermgr::validate::*;
use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixListener;
use std::process::Command;

const SOCKET_PATH: &str = "/run/octo/usermgr.sock";

/// Allowed path prefixes for mkdir/chown/chmod operations.
const ALLOWED_PATH_PREFIXES: &[&str] = &["/run/octo/runner-sockets/", "/home/octo_"];

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
        "setup-user-runner" => cmd_setup_user_runner(&req.args),
        "create-workspace" => cmd_create_workspace(&req.args),
        "ping" => Response::success(),
        other => Response::error(format!("unknown command: {other}")),
    }
}

// --- Helpers ---

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

fn get_str<'a>(args: &'a serde_json::Value, key: &str) -> Result<&'a str, Response> {
    args.get(key)
        .and_then(|v| v.as_str())
        .ok_or_else(|| Response::error(format!("missing '{key}'")))
}

fn get_u32(args: &serde_json::Value, key: &str) -> Result<u32, Response> {
    args.get(key)
        .and_then(|v| v.as_u64())
        .map(|v| v as u32)
        .ok_or_else(|| Response::error(format!("missing '{key}'")))
}

// --- Command handlers ---

fn cmd_create_group(args: &serde_json::Value) -> Response {
    let group = match get_str(args, "group") {
        Ok(g) => g,
        Err(r) => return r,
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
    let username = match get_str(args, "username") {
        Ok(u) => u,
        Err(r) => return r,
    };
    let uid = match get_u32(args, "uid") {
        Ok(u) => u,
        Err(r) => return r,
    };
    let group = match get_str(args, "group") {
        Ok(g) => g,
        Err(r) => return r,
    };
    let shell = match get_str(args, "shell") {
        Ok(s) => s,
        Err(r) => return r,
    };
    let gecos = match get_str(args, "gecos") {
        Ok(g) => g,
        Err(r) => return r,
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
        Ok(_) => {
            // Create workspace directory inside the user's home with group-write
            // so the octo backend (same group) can manage workspaces.
            let workspace = format!("/home/{username}/octo");
            let _ = run_cmd("/bin/mkdir", &["-p", &workspace]);
            let _ = run_cmd(
                "/usr/bin/chown",
                &[&format!("{username}:{group}"), &workspace],
            );
            let _ = run_cmd("/usr/bin/chmod", &["2770", &workspace]);
            Response::success()
        }
        Err(e) => Response::error(e),
    }
}

fn cmd_delete_user(args: &serde_json::Value) -> Response {
    let username = match get_str(args, "username") {
        Ok(u) => u,
        Err(r) => return r,
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
    let path = match get_str(args, "path") {
        Ok(p) => p,
        Err(r) => return r,
    };

    if let Err(e) = validate_path(path, ALLOWED_PATH_PREFIXES) {
        return Response::error(e);
    }

    match run_cmd("/bin/mkdir", &["-p", path]) {
        Ok(_) => Response::success(),
        Err(e) => Response::error(e),
    }
}

fn cmd_chown(args: &serde_json::Value) -> Response {
    let owner = match get_str(args, "owner") {
        Ok(o) => o,
        Err(r) => return r,
    };
    let path = match get_str(args, "path") {
        Ok(p) => p,
        Err(r) => return r,
    };
    let recursive = args
        .get("recursive")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    if let Err(e) = validate_owner(owner) {
        return Response::error(e);
    }

    if let Err(e) = validate_path(path, ALLOWED_PATH_PREFIXES) {
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
    let mode = match get_str(args, "mode") {
        Ok(m) => m,
        Err(r) => return r,
    };
    let path = match get_str(args, "path") {
        Ok(p) => p,
        Err(r) => return r,
    };

    if let Err(e) = validate_chmod_mode(mode) {
        return Response::error(e);
    }

    if let Err(e) = validate_path(path, ALLOWED_PATH_PREFIXES) {
        return Response::error(e);
    }

    match run_cmd("/usr/bin/chmod", &[mode, path]) {
        Ok(_) => Response::success(),
        Err(e) => Response::error(e),
    }
}

fn cmd_enable_linger(args: &serde_json::Value) -> Response {
    let username = match get_str(args, "username") {
        Ok(u) => u,
        Err(r) => return r,
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
    let uid = match get_u32(args, "uid") {
        Ok(u) => u,
        Err(r) => return r,
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

/// Hardcoded runner binary path -- never trust client-supplied paths for ExecStart.
const RUNNER_BINARY: &str = "/usr/local/bin/octo-runner";

/// High-level command: install, enable, and start octo-runner for a user.
///
/// SECURITY: The service file content is constructed server-side from validated
/// inputs. The client only provides username and uid -- never executable paths
/// or service file content. This prevents a compromised octo process from
/// injecting arbitrary ExecStart commands that would run as root or as the
/// target user.
///
/// Steps:
/// 1. Create ~/.config/systemd/user/ directory
/// 2. Write octo-runner.service file (content generated here, not from client)
/// 3. Set ownership to the target user
/// 4. Enable systemd linger
/// 5. Start user@{uid}.service
/// 6. Daemon-reload + enable+start octo-runner via machinectl
fn cmd_setup_user_runner(args: &serde_json::Value) -> Response {
    let username = match get_str(args, "username") {
        Ok(u) => u,
        Err(r) => return r,
    };
    let uid = match get_u32(args, "uid") {
        Ok(u) => u,
        Err(r) => return r,
    };

    if let Err(e) = validate_username(username) {
        return Response::error(e);
    }
    if let Err(e) = validate_uid(uid) {
        return Response::error(e);
    }

    // Verify the runner binary exists at the hardcoded path
    if !std::path::Path::new(RUNNER_BINARY).exists() {
        return Response::error(format!("runner binary not found: {RUNNER_BINARY}"));
    }

    // Derive the socket path from the validated username (never from client input)
    let socket_path = format!("/run/octo/runner-sockets/{username}/octo-runner.sock");

    // Construct service file content server-side
    // Service file contents are constructed after home dir is resolved (below),
    // since we need the home path for the Environment=PATH directive.

    // Get user home directory from passwd (not from client)
    let home = match run_cmd("/usr/bin/getent", &["passwd", username]) {
        Ok(output) => {
            let fields: Vec<&str> = output.trim().split(':').collect();
            if fields.len() < 6 {
                return Response::error(format!("cannot parse home dir for {username}"));
            }
            fields[5].to_string()
        }
        Err(e) => return Response::error(format!("cannot find user {username}: {e}")),
    };

    // Validate the home directory is under the expected prefix
    if !home.starts_with("/home/octo_") {
        return Response::error(format!(
            "unexpected home directory for {username}: {home} (expected /home/octo_*)"
        ));
    }

    let group = "octo";

    // Construct a PATH that includes the user's local bin dirs and system paths.
    // Systemd user services run with a minimal environment, so tools like bun/node
    // (needed by hstry) won't be found without an explicit PATH.
    let user_path = format!(
        "{home}/.bun/bin:{home}/.cargo/bin:{home}/.local/bin:/usr/local/bin:/usr/bin:/bin"
    );

    // Service file contents -- all constructed server-side, never from client input.
    // hstry and mmry run as simple foreground services.
    // octo-runner uses Type=notify and depends on both.
    let hstry_service = format!(
        r#"[Unit]
Description=Octo Chat History Service
After=default.target

[Service]
Type=simple
ExecStart=/usr/local/bin/hstry service run
Restart=on-failure
RestartSec=3
Environment=PATH={user_path}
Environment=HOME={home}

[Install]
WantedBy=default.target
"#
    );

    let mmry_service = format!(
        r#"[Unit]
Description=Octo Memory Service
After=default.target

[Service]
Type=simple
ExecStart=/usr/local/bin/mmry-service
Restart=on-failure
RestartSec=3
Environment=PATH={user_path}
Environment=HOME={home}

[Install]
WantedBy=default.target
"#
    );

    let runner_service = format!(
        r#"[Unit]
Description=Octo Runner - Process isolation daemon
Requires=hstry.service mmry.service
After=hstry.service mmry.service

[Service]
Type=notify
ExecStart={RUNNER_BINARY} --socket {socket_path}
Restart=on-failure
RestartSec=5
Environment=RUST_LOG=info
Environment=PATH={user_path}
Environment=HOME={home}
# systemd waits up to 30s for READY=1 from the runner
WatchdogSec=30

[Install]
WantedBy=default.target
"#
    );

    let service_dir = format!("{home}/.config/systemd/user");

    // 1. Create service directory
    if let Err(e) = run_cmd("/bin/mkdir", &["-p", &service_dir]) {
        return Response::error(format!("mkdir {service_dir}: {e}"));
    }

    // 2. Write all service files
    let services = [
        ("hstry.service", hstry_service),
        ("mmry.service", mmry_service),
        ("octo-runner.service", runner_service),
    ];
    for (name, content) in &services {
        let path = format!("{service_dir}/{name}");
        if let Err(e) = std::fs::write(&path, content) {
            return Response::error(format!("writing {path}: {e}"));
        }
    }

    // 3. Set ownership of .config tree
    let config_dir = format!("{home}/.config");
    if let Err(e) = run_cmd(
        "/usr/bin/chown",
        &["-R", &format!("{username}:{group}"), &config_dir],
    ) {
        return Response::error(format!("chown {config_dir}: {e}"));
    }

    // 4. Create per-user socket directory
    let socket_dir = format!("/run/octo/runner-sockets/{username}");
    if let Err(e) = run_cmd("/bin/mkdir", &["-p", &socket_dir]) {
        return Response::error(format!("mkdir {socket_dir}: {e}"));
    }
    if let Err(e) = run_cmd(
        "/usr/bin/chown",
        &[&format!("{username}:{group}"), &socket_dir],
    ) {
        return Response::error(format!("chown {socket_dir}: {e}"));
    }
    if let Err(e) = run_cmd("/usr/bin/chmod", &["2770", &socket_dir]) {
        return Response::error(format!("chmod {socket_dir}: {e}"));
    }

    // 5. Enable linger
    if let Err(e) = run_cmd("/usr/bin/loginctl", &["enable-linger", username]) {
        return Response::error(format!("enable-linger: {e}"));
    }

    // 5. Start user systemd instance
    let user_service = format!("user@{uid}.service");
    if let Err(e) = run_cmd("/usr/bin/systemctl", &["start", &user_service]) {
        return Response::error(format!("start {user_service}: {e}"));
    }

    // Give the user's systemd instance time to initialize
    std::thread::sleep(std::time::Duration::from_millis(500));

    // 6. Daemon-reload + enable all services + start octo-runner
    //    Starting octo-runner pulls in hstry and mmry via Requires= dependency.
    //    Type=notify on runner means `systemctl start` blocks until READY=1.
    let machine_arg = format!("{username}@.host");
    let runtime_dir = format!("/run/user/{uid}");
    let bus = format!("unix:path={runtime_dir}/bus");

    // Helper: run systemctl as the target user. Try --machine first, fall back to runuser.
    let run_user_systemctl = |args: &[&str]| -> Result<String, String> {
        // Try --machine method first
        let mut machine_args = vec!["--machine", &machine_arg, "--user"];
        machine_args.extend_from_slice(args);
        if let Ok(output) = run_cmd("/usr/bin/systemctl", &machine_args) {
            return Ok(output);
        }

        // Fallback: runuser
        let mut cmd = Command::new("/sbin/runuser");
        cmd.args(["-u", username, "--"])
            .arg("env")
            .arg(format!("XDG_RUNTIME_DIR={runtime_dir}"))
            .arg(format!("DBUS_SESSION_BUS_ADDRESS={bus}"))
            .arg("systemctl")
            .arg("--user");
        for arg in args {
            cmd.arg(arg);
        }
        let output = cmd.output().map_err(|e| format!("runuser: {e}"))?;
        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).to_string())
        } else {
            Err(format!(
                "exit {}: {}",
                output.status.code().unwrap_or(-1),
                String::from_utf8_lossy(&output.stderr).trim()
            ))
        }
    };

    // Always daemon-reload so updated service files take effect.
    if let Err(e) = run_user_systemctl(&["daemon-reload"]) {
        eprintln!("octo-usermgr: daemon-reload failed: {e}");
    }

    // Enable all three services
    for svc in ["hstry.service", "mmry.service", "octo-runner.service"] {
        if let Err(e) = run_user_systemctl(&["enable", svc]) {
            eprintln!("octo-usermgr: enable {svc} failed: {e}");
        }
    }

    // Check if the runner is already active. If so, restart to pick up
    // any service file changes. If not, start fresh.
    let runner_active = run_user_systemctl(&["is-active", "octo-runner.service"]).is_ok();
    let action = if runner_active { "restart" } else { "start" };

    // Start/restart octo-runner (pulls in hstry + mmry via Requires=).
    // With Type=notify, this blocks until the runner signals READY=1.
    if let Err(e) = run_user_systemctl(&[action, "octo-runner.service"]) {
        return Response::error(format!("{action} octo-runner failed: {e}"));
    }

    // Wait for the runner socket to appear and ensure correct permissions.
    // The runner needs time to start, bind the socket, and initialize hstry/mmry.
    let socket = std::path::Path::new(&socket_path);
    for i in 0..20 {
        if socket.exists() {
            // Ensure group-writable so the octo backend can connect.
            // The runner creates the socket with default umask (0755),
            // but Unix socket connect() requires write permission.
            if let Err(e) = run_cmd("/usr/bin/chmod", &["0770", &socket_path]) {
                eprintln!("octo-usermgr: chmod socket: {e}");
            }
            eprintln!(
                "octo-usermgr: runner socket ready after {}ms: {socket_path}",
                i * 500
            );
            return Response::success();
        }
        std::thread::sleep(std::time::Duration::from_millis(500));
    }

    Response::error(format!(
        "runner socket did not appear at {socket_path} after 10s"
    ))
}

/// Create a workspace directory with files, owned by the target user.
///
/// Accepts: username, path (must be under /home/octo_*), files (map of filename -> content)
fn cmd_create_workspace(args: &serde_json::Value) -> Response {
    let username = match get_str(args, "username") {
        Ok(u) => u,
        Err(r) => return r,
    };
    let path = match get_str(args, "path") {
        Ok(p) => p,
        Err(r) => return r,
    };

    if let Err(e) = validate_username(username) {
        return Response::error(e);
    }

    // Path must be under the user's home
    if !path.starts_with(&format!("/home/{username}/")) {
        return Response::error(format!("path must be under /home/{username}/"));
    }

    // No traversal
    if let Err(e) = validate_path(path, &[&format!("/home/{username}/")]) {
        return Response::error(e);
    }

    // Create directory
    if let Err(e) = run_cmd("/bin/mkdir", &["-p", path]) {
        return Response::error(format!("mkdir: {e}"));
    }

    // Write files if provided
    if let Some(files) = args.get("files").and_then(|f| f.as_object()) {
        for (name, content) in files {
            // Sanitize filename: no slashes, no dots-only, no control chars
            if name.contains('/') || name.contains('\0') || name == "." || name == ".." {
                return Response::error(format!("invalid filename: {name}"));
            }
            if let Some(text) = content.as_str() {
                let file_path = format!("{path}/{name}");
                if let Err(e) = std::fs::write(&file_path, text) {
                    return Response::error(format!("writing {file_path}: {e}"));
                }
            }
        }
    }

    // Chown everything to the user
    let group = "octo";
    if let Err(e) = run_cmd("/usr/bin/chown", &["-R", &format!("{username}:{group}"), path]) {
        return Response::error(format!("chown: {e}"));
    }
    if let Err(e) = run_cmd("/usr/bin/chmod", &["2770", path]) {
        return Response::error(format!("chmod: {e}"));
    }

    Response::success()
}
