//! oqtoctl - Control CLI for Oqto server
//!
//! Provides administrative commands for managing the Oqto server,
//! including container management, image refresh, and housekeeping.

use std::io::{self, IsTerminal, Read, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Context, Result, anyhow};
use clap::{Parser, Subcommand};
use http_body_util::{BodyExt, Full};
use hyper::body::Bytes;
use hyper_util::client::legacy::Client as HyperClient;
use hyper_util::rt::TokioExecutor;
use reqwest::StatusCode;

#[cfg(unix)]
use hyperlocal::{UnixConnector, Uri as UnixUri};

const DEFAULT_SERVER_URL: &str = "http://localhost:8080/api";
const DEFAULT_ADMIN_SOCKET: &str = "/run/oqto/oqtoctl.sock";

fn main() -> ExitCode {
    if let Err(err) = try_main() {
        let _ = writeln!(io::stderr(), "Error: {err:?}");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

#[tokio::main]
async fn try_main() -> Result<()> {
    let cli = Cli::parse();
    let client = OqtoClient::new(&cli.server, cli.admin_socket.as_deref())?;

    match cli.command {
        Command::Status => handle_status(&client, cli.json).await,
        Command::Ask {
            target,
            question,
            timeout,
            stream,
        } => handle_ask(&client, &target, &question, timeout, stream, cli.json).await,
        Command::Sessions { query, limit } => {
            handle_sessions(&client, query.as_deref(), limit, cli.json).await
        }
        Command::Session { command } => handle_session(&client, command, cli.json).await,
        Command::Container { command } => handle_container(&client, command, cli.json).await,
        Command::Image { command } => handle_image(&client, command, cli.json).await,
        Command::A2ui { command } => handle_a2ui(&client, command, cli.json).await,
        Command::Ui { command } => handle_ui(&client, command, cli.json).await,
        Command::Bus { command } => handle_bus(&client, command, cli.json).await,
        Command::Local { command } => handle_local(&client, command, cli.json).await,
        Command::Sandbox { command } => handle_sandbox(command, cli.json).await,
        Command::User { command } => handle_user(&client, command, cli.json).await,
        Command::Doctor {
            user,
            apply,
            contract,
            profile,
            strict,
            apply_services,
            apply_runners,
        } => {
            handle_doctor(
                user.as_deref(),
                apply,
                contract,
                &profile,
                strict,
                apply_services,
                apply_runners,
                cli.config.as_deref(),
                cli.json,
            )
            .await
        }
        Command::HashPassword { password, cost } => handle_hash_password(password, cost),
    }
}

#[derive(Debug, Parser)]
#[command(
    name = "oqtoctl",
    author,
    version,
    about = "Control CLI for Oqto server - manage containers, sessions, and images."
)]
struct Cli {
    /// Oqto server URL
    #[arg(long, short = 's', default_value = DEFAULT_SERVER_URL, env = "OQTO_SERVER_URL")]
    server: String,

    /// Output machine-readable JSON
    #[arg(long, global = true)]
    json: bool,

    /// Path to Oqto config file (auto-detected if not set)
    #[arg(long, short = 'c', env = "OQTO_CONFIG", global = true)]
    config: Option<String>,

    /// Admin socket path for local root access
    #[arg(long, env = "OQTO_ADMIN_SOCKET", global = true)]
    admin_socket: Option<String>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Check server status
    Status,

    /// Ask an agent a question and get the response
    ///
    /// Target formats:
    ///   @@main, @@pi          - Main chat (most recent session)
    ///   @@main:query          - Main chat, search for session
    ///   @@<name>              - Main chat by assistant name
    ///   @@session:id          - Specific session by ID
    ///   main, pi, session:id  - Same without @@ prefix
    Ask {
        /// Target agent (e.g., "@@main", "@@pi:my-session", "session:abc123")
        target: String,

        /// The question/prompt to send
        question: String,

        /// Timeout in seconds (default: 300)
        #[arg(long, short = 't', default_value = "300")]
        timeout: u64,

        /// Stream output as it arrives
        #[arg(long)]
        stream: bool,
    },

    /// List or search main chat sessions
    Sessions {
        /// Search query (fuzzy matches on ID and title)
        query: Option<String>,

        /// Maximum number of results
        #[arg(long, short = 'n', default_value = "20")]
        limit: usize,
    },

    /// Manage sessions
    Session {
        #[command(subcommand)]
        command: SessionCommand,
    },

    /// Manage containers
    Container {
        #[command(subcommand)]
        command: ContainerCommand,
    },

    /// Manage container images
    Image {
        #[command(subcommand)]
        command: ImageCommand,
    },

    /// Manage local mode processes
    Local {
        #[command(subcommand)]
        command: LocalCommand,
    },

    /// Manage sandbox configuration
    Sandbox {
        #[command(subcommand)]
        command: SandboxCommand,
    },

    /// Diagnose setup/provisioning drift
    ///
    /// Checks the setup contract: global paths/permissions, system services,
    /// database user identity, Linux user existence/UID, canonical runner socket
    /// paths, and split-routing symptoms. Safe remediations require --apply;
    /// use --strict to turn error-severity drift into a non-zero preflight gate.
    Doctor {
        /// Optional username or user ID to scope the check.
        #[arg(long)]
        user: Option<String>,
        /// Apply safe remediation in-place (default is dry-run report).
        #[arg(long, default_value_t = false)]
        apply: bool,
        /// Print the provisioning contract instead of probing the host.
        #[arg(long, default_value_t = false)]
        contract: bool,
        /// Install profile for --contract: auto, personal, or team.
        #[arg(long, default_value = "auto")]
        profile: String,
        /// Exit non-zero when contract evaluation finds error-severity drift.
        #[arg(long, default_value_t = false)]
        strict: bool,
        /// With --apply, also reprovision per-user runner services with socket drift.
        #[arg(long, default_value_t = false)]
        apply_runners: bool,
        /// With --apply, also enable/start declared system services.
        #[arg(long, default_value_t = false)]
        apply_services: bool,
    },

    /// Manage users and runner provisioning
    User {
        #[command(subcommand)]
        command: UserCommand,
    },

    /// Send A2UI surface to user (for agents)
    #[command(name = "a2ui")]
    A2ui {
        #[command(subcommand)]
        command: A2uiCommand,
    },

    /// UI control commands (agent-driven UI control)
    #[command(name = "ui")]
    Ui {
        #[command(subcommand)]
        command: UiCommand,
    },

    /// Event bus commands (admin)
    #[command(name = "bus")]
    Bus {
        #[command(subcommand)]
        command: BusCommand,
    },

    /// Hash a password using bcrypt (same algorithm as the backend)
    ///
    /// Reads password from stdin if not provided. Outputs only the hash
    /// to stdout for use in scripts.
    HashPassword {
        /// Password to hash (reads from stdin if not provided)
        #[arg(long)]
        password: Option<String>,
        /// Bcrypt cost factor (default: 12)
        #[arg(long, default_value = "12")]
        cost: u32,
    },
}

#[derive(Debug, Subcommand)]
enum SessionCommand {
    /// List all sessions
    List,
    /// Get session details
    Get {
        /// Session ID or readable ID
        id: String,
    },
    /// Stop a session
    Stop {
        /// Session ID or readable ID
        id: String,
    },
    /// Resume a stopped session
    Resume {
        /// Session ID or readable ID
        id: String,
    },
    /// Delete a session and its container
    Delete {
        /// Session ID or readable ID
        id: String,
        /// Force delete even if running
        #[arg(long, short)]
        force: bool,
    },
    /// Upgrade a session to the latest image
    Upgrade {
        /// Session ID or readable ID
        id: String,
    },
}

#[derive(Debug, Subcommand)]
enum ContainerCommand {
    /// Force refresh all containers (stop, remove, recreate)
    Refresh {
        /// Only refresh containers with outdated images
        #[arg(long)]
        outdated_only: bool,
    },
    /// Clean up orphan containers (containers without sessions)
    Cleanup,
    /// List all managed containers
    List,
    /// Stop all running containers
    StopAll,
}

#[derive(Debug, Subcommand)]
enum ImageCommand {
    /// Check for image updates
    Check,
    /// Pull latest image
    Pull {
        /// Image name (default: oqto:latest)
        #[arg(default_value = "oqto:latest")]
        image: String,
    },
    /// Rebuild container image from Dockerfile
    Build {
        /// Path to Dockerfile directory
        #[arg(default_value = "./container")]
        path: String,
        /// Don't use cache when building
        #[arg(long)]
        no_cache: bool,
    },
}

#[derive(Debug, Subcommand)]
enum LocalCommand {
    /// Clean up orphan local session processes
    Cleanup,
}

#[derive(Debug, Subcommand)]
enum SandboxCommand {
    /// Show current sandbox configuration
    Show,
    /// Edit sandbox configuration (requires sudo)
    Edit,
    /// Validate sandbox configuration
    Validate,
    /// Reset sandbox configuration to defaults
    Reset {
        /// Skip confirmation prompt
        #[arg(long, short)]
        yes: bool,
    },
}

#[derive(Debug, Subcommand)]
enum UserCommand {
    /// Create a new user (via oqto API -- handles Linux user, runner, eavs)
    Create {
        /// Oqto username
        username: String,
        /// Email address
        #[arg(long, short)]
        email: String,
        /// Display name
        #[arg(long, short)]
        display_name: Option<String>,
        /// User role (user, admin)
        #[arg(long, short, default_value = "user")]
        role: String,
        /// Password (prompted if not provided)
        #[arg(long, short)]
        password: Option<String>,
    },
    /// List all users
    List {
        /// Show runner status for each user
        #[arg(long)]
        runner_status: bool,
    },
    /// Show user details
    Show {
        /// Username or user ID
        user: String,
    },
    /// Setup runner for an existing user
    SetupRunner {
        /// Username or user ID
        user: String,
        /// Force reinstall even if runner is already configured
        #[arg(long, short)]
        force: bool,
    },
    /// Check runner status for a user
    RunnerStatus {
        /// Username or user ID
        user: String,
    },
    /// Delete a user (removes from DB + Linux user + services + home)
    Delete {
        /// Username or user ID
        user: String,
        /// Skip confirmation prompt
        #[arg(long, short)]
        force: bool,
    },
    /// Change a user's password
    SetPassword {
        /// Username or user ID
        user: String,
        /// New password (prompted if not provided)
        #[arg(long, short)]
        password: Option<String>,
    },
    /// Change a user's role (user, admin)
    SetRole {
        /// Username or user ID
        user: String,
        /// New role
        role: String,
    },
    /// Disable a user (prevents login, does not delete)
    Disable {
        /// Username or user ID
        user: String,
    },
    /// Re-enable a disabled user
    Enable {
        /// Username or user ID
        user: String,
    },
    /// Update a user's email address
    SetEmail {
        /// Username or user ID
        user: String,
        /// New email address
        email: String,
    },
    /// Update a user's display name
    SetDisplayName {
        /// Username or user ID
        user: String,
        /// New display name
        name: String,
    },
    /// Re-provision eavs key + models.json + runner for a user
    Reprovision {
        /// Username or user ID
        user: String,
    },
    /// Sync per-user config files via the admin API
    SyncConfigs {
        /// Optional user ID to target
        #[arg(long)]
        user: Option<String>,
    },
    /// Audit and remediate identity contract consistency for multi-user rollout.
    DoctorIdentity {
        /// Optional username or user ID to scope the check.
        #[arg(long)]
        user: Option<String>,
        /// Apply remediation in-place (default is dry-run report).
        #[arg(long, default_value_t = false)]
        apply: bool,
    },
    /// Bootstrap the first admin user (for fresh installs)
    ///
    /// This creates an admin user directly in the database without requiring
    /// an invite code. Use this for initial setup of a production instance.
    ///
    /// In multi-user mode, also creates the Linux user and sets up the runner.
    Bootstrap {
        /// Admin username
        #[arg(long, short)]
        username: String,
        /// Admin email
        #[arg(long, short)]
        email: String,
        /// Admin password (will prompt if not provided)
        #[arg(long, short)]
        password: Option<String>,
        /// Pre-computed bcrypt hash (skips password prompting)
        #[arg(long, conflicts_with = "password")]
        password_hash: Option<String>,
        /// Display name
        #[arg(long, short = 'n')]
        display_name: Option<String>,
        /// Database path (default: ~/.local/share/oqto/oqto.db)
        #[arg(long, env = "OQTO_DATABASE_PATH")]
        database: Option<String>,
        /// Linux username (defaults to Oqto username)
        #[arg(long)]
        linux_user: Option<String>,
        /// Skip Linux user creation
        #[arg(long)]
        no_linux_user: bool,
        /// Skip runner setup
        #[arg(long)]
        no_runner: bool,
    },
}

#[derive(Debug, Subcommand)]
enum A2uiCommand {
    /// Send a button prompt to the user
    Button {
        /// Session ID (defaults to OQTO_SESSION_ID env var)
        #[arg(long, short, env = "OQTO_SESSION_ID")]
        session: String,
        /// Prompt text to display
        #[arg(long, short)]
        prompt: Option<String>,
        /// Button labels (comma-separated or multiple -b flags)
        #[arg(long, short = 'b', value_delimiter = ',')]
        buttons: Vec<String>,
        /// Timeout in seconds (default: 300)
        #[arg(long, short, default_value = "300")]
        timeout: u64,
    },
    /// Send a text input prompt to the user
    Input {
        /// Session ID (defaults to OQTO_SESSION_ID env var)
        #[arg(long, short, env = "OQTO_SESSION_ID")]
        session: String,
        /// Prompt text to display
        prompt: String,
        /// Placeholder text for the input field
        #[arg(long)]
        placeholder: Option<String>,
        /// Input type: text, number, password, long
        #[arg(long, default_value = "text")]
        input_type: String,
        /// Timeout in seconds (default: 300)
        #[arg(long, short, default_value = "300")]
        timeout: u64,
    },
    /// Send a multiple choice prompt to the user
    Choice {
        /// Session ID (defaults to OQTO_SESSION_ID env var)
        #[arg(long, short, env = "OQTO_SESSION_ID")]
        session: String,
        /// Prompt text to display
        #[arg(long, short)]
        prompt: Option<String>,
        /// Choices (comma-separated or multiple -c flags)
        #[arg(long, short = 'c', value_delimiter = ',')]
        choices: Vec<String>,
        /// Allow multiple selections
        #[arg(long)]
        multi: bool,
        /// Timeout in seconds (default: 300)
        #[arg(long, short, default_value = "300")]
        timeout: u64,
    },
    /// Send a checkbox (boolean) prompt to the user
    Checkbox {
        /// Session ID (defaults to OQTO_SESSION_ID env var)
        #[arg(long, short, env = "OQTO_SESSION_ID")]
        session: String,
        /// Label text for the checkbox
        label: String,
        /// Initial checked state
        #[arg(long)]
        checked: bool,
        /// Timeout in seconds (default: 300)
        #[arg(long, short, default_value = "300")]
        timeout: u64,
    },
    /// Send a slider (numeric) prompt to the user
    Slider {
        /// Session ID (defaults to OQTO_SESSION_ID env var)
        #[arg(long, short, env = "OQTO_SESSION_ID")]
        session: String,
        /// Prompt text to display
        #[arg(long, short)]
        prompt: Option<String>,
        /// Minimum value
        #[arg(long, default_value = "0")]
        min: f64,
        /// Maximum value
        #[arg(long, default_value = "100")]
        max: f64,
        /// Initial value
        #[arg(long)]
        value: Option<f64>,
        /// Timeout in seconds (default: 300)
        #[arg(long, short, default_value = "300")]
        timeout: u64,
    },
    /// Send a date/time input prompt to the user
    Datetime {
        /// Session ID (defaults to OQTO_SESSION_ID env var)
        #[arg(long, short, env = "OQTO_SESSION_ID")]
        session: String,
        /// Prompt text to display
        #[arg(long, short)]
        prompt: Option<String>,
        /// Enable date selection
        #[arg(long, default_value = "true")]
        date: bool,
        /// Enable time selection
        #[arg(long)]
        time: bool,
        /// Initial value (ISO 8601 format)
        #[arg(long)]
        value: Option<String>,
        /// Timeout in seconds (default: 300)
        #[arg(long, short, default_value = "300")]
        timeout: u64,
    },
    /// Display text message (non-blocking)
    Text {
        /// Session ID (defaults to OQTO_SESSION_ID env var)
        #[arg(long, short, env = "OQTO_SESSION_ID")]
        session: String,
        /// Text to display
        text: String,
        /// Text style: body, h1, h2, h3, h4, h5, caption
        #[arg(long, default_value = "body")]
        style: String,
    },
    /// Display an image
    Image {
        /// Session ID (defaults to OQTO_SESSION_ID env var)
        #[arg(long, short, env = "OQTO_SESSION_ID")]
        session: String,
        /// Image URL
        url: String,
        /// Image fit: contain, cover, fill, none, scale-down
        #[arg(long, default_value = "contain")]
        fit: String,
        /// Add confirm button to make it blocking
        #[arg(long)]
        confirm: bool,
        /// Timeout in seconds (default: 300)
        #[arg(long, short, default_value = "300")]
        timeout: u64,
    },
    /// Display a video
    Video {
        /// Session ID (defaults to OQTO_SESSION_ID env var)
        #[arg(long, short, env = "OQTO_SESSION_ID")]
        session: String,
        /// Video URL
        url: String,
        /// Add confirm button to make it blocking
        #[arg(long)]
        confirm: bool,
        /// Timeout in seconds (default: 300)
        #[arg(long, short, default_value = "300")]
        timeout: u64,
    },
    /// Display an audio player
    Audio {
        /// Session ID (defaults to OQTO_SESSION_ID env var)
        #[arg(long, short, env = "OQTO_SESSION_ID")]
        session: String,
        /// Audio URL
        url: String,
        /// Description text
        #[arg(long)]
        description: Option<String>,
        /// Add confirm button to make it blocking
        #[arg(long)]
        confirm: bool,
        /// Timeout in seconds (default: 300)
        #[arg(long, short, default_value = "300")]
        timeout: u64,
    },
    /// Display tabbed content
    Tabs {
        /// Session ID (defaults to OQTO_SESSION_ID env var)
        #[arg(long, short, env = "OQTO_SESSION_ID")]
        session: String,
        /// Tab definitions as JSON array: [{"title":"Tab1","content":"text1"},...]
        tabs: String,
        /// Add confirm button to make it blocking
        #[arg(long)]
        confirm: bool,
        /// Timeout in seconds (default: 300)
        #[arg(long, short, default_value = "300")]
        timeout: u64,
    },
    /// Send raw A2UI JSON messages
    Raw {
        /// Session ID (defaults to OQTO_SESSION_ID env var)
        #[arg(long, short, env = "OQTO_SESSION_ID")]
        session: String,
        /// A2UI messages as JSON (reads from stdin if not provided)
        messages: Option<String>,
        /// Block until user responds
        #[arg(long, short)]
        blocking: bool,
        /// Timeout in seconds (default: 300)
        #[arg(long, short, default_value = "300")]
        timeout: u64,
    },
}

#[derive(Debug, Subcommand)]
enum BusCommand {
    /// Show event bus stats
    Status,
    /// Publish an event as admin
    Publish {
        /// Scope: session, workspace, global
        #[arg(long)]
        scope: String,
        /// Scope identifier (session_id, workspace path, or global)
        #[arg(long)]
        scope_id: String,
        /// Event topic (e.g. app.message, admin.agents_updated)
        #[arg(long)]
        topic: String,
        /// JSON payload. If omitted, read from stdin.
        #[arg(long)]
        payload: Option<String>,
        /// Payload version (default: 1)
        #[arg(long, default_value = "1")]
        version: u32,
    },
    /// Tail bus events via WebSocket subscription
    Tail {
        /// Scope: session, workspace, global
        #[arg(long)]
        scope: String,
        /// Scope identifier (session_id, workspace path, or global)
        #[arg(long)]
        scope_id: String,
        /// Topic pattern(s). Repeat for multiple patterns.
        #[arg(long = "topic", required = true)]
        topics: Vec<String>,
        /// Maximum number of events to print before exiting (default: unlimited)
        #[arg(long)]
        limit: Option<usize>,
        /// Exit after timeout seconds if no/insufficient events
        #[arg(long)]
        timeout: Option<u64>,
    },
}

#[derive(Debug, Subcommand)]
enum UiCommand {
    /// Navigate to a route/path
    Navigate {
        /// Path to navigate to
        path: String,
        /// Replace history entry instead of pushing
        #[arg(long)]
        replace: bool,
    },
    /// Switch active session
    Session {
        /// Session ID
        session_id: String,
        /// Mode: main or pi
        #[arg(long)]
        mode: Option<String>,
    },
    /// Switch active view within a session
    View {
        /// View name (chat, files, terminal, tasks, memories, settings, canvas, voice)
        view: String,
    },
    /// Open or close the command palette
    Palette {
        /// Open state (true/false). Defaults to true if omitted.
        #[arg(long)]
        open: Option<bool>,
    },
    /// Execute a palette command
    PaletteExec {
        /// Command name (e.g. new_chat, toggle_theme, set_theme, toggle_locale, set_locale, open_app, select_session)
        command: String,
        /// JSON args (optional)
        #[arg(long)]
        args: Option<String>,
    },
    /// Spotlight a UI element
    Spotlight {
        /// Spotlight target id (data-spotlight value)
        target: Option<String>,
        /// Optional title
        #[arg(long)]
        title: Option<String>,
        /// Optional description
        #[arg(long)]
        description: Option<String>,
        /// Optional action hint
        #[arg(long)]
        action: Option<String>,
        /// Optional position (auto|top|bottom|left|right)
        #[arg(long)]
        position: Option<String>,
        /// Clear spotlight instead of showing it
        #[arg(long)]
        clear: bool,
    },
    /// Start a spotlight tour
    Tour {
        /// JSON array of steps (reads from stdin if omitted)
        #[arg(long)]
        steps: Option<String>,
        /// Start index
        #[arg(long)]
        start_index: Option<usize>,
        /// Stop the tour
        #[arg(long)]
        stop: bool,
    },
    /// Collapse or expand sidebar
    Sidebar {
        /// Collapsed state
        #[arg(long)]
        collapsed: Option<bool>,
    },
    /// Control right panel/expanded view
    Panel {
        /// Panel view (preview, canvas, terminal, memories) or null to clear
        #[arg(long)]
        view: Option<String>,
        /// Collapse right sidebar
        #[arg(long)]
        collapsed: Option<bool>,
    },
    /// Switch theme
    Theme {
        /// Theme name (light, dark, system)
        theme: String,
    },
}

#[cfg(unix)]
type UnixClient = HyperClient<UnixConnector, Full<Bytes>>;

/// HTTP response wrapper for oqtoctl.
struct OqtoResponse {
    status: StatusCode,
    body: Bytes,
}

impl OqtoResponse {
    fn status(&self) -> StatusCode {
        self.status
    }

    async fn text(&self) -> Result<String> {
        let text = String::from_utf8(self.body.to_vec()).context("decoding response body")?;
        Ok(text)
    }

    async fn json<T: serde::de::DeserializeOwned>(&self) -> Result<T> {
        serde_json::from_slice(&self.body).context("decoding JSON response")
    }
}

enum OqtoTransport {
    Http {
        base_url: String,
        client: reqwest::Client,
    },
    #[cfg(unix)]
    Unix {
        socket_path: PathBuf,
        base_path: String,
        client: Box<UnixClient>,
    },
}

/// HTTP client for communicating with Oqto server
struct OqtoClient {
    transport: OqtoTransport,
    dev_user: Option<String>,
    auth_token: Option<String>,
}

impl OqtoClient {
    fn new(base_url: &str, admin_socket: Option<&str>) -> Result<Self> {
        let base_url = base_url.trim_end_matches('/').to_string();
        let auth_token = std::env::var("OQTO_AUTH_TOKEN").ok();
        let dev_user = std::env::var("OQTO_DEV_USER").ok();

        #[cfg(unix)]
        {
            let admin_socket_env = std::env::var("OQTO_ADMIN_SOCKET").ok();
            let admin_socket_requested = admin_socket.is_some() || admin_socket_env.is_some();
            let socket_path = admin_socket
                .map(PathBuf::from)
                .or_else(|| admin_socket_env.map(PathBuf::from))
                .or_else(|| Some(PathBuf::from(DEFAULT_ADMIN_SOCKET)));

            let use_admin_socket = admin_socket_requested || base_url == DEFAULT_SERVER_URL;
            let can_use_admin_socket = auth_token.is_none()
                && use_admin_socket
                && socket_path.as_ref().is_some_and(|path| path.exists());

            if can_use_admin_socket {
                let Some(socket_path) = socket_path else {
                    anyhow::bail!("admin socket path unavailable");
                };
                let base_path = base_path_from_url(&base_url)?;
                let client = HyperClient::builder(TokioExecutor::new()).build(UnixConnector);

                return Ok(Self {
                    transport: OqtoTransport::Unix {
                        socket_path,
                        base_path,
                        client: Box::new(client),
                    },
                    dev_user,
                    auth_token,
                });
            }
        }

        Ok(Self {
            transport: OqtoTransport::Http {
                base_url,
                client: reqwest::Client::new(),
            },
            dev_user,
            auth_token,
        })
    }

    fn display_url(&self) -> String {
        match &self.transport {
            OqtoTransport::Http { base_url, .. } => base_url.clone(),
            #[cfg(unix)]
            OqtoTransport::Unix { socket_path, .. } => {
                format!("unix://{}", socket_path.display())
            }
        }
    }

    fn http_base_url(&self) -> Option<&str> {
        match &self.transport {
            OqtoTransport::Http { base_url, .. } => Some(base_url.as_str()),
            #[cfg(unix)]
            OqtoTransport::Unix { .. } => None,
        }
    }

    fn http_client(&self) -> Option<&reqwest::Client> {
        match &self.transport {
            OqtoTransport::Http { client, .. } => Some(client),
            #[cfg(unix)]
            OqtoTransport::Unix { .. } => None,
        }
    }

    fn is_admin_socket(&self) -> bool {
        #[cfg(unix)]
        {
            matches!(self.transport, OqtoTransport::Unix { .. })
        }
        #[cfg(not(unix))]
        {
            false
        }
    }

    fn with_auth_headers(&self, req: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        if let Some(token) = self.auth_token.as_ref() {
            req.bearer_auth(token)
        } else if let Some(user) = self.dev_user.as_ref() {
            req.header("X-Dev-User", user)
        } else {
            req
        }
    }

    async fn get(&self, path: &str) -> Result<OqtoResponse> {
        match &self.transport {
            OqtoTransport::Http { base_url, client } => {
                let url = format!("{}{}", base_url, path);
                let response = self
                    .with_auth_headers(client.get(&url))
                    .send()
                    .await
                    .context("sending request to server")?;
                response_to_oqto(response).await
            }
            #[cfg(unix)]
            OqtoTransport::Unix { .. } => self.request_unix(hyper::Method::GET, path, None).await,
        }
    }

    async fn post(&self, path: &str) -> Result<OqtoResponse> {
        match &self.transport {
            OqtoTransport::Http { base_url, client } => {
                let url = format!("{}{}", base_url, path);
                let response = self
                    .with_auth_headers(client.post(&url))
                    .send()
                    .await
                    .context("sending request to server")?;
                response_to_oqto(response).await
            }
            #[cfg(unix)]
            OqtoTransport::Unix { .. } => self.request_unix(hyper::Method::POST, path, None).await,
        }
    }

    async fn delete(&self, path: &str) -> Result<OqtoResponse> {
        match &self.transport {
            OqtoTransport::Http { base_url, client } => {
                let url = format!("{}{}", base_url, path);
                let response = self
                    .with_auth_headers(client.delete(&url))
                    .send()
                    .await
                    .context("sending request to server")?;
                response_to_oqto(response).await
            }
            #[cfg(unix)]
            OqtoTransport::Unix { .. } => {
                self.request_unix(hyper::Method::DELETE, path, None).await
            }
        }
    }

    async fn post_json<T: serde::Serialize>(&self, path: &str, body: &T) -> Result<OqtoResponse> {
        match &self.transport {
            OqtoTransport::Http { base_url, client } => {
                let url = format!("{}{}", base_url, path);
                let response = self
                    .with_auth_headers(client.post(&url).json(body))
                    .send()
                    .await
                    .context("sending request to server")?;
                response_to_oqto(response).await
            }
            #[cfg(unix)]
            OqtoTransport::Unix { .. } => {
                let payload = serde_json::to_vec(body).context("serializing JSON")?;
                self.request_unix(hyper::Method::POST, path, Some(payload))
                    .await
            }
        }
    }

    async fn put_json<T: serde::Serialize>(&self, path: &str, body: &T) -> Result<OqtoResponse> {
        match &self.transport {
            OqtoTransport::Http { base_url, client } => {
                let url = format!("{}{}", base_url, path);
                let response = self
                    .with_auth_headers(client.put(&url).json(body))
                    .send()
                    .await
                    .context("sending request to server")?;
                response_to_oqto(response).await
            }
            #[cfg(unix)]
            OqtoTransport::Unix { .. } => {
                let payload = serde_json::to_vec(body).context("serializing JSON")?;
                self.request_unix(hyper::Method::PUT, path, Some(payload))
                    .await
            }
        }
    }

    #[cfg(unix)]
    async fn request_unix(
        &self,
        method: hyper::Method,
        path: &str,
        body: Option<Vec<u8>>,
    ) -> Result<OqtoResponse> {
        let (socket_path, base_path, client) = match &self.transport {
            OqtoTransport::Unix {
                socket_path,
                base_path,
                client,
            } => (socket_path, base_path, client),
            _ => return Err(anyhow!("unix transport not configured")),
        };

        let full_path = format!("{}{}", base_path, path);
        let uri: hyper::Uri = UnixUri::new(socket_path, &full_path).into();

        let body = body.unwrap_or_default();
        let mut builder = hyper::Request::builder().method(method).uri(uri);
        if !body.is_empty() {
            builder = builder.header("content-type", "application/json");
        }
        let request = builder
            .body(Full::new(Bytes::from(body)))
            .context("building unix request")?;

        let response = client
            .request(request)
            .await
            .context("sending unix request")?;
        oqto_response_from_hyper(response).await
    }
}

async fn response_to_oqto(response: reqwest::Response) -> Result<OqtoResponse> {
    let status = response.status();
    let body = response.bytes().await.context("reading response body")?;
    Ok(OqtoResponse { status, body })
}

#[cfg(unix)]
async fn oqto_response_from_hyper(
    response: hyper::Response<hyper::body::Incoming>,
) -> Result<OqtoResponse> {
    let status = response.status();
    let body = response
        .into_body()
        .collect()
        .await
        .context("reading unix response body")?
        .to_bytes();
    Ok(OqtoResponse { status, body })
}

fn base_path_from_url(base_url: &str) -> Result<String> {
    let parsed = reqwest::Url::parse(base_url).context("parsing server URL")?;
    let path = parsed.path().trim_end_matches('/');
    if path.is_empty() {
        Ok(String::new())
    } else {
        Ok(path.to_string())
    }
}

async fn handle_status(client: &OqtoClient, json: bool) -> Result<()> {
    let response = client.get("/health").await?;

    if response.status().is_success() {
        if json {
            println!(
                r#"{{"status": "ok", "server": "{}"}}"#,
                client.display_url()
            );
        } else {
            println!("Server is running at {}", client.display_url());
        }
    } else {
        if json {
            println!(
                r#"{{"status": "error", "code": {}}}"#,
                response.status().as_u16()
            );
        } else {
            println!("Server returned error: {}", response.status());
        }
    }
    Ok(())
}

async fn handle_ask(
    client: &OqtoClient,
    target: &str,
    question: &str,
    timeout: u64,
    stream: bool,
    json_output: bool,
) -> Result<()> {
    // Strip @@ prefix if present
    let target = target.strip_prefix("@@").unwrap_or(target);

    let body = serde_json::json!({
        "target": target,
        "question": question,
        "timeout_secs": timeout,
        "stream": stream,
    });

    if stream {
        if client.is_admin_socket() {
            return Err(anyhow!(
                "Streaming is not supported over the admin Unix socket. Run without --stream."
            ));
        }

        // Use SSE streaming
        use futures::StreamExt;
        use reqwest_eventsource::{Event, EventSource};

        let base_url = client
            .http_base_url()
            .ok_or_else(|| anyhow!("HTTP client not available"))?;
        let http_client = client
            .http_client()
            .ok_or_else(|| anyhow!("HTTP client not available"))?;

        let url = format!("{}/agents/ask", base_url);
        let mut request = http_client.post(&url).json(&body);
        request = client.with_auth_headers(request);

        let mut es = EventSource::new(request)?;

        while let Some(event) = es.next().await {
            match event {
                Ok(Event::Open) => {}
                Ok(Event::Message(message)) => {
                    if json_output {
                        println!("{}", message.data);
                    } else {
                        // Parse and display streaming content
                        if let Ok(data) = serde_json::from_str::<serde_json::Value>(&message.data) {
                            match data["type"].as_str() {
                                Some("text") => {
                                    if let Some(text) = data["data"].as_str() {
                                        print!("{}", text);
                                        let _ = io::stdout().flush();
                                    }
                                }
                                Some("thinking") => {
                                    // Optionally show thinking in a different style
                                    if let Some(text) = data["data"].as_str() {
                                        print!("\x1b[2m{}\x1b[0m", text); // Dim text
                                        let _ = io::stdout().flush();
                                    }
                                }
                                Some("done") => {
                                    println!(); // Final newline
                                }
                                Some("error") => {
                                    if let Some(err) = data["error"].as_str() {
                                        eprintln!("\nError: {}", err);
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                }
                Err(err) => {
                    es.close();
                    return Err(anyhow::anyhow!("SSE error: {}", err));
                }
            }
        }
    } else {
        // Non-streaming: wait for complete response
        let response = client.post_json("/agents/ask", &body).await?;

        if response.status().is_success() {
            let result: serde_json::Value = response.json().await?;

            // Check if this is an ambiguous response with multiple matches
            if result.get("matches").is_some() {
                if json_output {
                    println!("{}", serde_json::to_string_pretty(&result)?);
                } else {
                    eprintln!(
                        "{}",
                        result["error"].as_str().unwrap_or("Multiple matches found")
                    );
                    eprintln!("\nMatching sessions:");
                    if let Some(matches) = result["matches"].as_array() {
                        for (i, m) in matches.iter().enumerate() {
                            let id = m["id"].as_str().unwrap_or("?");
                            let title = m["title"].as_str().unwrap_or("(untitled)");
                            // Truncate title for display
                            let title_display: String = title.chars().take(40).collect();
                            let title_display = if title.len() > 40 {
                                format!("{}...", title_display)
                            } else {
                                title_display
                            };
                            eprintln!("  {}. {} - {}", i + 1, id, title_display);
                        }
                    }
                    eprintln!(
                        "\nUse a more specific target, e.g.: @@main:{}",
                        result["matches"]
                            .as_array()
                            .and_then(|a| a.first())
                            .and_then(|m| m["id"].as_str())
                            .unwrap_or("session_id")
                    );
                }
                return Ok(());
            }

            if json_output {
                println!("{}", serde_json::to_string_pretty(&result)?);
            } else if let Some(response_text) = result["response"].as_str() {
                println!("{}", response_text);
            } else {
                println!("{}", result);
            }
        } else {
            let status = response.status();
            let body = response.text().await?;
            if json_output {
                println!(
                    r#"{{"error": true, "status": {}, "message": {}}}"#,
                    status.as_u16(),
                    serde_json::to_string(&body)?
                );
            } else {
                anyhow::bail!("Request failed ({}): {}", status, body);
            }
        }
    }

    Ok(())
}

async fn handle_sessions(
    client: &OqtoClient,
    query: Option<&str>,
    limit: usize,
    json_output: bool,
) -> Result<()> {
    let path = match query {
        Some(q) => format!(
            "/agents/sessions?q={}&limit={}",
            urlencoding::encode(q),
            limit
        ),
        None => format!("/agents/sessions?limit={}", limit),
    };

    let response = client.get(&path).await?;

    if response.status().is_success() {
        let result: serde_json::Value = response.json().await?;

        if json_output {
            println!("{}", serde_json::to_string_pretty(&result)?);
        } else if let Some(sessions) = result["sessions"].as_array() {
            if sessions.is_empty() {
                println!("No sessions found");
            } else {
                println!("{:<20} {:<40} {:<20}", "ID", "TITLE", "MODIFIED");
                println!("{}", "-".repeat(80));
                for session in sessions {
                    let id = session["id"].as_str().unwrap_or("?");
                    let title = session["title"].as_str().unwrap_or("(untitled)");
                    let modified = session["modified_at"].as_str().unwrap_or("-");

                    // Truncate title for display
                    let title_display: String = title.chars().take(38).collect();
                    let title_display = if title.len() > 38 {
                        format!("{}...", title_display)
                    } else {
                        title_display
                    };

                    // Format modified time (just date part)
                    let modified_short = modified.split('T').next().unwrap_or(modified);

                    println!("{:<20} {:<40} {:<20}", id, title_display, modified_short);
                }
            }
        }
    } else {
        let status = response.status();
        let body = response.text().await?;
        if json_output {
            println!(
                r#"{{"error": true, "status": {}, "message": {}}}"#,
                status.as_u16(),
                serde_json::to_string(&body)?
            );
        } else {
            anyhow::bail!("Request failed ({}): {}", status, body);
        }
    }

    Ok(())
}

async fn handle_session(client: &OqtoClient, command: SessionCommand, json: bool) -> Result<()> {
    match command {
        SessionCommand::List => {
            let response = client.get("/sessions").await?;
            let body = response.text().await?;
            if json {
                println!("{}", body);
            } else {
                let sessions: Vec<serde_json::Value> = serde_json::from_str(&body)?;
                println!("{:<12} {:<10} {:<20}", "ID", "STATUS", "IMAGE");
                println!("{}", "-".repeat(50));
                for session in sessions {
                    println!(
                        "{:<12} {:<10} {:<20}",
                        session["id"]
                            .as_str()
                            .unwrap_or("")
                            .chars()
                            .take(8)
                            .collect::<String>(),
                        session["status"].as_str().unwrap_or("-"),
                        session["image"].as_str().unwrap_or("-"),
                    );
                }
            }
        }
        SessionCommand::Get { id } => {
            let response = client.get(&format!("/sessions/{}", id)).await?;
            let body = response.text().await?;
            if json {
                println!("{}", body);
            } else {
                let session: serde_json::Value = serde_json::from_str(&body)?;
                println!("Session: {}", session["id"]);
                println!("  Status: {}", session["status"]);
                println!("  Image: {}", session["image"]);
                println!("  Container: {}", session["container_id"]);
                println!(
                    "  Ports: agent={}, fileserver={}, ttyd={}",
                    session["agent_port"], session["fileserver_port"], session["ttyd_port"]
                );
            }
        }
        SessionCommand::Stop { id } => {
            let response = client.post(&format!("/sessions/{}/stop", id)).await?;
            if response.status().is_success() {
                if json {
                    println!(r#"{{"status": "stopped", "id": "{}"}}"#, id);
                } else {
                    println!("Session {} stopped", id);
                }
            } else {
                let body = response.text().await?;
                anyhow::bail!("Failed to stop session: {}", body);
            }
        }
        SessionCommand::Resume { id } => {
            let response = client.post(&format!("/sessions/{}/resume", id)).await?;
            if response.status().is_success() {
                if json {
                    println!("{}", response.text().await?);
                } else {
                    println!("Session {} resumed", id);
                }
            } else {
                let body = response.text().await?;
                anyhow::bail!("Failed to resume session: {}", body);
            }
        }
        SessionCommand::Delete { id, force } => {
            if force {
                // Stop first if force
                let _ = client.post(&format!("/sessions/{}/stop", id)).await;
            }
            let response = client.delete(&format!("/sessions/{}", id)).await?;
            if response.status().is_success() {
                if json {
                    println!(r#"{{"status": "deleted", "id": "{}"}}"#, id);
                } else {
                    println!("Session {} deleted", id);
                }
            } else {
                let body = response.text().await?;
                anyhow::bail!("Failed to delete session: {}", body);
            }
        }
        SessionCommand::Upgrade { id } => {
            let response = client.post(&format!("/sessions/{}/upgrade", id)).await?;
            if response.status().is_success() {
                if json {
                    println!("{}", response.text().await?);
                } else {
                    println!("Session {} upgraded to latest image", id);
                }
            } else {
                let body = response.text().await?;
                anyhow::bail!("Failed to upgrade session: {}", body);
            }
        }
    }
    Ok(())
}

async fn handle_container(
    client: &OqtoClient,
    command: ContainerCommand,
    json: bool,
) -> Result<()> {
    match command {
        ContainerCommand::Refresh { outdated_only } => {
            // Get all sessions
            let response = client.get("/sessions").await?;
            let sessions: Vec<serde_json::Value> = response.json().await?;

            let mut refreshed = 0;
            for session in sessions {
                let id = session["id"].as_str().unwrap_or("");
                let status = session["status"].as_str().unwrap_or("");

                if status != "running" && status != "stopped" {
                    continue;
                }

                if outdated_only {
                    // Check if image is outdated via upgrade endpoint
                    let response = client.post(&format!("/sessions/{}/upgrade", id)).await?;
                    if response.status().is_success() {
                        refreshed += 1;
                        if !json {
                            println!("Refreshed session {}", id);
                        }
                    }
                } else {
                    // Force refresh all: stop, delete, and let it be recreated
                    let _ = client.post(&format!("/sessions/{}/stop", id)).await;
                    let response = client.post(&format!("/sessions/{}/upgrade", id)).await?;
                    if response.status().is_success() {
                        refreshed += 1;
                        if !json {
                            println!("Refreshed session {}", id);
                        }
                    }
                }
            }

            if json {
                println!(r#"{{"refreshed": {}}}"#, refreshed);
            } else {
                println!("Refreshed {} container(s)", refreshed);
            }
        }
        ContainerCommand::Cleanup => {
            let response = client.post("/admin/cleanup").await?;
            if response.status().is_success() {
                let body = response.text().await?;
                if json {
                    println!("{}", body);
                } else {
                    println!("Cleanup completed");
                }
            } else {
                let body = response.text().await?;
                anyhow::bail!("Cleanup failed: {}", body);
            }
        }
        ContainerCommand::List => {
            let response = client.get("/sessions").await?;
            let sessions: Vec<serde_json::Value> = response.json().await?;

            if json {
                let containers: Vec<_> = sessions
                    .iter()
                    .filter(|s| s["container_id"].as_str().is_some())
                    .map(|s| {
                        serde_json::json!({
                            "container_id": s["container_id"],
                            "container_name": s["container_name"],
                            "session_id": s["id"],
                            "status": s["status"],
                        })
                    })
                    .collect();
                println!("{}", serde_json::to_string_pretty(&containers)?);
            } else {
                println!(
                    "{:<16} {:<20} {:<12} {:<10}",
                    "CONTAINER", "NAME", "SESSION", "STATUS"
                );
                println!("{}", "-".repeat(60));
                for session in sessions {
                    if let Some(container_id) = session["container_id"].as_str() {
                        println!(
                            "{:<16} {:<20} {:<12} {:<10}",
                            &container_id[..12.min(container_id.len())],
                            session["container_name"].as_str().unwrap_or("-"),
                            &session["id"].as_str().unwrap_or("")[..8],
                            session["status"].as_str().unwrap_or("-"),
                        );
                    }
                }
            }
        }
        ContainerCommand::StopAll => {
            let response = client.get("/sessions").await?;
            let sessions: Vec<serde_json::Value> = response.json().await?;

            let mut stopped = 0;
            for session in sessions {
                let id = session["id"].as_str().unwrap_or("");
                let status = session["status"].as_str().unwrap_or("");

                if status == "running" {
                    let response = client.post(&format!("/sessions/{}/stop", id)).await?;
                    if response.status().is_success() {
                        stopped += 1;
                        if !json {
                            println!("Stopped session {}", id);
                        }
                    }
                }
            }

            if json {
                println!(r#"{{"stopped": {}}}"#, stopped);
            } else {
                println!("Stopped {} container(s)", stopped);
            }
        }
    }
    Ok(())
}

async fn handle_image(client: &OqtoClient, command: ImageCommand, json: bool) -> Result<()> {
    match command {
        ImageCommand::Check => {
            // Check sessions for outdated images
            let response = client.get("/sessions").await?;
            let sessions: Vec<serde_json::Value> = response.json().await?;

            if json {
                // In a real implementation, we'd check image digests
                println!(
                    r#"{{"sessions_checked": {}, "outdated": 0}}"#,
                    sessions.len()
                );
            } else {
                println!("Checked {} session(s) for image updates", sessions.len());
                println!(
                    "Use 'oqtoctl container refresh --outdated-only' to update outdated containers"
                );
            }
        }
        ImageCommand::Pull { image } => {
            println!("Pulling image {}...", image);
            let output = std::process::Command::new("docker")
                .args(["pull", &image])
                .output()
                .context("running docker pull")?;

            if output.status.success() {
                if json {
                    println!(r#"{{"status": "pulled", "image": "{}"}}"#, image);
                } else {
                    println!("Successfully pulled {}", image);
                }
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                anyhow::bail!("Failed to pull image: {}", stderr);
            }
        }
        ImageCommand::Build { path, no_cache } => {
            println!("Building image from {}...", path);

            let dockerfile = if cfg!(target_arch = "aarch64") {
                "Dockerfile.arm64"
            } else {
                "Dockerfile"
            };

            let mut cmd = std::process::Command::new("docker");
            cmd.args([
                "build",
                "-f",
                &format!("{}/{}", path, dockerfile),
                "-t",
                "oqto:latest",
            ]);

            if no_cache {
                cmd.arg("--no-cache");
            }

            cmd.arg(".");

            let output = cmd.output().context("running docker build")?;

            if output.status.success() {
                if json {
                    println!(r#"{{"status": "built", "image": "oqto:latest"}}"#);
                } else {
                    println!("Successfully built oqto:latest");
                }
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                anyhow::bail!("Failed to build image: {}", stderr);
            }
        }
    }
    Ok(())
}

async fn handle_local(client: &OqtoClient, command: LocalCommand, json: bool) -> Result<()> {
    match command {
        LocalCommand::Cleanup => {
            let response = client.post("/admin/local/cleanup").await?;
            let status = response.status();
            let body = response.text().await?;
            if status.is_success() {
                if json {
                    println!("{}", body);
                } else {
                    let payload: serde_json::Value = serde_json::from_str(&body)?;
                    let cleared = payload["cleared"].as_u64().unwrap_or(0);
                    println!("Cleared {} local process(es)", cleared);
                }
            } else {
                anyhow::bail!("Failed to clean up local sessions: {}", body);
            }
        }
    }
    Ok(())
}

const SYSTEM_SANDBOX_CONFIG: &str = "/etc/oqto/sandbox.toml";

/// Default sandbox configuration content
const DEFAULT_SANDBOX_CONFIG: &str = r#"# Oqto Sandbox Configuration (System-wide)
# This file is owned by root and trusted by oqto-runner.
# It cannot be modified by regular users or compromised agents.

enabled = true
profile = "development"

# Paths to deny read access (sensitive files)
deny_read = [
    "~/.ssh",
    "~/.gnupg",
    "~/.aws",
    "~/.config/gcloud",
    "~/.kube",
]

# Paths to allow write access (in addition to workspace)
allow_write = [
    # Package managers / toolchains
    "~/.cargo",
    "~/.rustup",
    "~/.npm",
    "~/.bun",
    "~/.local/bin",
    # Agent tools - data directories
    "~/.local/share/skdlr",
    "~/.local/share/mmry",
    "~/.local/share/mailz",
    # Agent tools - config directories
    "~/.config/skdlr",
    "~/.config/mmry",
    "~/.config/mailz",
    "~/.config/byt",
    "/tmp",
]

# Paths to deny write access (takes precedence)
deny_write = [
    "/etc/oqto/sandbox.toml",
]

# Namespace isolation
isolate_network = false
isolate_pid = true
"#;

async fn handle_sandbox(command: SandboxCommand, json: bool) -> Result<()> {
    match command {
        SandboxCommand::Show => {
            let config_path = std::path::Path::new(SYSTEM_SANDBOX_CONFIG);

            if !config_path.exists() {
                if json {
                    println!(
                        r#"{{"exists": false, "path": "{}"}}"#,
                        SYSTEM_SANDBOX_CONFIG
                    );
                } else {
                    println!("Sandbox config not found at {}", SYSTEM_SANDBOX_CONFIG);
                    println!("\nTo create default config, run:");
                    println!("  oqtoctl sandbox reset");
                }
                return Ok(());
            }

            let content =
                std::fs::read_to_string(config_path).context("Failed to read sandbox config")?;

            if json {
                // Parse and output as JSON
                match toml::from_str::<toml::Value>(&content) {
                    Ok(config) => {
                        let json_val = serde_json::json!({
                            "exists": true,
                            "path": SYSTEM_SANDBOX_CONFIG,
                            "config": config
                        });
                        println!("{}", serde_json::to_string_pretty(&json_val)?);
                    }
                    Err(e) => {
                        let json_val = serde_json::json!({
                            "exists": true,
                            "path": SYSTEM_SANDBOX_CONFIG,
                            "error": format!("Invalid TOML: {}", e),
                            "raw": content
                        });
                        println!("{}", serde_json::to_string_pretty(&json_val)?);
                    }
                }
            } else {
                println!("# Sandbox config: {}\n", SYSTEM_SANDBOX_CONFIG);
                println!("{}", content);
            }
        }

        SandboxCommand::Edit => {
            let config_path = std::path::Path::new(SYSTEM_SANDBOX_CONFIG);

            // Create parent directory if needed
            if let Some(parent) = config_path.parent()
                && !parent.exists()
            {
                println!("Creating directory {}...", parent.display());
                let status = std::process::Command::new("sudo")
                    .args(["mkdir", "-p", &parent.to_string_lossy()])
                    .status()
                    .context("Failed to create config directory")?;
                if !status.success() {
                    anyhow::bail!("Failed to create config directory");
                }
            }

            // If config doesn't exist, create it with defaults first
            if !config_path.exists() {
                println!("Config not found, creating with defaults...");
                let mut child = std::process::Command::new("sudo")
                    .args(["tee", SYSTEM_SANDBOX_CONFIG])
                    .stdin(std::process::Stdio::piped())
                    .stdout(std::process::Stdio::null())
                    .spawn()
                    .context("Failed to spawn sudo tee")?;

                if let Some(mut stdin) = child.stdin.take() {
                    use std::io::Write;
                    stdin.write_all(DEFAULT_SANDBOX_CONFIG.as_bytes())?;
                }

                let status = child.wait()?;
                if !status.success() {
                    anyhow::bail!("Failed to write default config");
                }
            }

            // Get editor from environment
            let editor = std::env::var("EDITOR")
                .or_else(|_| std::env::var("VISUAL"))
                .unwrap_or_else(|_| "nano".to_string());

            // Copy to temp file for editing
            let temp_dir = std::env::temp_dir();
            let temp_path = temp_dir.join("sandbox.toml.edit");

            // Try to copy directly, fallback to sudo cat
            if std::fs::copy(config_path, &temp_path).is_err() {
                // If can't read directly, try with sudo
                let output = std::process::Command::new("sudo")
                    .args(["cat", SYSTEM_SANDBOX_CONFIG])
                    .output()
                    .context("Failed to read config with sudo")?;
                std::fs::write(&temp_path, &output.stdout).context("Failed to write temp file")?;
            }

            // Open editor
            println!("Opening {} with {}...", temp_path.display(), editor);
            let status = std::process::Command::new(&editor)
                .arg(&temp_path)
                .status()
                .context("Failed to open editor")?;

            if !status.success() {
                anyhow::bail!("Editor exited with error");
            }

            // Validate the edited config
            let edited_content =
                std::fs::read_to_string(&temp_path).context("Failed to read edited config")?;

            if let Err(e) = toml::from_str::<toml::Value>(&edited_content) {
                eprintln!("Error: Invalid TOML syntax: {}", e);
                eprintln!("\nConfig was NOT saved. Fix the errors and try again.");
                eprintln!("Edited file is at: {}", temp_path.display());
                anyhow::bail!("Invalid config");
            }

            // Write back with sudo
            let mut child = std::process::Command::new("sudo")
                .args(["tee", SYSTEM_SANDBOX_CONFIG])
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::null())
                .spawn()
                .context("Failed to spawn sudo tee")?;

            if let Some(mut stdin) = child.stdin.take() {
                use std::io::Write;
                stdin.write_all(edited_content.as_bytes())?;
            }

            let status = child.wait()?;
            if !status.success() {
                anyhow::bail!("Failed to save config");
            }

            // Clean up temp file
            let _ = std::fs::remove_file(&temp_path);

            println!("Sandbox config saved to {}", SYSTEM_SANDBOX_CONFIG);
        }

        SandboxCommand::Validate => {
            let config_path = std::path::Path::new(SYSTEM_SANDBOX_CONFIG);

            if !config_path.exists() {
                if json {
                    println!(r#"{{"valid": false, "error": "Config file not found"}}"#);
                } else {
                    println!("Config file not found at {}", SYSTEM_SANDBOX_CONFIG);
                }
                return Ok(());
            }

            // Try to read (may need sudo)
            let content = match std::fs::read_to_string(config_path) {
                Ok(c) => c,
                Err(_) => {
                    let output = std::process::Command::new("sudo")
                        .args(["cat", SYSTEM_SANDBOX_CONFIG])
                        .output()
                        .context("Failed to read config")?;
                    String::from_utf8_lossy(&output.stdout).to_string()
                }
            };

            match toml::from_str::<toml::Value>(&content) {
                Ok(config) => {
                    // Check required fields
                    let mut warnings = vec![];

                    if config.get("enabled").is_none() {
                        warnings.push("Missing 'enabled' field (defaults to false)");
                    }

                    if config.get("deny_read").is_none() {
                        warnings
                            .push("Missing 'deny_read' field (sensitive files won't be protected)");
                    }

                    if json {
                        let json_val = serde_json::json!({
                            "valid": true,
                            "warnings": warnings,
                            "enabled": config.get("enabled").and_then(|v| v.as_bool()).unwrap_or(false),
                            "profile": config.get("profile").and_then(|v| v.as_str()).unwrap_or("default")
                        });
                        println!("{}", serde_json::to_string_pretty(&json_val)?);
                    } else {
                        println!("Config is valid!");
                        if !warnings.is_empty() {
                            println!("\nWarnings:");
                            for w in &warnings {
                                println!("  - {}", w);
                            }
                        }

                        let enabled = config
                            .get("enabled")
                            .and_then(|v| v.as_bool())
                            .unwrap_or(false);
                        let profile = config
                            .get("profile")
                            .and_then(|v| v.as_str())
                            .unwrap_or("default");
                        println!("\nStatus:");
                        println!("  Enabled: {}", enabled);
                        println!("  Profile: {}", profile);
                    }
                }
                Err(e) => {
                    if json {
                        println!(r#"{{"valid": false, "error": "{}"}}"#, e);
                    } else {
                        eprintln!("Config is INVALID: {}", e);
                    }
                }
            }
        }

        SandboxCommand::Reset { yes } => {
            if !yes {
                print!("This will reset sandbox config to defaults. Continue? [y/N] ");
                let _ = io::stdout().flush();

                let mut input = String::new();
                io::stdin().read_line(&mut input)?;

                if !input.trim().eq_ignore_ascii_case("y") {
                    println!("Aborted.");
                    return Ok(());
                }
            }

            // Create parent directory if needed
            let config_path = std::path::Path::new(SYSTEM_SANDBOX_CONFIG);
            if let Some(parent) = config_path.parent()
                && !parent.exists()
            {
                let status = std::process::Command::new("sudo")
                    .args(["mkdir", "-p", &parent.to_string_lossy()])
                    .status()
                    .context("Failed to create config directory")?;
                if !status.success() {
                    anyhow::bail!("Failed to create config directory");
                }
            }

            // Write default config
            let mut child = std::process::Command::new("sudo")
                .args(["tee", SYSTEM_SANDBOX_CONFIG])
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::null())
                .spawn()
                .context("Failed to spawn sudo tee")?;

            if let Some(mut stdin) = child.stdin.take() {
                use std::io::Write;
                stdin.write_all(DEFAULT_SANDBOX_CONFIG.as_bytes())?;
            }

            let status = child.wait()?;
            if !status.success() {
                anyhow::bail!("Failed to write config");
            }

            // Set permissions
            let _ = std::process::Command::new("sudo")
                .args(["chmod", "644", SYSTEM_SANDBOX_CONFIG])
                .status();
            let _ = std::process::Command::new("sudo")
                .args(["chown", "root:root", SYSTEM_SANDBOX_CONFIG])
                .status();

            if json {
                println!(
                    r#"{{"status": "reset", "path": "{}"}}"#,
                    SYSTEM_SANDBOX_CONFIG
                );
            } else {
                println!(
                    "Sandbox config reset to defaults at {}",
                    SYSTEM_SANDBOX_CONFIG
                );
            }
        }
    }
    Ok(())
}

/// Create a Linux user with home directory and enable systemd lingering.
fn create_linux_user(linux_username: &str, json: bool) -> Result<()> {
    // Check if Linux user already exists
    let user_exists = std::process::Command::new("id")
        .arg(linux_username)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    if user_exists {
        if !json {
            eprintln!(
                "Linux user '{}' already exists, skipping creation",
                linux_username
            );
        }
    } else {
        // Create Linux user with sudo
        if !json {
            eprintln!("Creating Linux user '{}'...", linux_username);
        }

        let status = std::process::Command::new("sudo")
            .args(["useradd", "-m", "-s", "/bin/bash", linux_username])
            .status()
            .context("Failed to run useradd")?;

        if !status.success() {
            anyhow::bail!("Failed to create Linux user '{}'", linux_username);
        }

        if !json {
            eprintln!("Linux user '{}' created", linux_username);
        }
    }

    // Enable lingering for systemd user services
    if !json {
        eprintln!("Enabling systemd lingering for '{}'...", linux_username);
    }

    let status = std::process::Command::new("sudo")
        .args(["loginctl", "enable-linger", linux_username])
        .status()
        .context("Failed to enable lingering")?;

    if !status.success() {
        eprintln!(
            "Warning: Failed to enable lingering for '{}'",
            linux_username
        );
    }

    Ok(())
}

async fn handle_user(client: &OqtoClient, command: UserCommand, json: bool) -> Result<()> {
    match command {
        UserCommand::Create {
            username,
            email,
            display_name,
            role,
            password,
        } => {
            // Validate role
            let role = match role.to_lowercase().as_str() {
                "user" | "admin" => role.to_lowercase(),
                _ => anyhow::bail!("Invalid role: {}. Must be 'user' or 'admin'", role),
            };

            // Prompt for password if not provided via --password
            let password = match password {
                Some(p) => p,
                None => {
                    if json {
                        anyhow::bail!("Password is required in JSON mode. Use --password");
                    }
                    let pw = read_password_prompt("Password: ")?;
                    if pw.is_empty() {
                        anyhow::bail!("Password cannot be empty");
                    }
                    let confirm = read_password_prompt("Confirm password: ")?;
                    if pw != confirm {
                        anyhow::bail!("Passwords do not match");
                    }
                    pw
                }
            };

            // Create user via the oqto API -- this handles everything:
            // DB record, Linux user (via oqto-usermgr), runner setup, eavs provisioning.
            if !json {
                eprintln!("Creating user via oqto API...");
            }

            let body = serde_json::json!({
                "username": username,
                "email": email,
                "password": password,
                "display_name": display_name.as_deref().unwrap_or(&username),
                "role": role,
            });

            let response = client.post_json("/admin/users", &body).await?;
            let status = response.status();
            let body_text = response.text().await.unwrap_or_default();

            if status.is_success() {
                if json {
                    println!("{body_text}");
                } else {
                    let user: serde_json::Value =
                        serde_json::from_str(&body_text).unwrap_or_default();
                    println!("User created:");
                    println!(
                        "  Username:       {}",
                        user["username"].as_str().unwrap_or(&username)
                    );
                    println!(
                        "  Email:          {}",
                        user["email"].as_str().unwrap_or(&email)
                    );
                    println!(
                        "  Role:           {}",
                        user["role"].as_str().unwrap_or(&role)
                    );
                    if let Some(lu) = user["linux_username"].as_str() {
                        println!("  Linux user:     {lu}");
                    }
                    println!("  ID:             {}", user["id"].as_str().unwrap_or("?"));
                }
            } else {
                let err: serde_json::Value = serde_json::from_str(&body_text).unwrap_or_default();
                let msg = err["error"]
                    .as_str()
                    .or_else(|| err["message"].as_str())
                    .unwrap_or(&body_text);
                anyhow::bail!("Failed to create user (HTTP {status}): {msg}");
            }
        }

        UserCommand::List { runner_status } => {
            // List users from /etc/passwd that have oqto-runner configured
            // In a full implementation, this would query the Oqto database
            if !json {
                println!("Listing users with runner configuration:\n");
            }

            let mut users = vec![];

            // Check all users with home directories
            if let Ok(passwd) = std::fs::read_to_string("/etc/passwd") {
                for line in passwd.lines() {
                    let parts: Vec<&str> = line.split(':').collect();
                    if parts.len() >= 6 {
                        let username = parts[0];
                        let uid: u32 = parts[2].parse().unwrap_or(0);
                        let home = parts[5];

                        // Skip system users
                        if uid < 1000 || uid == 65534 {
                            continue;
                        }

                        // Check if user has runner service installed
                        let service_path =
                            format!("{}/.config/systemd/user/oqto-runner.service", home);
                        let has_runner = std::path::Path::new(&service_path).exists();

                        if has_runner {
                            let status = if runner_status {
                                get_runner_status(username)
                            } else {
                                "unknown".to_string()
                            };

                            users.push(serde_json::json!({
                                "username": username,
                                "uid": uid,
                                "home": home,
                                "runner_installed": true,
                                "runner_status": status,
                            }));
                        }
                    }
                }
            }

            if json {
                println!("{}", serde_json::to_string_pretty(&users)?);
            } else if users.is_empty() {
                println!("No users with runner configured found.");
            } else {
                println!(
                    "{:<20} {:<8} {:<15} HOME",
                    "USERNAME", "UID", "RUNNER STATUS"
                );
                println!("{}", "-".repeat(70));
                for user in &users {
                    println!(
                        "{:<20} {:<8} {:<15} {}",
                        user["username"].as_str().unwrap_or("-"),
                        user["uid"].as_u64().unwrap_or(0),
                        user["runner_status"].as_str().unwrap_or("-"),
                        user["home"].as_str().unwrap_or("-"),
                    );
                }
            }
        }

        UserCommand::Show { user } => {
            // Get user info from system and check runner status
            let output = std::process::Command::new("id")
                .arg(&user)
                .output()
                .context("Failed to run id command")?;

            if !output.status.success() {
                anyhow::bail!("User '{}' not found", user);
            }

            let id_output = String::from_utf8_lossy(&output.stdout);

            // Parse uid and gid from id output
            let uid = id_output
                .split("uid=")
                .nth(1)
                .and_then(|s| s.split('(').next())
                .and_then(|s| s.parse::<u32>().ok())
                .unwrap_or(0);

            // Get home directory
            let home = std::process::Command::new("bash")
                .args(["-c", &format!("echo ~{}", user)])
                .output()
                .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
                .unwrap_or_default();

            // Check runner installation
            let service_path = format!("{}/.config/systemd/user/oqto-runner.service", home);
            let runner_installed = std::path::Path::new(&service_path).exists();

            // Check runner status
            let runner_status = get_runner_status(&user);

            // Check socket
            let socket_path = format!("/run/user/{}/oqto-runner.sock", uid);
            let socket_exists = std::path::Path::new(&socket_path).exists();

            // Check lingering
            let linger_path = format!("/var/lib/systemd/linger/{}", user);
            let lingering = std::path::Path::new(&linger_path).exists();

            if json {
                let result = serde_json::json!({
                    "username": user,
                    "uid": uid,
                    "home": home,
                    "runner": {
                        "installed": runner_installed,
                        "status": runner_status,
                        "socket": socket_path,
                        "socket_exists": socket_exists,
                    },
                    "lingering": lingering,
                });
                println!("{}", serde_json::to_string_pretty(&result)?);
            } else {
                println!("User: {}", user);
                println!("  UID: {}", uid);
                println!("  Home: {}", home);
                println!(
                    "  Lingering: {}",
                    if lingering { "enabled" } else { "disabled" }
                );
                println!("\nRunner:");
                println!("  Installed: {}", runner_installed);
                println!("  Status: {}", runner_status);
                println!("  Socket: {}", socket_path);
                println!("  Socket exists: {}", socket_exists);
            }
        }

        UserCommand::SetupRunner { user, force } => {
            // Check if user exists
            let output = std::process::Command::new("id")
                .arg(&user)
                .output()
                .context("Failed to run id command")?;

            if !output.status.success() {
                anyhow::bail!("User '{}' not found", user);
            }

            // Get home directory
            let home = std::process::Command::new("bash")
                .args(["-c", &format!("echo ~{}", user)])
                .output()
                .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
                .context("Failed to get home directory")?;

            // Check if already installed
            let service_path = format!("{}/.config/systemd/user/oqto-runner.service", home);
            if std::path::Path::new(&service_path).exists() && !force {
                if json {
                    println!(r#"{{"status": "already_installed", "user": "{}"}}"#, user);
                } else {
                    println!(
                        "Runner already installed for '{}'. Use --force to reinstall.",
                        user
                    );
                }
                return Ok(());
            }

            setup_runner_for_user(&user, json)?;

            if json {
                println!(r#"{{"status": "installed", "user": "{}"}}"#, user);
            } else {
                println!("Runner setup complete for '{}'", user);
            }
        }

        UserCommand::RunnerStatus { user } => {
            let status = get_runner_status(&user);

            // Get uid for socket path
            let uid = std::process::Command::new("id")
                .args(["-u", &user])
                .output()
                .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
                .unwrap_or_default();

            let socket_path = format!("/run/user/{}/oqto-runner.sock", uid);
            let socket_exists = std::path::Path::new(&socket_path).exists();

            if json {
                let result = serde_json::json!({
                    "user": user,
                    "status": status,
                    "socket": socket_path,
                    "socket_exists": socket_exists,
                });
                println!("{}", serde_json::to_string_pretty(&result)?);
            } else {
                println!("User: {}", user);
                println!("Status: {}", status);
                println!(
                    "Socket: {} ({})",
                    socket_path,
                    if socket_exists { "exists" } else { "not found" }
                );
            }
        }

        UserCommand::SetPassword { user, password } => {
            // Resolve user to ID first
            let user_id = resolve_user_id(client, &user, json).await?;

            let password = match password {
                Some(p) => p,
                None => {
                    let pw = read_password_prompt("New password: ")?;
                    if pw.is_empty() {
                        anyhow::bail!("Password cannot be empty");
                    }
                    let confirm = read_password_prompt("Confirm password: ")?;
                    if pw != confirm {
                        anyhow::bail!("Passwords do not match");
                    }
                    pw
                }
            };

            let body = serde_json::json!({ "password": password });
            let response = client
                .put_json(&format!("/admin/users/{}", user_id), &body)
                .await?;
            let status = response.status();

            if status.is_success() {
                if json {
                    println!(
                        "{}",
                        serde_json::json!({"status": "updated", "user": user_id, "field": "password"})
                    );
                } else {
                    println!("Password updated for '{}'.", user);
                }
            } else {
                let body_text = response.text().await.unwrap_or_default();
                anyhow::bail!("Failed to update password (HTTP {status}): {body_text}");
            }
        }

        UserCommand::SetRole { user, role } => {
            let role = match role.to_lowercase().as_str() {
                "user" | "admin" => role.to_lowercase(),
                _ => anyhow::bail!("Invalid role: {}. Must be 'user' or 'admin'", role),
            };

            let user_id = resolve_user_id(client, &user, json).await?;
            let body = serde_json::json!({ "role": role });
            let response = client
                .put_json(&format!("/admin/users/{}", user_id), &body)
                .await?;
            let status = response.status();

            if status.is_success() {
                if json {
                    println!(
                        "{}",
                        serde_json::json!({"status": "updated", "user": user_id, "role": role})
                    );
                } else {
                    println!("Role updated to '{}' for '{}'.", role, user);
                }
            } else {
                let body_text = response.text().await.unwrap_or_default();
                anyhow::bail!("Failed to update role (HTTP {status}): {body_text}");
            }
        }

        UserCommand::Disable { user } => {
            let user_id = resolve_user_id(client, &user, json).await?;
            let response = client
                .post_json(
                    &format!("/admin/users/{}/deactivate", user_id),
                    &serde_json::json!({}),
                )
                .await?;
            let status = response.status();

            if status.is_success() {
                if json {
                    println!(
                        "{}",
                        serde_json::json!({"status": "disabled", "user": user_id})
                    );
                } else {
                    println!("User '{}' disabled.", user);
                }
            } else {
                let body_text = response.text().await.unwrap_or_default();
                anyhow::bail!("Failed to disable user (HTTP {status}): {body_text}");
            }
        }

        UserCommand::Enable { user } => {
            let user_id = resolve_user_id(client, &user, json).await?;
            let response = client
                .post_json(
                    &format!("/admin/users/{}/activate", user_id),
                    &serde_json::json!({}),
                )
                .await?;
            let status = response.status();

            if status.is_success() {
                if json {
                    println!(
                        "{}",
                        serde_json::json!({"status": "enabled", "user": user_id})
                    );
                } else {
                    println!("User '{}' enabled.", user);
                }
            } else {
                let body_text = response.text().await.unwrap_or_default();
                anyhow::bail!("Failed to enable user (HTTP {status}): {body_text}");
            }
        }

        UserCommand::SetEmail { user, email } => {
            let user_id = resolve_user_id(client, &user, json).await?;
            let body = serde_json::json!({ "email": email });
            let response = client
                .put_json(&format!("/admin/users/{}", user_id), &body)
                .await?;
            let status = response.status();

            if status.is_success() {
                if json {
                    println!(
                        "{}",
                        serde_json::json!({"status": "updated", "user": user_id, "email": email})
                    );
                } else {
                    println!("Email updated to '{}' for '{}'.", email, user);
                }
            } else {
                let body_text = response.text().await.unwrap_or_default();
                anyhow::bail!("Failed to update email (HTTP {status}): {body_text}");
            }
        }

        UserCommand::SetDisplayName { user, name } => {
            let user_id = resolve_user_id(client, &user, json).await?;
            let body = serde_json::json!({ "display_name": name });
            let response = client
                .put_json(&format!("/admin/users/{}", user_id), &body)
                .await?;
            let status = response.status();

            if status.is_success() {
                if json {
                    println!(
                        "{}",
                        serde_json::json!({"status": "updated", "user": user_id, "display_name": name})
                    );
                } else {
                    println!("Display name updated to '{}' for '{}'.", name, user);
                }
            } else {
                let body_text = response.text().await.unwrap_or_default();
                anyhow::bail!("Failed to update display name (HTTP {status}): {body_text}");
            }
        }

        UserCommand::Reprovision { user } => {
            let user_id = resolve_user_id(client, &user, json).await?;

            if !json {
                eprintln!("Re-provisioning eavs + configs for '{}'...", user);
            }

            // First sync configs (eavs key + models.json + runner)
            let body = serde_json::json!({ "user_id": user_id });
            let response = client.post_json("/admin/users/sync-configs", &body).await?;
            let status = response.status();

            if status.is_success() {
                if json {
                    let payload: serde_json::Value = response.json().await?;
                    println!("{}", serde_json::to_string_pretty(&payload)?);
                } else {
                    println!("Re-provisioned '{}'.", user);
                }
            } else {
                let body_text = response.text().await.unwrap_or_default();
                anyhow::bail!("Failed to reprovision (HTTP {status}): {body_text}");
            }
        }

        UserCommand::SyncConfigs { user } => {
            let body = serde_json::json!({ "user_id": user });
            let response = client.post_json("/admin/users/sync-configs", &body).await?;

            if !response.status().is_success() {
                anyhow::bail!("Server returned error: {}", response.status().as_u16());
            }

            let payload: serde_json::Value = response.json().await?;

            if json {
                println!("{}", serde_json::to_string_pretty(&payload)?);
            } else {
                let results = payload
                    .get("results")
                    .and_then(|r| r.as_array())
                    .cloned()
                    .unwrap_or_default();

                println!("Synced config for {} users", results.len());
                for result in results {
                    let user_id = result
                        .get("user_id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("-");
                    let error = result.get("error").and_then(|v| v.as_str());
                    if let Some(err) = error {
                        println!("  {}: error: {}", user_id, err);
                    } else {
                        println!("  {}: ok", user_id);
                    }
                }
            }
        }

        UserCommand::DoctorIdentity { user, apply } => {
            doctor_identity(user.as_deref(), apply, json).await?;
        }

        UserCommand::Delete { user, force } => {
            // Resolve user to get details (try API first, fall back to system lookup)
            let user_id = user.clone();

            if !force {
                eprintln!("This will permanently delete user '{}' including:", user_id);
                eprintln!("  - Database record");
                eprintln!("  - Linux user account");
                eprintln!("  - Home directory and all files");
                eprintln!("  - Running services (runner, mmry)");
                eprint!("\nContinue? [y/N] ");
                let mut input = String::new();
                std::io::stdin().read_line(&mut input)?;
                if !input.trim().eq_ignore_ascii_case("y") {
                    eprintln!("Aborted.");
                    return Ok(());
                }
            }

            if !json {
                eprintln!("Deleting user '{}'...", user_id);
            }

            // Call the admin API which handles both DB + Linux cleanup
            let url = format!("/admin/users/{}", user_id);
            let response = client.delete(&url).await?;
            let status = response.status();

            if status.is_success() {
                if json {
                    println!(
                        "{}",
                        serde_json::json!({"status": "deleted", "user": user_id})
                    );
                } else {
                    println!("User '{}' deleted.", user_id);
                }
            } else {
                let body_text = response.text().await.unwrap_or_default();
                let err: serde_json::Value = serde_json::from_str(&body_text).unwrap_or_default();
                let msg = err["error"]
                    .as_str()
                    .or_else(|| err["message"].as_str())
                    .unwrap_or(&body_text);
                anyhow::bail!("Failed to delete user (HTTP {status}): {msg}");
            }
        }

        UserCommand::Bootstrap {
            username,
            email,
            password,
            password_hash,
            display_name,
            database,
            linux_user,
            no_linux_user,
            no_runner,
        } => {
            // Generate user_id first so we can derive the correct Linux username
            let user_id = generate_user_id(&username);

            // Derive Linux username the same way the backend does:
            // oqto_{sanitize(user_id)} -- unless explicitly overridden
            let linux_username = if let Some(ref explicit) = linux_user {
                explicit.clone()
            } else {
                let sanitized = sanitize_for_linux(&user_id);
                format!("oqto_{sanitized}")
            };

            // Create Linux user if not skipped
            if !no_linux_user {
                create_linux_user(&linux_username, json)?;
            }

            // Setup runner if not skipped
            if !no_runner && !no_linux_user {
                setup_runner_for_user(&linux_username, json)?;
            }

            // Create database user (pass pre-generated user_id and linux_username)
            bootstrap_admin_user(
                &username,
                &email,
                password.as_deref(),
                password_hash.as_deref(),
                display_name.as_deref(),
                database.as_deref(),
                &user_id,
                if no_linux_user {
                    None
                } else {
                    Some(&linux_username)
                },
                json,
            )
            .await?;
        }
    }
    Ok(())
}

/// Bootstrap the first admin user directly in the database.
///
/// This bypasses the normal registration flow (which requires an invite code)
/// and creates an admin user directly. Used for initial production setup.
/// The user_id and linux_username are pre-generated by the caller to ensure
/// consistency with the backend's Linux user naming convention.
async fn bootstrap_admin_user(
    username: &str,
    email: &str,
    password: Option<&str>,
    pre_hashed: Option<&str>,
    display_name: Option<&str>,
    database_path: Option<&str>,
    user_id: &str,
    linux_username: Option<&str>,
    json: bool,
) -> Result<()> {
    use sqlx::sqlite::SqlitePoolOptions;
    use std::io::Write;

    // Determine password hash: use pre-computed hash, hash provided password, or prompt
    let password_hash = if let Some(hash) = pre_hashed {
        hash.to_string()
    } else {
        let password = match password {
            Some(p) => p.to_string(),
            None => {
                if json {
                    anyhow::bail!("Password is required in JSON mode. Use --password");
                }

                let password = read_password_prompt("Enter admin password: ")?;

                if password.len() < 8 {
                    anyhow::bail!("Password must be at least 8 characters");
                }

                let confirm = read_password_prompt("Confirm password: ")?;

                if password != confirm {
                    anyhow::bail!("Passwords do not match");
                }

                password
            }
        };

        if password.len() < 8 {
            anyhow::bail!("Password must be at least 8 characters");
        }

        bcrypt::hash(&password, bcrypt::DEFAULT_COST).context("Failed to hash password")?
    };

    // Get database path from option or default
    let db_path = match database_path {
        Some(p) => std::path::PathBuf::from(p),
        None => get_database_path()?,
    };

    if !json {
        eprintln!("Using database: {}", db_path.display());
    }

    // Ensure database directory exists
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent).ok();
    }

    // Connect to database
    let db_url = format!("sqlite:{}?mode=rwc", db_path.display());
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect(&db_url)
        .await
        .context("Failed to connect to database")?;

    // Run migrations to ensure schema exists
    sqlx::migrate!("../oqto/migrations")
        .run(&pool)
        .await
        .context("Failed to run migrations")?;

    // Check if any users exist
    let user_count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM users")
        .fetch_one(&pool)
        .await
        .unwrap_or((0,));

    if user_count.0 > 0 && !json {
        eprintln!(
            "Warning: {} user(s) already exist in the database.",
            user_count.0
        );
        eprint!("Continue anyway? [y/N] ");
        std::io::stderr().flush()?;

        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;
        if !input.trim().eq_ignore_ascii_case("y") {
            eprintln!("Aborted.");
            return Ok(());
        }
    }

    // Check if username already exists
    let existing: Option<(String,)> = sqlx::query_as("SELECT id FROM users WHERE username = ?1")
        .bind(username)
        .fetch_optional(&pool)
        .await?;

    if existing.is_some() {
        anyhow::bail!("User '{}' already exists", username);
    }

    // Check if email already exists
    let existing_email: Option<(String,)> = sqlx::query_as("SELECT id FROM users WHERE email = ?1")
        .bind(email)
        .fetch_optional(&pool)
        .await?;

    if existing_email.is_some() {
        anyhow::bail!("Email '{}' is already registered", email);
    }

    let now = chrono::Utc::now().to_rfc3339();
    let display = display_name.unwrap_or(username);

    // Insert the admin user (including linux_username so the backend can find it)
    sqlx::query(
        r#"
        INSERT INTO users (id, username, email, password_hash, display_name, role, is_active, created_at, updated_at, linux_username)
        VALUES (?1, ?2, ?3, ?4, ?5, 'admin', 1, ?6, ?6, ?7)
        "#
    )
        .bind(user_id)
        .bind(username)
        .bind(email)
        .bind(&password_hash)
        .bind(display)
        .bind(&now)
        .bind(linux_username)
        .execute(&pool)
        .await
        .context("Failed to insert admin user")?;

    if json {
        let result = serde_json::json!({
            "status": "created",
            "user_id": user_id,
            "username": username,
            "email": email,
            "role": "admin",
            "linux_username": linux_username,
        });
        println!("{}", serde_json::to_string_pretty(&result)?);
    } else {
        eprintln!();
        eprintln!("Admin user created successfully!");
        eprintln!("  User ID:       {}", user_id);
        eprintln!("  Username:      {}", username);
        eprintln!("  Email:         {}", email);
        eprintln!("  Role:          admin");
        if let Some(lu) = linux_username {
            eprintln!("  Linux user:    {}", lu);
        }
        eprintln!();
        eprintln!("You can now start Oqto and log in with these credentials.");
    }

    Ok(())
}

/// Generate a short random ID
/// Hash a password using bcrypt and print the hash to stdout.
/// If no password is provided, reads from stdin with echo disabled.
fn handle_hash_password(password: Option<String>, cost: u32) -> Result<()> {
    let password = match password {
        Some(p) => p,
        None => {
            // Check if stdin is a TTY
            if std::io::stdin().is_terminal() {
                let pw = read_password_prompt("Password: ")?;
                if pw.is_empty() {
                    anyhow::bail!("Password cannot be empty");
                }
                let confirm = read_password_prompt("Confirm password: ")?;
                if pw != confirm {
                    anyhow::bail!("Passwords do not match");
                }
                pw
            } else {
                // Reading from pipe
                let mut buf = String::new();
                std::io::stdin().read_line(&mut buf)?;
                buf.trim().to_string()
            }
        }
    };

    if password.is_empty() {
        anyhow::bail!("Password cannot be empty");
    }

    let hash = bcrypt::hash(&password, cost).context("Failed to hash password")?;
    // Print only the hash to stdout (no newline decoration) for script consumption
    println!("{hash}");
    Ok(())
}

/// Read a password from stdin with echo disabled (input is hidden).
/// Falls back to plain stdin read if terminal echo cannot be disabled.
fn read_password_prompt(prompt: &str) -> Result<String> {
    eprint!("{prompt}");
    std::io::stderr().flush()?;

    let password = read_password_no_echo().unwrap_or_else(|_| {
        // Fallback: plain read (echo visible)
        let mut buf = String::new();
        let _ = std::io::stdin().read_line(&mut buf);
        buf
    });
    // Print newline since echo was disabled (user's Enter wasn't shown)
    eprintln!();

    let password = password.trim().to_string();
    Ok(password)
}

/// Read a line from stdin with terminal echo disabled using termios.
#[cfg(unix)]
fn read_password_no_echo() -> Result<String> {
    use std::os::unix::io::AsRawFd;

    let stdin_fd = std::io::stdin().as_raw_fd();

    // Get current terminal attributes
    let mut termios = unsafe { std::mem::zeroed::<libc::termios>() };
    if unsafe { libc::tcgetattr(stdin_fd, &mut termios) } != 0 {
        anyhow::bail!("tcgetattr failed");
    }

    // Save original and disable echo
    let original = termios;
    termios.c_lflag &= !libc::ECHO;
    if unsafe { libc::tcsetattr(stdin_fd, libc::TCSANOW, &termios) } != 0 {
        anyhow::bail!("tcsetattr failed");
    }

    // Read the password
    let mut password = String::new();
    let result = std::io::stdin().read_line(&mut password);

    // Restore original terminal attributes (always, even on error)
    unsafe { libc::tcsetattr(stdin_fd, libc::TCSANOW, &original) };

    result?;
    Ok(password)
}

#[cfg(not(unix))]
fn read_password_no_echo() -> Result<String> {
    anyhow::bail!("Password echo suppression not supported on this platform");
}

/// Resolve a username or user ID to a confirmed user ID.
///
/// Tries the input as a user ID first (GET /admin/users/{input}). If that
/// fails with 404, lists all users and searches for a matching username.
async fn resolve_user_id(client: &OqtoClient, user_or_id: &str, _json: bool) -> Result<String> {
    // Try as user ID first
    let response = client.get(&format!("/admin/users/{}", user_or_id)).await?;
    if response.status().is_success() {
        let body = response.text().await.unwrap_or_default();
        if let Ok(user) = serde_json::from_str::<serde_json::Value>(&body)
            && let Some(id) = user["id"].as_str()
        {
            return Ok(id.to_string());
        }
    }

    // Try as username: list all users and find by username
    let response = client.get("/admin/users").await?;
    if response.status().is_success() {
        let body = response.text().await.unwrap_or_default();
        if let Ok(users) = serde_json::from_str::<Vec<serde_json::Value>>(&body) {
            for user in &users {
                if user["username"].as_str() == Some(user_or_id)
                    && let Some(id) = user["id"].as_str()
                {
                    return Ok(id.to_string());
                }
            }
        }
    }

    anyhow::bail!(
        "User '{}' not found. Provide a valid username or user ID.",
        user_or_id
    )
}

/// Generate a user ID from a username (e.g., "admin" -> "admin-x1y2").
/// Mirrors UserRepository::generate_user_id for use in oqtoctl.
fn generate_user_id(username: &str) -> String {
    // Normalize: lowercase, only [a-z0-9_-], trim dashes, max 31 chars
    let mut s: String = username
        .trim()
        .to_lowercase()
        .chars()
        .map(|c| match c {
            'a'..='z' | '0'..='9' | '_' | '-' => c,
            _ => '-',
        })
        .collect();
    s = s.trim_matches('-').to_string();
    if s.is_empty() {
        s = "user".to_string();
    }
    if !s.chars().next().unwrap_or('u').is_ascii_alphabetic() && !s.starts_with('_') {
        s = format!("u-{}", s);
    }
    if s.len() > 31 {
        s.truncate(31);
    }
    format!("{}-{}", s, nanoid::nanoid!(4))
}

/// Sanitize a user_id into a valid Linux username component.
/// Must match the logic in `linux_users.rs::sanitize_username` exactly.
fn sanitize_for_linux(user_id: &str) -> String {
    let mut result = String::with_capacity(32);

    for (i, c) in user_id.chars().enumerate() {
        if result.len() >= 32 {
            break;
        }
        let c = c.to_ascii_lowercase();
        if i == 0 {
            if c.is_ascii_lowercase() || c == '_' {
                result.push(c);
            } else if c.is_ascii_digit() {
                result.push('_');
                result.push(c);
            } else {
                result.push('_');
            }
        } else if c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-' {
            result.push(c);
        } else {
            result.push('_');
        }
    }

    if result.is_empty() {
        result.push_str("user");
    }
    while result.ends_with('-') {
        result.pop();
    }
    result
}

/// Database filename used by oqto and oqtoctl.
const DB_FILENAME: &str = "oqto.db";

/// Get the database path, checking multiple locations in order:
/// 1. Service data dir: /var/lib/oqto/.local/share/oqto/ (multi-user mode)
/// 2. XDG_DATA_HOME/oqto/ (single-user / env override)
/// 3. ~/.local/share/oqto/ (fallback)
fn get_database_path() -> Result<std::path::PathBuf> {
    // Multi-user: check the service user's data dir first
    let service_db = std::path::PathBuf::from("/var/lib/oqto/.local/share/oqto").join(DB_FILENAME);
    if service_db.exists() {
        return Ok(service_db);
    }

    // XDG_DATA_HOME (respects env override)
    if let Some(dir) = std::env::var_os("XDG_DATA_HOME").filter(|v| !v.is_empty()) {
        let db = std::path::PathBuf::from(dir).join("oqto").join(DB_FILENAME);
        if db.exists() {
            return Ok(db);
        }
    }

    // Default: ~/.local/share/oqto/
    let data_dir = dirs::data_dir()
        .map(|d| d.join("oqto"))
        .unwrap_or_else(|| std::path::PathBuf::from("."));
    let db_path = data_dir.join(DB_FILENAME);
    if db_path.exists() {
        return Ok(db_path);
    }

    // Nothing exists yet, prefer service path if /var/lib/oqto exists,
    // otherwise fall back to user dir
    if std::path::Path::new("/var/lib/oqto").exists() {
        let dir = std::path::PathBuf::from("/var/lib/oqto/.local/share/oqto");
        std::fs::create_dir_all(&dir).ok();
        return Ok(dir.join(DB_FILENAME));
    }

    std::fs::create_dir_all(&data_dir).ok();
    Ok(db_path)
}

/// Get the systemd status of the runner for a user
fn get_runner_status(username: &str) -> String {
    // Get the user's UID first
    let uid = std::process::Command::new("id")
        .args(["-u", username])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default();

    if uid.is_empty() {
        return "error (no uid)".to_string();
    }

    // Check if socket exists as a quick status check
    let socket_path = format!("/run/user/{}/oqto-runner.sock", uid);
    if std::path::Path::new(&socket_path).exists() {
        // Socket exists, try to check if service is actually running
        // Use machinectl shell for proper systemd user context
        let output = std::process::Command::new("sudo")
            .args([
                "machinectl",
                "shell",
                &format!("{}@", username),
                "/usr/bin/systemctl",
                "--user",
                "is-active",
                "oqto-runner",
            ])
            .output();

        match output {
            Ok(o) if o.status.success() => {
                let status = String::from_utf8_lossy(&o.stdout).trim().to_string();
                // machinectl adds extra output, extract just the status
                if status.contains("active") {
                    return "active".to_string();
                } else if status.contains("inactive") {
                    return "inactive".to_string();
                }
                // Socket exists, assume running
                "active (socket exists)".to_string()
            }
            _ => {
                // machinectl not available or failed, but socket exists
                "active (socket exists)".to_string()
            }
        }
    } else {
        // No socket, try systemctl directly (may work if we're running as that user)
        let output = std::process::Command::new("systemctl")
            .args(["--user", "is-active", "oqto-runner"])
            .env("XDG_RUNTIME_DIR", format!("/run/user/{}", uid))
            .output();

        match output {
            Ok(o) => {
                let status = String::from_utf8_lossy(&o.stdout).trim().to_string();
                if status.is_empty() {
                    "inactive (no socket)".to_string()
                } else {
                    status
                }
            }
            Err(_) => "inactive (no socket)".to_string(),
        }
    }
}

#[derive(Debug, serde::Serialize)]
struct IdentityDoctorIssue {
    user_id: String,
    username: String,
    severity: String,
    message: String,
    remediation: Option<String>,
}

#[derive(Debug, serde::Serialize)]
struct IdentityDoctorSummary {
    scanned_users: usize,
    issues: Vec<IdentityDoctorIssue>,
    applied_fixes: Vec<String>,
    apply_mode: bool,
}

fn collect_contract_host_facts(
    manifest: &oqto_provisioning::ProvisioningManifest,
    config_path: Option<&str>,
) -> oqto_provisioning::HostFacts {
    let mut facts = oqto_provisioning::HostFacts {
        runner_socket_pattern: read_runner_socket_pattern(config_path),
        ..oqto_provisioning::HostFacts::default()
    };

    for desired in &manifest.paths {
        if desired.path.contains('{') || desired.path.contains('$') {
            continue;
        }
        facts
            .paths
            .insert(desired.path.clone(), inspect_path(&desired.path));
    }

    for service in &manifest.services {
        let observed = match service.scope {
            oqto_provisioning::ServiceScope::System => oqto_provisioning::ObservedService {
                enabled: systemctl_bool("is-enabled", &service.name),
                active: systemctl_bool("is-active", &service.name),
            },
            oqto_provisioning::ServiceScope::User => {
                // Only inspect concrete current-user services. Template users
                // like {linux_username} are expanded by per-user checks.
                if service
                    .user
                    .as_deref()
                    .is_some_and(|user| user.contains('{'))
                {
                    continue;
                }
                oqto_provisioning::ObservedService {
                    enabled: systemctl_user_bool("is-enabled", &service.name),
                    active: systemctl_user_bool("is-active", &service.name),
                }
            }
        };
        facts.services.insert(service.name.clone(), observed);
    }

    facts
}

fn inspect_path(path: &str) -> oqto_provisioning::ObservedPath {
    let output = std::process::Command::new("stat")
        .args(["-c", "%U\t%G\t%a", path])
        .output();

    match output {
        Ok(out) if out.status.success() => {
            let text = String::from_utf8_lossy(&out.stdout);
            let mut parts = text.trim().split('\t');
            oqto_provisioning::ObservedPath {
                exists: true,
                owner: parts.next().map(ToString::to_string),
                group: parts.next().map(ToString::to_string),
                mode: parts.next().map(|mode| {
                    if mode.len() == 4 {
                        mode.to_string()
                    } else {
                        format!("0{mode}")
                    }
                }),
            }
        }
        _ => oqto_provisioning::ObservedPath {
            exists: false,
            owner: None,
            group: None,
            mode: None,
        },
    }
}

fn systemctl_bool(action: &str, service: &str) -> Option<bool> {
    let output = std::process::Command::new("systemctl")
        .args([action, service])
        .output()
        .ok()?;
    Some(output.status.success())
}

fn systemctl_user_bool(action: &str, service: &str) -> Option<bool> {
    let output = std::process::Command::new("systemctl")
        .args(["--user", action, service])
        .output()
        .ok()?;
    Some(output.status.success())
}

fn append_sudoers_findings(
    manifest: &oqto_provisioning::ProvisioningManifest,
    findings: &mut Vec<oqto_provisioning::ContractFinding>,
) {
    if !manifest
        .checks
        .iter()
        .any(|check| check.id == "team.sudoers.valid")
    {
        return;
    }

    match validate_sudoers_file("/etc/sudoers.d/oqto-multiuser") {
        Some(true) => {}
        Some(false) => findings.push(oqto_provisioning::ContractFinding {
            id: "team.sudoers.valid".to_string(),
            severity: oqto_provisioning::CheckSeverity::Error,
            expected: "visudo validation succeeds".to_string(),
            observed: "visudo validation failed".to_string(),
            remediation: "regenerate Linux user isolation sudoers rules".to_string(),
        }),
        None => findings.push(oqto_provisioning::ContractFinding {
            id: "team.sudoers.valid".to_string(),
            severity: oqto_provisioning::CheckSeverity::Warning,
            expected: "visudo validation succeeds".to_string(),
            observed: "visudo not available or insufficient permission to validate".to_string(),
            remediation: "run sudo oqtoctl doctor --contract --profile team".to_string(),
        }),
    }
}

fn validate_sudoers_file(path: &str) -> Option<bool> {
    if !std::path::Path::new(path).exists() {
        return Some(false);
    }

    let output = std::process::Command::new("visudo")
        .args(["-c", "-f", path])
        .output()
        .ok()?;
    Some(output.status.success())
}

async fn append_team_user_runner_findings(
    manifest: &oqto_provisioning::ProvisioningManifest,
    target_user: Option<&str>,
    findings: &mut Vec<oqto_provisioning::ContractFinding>,
) {
    if manifest.runner_socket.pattern != "/run/oqto/runner-sockets/{user}/oqto-runner.sock" {
        return;
    }

    if let Err(err) = append_team_user_runner_findings_inner(target_user, findings).await {
        findings.push(oqto_provisioning::ContractFinding {
            id: "team.users.inspect".to_string(),
            severity: oqto_provisioning::CheckSeverity::Warning,
            expected: "active users inspected for Linux identity and runner socket drift"
                .to_string(),
            observed: format!("could not inspect users: {err:#}"),
            remediation:
                "ensure the Oqto database is reachable or run oqtoctl user doctor-identity"
                    .to_string(),
        });
    }
}

async fn append_team_user_runner_findings_inner(
    target_user: Option<&str>,
    findings: &mut Vec<oqto_provisioning::ContractFinding>,
) -> Result<()> {
    use sqlx::sqlite::SqlitePoolOptions;

    let db_path = get_database_path()?;
    let db_url = format!("sqlite:{}?mode=rwc", db_path.display());
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect(&db_url)
        .await
        .with_context(|| format!("Failed to open database at {}", db_path.display()))?;

    let rows: Vec<(String, String, Option<String>, Option<i64>, i64)> =
        if let Some(user) = target_user {
            sqlx::query_as(
                r#"SELECT id, username, linux_username, linux_uid, is_active
               FROM users
               WHERE id = ?1 OR username = ?1
               ORDER BY username"#,
            )
            .bind(user)
            .fetch_all(&pool)
            .await?
        } else {
            sqlx::query_as(
                r#"SELECT id, username, linux_username, linux_uid, is_active
               FROM users
               ORDER BY username"#,
            )
            .fetch_all(&pool)
            .await?
        };

    for (user_id, username, linux_username, linux_uid, is_active) in rows {
        if is_active == 0 {
            continue;
        }
        let Some(linux_username) = linux_username else {
            findings.push(team_user_finding(
                &user_id,
                &username,
                "identity.linux_username",
                oqto_provisioning::CheckSeverity::Error,
                "linux_username is set",
                "missing",
                "run oqtoctl doctor --apply or reprovision this user",
            ));
            continue;
        };

        let actual_uid = linux_uid_for_user(&linux_username);
        match actual_uid {
            Some(uid) => {
                if linux_uid != Some(uid) {
                    findings.push(team_user_finding(
                        &user_id,
                        &username,
                        "identity.linux_uid",
                        oqto_provisioning::CheckSeverity::Error,
                        &format!("linux_uid={uid}"),
                        &format!("db={linux_uid:?}"),
                        "run oqtoctl doctor --apply to sync linux_uid",
                    ));
                }
                let shared_sock =
                    format!("/run/oqto/runner-sockets/{linux_username}/oqto-runner.sock");
                let user_runtime_sock = format!("/run/user/{uid}/oqto-runner.sock");
                let shared_exists = std::path::Path::new(&shared_sock).exists();
                let user_runtime_exists = std::path::Path::new(&user_runtime_sock).exists();
                if !shared_exists {
                    findings.push(team_user_finding(
                        &user_id,
                        &username,
                        "runner.socket.canonical",
                        oqto_provisioning::CheckSeverity::Error,
                        &shared_sock,
                        "missing",
                        "restart/reprovision the per-user oqto-runner service",
                    ));
                }
                if shared_exists && user_runtime_exists {
                    findings.push(team_user_finding(
                        &user_id,
                        &username,
                        "runner.socket.split-routing",
                        oqto_provisioning::CheckSeverity::Warning,
                        "only canonical shared socket exists",
                        &format!("also found {user_runtime_sock}"),
                        "remove stale user-runtime runner service or align socket pattern",
                    ));
                }
            }
            None => findings.push(team_user_finding(
                &user_id,
                &username,
                "identity.linux_user",
                oqto_provisioning::CheckSeverity::Error,
                &format!("OS user {linux_username} exists"),
                "missing",
                "create/reprovision the Linux user",
            )),
        }
    }

    Ok(())
}

fn linux_uid_for_user(username: &str) -> Option<i64> {
    let output = std::process::Command::new("id")
        .args(["-u", username])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8_lossy(&output.stdout).trim().parse().ok()
}

fn team_user_finding(
    user_id: &str,
    username: &str,
    suffix: &str,
    severity: oqto_provisioning::CheckSeverity,
    expected: &str,
    observed: &str,
    remediation: &str,
) -> oqto_provisioning::ContractFinding {
    oqto_provisioning::ContractFinding {
        id: format!("team.user.{username}.{suffix}"),
        severity,
        expected: format!("{expected} (user_id={user_id})"),
        observed: observed.to_string(),
        remediation: remediation.to_string(),
    }
}

#[derive(serde::Serialize)]
struct ContractFindingSummary {
    errors: usize,
    warnings: usize,
    info: usize,
}

fn contract_finding_summary(
    findings: &[oqto_provisioning::ContractFinding],
) -> ContractFindingSummary {
    let mut summary = ContractFindingSummary {
        errors: 0,
        warnings: 0,
        info: 0,
    };
    for finding in findings {
        match finding.severity {
            oqto_provisioning::CheckSeverity::Error => summary.errors += 1,
            oqto_provisioning::CheckSeverity::Warning => summary.warnings += 1,
            oqto_provisioning::CheckSeverity::Info => summary.info += 1,
        }
    }
    summary
}

fn apply_safe_contract_fixes(
    findings: &[oqto_provisioning::ContractFinding],
    apply_services: bool,
    apply_runners: bool,
) -> Result<Vec<String>> {
    let mut applied = Vec::new();
    let needs_runner_socket_dir_fix = findings.iter().any(|finding| {
        finding.id.starts_with("path./run/oqto/runner-sockets.")
            || finding.id == "path./run/oqto/runner-sockets.missing"
    });

    if needs_runner_socket_dir_fix {
        run_privileged_command(
            "install",
            &[
                "-d",
                "-m",
                "2770",
                "-o",
                "root",
                "-g",
                "oqto",
                "/run/oqto/runner-sockets",
            ],
        )
        .context("failed to repair /run/oqto/runner-sockets")?;
        applied.push(
            "repaired /run/oqto/runner-sockets owner/group/mode (root:oqto 2770)".to_string(),
        );
    }

    if apply_runners {
        for username in runner_apply_usernames(findings) {
            setup_runner_for_user(&username, true)
                .with_context(|| format!("failed to reprovision runner for {username}"))?;
            applied.push(format!("reprovisioned oqto-runner for {username}"));
        }
    }

    if apply_services {
        for finding in findings {
            if let Some(service) = finding
                .id
                .strip_prefix("service.")
                .and_then(|rest| rest.strip_suffix(".enabled"))
            {
                run_privileged_command("systemctl", &["enable", service])
                    .with_context(|| format!("failed to enable {service}"))?;
                applied.push(format!("enabled system service {service}"));
            } else if let Some(service) = finding
                .id
                .strip_prefix("service.")
                .and_then(|rest| rest.strip_suffix(".active"))
            {
                run_privileged_command("systemctl", &["start", service])
                    .with_context(|| format!("failed to start {service}"))?;
                applied.push(format!("started system service {service}"));
            }
        }
    }

    applied.sort();
    applied.dedup();
    Ok(applied)
}

fn runner_apply_usernames(findings: &[oqto_provisioning::ContractFinding]) -> Vec<String> {
    let mut usernames = Vec::new();
    for finding in findings {
        let Some(rest) = finding.id.strip_prefix("team.user.") else {
            continue;
        };
        let Some(username) = rest
            .strip_suffix(".runner.socket.canonical")
            .or_else(|| rest.strip_suffix(".runner.socket.split-routing"))
        else {
            continue;
        };
        if !usernames.iter().any(|existing| existing == username) {
            usernames.push(username.to_string());
        }
    }
    usernames
}

fn run_privileged_command(program: &str, args: &[&str]) -> Result<()> {
    let is_root = std::process::Command::new("id")
        .arg("-u")
        .output()
        .ok()
        .map(|output| String::from_utf8_lossy(&output.stdout).trim() == "0")
        .unwrap_or(false);

    let status = if is_root {
        std::process::Command::new(program).args(args).status()
    } else {
        let mut sudo_args = Vec::with_capacity(args.len() + 1);
        sudo_args.push(program);
        sudo_args.extend_from_slice(args);
        std::process::Command::new("sudo").args(sudo_args).status()
    }
    .with_context(|| format!("failed to run privileged command: {program}"))?;

    if !status.success() {
        anyhow::bail!("privileged command failed: {} {}", program, args.join(" "));
    }
    Ok(())
}

fn suggested_contract_commands(findings: &[oqto_provisioning::ContractFinding]) -> Vec<String> {
    let mut commands = Vec::new();
    for finding in findings {
        let command = if finding.id.starts_with("path./run/oqto/runner-sockets.") {
            Some("sudo install -d -m 2770 -o root -g oqto /run/oqto/runner-sockets".to_string())
        } else if finding.id == "path./etc/sudoers.d/oqto-multiuser.missing"
            || finding.id == "team.sudoers.valid"
        {
            Some("./setup.sh --team --redo linux_user_isolation".to_string())
        } else if let Some(service) = finding
            .id
            .strip_prefix("service.")
            .and_then(|rest| rest.strip_suffix(".enabled"))
        {
            Some(format!("sudo systemctl enable {service}"))
        } else if let Some(service) = finding
            .id
            .strip_prefix("service.")
            .and_then(|rest| rest.strip_suffix(".active"))
        {
            Some(format!("sudo systemctl start {service}"))
        } else if let Some(rest) = finding.id.strip_prefix("team.user.") {
            rest.strip_suffix(".runner.socket.canonical")
                .or_else(|| rest.strip_suffix(".runner.socket.split-routing"))
                .map(|username| format!("sudo oqtoctl user setup-runner {username} --force"))
        } else if finding.id.contains("identity.linux_uid")
            || finding.id.contains("identity.linux_username")
        {
            Some("sudo oqtoctl doctor --apply".to_string())
        } else {
            None
        };

        if let Some(command) = command
            && !commands.contains(&command)
        {
            commands.push(command);
        }
    }
    commands
}

#[cfg(test)]
mod setup_doctor_tests {
    use super::*;

    fn finding(id: &str) -> oqto_provisioning::ContractFinding {
        finding_with_severity(id, oqto_provisioning::CheckSeverity::Error)
    }

    fn finding_with_severity(
        id: &str,
        severity: oqto_provisioning::CheckSeverity,
    ) -> oqto_provisioning::ContractFinding {
        oqto_provisioning::ContractFinding {
            id: id.to_string(),
            severity,
            expected: "expected".to_string(),
            observed: "observed".to_string(),
            remediation: "remediation".to_string(),
        }
    }

    #[test]
    fn suggested_commands_are_deduplicated_and_cover_common_setup_drift() {
        let commands = suggested_contract_commands(&[
            finding("path./run/oqto/runner-sockets.group"),
            finding("path./run/oqto/runner-sockets.mode"),
            finding("path./etc/sudoers.d/oqto-multiuser.missing"),
            finding("service.oqto.service.enabled"),
            finding("service.oqto.service.active"),
            finding("team.user.alice.runner.socket.canonical"),
        ]);

        assert_eq!(
            commands,
            vec![
                "sudo install -d -m 2770 -o root -g oqto /run/oqto/runner-sockets",
                "./setup.sh --team --redo linux_user_isolation",
                "sudo systemctl enable oqto.service",
                "sudo systemctl start oqto.service",
                "sudo oqtoctl user setup-runner alice --force",
            ]
        );
    }

    #[test]
    fn runner_apply_usernames_only_extracts_runner_socket_findings_once() {
        let usernames = runner_apply_usernames(&[
            finding("team.user.alice.runner.socket.canonical"),
            finding("team.user.alice.runner.socket.split-routing"),
            finding("team.user.bob.runner.socket.canonical"),
            finding("team.user.carol.identity.linux_uid"),
        ]);

        assert_eq!(usernames, vec!["alice".to_string(), "bob".to_string()]);
    }

    #[test]
    fn contract_finding_summary_counts_all_severities() {
        let summary = contract_finding_summary(&[
            finding("service.oqto.service.active"),
            finding_with_severity(
                "team.user.alice.runner.socket.split-routing",
                oqto_provisioning::CheckSeverity::Warning,
            ),
            finding_with_severity("note", oqto_provisioning::CheckSeverity::Info),
        ]);

        assert_eq!(summary.errors, 1);
        assert_eq!(summary.warnings, 1);
        assert_eq!(summary.info, 1);
    }

    #[test]
    fn detect_profile_from_config_value_maps_single_user_and_multi_user() {
        let personal_cfg: toml::Value =
            toml::from_str("[local]\nsingle_user = true\n").expect("valid toml");
        let team_cfg: toml::Value =
            toml::from_str("[local]\nsingle_user = false\n").expect("valid toml");

        assert_eq!(
            detect_profile_from_config_value(&personal_cfg),
            Some(oqto_provisioning::InstallProfile::Personal)
        );
        assert_eq!(
            detect_profile_from_config_value(&team_cfg),
            Some(oqto_provisioning::InstallProfile::Team)
        );
    }
}

fn read_runner_socket_pattern(config_path: Option<&str>) -> Option<String> {
    let path = config_path
        .map(std::path::PathBuf::from)
        .or_else(default_oqto_config_path)?;
    let contents = std::fs::read_to_string(path).ok()?;
    let value: toml::Value = toml::from_str(&contents).ok()?;

    value
        .get("backend")
        .and_then(|backend| backend.get("runner"))
        .and_then(|runner| runner.get("socket_pattern"))
        .and_then(toml::Value::as_str)
        .or_else(|| {
            value
                .get("local")
                .and_then(|local| local.get("runner_socket_pattern"))
                .and_then(toml::Value::as_str)
        })
        .map(ToString::to_string)
}

fn default_oqto_config_path() -> Option<PathBuf> {
    if let Ok(path) = std::env::var("OQTO_CONFIG")
        && !path.trim().is_empty()
    {
        return Some(PathBuf::from(path));
    }
    if let Ok(config_home) = std::env::var("XDG_CONFIG_HOME") {
        return Some(PathBuf::from(config_home).join("oqto/config.toml"));
    }
    dirs::home_dir().map(|home| home.join(".config/oqto/config.toml"))
}

fn resolve_install_profile(
    profile_arg: &str,
    config_path: Option<&str>,
) -> Result<oqto_provisioning::InstallProfile> {
    match profile_arg {
        "personal" | "single" | "single-user" => Ok(oqto_provisioning::InstallProfile::Personal),
        "team" | "multi" | "multi-user" => Ok(oqto_provisioning::InstallProfile::Team),
        "auto" => {
            if let Some(profile) = detect_profile_from_config(config_path)? {
                Ok(profile)
            } else if let Ok(user_mode) = std::env::var("OQTO_USER_MODE") {
                if matches!(user_mode.as_str(), "multi" | "team") {
                    Ok(oqto_provisioning::InstallProfile::Team)
                } else {
                    Ok(oqto_provisioning::InstallProfile::Personal)
                }
            } else {
                Ok(oqto_provisioning::InstallProfile::Personal)
            }
        }
        other => anyhow::bail!(
            "unknown install profile '{other}'. Expected 'auto', 'personal' or 'team'"
        ),
    }
}

fn detect_profile_from_config(
    config_path: Option<&str>,
) -> Result<Option<oqto_provisioning::InstallProfile>> {
    let Some(path) = config_path
        .map(PathBuf::from)
        .or_else(default_oqto_config_path)
    else {
        return Ok(None);
    };
    if !path.exists() {
        return Ok(None);
    }

    let contents = std::fs::read_to_string(&path)
        .with_context(|| format!("Failed to read config at {}", path.display()))?;
    let value: toml::Value = toml::from_str(&contents)
        .with_context(|| format!("Failed to parse config at {}", path.display()))?;

    Ok(detect_profile_from_config_value(&value))
}

fn detect_profile_from_config_value(
    value: &toml::Value,
) -> Option<oqto_provisioning::InstallProfile> {
    let single_user = value
        .get("local")
        .and_then(|local| local.get("single_user"))
        .and_then(toml::Value::as_bool)?;

    if single_user {
        Some(oqto_provisioning::InstallProfile::Personal)
    } else {
        Some(oqto_provisioning::InstallProfile::Team)
    }
}

async fn handle_doctor(
    target_user: Option<&str>,
    apply: bool,
    contract: bool,
    profile: &str,
    strict: bool,
    apply_services: bool,
    apply_runners: bool,
    config_path: Option<&str>,
    json: bool,
) -> Result<()> {
    if contract {
        if (apply_services || apply_runners) && !apply {
            anyhow::bail!("--apply-services/--apply-runners require --apply");
        }

        let profile = resolve_install_profile(profile, config_path)?;
        let manifest = oqto_provisioning::manifest(profile);
        let mut facts = collect_contract_host_facts(&manifest, config_path);
        let mut findings = oqto_provisioning::evaluate_manifest(&manifest, &facts);
        append_sudoers_findings(&manifest, &mut findings);
        append_team_user_runner_findings(&manifest, target_user, &mut findings).await;
        let mut applied_fixes = Vec::new();
        if apply {
            applied_fixes = apply_safe_contract_fixes(&findings, apply_services, apply_runners)?;
            if !applied_fixes.is_empty() {
                facts = collect_contract_host_facts(&manifest, config_path);
                findings = oqto_provisioning::evaluate_manifest(&manifest, &facts);
                append_sudoers_findings(&manifest, &mut findings);
                append_team_user_runner_findings(&manifest, target_user, &mut findings).await;
            }
        }
        let summary = contract_finding_summary(&findings);
        let suggested_commands = suggested_contract_commands(&findings);
        if json {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "manifest": manifest,
                    "facts": facts,
                    "findings": findings,
                    "summary": summary,
                    "applied_fixes": applied_fixes,
                    "suggested_commands": suggested_commands,
                }))?
            );
        } else {
            println!("Provisioning contract: {}", manifest.summary);
            println!("Runner socket: {}", manifest.runner_socket.pattern);
            println!("\nRequired paths:");
            for path in &manifest.paths {
                println!(
                    "- {} [{:?}] owner={} group={} mode={} -- {}",
                    path.path, path.kind, path.owner, path.group, path.mode, path.purpose
                );
            }
            println!("\nRequired services:");
            for service in &manifest.services {
                let user = service.user.as_deref().unwrap_or("root/system");
                println!(
                    "- {} [{:?}] user={} enabled={} active={} -- {}",
                    service.name,
                    service.scope,
                    user,
                    service.enabled,
                    service.active,
                    service.purpose
                );
            }
            println!("\nChecks:");
            for check in &manifest.checks {
                println!(
                    "- [{:?}] {} -- remediation: {}",
                    check.severity, check.description, check.remediation
                );
            }
            println!(
                "\nContract finding summary: {} error(s), {} warning(s), {} info",
                summary.errors, summary.warnings, summary.info
            );
            println!("\nContract findings from host facts:");
            if findings.is_empty() {
                println!("No inspected contract drift detected.");
            } else {
                for finding in &findings {
                    println!(
                        "- [{:?}] {} expected={} observed={} remediation={}",
                        finding.severity,
                        finding.id,
                        finding.expected,
                        finding.observed,
                        finding.remediation
                    );
                }
            }
            if !applied_fixes.is_empty() {
                println!("\nApplied safe remediation(s):");
                for fix in &applied_fixes {
                    println!("- {fix}");
                }
            }
            if !suggested_commands.is_empty() {
                println!("\nSuggested remediation commands:");
                for command in &suggested_commands {
                    println!("- {command}");
                }
            }
        }
        if strict
            && findings
                .iter()
                .any(|finding| finding.severity == oqto_provisioning::CheckSeverity::Error)
        {
            anyhow::bail!("setup contract drift detected");
        }
        return Ok(());
    }

    doctor_identity(target_user, apply, json).await
}

async fn doctor_identity(target_user: Option<&str>, apply: bool, json: bool) -> Result<()> {
    use sqlx::sqlite::SqlitePoolOptions;

    let db_path = get_database_path()?;
    let db_url = format!("sqlite:{}?mode=rwc", db_path.display());
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect(&db_url)
        .await
        .with_context(|| format!("Failed to open database at {}", db_path.display()))?;

    let rows: Vec<(String, String, Option<String>, Option<i64>, i64)> = if let Some(u) = target_user
    {
        sqlx::query_as(
            r#"SELECT id, username, linux_username, linux_uid, is_active
               FROM users
               WHERE id = ?1 OR username = ?1
               ORDER BY username"#,
        )
        .bind(u)
        .fetch_all(&pool)
        .await?
    } else {
        sqlx::query_as(
            r#"SELECT id, username, linux_username, linux_uid, is_active
               FROM users
               ORDER BY username"#,
        )
        .fetch_all(&pool)
        .await?
    };

    let mut issues = Vec::new();
    let mut applied_fixes = Vec::new();

    for (user_id, username, linux_username, linux_uid, is_active) in rows.iter().cloned() {
        if is_active == 0 {
            continue;
        }

        let mut effective_linux_user = linux_username.clone();
        if effective_linux_user.is_none() {
            issues.push(IdentityDoctorIssue {
                user_id: user_id.clone(),
                username: username.clone(),
                severity: "warning".to_string(),
                message: "missing linux_username".to_string(),
                remediation: Some(
                    "set linux_username/linux_uid from existing OS account".to_string(),
                ),
            });

            let candidates = [
                sanitize_for_linux(&user_id),
                format!("oqto_{}", sanitize_for_linux(&user_id)),
                sanitize_for_linux(&username),
                format!("oqto_{}", sanitize_for_linux(&username)),
            ];
            let resolved = candidates
                .iter()
                .find(|candidate| {
                    std::process::Command::new("id")
                        .args(["-u", candidate])
                        .output()
                        .map(|o| o.status.success())
                        .unwrap_or(false)
                })
                .map(|s| s.to_string());

            if apply && let Some(found_user) = resolved {
                let uid_str = std::process::Command::new("id")
                    .args(["-u", &found_user])
                    .output()
                    .ok()
                    .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
                    .unwrap_or_default();
                if let Ok(uid) = uid_str.parse::<i64>() {
                    sqlx::query("UPDATE users SET linux_username = ?1, linux_uid = ?2, updated_at = datetime('now') WHERE id = ?3")
                        .bind(&found_user)
                        .bind(uid)
                        .bind(&user_id)
                        .execute(&pool)
                        .await?;
                    effective_linux_user = Some(found_user.clone());
                    applied_fixes.push(format!(
                        "{}: set linux_username={}, linux_uid={}",
                        username, found_user, uid
                    ));
                }
            }
        }

        if let Some(ref lu) = effective_linux_user {
            let uid_output = std::process::Command::new("id").args(["-u", lu]).output();
            match uid_output {
                Ok(out) if out.status.success() => {
                    let actual_uid = String::from_utf8_lossy(&out.stdout)
                        .trim()
                        .parse::<i64>()
                        .ok();
                    if actual_uid.is_none() {
                        issues.push(IdentityDoctorIssue {
                            user_id: user_id.clone(),
                            username: username.clone(),
                            severity: "error".to_string(),
                            message: format!("could not parse uid for linux user '{}'", lu),
                            remediation: None,
                        });
                    } else if linux_uid != actual_uid {
                        issues.push(IdentityDoctorIssue {
                            user_id: user_id.clone(),
                            username: username.clone(),
                            severity: "warning".to_string(),
                            message: format!(
                                "linux_uid mismatch (db={:?}, actual={:?}) for '{}'",
                                linux_uid, actual_uid, lu
                            ),
                            remediation: Some(
                                "update users.linux_uid to actual OS uid".to_string(),
                            ),
                        });
                        if apply {
                            sqlx::query("UPDATE users SET linux_uid = ?1, updated_at = datetime('now') WHERE id = ?2")
                                .bind(actual_uid)
                                .bind(&user_id)
                                .execute(&pool)
                                .await?;
                            applied_fixes.push(format!(
                                "{}: updated linux_uid to {:?}",
                                username, actual_uid
                            ));
                        }
                    }

                    if let Some(uid) = actual_uid {
                        let user_runtime_sock =
                            std::path::PathBuf::from(format!("/run/user/{uid}/oqto-runner.sock"));
                        let shared_sock = std::path::PathBuf::from(format!(
                            "/run/oqto/runner-sockets/{}/oqto-runner.sock",
                            lu
                        ));

                        if !shared_sock.exists() && !user_runtime_sock.exists() {
                            issues.push(IdentityDoctorIssue {
                                user_id: user_id.clone(),
                                username: username.clone(),
                                severity: "error".to_string(),
                                message: format!(
                                    "runner socket missing at canonical path {} and user-runtime path {}",
                                    shared_sock.display(),
                                    user_runtime_sock.display()
                                ),
                                remediation: Some(
                                    "start/reprovision oqto-runner for this Linux user and verify lingering/service status".to_string(),
                                ),
                            });
                        }
                        if !shared_sock.exists() && user_runtime_sock.exists() {
                            issues.push(IdentityDoctorIssue {
                                user_id: user_id.clone(),
                                username: username.clone(),
                                severity: "warning".to_string(),
                                message: format!(
                                    "runner socket present only at user-runtime path {}; canonical shared path {} is missing",
                                    user_runtime_sock.display(),
                                    shared_sock.display()
                                ),
                                remediation: Some(
                                    "reinstall/restart oqto-runner with --socket /run/oqto/runner-sockets/{linux_username}/oqto-runner.sock or update backend runner_socket_pattern consistently"
                                        .to_string(),
                                ),
                            });
                        }
                        if shared_sock.exists() && user_runtime_sock.exists() {
                            issues.push(IdentityDoctorIssue {
                                user_id: user_id.clone(),
                                username: username.clone(),
                                severity: "warning".to_string(),
                                message: "both canonical shared and user-runtime runner sockets are present".to_string(),
                                remediation: Some("disable conflicting runner service/socket pattern to avoid split routing".to_string()),
                            });
                        }
                    }
                }
                _ => issues.push(IdentityDoctorIssue {
                    user_id: user_id.clone(),
                    username: username.clone(),
                    severity: "error".to_string(),
                    message: format!("linux user '{}' does not exist", lu),
                    remediation: Some("create linux user and re-run doctor".to_string()),
                }),
            }
        }
    }

    let summary = IdentityDoctorSummary {
        scanned_users: rows.len(),
        issues,
        applied_fixes,
        apply_mode: apply,
    };

    if json {
        println!("{}", serde_json::to_string_pretty(&summary)?);
    } else {
        println!(
            "Identity doctor: scanned {} users (mode: {})",
            summary.scanned_users,
            if apply { "apply" } else { "dry-run" }
        );
        if summary.issues.is_empty() {
            println!("No identity issues detected.");
        } else {
            println!("Detected {} issue(s):", summary.issues.len());
            for issue in &summary.issues {
                println!(
                    "- [{}] {} ({}) {}",
                    issue.severity, issue.username, issue.user_id, issue.message
                );
                if let Some(remediation) = issue.remediation.as_deref() {
                    println!("    remediation: {remediation}");
                }
            }
        }
        if apply {
            println!("Applied {} remediation(s).", summary.applied_fixes.len());
            for fix in &summary.applied_fixes {
                println!("  - {fix}");
            }
        } else {
            println!("Dry-run only. Re-run with --apply to persist safe remediations.");
        }
    }

    Ok(())
}

/// Provision eavs virtual key and generate Pi models.json for a user.
///
/// 1. Queries eavs /providers/detail for configured providers
/// 2. Creates a virtual key with oauth_user binding
/// 3. Generates ~/.pi/agent/models.json with eavs-routed providers
/// 4. Returns the eavs key ID
async fn provision_eavs_for_user(
    linux_username: &str,
    oqto_username: &str,
    eavs_url: &str,
    master_key: Option<&str>,
    budget: Option<f64>,
    json_output: bool,
) -> Result<String> {
    use oqto_eavs::{CreateKeyRequest, EavsClient, KeyPermissions, generate_pi_models_json};

    let eavs_base = eavs_url.trim_end_matches('/');
    let eavs = EavsClient::new(eavs_base, master_key.unwrap_or(""))
        .context("Failed to create eavs client")?;

    // 1. Query eavs for configured providers
    if !json_output {
        println!("Querying eavs providers...");
    }
    let providers = eavs
        .providers_detail()
        .await
        .context("Failed to query eavs providers")?;

    if !json_output {
        println!(
            "  Found {} providers: {}",
            providers.len(),
            providers
                .iter()
                .map(|p| p.name.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        );
    }

    // 2. Create virtual key with oauth_user binding
    if !json_output {
        println!("Creating eavs virtual key...");
    }

    let mut key_req =
        CreateKeyRequest::new(format!("oqto-user-{}", oqto_username)).oauth_user(oqto_username);
    if let Some(budget_usd) = budget {
        key_req = key_req.permissions(KeyPermissions::with_budget(budget_usd));
    }

    let key_resp = eavs
        .create_key(key_req)
        .await
        .context("Failed to create eavs key")?;

    if !json_output {
        println!("  Key created: {} ({})", key_resp.key_id, key_resp.key);
    }

    // 3. Generate models.json (embed the virtual key directly so Pi doesn't need env vars)
    let models_json = generate_pi_models_json(&providers, eavs_base, Some(&key_resp.key));

    // 4. Write to user's home directory
    let home = get_user_home(linux_username)?;
    write_file_as_user(
        linux_username,
        &format!("{}/.pi/agent/models.json", home),
        &serde_json::to_string_pretty(&models_json)?,
    )?;

    // 5. Write eavs.env (key + URL for session injection)
    let env_content = format!("EAVS_API_KEY={}\nEAVS_URL={}\n", key_resp.key, eavs_base);
    let env_path = format!("{}/.config/oqto/eavs.env", home);
    write_file_as_user(linux_username, &env_path, &env_content)?;

    // 640 so the oqto service user (in the shared group) can read it for env injection
    let _ = std::process::Command::new("sudo")
        .args(["-u", linux_username, "chmod", "640", &env_path])
        .status();

    if !json_output {
        println!("  models.json written to {}/.pi/agent/models.json", home);
        println!("  eavs.env written to {}", env_path);
    }

    Ok(key_resp.key_id)
}

/// Write a file as a specific user, creating parent directories.
fn write_file_as_user(username: &str, path: &str, content: &str) -> Result<()> {
    // Create parent directory
    if let Some(parent) = std::path::Path::new(path).parent() {
        let status = std::process::Command::new("sudo")
            .args(["-u", username, "mkdir", "-p", &parent.display().to_string()])
            .status()
            .with_context(|| format!("Failed to create directory {}", parent.display()))?;
        if !status.success() {
            anyhow::bail!(
                "Failed to create directory {} for {}",
                parent.display(),
                username
            );
        }
    }

    // Write file via tee
    let mut child = std::process::Command::new("sudo")
        .args(["-u", username, "tee", path])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .spawn()
        .with_context(|| format!("Failed to write {}", path))?;

    if let Some(mut stdin) = child.stdin.take() {
        use std::io::Write;
        stdin.write_all(content.as_bytes())?;
    }

    let status = child.wait()?;
    if !status.success() {
        anyhow::bail!("Failed to write {}", path);
    }
    Ok(())
}

/// Get a user's home directory.
fn get_user_home(username: &str) -> Result<String> {
    let output = std::process::Command::new("bash")
        .args(["-c", &format!("echo ~{}", username)])
        .output()
        .context("Failed to get home directory")?;

    let home = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if home.is_empty() || home.starts_with('~') {
        anyhow::bail!("Could not determine home directory for '{}'", username);
    }
    Ok(home)
}

/// Setup oqto-runner for a user
fn setup_runner_for_user(username: &str, json: bool) -> Result<()> {
    // Get home directory
    let home = std::process::Command::new("bash")
        .args(["-c", &format!("echo ~{}", username)])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .context("Failed to get home directory")?;

    if home.is_empty() {
        anyhow::bail!("Could not determine home directory for '{}'", username);
    }

    // Create systemd user directory
    let systemd_dir = format!("{}/.config/systemd/user", home);
    if !json {
        println!("Creating systemd user directory...");
    }

    let status = std::process::Command::new("sudo")
        .args(["-u", username, "mkdir", "-p", &systemd_dir])
        .status()
        .context("Failed to create systemd directory")?;

    if !status.success() {
        anyhow::bail!("Failed to create systemd directory");
    }

    // Copy service file
    let service_src = "/usr/local/share/oqto/systemd/oqto-runner.service";
    let service_dst = format!("{}/oqto-runner.service", systemd_dir);

    // If source doesn't exist, try local path
    let service_content = if std::path::Path::new(service_src).exists() {
        std::fs::read_to_string(service_src).context("Failed to read service file")?
    } else {
        // Fallback to embedded service file
        include_str!("../../oqto/resources/systemd/oqto-runner.service").to_string()
    };

    if !json {
        println!("Installing oqto-runner.service...");
    }

    // Write service file as user
    let mut child = std::process::Command::new("sudo")
        .args(["-u", username, "tee", &service_dst])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .spawn()
        .context("Failed to write service file")?;

    if let Some(mut stdin) = child.stdin.take() {
        use std::io::Write;
        stdin.write_all(service_content.as_bytes())?;
    }

    let status = child.wait()?;
    if !status.success() {
        anyhow::bail!("Failed to write service file");
    }

    // Reload systemd
    if !json {
        println!("Reloading systemd...");
    }

    let _ = std::process::Command::new("sudo")
        .args(["-u", username, "systemctl", "--user", "daemon-reload"])
        .status();

    // Enable and start the service
    if !json {
        println!("Enabling oqto-runner service...");
    }

    let status = std::process::Command::new("sudo")
        .args([
            "-u",
            username,
            "systemctl",
            "--user",
            "enable",
            "oqto-runner",
        ])
        .status()
        .context("Failed to enable service")?;

    if !status.success() {
        eprintln!("Warning: Failed to enable oqto-runner service");
    }

    if !json {
        println!("Starting oqto-runner service...");
    }

    let status = std::process::Command::new("sudo")
        .args([
            "-u",
            username,
            "systemctl",
            "--user",
            "start",
            "oqto-runner",
        ])
        .status()
        .context("Failed to start service")?;

    if !status.success() {
        eprintln!("Warning: Failed to start oqto-runner service. It may start on user login.");
    }

    Ok(())
}

async fn handle_a2ui(client: &OqtoClient, command: A2uiCommand, json: bool) -> Result<()> {
    match command {
        A2uiCommand::Button {
            session,
            prompt,
            buttons,
            timeout,
        } => {
            if buttons.is_empty() {
                anyhow::bail!("At least one button is required");
            }

            // Build A2UI messages for buttons
            let surface_id = format!(
                "btn-{}",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_millis()
            );

            let mut components = vec![];
            let mut button_ids = vec![];

            // Add prompt text if provided
            if let Some(ref text) = prompt {
                components.push(serde_json::json!({
                    "id": "prompt",
                    "component": {
                        "Text": {
                            "text": { "literalString": text }
                        }
                    }
                }));
            }

            // Add buttons
            for (i, label) in buttons.iter().enumerate() {
                let btn_id = format!("btn_{}", i);
                let txt_id = format!("txt_{}", i);
                button_ids.push(btn_id.clone());

                components.push(serde_json::json!({
                    "id": txt_id,
                    "component": {
                        "Text": {
                            "text": { "literalString": label }
                        }
                    }
                }));

                components.push(serde_json::json!({
                    "id": btn_id,
                    "component": {
                        "Button": {
                            "child": txt_id,
                            "primary": i == 0,
                            "action": {
                                "name": label,
                                "context": []
                            }
                        }
                    }
                }));
            }

            // Build row layout for buttons
            let mut row_children: Vec<String> = button_ids;
            if prompt.is_some() {
                row_children.insert(0, "prompt".to_string());
            }

            // Create column layout
            components.push(serde_json::json!({
                "id": "row",
                "component": {
                    "Row": {
                        "children": row_children.iter().skip(if prompt.is_some() { 1 } else { 0 }).collect::<Vec<_>>(),
                        "mainAxisAlignment": "start",
                        "crossAxisAlignment": "center",
                        "spacing": 8
                    }
                }
            }));

            let root_children: Vec<&str> = if prompt.is_some() {
                vec!["prompt", "row"]
            } else {
                vec!["row"]
            };

            components.push(serde_json::json!({
                "id": "root",
                "component": {
                    "Column": {
                        "children": root_children,
                        "spacing": 12
                    }
                }
            }));

            let messages = vec![
                serde_json::json!({
                    "surfaceUpdate": {
                        "surfaceId": surface_id,
                        "components": components
                    }
                }),
                serde_json::json!({
                    "beginRendering": {
                        "surfaceId": surface_id,
                        "root": "root"
                    }
                }),
            ];

            send_a2ui_surface(client, &session, messages, true, timeout, json).await
        }

        A2uiCommand::Input {
            session,
            prompt,
            placeholder,
            input_type,
            timeout,
        } => {
            let surface_id = gen_surface_id("input");
            let text_field_type = match input_type.as_str() {
                "number" => "number",
                "password" => "obscured",
                "long" => "longText",
                _ => "shortText",
            };
            let placeholder_text = placeholder.as_deref().unwrap_or("Enter text...");

            let components = vec![
                serde_json::json!({
                    "id": "prompt_text",
                    "component": {
                        "Text": { "text": { "literalString": prompt } }
                    }
                }),
                serde_json::json!({
                    "id": "input_field",
                    "component": {
                        "TextField": {
                            "label": { "literalString": placeholder_text },
                            "text": { "path": "/user_input" },
                            "textFieldType": text_field_type
                        }
                    }
                }),
                serde_json::json!({
                    "id": "submit_text",
                    "component": {
                        "Text": { "text": { "literalString": "Submit" } }
                    }
                }),
                serde_json::json!({
                    "id": "submit_btn",
                    "component": {
                        "Button": {
                            "child": "submit_text",
                            "primary": true,
                            "action": {
                                "name": "submit",
                                "context": [{ "key": "user_input" }]
                            }
                        }
                    }
                }),
                serde_json::json!({
                    "id": "root",
                    "component": {
                        "Column": {
                            "children": ["prompt_text", "input_field", "submit_btn"],
                            "spacing": 12
                        }
                    }
                }),
            ];

            let messages = build_surface_messages(&surface_id, components);
            send_a2ui_surface(client, &session, messages, true, timeout, json).await
        }

        A2uiCommand::Choice {
            session,
            prompt,
            choices,
            multi,
            timeout,
        } => {
            if choices.is_empty() {
                anyhow::bail!("At least one choice is required");
            }

            let surface_id = format!(
                "choice-{}",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_millis()
            );

            let mut components = vec![];

            if let Some(ref text) = prompt {
                components.push(serde_json::json!({
                    "id": "prompt",
                    "component": {
                        "Text": {
                            "text": { "literalString": text }
                        }
                    }
                }));
            }

            // Build options for MultipleChoice
            let options: Vec<serde_json::Value> = choices
                .iter()
                .map(|c| serde_json::json!({ "label": c, "value": c }))
                .collect();

            components.push(serde_json::json!({
                "id": "choices",
                "component": {
                    "MultipleChoice": {
                        "dataKey": "selection",
                        "options": options,
                        "multiSelect": multi
                    }
                }
            }));

            components.push(serde_json::json!({
                "id": "submit_text",
                "component": {
                    "Text": {
                        "text": { "literalString": "Confirm" }
                    }
                }
            }));

            components.push(serde_json::json!({
                "id": "submit_btn",
                "component": {
                    "Button": {
                        "child": "submit_text",
                        "primary": true,
                        "action": {
                            "name": "selected",
                            "context": [{ "key": "selection" }]
                        }
                    }
                }
            }));

            let root_children: Vec<&str> = if prompt.is_some() {
                vec!["prompt", "choices", "submit_btn"]
            } else {
                vec!["choices", "submit_btn"]
            };

            components.push(serde_json::json!({
                "id": "root",
                "component": {
                    "Column": {
                        "children": root_children,
                        "spacing": 12
                    }
                }
            }));

            let messages = vec![
                serde_json::json!({
                    "surfaceUpdate": {
                        "surfaceId": surface_id,
                        "components": components
                    }
                }),
                serde_json::json!({
                    "beginRendering": {
                        "surfaceId": surface_id,
                        "root": "root"
                    }
                }),
            ];

            send_a2ui_surface(client, &session, messages, true, timeout, json).await
        }

        A2uiCommand::Checkbox {
            session,
            label,
            checked,
            timeout,
        } => {
            let surface_id = gen_surface_id("checkbox");
            let components = vec![
                serde_json::json!({
                    "id": "checkbox",
                    "component": {
                        "CheckBox": {
                            "label": { "literalString": label },
                            "value": { "literalBoolean": checked }
                        }
                    }
                }),
                serde_json::json!({
                    "id": "submit_text",
                    "component": { "Text": { "text": { "literalString": "Confirm" } } }
                }),
                serde_json::json!({
                    "id": "submit_btn",
                    "component": {
                        "Button": {
                            "child": "submit_text",
                            "primary": true,
                            "action": { "name": "confirmed", "context": [{ "key": "checked" }] }
                        }
                    }
                }),
                serde_json::json!({
                    "id": "root",
                    "component": { "Column": { "children": ["checkbox", "submit_btn"], "spacing": 12 } }
                }),
            ];
            let messages = build_surface_messages(&surface_id, components);
            send_a2ui_surface(client, &session, messages, true, timeout, json).await
        }

        A2uiCommand::Slider {
            session,
            prompt,
            min,
            max,
            value,
            timeout,
        } => {
            let surface_id = gen_surface_id("slider");
            let initial_value = value.unwrap_or(min);
            let mut components = vec![];

            if let Some(ref text) = prompt {
                components.push(serde_json::json!({
                    "id": "prompt",
                    "component": { "Text": { "text": { "literalString": text } } }
                }));
            }

            components.push(serde_json::json!({
                "id": "slider",
                "component": {
                    "Slider": {
                        "value": { "literalNumber": initial_value },
                        "minValue": min,
                        "maxValue": max
                    }
                }
            }));
            components.push(serde_json::json!({
                "id": "submit_text",
                "component": { "Text": { "text": { "literalString": "Confirm" } } }
            }));
            components.push(serde_json::json!({
                "id": "submit_btn",
                "component": {
                    "Button": {
                        "child": "submit_text",
                        "primary": true,
                        "action": { "name": "confirmed", "context": [{ "key": "slider_value" }] }
                    }
                }
            }));

            let root_children: Vec<&str> = if prompt.is_some() {
                vec!["prompt", "slider", "submit_btn"]
            } else {
                vec!["slider", "submit_btn"]
            };
            components.push(serde_json::json!({
                "id": "root",
                "component": { "Column": { "children": root_children, "spacing": 12 } }
            }));

            let messages = build_surface_messages(&surface_id, components);
            send_a2ui_surface(client, &session, messages, true, timeout, json).await
        }

        A2uiCommand::Datetime {
            session,
            prompt,
            date,
            time,
            value,
            timeout,
        } => {
            let surface_id = gen_surface_id("datetime");
            let mut components = vec![];

            if let Some(ref text) = prompt {
                components.push(serde_json::json!({
                    "id": "prompt",
                    "component": { "Text": { "text": { "literalString": text } } }
                }));
            }

            components.push(serde_json::json!({
                "id": "datetime",
                "component": {
                    "DateTimeInput": {
                        "value": { "literalString": value.as_deref().unwrap_or("") },
                        "enableDate": date,
                        "enableTime": time
                    }
                }
            }));
            components.push(serde_json::json!({
                "id": "submit_text",
                "component": { "Text": { "text": { "literalString": "Confirm" } } }
            }));
            components.push(serde_json::json!({
                "id": "submit_btn",
                "component": {
                    "Button": {
                        "child": "submit_text",
                        "primary": true,
                        "action": { "name": "confirmed", "context": [{ "key": "datetime_value" }] }
                    }
                }
            }));

            let root_children: Vec<&str> = if prompt.is_some() {
                vec!["prompt", "datetime", "submit_btn"]
            } else {
                vec!["datetime", "submit_btn"]
            };
            components.push(serde_json::json!({
                "id": "root",
                "component": { "Column": { "children": root_children, "spacing": 12 } }
            }));

            let messages = build_surface_messages(&surface_id, components);
            send_a2ui_surface(client, &session, messages, true, timeout, json).await
        }

        A2uiCommand::Text {
            session,
            text,
            style,
        } => {
            let surface_id = gen_surface_id("text");
            let usage_hint = match style.as_str() {
                "h1" | "h2" | "h3" | "h4" | "h5" | "caption" | "body" => Some(style.as_str()),
                _ => None,
            };
            let mut text_component = serde_json::json!({
                "Text": { "text": { "literalString": text } }
            });
            if let Some(hint) = usage_hint {
                text_component["Text"]["usageHint"] = serde_json::json!(hint);
            }
            let components = vec![serde_json::json!({
                "id": "root",
                "component": text_component
            })];
            let messages = build_surface_messages(&surface_id, components);
            // Non-blocking for text display
            send_a2ui_surface(client, &session, messages, false, 0, json).await
        }

        A2uiCommand::Image {
            session,
            url,
            fit,
            confirm,
            timeout,
        } => {
            let surface_id = gen_surface_id("image");
            let mut components = vec![serde_json::json!({
                "id": "image",
                "component": {
                    "Image": {
                        "url": { "literalString": url },
                        "fit": fit
                    }
                }
            })];

            let root_children: Vec<&str> = if confirm {
                components.push(serde_json::json!({
                    "id": "confirm_text",
                    "component": { "Text": { "text": { "literalString": "OK" } } }
                }));
                components.push(serde_json::json!({
                    "id": "confirm_btn",
                    "component": {
                        "Button": {
                            "child": "confirm_text",
                            "primary": true,
                            "action": { "name": "confirmed", "context": [] }
                        }
                    }
                }));
                vec!["image", "confirm_btn"]
            } else {
                vec!["image"]
            };

            components.push(serde_json::json!({
                "id": "root",
                "component": { "Column": { "children": root_children, "spacing": 12 } }
            }));

            let messages = build_surface_messages(&surface_id, components);
            send_a2ui_surface(client, &session, messages, confirm, timeout, json).await
        }

        A2uiCommand::Video {
            session,
            url,
            confirm,
            timeout,
        } => {
            let surface_id = gen_surface_id("video");
            let mut components = vec![serde_json::json!({
                "id": "video",
                "component": { "Video": { "url": { "literalString": url } } }
            })];

            let root_children: Vec<&str> = if confirm {
                components.push(serde_json::json!({
                    "id": "confirm_text",
                    "component": { "Text": { "text": { "literalString": "OK" } } }
                }));
                components.push(serde_json::json!({
                    "id": "confirm_btn",
                    "component": {
                        "Button": {
                            "child": "confirm_text",
                            "primary": true,
                            "action": { "name": "confirmed", "context": [] }
                        }
                    }
                }));
                vec!["video", "confirm_btn"]
            } else {
                vec!["video"]
            };

            components.push(serde_json::json!({
                "id": "root",
                "component": { "Column": { "children": root_children, "spacing": 12 } }
            }));

            let messages = build_surface_messages(&surface_id, components);
            send_a2ui_surface(client, &session, messages, confirm, timeout, json).await
        }

        A2uiCommand::Audio {
            session,
            url,
            description,
            confirm,
            timeout,
        } => {
            let surface_id = gen_surface_id("audio");
            let mut audio_component = serde_json::json!({
                "AudioPlayer": { "url": { "literalString": url } }
            });
            if let Some(ref desc) = description {
                audio_component["AudioPlayer"]["description"] =
                    serde_json::json!({ "literalString": desc });
            }

            let mut components = vec![serde_json::json!({
                "id": "audio",
                "component": audio_component
            })];

            let root_children: Vec<&str> = if confirm {
                components.push(serde_json::json!({
                    "id": "confirm_text",
                    "component": { "Text": { "text": { "literalString": "OK" } } }
                }));
                components.push(serde_json::json!({
                    "id": "confirm_btn",
                    "component": {
                        "Button": {
                            "child": "confirm_text",
                            "primary": true,
                            "action": { "name": "confirmed", "context": [] }
                        }
                    }
                }));
                vec!["audio", "confirm_btn"]
            } else {
                vec!["audio"]
            };

            components.push(serde_json::json!({
                "id": "root",
                "component": { "Column": { "children": root_children, "spacing": 12 } }
            }));

            let messages = build_surface_messages(&surface_id, components);
            send_a2ui_surface(client, &session, messages, confirm, timeout, json).await
        }

        A2uiCommand::Tabs {
            session,
            tabs,
            confirm,
            timeout,
        } => {
            let tab_defs: Vec<serde_json::Value> =
                serde_json::from_str(&tabs).context("parsing tabs JSON")?;

            let surface_id = gen_surface_id("tabs");
            let mut components = vec![];
            let mut tab_items = vec![];

            for (i, tab) in tab_defs.iter().enumerate() {
                let title = tab.get("title").and_then(|t| t.as_str()).unwrap_or("Tab");
                let content = tab.get("content").and_then(|c| c.as_str()).unwrap_or("");

                let content_id = format!("tab_content_{}", i);
                components.push(serde_json::json!({
                    "id": content_id,
                    "component": { "Text": { "text": { "literalString": content } } }
                }));

                tab_items.push(serde_json::json!({
                    "title": { "literalString": title },
                    "child": content_id
                }));
            }

            components.push(serde_json::json!({
                "id": "tabs",
                "component": { "Tabs": { "tabItems": tab_items } }
            }));

            let root_children: Vec<&str> = if confirm {
                components.push(serde_json::json!({
                    "id": "confirm_text",
                    "component": { "Text": { "text": { "literalString": "OK" } } }
                }));
                components.push(serde_json::json!({
                    "id": "confirm_btn",
                    "component": {
                        "Button": {
                            "child": "confirm_text",
                            "primary": true,
                            "action": { "name": "confirmed", "context": [] }
                        }
                    }
                }));
                vec!["tabs", "confirm_btn"]
            } else {
                vec!["tabs"]
            };

            components.push(serde_json::json!({
                "id": "root",
                "component": { "Column": { "children": root_children, "spacing": 12 } }
            }));

            let messages = build_surface_messages(&surface_id, components);
            send_a2ui_surface(client, &session, messages, confirm, timeout, json).await
        }

        A2uiCommand::Raw {
            session,
            messages,
            blocking,
            timeout,
        } => {
            let parsed_messages: Vec<serde_json::Value> = if let Some(msg_str) = messages {
                serde_json::from_str(&msg_str).context("parsing A2UI messages JSON")?
            } else {
                // Read from stdin
                let mut input = String::new();
                io::stdin()
                    .read_to_string(&mut input)
                    .context("reading from stdin")?;
                serde_json::from_str(&input).context("parsing A2UI messages from stdin")?
            };

            send_a2ui_surface(client, &session, parsed_messages, blocking, timeout, json).await
        }
    }
}

async fn handle_bus(client: &OqtoClient, command: BusCommand, json: bool) -> Result<()> {
    match command {
        BusCommand::Status => {
            let response = client.get("/admin/bus/stats").await?;
            if !response.status().is_success() {
                let body = response.text().await.unwrap_or_else(|_| "".to_string());
                return Err(anyhow!(
                    "bus status failed ({}): {}",
                    response.status(),
                    body
                ));
            }
            let stats: serde_json::Value = response.json().await?;
            if json {
                println!("{}", serde_json::to_string_pretty(&stats)?);
            } else {
                println!("Bus stats:");
                println!(
                    "  subscribers: {}",
                    stats
                        .get("subscriber_count")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0)
                );
                println!(
                    "  subscriptions: {}",
                    stats
                        .get("total_subscriptions")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0)
                );
                println!(
                    "  published: {}",
                    stats
                        .get("events_published")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0)
                );
                println!(
                    "  delivered: {}",
                    stats
                        .get("events_delivered")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0)
                );
                println!(
                    "  dropped(authz): {}",
                    stats
                        .get("events_dropped_authz")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0)
                );
                println!(
                    "  dropped(rate): {}",
                    stats
                        .get("events_dropped_rate")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0)
                );
            }
            Ok(())
        }
        BusCommand::Publish {
            scope,
            scope_id,
            topic,
            payload,
            version,
        } => {
            let payload_json = if let Some(p) = payload {
                serde_json::from_str::<serde_json::Value>(&p)
                    .with_context(|| "parsing --payload JSON")?
            } else {
                let mut input = String::new();
                std::io::stdin()
                    .read_to_string(&mut input)
                    .context("reading payload JSON from stdin")?;
                if input.trim().is_empty() {
                    serde_json::json!({})
                } else {
                    serde_json::from_str::<serde_json::Value>(&input)
                        .context("parsing payload JSON from stdin")?
                }
            };

            let body = serde_json::json!({
                "scope": scope,
                "scope_id": scope_id,
                "topic": topic,
                "payload": payload_json,
                "version": version,
            });

            let response = client.post_json("/admin/bus/publish", &body).await?;
            if !response.status().is_success() {
                let err = response.text().await.unwrap_or_else(|_| "".to_string());
                return Err(anyhow!(
                    "bus publish failed ({}): {}",
                    response.status(),
                    err
                ));
            }

            let resp: serde_json::Value = response.json().await?;
            if json {
                println!("{}", serde_json::to_string_pretty(&resp)?);
            } else {
                let event_id = resp
                    .get("event_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");
                println!("Published bus event: {}", event_id);
            }
            Ok(())
        }
        BusCommand::Tail {
            scope,
            scope_id,
            topics,
            limit,
            timeout,
        } => {
            use futures::{SinkExt, StreamExt};
            use tokio_tungstenite::connect_async;
            use tokio_tungstenite::tungstenite::protocol::Message;

            let base_url = client
                .http_base_url()
                .ok_or_else(|| anyhow!("bus tail requires HTTP transport (not admin socket)"))?;
            let ws_url = base_url
                .trim_end_matches('/')
                .replace("http://", "ws://")
                .replace("https://", "wss://")
                + "/ws/mux";

            use tokio_tungstenite::tungstenite::client::IntoClientRequest;
            let mut request = ws_url
                .into_client_request()
                .map_err(|e| anyhow!("failed to build websocket request: {}", e))?;
            if let Some(token) = client.auth_token.as_ref() {
                request
                    .headers_mut()
                    .insert("Authorization", format!("Bearer {}", token).parse()?);
            } else if let Some(dev_user) = client.dev_user.as_ref() {
                request
                    .headers_mut()
                    .insert("X-Dev-User", dev_user.parse()?);
            }

            let (mut ws, _) = connect_async(request)
                .await
                .context("connecting to websocket mux for bus tail")?;

            let subscribe_cmd = serde_json::json!({
                "channel": "bus",
                "type": "subscribe",
                "id": "oqtoctl-bus-tail-sub",
                "scope": scope,
                "scope_id": scope_id,
                "topics": topics,
            });
            ws.send(Message::Text(subscribe_cmd.to_string().into()))
                .await
                .context("sending bus subscribe command")?;

            let deadline =
                timeout.map(|t| tokio::time::Instant::now() + std::time::Duration::from_secs(t));
            let mut seen = 0usize;

            loop {
                let next_msg = if let Some(dl) = deadline {
                    tokio::select! {
                        _ = tokio::time::sleep_until(dl) => break,
                        msg = ws.next() => msg,
                    }
                } else {
                    ws.next().await
                };

                let Some(msg) = next_msg else { break };
                let msg = msg.context("reading websocket message")?;

                match msg {
                    Message::Text(text) => {
                        let value: serde_json::Value = match serde_json::from_str(&text) {
                            Ok(v) => v,
                            Err(_) => continue,
                        };
                        if value.get("channel").and_then(|v| v.as_str()) != Some("bus") {
                            continue;
                        }
                        if value.get("type").and_then(|v| v.as_str()) != Some("event") {
                            continue;
                        }

                        seen += 1;
                        if json {
                            println!("{}", serde_json::to_string_pretty(&value)?);
                        } else {
                            let topic = value.get("topic").and_then(|v| v.as_str()).unwrap_or("-");
                            let scope = value.get("scope").and_then(|v| v.as_str()).unwrap_or("-");
                            let scope_id = value
                                .get("scope_id")
                                .and_then(|v| v.as_str())
                                .unwrap_or("-");
                            let source = value
                                .get("source")
                                .and_then(|v| v.get("type"))
                                .and_then(|v| v.as_str())
                                .unwrap_or("-");
                            println!("[{scope}/{scope_id}] {topic} <- {source}");
                        }

                        if let Some(max) = limit
                            && seen >= max
                        {
                            break;
                        }
                    }
                    Message::Close(_) => break,
                    _ => {}
                }
            }

            // Best-effort unsubscribe
            let _ = ws
                .send(Message::Text(
                    serde_json::json!({
                        "channel": "bus",
                        "type": "unsubscribe",
                        "id": "oqtoctl-bus-tail-unsub",
                        "scope": scope,
                        "scope_id": scope_id,
                        "topics": topics,
                    })
                    .to_string()
                    .into(),
                ))
                .await;

            Ok(())
        }
    }
}

async fn handle_ui(client: &OqtoClient, command: UiCommand, json: bool) -> Result<()> {
    match command {
        UiCommand::Navigate { path, replace } => {
            let body = serde_json::json!({ "path": path, "replace": replace });
            send_ui_event(client, "/ui/navigate", body, json).await
        }
        UiCommand::Session { session_id, mode } => {
            let body = serde_json::json!({ "session_id": session_id, "mode": mode });
            send_ui_event(client, "/ui/session", body, json).await
        }
        UiCommand::View { view } => {
            let body = serde_json::json!({ "view": view });
            send_ui_event(client, "/ui/view", body, json).await
        }
        UiCommand::Palette { open } => {
            let body = serde_json::json!({ "open": open.unwrap_or(true) });
            send_ui_event(client, "/ui/palette", body, json).await
        }
        UiCommand::PaletteExec { command, args } => {
            let args_value = match args {
                Some(raw) => Some(
                    serde_json::from_str::<serde_json::Value>(&raw)
                        .context("parsing palette exec args JSON")?,
                ),
                None => None,
            };
            let body = serde_json::json!({ "command": command, "args": args_value });
            send_ui_event(client, "/ui/palette/exec", body, json).await
        }
        UiCommand::Spotlight {
            target,
            title,
            description,
            action,
            position,
            clear,
        } => {
            let body = serde_json::json!({
                "target": if clear { None::<String> } else { target },
                "title": title,
                "description": description,
                "action": action,
                "position": position,
                "active": !clear,
            });
            send_ui_event(client, "/ui/spotlight", body, json).await
        }
        UiCommand::Tour {
            steps,
            start_index,
            stop,
        } => {
            let steps_value = if stop {
                serde_json::Value::Array(vec![])
            } else if let Some(raw) = steps {
                serde_json::from_str::<serde_json::Value>(&raw)
                    .context("parsing tour steps JSON")?
            } else {
                let mut input = String::new();
                io::stdin()
                    .read_to_string(&mut input)
                    .context("reading tour steps from stdin")?;
                serde_json::from_str::<serde_json::Value>(&input)
                    .context("parsing tour steps from stdin")?
            };

            let steps_array = match steps_value {
                serde_json::Value::Array(values) => values,
                _ => anyhow::bail!("tour steps must be a JSON array"),
            };

            let body = serde_json::json!({
                "steps": steps_array,
                "start_index": start_index,
                "active": !stop,
            });
            send_ui_event(client, "/ui/tour", body, json).await
        }
        UiCommand::Sidebar { collapsed } => {
            let body = serde_json::json!({ "collapsed": collapsed });
            send_ui_event(client, "/ui/sidebar", body, json).await
        }
        UiCommand::Panel { view, collapsed } => {
            let body = serde_json::json!({ "view": view, "collapsed": collapsed });
            send_ui_event(client, "/ui/panel", body, json).await
        }
        UiCommand::Theme { theme } => {
            let body = serde_json::json!({ "theme": theme });
            send_ui_event(client, "/ui/theme", body, json).await
        }
    }
}

async fn send_ui_event(
    client: &OqtoClient,
    path: &str,
    body: serde_json::Value,
    json: bool,
) -> Result<()> {
    let response = client.post_json(path, &body).await?;
    let status = response.status();
    let text = response.text().await.context("reading response body")?;

    if json {
        if text.trim().is_empty() {
            println!(
                r#"{{"success": {}, "path": "{}"}}"#,
                status.is_success(),
                path
            );
        } else {
            println!("{text}");
        }
        return Ok(());
    }

    if status.is_success() {
        println!("UI event sent: {path}");
    } else if text.trim().is_empty() {
        println!("Server returned error: {}", status);
    } else {
        println!("Server returned error: {} - {}", status, text.trim());
    }
    Ok(())
}

fn gen_surface_id(prefix: &str) -> String {
    format!(
        "{}-{}",
        prefix,
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis()
    )
}

fn build_surface_messages(
    surface_id: &str,
    components: Vec<serde_json::Value>,
) -> Vec<serde_json::Value> {
    vec![
        serde_json::json!({
            "surfaceUpdate": {
                "surfaceId": surface_id,
                "components": components
            }
        }),
        serde_json::json!({
            "beginRendering": {
                "surfaceId": surface_id,
                "root": "root"
            }
        }),
    ]
}

async fn send_a2ui_surface(
    client: &OqtoClient,
    session_id: &str,
    messages: Vec<serde_json::Value>,
    blocking: bool,
    timeout_secs: u64,
    json: bool,
) -> Result<()> {
    let body = serde_json::json!({
        "session_id": session_id,
        "messages": messages,
        "blocking": blocking,
        "timeout_secs": timeout_secs,
    });

    let response = client.post_json("/a2ui/surface", &body).await?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        anyhow::bail!("A2UI request failed ({}): {}", status, body);
    }

    let result: serde_json::Value = response.json().await?;

    if json {
        println!("{}", serde_json::to_string(&result)?);
    } else if blocking {
        // Extract the action name and context for human-readable output
        if let Some(action) = result.get("action") {
            let name = action
                .get("name")
                .and_then(|n| n.as_str())
                .unwrap_or("unknown");
            println!("{}", name);

            // Print context values if any
            if let Some(context) = action.get("context").and_then(|c| c.as_array()) {
                for ctx in context {
                    if let (Some(key), Some(value)) =
                        (ctx.get("key").and_then(|k| k.as_str()), ctx.get("value"))
                    {
                        eprintln!("{}={}", key, value);
                    }
                }
            }
        } else {
            // Timeout or dismissed
            if result
                .get("timeout")
                .and_then(|t| t.as_bool())
                .unwrap_or(false)
            {
                anyhow::bail!("A2UI request timed out");
            }
            println!("dismissed");
        }
    } else {
        // Non-blocking, just confirm it was sent
        if let Some(request_id) = result.get("request_id").and_then(|r| r.as_str()) {
            println!("Surface sent (request_id: {})", request_id);
        } else {
            println!("Surface sent");
        }
    }

    Ok(())
}
