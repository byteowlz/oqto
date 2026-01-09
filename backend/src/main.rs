use std::env;
use std::fmt;
use std::fs;
use std::io::{self, IsTerminal, Write};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow};
use clap::{Args, CommandFactory, Parser, Subcommand, ValueEnum};
use clap_complete::Shell;
use config::{Config, Environment, File, FileFormat};

use log::{LevelFilter, debug, error, info, warn};
use serde::{Deserialize, Serialize};
use tokio::net::TcpListener;

mod agent;
mod agent_rpc;
mod api;
mod auth;
mod container;
mod db;
mod eavs;
mod history;
mod invite;
mod local;
mod main_chat;
mod markdown;
mod observability;
mod pi;
mod session;
mod session_ui;
mod settings;
mod user;
mod wordlist;
mod ws;

const APP_NAME: &str = "octo";

use crate::session_ui::SessionAutoAttachMode;

fn main() {
    if let Err(err) = try_main() {
        let _ = writeln!(io::stderr(), "{err:?}");
        std::process::exit(1);
    }
}

#[tokio::main]
async fn async_main(ctx: RuntimeContext, cmd: ServeCommand) -> Result<()> {
    handle_serve(&ctx, cmd).await
}

#[tokio::main]
async fn async_invite_codes(ctx: RuntimeContext, cmd: InviteCodesCommand) -> Result<()> {
    handle_invite_codes(&ctx, cmd).await
}

fn try_main() -> Result<()> {
    let cli = Cli::parse();

    let mut ctx = RuntimeContext::new(cli.common.clone())?;
    ctx.init_logging()?;
    debug!("resolved paths: {:#?}", ctx.paths);

    match cli.command {
        Command::Serve(cmd) => async_main(ctx, cmd),
        Command::Run(cmd) => handle_run(&mut ctx, cmd),
        Command::Init(cmd) => handle_init(&ctx, cmd),
        Command::Config { command } => handle_config(&ctx, command),
        Command::InviteCodes { command } => async_invite_codes(ctx, command),
        Command::Completions { shell } => handle_completions(shell),
    }
}

#[derive(Debug, Parser)]
#[command(
    author,
    version,
    about = "Octo - AI Agent Workspace Platform server.",
    propagate_version = true
)]
struct Cli {
    #[command(flatten)]
    common: CommonOpts,
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Clone, Args)]
struct CommonOpts {
    /// Override the config file path
    #[arg(long, value_name = "PATH", global = true)]
    config: Option<PathBuf>,
    /// Reduce output to only errors
    #[arg(short, long, action = clap::ArgAction::SetTrue, global = true)]
    quiet: bool,
    /// Increase logging verbosity (stackable)
    #[arg(short = 'v', long = "verbose", action = clap::ArgAction::Count, global = true)]
    verbose: u8,
    /// Enable debug logging (equivalent to -vv)
    #[arg(long, global = true)]
    debug: bool,
    /// Enable trace logging (overrides other levels)
    #[arg(long, global = true)]
    trace: bool,
    /// Output machine readable JSON
    #[arg(long, global = true, conflicts_with = "yaml")]
    json: bool,
    /// Output machine readable YAML
    #[arg(long, global = true)]
    yaml: bool,
    /// Disable ANSI colors in output
    #[arg(long = "no-color", global = true, conflicts_with = "color")]
    no_color: bool,
    /// Control color output (auto, always, never)
    #[arg(long, value_enum, default_value_t = ColorOption::Auto, global = true)]
    color: ColorOption,
    /// Do not change anything on disk
    #[arg(long = "dry-run", global = true)]
    dry_run: bool,
    /// Assume "yes" for interactive prompts
    #[arg(short = 'y', long = "yes", alias = "force", global = true)]
    assume_yes: bool,
    /// Maximum seconds to allow an operation to run
    #[arg(long = "timeout", value_name = "SECONDS", global = true)]
    timeout: Option<u64>,
    /// Override the degree of parallelism
    #[arg(long = "parallel", value_name = "N", global = true)]
    parallel: Option<usize>,
    /// Disable progress indicators
    #[arg(long = "no-progress", global = true)]
    no_progress: bool,
    /// Emit additional diagnostics for troubleshooting
    #[arg(long = "diagnostics", global = true)]
    diagnostics: bool,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum ColorOption {
    Auto,
    Always,
    Never,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Start the HTTP API server
    Serve(ServeCommand),
    /// Execute the CLI's primary behavior
    Run(RunCommand),
    /// Create config directories and default files
    Init(InitCommand),
    /// Inspect and manage configuration
    Config {
        #[command(subcommand)]
        command: ConfigCommand,
    },
    /// Manage invite codes for user registration
    InviteCodes {
        #[command(subcommand)]
        command: InviteCodesCommand,
    },
    /// Generate shell completions
    Completions {
        #[arg(value_enum)]
        shell: Shell,
    },
}

#[derive(Debug, Clone, Args)]
struct ServeCommand {
    /// Host address to bind to
    #[arg(long, default_value = "0.0.0.0")]
    host: String,
    /// Port to listen on
    #[arg(short, long, default_value = "8080")]
    port: u16,
    /// Default container image
    #[arg(long, default_value = "octo-dev:latest")]
    image: String,
    /// Base port for session allocation
    #[arg(long, default_value = "41820")]
    base_port: u16,
    /// Base directory for user data (home directories)
    #[arg(long, default_value = "./data", value_name = "PATH")]
    user_data_path: PathBuf,
    /// Path to skeleton directory for new user homes
    #[arg(long, value_name = "PATH")]
    skel_path: Option<PathBuf>,
    /// Run in local mode (no containers, spawn processes directly)
    #[arg(long = "local-mode")]
    local_mode: bool,
}

#[derive(Debug, Clone, Args)]
struct RunCommand {
    /// Named task to execute
    #[arg(value_name = "TASK", default_value = "default")]
    task: String,
    /// Override the profile to run under
    #[arg(long, value_name = "PROFILE")]
    profile: Option<String>,
}

#[derive(Debug, Clone, Args)]
struct InitCommand {
    /// Recreate configuration even if it already exists
    #[arg(long = "force")]
    force: bool,
}

#[derive(Debug, Subcommand)]
enum ConfigCommand {
    /// Output the effective configuration
    Show,
    /// Print the resolved config file path
    Path,
    /// Regenerate the default configuration file
    Reset,
}

#[derive(Debug, Subcommand)]
enum InviteCodesCommand {
    /// Generate new invite codes
    Generate(InviteCodesGenerateCommand),
    /// List existing invite codes
    List(InviteCodesListCommand),
    /// Revoke an invite code
    Revoke(InviteCodesRevokeCommand),
}

#[derive(Debug, Clone, Args)]
struct InviteCodesGenerateCommand {
    /// Number of codes to generate
    #[arg(short, long, default_value = "1")]
    count: u32,
    /// Number of uses per code
    #[arg(short = 'u', long, default_value = "1")]
    uses_per_code: i32,
    /// Expiration time (e.g., "7d", "24h", "30m")
    #[arg(short, long)]
    expires_in: Option<String>,
    /// Prefix for generated codes
    #[arg(short, long)]
    prefix: Option<String>,
    /// Note/label for the codes
    #[arg(short, long)]
    note: Option<String>,
    /// Admin user ID creating the codes
    #[arg(long, default_value = "usr_admin")]
    admin_id: String,
}

#[derive(Debug, Clone, Args)]
struct InviteCodesListCommand {
    /// Filter by validity (valid, invalid, all)
    #[arg(short, long, default_value = "all")]
    filter: String,
    /// Maximum number of codes to list
    #[arg(short, long, default_value = "100")]
    limit: i64,
}

#[derive(Debug, Clone, Args)]
struct InviteCodesRevokeCommand {
    /// ID of the invite code to revoke
    code_id: String,
}

#[derive(Debug, Clone)]
struct RuntimeContext {
    common: CommonOpts,
    paths: AppPaths,
    config: AppConfig,
}

impl RuntimeContext {
    fn new(common: CommonOpts) -> Result<Self> {
        let mut paths = AppPaths::discover(common.config.clone())?;
        let config = load_or_init_config(&mut paths, &common)?;
        let paths = paths.apply_overrides(&config)?;
        let ctx = Self {
            common,
            paths,
            config,
        };
        ctx.ensure_directories()?;
        Ok(ctx)
    }

    fn init_logging(&self) -> Result<()> {
        use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

        if self.common.quiet {
            log::set_max_level(LevelFilter::Off);
            return Ok(());
        }

        // Determine filter level
        let level = match self.effective_log_level() {
            LevelFilter::Off => "off",
            LevelFilter::Error => "error",
            LevelFilter::Warn => "warn",
            LevelFilter::Info => "info",
            LevelFilter::Debug => "debug",
            LevelFilter::Trace => "trace",
        };

        let env_filter = EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| EnvFilter::new(format!("octo={level},tower_http={level}")));

        // Use JSON output if --json flag is set, otherwise pretty format
        if self.common.json {
            tracing_subscriber::registry()
                .with(env_filter)
                .with(tracing_subscriber::fmt::layer().json())
                .try_init()
                .ok();
        } else {
            let force_color = matches!(self.common.color, ColorOption::Always)
                || env::var_os("FORCE_COLOR").is_some();
            let disable_color = self.common.no_color
                || matches!(self.common.color, ColorOption::Never)
                || env::var_os("NO_COLOR").is_some()
                || (!force_color && !io::stderr().is_terminal());

            tracing_subscriber::registry()
                .with(env_filter)
                .with(
                    tracing_subscriber::fmt::layer()
                        .with_ansi(!disable_color)
                        .with_target(self.common.diagnostics)
                        .with_file(self.common.diagnostics)
                        .with_line_number(self.common.diagnostics),
                )
                .try_init()
                .ok();
        }

        // Also init env_logger for compatibility with log crate users
        let mut builder =
            env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"));
        builder.filter_level(self.effective_log_level());
        builder.try_init().ok();

        Ok(())
    }

    fn effective_log_level(&self) -> LevelFilter {
        if self.common.trace {
            LevelFilter::Trace
        } else if self.common.debug {
            LevelFilter::Debug
        } else {
            match self.common.verbose {
                0 => LevelFilter::Info,
                1 => LevelFilter::Debug,
                _ => LevelFilter::Trace,
            }
        }
    }

    fn ensure_directories(&self) -> Result<()> {
        if self.common.dry_run {
            info!(
                "dry-run: would ensure data dir {} and state dir {}",
                self.paths.data_dir.display(),
                self.paths.state_dir.display()
            );
            return Ok(());
        }

        fs::create_dir_all(&self.paths.data_dir).with_context(|| {
            format!("creating data directory {}", self.paths.data_dir.display())
        })?;
        fs::create_dir_all(&self.paths.state_dir).with_context(|| {
            format!(
                "creating state directory {}",
                self.paths.state_dir.display()
            )
        })?;
        Ok(())
    }
}

#[derive(Debug, Clone)]
struct AppPaths {
    config_file: PathBuf,
    data_dir: PathBuf,
    state_dir: PathBuf,
}

impl AppPaths {
    fn discover(override_path: Option<PathBuf>) -> Result<Self> {
        let config_file = match override_path {
            Some(path) => {
                let expanded = expand_path(path)?;
                if expanded.is_dir() {
                    expanded.join("config.toml")
                } else {
                    expanded
                }
            }
            None => default_config_dir()?.join("config.toml"),
        };

        if config_file.parent().is_none() {
            return Err(anyhow!("invalid config file path: {config_file:?}"));
        }

        let data_dir = default_data_dir()?;
        let state_dir = default_state_dir()?;

        Ok(Self {
            config_file,
            data_dir,
            state_dir,
        })
    }

    fn apply_overrides(mut self, cfg: &AppConfig) -> Result<Self> {
        if let Some(ref data_override) = cfg.paths.data_dir {
            self.data_dir = expand_str_path(data_override)?;
        }
        if let Some(ref state_override) = cfg.paths.state_dir {
            self.state_dir = expand_str_path(state_override)?;
        }
        Ok(self)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
struct AppConfig {
    profile: String,
    logging: LoggingConfig,
    runtime: RuntimeConfig,
    paths: PathsConfig,
    /// Backend configuration (mode selection).
    backend: BackendConfig,
    container: ContainerRuntimeConfig,
    local: LocalModeConfig,
    eavs: Option<EavsConfig>,
    mmry: MmryConfig,
    voice: VoiceConfig,
    sessions: SessionUiConfig,
    auth: auth::AuthConfig,
    /// Agent scaffolding configuration.
    scaffold: ScaffoldConfig,
    /// Pi agent configuration for Main Chat.
    pi: PiConfig,
}

/// Backend mode selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum BackendMode {
    /// Local mode - opencode runs as native process
    Local,
    /// Container mode - opencode runs in Docker/Podman container
    #[default]
    Container,
    /// Auto mode - prefers local if configured, falls back to container
    Auto,
}

/// Backend configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
struct BackendConfig {
    /// Backend mode: "local", "container", or "auto"
    mode: BackendMode,
    /// Use the new AgentRPC abstraction (experimental)
    use_agent_rpc: bool,
}

impl Default for BackendConfig {
    fn default() -> Self {
        Self {
            mode: BackendMode::Container,
            use_agent_rpc: false,
        }
    }
}

impl AppConfig {
    fn with_profile_override(mut self, profile: Option<String>) -> Self {
        if let Some(profile) = profile {
            self.profile = profile;
        }
        self
    }
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            profile: "default".to_string(),
            logging: LoggingConfig::default(),
            runtime: RuntimeConfig::default(),
            paths: PathsConfig::default(),
            backend: BackendConfig::default(),
            container: ContainerRuntimeConfig::default(),
            local: LocalModeConfig::default(),
            eavs: None,
            mmry: MmryConfig::default(),
            voice: VoiceConfig::default(),
            sessions: SessionUiConfig::default(),
            auth: auth::AuthConfig::default(),
            scaffold: ScaffoldConfig::default(),
            pi: PiConfig::default(),
        }
    }
}

/// EAVS (LLM proxy) configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct EavsConfig {
    /// Whether EAVS integration is enabled.
    #[serde(default = "default_true")]
    enabled: bool,
    /// URL of the EAVS server (e.g., "http://localhost:41800").
    #[serde(default = "default_eavs_base_url")]
    base_url: String,
    /// URL for containers to reach EAVS (e.g., "http://host.docker.internal:41800").
    container_url: Option<String>,
    /// Master key for EAVS admin operations.
    master_key: Option<String>,
    /// Default session budget limit in USD.
    default_session_budget_usd: Option<f64>,
    /// Default session rate limit in requests per minute.
    default_session_rpm: Option<u32>,
}

/// Voice mode configuration.
///
/// Enables real-time speech-to-text (STT) and text-to-speech (TTS) integration.
/// Uses external WebSocket services:
/// - eaRS for STT (speech-to-text with VAD)
/// - kokorox for TTS (text-to-speech with streaming)
///
/// Both services can run on any machine - clients connect directly via WebSocket.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct VoiceConfig {
    /// Whether voice mode is enabled.
    pub enabled: bool,
    /// WebSocket URL for the eaRS STT service.
    /// Default: "ws://localhost:8765"
    pub stt_url: String,
    /// WebSocket URL for the kokorox TTS service.
    /// Default: "ws://localhost:8766"
    pub tts_url: String,
    /// Voice Activity Detection timeout in milliseconds.
    /// After this duration of silence, the transcript is auto-sent.
    /// Default: 1500ms
    pub vad_timeout_ms: u32,
    /// Default kokorox voice ID.
    /// Default: "af_heart"
    pub default_voice: String,
    /// Default TTS speech speed (0.1 - 3.0).
    /// Default: 1.0
    pub default_speed: f32,
    /// Enable automatic language detection for TTS.
    /// Default: true
    pub auto_language_detect: bool,
    /// Whether TTS output is muted by default (user can still read responses).
    /// Default: false
    pub tts_muted: bool,
    /// Continuous conversation mode - auto-listen after TTS finishes.
    /// Default: true
    pub continuous_mode: bool,
    /// Default visualizer style: "orb" or "kitt"
    /// Default: "orb"
    pub default_visualizer: String,
    /// Minimum words spoken by user to interrupt TTS playback.
    /// Set to 0 to disable interrupt-by-speaking.
    /// Default: 2
    pub interrupt_word_count: u32,
    /// Reset interrupt word count after this silence duration in ms.
    /// Set to 0 to disable backoff (words accumulate forever until threshold).
    /// Default: 5000
    pub interrupt_backoff_ms: u32,
    /// Per-visualizer voice/speed settings.
    /// Keys are visualizer IDs (e.g., "orb", "kitt"), values are VisualizerVoice.
    #[serde(default)]
    pub visualizer_voices: std::collections::HashMap<String, VisualizerVoice>,
}

/// Per-visualizer voice settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VisualizerVoice {
    /// Voice ID for this visualizer.
    pub voice: String,
    /// Speech speed for this visualizer (0.1 - 3.0).
    #[serde(default = "default_speed")]
    pub speed: f32,
}

fn default_speed() -> f32 {
    1.0
}

impl Default for VoiceConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            stt_url: "ws://localhost:8765".to_string(),
            tts_url: "ws://localhost:8766".to_string(),
            vad_timeout_ms: 1500,
            default_voice: "af_heart".to_string(),
            default_speed: 1.0,
            auto_language_detect: true,
            tts_muted: false,
            continuous_mode: true,
            default_visualizer: "orb".to_string(),
            interrupt_word_count: 2,
            interrupt_backoff_ms: 5000,
            visualizer_voices: [
                (
                    "orb".to_string(),
                    VisualizerVoice {
                        voice: "af_heart".to_string(),
                        speed: 1.0,
                    },
                ),
                (
                    "kitt".to_string(),
                    VisualizerVoice {
                        voice: "am_michael".to_string(),
                        speed: 1.1,
                    },
                ),
            ]
            .into_iter()
            .collect(),
        }
    }
}

/// Session UX configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
struct SessionUiConfig {
    /// Auto-attach to a running session (or resume/start if configured).
    auto_attach: SessionAutoAttachMode,
    /// Scan running sessions for the selected chat session ID.
    auto_attach_scan: bool,
    /// Maximum concurrent running sessions per user.
    max_concurrent_sessions: i64,
    /// Idle timeout in minutes before stopping a session.
    idle_timeout_minutes: i64,
    /// Idle cleanup check interval in seconds.
    idle_check_interval_seconds: u64,
}

impl Default for SessionUiConfig {
    fn default() -> Self {
        Self {
            auto_attach: SessionAutoAttachMode::Off,
            auto_attach_scan: false,
            max_concurrent_sessions: session::SessionService::DEFAULT_MAX_CONCURRENT_SESSIONS,
            idle_timeout_minutes: session::SessionService::DEFAULT_IDLE_TIMEOUT_MINUTES,
            idle_check_interval_seconds: 5 * 60,
        }
    }
}

/// mmry (memory system) configuration.
///
/// Supports two modes:
/// 1. Single-user local: Proxy to user's existing mmry service (no process management)
/// 2. Multi-user: Per-user mmry instances with isolated databases and ports
///
/// In multi-user mode, a hub-spoke architecture is used where a central host service
/// handles embeddings/reranking while per-user lean instances maintain isolated databases.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct MmryConfig {
    /// Whether mmry integration is enabled.
    pub enabled: bool,
    /// URL of the user's local mmry service for single-user mode.
    /// In single-user local mode, we proxy directly to this URL.
    /// Default: "http://localhost:8081"
    pub local_service_url: String,
    /// URL of the central mmry service for embeddings in multi-user mode.
    /// This service handles heavy embedding/reranking operations for all users.
    /// Per-user instances delegate embeddings to this service.
    pub host_service_url: String,
    /// API key for authenticating with the host mmry service.
    pub host_api_key: Option<String>,
    /// Default embedding model name.
    pub default_model: String,
    /// Embedding dimension (must match the model).
    pub dimension: u16,
    /// Path to mmry binary (for spawning per-user instances in multi-user mode).
    pub binary: String,
    /// URL for containers to reach the host mmry service.
    /// e.g., "http://host.docker.internal:8081" or "http://host.containers.internal:8081"
    pub container_url: Option<String>,
}

impl Default for MmryConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            local_service_url: "http://localhost:8081".to_string(),
            host_service_url: "http://localhost:8081".to_string(),
            host_api_key: None,
            default_model: "nomic-ai/nomic-embed-text-v1.5".to_string(),
            dimension: 768,
            binary: "mmry".to_string(),
            container_url: None,
        }
    }
}

fn default_true() -> bool {
    true
}

fn default_eavs_base_url() -> String {
    "http://localhost:41800".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
struct LoggingConfig {
    level: String,
    file: Option<String>,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: "info".to_string(),
            file: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
struct RuntimeConfig {
    parallelism: Option<usize>,
    timeout: Option<u64>,
    fail_fast: bool,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            parallelism: None,
            timeout: Some(60),
            fail_fast: true,
        }
    }
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(default)]
struct PathsConfig {
    data_dir: Option<String>,
    state_dir: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
struct ContainerRuntimeConfig {
    /// Container runtime type: "docker" or "podman" (auto-detected if not set)
    runtime: Option<container::RuntimeType>,
    /// Custom path to the container runtime binary
    binary: Option<String>,
    /// Default container image for sessions
    default_image: String,
    /// Base port for allocating session ports
    base_port: u16,
    /// Base directory for user home directories
    user_data_path: Option<String>,
    /// Path to skeleton directory for new user homes
    skel_path: Option<String>,
}

impl Default for ContainerRuntimeConfig {
    fn default() -> Self {
        Self {
            runtime: None,
            binary: None,
            default_image: "octo-dev:latest".to_string(),
            base_port: 41820,
            user_data_path: None,
            skel_path: None,
        }
    }
}

/// Local runtime configuration (for running without containers).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
struct LocalModeConfig {
    /// Enable local mode (run services as native processes instead of containers)
    enabled: bool,
    /// Path to the opencode binary
    opencode_binary: String,
    /// Path to the fileserver binary
    fileserver_binary: String,
    /// Path to the ttyd binary
    ttyd_binary: String,
    /// Base directory for user workspaces in local mode.
    /// Supports ~ and environment variables. The {user_id} placeholder is replaced with the user ID.
    /// Default: $HOME/octo/{user_id}
    workspace_dir: String,
    /// Default agent name to pass to opencode via --agent flag.
    /// Agents are defined in opencode's global config or workspace's opencode.json.
    default_agent: Option<String>,
    /// Enable single-user mode. When true, the platform operates with a single user
    /// (no multi-tenancy), but password protection is still available.
    /// This simplifies setup for personal/single-user deployments.
    single_user: bool,
    /// Linux user isolation configuration
    #[serde(default)]
    linux_users: LinuxUsersConfig,
}

/// Configuration for Linux user isolation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
struct LinuxUsersConfig {
    /// Enable Linux user isolation (requires root or sudo privileges)
    enabled: bool,
    /// Prefix for auto-created Linux usernames (e.g., "octo_" -> "octo_alice")
    prefix: String,
    /// Starting UID for new users
    uid_start: u32,
    /// Shared group for all octo users
    group: String,
    /// Shell for new users
    shell: String,
    /// Use sudo to switch users
    use_sudo: bool,
    /// Create home directories for new users
    create_home: bool,
}

impl Default for LinuxUsersConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            prefix: "octo_".to_string(),
            uid_start: 2000,
            group: "octo".to_string(),
            shell: "/bin/bash".to_string(),
            use_sudo: true,
            create_home: true,
        }
    }
}

/// Agent scaffolding configuration.
/// Defines the external command used to scaffold new agent directories.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ScaffoldConfig {
    /// Binary to use for scaffolding (e.g., "byt", "cookiecutter", custom script)
    pub binary: String,
    /// Subcommand to invoke (e.g., "new" for "byt new")
    pub subcommand: String,
    /// Argument format for template name (e.g., "--template" for "--template rust-cli")
    pub template_arg: String,
    /// Argument format for output directory
    pub output_arg: String,
    /// Argument to create GitHub repo
    pub github_arg: Option<String>,
    /// Argument to make repo private
    pub private_arg: Option<String>,
    /// Argument format for description
    pub description_arg: Option<String>,
}

impl Default for ScaffoldConfig {
    fn default() -> Self {
        Self {
            binary: "byt".to_string(),
            subcommand: "new".to_string(),
            template_arg: "--template".to_string(),
            output_arg: "--output".to_string(),
            github_arg: Some("--github".to_string()),
            private_arg: Some("--private".to_string()),
            description_arg: Some("--description".to_string()),
        }
    }
}

/// Pi agent configuration for Main Chat.
///
/// Pi is used as the agent runtime for Main Chat, providing streaming
/// responses and built-in compaction.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct PiConfig {
    /// Whether Pi integration is enabled for Main Chat.
    pub enabled: bool,
    /// Path to the Pi CLI executable (e.g., "pi" or "/usr/local/bin/pi")
    pub executable: String,
    /// Default LLM provider (e.g., "anthropic", "openai")
    pub default_provider: Option<String>,
    /// Default model name (e.g., "claude-sonnet-4-20250514")
    pub default_model: Option<String>,
    /// Extension files to load (passed via --extension).
    /// If empty, looks for bundled extensions in $DATA_DIR/extensions/
    #[serde(default)]
    pub extensions: Vec<String>,
    /// Maximum session age before forcing fresh start (hours).
    /// Default: 4 hours.
    pub max_session_age_hours: Option<u64>,
    /// Maximum session file size before forcing fresh start (bytes).
    /// Default: 500KB.
    pub max_session_size_bytes: Option<u64>,
}

impl Default for PiConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            executable: "pi".to_string(),
            default_provider: Some("anthropic".to_string()),
            default_model: Some("claude-sonnet-4-20250514".to_string()),
            extensions: Vec::new(),
            max_session_age_hours: None,
            max_session_size_bytes: None,
        }
    }
}

impl Default for LocalModeConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            opencode_binary: "opencode".to_string(),
            fileserver_binary: "fileserver".to_string(),
            ttyd_binary: "ttyd".to_string(),
            workspace_dir: "$HOME/octo/{user_id}".to_string(),
            default_agent: None,
            single_user: false,
            linux_users: LinuxUsersConfig::default(),
        }
    }
}

fn handle_run(ctx: &mut RuntimeContext, cmd: RunCommand) -> Result<()> {
    let effective = ctx.config.clone().with_profile_override(cmd.profile);
    let output = if ctx.common.json {
        serde_json::to_string_pretty(&effective).context("serializing run output to JSON")?
    } else if ctx.common.yaml {
        serde_yaml::to_string(&effective).context("serializing run output to YAML")?
    } else {
        format!(
            "Running task '{}' with profile '{}' (parallelism: {})",
            cmd.task,
            effective.profile,
            effective
                .runtime
                .parallelism
                .unwrap_or_else(default_parallelism)
        )
    };

    println!("{output}");
    Ok(())
}

fn handle_init(ctx: &RuntimeContext, cmd: InitCommand) -> Result<()> {
    if ctx.paths.config_file.exists() && !(cmd.force || ctx.common.assume_yes) {
        return Err(anyhow!(
            "config already exists at {} (use --force to overwrite)",
            ctx.paths.config_file.display()
        ));
    }

    if ctx.common.dry_run {
        info!(
            "dry-run: would write default config to {}",
            ctx.paths.config_file.display()
        );
        return Ok(());
    }

    write_default_config(&ctx.paths.config_file)
}

fn handle_config(ctx: &RuntimeContext, command: ConfigCommand) -> Result<()> {
    match command {
        ConfigCommand::Show => {
            if ctx.common.json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&ctx.config)
                        .context("serializing config to JSON")?
                );
            } else if ctx.common.yaml {
                println!(
                    "{}",
                    serde_yaml::to_string(&ctx.config).context("serializing config to YAML")?
                );
            } else {
                println!("{:#?}", ctx.config);
            }
            Ok(())
        }
        ConfigCommand::Path => {
            println!("{}", ctx.paths.config_file.display());
            Ok(())
        }
        ConfigCommand::Reset => {
            if ctx.common.dry_run {
                info!(
                    "dry-run: would reset config at {}",
                    ctx.paths.config_file.display()
                );
                return Ok(());
            }
            write_default_config(&ctx.paths.config_file)
        }
    }
}

fn handle_completions(shell: Shell) -> Result<()> {
    let mut cmd = Cli::command();
    clap_complete::generate(shell, &mut cmd, APP_NAME, &mut io::stdout());
    Ok(())
}

async fn handle_invite_codes(ctx: &RuntimeContext, cmd: InviteCodesCommand) -> Result<()> {
    // Initialize database
    let db_path = ctx.paths.data_dir.join("sessions.db");
    let database = db::Database::new(&db_path).await?;
    let invite_repo = invite::InviteCodeRepository::new(database.pool().clone());

    match cmd {
        InviteCodesCommand::Generate(gen_cmd) => {
            // Parse expiration duration
            let expires_in_secs = gen_cmd
                .expires_in
                .as_ref()
                .map(|s| parse_duration(s))
                .transpose()?;

            let codes = invite_repo
                .create_batch(
                    gen_cmd.count,
                    gen_cmd.uses_per_code,
                    expires_in_secs,
                    gen_cmd.prefix.as_deref(),
                    gen_cmd.note.as_deref(),
                    &gen_cmd.admin_id,
                )
                .await?;

            if ctx.common.json {
                let output: Vec<_> = codes
                    .iter()
                    .map(|c| {
                        serde_json::json!({
                            "id": c.id,
                            "code": c.code,
                            "uses_remaining": c.uses_remaining,
                            "expires_at": c.expires_at,
                        })
                    })
                    .collect();
                println!("{}", serde_json::to_string_pretty(&output)?);
            } else {
                println!("Generated {} invite code(s):", codes.len());
                println!();
                for code in &codes {
                    println!("{}", code.code);
                }
                if codes.len() > 1 {
                    println!();
                    println!("Use --json for machine-readable output");
                }
            }
        }
        InviteCodesCommand::List(list_cmd) => {
            let valid_filter = match list_cmd.filter.as_str() {
                "valid" => Some(true),
                "invalid" => Some(false),
                _ => None,
            };

            let query = invite::InviteCodeListQuery {
                valid: valid_filter,
                limit: Some(list_cmd.limit),
                ..Default::default()
            };

            let codes = invite_repo.list(query).await?;

            if ctx.common.json {
                let output: Vec<_> = codes
                    .iter()
                    .map(|c| {
                        serde_json::json!({
                            "id": c.id,
                            "code": c.code,
                            "uses_remaining": c.uses_remaining,
                            "max_uses": c.max_uses,
                            "expires_at": c.expires_at,
                            "created_at": c.created_at,
                            "is_valid": c.is_valid(),
                        })
                    })
                    .collect();
                println!("{}", serde_json::to_string_pretty(&output)?);
            } else {
                println!(
                    "{:<16} {:<12} {:>5}/{:<5} {:>8} {}",
                    "ID", "CODE", "USED", "MAX", "VALID", "EXPIRES"
                );
                println!("{}", "-".repeat(70));
                for code in &codes {
                    let used = code.max_uses - code.uses_remaining;
                    let valid = if code.is_valid() { "yes" } else { "no" };
                    let expires = code.expires_at.as_deref().unwrap_or("never");
                    println!(
                        "{:<16} {:<12} {:>5}/{:<5} {:>8} {}",
                        code.id, code.code, used, code.max_uses, valid, expires
                    );
                }
                println!();
                println!("Total: {} codes", codes.len());
            }
        }
        InviteCodesCommand::Revoke(revoke_cmd) => {
            invite_repo.revoke(&revoke_cmd.code_id).await?;

            if ctx.common.json {
                println!(r#"{{"status": "revoked", "id": "{}"}}"#, revoke_cmd.code_id);
            } else {
                println!("Revoked invite code: {}", revoke_cmd.code_id);
            }
        }
    }

    Ok(())
}

/// Parse a duration string like "7d", "24h", "30m" into seconds.
fn parse_duration(s: &str) -> Result<i64> {
    let s = s.trim();
    if s.is_empty() {
        return Err(anyhow!("empty duration string"));
    }

    let (num_str, unit) = s.split_at(s.len() - 1);
    let num: i64 = num_str.parse().context("invalid duration number")?;

    let seconds = match unit {
        "s" => num,
        "m" => num * 60,
        "h" => num * 3600,
        "d" => num * 86400,
        "w" => num * 604800,
        _ => return Err(anyhow!("invalid duration unit '{}', use s/m/h/d/w", unit)),
    };

    Ok(seconds)
}

async fn handle_serve(ctx: &RuntimeContext, cmd: ServeCommand) -> Result<()> {
    info!("Starting workspace backend server...");

    // Initialize database
    let db_path = ctx.paths.data_dir.join("sessions.db");
    info!("Database path: {}", db_path.display());
    let database = db::Database::new(&db_path).await?;

    // Initialize authentication from config
    let auth_config = ctx.config.auth.clone();
    auth_config
        .validate()
        .context("Invalid auth configuration")?;
    info!(
        "Auth mode: {}",
        if auth_config.dev_mode {
            "development"
        } else {
            "production"
        }
    );
    let auth_state = auth::AuthState::new(auth_config);

    // Determine runtime mode: CLI --local-mode overrides config
    let runtime_mode = match ctx.config.backend.mode {
        BackendMode::Local => session::RuntimeMode::Local,
        BackendMode::Container => session::RuntimeMode::Container,
        BackendMode::Auto => {
            // Auto: prefer local if explicitly enabled, otherwise container
            if ctx.config.local.enabled || cmd.local_mode {
                session::RuntimeMode::Local
            } else {
                session::RuntimeMode::Container
            }
        }
    };
    // CLI override
    let runtime_mode = if cmd.local_mode {
        session::RuntimeMode::Local
    } else {
        runtime_mode
    };
    let local_mode = runtime_mode == session::RuntimeMode::Local;
    info!(
        "Runtime mode: {:?} (backend.mode={:?})",
        runtime_mode, ctx.config.backend.mode
    );

    // Initialize runtimes based on mode
    let container_runtime: Option<std::sync::Arc<container::ContainerRuntime>> = if !local_mode {
        let runtime = match (&ctx.config.container.runtime, &ctx.config.container.binary) {
            (Some(rt), Some(binary)) => {
                container::ContainerRuntime::with_binary(*rt, binary.clone())
            }
            (Some(rt), None) => container::ContainerRuntime::with_type(*rt),
            (None, _) => container::ContainerRuntime::new(),
        };

        // Check container runtime is available
        match runtime.health_check().await {
            Ok(_) => info!(
                "Container runtime ({}) is available",
                runtime.runtime_type()
            ),
            Err(e) => log::warn!(
                "Container runtime health check failed: {:?}. Container operations may fail.",
                e
            ),
        }

        Some(std::sync::Arc::new(runtime))
    } else {
        None
    };

    let local_runtime: Option<local::LocalRuntime> = if local_mode {
        // Build Linux users config
        let linux_users_config = local::LinuxUsersConfig {
            enabled: ctx.config.local.linux_users.enabled,
            prefix: ctx.config.local.linux_users.prefix.clone(),
            uid_start: ctx.config.local.linux_users.uid_start,
            group: ctx.config.local.linux_users.group.clone(),
            shell: ctx.config.local.linux_users.shell.clone(),
            use_sudo: ctx.config.local.linux_users.use_sudo,
            create_home: ctx.config.local.linux_users.create_home,
        };

        let mut local_config = local::LocalRuntimeConfig {
            opencode_binary: ctx.config.local.opencode_binary.clone(),
            fileserver_binary: ctx.config.local.fileserver_binary.clone(),
            ttyd_binary: ctx.config.local.ttyd_binary.clone(),
            workspace_dir: ctx.config.local.workspace_dir.clone(),
            default_agent: ctx.config.local.default_agent.clone(),
            single_user: ctx.config.local.single_user,
            linux_users: linux_users_config,
        };
        local_config.expand_paths();

        // Validate that all binaries are available
        if let Err(e) = local_config.validate() {
            error!("Local mode validation failed: {:?}", e);
            anyhow::bail!(
                "Local mode requires opencode, fileserver, and ttyd binaries. Error: {}",
                e
            );
        }

        // Check Linux user isolation privileges if enabled
        if local_config.linux_users.enabled {
            if let Err(e) = local_config.linux_users.check_privileges() {
                error!("Linux user isolation check failed: {:?}", e);
                anyhow::bail!(
                    "Linux user isolation requires root or sudo privileges. Error: {}",
                    e
                );
            }
            info!(
                "Linux user isolation enabled: prefix={}, group={}, uid_start={}",
                local_config.linux_users.prefix,
                local_config.linux_users.group,
                local_config.linux_users.uid_start
            );
        }

        info!(
            "Local runtime ready: opencode={}, fileserver={}, ttyd={}, workspace={}",
            local_config.opencode_binary,
            local_config.fileserver_binary,
            local_config.ttyd_binary,
            local_config.workspace_dir
        );

        if local_config.single_user {
            info!("Single-user mode enabled");
        }

        Some(local::LocalRuntime::new(local_config))
    } else {
        None
    };

    // Session config: CLI args override config file values
    let default_image = if cmd.image != "octo-dev:latest" {
        cmd.image.clone()
    } else {
        ctx.config.container.default_image.clone()
    };
    let base_port = if cmd.base_port != 41820 {
        cmd.base_port as i64
    } else {
        ctx.config.container.base_port as i64
    };

    // CLI --skel-path overrides config file
    let skel_path = cmd
        .skel_path
        .as_ref()
        .map(|p| p.to_string_lossy().to_string())
        .or_else(|| ctx.config.container.skel_path.clone())
        .map(|p| {
            std::path::Path::new(&p)
                .canonicalize()
                .unwrap_or_else(|_| std::path::PathBuf::from(&p))
                .to_string_lossy()
                .to_string()
        });

    // User data path: CLI overrides config, config overrides default
    let user_data_path = if cmd.user_data_path != std::path::PathBuf::from("./data") {
        // CLI explicitly set
        cmd.user_data_path.clone()
    } else if let Some(ref config_path) = ctx.config.container.user_data_path {
        // Use config file value
        std::path::PathBuf::from(shellexpand::tilde(config_path).to_string())
    } else {
        // Use CLI default
        cmd.user_data_path.clone()
    };
    let user_data_path = user_data_path
        .canonicalize()
        .unwrap_or(user_data_path)
        .to_string_lossy()
        .to_string();

    // Build local runtime config if in local mode
    let local_runtime_config = if local_mode {
        local_runtime.as_ref().map(|r| r.config().clone())
    } else {
        None
    };

    // Determine single_user mode from local config
    let single_user = ctx.config.local.single_user;

    let eavs_url = if local_mode {
        ctx.config.eavs.as_ref().map(|e| e.base_url.clone())
    } else {
        ctx.config
            .eavs
            .as_ref()
            .and_then(|e| e.container_url.clone())
    };

    let session_config = session::SessionServiceConfig {
        default_image,
        base_port,
        user_data_path,
        skel_path,
        default_user_id: "default".to_string(),
        default_session_budget_usd: ctx
            .config
            .eavs
            .as_ref()
            .and_then(|e| e.default_session_budget_usd),
        default_session_rpm: ctx.config.eavs.as_ref().and_then(|e| e.default_session_rpm),
        eavs_container_url: eavs_url,
        runtime_mode,
        local_config: local_runtime_config,
        single_user,
        mmry_enabled: ctx.config.mmry.enabled,
        mmry_container_url: ctx.config.mmry.container_url.clone(),
        max_concurrent_sessions: ctx.config.sessions.max_concurrent_sessions,
        idle_timeout_minutes: ctx.config.sessions.idle_timeout_minutes,
        idle_check_interval_seconds: ctx.config.sessions.idle_check_interval_seconds,
    };

    let session_repo = session::SessionRepository::new(database.pool().clone());

    // Initialize EAVS client if configured
    let eavs_client: Option<std::sync::Arc<dyn eavs::EavsApi>> = if let Some(ref eavs_config) =
        ctx.config.eavs
    {
        if eavs_config.enabled {
            if let Some(ref master_key) = eavs_config.master_key {
                match eavs::EavsClient::new(&eavs_config.base_url, master_key) {
                    Ok(client) => Some(std::sync::Arc::new(client)),
                    Err(err) => {
                        log::error!("Failed to initialize EAVS client: {}", err);
                        None
                    }
                }
            } else if let Ok(master_key) = std::env::var("EAVS_MASTER_KEY") {
                match eavs::EavsClient::new(&eavs_config.base_url, master_key) {
                    Ok(client) => Some(std::sync::Arc::new(client)),
                    Err(err) => {
                        log::error!("Failed to initialize EAVS client: {}", err);
                        None
                    }
                }
            } else {
                log::warn!(
                    "EAVS enabled but no master_key configured (set eavs.master_key or EAVS_MASTER_KEY env var)"
                );
                None
            }
        } else {
            None
        }
    } else {
        None
    };

    // Check container image (only in container mode)
    if !local_mode {
        if let Some(ref runtime) = container_runtime {
            match runtime.image_exists(&session_config.default_image).await {
                Ok(true) => {
                    info!("Container image '{}' found", session_config.default_image);
                }
                Ok(false) => {
                    error!(
                        "Container image '{}' not found. Please build it first:\n\
                         \n\
                         cd container && docker build -t {} -f Dockerfile ..\n\
                         \n\
                         Or specify a different image with --image or in config.toml",
                        session_config.default_image, session_config.default_image
                    );
                    anyhow::bail!(
                        "Required container image '{}' not found. Build it with: cd container && docker build -t {} -f Dockerfile ..",
                        session_config.default_image,
                        session_config.default_image
                    );
                }
                Err(e) => {
                    warn!(
                        "Could not check if image '{}' exists: {:?}. Container operations may fail.",
                        session_config.default_image, e
                    );
                }
            }
        }
    }

    // Create session service based on runtime mode
    let session_service = if local_mode {
        let local_rt = local_runtime.expect("local runtime should be set in local mode");
        if let Some(eavs) = eavs_client.clone() {
            session::SessionService::with_local_runtime_and_eavs(
                session_repo,
                local_rt,
                eavs,
                session_config.clone(),
            )
        } else {
            session::SessionService::with_local_runtime(
                session_repo,
                local_rt,
                session_config.clone(),
            )
        }
    } else {
        let container_rt = container_runtime
            .clone()
            .expect("container runtime should be set in container mode");
        if let Some(eavs) = eavs_client.clone() {
            session::SessionService::with_eavs(
                session_repo,
                container_rt,
                eavs,
                session_config.clone(),
            )
        } else {
            session::SessionService::new(session_repo, container_rt, session_config.clone())
        }
    };

    // Run startup cleanup to handle orphan containers and stale sessions
    if let Err(e) = session_service.startup_cleanup().await {
        warn!("Startup cleanup failed (continuing anyway): {:?}", e);
    }

    // Start idle session cleanup background task
    // Check every 5 minutes, stop sessions idle for 30 minutes
    let session_service_arc = std::sync::Arc::new(session_service.clone());
    let _idle_cleanup_handle = session_service_arc.start_idle_session_cleanup_task(
        session_config.idle_check_interval_seconds,
        session_config.idle_timeout_minutes,
    );

    // Initialize agent service for managing opencode instances
    // In local mode, we use a dummy container runtime (agent features limited)
    let agent_runtime: std::sync::Arc<dyn container::ContainerRuntimeApi> =
        if let Some(ref rt) = container_runtime {
            rt.clone()
        } else {
            // Create a container runtime for agent service even in local mode
            // This allows basic agent operations to work (though docker exec will fail)
            std::sync::Arc::new(container::ContainerRuntime::new())
        };
    let agent_repo = agent::AgentRepository::new(database.pool().clone());
    let scaffold_config = agent::ScaffoldConfig {
        binary: ctx.config.scaffold.binary.clone(),
        subcommand: ctx.config.scaffold.subcommand.clone(),
        template_arg: ctx.config.scaffold.template_arg.clone(),
        output_arg: ctx.config.scaffold.output_arg.clone(),
        github_arg: ctx.config.scaffold.github_arg.clone(),
        private_arg: ctx.config.scaffold.private_arg.clone(),
        description_arg: ctx.config.scaffold.description_arg.clone(),
    };
    let agent_service = agent::AgentService::with_scaffold_config(
        agent_runtime,
        session_service.clone(),
        agent_repo,
        scaffold_config,
    );

    // Initialize user service
    let user_repo = user::UserRepository::new(database.pool().clone());
    let user_service = user::UserService::new(user_repo);

    // Initialize invite code repository
    let invite_repo = invite::InviteCodeRepository::new(database.pool().clone());

    // Clone session_service before creating state for shutdown handler
    let session_service_for_shutdown = session_service.clone();

    // Create AgentBackend if enabled
    let agent_backend: Option<std::sync::Arc<dyn agent_rpc::AgentBackend>> =
        if ctx.config.backend.use_agent_rpc {
            info!("AgentRPC backend enabled");
            if local_mode {
                // Use LocalBackend - convert LocalModeConfig to LocalRuntimeConfig
                let runtime_config = local::LocalRuntimeConfig {
                    opencode_binary: ctx.config.local.opencode_binary.clone(),
                    fileserver_binary: ctx.config.local.fileserver_binary.clone(),
                    ttyd_binary: ctx.config.local.ttyd_binary.clone(),
                    workspace_dir: ctx.config.local.workspace_dir.clone(),
                    default_agent: ctx.config.local.default_agent.clone(),
                    single_user: ctx.config.local.single_user,
                    linux_users: local::LinuxUsersConfig {
                        enabled: ctx.config.local.linux_users.enabled,
                        prefix: ctx.config.local.linux_users.prefix.clone(),
                        uid_start: ctx.config.local.linux_users.uid_start,
                        group: ctx.config.local.linux_users.group.clone(),
                        shell: ctx.config.local.linux_users.shell.clone(),
                        use_sudo: ctx.config.local.linux_users.use_sudo,
                        create_home: ctx.config.local.linux_users.create_home,
                    },
                };
                let local_config = agent_rpc::LocalBackendConfig {
                    runtime: runtime_config,
                    data_dir: std::path::PathBuf::from(
                        &ctx.config
                            .container
                            .user_data_path
                            .clone()
                            .unwrap_or_else(|| "./data".to_string()),
                    ),
                    base_port: ctx.config.container.base_port,
                    single_user: ctx.config.local.single_user,
                };
                match agent_rpc::LocalBackend::new(local_config) {
                    Ok(backend) => {
                        info!("LocalBackend initialized");
                        Some(std::sync::Arc::new(backend))
                    }
                    Err(e) => {
                        warn!("Failed to create LocalBackend: {:?}", e);
                        None
                    }
                }
            } else {
                // Use ContainerBackend
                let container_config = agent_rpc::ContainerBackendConfig {
                    image: ctx.config.container.default_image.clone(),
                    base_port: ctx.config.container.base_port,
                    data_dir: std::path::PathBuf::from(
                        &ctx.config
                            .container
                            .user_data_path
                            .clone()
                            .unwrap_or_else(|| "./data".to_string()),
                    ),
                    host_network: false,
                    env: std::collections::HashMap::new(),
                };
                let backend = agent_rpc::ContainerBackend::with_auto_runtime(container_config);
                info!("ContainerBackend initialized");
                Some(std::sync::Arc::new(backend))
            }
        } else {
            None
        };

    // Build mmry state based on configuration
    let mmry_state = api::MmryState {
        enabled: ctx.config.mmry.enabled,
        single_user,
        local_service_url: ctx.config.mmry.local_service_url.clone(),
    };

    // Build voice state based on configuration
    let voice_state = api::VoiceState {
        enabled: ctx.config.voice.enabled,
        stt_url: ctx.config.voice.stt_url.clone(),
        tts_url: ctx.config.voice.tts_url.clone(),
        vad_timeout_ms: ctx.config.voice.vad_timeout_ms,
        default_voice: ctx.config.voice.default_voice.clone(),
        default_speed: ctx.config.voice.default_speed,
        auto_language_detect: ctx.config.voice.auto_language_detect,
        tts_muted: ctx.config.voice.tts_muted,
        continuous_mode: ctx.config.voice.continuous_mode,
        default_visualizer: ctx.config.voice.default_visualizer.clone(),
        interrupt_word_count: ctx.config.voice.interrupt_word_count,
        interrupt_backoff_ms: ctx.config.voice.interrupt_backoff_ms,
        visualizer_voices: ctx
            .config
            .voice
            .visualizer_voices
            .iter()
            .map(|(k, v)| {
                (
                    k.clone(),
                    api::VisualizerVoiceState {
                        voice: v.voice.clone(),
                        speed: v.speed,
                    },
                )
            })
            .collect(),
    };

    let session_ui_state = api::SessionUiState {
        auto_attach: ctx.config.sessions.auto_attach,
        auto_attach_scan: ctx.config.sessions.auto_attach_scan,
    };

    // Create settings services
    let octo_schema: serde_json::Value =
        serde_json::from_str(include_str!("../examples/backend.config.schema.json"))
            .expect("Failed to parse embedded octo schema");

    let octo_config_dir = default_config_dir()?;
    let settings_octo = settings::SettingsService::new(octo_schema, octo_config_dir, "config.toml")
        .context("Failed to create octo settings service")?;

    // Create mmry settings service if mmry is enabled
    let settings_mmry = if ctx.config.mmry.enabled {
        // mmry config is at ~/.config/mmry/config.toml
        let mmry_config_dir = default_config_dir()?
            .parent()
            .map(|p| p.join("mmry"))
            .unwrap_or_else(|| PathBuf::from("~/.config/mmry"));

        // Try to load mmry schema if it exists, otherwise create minimal schema
        let mmry_schema = std::fs::read_to_string(mmry_config_dir.join("config.schema.json"))
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_else(|| {
                serde_json::json!({
                    "$schema": "http://json-schema.org/draft-07/schema#",
                    "title": "mmry Configuration",
                    "type": "object",
                    "properties": {}
                })
            });

        settings::SettingsService::new(mmry_schema, mmry_config_dir, "config.toml").ok()
    } else {
        None
    };

    // Create app state
    let mut state = if let Some(backend) = agent_backend {
        api::AppState::with_agent_backend(
            session_service,
            agent_service,
            user_service,
            invite_repo,
            auth_state,
            backend,
            mmry_state,
            voice_state,
            session_ui_state,
        )
    } else {
        api::AppState::new(
            session_service,
            agent_service,
            user_service,
            invite_repo,
            auth_state,
            mmry_state,
            voice_state,
            session_ui_state,
        )
    };

    // Add settings services to state
    state = state.with_settings_octo(settings_octo);
    if let Some(mmry_settings) = settings_mmry {
        state = state.with_settings_mmry(mmry_settings);
    }

    // Initialize Main Chat service
    // Uses the user data path as the workspace dir for per-user Main Chat data
    let main_chat_workspace_dir = ctx.paths.data_dir.join("users");
    let main_chat_service =
        main_chat::MainChatService::new(main_chat_workspace_dir.clone(), ctx.config.local.single_user);
    info!("Main Chat service initialized");
    state = state.with_main_chat(main_chat_service);

    // Initialize Main Chat Pi service for agent runtime (if enabled)
    if ctx.config.pi.enabled {
        // Resolve extensions: use config or fall back to bundled extension
        let extensions = if ctx.config.pi.extensions.is_empty() {
            // Look for bundled extension in data directory
            let bundled_ext = ctx.paths.data_dir.join("extensions").join("octo-delegate.ts");
            if bundled_ext.exists() {
                info!("Using bundled Pi extension: {:?}", bundled_ext);
                vec![bundled_ext.to_string_lossy().to_string()]
            } else {
                debug!("No bundled Pi extension found at {:?}", bundled_ext);
                Vec::new()
            }
        } else {
            ctx.config.pi.extensions.clone()
        };

        let main_chat_pi_config = main_chat::MainChatPiServiceConfig {
            pi_executable: ctx.config.pi.executable.clone(),
            default_provider: ctx.config.pi.default_provider.clone(),
            default_model: ctx.config.pi.default_model.clone(),
            extensions,
            max_session_age_hours: ctx
                .config
                .pi
                .max_session_age_hours
                .unwrap_or(4),
            max_session_size_bytes: ctx
                .config
                .pi
                .max_session_size_bytes
                .unwrap_or(500 * 1024),
        };
        let main_chat_pi_service = main_chat::MainChatPiService::new(
            main_chat_workspace_dir,
            ctx.config.local.single_user,
            main_chat_pi_config,
        );
        info!(
            "Main Chat Pi service initialized (executable: {})",
            ctx.config.pi.executable
        );
        state = state.with_main_chat_pi(main_chat_pi_service);
    } else {
        info!("Main Chat Pi service disabled");
    }

    // Create router
    let app = api::create_router(state);

    // Bind and serve
    let addr: SocketAddr = format!("{}:{}", cmd.host, cmd.port)
        .parse()
        .context("invalid address")?;

    info!("Listening on http://{}", addr);

    let listener = TcpListener::bind(addr)
        .await
        .context("binding to address")?;

    // Set up graceful shutdown
    let shutdown_signal = async move {
        let ctrl_c = async {
            tokio::signal::ctrl_c()
                .await
                .expect("failed to install Ctrl+C handler");
        };

        #[cfg(unix)]
        let terminate = async {
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                .expect("failed to install signal handler")
                .recv()
                .await;
        };

        #[cfg(not(unix))]
        let terminate = std::future::pending::<()>();

        tokio::select! {
            _ = ctrl_c => {},
            _ = terminate => {},
        }

        info!("Shutdown signal received, stopping containers...");

        // Stop all running containers gracefully
        if let Err(e) = shutdown_all_sessions(&session_service_for_shutdown).await {
            warn!("Error during shutdown: {:?}", e);
        }

        info!("Shutdown complete");
    };

    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal)
    .await
    .context("running server")?;

    Ok(())
}

/// Stop all running sessions during shutdown.
async fn shutdown_all_sessions(session_service: &session::SessionService) -> Result<()> {
    let sessions = session_service.list_sessions().await?;
    let running_count = sessions.iter().filter(|s| s.is_active()).count();

    if running_count == 0 {
        info!("No active sessions to stop");
        return Ok(());
    }

    info!("Stopping {} active session(s)...", running_count);

    for session in sessions {
        if session.is_active() {
            match session_service.stop_session(&session.id).await {
                Ok(()) => info!("Stopped session {}", session.id),
                Err(e) => warn!("Failed to stop session {}: {:?}", session.id, e),
            }
        }
    }

    Ok(())
}

fn load_or_init_config(paths: &mut AppPaths, common: &CommonOpts) -> Result<AppConfig> {
    if !paths.config_file.exists() {
        if common.dry_run {
            info!(
                "dry-run: would create default config at {}",
                paths.config_file.display()
            );
        } else {
            write_default_config(&paths.config_file)?;
        }
    }

    let env_prefix = env_prefix();
    let built = Config::builder()
        .set_default("profile", "default")?
        .set_default("logging.level", "info")?
        .set_default("runtime.parallelism", default_parallelism() as i64)?
        .set_default("runtime.timeout", 60_i64)?
        .set_default("runtime.fail_fast", true)?
        .add_source(
            File::from(paths.config_file.as_path())
                .format(FileFormat::Toml)
                .required(false),
        )
        .add_source(Environment::with_prefix(env_prefix.as_str()).separator("__"))
        .build()?;

    let mut config: AppConfig = built.try_deserialize()?;

    if let Some(ref file) = config.logging.file {
        let expanded = expand_str_path(file)?;
        config.logging.file = Some(expanded.display().to_string());
    }

    Ok(config)
}

fn write_default_config(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("creating config directory {parent:?}"))?;
    }

    let config = AppConfig::default();
    let toml = toml::to_string_pretty(&config).context("serializing default config to TOML")?;
    let mut body = default_config_header(path)?;
    body.push_str(&toml);
    fs::write(path, body).with_context(|| format!("writing config file to {}", path.display()))
}

fn default_config_header(path: &Path) -> Result<String> {
    let mut buffer = String::new();
    buffer.push_str("# Configuration for ");
    buffer.push_str(APP_NAME);
    buffer.push('\n');
    buffer.push_str("# File: ");
    buffer.push_str(&path.display().to_string());
    buffer.push('\n');
    buffer.push('\n');
    Ok(buffer)
}

fn expand_path(path: PathBuf) -> Result<PathBuf> {
    if let Some(text) = path.to_str() {
        expand_str_path(text)
    } else {
        Ok(path)
    }
}

fn expand_str_path(text: &str) -> Result<PathBuf> {
    let expanded = shellexpand::full(text).context("expanding path")?;
    Ok(PathBuf::from(expanded.to_string()))
}

fn default_config_dir() -> Result<PathBuf> {
    if let Some(dir) = env::var_os("XDG_CONFIG_HOME").filter(|v| !v.is_empty()) {
        let mut path = PathBuf::from(dir);
        path.push(APP_NAME);
        return Ok(path);
    }

    if let Some(mut dir) = dirs::config_dir() {
        dir.push(APP_NAME);
        return Ok(dir);
    }

    dirs::home_dir()
        .map(|home| home.join(".config").join(APP_NAME))
        .ok_or_else(|| anyhow!("unable to determine configuration directory"))
}

fn default_data_dir() -> Result<PathBuf> {
    if let Some(dir) = env::var_os("XDG_DATA_HOME").filter(|v| !v.is_empty()) {
        return Ok(PathBuf::from(dir).join(APP_NAME));
    }

    if let Some(mut dir) = dirs::data_dir() {
        dir.push(APP_NAME);
        return Ok(dir);
    }

    dirs::home_dir()
        .map(|home| home.join(".local").join("share").join(APP_NAME))
        .ok_or_else(|| anyhow!("unable to determine data directory"))
}

fn default_state_dir() -> Result<PathBuf> {
    if let Some(dir) = env::var_os("XDG_STATE_HOME").filter(|v| !v.is_empty()) {
        return Ok(PathBuf::from(dir).join(APP_NAME));
    }

    if let Some(mut dir) = dirs::state_dir() {
        dir.push(APP_NAME);
        return Ok(dir);
    }

    dirs::home_dir()
        .map(|home| home.join(".local").join("state").join(APP_NAME))
        .ok_or_else(|| anyhow!("unable to determine state directory"))
}

fn env_prefix() -> String {
    APP_NAME
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_uppercase()
            } else {
                '_'
            }
        })
        .collect()
}

fn default_parallelism() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1)
}

impl fmt::Display for AppPaths {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "config: {}, data: {}, state: {}",
            self.config_file.display(),
            self.data_dir.display(),
            self.state_dir.display()
        )
    }
}
