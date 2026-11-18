//! `config.toml` at `$XDG_CONFIG_HOME/oqt-tui/` with a JSON schema.
//!
//! Priority, highest first: command-line arguments, a config file named on the command line,
//! environment variables (`OQT_TUI_*`), a local `.oqt-tui.toml` in the working directory, the
//! global file. The global file is created with defaults on first run.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::keys;

/// Application name, used for the XDG directories.
pub const APP: &str = "oqt-tui";
/// The local override file in the working directory.
pub const LOCAL_FILE: &str = ".oqt-tui.toml";

/// The whole configuration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    /// Name of the entry in `hosts` to attach to.
    pub host: String,
    /// Hosts this client knows.
    pub hosts: Vec<HostConfig>,
    /// Key bindings.
    pub keys: KeysConfig,
    /// What the transcript shows.
    pub display: DisplayConfig,
    /// Terminal features; `auto` decides from the terminal's replies, never from `TERM`.
    pub terminal: TerminalConfig,
    /// The encrypted local cache.
    pub cache: CacheConfig,
    /// Connection settings.
    pub connection: ConnectionConfig,
}

/// One host speaking `oqto.session-view`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct HostConfig {
    /// Name used by `host` and `--host`.
    pub name: String,
    /// WebSocket URL of the host's `/session` path (`ws://127.0.0.1:PORT/session`).
    pub url: String,
    /// Session id to attach to.
    pub session: String,
    /// Environment variable holding the bearer secret (never put the secret here).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secret_env: Option<String>,
    /// File holding the bearer secret (must not be readable by others).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secret_file: Option<PathBuf>,
}

/// Key bindings by action id. Every binding is configurable; missing ids keep their default.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct KeysConfig {
    /// The leader key that starts key progressions (`leader s`). Default `ctrl+space`.
    pub leader: String,
    /// Action id to key paths, for example `"oqt.palette" = ["leader :", "ctrl+k"]`.
    /// A path is keys separated by spaces; `leader` stands for the leader key.
    pub bindings: BTreeMap<String, Vec<String>>,
}

impl Default for KeysConfig {
    fn default() -> Self {
        Self {
            leader: "ctrl+space".to_owned(),
            bindings: keys::default_bindings(),
        }
    }
}

/// How thinking blocks show.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Thinking {
    /// One line with its length; `ctrl+t` expands.
    Collapsed,
    /// Full text.
    Shown,
    /// Not shown.
    Hidden,
}

/// Display settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct DisplayConfig {
    /// Thinking blocks.
    pub thinking: Thinking,
    /// Lines of tool output shown on a collapsed tool card.
    pub tool_lines: u16,
}

impl Default for DisplayConfig {
    fn default() -> Self {
        Self {
            thinking: Thinking::Collapsed,
            tool_lines: 3,
        }
    }
}

/// `auto` follows the terminal's replies; `on` forces; `off` disables.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Toggle {
    /// From the probe.
    Auto,
    /// Always.
    On,
    /// Never.
    Off,
}

/// Image output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Images {
    /// Kitty graphics when the terminal replied; Unicode placeholders inside tmux; else text.
    Auto,
    /// Direct Kitty placements.
    Kitty,
    /// Kitty Unicode placeholders.
    Placeholders,
    /// Text only.
    Off,
}

/// Terminal features.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct TerminalConfig {
    /// Images.
    pub images: Images,
    /// OSC 7501 program status.
    pub status: Toggle,
    /// herdr `report-agent` when `HERDR_ENV` is set.
    pub herdr: Toggle,
    /// Synchronized output (DEC mode 2026).
    pub synchronized_output: Toggle,
    /// Kitty keyboard protocol.
    pub kitty_keyboard: Toggle,
    /// How long the start-up probe waits for replies, in milliseconds.
    pub probe_timeout_ms: u64,
}

impl Default for TerminalConfig {
    fn default() -> Self {
        Self {
            images: Images::Auto,
            status: Toggle::Auto,
            herdr: Toggle::Auto,
            synchronized_output: Toggle::Auto,
            kitty_keyboard: Toggle::Auto,
            probe_timeout_ms: 150,
        }
    }
}

/// Where the cache key lives.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CacheKeyStore {
    /// The OS keychain, falling back to a 0600 file.
    Auto,
    /// A 0600 file next to the cache.
    File,
}

/// The encrypted cache.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct CacheConfig {
    /// Keep a cache so a Session shows at once.
    pub enabled: bool,
    /// Where the encryption key lives.
    pub key: CacheKeyStore,
    /// Entries kept per Session; older pages are read again on demand.
    pub max_entries: usize,
    /// Cache directory; default `$XDG_CACHE_HOME/oqt-tui/sessions`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dir: Option<PathBuf>,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            key: CacheKeyStore::Auto,
            max_entries: 2000,
            dir: None,
        }
    }
}

/// Connection settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct ConnectionConfig {
    /// Connect timeout in milliseconds.
    pub connect_timeout_ms: u64,
    /// Entries per `read_entries` page.
    pub page_size: u64,
    /// Window size asked for on attach; the host's default when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub window_min: Option<u64>,
}

impl Default for ConnectionConfig {
    fn default() -> Self {
        Self {
            connect_timeout_ms: 5000,
            page_size: 50,
            window_min: None,
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            host: "local".to_owned(),
            hosts: vec![HostConfig {
                name: "local".to_owned(),
                url: "ws://127.0.0.1:8787/session".to_owned(),
                session: "ses_chat".to_owned(),
                secret_env: Some("BYOAGNT_SECRET".to_owned()),
                secret_file: None,
            }],
            keys: KeysConfig::default(),
            display: DisplayConfig::default(),
            terminal: TerminalConfig::default(),
            cache: CacheConfig::default(),
            connection: ConnectionConfig::default(),
        }
    }
}

/// Overrides from the command line (highest priority).
#[derive(Debug, Clone, Default)]
pub struct Overrides {
    /// `--config PATH`.
    pub config: Option<PathBuf>,
    /// `--host NAME`.
    pub host: Option<String>,
    /// `--url URL`.
    pub url: Option<String>,
    /// `--session ID`.
    pub session: Option<String>,
    /// `--images MODE`.
    pub images: Option<Images>,
}

/// The JSON schema of `config.toml`.
pub fn schema_json() -> Result<String> {
    let schema = schemars::schema_for!(Config);
    Ok(serde_json::to_string_pretty(&schema)? + "\n")
}

/// `$XDG_CONFIG_HOME/oqt-tui` (or `~/.config/oqt-tui`).
pub fn config_dir() -> Result<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| dirs::home_dir().map(|h| h.join(".config")))
        .context("no home directory")?;
    Ok(base.join(APP))
}

/// `$XDG_CACHE_HOME/oqt-tui` (or `~/.cache/oqt-tui`).
pub fn cache_dir() -> Result<PathBuf> {
    let base = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| dirs::home_dir().map(|h| h.join(".cache")))
        .context("no home directory")?;
    Ok(base.join(APP))
}

/// The global config file path.
pub fn global_path() -> Result<PathBuf> {
    Ok(config_dir()?.join("config.toml"))
}

/// Write the defaults and the schema when no global file exists yet.
pub fn ensure_global() -> Result<PathBuf> {
    let path = global_path()?;
    if path.exists() {
        return Ok(path);
    }
    let dir = path.parent().context("config dir")?;
    fs::create_dir_all(dir).with_context(|| format!("create {}", dir.display()))?;
    fs::write(dir.join("config.schema.json"), schema_json()?).context("write config schema")?;
    let body = toml::to_string_pretty(&Config::default()).context("encode defaults")?;
    let text = format!(
        "#:schema ./config.schema.json\n# oqt-tui configuration. Created with defaults on first run.\n\
         # Never put a host secret here: use secret_env or secret_file.\n\n{body}"
    );
    fs::write(&path, text).with_context(|| format!("write {}", path.display()))?;
    Ok(path)
}

fn read_table(path: &Path) -> Result<Option<toml::Table>> {
    match fs::read_to_string(path) {
        Ok(text) => toml::from_str(&text)
            .map(Some)
            .with_context(|| format!("parse {}", path.display())),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error).with_context(|| format!("read {}", path.display())),
    }
}

/// Deep merge: tables merge key by key, everything else is replaced.
fn merge(base: &mut toml::Table, top: toml::Table) {
    for (key, value) in top {
        match (base.get_mut(&key), value) {
            (Some(toml::Value::Table(left)), toml::Value::Table(right)) => merge(left, right),
            (_, value) => {
                base.insert(key, value);
            }
        }
    }
}

fn set(table: &mut toml::Table, path: &[&str], value: toml::Value) {
    let Some((last, parents)) = path.split_last() else {
        return;
    };
    let mut node = table;
    for key in parents {
        let entry = node
            .entry((*key).to_owned())
            .or_insert_with(|| toml::Value::Table(toml::Table::new()));
        let toml::Value::Table(next) = entry else {
            return;
        };
        node = next;
    }
    node.insert((*last).to_owned(), value);
}

/// Environment overrides as a table: `OQT_TUI_HOST`, `OQT_TUI_LEADER`, `OQT_TUI_IMAGES`,
/// `OQT_TUI_STATUS`, `OQT_TUI_HERDR`.
fn env_table(env: &dyn Fn(&str) -> Option<String>) -> toml::Table {
    let mut table = toml::Table::new();
    let pairs: [(&str, &[&str]); 5] = [
        ("OQT_TUI_HOST", &["host"]),
        ("OQT_TUI_LEADER", &["keys", "leader"]),
        ("OQT_TUI_IMAGES", &["terminal", "images"]),
        ("OQT_TUI_STATUS", &["terminal", "status"]),
        ("OQT_TUI_HERDR", &["terminal", "herdr"]),
    ];
    for (name, path) in pairs {
        if let Some(value) = env(name).filter(|v| !v.is_empty()) {
            set(&mut table, path, toml::Value::String(value));
        }
    }
    table
}

/// Load with the documented priority. `global` and `local` are the file paths to read.
pub fn load_from(
    global: &Path,
    local: &Path,
    env: &dyn Fn(&str) -> Option<String>,
    overrides: &Overrides,
) -> Result<Config> {
    let mut table = toml::Table::try_from(Config::default()).context("defaults")?;
    for layer in [
        read_table(global)?,
        read_table(local)?,
        Some(env_table(env)),
    ]
    .into_iter()
    .flatten()
    {
        merge(&mut table, layer);
    }
    if let Some(path) = &overrides.config {
        let layer =
            read_table(path)?.with_context(|| format!("{} does not exist", path.display()))?;
        merge(&mut table, layer);
    }
    let mut config: Config = toml::Value::Table(table)
        .try_into()
        .context("invalid configuration")?;
    // Bindings merge per action: a file that sets one action keeps every other default.
    let mut bindings = keys::default_bindings();
    bindings.extend(config.keys.bindings);
    config.keys.bindings = bindings;
    if let Some(host) = &overrides.host {
        config.host.clone_from(host);
    }
    if let Some(images) = overrides.images {
        config.terminal.images = images;
    }
    if overrides.url.is_some() || overrides.session.is_some() {
        let mut host = config
            .selected_host()
            .cloned()
            .unwrap_or_else(|| HostConfig {
                name: "cli".to_owned(),
                url: String::new(),
                session: String::new(),
                secret_env: Some("BYOAGNT_SECRET".to_owned()),
                secret_file: None,
            });
        if let Some(url) = &overrides.url {
            host.url.clone_from(url);
        }
        if let Some(session) = &overrides.session {
            host.session.clone_from(session);
        }
        host.name = "cli".to_owned();
        config.hosts.retain(|h| h.name != "cli");
        config.hosts.push(host);
        config.host = "cli".to_owned();
    }
    Ok(config)
}

/// Load from the real global and local paths and the process environment.
pub fn load(overrides: &Overrides) -> Result<Config> {
    let global = ensure_global()?;
    let local = std::env::current_dir()
        .context("working directory")?
        .join(LOCAL_FILE);
    load_from(&global, &local, &|name| std::env::var(name).ok(), overrides)
}

impl Config {
    /// The host named by `host`.
    pub fn selected_host(&self) -> Option<&HostConfig> {
        self.hosts.iter().find(|h| h.name == self.host)
    }
}

impl HostConfig {
    /// Read the bearer secret from its file or environment variable; never from argv.
    pub fn secret(&self, env: &dyn Fn(&str) -> Option<String>) -> Result<String> {
        if let Some(path) = &self.secret_file {
            check_private(path)?;
            let text =
                fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
            return Ok(text.trim().to_owned());
        }
        let name = self.secret_env.as_deref().unwrap_or("BYOAGNT_SECRET");
        env(name)
            .filter(|s| !s.is_empty())
            .with_context(|| format!("set the host secret in ${name} (or secret_file)"))
    }
}

#[cfg(unix)]
fn check_private(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mode = fs::metadata(path)
        .with_context(|| format!("stat {}", path.display()))?
        .permissions()
        .mode();
    if mode & 0o077 != 0 {
        bail!("{} is readable by others; chmod 600 it", path.display());
    }
    Ok(())
}

#[cfg(not(unix))]
fn check_private(_path: &Path) -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn none(_: &str) -> Option<String> {
        None
    }

    #[test]
    fn priority_cli_then_config_file_then_env_then_local_then_global() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let global = temp.path().join("global.toml");
        let local = temp.path().join("local.toml");
        let named = temp.path().join("named.toml");
        fs::write(
            &global,
            "host = \"g\"\n[keys]\nleader = \"ctrl+a\"\n[display]\ntool_lines = 7\n",
        )?;
        let defaults = load_from(&global, &local, &none, &Overrides::default())?;
        assert_eq!(
            (
                defaults.host.as_str(),
                defaults.keys.leader.as_str(),
                defaults.display.tool_lines
            ),
            ("g", "ctrl+a", 7)
        );
        fs::write(&local, "host = \"l\"\n")?;
        let env = |name: &str| (name == "OQT_TUI_HOST").then(|| "e".to_owned());
        assert_eq!(
            load_from(&global, &local, &none, &Overrides::default())?.host,
            "l"
        );
        assert_eq!(
            load_from(&global, &local, &env, &Overrides::default())?.host,
            "e"
        );
        fs::write(&named, "host = \"n\"\n")?;
        let with_file = Overrides {
            config: Some(named.clone()),
            ..Overrides::default()
        };
        assert_eq!(load_from(&global, &local, &env, &with_file)?.host, "n");
        let with_cli = Overrides {
            config: Some(named),
            host: Some("c".into()),
            ..Overrides::default()
        };
        let config = load_from(&global, &local, &env, &with_cli)?;
        assert_eq!(config.host, "c");
        // Untouched layers keep lower values and per-action defaults.
        assert_eq!(config.display.tool_lines, 7);
        assert!(config.keys.bindings.contains_key("oqt.palette"));
        Ok(())
    }

    #[test]
    fn one_binding_keeps_the_other_defaults() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let global = temp.path().join("g.toml");
        fs::write(&global, "[keys.bindings]\n\"oqt.palette\" = [\"ctrl+k\"]\n")?;
        let config = load_from(
            &global,
            &temp.path().join("none"),
            &none,
            &Overrides::default(),
        )?;
        assert_eq!(
            config.keys.bindings["oqt.palette"],
            vec!["ctrl+k".to_owned()]
        );
        assert_eq!(
            config.keys.bindings["app.message.followUp"],
            vec!["alt+enter".to_owned()]
        );
        Ok(())
    }

    #[test]
    fn unknown_fields_are_refused() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let global = temp.path().join("g.toml");
        fs::write(&global, "[display]\ncolour = \"red\"\n")?;
        assert!(
            load_from(
                &global,
                &temp.path().join("none"),
                &none,
                &Overrides::default()
            )
            .is_err()
        );
        Ok(())
    }

    #[test]
    fn defaults_round_trip_through_toml() -> Result<()> {
        let text = toml::to_string_pretty(&Config::default())?;
        let back: Config = toml::from_str(&text)?;
        assert_eq!(back, Config::default());
        Ok(())
    }

    #[test]
    fn committed_schema_is_current() -> Result<()> {
        let committed = include_str!("../config.schema.json");
        assert_eq!(
            committed,
            schema_json()?,
            "run `oqt-tui config schema > crates/oqt-tui/config.schema.json`"
        );
        Ok(())
    }
}
