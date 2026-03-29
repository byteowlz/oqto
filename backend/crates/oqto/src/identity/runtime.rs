use crate::local::LinuxUsersConfig;

/// Returns true when Linux user isolation is enabled (multi-user mode).
pub fn user_isolation_enabled(linux_users: Option<&LinuxUsersConfig>) -> bool {
    linux_users.is_some_and(|cfg| cfg.enabled)
}

/// Resolve the effective Linux username used for runner/user-plane routing.
///
/// Security model:
/// - isolated multi-user mode: map auth user id -> provisioned linux username
/// - non-isolated mode: always use current OS user (fallback: auth user id)
pub fn effective_linux_username(linux_users: Option<&LinuxUsersConfig>, user_id: &str) -> String {
    if let Some(linux_users_cfg) = linux_users.filter(|cfg| cfg.enabled) {
        return linux_users_cfg.linux_username(user_id);
    }

    os_username().unwrap_or_else(|| user_id.to_string())
}

fn os_username() -> Option<String> {
    std::env::var("USER").ok().filter(|v| !v.trim().is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn effective_linux_username_uses_os_user_when_not_isolated() {
        let expected = os_username().unwrap_or_else(|| "auth-user-id".to_string());

        let resolved = effective_linux_username(None, "auth-user-id");
        assert_eq!(resolved, expected);
    }

    #[test]
    fn effective_linux_username_maps_with_linux_user_isolation() {
        let cfg = LinuxUsersConfig {
            enabled: true,
            prefix: "oqto_".to_string(),
            ..LinuxUsersConfig::default()
        };

        let resolved = effective_linux_username(Some(&cfg), "alice");
        assert!(resolved == "alice" || resolved == "oqto_alice");
    }

    #[test]
    fn effective_linux_username_uses_os_user_if_linux_config_present_but_disabled() {
        let cfg = LinuxUsersConfig {
            enabled: false,
            ..LinuxUsersConfig::default()
        };
        let expected = os_username().unwrap_or_else(|| "wismut-2ev6".to_string());
        let resolved = effective_linux_username(Some(&cfg), "wismut-2ev6");
        assert_eq!(resolved, expected);
    }
}
