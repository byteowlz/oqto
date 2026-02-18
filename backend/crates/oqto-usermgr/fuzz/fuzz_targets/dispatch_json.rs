#![no_main]
use libfuzzer_sys::fuzz_target;
use oqto_usermgr::validate::*;

/// Fuzz the JSON request parsing and validation dispatch.
/// This tests what happens when arbitrary JSON arrives on the socket.
///
/// We can't call the actual dispatch() (it has side effects), but we can
/// verify that for any JSON input, the validation layer correctly gates
/// all operations.
fuzz_target!(|data: &[u8]| {
    // Try to parse as JSON
    let Ok(value) = serde_json::from_slice::<serde_json::Value>(data) else {
        return; // Invalid JSON is rejected at the protocol layer
    };

    // Extract command and args
    let Some(cmd) = value.get("cmd").and_then(|v| v.as_str()) else {
        return; // Missing cmd is rejected
    };
    let args = value.get("args").cloned().unwrap_or(serde_json::Value::Null);

    // Simulate validation for each command type
    match cmd {
        "create-user" => {
            if let (Some(username), Some(uid), Some(group), Some(shell), Some(gecos)) = (
                args.get("username").and_then(|v| v.as_str()),
                args.get("uid").and_then(|v| v.as_u64()),
                args.get("group").and_then(|v| v.as_str()),
                args.get("shell").and_then(|v| v.as_str()),
                args.get("gecos").and_then(|v| v.as_str()),
            ) {
                let result = validate_create_user(username, uid as u32, group, shell, gecos);

                // If validation passes, ALL invariants must hold
                if result.is_ok() {
                    assert!(username.starts_with(USERNAME_PREFIX));
                    assert!(username.len() <= USERNAME_MAX_LEN);
                    assert!((UID_MIN..=UID_MAX).contains(&(uid as u32)));
                    assert_eq!(group, REQUIRED_GROUP);
                    assert!(ALLOWED_SHELLS.contains(&shell));
                    assert!(gecos.starts_with(GECOS_PREFIX));
                    assert!(!gecos.contains(':'));
                    assert!(!gecos.contains('\n'));
                }
            }
        }
        "create-group" => {
            if let Some(group) = args.get("group").and_then(|v| v.as_str()) {
                if validate_group(group).is_ok() {
                    assert_eq!(group, REQUIRED_GROUP);
                }
            }
        }
        "delete-user" => {
            if let Some(username) = args.get("username").and_then(|v| v.as_str()) {
                if validate_username(username).is_ok() {
                    assert!(username.starts_with(USERNAME_PREFIX));
                    assert!(username.len() <= USERNAME_MAX_LEN);
                }
            }
        }
        "mkdir" | "chmod" => {
            if let Some(path) = args.get("path").and_then(|v| v.as_str()) {
                let prefixes = &["/run/oqto/runner-sockets/", "/home/oqto_"];
                if validate_path(path, prefixes).is_ok() {
                    assert!(path.starts_with('/'));
                    assert!(!path.contains(".."));
                    assert!(!path.contains("//"));
                    assert!(!path.contains('\0'));
                    assert!(prefixes.iter().any(|p| path.starts_with(p)));
                }
            }
        }
        "chown" => {
            if let Some(owner) = args.get("owner").and_then(|v| v.as_str()) {
                if validate_owner(owner).is_ok() {
                    let parts: Vec<&str> = owner.split(':').collect();
                    assert_eq!(parts.len(), 2);
                    assert!(parts[0].starts_with(USERNAME_PREFIX));
                    assert_eq!(parts[1], REQUIRED_GROUP);
                }
            }
            if let Some(path) = args.get("path").and_then(|v| v.as_str()) {
                let prefixes = &["/run/oqto/runner-sockets/", "/home/oqto_"];
                if validate_path(path, prefixes).is_ok() {
                    assert!(!path.contains(".."));
                    assert!(prefixes.iter().any(|p| path.starts_with(p)));
                }
            }
        }
        "enable-linger" => {
            if let Some(username) = args.get("username").and_then(|v| v.as_str()) {
                if validate_username(username).is_ok() {
                    assert!(username.starts_with(USERNAME_PREFIX));
                }
            }
        }
        "start-user-service" => {
            if let Some(uid) = args.get("uid").and_then(|v| v.as_u64()) {
                if validate_uid(uid as u32).is_ok() {
                    assert!((UID_MIN..=UID_MAX).contains(&(uid as u32)));
                }
            }
        }
        _ => {
            // Unknown commands are rejected by dispatch
        }
    }
});
