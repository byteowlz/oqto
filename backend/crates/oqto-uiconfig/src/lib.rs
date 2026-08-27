//! Sandboxed evaluation of Oqto UI customizations.
//!
//! Lua is an ergonomic producer. [`OqtoUiConfigV1`] is the portable contract.

use std::sync::{Arc, Mutex};

use mlua::{HookTriggers, Lua, LuaOptions, LuaSerdeExt, StdLib, Value, VmState};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const SCHEMA_VERSION: u16 = 1;
const MEMORY_LIMIT_BYTES: usize = 2 * 1024 * 1024;
const HOOK_INTERVAL: u32 = 1_000;
const MAX_INSTRUCTIONS: u32 = 100_000;

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct OqtoUiConfigV1 {
    pub version: u16,
    #[serde(default = "default_preset")]
    pub preset: String,
    #[serde(default)]
    pub appearance: AppearanceConfig,
    #[serde(default)]
    pub layout: LayoutConfig,
    #[serde(default)]
    pub bindings: Vec<KeyBinding>,
    #[serde(default)]
    pub status_line: StatusLineConfig,
    #[serde(default)]
    pub mobile: MobileConfig,
}

impl Default for OqtoUiConfigV1 {
    fn default() -> Self {
        Self {
            version: SCHEMA_VERSION,
            preset: default_preset(),
            appearance: AppearanceConfig::default(),
            layout: LayoutConfig::default(),
            bindings: vec![
                KeyBinding::new("ctrl+shift+p", "shell.openCommandPalette"),
                KeyBinding::new("ctrl+shift+f", "view.openFiles"),
            ],
            status_line: StatusLineConfig::default(),
            mobile: MobileConfig::default(),
        }
    }
}

fn default_preset() -> String {
    "classic".to_owned()
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, JsonSchema, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct AppearanceConfig {
    #[serde(default)]
    pub scheme: Scheme,
    #[serde(default)]
    pub radius: Radius,
    #[serde(default)]
    pub density: Density,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Scheme {
    #[default]
    OqtoDark,
    OqtoLight,
    NordDark,
    NordLight,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Radius {
    #[default]
    Square,
    Compact,
    Soft,
}

impl Radius {
    pub const fn css_value(self) -> &'static str {
        match self {
            Self::Square => "0px",
            Self::Compact => "4px",
            Self::Soft => "10px",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Density {
    Compact,
    #[default]
    Standard,
    Comfortable,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, JsonSchema, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct LayoutConfig {
    #[serde(default)]
    pub files: FilesPlacement,
    #[serde(default)]
    pub navigator: NavigatorPlacement,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum FilesPlacement {
    Left,
    #[default]
    Right,
    Hidden,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum NavigatorPlacement {
    #[default]
    Left,
    Hidden,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct KeyBinding {
    pub keys: String,
    pub action: String,
}

impl KeyBinding {
    fn new(keys: &str, action: &str) -> Self {
        Self {
            keys: keys.to_owned(),
            action: action.to_owned(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct MobileConfig {
    #[serde(default)]
    pub mode: MobileMode,
    #[serde(default = "default_hold_ms")]
    pub hold_ms: u16,
    #[serde(default)]
    pub corners: CornerSlots,
}

impl Default for MobileConfig {
    fn default() -> Self {
        Self {
            mode: MobileMode::Classic,
            hold_ms: default_hold_ms(),
            corners: CornerSlots::default(),
        }
    }
}

const fn default_hold_ms() -> u16 {
    320
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum MobileMode {
    #[default]
    Classic,
    Corner,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CornerSlots {
    pub top_left: CornerBinding,
    pub top_right: CornerBinding,
    pub bottom_left: CornerBinding,
    pub bottom_right: CornerBinding,
}

impl Default for CornerSlots {
    fn default() -> Self {
        Self {
            top_left: CornerBinding::new("navigator.open", "menu.projects"),
            top_right: CornerBinding::new("view.openFiles", "menu.tools"),
            bottom_left: CornerBinding::new("session.openPrevious", "menu.sessionMru"),
            bottom_right: CornerBinding::new("chat.send", "menu.chatActions"),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CornerBinding {
    pub tap: String,
    pub hold: String,
}

impl CornerBinding {
    fn new(tap: &str, hold: &str) -> Self {
        Self {
            tap: tap.to_owned(),
            hold: hold.to_owned(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct StatusLineConfig {
    #[serde(default = "default_segments")]
    pub segments: Vec<StatusSegment>,
}

impl Default for StatusLineConfig {
    fn default() -> Self {
        Self {
            segments: default_segments(),
        }
    }
}

fn default_segments() -> Vec<StatusSegment> {
    vec![
        StatusSegment::Session,
        StatusSegment::Model,
        StatusSegment::Context,
        StatusSegment::Connection,
    ]
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum StatusSegment {
    Session,
    Model,
    Context,
    Connection,
    RunnerLoad,
    Version,
}

#[derive(Clone, Debug, Serialize, JsonSchema, PartialEq)]
pub struct EvaluatedConfig {
    pub config: OqtoUiConfigV1,
    pub source: ConfigSource,
    pub diagnostics: Vec<ConfigDiagnostic>,
}

#[derive(Clone, Copy, Debug, Serialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ConfigSource {
    DistDefault,
    UserLua,
    UserLuaFallback,
}

#[derive(Clone, Debug, Serialize, JsonSchema, PartialEq, Eq)]
pub struct ConfigDiagnostic {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Error)]
pub enum EvalError {
    #[error("Lua sandbox setup failed: {0}")]
    Sandbox(String),
    #[error("Lua evaluation failed: {0}")]
    Lua(String),
    #[error("configuration did not call oqto.setup({{...}})")]
    MissingSetup,
    #[error("configuration schema validation failed: {0}")]
    Schema(String),
    #[error("unsupported schema version {found}; expected {expected}")]
    Version { found: u16, expected: u16 },
}

/// Evaluate one Lua source in isolation. No filesystem, process, network,
/// package loading, or debug library is present in the VM.
pub fn evaluate(source: &str) -> Result<OqtoUiConfigV1, EvalError> {
    let libraries = StdLib::TABLE | StdLib::STRING | StdLib::MATH | StdLib::UTF8;
    let lua = Lua::new_with(libraries, LuaOptions::default())
        .map_err(|error| EvalError::Sandbox(error.to_string()))?;
    lua.set_memory_limit(MEMORY_LIMIT_BYTES)
        .map_err(|error| EvalError::Sandbox(error.to_string()))?;

    let consumed = Arc::new(std::sync::atomic::AtomicU32::new(0));
    let hook_consumed = Arc::clone(&consumed);
    lua.set_hook(
        HookTriggers::new().every_nth_instruction(HOOK_INTERVAL),
        move |_, _| {
            let total = hook_consumed
                .fetch_add(HOOK_INTERVAL, std::sync::atomic::Ordering::Relaxed)
                + HOOK_INTERVAL;
            if total > MAX_INSTRUCTIONS {
                return Err(mlua::Error::runtime(
                    "configuration instruction budget exceeded",
                ));
            }
            Ok(VmState::Continue)
        },
    )
    .map_err(|error| EvalError::Sandbox(error.to_string()))?;

    let captured = Arc::new(Mutex::new(None::<serde_json::Value>));
    let setup_capture = Arc::clone(&captured);
    let setup = lua
        .create_function(move |lua, value: Value| {
            let json = lua.from_value(value)?;
            let mut guard = setup_capture
                .lock()
                .map_err(|_| mlua::Error::runtime("configuration capture lock poisoned"))?;
            if guard.is_some() {
                return Err(mlua::Error::runtime("oqto.setup may only be called once"));
            }
            *guard = Some(json);
            Ok(())
        })
        .map_err(|error| EvalError::Sandbox(error.to_string()))?;
    let oqto = lua
        .create_table()
        .map_err(|error| EvalError::Sandbox(error.to_string()))?;
    oqto.set("setup", setup)
        .map_err(|error| EvalError::Sandbox(error.to_string()))?;
    oqto.set("schema_version", SCHEMA_VERSION)
        .map_err(|error| EvalError::Sandbox(error.to_string()))?;
    lua.globals()
        .set("oqto", oqto)
        .map_err(|error| EvalError::Sandbox(error.to_string()))?;

    lua.load(source)
        .set_name("@oqto-ui.lua")
        .exec()
        .map_err(|error| EvalError::Lua(error.to_string()))?;

    let value = captured
        .lock()
        .map_err(|_| EvalError::Sandbox("configuration capture lock poisoned".to_owned()))?
        .take()
        .ok_or(EvalError::MissingSetup)?;
    let config: OqtoUiConfigV1 =
        serde_json::from_value(value).map_err(|error| EvalError::Schema(error.to_string()))?;
    if config.version != SCHEMA_VERSION {
        return Err(EvalError::Version {
            found: config.version,
            expected: SCHEMA_VERSION,
        });
    }
    validate(&config)?;
    Ok(config)
}

fn validate(config: &OqtoUiConfigV1) -> Result<(), EvalError> {
    if config.preset.trim().is_empty() {
        return Err(EvalError::Schema("preset must not be empty".to_owned()));
    }
    if !(250..=500).contains(&config.mobile.hold_ms) {
        return Err(EvalError::Schema(
            "mobile hold_ms must be between 250 and 500".to_owned(),
        ));
    }
    for corner in [
        &config.mobile.corners.top_left,
        &config.mobile.corners.top_right,
        &config.mobile.corners.bottom_left,
        &config.mobile.corners.bottom_right,
    ] {
        if !corner.tap.contains('.') || !corner.hold.starts_with("menu.") {
            return Err(EvalError::Schema(
                "corner tap must be a namespaced Action and hold must reference menu.*".to_owned(),
            ));
        }
    }
    for binding in &config.bindings {
        if binding.keys.trim().is_empty() || binding.action.trim().is_empty() {
            return Err(EvalError::Schema(
                "binding keys and action must not be empty".to_owned(),
            ));
        }
        if !binding.action.contains('.') {
            return Err(EvalError::Schema(format!(
                "action '{}' must be namespaced",
                binding.action
            )));
        }
    }
    Ok(())
}

/// Resolve a user layer over immutable dist defaults. A broken user layer is
/// dropped and reported; it can never prevent the shell from booting.
pub fn resolve_user_source(source: Option<&str>) -> EvaluatedConfig {
    let Some(source) = source else {
        return EvaluatedConfig {
            config: OqtoUiConfigV1::default(),
            source: ConfigSource::DistDefault,
            diagnostics: Vec::new(),
        };
    };
    match evaluate(source) {
        Ok(config) => EvaluatedConfig {
            config,
            source: ConfigSource::UserLua,
            diagnostics: Vec::new(),
        },
        Err(error) => EvaluatedConfig {
            config: OqtoUiConfigV1::default(),
            source: ConfigSource::UserLuaFallback,
            diagnostics: vec![ConfigDiagnostic {
                code: "customization.user.invalid".to_owned(),
                message: error.to_string(),
            }],
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALID: &str = r#"
        oqto.setup({
          version = oqto.schema_version,
          preset = "instrument-panel",
          appearance = { scheme = "nord-dark", radius = "soft", density = "compact" },
          layout = { files = "left", navigator = "hidden" },
          bindings = {
            { keys = "ctrl+p", action = "shell.openCommandPalette" },
          },
          status_line = { segments = { "session", "context", "runnerload" } },
          mobile = {
            mode = "corner",
            hold_ms = 320,
            corners = {
              top_left = { tap = "navigator.open", hold = "menu.projects" },
              top_right = { tap = "view.openFiles", hold = "menu.tools" },
              bottom_left = { tap = "session.openPrevious", hold = "menu.sessionMru" },
              bottom_right = { tap = "chat.send", hold = "menu.chatActions" },
            },
          },
        })
    "#;

    #[test]
    fn evaluates_typed_config() {
        let config = evaluate(VALID).expect("valid config");
        assert_eq!(config.preset, "instrument-panel");
        assert_eq!(config.appearance.radius, Radius::Soft);
        assert_eq!(config.layout.files, FilesPlacement::Left);
        assert_eq!(config.mobile.mode, MobileMode::Corner);
        assert_eq!(config.mobile.hold_ms, 320);
        assert_eq!(
            config.status_line.segments,
            vec![
                StatusSegment::Session,
                StatusSegment::Context,
                StatusSegment::RunnerLoad,
            ]
        );
    }

    #[test]
    fn filesystem_process_and_package_access_are_absent() {
        for source in [
            "local x = io.open('/etc/passwd'); oqto.setup(x)",
            "local x = os.getenv('HOME'); oqto.setup(x)",
            "local x = require('socket'); oqto.setup(x)",
            "local x = dofile('/etc/passwd'); oqto.setup(x)",
            "local x = loadfile('/etc/passwd'); oqto.setup(x)",
        ] {
            evaluate(source).expect_err("ambient-authority global must not exist");
        }
    }

    #[test]
    fn enforces_memory_budget() {
        let error = evaluate("local x = string.rep('x', 3000000); oqto.setup(x)")
            .expect_err("memory budget must reject oversized allocation");
        assert!(error.to_string().contains("memory"));
    }

    #[test]
    fn stops_unbounded_execution() {
        let error = evaluate("while true do end").expect_err("instruction budget must stop loop");
        assert!(error.to_string().contains("instruction budget exceeded"));
    }

    #[test]
    fn rejects_unknown_fields() {
        let error = evaluate("oqto.setup({ version = 1, fetch = 'https://example.com' })")
            .expect_err("unknown field must fail");
        assert!(error.to_string().contains("unknown field `fetch`"));
    }

    #[test]
    fn broken_user_layer_falls_back_with_diagnostic() {
        let result = resolve_user_source(Some("this is not Lua"));
        assert_eq!(result.source, ConfigSource::UserLuaFallback);
        assert_eq!(result.config, OqtoUiConfigV1::default());
        assert_eq!(result.diagnostics[0].code, "customization.user.invalid");
    }
}
