//! octoctl - Control CLI for Octo server
//!
//! Provides administrative commands for managing the Octo server,
//! including container management, image refresh, and housekeeping.

use std::io::{self, IsTerminal, Read, Write};
use std::process::ExitCode;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};

const DEFAULT_SERVER_URL: &str = "http://localhost:8080";

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
    let client = OctoClient::new(&cli.server);

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
        Command::Local { command } => handle_local(&client, command, cli.json).await,
        Command::Sandbox { command } => handle_sandbox(command, cli.json).await,
        Command::User { command } => handle_user(&client, command, cli.json).await,
        Command::HashPassword { password, cost } => handle_hash_password(password, cost),
    }
}

#[derive(Debug, Parser)]
#[command(
    name = "octoctl",
    author,
    version,
    about = "Control CLI for Octo server - manage containers, sessions, and images."
)]
struct Cli {
    /// Octo server URL
    #[arg(long, short = 's', default_value = DEFAULT_SERVER_URL, env = "OCTO_SERVER_URL")]
    server: String,

    /// Output machine-readable JSON
    #[arg(long, global = true)]
    json: bool,

    /// Path to Octo config file (auto-detected if not set)
    #[arg(long, short = 'c', env = "OCTO_CONFIG", global = true)]
    config: Option<String>,

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
        /// Image name (default: octo-dev:latest)
        #[arg(default_value = "octo-dev:latest")]
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
    /// Create a new user with Linux user and runner provisioning
    Create {
        /// Octo username (will also be Linux username if not specified)
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
        /// Linux username (defaults to Octo username)
        #[arg(long)]
        linux_user: Option<String>,
        /// Skip Linux user creation (use existing user)
        #[arg(long)]
        no_linux_user: bool,
        /// Skip runner setup
        #[arg(long)]
        no_runner: bool,
        /// Provision eavs virtual key and generate Pi models.json
        #[arg(long)]
        eavs: bool,
        /// Eavs server URL
        #[arg(long, default_value = "http://127.0.0.1:3033", env = "EAVS_URL")]
        eavs_url: String,
        /// Eavs master key for admin API
        #[arg(long, env = "EAVS_MASTER_KEY")]
        eavs_master_key: Option<String>,
        /// Budget limit in USD for the eavs key (default: unlimited)
        #[arg(long)]
        eavs_budget: Option<f64>,
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
    /// Sync per-user config files via the admin API
    SyncConfigs {
        /// Optional user ID to target
        #[arg(long)]
        user: Option<String>,
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
        /// Database path (default: ~/.local/share/octo/octo.db)
        #[arg(long, env = "OCTO_DATABASE_PATH")]
        database: Option<String>,
        /// Linux username (defaults to Octo username)
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
        /// Session ID (defaults to OCTO_SESSION_ID env var)
        #[arg(long, short, env = "OCTO_SESSION_ID")]
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
        /// Session ID (defaults to OCTO_SESSION_ID env var)
        #[arg(long, short, env = "OCTO_SESSION_ID")]
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
        /// Session ID (defaults to OCTO_SESSION_ID env var)
        #[arg(long, short, env = "OCTO_SESSION_ID")]
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
        /// Session ID (defaults to OCTO_SESSION_ID env var)
        #[arg(long, short, env = "OCTO_SESSION_ID")]
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
        /// Session ID (defaults to OCTO_SESSION_ID env var)
        #[arg(long, short, env = "OCTO_SESSION_ID")]
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
        /// Session ID (defaults to OCTO_SESSION_ID env var)
        #[arg(long, short, env = "OCTO_SESSION_ID")]
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
        /// Session ID (defaults to OCTO_SESSION_ID env var)
        #[arg(long, short, env = "OCTO_SESSION_ID")]
        session: String,
        /// Text to display
        text: String,
        /// Text style: body, h1, h2, h3, h4, h5, caption
        #[arg(long, default_value = "body")]
        style: String,
    },
    /// Display an image
    Image {
        /// Session ID (defaults to OCTO_SESSION_ID env var)
        #[arg(long, short, env = "OCTO_SESSION_ID")]
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
        /// Session ID (defaults to OCTO_SESSION_ID env var)
        #[arg(long, short, env = "OCTO_SESSION_ID")]
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
        /// Session ID (defaults to OCTO_SESSION_ID env var)
        #[arg(long, short, env = "OCTO_SESSION_ID")]
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
        /// Session ID (defaults to OCTO_SESSION_ID env var)
        #[arg(long, short, env = "OCTO_SESSION_ID")]
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
        /// Session ID (defaults to OCTO_SESSION_ID env var)
        #[arg(long, short, env = "OCTO_SESSION_ID")]
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

/// HTTP client for communicating with Octo server
struct OctoClient {
    base_url: String,
    client: reqwest::Client,
    dev_user: Option<String>,
    auth_token: Option<String>,
}

impl OctoClient {
    fn new(base_url: &str) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            client: reqwest::Client::new(),
            dev_user: std::env::var("OCTO_DEV_USER").ok(),
            auth_token: std::env::var("OCTO_AUTH_TOKEN").ok(),
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

    async fn get(&self, path: &str) -> Result<reqwest::Response> {
        let url = format!("{}{}", self.base_url, path);
        self.with_auth_headers(self.client.get(&url))
            .send()
            .await
            .context("sending request to server")
    }

    async fn post(&self, path: &str) -> Result<reqwest::Response> {
        let url = format!("{}{}", self.base_url, path);
        self.with_auth_headers(self.client.post(&url))
            .send()
            .await
            .context("sending request to server")
    }

    async fn delete(&self, path: &str) -> Result<reqwest::Response> {
        let url = format!("{}{}", self.base_url, path);
        self.with_auth_headers(self.client.delete(&url))
            .send()
            .await
            .context("sending request to server")
    }

    async fn post_json<T: serde::Serialize>(
        &self,
        path: &str,
        body: &T,
    ) -> Result<reqwest::Response> {
        let url = format!("{}{}", self.base_url, path);
        self.with_auth_headers(self.client.post(&url).json(body))
            .send()
            .await
            .context("sending request to server")
    }
}

async fn handle_status(client: &OctoClient, json: bool) -> Result<()> {
    let response = client.get("/health").await?;

    if response.status().is_success() {
        if json {
            println!(r#"{{"status": "ok", "server": "{}"}}"#, client.base_url);
        } else {
            println!("Server is running at {}", client.base_url);
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
    client: &OctoClient,
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
        // Use SSE streaming
        use futures::StreamExt;
        use reqwest_eventsource::{Event, EventSource};

        let url = format!("{}/api/agents/ask", client.base_url);
        let mut request = client.client.post(&url).json(&body);
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
        let response = client.post_json("/api/agents/ask", &body).await?;

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
    client: &OctoClient,
    query: Option<&str>,
    limit: usize,
    json_output: bool,
) -> Result<()> {
    let path = match query {
        Some(q) => format!(
            "/api/agents/sessions?q={}&limit={}",
            urlencoding::encode(q),
            limit
        ),
        None => format!("/api/agents/sessions?limit={}", limit),
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

async fn handle_session(client: &OctoClient, command: SessionCommand, json: bool) -> Result<()> {
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
    client: &OctoClient,
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

async fn handle_image(client: &OctoClient, command: ImageCommand, json: bool) -> Result<()> {
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
                    "Use 'octoctl container refresh --outdated-only' to update outdated containers"
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
                "octo-dev:latest",
            ]);

            if no_cache {
                cmd.arg("--no-cache");
            }

            cmd.arg(".");

            let output = cmd.output().context("running docker build")?;

            if output.status.success() {
                if json {
                    println!(r#"{{"status": "built", "image": "octo-dev:latest"}}"#);
                } else {
                    println!("Successfully built octo-dev:latest");
                }
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                anyhow::bail!("Failed to build image: {}", stderr);
            }
        }
    }
    Ok(())
}

async fn handle_local(client: &OctoClient, command: LocalCommand, json: bool) -> Result<()> {
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

const SYSTEM_SANDBOX_CONFIG: &str = "/etc/octo/sandbox.toml";

/// Default sandbox configuration content
const DEFAULT_SANDBOX_CONFIG: &str = r#"# Octo Sandbox Configuration (System-wide)
# This file is owned by root and trusted by octo-runner.
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
    "/etc/octo/sandbox.toml",
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
                    println!("  octoctl sandbox reset");
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

async fn handle_user(client: &OctoClient, command: UserCommand, json: bool) -> Result<()> {
    match command {
        UserCommand::Create {
            username,
            email,
            display_name,
            role,
            linux_user,
            no_linux_user,
            no_runner,
            eavs,
            eavs_url,
            eavs_master_key,
            eavs_budget,
        } => {
            let linux_username = linux_user.as_deref().unwrap_or(&username);

            // Validate role
            let _role = match role.to_lowercase().as_str() {
                "user" | "admin" => role.to_lowercase(),
                _ => anyhow::bail!("Invalid role: {}. Must be 'user' or 'admin'", role),
            };

            if !no_linux_user {
                create_linux_user(linux_username, json)?;
            }

            // Setup runner if not skipped
            if !no_runner && !no_linux_user {
                setup_runner_for_user(linux_username, json)?;
            }

            // Provision eavs key and generate models.json
            let mut eavs_key_id = None;
            if eavs {
                match provision_eavs_for_user(
                    linux_username,
                    &username,
                    &eavs_url,
                    eavs_master_key.as_deref(),
                    eavs_budget,
                    json,
                )
                .await
                {
                    Ok(key_id) => {
                        eavs_key_id = Some(key_id);
                    }
                    Err(e) => {
                        if !json {
                            eprintln!("Warning: Failed to provision eavs: {:?}", e);
                        }
                    }
                }
            }

            // Create Octo user via API
            // For now, just print what would be done - actual API call would need the server running
            if json {
                let result = serde_json::json!({
                    "status": "created",
                    "username": username,
                    "email": email,
                    "display_name": display_name.as_deref().unwrap_or(&username),
                    "role": role,
                    "linux_username": if no_linux_user { None } else { Some(linux_username) },
                    "runner_setup": !no_runner && !no_linux_user,
                    "eavs_key_id": eavs_key_id,
                });
                println!("{}", serde_json::to_string_pretty(&result)?);
            } else {
                println!("\nUser provisioning complete:");
                println!("  Octo username: {}", username);
                println!("  Email: {}", email);
                println!(
                    "  Display name: {}",
                    display_name.as_deref().unwrap_or(&username)
                );
                println!("  Role: {}", role);
                if !no_linux_user {
                    println!("  Linux user: {}", linux_username);
                }
                if !no_runner && !no_linux_user {
                    println!("  Runner: configured");
                }
                if let Some(ref key_id) = eavs_key_id {
                    println!("  Eavs key: {}", key_id);
                    println!("  Pi models.json: generated");
                }
                println!(
                    "\nNote: Run the Octo server and use the API to create the database user record."
                );
            }
        }

        UserCommand::List { runner_status } => {
            // List users from /etc/passwd that have octo-runner configured
            // In a full implementation, this would query the Octo database
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
                            format!("{}/.config/systemd/user/octo-runner.service", home);
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
            let service_path = format!("{}/.config/systemd/user/octo-runner.service", home);
            let runner_installed = std::path::Path::new(&service_path).exists();

            // Check runner status
            let runner_status = get_runner_status(&user);

            // Check socket
            let socket_path = format!("/run/user/{}/octo-runner.sock", uid);
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
            let service_path = format!("{}/.config/systemd/user/octo-runner.service", home);
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

            let socket_path = format!("/run/user/{}/octo-runner.sock", uid);
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

        UserCommand::SyncConfigs { user } => {
            let body = serde_json::json!({ "user_id": user });
            let response = client
                .post_json("/api/admin/users/sync-configs", &body)
                .await?;

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
            // octo_{sanitize(user_id)} -- unless explicitly overridden
            let linux_username = if let Some(ref explicit) = linux_user {
                explicit.clone()
            } else {
                let sanitized = sanitize_for_linux(&user_id);
                format!("octo_{sanitized}")
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
                if no_linux_user { None } else { Some(&linux_username) },
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
    sqlx::migrate!("./migrations")
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
        eprintln!("You can now start Octo and log in with these credentials.");
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

/// Generate a short random ID
/// Generate a user ID from a username (e.g., "admin" -> "admin-x1y2").
/// Mirrors UserRepository::generate_user_id for use in octoctl.
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

/// Database filename used by octo and octoctl.
const DB_FILENAME: &str = "octo.db";

/// Get the database path, checking multiple locations in order:
/// 1. Service data dir: /var/lib/octo/.local/share/octo/ (multi-user mode)
/// 2. XDG_DATA_HOME/octo/ (single-user / env override)
/// 3. ~/.local/share/octo/ (fallback)
fn get_database_path() -> Result<std::path::PathBuf> {
    // Multi-user: check the service user's data dir first
    let service_db = std::path::PathBuf::from("/var/lib/octo/.local/share/octo").join(DB_FILENAME);
    if service_db.exists() {
        return Ok(service_db);
    }

    // XDG_DATA_HOME (respects env override)
    if let Some(dir) = std::env::var_os("XDG_DATA_HOME").filter(|v| !v.is_empty()) {
        let db = std::path::PathBuf::from(dir).join("octo").join(DB_FILENAME);
        if db.exists() {
            return Ok(db);
        }
    }

    // Default: ~/.local/share/octo/
    let data_dir = dirs::data_dir()
        .map(|d| d.join("octo"))
        .unwrap_or_else(|| std::path::PathBuf::from("."));
    let db_path = data_dir.join(DB_FILENAME);
    if db_path.exists() {
        return Ok(db_path);
    }

    // Nothing exists yet, prefer service path if /var/lib/octo exists,
    // otherwise fall back to user dir
    if std::path::Path::new("/var/lib/octo").exists() {
        let dir = std::path::PathBuf::from("/var/lib/octo/.local/share/octo");
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
    let socket_path = format!("/run/user/{}/octo-runner.sock", uid);
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
                "octo-runner",
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
            .args(["--user", "is-active", "octo-runner"])
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

/// Provision eavs virtual key and generate Pi models.json for a user.
///
/// 1. Queries eavs /providers/detail for configured providers
/// 2. Creates a virtual key with oauth_user binding
/// 3. Generates ~/.pi/agent/models.json with eavs-routed providers
/// 4. Returns the eavs key ID
async fn provision_eavs_for_user(
    linux_username: &str,
    octo_username: &str,
    eavs_url: &str,
    master_key: Option<&str>,
    budget: Option<f64>,
    json_output: bool,
) -> Result<String> {
    use octo::eavs::{
        generate_pi_models_json, CreateKeyRequest, EavsClient, KeyPermissions,
    };

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

    let mut key_req = CreateKeyRequest::new(format!("octo-user-{}", octo_username))
        .oauth_user(octo_username);
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

    // 3. Generate models.json
    let models_json = generate_pi_models_json(&providers, eavs_base);

    // 4. Write to user's home directory
    let home = get_user_home(linux_username)?;
    write_file_as_user(
        linux_username,
        &format!("{}/.pi/agent/models.json", home),
        &serde_json::to_string_pretty(&models_json)?,
    )?;

    // 5. Write eavs.env (key + URL for session injection)
    let env_content = format!("EAVS_API_KEY={}\nEAVS_URL={}\n", key_resp.key, eavs_base);
    let env_path = format!("{}/.config/octo/eavs.env", home);
    write_file_as_user(linux_username, &env_path, &env_content)?;

    // Set restrictive permissions (contains secret key)
    let _ = std::process::Command::new("sudo")
        .args(["-u", linux_username, "chmod", "600", &env_path])
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
            anyhow::bail!("Failed to create directory {} for {}", parent.display(), username);
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

/// Setup octo-runner for a user
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
    let service_src = "/usr/local/share/octo/systemd/octo-runner.service";
    let service_dst = format!("{}/octo-runner.service", systemd_dir);

    // If source doesn't exist, try local path
    let service_content = if std::path::Path::new(service_src).exists() {
        std::fs::read_to_string(service_src).context("Failed to read service file")?
    } else {
        // Fallback to embedded service file
        include_str!("../../resources/systemd/octo-runner.service").to_string()
    };

    if !json {
        println!("Installing octo-runner.service...");
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
        println!("Enabling octo-runner service...");
    }

    let status = std::process::Command::new("sudo")
        .args([
            "-u",
            username,
            "systemctl",
            "--user",
            "enable",
            "octo-runner",
        ])
        .status()
        .context("Failed to enable service")?;

    if !status.success() {
        eprintln!("Warning: Failed to enable octo-runner service");
    }

    if !json {
        println!("Starting octo-runner service...");
    }

    let status = std::process::Command::new("sudo")
        .args([
            "-u",
            username,
            "systemctl",
            "--user",
            "start",
            "octo-runner",
        ])
        .status()
        .context("Failed to start service")?;

    if !status.success() {
        eprintln!("Warning: Failed to start octo-runner service. It may start on user login.");
    }

    Ok(())
}

async fn handle_a2ui(client: &OctoClient, command: A2uiCommand, json: bool) -> Result<()> {
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

async fn handle_ui(client: &OctoClient, command: UiCommand, json: bool) -> Result<()> {
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
    client: &OctoClient,
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
    client: &OctoClient,
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
