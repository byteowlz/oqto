//! OqtoUI customization config evaluation.

use std::path::PathBuf;

use axum::{Json, extract::State};
use oqto_uiconfig::{EvaluatedConfig, resolve_user_source};

use crate::api::state::AppState;
use crate::auth::CurrentUser;

const USER_CONFIG_RELATIVE_PATH: &str = ".config/oqto/oqto-ui.lua";

fn user_config_path(state: &AppState, user_id: &str) -> PathBuf {
    if let Ok(override_path) = std::env::var("OQTO_UI_CONFIG")
        && !override_path.trim().is_empty()
    {
        return PathBuf::from(override_path);
    }
    if let Some(linux_users) = state.linux_users.as_ref().filter(|config| config.enabled) {
        if let Ok(Some(home)) = linux_users.get_home_dir(user_id) {
            return home.join(USER_CONFIG_RELATIVE_PATH);
        }
        return PathBuf::from(format!("/home/{}", linux_users.linux_username(user_id)))
            .join(USER_CONFIG_RELATIVE_PATH);
    }
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("/nonexistent"))
        .join(USER_CONFIG_RELATIVE_PATH)
}

/// Resolve the authenticated user's customization over dist defaults.
///
/// An absent file returns the dist preset. An unreadable or invalid file is a
/// dropped user layer: the response still contains bootable defaults and a
/// named diagnostic.
pub async fn get_oqto_ui_config(
    State(state): State<AppState>,
    user: CurrentUser,
) -> Json<EvaluatedConfig> {
    let path = user_config_path(&state, user.id());
    let source = match tokio::fs::read_to_string(&path).await {
        Ok(source) => Some(source),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => {
            let mut resolved = resolve_user_source(None);
            resolved.source = oqto_uiconfig::ConfigSource::UserLuaFallback;
            resolved.diagnostics.push(oqto_uiconfig::ConfigDiagnostic {
                code: "customization.user.unreadable".to_owned(),
                message: format!("could not read {}: {error}", path.display()),
            });
            return Json(resolved);
        }
    };
    Json(resolve_user_source(source.as_deref()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_path_is_under_user_config_directory() {
        let state = AppState::default();
        let path = user_config_path(&state, "test-user");
        assert!(path.ends_with(USER_CONFIG_RELATIVE_PATH));
    }
}
