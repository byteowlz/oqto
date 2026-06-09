//! Post-registration environment provisioning.
//!
//! Keeps auth handlers focused on identity and delegates runtime bootstrap
//! (mmry, Pi config, workspace scaffolding) to a dedicated module.

use tracing::{info, warn};

use crate::api::handlers::admin::provision_eavs_for_user;
use crate::api::state::AppState;
use crate::user::User;

pub async fn bootstrap_new_user_environment(
    state: &AppState,
    user: &User,
    resolved_linux_username: Option<&str>,
) {
    // Workspace memory now uses mmry-core JSONL files via runner/backend paths.
    // No per-user mmry service port allocation is required.
    // Provision EAVS virtual key and write Pi models.json + settings.json.
    // Track whether settings.json was written so we can fall back to config defaults.
    let mut pi_settings_written = false;

    if let (Some(eavs_client), Some(linux_users)) = (&state.eavs_client, &state.linux_users) {
        let linux_username = resolved_linux_username
            .or(user.linux_username.as_deref())
            .unwrap_or(&user.id);

        match provision_eavs_for_user(
            eavs_client,
            linux_users,
            linux_username,
            &user.id,
            Some(&state.auto_rename_config),
        )
        .await
        {
            Ok(key_id) => {
                info!(
                    user_id = %user.id,
                    eavs_key_id = %key_id,
                    "Provisioned EAVS key and models.json"
                );

                // Write settings.json with default provider/model from eavs catalog.
                if let Ok(home) = linux_users.get_user_home(linux_username) {
                    let models_path = std::path::PathBuf::from(&home).join(".pi/agent/models.json");
                    if let Ok(content) = std::fs::read_to_string(&models_path)
                        && let Ok(config) = serde_json::from_str::<serde_json::Value>(&content)
                        && let Some(providers) = config.get("providers").and_then(|p| p.as_object())
                    {
                        // Prefer broadly compatible APIs for default selection.
                        // Some providers may rely on optional extension APIs (e.g.
                        // openai-completions-convert-think-tags) which can fail if
                        // extension installation drifts. Pick a stable built-in API first.
                        let preferred_apis = [
                            "openai-responses",
                            "openai-completions",
                            "anthropic",
                            "gemini",
                        ];

                        let mut selected: Option<(&str, &str)> = None;

                        for api in preferred_apis {
                            if let Some((provider_name, provider_config)) =
                                providers.iter().find(|(_, cfg)| {
                                    cfg.get("api")
                                        .and_then(|v| v.as_str())
                                        .is_some_and(|value| value == api)
                                        && cfg
                                            .get("models")
                                            .and_then(|m| m.as_array())
                                            .is_some_and(|arr| !arr.is_empty())
                                })
                                && let Some(first_model) = provider_config
                                    .get("models")
                                    .and_then(|m| m.as_array())
                                    .and_then(|a| a.first())
                                    .and_then(|m| m.get("id"))
                                    .and_then(|id| id.as_str())
                            {
                                selected = Some((provider_name.as_str(), first_model));
                                break;
                            }
                        }

                        if selected.is_none()
                            && let Some((provider_name, provider_config)) =
                                providers.iter().find(|(_, cfg)| {
                                    cfg.get("models")
                                        .and_then(|m| m.as_array())
                                        .is_some_and(|arr| !arr.is_empty())
                                })
                            && let Some(first_model) = provider_config
                                .get("models")
                                .and_then(|m| m.as_array())
                                .and_then(|a| a.first())
                                .and_then(|m| m.get("id"))
                                .and_then(|id| id.as_str())
                        {
                            selected = Some((provider_name.as_str(), first_model));
                        }

                        if let Some((provider_name, first_model)) = selected {
                            let settings = serde_json::json!({
                                "defaultProvider": provider_name,
                                "defaultModel": first_model,
                            });
                            let settings_str =
                                serde_json::to_string_pretty(&settings).unwrap_or_default();
                            let rel_path = ".pi/agent/settings.json";
                            if let Err(e) = crate::local::linux_users::usermgr_request(
                                "write-file",
                                serde_json::json!({
                                    "username": linux_username,
                                    "path": rel_path,
                                    "content": settings_str,
                                    "group": "oqto",
                                }),
                            ) {
                                warn!(
                                    user_id = %user.id,
                                    error = ?e,
                                    "Failed to write Pi settings.json (non-fatal)"
                                );
                            } else {
                                pi_settings_written = true;
                                info!(
                                    user_id = %user.id,
                                    provider = %provider_name,
                                    model = %first_model,
                                    "Wrote default Pi settings.json"
                                );
                            }
                        }
                    }
                }
            }
            Err(e) => {
                warn!(
                    user_id = %user.id,
                    error = ?e,
                    "Failed to provision EAVS (non-fatal)"
                );
            }
        }
    }

    // Fallback: if eavs was not configured (or failed), still write Pi config files.
    if !pi_settings_written && state.linux_users.is_some() {
        let linux_username = resolved_linux_username
            .or(user.linux_username.as_deref())
            .unwrap_or(&user.id);

        if let (Some(provider), Some(model)) = (&state.pi_default_provider, &state.pi_default_model)
        {
            let settings = serde_json::json!({
                "defaultProvider": provider,
                "defaultModel": model,
            });
            let settings_str = serde_json::to_string_pretty(&settings).unwrap_or_default();
            if let Err(e) = crate::local::linux_users::usermgr_request(
                "write-file",
                serde_json::json!({
                    "username": linux_username,
                    "path": ".pi/agent/settings.json",
                    "content": settings_str,
                    "group": "oqto",
                }),
            ) {
                warn!(
                    user_id = %user.id,
                    error = ?e,
                    "Failed to write fallback Pi settings.json (non-fatal)"
                );
            } else {
                info!(
                    user_id = %user.id,
                    provider = %provider,
                    model = %model,
                    "Wrote Pi settings.json from config defaults (no eavs)"
                );
            }
        }

        if let Some(ref template_path) = state.pi_models_template_path
            && template_path.exists()
        {
            match std::fs::read_to_string(template_path) {
                Ok(content) => {
                    if let Err(e) = crate::local::linux_users::usermgr_request(
                        "write-file",
                        serde_json::json!({
                            "username": linux_username,
                            "path": ".pi/agent/models.json",
                            "content": content,
                            "group": "oqto",
                        }),
                    ) {
                        warn!(
                            user_id = %user.id,
                            error = ?e,
                            "Failed to copy models.json template (non-fatal)"
                        );
                    } else {
                        info!(
                            user_id = %user.id,
                            template = %template_path.display(),
                            "Copied models.json template to new user"
                        );
                    }
                }
                Err(e) => {
                    warn!(
                        user_id = %user.id,
                        error = ?e,
                        "Failed to read models.json template (non-fatal)"
                    );
                }
            }
        }
    }

    // Create the default "main" workspace so the user lands in a ready-to-use state.
    if state.linux_users.is_some() {
        let linux_username = resolved_linux_username
            .or(user.linux_username.as_deref())
            .unwrap_or(&user.id);

        let workspace_path = format!("/home/{linux_username}/oqto/main");
        let mut template_files = serde_json::Map::new();
        let meta_toml = "display_name = \"Main\"\npinned = true\n".to_string();
        template_files.insert(
            ".oqto/workspace.toml".into(),
            serde_json::Value::String(meta_toml),
        );

        let context_json = serde_json::json!({
            "contextFiles": [
                { "names": ["BOOTSTRAP.md"], "optional": false },
                { "names": ["PERSONALITY.md"], "optional": true },
                { "names": ["USER.md"], "optional": true }
            ]
        })
        .to_string();
        template_files.insert(
            ".pi/context.json".into(),
            serde_json::Value::String(context_json),
        );

        let template_src = state
            .onboarding_templates
            .as_ref()
            .map(|t| t.templates_dir().join(t.subdirectory()));

        if let Some(ref templates_service) = state.onboarding_templates {
            match templates_service.resolve(None).await {
                Ok(templates) => {
                    template_files.insert(
                        "BOOTSTRAP.md".into(),
                        serde_json::Value::String(templates.onboard),
                    );
                    template_files.insert(
                        "PERSONALITY.md".into(),
                        serde_json::Value::String(templates.personality),
                    );
                    template_files
                        .insert("USER.md".into(), serde_json::Value::String(templates.user));
                    template_files.insert(
                        "AGENTS.md".into(),
                        serde_json::Value::String(templates.agents),
                    );
                }
                Err(e) => {
                    warn!(user_id = %user.id, error = ?e, "Failed to resolve templates (non-fatal)");
                }
            }
        }

        let mut create_args = serde_json::json!({
            "username": linux_username,
            "path": workspace_path,
            "files": template_files,
        });

        if let Some(ref src) = template_src
            && src.is_dir()
        {
            create_args["template_src"] =
                serde_json::Value::String(src.to_string_lossy().into_owned());
        }

        match crate::local::linux_users::usermgr_request("create-workspace", create_args) {
            Ok(_) => {
                info!(user_id = %user.id, "Created default workspace for new user");
            }
            Err(e) => {
                warn!(user_id = %user.id, error = ?e, "Failed to create default workspace (non-fatal)");
            }
        }
    }
}
