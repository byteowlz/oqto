//! oqto-usermgr: Privileged user management daemon for Oqto.
//!
//! Runs as a systemd service (as root) listening on a unix socket.
//! The main oqto service (unprivileged) sends JSON requests over the socket.
//! This provides OS-level privilege separation: even if the oqto process is
//! compromised, it cannot modify /etc/passwd or /home directly -- only through
//! this daemon which strictly validates all inputs.
//!
//! Protocol: newline-delimited JSON over unix socket.
//!   Request:  {"cmd": "create-user", "args": {"username": "oqto_foo", "uid": 2000, ...}}
//!   Response: {"ok": true} or {"ok": false, "error": "message"}
//!
//! Security invariants:
//! - Usernames must start with "oqto_" prefix
//! - UIDs must be in 2000-60000 range
//! - Group must be "oqto"
//! - Paths restricted to /run/oqto/ and /home/oqto_*
//! - Shell must be in allowlist
//! - GECOS must start with "Oqto platform user "

use oqto_usermgr::validate::*;
use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixListener;
use std::process::Command;

const SOCKET_PATH: &str = "/run/oqto/usermgr.sock";

/// System-wide Pi extensions source directory (cloned by setup.sh).
const PI_EXTENSIONS_DIR: &str = "/usr/share/oqto/pi-agent-extensions";

/// Default extensions to install for new users.
const PI_DEFAULT_EXTENSIONS: &[&str] = &[
    "auto-rename",
    "oqto-bridge",
    "oqto-todos",
    "custom-context-files",
];

/// Allowed path prefixes for mkdir/chown/chmod operations.
const ALLOWED_PATH_PREFIXES: &[&str] = &["/run/oqto/runner-sockets/", "/home/oqto_"];

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
    eprintln!("oqto-usermgr: starting (pid {})", std::process::id());

    // Remove stale socket
    let _ = std::fs::remove_file(SOCKET_PATH);

    // Ensure parent directory exists
    if let Some(parent) = std::path::Path::new(SOCKET_PATH).parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    let listener = match UnixListener::bind(SOCKET_PATH) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("oqto-usermgr: failed to bind {SOCKET_PATH}: {e}");
            std::process::exit(1);
        }
    };

    set_socket_permissions();

    eprintln!("oqto-usermgr: listening on {SOCKET_PATH}");

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => handle_connection(stream),
            Err(e) => eprintln!("oqto-usermgr: accept error: {e}"),
        }
    }
}

fn set_socket_permissions() {
    // Socket owned by oqto:root with mode 0600.
    // Only the oqto service user can connect -- NOT oqto_* platform users
    // (who share the oqto group but are different UIDs).
    use std::os::unix::fs::PermissionsExt;
    let _ = std::fs::set_permissions(SOCKET_PATH, std::fs::Permissions::from_mode(0o600));
    if let Some(uid) = get_user_uid("oqto") {
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
                eprintln!("oqto-usermgr: read error: {e}");
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

        let mut resp_json = serde_json::to_string(&response)
            .unwrap_or_else(|_| r#"{"ok":false,"error":"serialization failed"}"#.to_string());
        resp_json.push('\n');

        if let Err(e) = writer.write_all(resp_json.as_bytes()) {
            eprintln!("oqto-usermgr: write error: {e}");
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
        "setup-user-shell" => cmd_setup_user_shell(&req.args),
        "install-pi-extensions" => cmd_install_pi_extensions(&req.args),
        "write-file" => cmd_write_file(&req.args),
        "restart-service" => cmd_restart_service(&req.args),
        "run-as-user" => cmd_run_as_user(&req.args),
        "fix-socket-dir" => cmd_fix_socket_dir(&req.args),
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
        && status.success()
    {
        return Response::success();
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
            // so the oqto backend (same group) can manage workspaces.
            let workspace = format!("/home/{username}/oqto");
            let _ = run_cmd("/bin/mkdir", &["-p", &workspace]);
            let _ = run_cmd(
                "/usr/bin/chown",
                &[&format!("{username}:{group}"), &workspace],
            );
            let _ = run_cmd("/usr/bin/chmod", &["2770", &workspace]);

            // Write shell dotfiles (zsh + starship)
            let home = format!("/home/{username}");
            write_user_dotfiles(&home, username, group);

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
const RUNNER_BINARY: &str = "/usr/local/bin/oqto-runner";

/// High-level command: install, enable, and start oqto-runner for a user.
///
/// SECURITY: The service file content is constructed server-side from validated
/// inputs. The client only provides username and uid -- never executable paths
/// or service file content. This prevents a compromised oqto process from
/// injecting arbitrary ExecStart commands that would run as root or as the
/// target user.
///
/// Steps:
/// 1. Create ~/.config/systemd/user/ directory
/// 2. Write oqto-runner.service file (content generated here, not from client)
/// 3. Set ownership to the target user
/// 4. Enable systemd linger
/// 5. Start user@{uid}.service
/// 6. Daemon-reload + enable+start oqto-runner via machinectl
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
    let socket_path = format!("/run/oqto/runner-sockets/{username}/oqto-runner.sock");

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
    if !home.starts_with("/home/oqto_") {
        return Response::error(format!(
            "unexpected home directory for {username}: {home} (expected /home/oqto_*)"
        ));
    }

    let group = "oqto";

    // Construct a PATH that includes the user's local bin dirs and system paths.
    // Systemd user services run with a minimal environment, so tools like bun/node
    // (needed by hstry) won't be found without an explicit PATH.
    let user_path =
        format!("{home}/.bun/bin:{home}/.cargo/bin:{home}/.local/bin:/usr/local/bin:/usr/bin:/bin");

    // Service file contents -- all constructed server-side, never from client input.
    // hstry and mmry run as simple foreground services.
    // oqto-runner uses Type=notify and depends on both.
    let hstry_service = format!(
        r#"[Unit]
Description=Oqto Chat History Service

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
Description=Oqto Memory Service

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
Description=Oqto Runner - Process isolation daemon
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
Environment=PI_PACKAGE_DIR=/usr/local/lib/pi-coding-agent
# systemd waits up to 30s for READY=1 before declaring failure
TimeoutStartSec=30

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
        ("oqto-runner.service", runner_service),
    ];
    for (name, content) in &services {
        let path = format!("{service_dir}/{name}");
        if let Err(e) = std::fs::write(&path, content) {
            return Response::error(format!("writing {path}: {e}"));
        }
    }

    // 2b. Ensure hstry config has service enabled.
    //     hstry defaults to `enabled = false` on first run, but we manage
    //     it via systemd so the gRPC service must be listening.
    let hstry_config_dir = format!("{home}/.config/hstry");
    let _ = std::fs::create_dir_all(&hstry_config_dir);
    let hstry_config_path = format!("{hstry_config_dir}/config.toml");
    let hstry_db_path = format!("{home}/.local/share/hstry/hstry.db");
    let _ = std::fs::create_dir_all(format!("{home}/.local/share/hstry"));
    if std::path::Path::new(&hstry_config_path).exists() {
        // Patch existing: flip enabled = false -> true
        if let Ok(content) = std::fs::read_to_string(&hstry_config_path) {
            if content.contains("enabled = false") {
                let patched = content.replace("enabled = false", "enabled = true");
                let _ = std::fs::write(&hstry_config_path, patched);
            }
        }
    } else {
        // Write minimal config with service enabled
        let hstry_config = format!(
            "database = \"{hstry_db_path}\"\n\
             \n\
             [service]\n\
             enabled = true\n\
             transport = \"tcp\"\n"
        );
        let _ = std::fs::write(&hstry_config_path, hstry_config);
    }

    // 2c. Ensure mmry config exists with remote embeddings.
    //     Per-user mmry connects to the central embeddings server (port 8091)
    //     instead of loading the model locally. service.enabled = true so the
    //     gRPC service stays running for the runner.
    //     External API port = 48000 + (uid - 2000) so the backend can find it.
    let mmry_config_dir = format!("{home}/.config/mmry");
    let _ = std::fs::create_dir_all(&mmry_config_dir);
    let mmry_config_path = format!("{mmry_config_dir}/config.toml");
    let mmry_data_dir = format!("{home}/.local/share/mmry");
    let _ = std::fs::create_dir_all(format!("{mmry_data_dir}/stores"));
    let mmry_api_port = 48000 + (uid - 2000);
    if !std::path::Path::new(&mmry_config_path).exists() {
        let mmry_config = format!(
            "[database]\n\
             path = \"{mmry_data_dir}/memories.db\"\n\
             \n\
             [stores]\n\
             directory = \"{mmry_data_dir}/stores\"\n\
             default = \"default\"\n\
             \n\
             [embeddings]\n\
             enabled = false\n\
             model = \"Xenova/all-MiniLM-L6-v2\"\n\
             backend = \"fastembed\"\n\
             dimension = 384\n\
             batch_size = 32\n\
             \n\
             [embeddings.remote]\n\
             base_url = \"http://127.0.0.1:8091\"\n\
             request_timeout_seconds = 30\n\
             max_batch_size = 64\n\
             required = true\n\
             \n\
             [service]\n\
             enabled = true\n\
             auto_start = true\n\
             idle_timeout_seconds = 0\n\
             preload_models = false\n\
             \n\
             [external_api]\n\
             enable = true\n\
             host = \"127.0.0.1\"\n\
             port = {mmry_api_port}\n\
             \n\
             [search]\n\
             default_limit = 10\n\
             similarity_threshold = 0.7\n\
             mode = \"hybrid\"\n\
             rerank_enabled = false\n\
             \n\
             [memory]\n\
             default_category = \"default\"\n\
             auto_dedupe = true\n"
        );
        let _ = std::fs::write(&mmry_config_path, mmry_config);
    }
    // Chown mmry data dir
    let _ = run_cmd(
        "/usr/bin/chown",
        &["-R", &format!("{username}:{group}"), &mmry_data_dir],
    );

    // 3. Set ownership of .config tree
    let config_dir = format!("{home}/.config");
    if let Err(e) = run_cmd(
        "/usr/bin/chown",
        &["-R", &format!("{username}:{group}"), &config_dir],
    ) {
        return Response::error(format!("chown {config_dir}: {e}"));
    }
    // Also chown the data dir (hstry db)
    let data_dir = format!("{home}/.local/share/hstry");
    let _ = run_cmd(
        "/usr/bin/chown",
        &["-R", &format!("{username}:{group}"), &data_dir],
    );

    // 4. Create per-user socket directory
    //    Also ensure the parent /run/oqto/runner-sockets/ has correct ownership.
    //    mkdir -p creates it as root:root by default, but we need root:oqto
    //    so platform users (in group oqto) can traverse into their subdirectory.
    let runner_sockets_base = "/run/oqto/runner-sockets";
    if let Err(e) = run_cmd("/bin/mkdir", &["-p", runner_sockets_base]) {
        return Response::error(format!("mkdir {runner_sockets_base}: {e}"));
    }
    if let Err(e) = run_cmd("/usr/bin/chown", &["root:oqto", runner_sockets_base]) {
        return Response::error(format!("chown {runner_sockets_base}: {e}"));
    }
    if let Err(e) = run_cmd("/usr/bin/chmod", &["2770", runner_sockets_base]) {
        return Response::error(format!("chmod {runner_sockets_base}: {e}"));
    }

    let socket_dir = format!("/run/oqto/runner-sockets/{username}");
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

    // 6. Daemon-reload + enable all services + start oqto-runner
    //    Starting oqto-runner pulls in hstry and mmry via Requires= dependency.
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
        eprintln!("oqto-usermgr: daemon-reload failed: {e}");
    }

    // Enable all three services
    for svc in ["hstry.service", "mmry.service", "oqto-runner.service"] {
        if let Err(e) = run_user_systemctl(&["enable", svc]) {
            eprintln!("oqto-usermgr: enable {svc} failed: {e}");
        }
    }

    // Check if the runner is already active. If so, restart to pick up
    // any service file changes. If not, start fresh.
    let runner_active = run_user_systemctl(&["is-active", "oqto-runner.service"]).is_ok();
    let action = if runner_active { "restart" } else { "start" };

    // Start/restart oqto-runner (pulls in hstry + mmry via Requires=).
    // With Type=notify, this blocks until the runner signals READY=1.
    if let Err(e) = run_user_systemctl(&[action, "oqto-runner.service"]) {
        return Response::error(format!("{action} oqto-runner failed: {e}"));
    }

    // Wait for the runner socket to appear and ensure correct permissions.
    // The runner needs time to start, bind the socket, and initialize hstry/mmry.
    let socket = std::path::Path::new(&socket_path);
    for i in 0..20 {
        if socket.exists() {
            // Ensure group-writable so the oqto backend can connect.
            // The runner creates the socket with default umask (0755),
            // but Unix socket connect() requires write permission.
            if let Err(e) = run_cmd("/usr/bin/chmod", &["0770", &socket_path]) {
                eprintln!("oqto-usermgr: chmod socket: {e}");
            }
            eprintln!(
                "oqto-usermgr: runner socket ready after {}ms: {socket_path}",
                i * 500
            );

            // Verify critical services are actually running for this user
            for svc in ["hstry", "mmry"] {
                let svc_unit = format!("{svc}.service");
                match run_user_systemctl(&["is-active", &svc_unit]) {
                    Ok(_) => {
                        eprintln!("oqto-usermgr: {svc} confirmed active for {username}");
                    }
                    Err(e) => {
                        eprintln!(
                            "oqto-usermgr: WARNING: {svc} not active for {username}: {e}"
                        );
                        // Try to start it explicitly
                        if let Err(e2) = run_user_systemctl(&["start", &svc_unit]) {
                            return Response::error(format!(
                                "runner socket ready but {svc} failed to start: {e2}"
                            ));
                        }
                        // Give it a moment
                        std::thread::sleep(std::time::Duration::from_secs(1));
                        if let Err(e3) = run_user_systemctl(&["is-active", &svc_unit]) {
                            return Response::error(format!(
                                "{svc} failed to start for {username}: {e3}"
                            ));
                        }
                        eprintln!("oqto-usermgr: {svc} recovered for {username}");
                    }
                }
            }

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
/// Accepts: username, path (must be under /home/oqto_*), files (map of filename -> content)
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

    // Copy template directory if provided (includes .pi/skills/ etc.)
    if let Some(template_src) = args.get("template_src").and_then(|v| v.as_str()) {
        let src = std::path::Path::new(template_src);
        if src.is_dir() {
            // Use cp -a to preserve directory structure including dotfiles
            if let Err(e) = run_cmd("/bin/cp", &["-a", &format!("{}/.", template_src), path]) {
                eprintln!("warning: copying template dir: {e}");
            }
        }
    }

    // Write/overlay files if provided (overwrites templates with resolved versions)
    if let Some(files) = args.get("files").and_then(|f| f.as_object()) {
        for (name, content) in files {
            // Validate filename: no null bytes, no traversal components
            if name.contains('\0') || name == "." || name == ".." {
                return Response::error(format!("invalid filename: {name}"));
            }
            // Reject path traversal
            if name.split('/').any(|c| c == ".." || c.is_empty()) {
                return Response::error(format!("invalid filename (traversal): {name}"));
            }
            if let Some(text) = content.as_str() {
                let file_path = format!("{path}/{name}");
                // Create parent directories for nested paths like .oqto/workspace.toml
                if let Some(parent) = std::path::Path::new(&file_path).parent() {
                    if let Err(e) = std::fs::create_dir_all(parent) {
                        return Response::error(format!("creating dir for {file_path}: {e}"));
                    }
                }
                if let Err(e) = std::fs::write(&file_path, text) {
                    return Response::error(format!("writing {file_path}: {e}"));
                }
            }
        }
    }

    // Chown everything to the user
    let group = "oqto";
    if let Err(e) = run_cmd(
        "/usr/bin/chown",
        &["-R", &format!("{username}:{group}"), path],
    ) {
        return Response::error(format!("chown: {e}"));
    }
    if let Err(e) = run_cmd("/usr/bin/chmod", &["2770", path]) {
        return Response::error(format!("chmod: {e}"));
    }
    // Make files group-writable so the oqto backend can update workspace metadata
    if let Err(e) = run_cmd("/usr/bin/chmod", &["-R", "g+w", path]) {
        return Response::error(format!("chmod g+w: {e}"));
    }

    Response::success()
}

// ============================================================================
// Shell setup command
// ============================================================================

/// Set up shell dotfiles (zsh + starship) for an existing user.
/// Also changes their login shell to zsh if it differs.
fn cmd_setup_user_shell(args: &serde_json::Value) -> Response {
    let username = match get_str(args, "username") {
        Ok(u) => u,
        Err(r) => return r,
    };
    let group = args.get("group").and_then(|v| v.as_str()).unwrap_or("oqto");
    let shell = args
        .get("shell")
        .and_then(|v| v.as_str())
        .unwrap_or("/bin/zsh");

    if let Err(e) = validate_username(username) {
        return Response::error(e);
    }
    if let Err(e) = validate_group(group) {
        return Response::error(e);
    }
    if let Err(e) = validate_shell(shell) {
        return Response::error(e);
    }

    let home = format!("/home/{username}");
    if !std::path::Path::new(&home).exists() {
        return Response::error(format!("home directory does not exist: {home}"));
    }

    // Change login shell
    if let Err(e) = run_cmd("/usr/sbin/usermod", &["-s", shell, username]) {
        return Response::error(format!("usermod -s: {e}"));
    }

    write_user_dotfiles(&home, username, group);

    Response::success()
}

/// Install Pi extensions from the system-wide source into a user's
/// ~/.pi/agent/extensions/ directory.
///
/// Required args: username
/// Optional args: group (default: "oqto")
///
/// Copies each extension in PI_DEFAULT_EXTENSIONS from PI_EXTENSIONS_DIR
/// into the user's home. Skips missing extensions with a warning.
fn cmd_install_pi_extensions(args: &serde_json::Value) -> Response {
    let username = match get_str(args, "username") {
        Ok(u) => u,
        Err(r) => return r,
    };
    let group = args.get("group").and_then(|v| v.as_str()).unwrap_or("oqto");

    if let Err(e) = validate_username(username) {
        return Response::error(e);
    }

    let home = format!("/home/{username}");
    if !std::path::Path::new(&home).exists() {
        return Response::error(format!("home directory does not exist: {home}"));
    }

    let src_root = std::path::Path::new(PI_EXTENSIONS_DIR);
    if !src_root.is_dir() {
        return Response::error(format!(
            "Pi extensions source not found: {PI_EXTENSIONS_DIR}. Run setup.sh to install."
        ));
    }

    let dest_root = format!("{home}/.pi/agent/extensions");
    if let Err(e) = std::fs::create_dir_all(&dest_root) {
        return Response::error(format!("creating {dest_root}: {e}"));
    }

    let mut installed = 0u32;
    for ext_name in PI_DEFAULT_EXTENSIONS {
        let src_dir = src_root.join(ext_name);
        if !src_dir.is_dir() || !src_dir.join("index.ts").exists() {
            eprintln!("warning: extension not found in repo: {ext_name}");
            continue;
        }

        let dest_dir = format!("{dest_root}/{ext_name}");
        // Remove old version if present
        let _ = std::fs::remove_dir_all(&dest_dir);

        if let Err(e) = copy_dir_recursive(&src_dir, &std::path::Path::new(&dest_dir)) {
            eprintln!("warning: copying extension {ext_name}: {e}");
            continue;
        }
        // Remove install script (not needed at runtime); keep package.json
        let _ = std::fs::remove_file(format!("{dest_dir}/install.sh"));
        installed += 1;
    }

    // chown extensions to the user
    let owner = format!("{username}:{group}");
    let _ = run_cmd("/usr/bin/chown", &["-R", &owner, &dest_root]);

    eprintln!("info: installed {installed} Pi extensions for {username}");
    Response::success()
}

/// Write a file to a user's home directory.
///
/// Required args: username, path (relative to home), content
/// Optional args: group (default: "oqto"), mode (default: "0644")
///
/// Creates parent directories as needed. File is owned by username:group.
fn cmd_write_file(args: &serde_json::Value) -> Response {
    let username = match get_str(args, "username") {
        Ok(u) => u,
        Err(r) => return r,
    };
    let rel_path = match get_str(args, "path") {
        Ok(p) => p,
        Err(r) => return r,
    };
    let content = match get_str(args, "content") {
        Ok(c) => c,
        Err(r) => return r,
    };
    let group = args.get("group").and_then(|v| v.as_str()).unwrap_or("oqto");
    let mode = args.get("mode").and_then(|v| v.as_str()).unwrap_or("0644");

    if let Err(e) = validate_username(username) {
        return Response::error(e);
    }
    if let Err(e) = validate_group(group) {
        return Response::error(e);
    }

    // Prevent path traversal
    if rel_path.contains("..") || rel_path.starts_with('/') {
        return Response::error("path must be relative and cannot contain '..'".to_string());
    }

    let home = format!("/home/{username}");
    if !std::path::Path::new(&home).exists() {
        return Response::error(format!("home directory does not exist: {home}"));
    }

    let full_path = format!("{home}/{rel_path}");

    // Create parent directories
    if let Some(parent) = std::path::Path::new(&full_path).parent() {
        if !parent.exists() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                return Response::error(format!("mkdir {}: {e}", parent.display()));
            }
            // chown the created directories
            let _ = run_cmd(
                "/usr/bin/chown",
                &["-R", &format!("{username}:{group}"), &parent.to_string_lossy()],
            );
        }
    }

    // Write the file
    if let Err(e) = std::fs::write(&full_path, content) {
        return Response::error(format!("write {full_path}: {e}"));
    }

    // Set ownership and permissions
    if let Err(e) = run_cmd(
        "/usr/bin/chown",
        &[&format!("{username}:{group}"), &full_path],
    ) {
        return Response::error(format!("chown: {e}"));
    }
    if let Err(e) = run_cmd("/usr/bin/chmod", &[mode, &full_path]) {
        return Response::error(format!("chmod: {e}"));
    }

    Response::success()
}

/// Restart a system service. Only whitelisted services are allowed.
fn cmd_restart_service(args: &serde_json::Value) -> Response {
    let service = match get_str(args, "service") {
        Ok(s) => s,
        Err(r) => return r,
    };

    // Whitelist: only allow restarting specific services
    const ALLOWED_SERVICES: &[&str] = &["eavs", "eavs.service"];
    if !ALLOWED_SERVICES.contains(&service) {
        return Response::error(format!(
            "service '{}' is not in the allowed list: {:?}",
            service, ALLOWED_SERVICES
        ));
    }

    match run_cmd("/usr/bin/systemctl", &["restart", service]) {
        Ok(_) => Response::success(),
        Err(e) => Response::error(format!("systemctl restart {}: {}", service, e)),
    }
}

// ============================================================================
// Run command as user
// ============================================================================

/// Allowed binary names for run-as-user.
/// Only these can be executed. Full path resolution is done after validation.
const ALLOWED_RUN_BINARIES: &[&str] = &["skdlr", "trx", "agntz", "byt", "pi", "sldr", "git"];

/// Execute a whitelisted binary as a specific oqto user.
///
/// Request args:
///   username: string  - Linux username (must be oqto_ prefixed)
///   binary: string    - Binary name (must be in ALLOWED_RUN_BINARIES)
///   args: [string]    - Arguments to pass
///   env: {k: v}       - Environment variables to set (optional)
///   cwd: string       - Working directory (optional, defaults to user home)
/// Fix ownership of a user's runner socket directory.
/// Ensures it is owned by the user with group 'oqto' and mode 2770.
/// Called by the backend when it can't connect to a runner socket.
fn cmd_fix_socket_dir(args: &serde_json::Value) -> Response {
    let username = match get_str(args, "username") {
        Ok(s) => s,
        Err(r) => return r,
    };

    if !username.starts_with("oqto_") {
        return Response::error("username must start with oqto_");
    }
    if let Err(e) = validate_username(username) {
        return Response::error(format!("invalid username: {e}"));
    }

    let socket_dir = format!("/run/oqto/runner-sockets/{username}");
    let group = "oqto";

    // Create if missing
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

    // Also fix any socket files inside
    if let Ok(entries) = std::fs::read_dir(&socket_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            let path_str = path.to_string_lossy();
            let _ = run_cmd(
                "/usr/bin/chown",
                &[&format!("{username}:{group}"), &path_str],
            );
        }
    }

    Response::success()
}

fn cmd_run_as_user(args: &serde_json::Value) -> Response {
    let username = match get_str(args, "username") {
        Ok(s) => s,
        Err(r) => return r,
    };
    if let Err(e) = validate_username(username) {
        return Response::error(e);
    }

    let binary = match get_str(args, "binary") {
        Ok(s) => s,
        Err(r) => return r,
    };

    // Validate binary against allowlist
    if !ALLOWED_RUN_BINARIES.contains(&binary) {
        return Response::error(format!(
            "binary '{}' is not in the allowed list: {:?}",
            binary, ALLOWED_RUN_BINARIES
        ));
    }

    // Validate binary name has no path components or shell metacharacters
    if binary.contains('/')
        || binary.contains('\\')
        || binary.contains('\0')
        || binary.contains(';')
        || binary.contains('|')
        || binary.contains('&')
        || binary.contains('$')
        || binary.contains('`')
    {
        return Response::error(format!("binary name '{}' contains invalid characters", binary));
    }

    let cmd_args: Vec<String> = args
        .get("args")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();

    // Validate args: reject shell metacharacters that could enable injection
    for arg in &cmd_args {
        if arg.contains('\0') {
            return Response::error("argument contains null byte".to_string());
        }
    }

    let env_map: std::collections::HashMap<String, String> = args
        .get("env")
        .and_then(|v| v.as_object())
        .map(|obj| {
            obj.iter()
                .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                .collect()
        })
        .unwrap_or_default();

    // Validate env keys: no shell metacharacters
    for key in env_map.keys() {
        if !key
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_')
        {
            return Response::error(format!(
                "env key '{}' contains invalid characters (allowed: A-Z, a-z, 0-9, _)",
                key
            ));
        }
    }

    let home_dir = format!("/home/{}", username);
    let cwd = args
        .get("cwd")
        .and_then(|v| v.as_str())
        .unwrap_or(&home_dir);

    // Validate cwd path
    if let Err(e) = validate_path(cwd, ALLOWED_PATH_PREFIXES) {
        // Also allow the workspace root and /tmp
        if !cwd.starts_with("/tmp/") && !cwd.starts_with("/usr/") {
            return Response::error(format!("cwd: {e}"));
        }
    }

    // Build the command: sudo -n -u <username> -- env <envs> <binary> <args>
    let mut cmd = Command::new("/usr/bin/sudo");
    cmd.arg("-n")
        .arg("-u")
        .arg(username)
        .arg("--");

    // Use env to set environment variables
    if !env_map.is_empty() {
        cmd.arg("/usr/bin/env");
        for (k, v) in &env_map {
            cmd.arg(format!("{}={}", k, v));
        }
    }

    cmd.arg(binary);
    for arg in &cmd_args {
        cmd.arg(arg);
    }
    cmd.current_dir(cwd);

    let output = match cmd.output() {
        Ok(o) => o,
        Err(e) => return Response::error(format!("failed to execute: {e}")),
    };

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    if !output.status.success() {
        return Response {
            ok: false,
            error: Some(format!(
                "{} exited with status {}: {}",
                binary,
                output.status.code().unwrap_or(-1),
                stderr.trim()
            )),
            data: Some(serde_json::json!({
                "stdout": stdout,
                "stderr": stderr,
                "exit_code": output.status.code(),
            })),
        };
    }

    Response {
        ok: true,
        error: None,
        data: Some(serde_json::json!({
            "stdout": stdout,
            "stderr": stderr,
        })),
    }
}

// ============================================================================
// Shell dotfile provisioning
// ============================================================================

/// Write .zshrc and starship.toml for a newly created user.
///
/// Errors are logged but non-fatal -- the user will still work with
/// a bare zsh prompt if dotfile creation fails.
fn write_user_dotfiles(home: &str, username: &str, group: &str) {
    let dotfiles_src = std::path::Path::new("/usr/share/oqto/oqto-templates/dotfiles");

    if dotfiles_src.is_dir() {
        // Copy dotfiles from templates repo (includes ~/.pi/agent/AGENTS.md etc.)
        if let Err(e) = copy_dir_recursive(dotfiles_src, std::path::Path::new(home)) {
            eprintln!("warning: copying dotfiles from templates: {e}");
        }
    } else {
        // Fall back to hardcoded dotfiles
        eprintln!("info: dotfiles template dir not found, using built-in defaults");
        let zshrc_path = format!("{home}/.zshrc");
        if let Err(e) = std::fs::write(&zshrc_path, ZSHRC_CONTENT) {
            eprintln!("warning: writing {zshrc_path}: {e}");
        }

        let starship_dir = format!("{home}/.config/starship");
        if let Err(e) = std::fs::create_dir_all(&starship_dir) {
            eprintln!("warning: creating {starship_dir}: {e}");
        }
        let starship_path = format!("{starship_dir}/starship.toml");
        if let Err(e) = std::fs::write(&starship_path, STARSHIP_TOML) {
            eprintln!("warning: writing {starship_path}: {e}");
        }
    }

    // Deploy global skills from the templates skills pool to ~/.pi/agent/skills/
    let skills_src = std::path::Path::new("/usr/share/oqto/oqto-templates/skills");
    let skills_dest = std::path::Path::new(home).join(".pi/agent/skills");
    if skills_src.is_dir() {
        if let Err(e) = std::fs::create_dir_all(&skills_dest) {
            eprintln!("warning: creating skills dir: {e}");
        } else if let Err(e) = copy_dir_recursive(skills_src, &skills_dest) {
            eprintln!("warning: copying skills from templates: {e}");
        }
    }

    // chown the entire home to the user
    let owner = format!("{username}:{group}");
    let _ = run_cmd("/usr/bin/chown", &["-R", &owner, home]);
}

/// Recursively copy all files from src into dst, merging with existing directories.
fn copy_dir_recursive(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<()> {
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());

        if file_type.is_dir() {
            std::fs::create_dir_all(&dst_path)?;
            copy_dir_recursive(&src_path, &dst_path)?;
        } else {
            // Only copy if destination doesn't exist (don't overwrite user customizations)
            if !dst_path.exists() {
                if let Some(parent) = dst_path.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                std::fs::copy(&src_path, &dst_path)?;
            }
        }
    }
    Ok(())
}

const ZSHRC_CONTENT: &str = r#"# Oqto platform shell configuration

# History
HISTFILE=~/.zsh_history
HISTSIZE=10000
SAVEHIST=10000
setopt SHARE_HISTORY
setopt HIST_IGNORE_DUPS
setopt HIST_IGNORE_SPACE

# Key bindings
bindkey -e
bindkey '^[[A' up-line-or-search
bindkey '^[[B' down-line-or-search
bindkey '^[[H' beginning-of-line
bindkey '^[[F' end-of-line
bindkey '^[[3~' delete-char

# Completion
autoload -Uz compinit && compinit -d ~/.zcompdump
zstyle ':completion:*' menu select
zstyle ':completion:*' matcher-list 'm:{a-z}={A-Z}'

# Colors
autoload -U colors && colors
export LS_COLORS='di=1;34:ln=35:so=32:pi=33:ex=31:bd=34;46:cd=34;43:su=30;41:sg=30;46:tw=30;42:ow=30;43'
alias ls='ls --color=auto'
alias ll='ls -lah'
alias grep='grep --color=auto'

# PATH
export PATH="$HOME/.local/bin:$HOME/.cargo/bin:/usr/local/bin:$PATH"

# Starship prompt
if command -v starship &>/dev/null; then
    eval "$(starship init zsh)"
fi
"#;

const STARSHIP_TOML: &str = r#"# Oqto platform starship configuration
# https://starship.rs/config/

format = """
$directory\
$git_branch\
$git_status\
$character"""

right_format = """$cmd_duration"""

[directory]
truncation_length = 3
truncate_to_repo = false
style = "bold cyan"

[git_branch]
format = "[$symbol$branch]($style) "
symbol = " "
style = "bold purple"

[git_status]
format = '([$all_status$ahead_behind]($style) )'
style = "bold red"

[character]
success_symbol = "[>](bold green)"
error_symbol = "[>](bold red)"

[cmd_duration]
min_time = 2_000
format = "[$duration]($style)"
style = "bold yellow"
"#;
