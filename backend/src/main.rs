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
mod api;
mod auth;
mod container;
mod db;
mod eavs;
mod invite;
mod local;
mod session;
mod user;
mod wordlist;

const APP_NAME: &str = "octo";

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

        let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
            EnvFilter::new(format!("octo={level},tower_http={level}"))
        });

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
    container: ContainerRuntimeConfig,
    local: LocalModeConfig,
    eavs: Option<EavsConfig>,
    auth: auth::AuthConfig,
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
            container: ContainerRuntimeConfig::default(),
            local: LocalModeConfig::default(),
            eavs: None,
            auth: auth::AuthConfig::default(),
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

impl Default for LocalModeConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            opencode_binary: "opencode".to_string(),
            fileserver_binary: "fileserver".to_string(),
            ttyd_binary: "ttyd".to_string(),
            workspace_dir: "$HOME/octo/{user_id}".to_string(),
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
    let local_mode = cmd.local_mode || ctx.config.local.enabled;
    let runtime_mode = if local_mode {
        session::RuntimeMode::Local
    } else {
        session::RuntimeMode::Container
    };
    info!("Runtime mode: {:?}", runtime_mode);

    // Initialize runtimes based on mode
    let container_runtime: Option<std::sync::Arc<container::ContainerRuntime>> = if !local_mode {
        let runtime = match (&ctx.config.container.runtime, &ctx.config.container.binary) {
            (Some(rt), Some(binary)) => container::ContainerRuntime::with_binary(*rt, binary.clone()),
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
            single_user: ctx.config.local.single_user,
            linux_users: linux_users_config,
        };
        local_config.expand_paths();

        // Validate that all binaries are available
        if let Err(e) = local_config.validate() {
            error!("Local mode validation failed: {:?}", e);
            anyhow::bail!("Local mode requires opencode, fileserver, and ttyd binaries. Error: {}", e);
        }

        // Check Linux user isolation privileges if enabled
        if local_config.linux_users.enabled {
            if let Err(e) = local_config.linux_users.check_privileges() {
                error!("Linux user isolation check failed: {:?}", e);
                anyhow::bail!("Linux user isolation requires root or sudo privileges. Error: {}", e);
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
        eavs_container_url: ctx
            .config
            .eavs
            .as_ref()
            .and_then(|e| e.container_url.clone()),
        runtime_mode,
        local_config: local_runtime_config,
        single_user,
    };

    let session_repo = session::SessionRepository::new(database.pool().clone());

    // Initialize EAVS client if configured
    let eavs_client: Option<std::sync::Arc<dyn eavs::EavsApi>> = if let Some(ref eavs_config) =
        ctx.config.eavs
    {
        if eavs_config.enabled {
            if let Some(ref master_key) = eavs_config.master_key {
                Some(std::sync::Arc::new(eavs::EavsClient::new(
                    &eavs_config.base_url,
                    master_key,
                )))
            } else if let Ok(master_key) = std::env::var("EAVS_MASTER_KEY") {
                Some(std::sync::Arc::new(eavs::EavsClient::new(
                    &eavs_config.base_url,
                    master_key,
                )))
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
            session::SessionService::with_local_runtime_and_eavs(session_repo, local_rt, eavs, session_config)
        } else {
            session::SessionService::with_local_runtime(session_repo, local_rt, session_config)
        }
    } else {
        let container_rt = container_runtime.clone().expect("container runtime should be set in container mode");
        if let Some(eavs) = eavs_client.clone() {
            session::SessionService::with_eavs(session_repo, container_rt, eavs, session_config)
        } else {
            session::SessionService::new(session_repo, container_rt, session_config)
        }
    };

    // Run startup cleanup to handle orphan containers and stale sessions
    if let Err(e) = session_service.startup_cleanup().await {
        warn!("Startup cleanup failed (continuing anyway): {:?}", e);
    }

    // Initialize agent service for managing opencode instances
    // In local mode, we use a dummy container runtime (agent features limited)
    let agent_runtime: std::sync::Arc<dyn container::ContainerRuntimeApi> = if let Some(ref rt) = container_runtime {
        rt.clone()
    } else {
        // Create a container runtime for agent service even in local mode
        // This allows basic agent operations to work (though docker exec will fail)
        std::sync::Arc::new(container::ContainerRuntime::new())
    };
    let agent_repo = agent::AgentRepository::new(database.pool().clone());
    let agent_service = agent::AgentService::new(agent_runtime, session_service.clone(), agent_repo);

    // Initialize user service
    let user_repo = user::UserRepository::new(database.pool().clone());
    let user_service = user::UserService::new(user_repo);

    // Initialize invite code repository
    let invite_repo = invite::InviteCodeRepository::new(database.pool().clone());

    // Clone session_service before creating state for shutdown handler
    let session_service_for_shutdown = session_service.clone();
    
    // Create app state
    let state = api::AppState::new(session_service, agent_service, user_service, invite_repo, auth_state);

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

    axum::serve(listener, app)
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
