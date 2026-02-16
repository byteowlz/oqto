//! Input validation for octo-usermgr.
//!
//! All validation is pure (no side effects) and fully testable.
//! Every function returns Ok(()) or Err(String) with a human-readable message.

/// Required prefix for all managed usernames.
pub const USERNAME_PREFIX: &str = "octo_";

/// Required group name.
pub const REQUIRED_GROUP: &str = "octo";

/// Minimum allowed UID.
pub const UID_MIN: u32 = 2000;

/// Maximum allowed UID.
pub const UID_MAX: u32 = 60000;

/// Maximum username length (Linux limit is 32).
pub const USERNAME_MAX_LEN: usize = 32;

/// Maximum GECOS field length.
pub const GECOS_MAX_LEN: usize = 256;

/// Required GECOS prefix.
pub const GECOS_PREFIX: &str = "Octo platform user ";

/// Allowed shells.
pub const ALLOWED_SHELLS: &[&str] = &[
    "/bin/bash",
    "/bin/sh",
    "/usr/bin/bash",
    "/usr/bin/sh",
    "/bin/false",
    "/usr/sbin/nologin",
];

/// Allowed chmod modes.
pub const ALLOWED_MODES: &[&str] = &["700", "750", "755", "770", "2770"];

/// Validate a username for use as a Linux user managed by octo.
///
/// Rules:
/// - Must start with "octo_" prefix
/// - Max 32 characters
/// - Only lowercase ascii, digits, underscore, hyphen
/// - Must not be empty after prefix
pub fn validate_username(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("username is empty".into());
    }
    if !name.starts_with(USERNAME_PREFIX) {
        return Err(format!(
            "username '{name}' must start with '{USERNAME_PREFIX}' prefix"
        ));
    }
    if name.len() > USERNAME_MAX_LEN {
        return Err(format!(
            "username too long ({} > {USERNAME_MAX_LEN})",
            name.len()
        ));
    }
    // Must have something after the prefix
    if name.len() <= USERNAME_PREFIX.len() {
        return Err("username has nothing after prefix".into());
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
    {
        return Err("username contains invalid characters (allowed: a-z, 0-9, _, -)".into());
    }
    Ok(())
}

/// Validate the group name. Must be exactly "octo".
pub fn validate_group(group: &str) -> Result<(), String> {
    if group != REQUIRED_GROUP {
        return Err(format!("group must be '{REQUIRED_GROUP}', got '{group}'"));
    }
    Ok(())
}

/// Validate a UID is in the allowed range.
pub fn validate_uid(uid: u32) -> Result<(), String> {
    if uid < UID_MIN || uid > UID_MAX {
        return Err(format!(
            "UID {uid} out of allowed range ({UID_MIN}-{UID_MAX})"
        ));
    }
    Ok(())
}

/// Validate a shell path against the allowlist.
pub fn validate_shell(shell: &str) -> Result<(), String> {
    if ALLOWED_SHELLS.contains(&shell) {
        Ok(())
    } else {
        Err(format!("shell '{shell}' not in allowlist"))
    }
}

/// Validate a filesystem path against allowed prefixes.
///
/// Rejects:
/// - Empty paths
/// - Relative paths (must start with /)
/// - Path traversal (..)
/// - Double slashes (//)
/// - Null bytes
/// - Paths not starting with an allowed prefix
pub fn validate_path(path: &str, allowed_prefixes: &[&str]) -> Result<(), String> {
    if path.is_empty() {
        return Err("path is empty".into());
    }
    if path.contains('\0') {
        return Err("path contains null byte".into());
    }
    if !path.starts_with('/') {
        return Err("path must be absolute".into());
    }
    if path.contains("..") {
        return Err("path contains '..' (path traversal)".into());
    }
    if path.contains("//") {
        return Err("path contains '//'".into());
    }
    // Reject paths with newlines or other control chars
    if path.chars().any(|c| c.is_control()) {
        return Err("path contains control characters".into());
    }
    for prefix in allowed_prefixes {
        if path.starts_with(prefix) {
            return Ok(());
        }
    }
    Err(format!("path '{path}' not in allowed directories"))
}

/// Validate a GECOS (comment) field.
///
/// Must start with "Octo platform user ", no dangerous characters.
pub fn validate_gecos(gecos: &str) -> Result<(), String> {
    if gecos.is_empty() {
        return Err("GECOS is empty".into());
    }
    if !gecos.starts_with(GECOS_PREFIX) {
        return Err(format!("GECOS must start with '{GECOS_PREFIX}'"));
    }
    if gecos.len() > GECOS_MAX_LEN {
        return Err(format!(
            "GECOS too long ({} > {GECOS_MAX_LEN})",
            gecos.len()
        ));
    }
    // Reject characters that could break /etc/passwd or enable injection
    if gecos.contains(':') {
        return Err("GECOS contains ':' (passwd field separator)".into());
    }
    if gecos.contains('\n') {
        return Err("GECOS contains newline".into());
    }
    if gecos.contains('\r') {
        return Err("GECOS contains carriage return".into());
    }
    if gecos.contains('\0') {
        return Err("GECOS contains null byte".into());
    }
    Ok(())
}

/// Validate a chown owner string (user:group format).
pub fn validate_owner(owner: &str) -> Result<(), String> {
    let parts: Vec<&str> = owner.split(':').collect();
    if parts.len() != 2 {
        return Err(format!(
            "owner must be in user:group format, got '{owner}'"
        ));
    }
    validate_username(parts[0])?;
    validate_group(parts[1])?;
    Ok(())
}

/// Validate a chmod mode string against the allowlist.
pub fn validate_chmod_mode(mode: &str) -> Result<(), String> {
    if ALLOWED_MODES.contains(&mode) {
        Ok(())
    } else {
        Err(format!(
            "mode '{mode}' not in allowlist ({ALLOWED_MODES:?})"
        ))
    }
}

/// Validate all fields for a create-user request.
/// Returns Ok(()) if all fields are valid, or the first error.
pub fn validate_create_user(
    username: &str,
    uid: u32,
    group: &str,
    shell: &str,
    gecos: &str,
) -> Result<(), String> {
    validate_username(username)?;
    validate_uid(uid)?;
    validate_group(group)?;
    validate_shell(shell)?;
    validate_gecos(gecos)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // ===== Username validation =====

    #[test]
    fn username_valid_simple() {
        assert!(validate_username("octo_admin").is_ok());
    }

    #[test]
    fn username_valid_with_hyphen() {
        assert!(validate_username("octo_hans-gerd").is_ok());
    }

    #[test]
    fn username_valid_with_digits() {
        assert!(validate_username("octo_user123").is_ok());
    }

    #[test]
    fn username_valid_with_nanoid_suffix() {
        assert!(validate_username("octo_admin-a1b2").is_ok());
    }

    #[test]
    fn username_valid_underscore() {
        assert!(validate_username("octo_my_user").is_ok());
    }

    #[test]
    fn username_reject_empty() {
        assert!(validate_username("").is_err());
    }

    #[test]
    fn username_reject_no_prefix() {
        assert!(validate_username("admin").is_err());
    }

    #[test]
    fn username_reject_wrong_prefix() {
        assert!(validate_username("root_admin").is_err());
    }

    #[test]
    fn username_reject_just_prefix() {
        assert!(validate_username("octo_").is_err());
    }

    #[test]
    fn username_reject_uppercase() {
        assert!(validate_username("octo_Admin").is_err());
    }

    #[test]
    fn username_reject_spaces() {
        assert!(validate_username("octo_my user").is_err());
    }

    #[test]
    fn username_reject_special_chars() {
        assert!(validate_username("octo_user;whoami").is_err());
        assert!(validate_username("octo_user$HOME").is_err());
        assert!(validate_username("octo_user`id`").is_err());
        assert!(validate_username("octo_user|cat").is_err());
        assert!(validate_username("octo_user&bg").is_err());
        assert!(validate_username("octo_user>file").is_err());
        assert!(validate_username("octo_user<file").is_err());
        assert!(validate_username("octo_user(parens)").is_err());
        assert!(validate_username("octo_user'quote").is_err());
        assert!(validate_username("octo_user\"dquote").is_err());
    }

    #[test]
    fn username_reject_path_traversal() {
        assert!(validate_username("octo_../etc/passwd").is_err());
    }

    #[test]
    fn username_reject_null_byte() {
        assert!(validate_username("octo_user\0evil").is_err());
    }

    #[test]
    fn username_reject_newline() {
        assert!(validate_username("octo_user\nevil").is_err());
    }

    #[test]
    fn username_reject_too_long() {
        let long_name = format!("octo_{}", "a".repeat(28)); // 33 chars total
        assert!(validate_username(&long_name).is_err());
    }

    #[test]
    fn username_accept_max_length() {
        let name = format!("octo_{}", "a".repeat(27)); // exactly 32 chars
        assert!(validate_username(&name).is_ok());
    }

    #[test]
    fn username_reject_unicode() {
        assert!(validate_username("octo_u\u{0308}ser").is_err()); // umlaut
        assert!(validate_username("octo_\u{200B}user").is_err()); // zero-width space
    }

    // ===== Group validation =====

    #[test]
    fn group_valid() {
        assert!(validate_group("octo").is_ok());
    }

    #[test]
    fn group_reject_other() {
        assert!(validate_group("root").is_err());
        assert!(validate_group("wheel").is_err());
        assert!(validate_group("").is_err());
        assert!(validate_group("octo_").is_err());
        assert!(validate_group("Octo").is_err());
    }

    // ===== UID validation =====

    #[test]
    fn uid_valid_boundaries() {
        assert!(validate_uid(2000).is_ok());
        assert!(validate_uid(60000).is_ok());
        assert!(validate_uid(30000).is_ok());
    }

    #[test]
    fn uid_reject_below_min() {
        assert!(validate_uid(0).is_err());
        assert!(validate_uid(1).is_err());
        assert!(validate_uid(999).is_err());
        assert!(validate_uid(1999).is_err());
    }

    #[test]
    fn uid_reject_above_max() {
        assert!(validate_uid(60001).is_err());
        assert!(validate_uid(65534).is_err()); // nobody
        assert!(validate_uid(65535).is_err()); // reserved
        assert!(validate_uid(u32::MAX).is_err());
    }

    #[test]
    fn uid_reject_system_uids() {
        // root
        assert!(validate_uid(0).is_err());
        // typical system range
        for uid in [1, 100, 500, 998, 999] {
            assert!(validate_uid(uid).is_err(), "UID {uid} should be rejected");
        }
    }

    // ===== Shell validation =====

    #[test]
    fn shell_valid_all() {
        for shell in ALLOWED_SHELLS {
            assert!(validate_shell(shell).is_ok(), "shell {shell} should be valid");
        }
    }

    #[test]
    fn shell_reject_arbitrary() {
        assert!(validate_shell("/bin/zsh").is_err());
        assert!(validate_shell("/usr/bin/python3").is_err());
        assert!(validate_shell("/tmp/evil").is_err());
        assert!(validate_shell("bash").is_err());
        assert!(validate_shell("").is_err());
        assert!(validate_shell("/bin/bash; whoami").is_err());
        assert!(validate_shell("/bin/bash\0-c id").is_err());
    }

    // ===== Path validation =====

    #[test]
    fn path_valid_runner_socket() {
        let prefixes = &["/run/octo/runner-sockets/", "/home/octo_"];
        assert!(validate_path("/run/octo/runner-sockets/octo_admin", prefixes).is_ok());
        assert!(validate_path(
            "/run/octo/runner-sockets/octo_user-a1b2",
            prefixes
        )
        .is_ok());
    }

    #[test]
    fn path_valid_home() {
        let prefixes = &["/run/octo/runner-sockets/", "/home/octo_"];
        assert!(validate_path("/home/octo_admin", prefixes).is_ok());
        assert!(validate_path("/home/octo_admin/workspace", prefixes).is_ok());
    }

    #[test]
    fn path_reject_traversal() {
        let prefixes = &["/run/octo/runner-sockets/", "/home/octo_"];
        assert!(validate_path("/home/octo_admin/../root", prefixes).is_err());
        assert!(validate_path("/run/octo/runner-sockets/../../../etc/passwd", prefixes).is_err());
        assert!(validate_path("/home/octo_admin/..%2f..%2fetc", prefixes).is_err()); // encoded .. still contains ..
    }

    #[test]
    fn path_reject_double_slash() {
        let prefixes = &["/run/octo/runner-sockets/", "/home/octo_"];
        assert!(validate_path("/home/octo_admin//evil", prefixes).is_err());
        assert!(validate_path("//etc/passwd", prefixes).is_err());
    }

    #[test]
    fn path_reject_relative() {
        let prefixes = &["/run/octo/runner-sockets/", "/home/octo_"];
        assert!(validate_path("home/octo_admin", prefixes).is_err());
        assert!(validate_path("./home/octo_admin", prefixes).is_err());
        assert!(validate_path("octo_admin", prefixes).is_err());
    }

    #[test]
    fn path_reject_wrong_prefix() {
        let prefixes = &["/run/octo/runner-sockets/", "/home/octo_"];
        assert!(validate_path("/etc/passwd", prefixes).is_err());
        assert!(validate_path("/root/.ssh/authorized_keys", prefixes).is_err());
        assert!(validate_path("/home/tommy", prefixes).is_err());
        assert!(validate_path("/home/root", prefixes).is_err());
        assert!(validate_path("/tmp/evil", prefixes).is_err());
        assert!(validate_path("/var/lib/octo", prefixes).is_err());
    }

    #[test]
    fn path_reject_null_byte() {
        let prefixes = &["/run/octo/runner-sockets/", "/home/octo_"];
        assert!(validate_path("/home/octo_admin\0/etc/passwd", prefixes).is_err());
    }

    #[test]
    fn path_reject_control_chars() {
        let prefixes = &["/run/octo/runner-sockets/", "/home/octo_"];
        assert!(validate_path("/home/octo_admin\n/etc/passwd", prefixes).is_err());
        assert!(validate_path("/home/octo_admin\t/evil", prefixes).is_err());
    }

    #[test]
    fn path_reject_empty() {
        let prefixes = &["/run/octo/runner-sockets/", "/home/octo_"];
        assert!(validate_path("", prefixes).is_err());
    }

    #[test]
    fn path_reject_symlink_trickery() {
        // While we can't prevent symlinks at validation time,
        // we ensure the string doesn't bypass prefix checks
        let prefixes = &["/run/octo/runner-sockets/", "/home/octo_"];
        assert!(validate_path("/home/octo_admin/../../etc/shadow", prefixes).is_err());
    }

    // ===== GECOS validation =====

    #[test]
    fn gecos_valid() {
        assert!(validate_gecos("Octo platform user admin-a1b2").is_ok());
        assert!(validate_gecos("Octo platform user hansgerd-x9z1").is_ok());
    }

    #[test]
    fn gecos_reject_wrong_prefix() {
        assert!(validate_gecos("Some other user").is_err());
        assert!(validate_gecos("root").is_err());
        assert!(validate_gecos("").is_err());
    }

    #[test]
    fn gecos_reject_colon() {
        // Colon separates fields in /etc/passwd
        assert!(validate_gecos("Octo platform user admin:0:0:root").is_err());
    }

    #[test]
    fn gecos_reject_newline() {
        assert!(validate_gecos("Octo platform user admin\nevil:0:0::/root:/bin/bash").is_err());
    }

    #[test]
    fn gecos_reject_null() {
        assert!(validate_gecos("Octo platform user admin\0evil").is_err());
    }

    #[test]
    fn gecos_reject_carriage_return() {
        assert!(validate_gecos("Octo platform user admin\revil").is_err());
    }

    #[test]
    fn gecos_reject_too_long() {
        let long = format!("Octo platform user {}", "a".repeat(GECOS_MAX_LEN));
        assert!(validate_gecos(&long).is_err());
    }

    #[test]
    fn gecos_accept_max_length() {
        let gecos = format!(
            "Octo platform user {}",
            "a".repeat(GECOS_MAX_LEN - GECOS_PREFIX.len())
        );
        assert_eq!(gecos.len(), GECOS_MAX_LEN);
        assert!(validate_gecos(&gecos).is_ok());
    }

    // ===== Owner validation =====

    #[test]
    fn owner_valid() {
        assert!(validate_owner("octo_admin:octo").is_ok());
        assert!(validate_owner("octo_user-a1b2:octo").is_ok());
    }

    #[test]
    fn owner_reject_wrong_format() {
        assert!(validate_owner("octo_admin").is_err()); // no colon
        assert!(validate_owner("octo_admin:octo:extra").is_err()); // too many colons
        assert!(validate_owner(":octo").is_err()); // empty user
        assert!(validate_owner("octo_admin:").is_err()); // empty group
    }

    #[test]
    fn owner_reject_wrong_user() {
        assert!(validate_owner("root:octo").is_err());
        assert!(validate_owner("tommy:octo").is_err());
    }

    #[test]
    fn owner_reject_wrong_group() {
        assert!(validate_owner("octo_admin:root").is_err());
        assert!(validate_owner("octo_admin:wheel").is_err());
    }

    // ===== Chmod mode validation =====

    #[test]
    fn mode_valid_all() {
        for mode in ALLOWED_MODES {
            assert!(
                validate_chmod_mode(mode).is_ok(),
                "mode {mode} should be valid"
            );
        }
    }

    #[test]
    fn mode_reject_arbitrary() {
        assert!(validate_chmod_mode("777").is_err());
        assert!(validate_chmod_mode("666").is_err());
        assert!(validate_chmod_mode("444").is_err());
        assert!(validate_chmod_mode("4755").is_err()); // setuid
        assert!(validate_chmod_mode("2755").is_err()); // not in list
        assert!(validate_chmod_mode("").is_err());
        assert!(validate_chmod_mode("rwxrwxrwx").is_err());
        assert!(validate_chmod_mode("u+s").is_err());
    }

    // ===== Composite create-user validation =====

    #[test]
    fn create_user_valid() {
        assert!(validate_create_user(
            "octo_admin-a1b2",
            2000,
            "octo",
            "/bin/bash",
            "Octo platform user admin-a1b2"
        )
        .is_ok());
    }

    #[test]
    fn create_user_rejects_each_field() {
        // Bad username
        assert!(validate_create_user(
            "root",
            2000,
            "octo",
            "/bin/bash",
            "Octo platform user root"
        )
        .is_err());

        // Bad UID
        assert!(validate_create_user(
            "octo_admin",
            0,
            "octo",
            "/bin/bash",
            "Octo platform user admin"
        )
        .is_err());

        // Bad group
        assert!(validate_create_user(
            "octo_admin",
            2000,
            "root",
            "/bin/bash",
            "Octo platform user admin"
        )
        .is_err());

        // Bad shell
        assert!(validate_create_user(
            "octo_admin",
            2000,
            "octo",
            "/tmp/evil",
            "Octo platform user admin"
        )
        .is_err());

        // Bad GECOS
        assert!(validate_create_user(
            "octo_admin",
            2000,
            "octo",
            "/bin/bash",
            "evil:0:0::/root:/bin/bash"
        )
        .is_err());
    }

    // ===== Injection attack patterns =====

    #[test]
    fn injection_username_shell_escape() {
        // Attempts to inject shell commands via username
        assert!(validate_username("octo_$(whoami)").is_err());
        assert!(validate_username("octo_`id`").is_err());
        assert!(validate_username("octo_;rm -rf /").is_err());
        assert!(validate_username("octo_|cat /etc/shadow").is_err());
    }

    #[test]
    fn injection_gecos_passwd_line() {
        // Attempt to inject a new /etc/passwd line via GECOS
        assert!(validate_gecos("Octo platform user x\nevil:0:0::/root:/bin/bash").is_err());
    }

    #[test]
    fn injection_gecos_field_escape() {
        // Attempt to break out of GECOS field via colon
        assert!(validate_gecos("Octo platform user x:0:0:root:/root:/bin/bash").is_err());
    }

    #[test]
    fn injection_path_escape_home() {
        let prefixes = &["/run/octo/runner-sockets/", "/home/octo_"];
        // Attempt to escape to /etc via path traversal
        assert!(validate_path("/home/octo_evil/../../../etc/shadow", prefixes).is_err());
        assert!(validate_path("/home/octo_evil/../../root/.ssh", prefixes).is_err());
    }

    #[test]
    fn injection_path_null_truncation() {
        let prefixes = &["/run/octo/runner-sockets/", "/home/octo_"];
        // Null byte truncation attack
        assert!(validate_path("/home/octo_admin\0/../../../etc/shadow", prefixes).is_err());
    }
}
