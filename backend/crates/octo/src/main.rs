use std::env;
use std::fmt;
use std::fs;
use std::io::{self, IsTerminal, Write};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use clap::{Args, CommandFactory, Parser, Subcommand, ValueEnum};
use clap_complete::Shell;
use config::{Config, Environment, File, FileFormat};

use log::{LevelFilter, debug, error, info, warn};
use serde::{Deserialize, Serialize};
use tokio::net::TcpListener;

mod agent;
mod agent_browser;
mod agent_rpc;
mod api;
mod auth;
mod canon;
mod container;
mod db;
mod eavs;
mod feedback;
mod history;
mod hstry;
mod invite;
mod local;
mod markdown;
mod observability;
mod onboarding;
mod pi;
// pi_workspace removed -- JSONL scanning replaced by hstry-only session listing
mod projects;
mod runner;
mod session;
mod session_ui;
mod settings;
mod templates;
mod user;
mod user_plane;
mod wordlist;
mod workspace;
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
        Command::Runner { command } => handle_runner(command),
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
    /// Manage the octo-runner daemon
    Runner {
        #[command(subcommand)]
        command: RunnerCommand,
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

#[derive(Debug, Subcommand)]
enum RunnerCommand {
    /// Start the octo-runner daemon
    Start,
    /// Stop the octo-runner daemon
    Stop,
    /// Restart the octo-runner daemon
    Restart,
    /// Check if the runner is running
    Status,
    /// Enable the runner systemd service (auto-start on login)
    Enable,
    /// Disable the runner systemd service
    Disable,
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
    /// Project template repository configuration.
    templates: TemplatesConfig,
    /// Agent scaffolding configuration.
    scaffold: ScaffoldConfig,
    /// Pi agent configuration for Main Chat.
    pi: PiConfig,
    /// Agent-browser daemon configuration.
    agent_browser: agent_browser::AgentBrowserConfig,
    /// Server configuration.
    server: ServerConfig,
    /// Onboarding templates configuration.
    onboarding_templates: templates::OnboardingTemplatesConfig,
    /// sldr configuration.
    sldr: SldrConfig,
    /// hstry (chat history) configuration.
    hstry: HstryConfig,
    /// Feedback collection configuration.
    feedback: feedback::FeedbackConfig,
}

/// Server configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
struct ServerConfig {
    /// Maximum file upload size in megabytes (default: 100).
    max_upload_size_mb: usize,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            max_upload_size_mb: 100,
        }
    }
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
    /// Runner configuration for user-plane isolation.
    runner: RunnerConfig,
}

impl Default for BackendConfig {
    fn default() -> Self {
        Self {
            mode: BackendMode::Container,
            use_agent_rpc: false,
            runner: RunnerConfig::default(),
        }
    }
}

/// Runner configuration for user-plane isolation.
///
/// When `user_plane_enabled` is true in local multi-user mode, all user data
/// operations are routed through per-user runner daemons, providing OS-level
/// isolation between users.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
struct RunnerConfig {
    /// Enable runner as the user-plane boundary.
    ///
    /// When true:
    /// - All user data access goes through per-user runner daemons
    /// - Backend cannot directly read user workspaces or per-user DBs
    /// - Provides OS-level isolation in local multi-user mode
    ///
    /// When false:
    /// - Backend accesses user data directly (legacy behavior)
    /// - Only sandbox provides isolation (if enabled)
    user_plane_enabled: bool,
    /// Socket directory pattern for per-user runner sockets.
    /// Default: /run/user/{uid}/octo-runner.sock
    socket_pattern: Option<String>,
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
            sldr: SldrConfig::default(),
            voice: VoiceConfig::default(),
            sessions: SessionUiConfig::default(),
            auth: auth::AuthConfig::default(),
            templates: TemplatesConfig::default(),
            scaffold: ScaffoldConfig::default(),
            pi: PiConfig::default(),
            agent_browser: agent_browser::AgentBrowserConfig::default(),
            server: ServerConfig::default(),
            onboarding_templates: templates::OnboardingTemplatesConfig::default(),
            hstry: HstryConfig::default(),
            feedback: feedback::FeedbackConfig::default(),
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
    /// Number of recent sessions to prefetch chat messages for.
    chat_prefetch_limit: usize,
}

impl Default for SessionUiConfig {
    fn default() -> Self {
        Self {
            auto_attach: SessionAutoAttachMode::Off,
            auto_attach_scan: false,
            max_concurrent_sessions: session::SessionService::DEFAULT_MAX_CONCURRENT_SESSIONS,
            idle_timeout_minutes: session::SessionService::DEFAULT_IDLE_TIMEOUT_MINUTES,
            idle_check_interval_seconds: 5 * 60,
            chat_prefetch_limit: 8,
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

    /// Dedicated base port for per-user mmry instances (local multi-user mode).
    pub user_base_port: u16,
    /// Size of the per-user mmry port range (local multi-user mode).
    pub user_port_range: u16,
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

            user_base_port: 48_000,
            user_port_range: 1_000,
        }
    }
}

/// sldr configuration for per-user slide services.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SldrConfig {
    /// Whether sldr integration is enabled.
    pub enabled: bool,
    /// Path to sldr-server binary (for spawning per-user instances in multi-user mode).
    pub binary: String,
    /// Dedicated base port for per-user sldr instances (local multi-user mode).
    pub user_base_port: u16,
    /// Size of the per-user sldr port range (local multi-user mode).
    pub user_port_range: u16,
}

impl Default for SldrConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            binary: "sldr-server".to_string(),
            user_base_port: 49_000,
            user_port_range: 1_000,
        }
    }
}

/// hstry (chat history) configuration.
///
/// hstry provides unified chat history storage across all AI agents.
/// In multi-user mode, per-user hstry instances are spawned via octo-runner
/// using the shared `local.runner_socket_pattern`.
/// In single-user mode, auto-starts hstry daemon directly.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct HstryConfig {
    /// Whether hstry integration is enabled.
    pub enabled: bool,
    /// Path to the hstry binary.
    pub binary: String,
}

impl Default for HstryConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            binary: "hstry".to_string(),
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
    /// Whether to clean up local session processes on startup.
    cleanup_on_startup: bool,
    /// Whether to stop sessions when the backend shuts down.
    stop_sessions_on_shutdown: bool,
    /// Runner socket path pattern for per-user runner daemons.
    /// Supports `{user}` (Linux username) and `{uid}`.
    /// Examples: "/run/user/{uid}/octo-runner.sock", "/run/octo/runner-{user}.sock".
    runner_socket_pattern: Option<String>,
    // Note: Sandbox config is loaded from separate ~/.config/octo/sandbox.toml
    // for security reasons (agents can modify config.toml but not sandbox.toml)
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

/// Project templates configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct TemplatesConfig {
    /// Path to the local templates repository on the host.
    pub repo_path: Option<String>,
    /// Repository type: "remote" (git) or "local" (filesystem).
    #[serde(rename = "type")]
    pub repo_type: api::TemplatesRepoType,
    /// Sync repository before listing/creating templates.
    pub sync_on_list: bool,
    /// Minimum seconds between sync attempts.
    pub sync_interval_seconds: u64,
}

impl Default for TemplatesConfig {
    fn default() -> Self {
        Self {
            repo_path: None,
            repo_type: api::TemplatesRepoType::Remote,
            sync_on_list: true,
            sync_interval_seconds: 120,
        }
    }
}

/// Runtime mode for Pi process isolation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PiRuntimeMode {
    #[default]
    Local,
    Runner,
    Container,
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
    /// Runtime mode for Pi process isolation.
    /// Options: "local" (default), "runner", "container"
    #[serde(default)]
    pub runtime_mode: PiRuntimeMode,
    /// Runner socket path pattern (for runner mode).
    /// Use {user} placeholder for username, e.g., "/run/octo/runner-{user}.sock"
    pub runner_socket_pattern: Option<String>,
    /// Pi bridge URL (for container mode).
    /// e.g., "http://localhost:41824"
    pub bridge_url: Option<String>,
    /// Whether to sandbox Pi processes (only applies to runner mode).
    /// The runner loads sandbox config from /etc/octo/sandbox.toml.
    pub sandboxed: Option<bool>,

    /// Idle timeout in seconds before stopping inactive Pi processes.
    /// Default: 300 (5 minutes).
    pub idle_timeout_secs: Option<u64>,
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
            runtime_mode: PiRuntimeMode::Local,
            runner_socket_pattern: None,
            bridge_url: None,
            sandboxed: None,

            idle_timeout_secs: None,
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
            cleanup_on_startup: false,
            stop_sessions_on_shutdown: false,
            runner_socket_pattern: Some(
                "/run/octo/runner-sockets/{user}/octo-runner.sock".to_string(),
            ),
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

fn handle_runner(command: RunnerCommand) -> Result<()> {
    use std::process::Command as StdCommand;

    // Get the socket path
    let runtime_dir = env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp".to_string());
    let socket_path = format!("{}/octo-runner.sock", runtime_dir);

    // Helper to check if runner is running
    let is_running = || std::path::Path::new(&socket_path).exists();

    // Helper to find the runner binary
    let find_runner = || -> Result<std::path::PathBuf> {
        // Check if octo-runner is in PATH
        if let Ok(output) = StdCommand::new("which").arg("octo-runner").output()
            && output.status.success()
        {
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            return Ok(std::path::PathBuf::from(path));
        }
        // Check common locations
        let home = env::var("HOME").unwrap_or_else(|_| ".".to_string());
        let candidates = [
            format!("{}/.cargo/bin/octo-runner", home),
            "/usr/local/bin/octo-runner".to_string(),
            "/usr/bin/octo-runner".to_string(),
        ];
        for candidate in &candidates {
            if std::path::Path::new(candidate).exists() {
                return Ok(std::path::PathBuf::from(candidate));
            }
        }
        Err(anyhow::anyhow!(
            "octo-runner not found. Run 'just install' first."
        ))
    };

    match command {
        RunnerCommand::Start => {
            if is_running() {
                println!("Runner is already running (socket: {})", socket_path);
                return Ok(());
            }
            let runner_path = find_runner()?;
            println!("Starting octo-runner...");

            // Spawn detached process
            let child = StdCommand::new(&runner_path)
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()
                .with_context(|| format!("Failed to start {}", runner_path.display()))?;

            // Wait a moment for socket to appear
            std::thread::sleep(std::time::Duration::from_millis(500));

            if is_running() {
                println!(
                    "Runner started (pid: {}, socket: {})",
                    child.id(),
                    socket_path
                );
            } else {
                println!(
                    "Runner process started (pid: {}) but socket not yet available",
                    child.id()
                );
            }
            Ok(())
        }
        RunnerCommand::Stop => {
            if !is_running() {
                println!("Runner is not running");
                return Ok(());
            }
            // Connect and send shutdown command
            println!("Stopping octo-runner...");
            let rt = tokio::runtime::Runtime::new()?;
            let result = rt.block_on(async {
                use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
                use tokio::net::UnixStream;

                let mut stream = UnixStream::connect(&socket_path)
                    .await
                    .context("Failed to connect to runner")?;

                let req = r#"{"type":"shutdown"}"#;
                stream.write_all(format!("{}\n", req).as_bytes()).await?;

                let mut reader = BufReader::new(stream);
                let mut line = String::new();
                reader.read_line(&mut line).await?;

                // Wait for socket to disappear
                for _ in 0..20 {
                    if !std::path::Path::new(&socket_path).exists() {
                        break;
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                }

                Ok::<_, anyhow::Error>(())
            });

            // If connection failed, try to clean up stale socket
            if result.is_err() {
                // Socket exists but can't connect - stale socket
                let _ = std::fs::remove_file(&socket_path);
                println!("Removed stale socket");
            } else {
                println!("Runner stopped");
            }
            Ok(())
        }
        RunnerCommand::Restart => {
            handle_runner(RunnerCommand::Stop)?;
            std::thread::sleep(std::time::Duration::from_millis(500));
            handle_runner(RunnerCommand::Start)
        }
        RunnerCommand::Status => {
            if is_running() {
                println!("Runner is running (socket: {})", socket_path);
            } else {
                println!("Runner is not running");
            }
            Ok(())
        }
        RunnerCommand::Enable => {
            // Create systemd user service
            let home = env::var("HOME").context("HOME not set")?;
            let service_dir = format!("{}/.config/systemd/user", home);
            std::fs::create_dir_all(&service_dir)?;

            let runner_path = find_runner()?;
            let service_content = format!(
                r#"[Unit]
Description=Octo Runner - Process isolation daemon
After=default.target

[Service]
Type=simple
ExecStart={}
Restart=on-failure
RestartSec=5

[Install]
WantedBy=default.target
"#,
                runner_path.display()
            );

            let service_path = format!("{}/octo-runner.service", service_dir);
            std::fs::write(&service_path, service_content)?;

            // Reload systemd and enable
            StdCommand::new("systemctl")
                .args(["--user", "daemon-reload"])
                .status()?;
            StdCommand::new("systemctl")
                .args(["--user", "enable", "octo-runner.service"])
                .status()?;

            println!("Enabled octo-runner.service");
            println!("Run 'octo runner start' or 'systemctl --user start octo-runner' to start");
            Ok(())
        }
        RunnerCommand::Disable => {
            StdCommand::new("systemctl")
                .args(["--user", "disable", "octo-runner.service"])
                .status()?;
            println!("Disabled octo-runner.service");
            Ok(())
        }
    }
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

    // Repositories that need to be available during early service construction.
    let user_repo_for_services = user::UserRepository::new(database.pool().clone());

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

        // Load sandbox config from separate file (~/.config/octo/sandbox.toml)
        let sandbox_config = match local::SandboxConfig::load_global() {
            Ok(config) => {
                if config.enabled {
                    info!("Sandbox enabled globally");
                }
                Some(config)
            }
            Err(e) => {
                warn!("Failed to load sandbox config: {}", e);
                None
            }
        };

        let mut local_config = local::LocalRuntimeConfig {
            opencode_binary: ctx.config.local.opencode_binary.clone(),
            fileserver_binary: ctx.config.local.fileserver_binary.clone(),
            ttyd_binary: ctx.config.local.ttyd_binary.clone(),
            workspace_dir: ctx.config.local.workspace_dir.clone(),
            default_agent: ctx.config.local.default_agent.clone(),
            single_user: ctx.config.local.single_user,
            linux_users: linux_users_config,
            sandbox: sandbox_config,
            cleanup_on_startup: ctx.config.local.cleanup_on_startup,
            stop_sessions_on_shutdown: ctx.config.local.stop_sessions_on_shutdown,
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

    // User data path: CLI overrides config, config overrides default.
    // In local mode, default to the standard data directory (~/.local/share/octo)
    // so that Main Chat workspace paths are properly allowed.
    let user_data_path = if cmd.user_data_path != std::path::PathBuf::from("./data") {
        // CLI explicitly set
        cmd.user_data_path.clone()
    } else if let Some(ref config_path) = ctx.config.container.user_data_path {
        // Use config file value
        std::path::PathBuf::from(shellexpand::tilde(config_path).to_string())
    } else if local_mode {
        // In local mode, use the standard data directory so Main Chat paths are allowed
        ctx.paths.data_dir.clone()
    } else {
        // Use CLI default for container mode
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
        // Enable pi-bridge in containers when Pi is enabled and runtime mode is container
        pi_bridge_enabled: ctx.config.pi.enabled
            && ctx.config.pi.runtime_mode == PiRuntimeMode::Container,
        pi_provider: ctx.config.pi.default_provider.clone(),
        pi_model: ctx.config.pi.default_model.clone(),
        agent_browser: ctx.config.agent_browser.clone(),
        runner_socket_pattern: ctx.config.local.runner_socket_pattern.clone(),
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
    if !local_mode && let Some(ref runtime) = container_runtime {
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

    // Create session service based on runtime mode
    let mut session_service = if local_mode {
        let local_rt = local_runtime.expect("local runtime should be set in local mode");
        let runner = runner::client::RunnerClient::default();
        if let Some(eavs) = eavs_client.clone() {
            session::SessionService::with_runner_and_eavs(
                session_repo,
                runner,
                local_rt,
                eavs,
                session_config.clone(),
            )
        } else {
            session::SessionService::with_runner(
                session_repo,
                runner,
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

    let mut sldr_users: Option<local::UserSldrManager> = None;

    // Enable per-user mmry instances in local multi-user mode.
    if local_mode
        && !single_user
        && ctx.config.mmry.enabled
        && let Some(ref local_cfg) = session_config.local_config
    {
        if !local_cfg.linux_users.enabled {
            warn!("mmry per-user instances require local.linux_users.enabled=true (skipping)");
        } else {
            let linux_users = local_cfg.linux_users.clone();
            let user_mmry = local::UserMmryManager::new(
                local::UserMmryConfig {
                    mmry_binary: ctx.config.mmry.binary.clone(),
                    base_port: ctx.config.mmry.user_base_port,
                    port_range: ctx.config.mmry.user_port_range,
                    runner_socket_pattern: ctx.config.local.runner_socket_pattern.clone(),
                },
                move |user_id| linux_users.linux_username(user_id),
                user_repo_for_services.clone(),
            );
            session_service = session_service.with_user_mmry(user_mmry);
        }
    }

    // Enable per-user sldr instances in local multi-user mode.
    if local_mode
        && !single_user
        && ctx.config.sldr.enabled
        && let Some(ref local_cfg) = session_config.local_config
    {
        if !local_cfg.linux_users.enabled {
            warn!("sldr per-user instances require local.linux_users.enabled=true (skipping)");
        } else {
            let linux_users = local_cfg.linux_users.clone();
            let user_sldr = local::UserSldrManager::new(
                local::UserSldrConfig {
                    sldr_binary: ctx.config.sldr.binary.clone(),
                    base_port: ctx.config.sldr.user_base_port,
                    port_range: ctx.config.sldr.user_port_range,
                    runner_socket_pattern: ctx.config.local.runner_socket_pattern.clone(),
                },
                move |user_id| linux_users.linux_username(user_id),
                user_repo_for_services.clone(),
            );
            sldr_users = Some(user_sldr);
        }
    }

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
                // Load sandbox config from separate file
                let sandbox_config = local::SandboxConfig::load_global().ok();

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
                    sandbox: sandbox_config,
                    cleanup_on_startup: ctx.config.local.cleanup_on_startup,
                    stop_sessions_on_shutdown: ctx.config.local.stop_sessions_on_shutdown,
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
        host_service_url: ctx.config.mmry.host_service_url.clone(),
        host_api_key: ctx.config.mmry.host_api_key.clone(),
        user_base_port: ctx.config.mmry.user_base_port,
        user_port_range: ctx.config.mmry.user_port_range,
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
    let templates_state = api::TemplatesState::new(
        ctx.config.templates.repo_path.as_deref().map(PathBuf::from),
        ctx.config.templates.repo_type,
        ctx.config.templates.sync_on_list,
        Duration::from_secs(ctx.config.templates.sync_interval_seconds),
    );
    templates_state.start_background_sync();

    let max_proxy_body_bytes = ctx
        .config
        .server
        .max_upload_size_mb
        .saturating_mul(1024 * 1024);

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

    // Create Pi agent settings services (settings.json + models.json)
    let pi_schema_root = PathBuf::from("/home/wismut/byteowlz/schemas/pi-agent");
    let pi_settings_schema = std::fs::read_to_string(pi_schema_root.join("settings.schema.json"))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_else(|| {
            serde_json::json!({
                "$schema": "http://json-schema.org/draft-07/schema#",
                "title": "Pi Agent Settings",
                "type": "object",
                "properties": {}
            })
        });
    let pi_models_schema = std::fs::read_to_string(pi_schema_root.join("models.schema.json"))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_else(|| {
            serde_json::json!({
                "$schema": "http://json-schema.org/draft-07/schema#",
                "title": "Pi Agent Models",
                "type": "object",
                "properties": {}
            })
        });

    let pi_config_dir = dirs::home_dir()
        .map(|home| home.join(".pi").join("agent"))
        .unwrap_or_else(|| PathBuf::from(".pi/agent"));

    let settings_pi_agent = settings::SettingsService::new_json(
        pi_settings_schema,
        pi_config_dir.clone(),
        "settings.json",
    )
    .ok();
    let settings_pi_models =
        settings::SettingsService::new_json(pi_models_schema, pi_config_dir, "models.json").ok();

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
            templates_state.clone(),
            max_proxy_body_bytes,
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
            templates_state.clone(),
            max_proxy_body_bytes,
        )
    };
    state = state.with_feedback_config(ctx.config.feedback.clone());

    if let Err(err) = feedback::ensure_feedback_dirs(&ctx.config.feedback) {
        warn!("Failed to initialize feedback directories: {}", err);
    } else {
        let feedback_config = ctx.config.feedback.clone();
        tokio::spawn(async move {
            feedback::sync_feedback_loop(feedback_config).await;
        });
    }

    // Add settings services to state
    state = state.with_settings_octo(settings_octo);
    if let Some(mmry_settings) = settings_mmry {
        state = state.with_settings_mmry(mmry_settings);
    }
    if let Some(pi_settings) = settings_pi_agent {
        state = state.with_settings_pi_agent(pi_settings);
    }
    if let Some(pi_models) = settings_pi_models {
        state = state.with_settings_pi_models(pi_models);
    }

    if let Some(manager) = sldr_users {
        state = state.with_sldr_users(manager);
    }

    // Add onboarding service
    let onboarding_service = onboarding::OnboardingService::new(database.pool().clone());
    state = state.with_onboarding(onboarding_service);
    info!("Onboarding service initialized");

    // Add Linux users config for multi-user isolation
    if ctx.config.local.linux_users.enabled {
        let linux_users_config = local::LinuxUsersConfig {
            enabled: ctx.config.local.linux_users.enabled,
            prefix: ctx.config.local.linux_users.prefix.clone(),
            uid_start: ctx.config.local.linux_users.uid_start,
            group: ctx.config.local.linux_users.group.clone(),
            shell: ctx.config.local.linux_users.shell.clone(),
            use_sudo: ctx.config.local.linux_users.use_sudo,
            create_home: ctx.config.local.linux_users.create_home,
        };
        state = state.with_linux_users(linux_users_config);
        // Also set runner socket pattern for multi-user chat history access
        state = state.with_runner_socket_pattern(ctx.config.local.runner_socket_pattern.clone());
    }

    // Initialize onboarding templates service
    let onboarding_templates_service = templates::OnboardingTemplatesService::new(
        ctx.config.onboarding_templates.clone(),
        &ctx.paths.data_dir,
    );
    info!("Onboarding templates service initialized");
    state = state.with_onboarding_templates(onboarding_templates_service);

    // Initialize hstry (chat history) service
    if ctx.config.hstry.enabled {
        // Always auto-start hstry daemon and connect.
        // In multi-user mode, per-user hstry instances may also be spawned via
        // runner, but the main octo process needs its own client for session listing.
        let hstry_config = hstry::HstryServiceConfig {
            binary: ctx.config.hstry.binary.clone(),
            auto_start: true,
            startup_timeout: std::time::Duration::from_secs(10),
        };
        let hstry_manager = hstry::HstryServiceManager::new(hstry_config);

        // Ensure daemon is running (auto-starts if needed)
        match hstry_manager.ensure_running().await {
            Ok(()) => {
                info!("hstry daemon is running");
                // Create client and connect
                let hstry_client = hstry::HstryClient::new();
                if let Err(e) = hstry_client.connect().await {
                    warn!(
                        "Failed to connect to hstry daemon: {}. Will retry on first use.",
                        e
                    );
                } else {
                    info!("hstry client connected");
                }
                state = state.with_hstry(hstry_client);
            }
            Err(e) => {
                warn!(
                    "Failed to start hstry daemon: {}. Chat history persistence disabled.",
                    e
                );
            }
        }
    } else {
        debug!("hstry integration disabled");
    }

    // Create router - all API routes are served under /api prefix only.
    // This is the single source of truth for routing. All clients (frontend,
    // internal services, containers) must use /api/* paths.
    let api_router = api::create_router_with_config(state, ctx.config.server.max_upload_size_mb);
    let app = axum::Router::new().nest("/api", api_router);

    // Bind and serve
    let addr: SocketAddr = format!("{}:{}", cmd.host, cmd.port)
        .parse()
        .context("invalid address")?;

    info!("Listening on http://{}", addr);

    let listener = TcpListener::bind(addr)
        .await
        .context("binding to address")?;

    let stop_sessions_on_shutdown = !local_mode || ctx.config.local.stop_sessions_on_shutdown;

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

        // Stop all running containers/sessions if enabled
        if stop_sessions_on_shutdown {
            if let Err(e) = shutdown_all_sessions(&session_service_for_shutdown).await {
                warn!("Error during shutdown: {:?}", e);
            }
        } else {
            info!("Skipping session shutdown (preserve running local sessions)");
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
