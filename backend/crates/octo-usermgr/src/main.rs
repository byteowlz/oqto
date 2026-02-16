//! octo-usermgr: Minimal privileged helper for Octo Linux user management.
//!
//! This binary is installed with file capabilities (CAP_SETUID, CAP_SETGID, CAP_DAC_OVERRIDE,
//! CAP_CHOWN, CAP_FOWNER) so the main octo process can manage Linux users without sudo.
//! This allows octo.service to keep NoNewPrivileges=true and ProtectSystem=strict.
//!
//! All inputs are strictly validated:
//! - Usernames must start with a configurable prefix (default "octo_")
//! - UIDs must be in a safe range (default 2000-60000)
//! - Group name is fixed
//! - Only specific operations are allowed
//!
//! Security: This binary should be owned by root and not writable by anyone else.
//! File capabilities are set via: setcap cap_setuid,cap_setgid,cap_dac_override,cap_chown,cap_fowner+ep

use std::env;
use std::process::{Command, ExitCode};

const DEFAULT_PREFIX: &str = "octo_";
const DEFAULT_GROUP: &str = "octo";
const DEFAULT_SHELL: &str = "/bin/bash";
const UID_MIN: u32 = 2000;
const UID_MAX: u32 = 60000;

fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        eprintln!("Usage: octo-usermgr <command> [args...]");
        eprintln!("Commands:");
        eprintln!("  create-group <group>");
        eprintln!("  create-user <username> <uid> <group> <shell> <gecos>");
        eprintln!("  delete-user <username>");
        eprintln!("  mkdir <path>");
        eprintln!("  chown <owner:group> <path>");
        eprintln!("  chmod <mode> <path>");
        eprintln!("  enable-linger <username>");
        eprintln!("  start-user-service <uid>");
        return ExitCode::from(1);
    }

    let result = match args[1].as_str() {
        "create-group" => cmd_create_group(&args[2..]),
        "create-user" => cmd_create_user(&args[2..]),
        "delete-user" => cmd_delete_user(&args[2..]),
        "mkdir" => cmd_mkdir(&args[2..]),
        "chown" => cmd_chown(&args[2..]),
        "chmod" => cmd_chmod(&args[2..]),
        "enable-linger" => cmd_enable_linger(&args[2..]),
        "start-user-service" => cmd_start_user_service(&args[2..]),
        other => {
            eprintln!("Unknown command: {other}");
            Err("unknown command")
        }
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(msg) => {
            eprintln!("Error: {msg}");
            ExitCode::from(1)
        }
    }
}

// --- Validation helpers ---

fn validate_username(name: &str) -> Result<(), &'static str> {
    if !name.starts_with(DEFAULT_PREFIX) {
        return Err("username must start with octo_ prefix");
    }
    if name.len() > 32 {
        return Err("username too long (max 32)");
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
    {
        return Err("username contains invalid characters");
    }
    Ok(())
}

fn validate_group(group: &str) -> Result<(), &'static str> {
    if group != DEFAULT_GROUP {
        return Err("group must be 'octo'");
    }
    Ok(())
}

fn validate_uid(uid_str: &str) -> Result<u32, &'static str> {
    let uid: u32 = uid_str.parse().map_err(|_| "invalid UID")?;
    if uid < UID_MIN || uid > UID_MAX {
        return Err("UID out of allowed range (2000-60000)");
    }
    Ok(uid)
}

fn validate_shell(shell: &str) -> Result<(), &'static str> {
    match shell {
        "/bin/bash" | "/bin/sh" | "/usr/bin/bash" | "/usr/bin/sh" | "/bin/false"
        | "/usr/sbin/nologin" => Ok(()),
        _ => Err("shell not in allowlist"),
    }
}

/// Validate a path is within allowed directories for octo operations.
fn validate_path(path: &str, allowed_prefixes: &[&str]) -> Result<(), &'static str> {
    // Reject path traversal
    if path.contains("..") {
        return Err("path contains '..' (path traversal)");
    }
    // Reject symbolic link tricks via multiple slashes
    if path.contains("//") {
        return Err("path contains '//'");
    }
    // Must be absolute
    if !path.starts_with('/') {
        return Err("path must be absolute");
    }
    // Must match an allowed prefix
    for prefix in allowed_prefixes {
        if path.starts_with(prefix) {
            return Ok(());
        }
    }
    Err("path not in allowed directories")
}

fn validate_gecos(gecos: &str) -> Result<(), &'static str> {
    if !gecos.starts_with("Octo platform user ") {
        return Err("GECOS must start with 'Octo platform user '");
    }
    // Reject shell metacharacters
    if gecos.chars().any(|c| matches!(c, '\n' | '\r' | ':' | '\0')) {
        return Err("GECOS contains invalid characters");
    }
    if gecos.len() > 256 {
        return Err("GECOS too long");
    }
    Ok(())
}

fn run(cmd: &str, args: &[&str]) -> Result<(), &'static str> {
    let output = Command::new(cmd)
        .args(args)
        .output()
        .map_err(|_| "failed to execute command")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        eprintln!("Command failed: {cmd} {args:?}");
        eprintln!("stderr: {}", stderr.trim());
        return Err("command failed");
    }
    Ok(())
}

// --- Commands ---

fn cmd_create_group(args: &[String]) -> Result<(), &'static str> {
    if args.len() != 1 {
        return Err("usage: create-group <group>");
    }
    validate_group(&args[0])?;

    // Check if group already exists
    let status = Command::new("/usr/bin/getent")
        .args(["group", &args[0]])
        .status()
        .map_err(|_| "failed to check group")?;

    if status.success() {
        // Already exists
        return Ok(());
    }

    run("/usr/sbin/groupadd", &[&args[0]])
}

fn cmd_create_user(args: &[String]) -> Result<(), &'static str> {
    // create-user <username> <uid> <group> <shell> <gecos>
    if args.len() != 5 {
        return Err("usage: create-user <username> <uid> <group> <shell> <gecos>");
    }

    let username = &args[0];
    let uid_str = &args[1];
    let group = &args[2];
    let shell = &args[3];
    let gecos = &args[4];

    validate_username(username)?;
    let uid = validate_uid(uid_str)?;
    validate_group(group)?;
    validate_shell(shell)?;
    validate_gecos(gecos)?;

    run(
        "/usr/sbin/useradd",
        &[
            "-u",
            &uid.to_string(),
            "-g",
            group,
            "-s",
            shell,
            "-m",
            "-c",
            gecos,
            username,
        ],
    )
}

fn cmd_delete_user(args: &[String]) -> Result<(), &'static str> {
    if args.len() != 1 {
        return Err("usage: delete-user <username>");
    }
    validate_username(&args[0])?;

    // No -r flag: don't remove home directory (data preservation)
    run("/usr/sbin/userdel", &[&args[0]])
}

fn cmd_mkdir(args: &[String]) -> Result<(), &'static str> {
    if args.len() != 1 {
        return Err("usage: mkdir <path>");
    }

    validate_path(
        &args[0],
        &["/run/octo/runner-sockets/", "/home/octo_"],
    )?;

    run("/bin/mkdir", &["-p", &args[0]])
}

fn cmd_chown(args: &[String]) -> Result<(), &'static str> {
    // chown <owner:group> <path> [-R]
    if args.len() < 2 || args.len() > 3 {
        return Err("usage: chown <owner:group> <path> [-R]");
    }

    let owner_group = &args[0];
    let path = &args[1];
    let recursive = args.get(2).is_some_and(|a| a == "-R");

    // Validate owner is an octo_ user
    let parts: Vec<&str> = owner_group.split(':').collect();
    if parts.len() != 2 {
        return Err("owner must be in user:group format");
    }
    validate_username(parts[0])?;
    validate_group(parts[1])?;

    validate_path(
        path,
        &["/run/octo/runner-sockets/", "/home/octo_"],
    )?;

    if recursive {
        run("/usr/bin/chown", &["-R", owner_group, path])
    } else {
        run("/usr/bin/chown", &[owner_group, path])
    }
}

fn cmd_chmod(args: &[String]) -> Result<(), &'static str> {
    if args.len() != 2 {
        return Err("usage: chmod <mode> <path>");
    }

    let mode = &args[0];
    let path = &args[1];

    // Only allow specific safe modes
    match mode.as_str() {
        "700" | "750" | "755" | "770" | "2770" => {}
        _ => return Err("mode not in allowlist (700, 750, 755, 770, 2770)"),
    }

    validate_path(
        path,
        &["/run/octo/runner-sockets/", "/home/octo_"],
    )?;

    run("/usr/bin/chmod", &[mode, path])
}

fn cmd_enable_linger(args: &[String]) -> Result<(), &'static str> {
    if args.len() != 1 {
        return Err("usage: enable-linger <username>");
    }
    validate_username(&args[0])?;

    run("/usr/bin/loginctl", &["enable-linger", &args[0]])
}

fn cmd_start_user_service(args: &[String]) -> Result<(), &'static str> {
    if args.len() != 1 {
        return Err("usage: start-user-service <uid>");
    }
    let uid = validate_uid(&args[0])?;

    let service = format!("user@{uid}.service");
    run("/usr/bin/systemctl", &["start", &service])
}
