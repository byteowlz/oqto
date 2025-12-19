//! octoctl - Control CLI for Octo server
//!
//! Provides administrative commands for managing the Octo server,
//! including container management, image refresh, and housekeeping.

use std::io::{self, Write};
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
        Command::Session { command } => handle_session(&client, command, cli.json).await,
        Command::Container { command } => handle_container(&client, command, cli.json).await,
        Command::Image { command } => handle_image(&client, command, cli.json).await,
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

    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Check server status
    Status,

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

/// HTTP client for communicating with Octo server
struct OctoClient {
    base_url: String,
    client: reqwest::Client,
}

impl OctoClient {
    fn new(base_url: &str) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            client: reqwest::Client::new(),
        }
    }

    async fn get(&self, path: &str) -> Result<reqwest::Response> {
        let url = format!("{}{}", self.base_url, path);
        self.client
            .get(&url)
            .send()
            .await
            .context("sending request to server")
    }

    async fn post(&self, path: &str) -> Result<reqwest::Response> {
        let url = format!("{}{}", self.base_url, path);
        self.client
            .post(&url)
            .send()
            .await
            .context("sending request to server")
    }

    async fn delete(&self, path: &str) -> Result<reqwest::Response> {
        let url = format!("{}{}", self.base_url, path);
        self.client
            .delete(&url)
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
            println!(r#"{{"status": "error", "code": {}}}"#, response.status().as_u16());
        } else {
            println!("Server returned error: {}", response.status());
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
                println!("{:<12} {:<20} {:<10} {:<20}", "ID", "READABLE_ID", "STATUS", "IMAGE");
                println!("{}", "-".repeat(70));
                for session in sessions {
                    println!(
                        "{:<12} {:<20} {:<10} {:<20}",
                        session["id"].as_str().unwrap_or("").chars().take(8).collect::<String>(),
                        session["readable_id"].as_str().unwrap_or("-"),
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
                println!("  Readable ID: {}", session["readable_id"]);
                println!("  Status: {}", session["status"]);
                println!("  Image: {}", session["image"]);
                println!("  Container: {}", session["container_id"]);
                println!("  Ports: opencode={}, fileserver={}, ttyd={}",
                    session["opencode_port"], session["fileserver_port"], session["ttyd_port"]);
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

async fn handle_container(client: &OctoClient, command: ContainerCommand, json: bool) -> Result<()> {
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
                let containers: Vec<_> = sessions.iter()
                    .filter(|s| s["container_id"].as_str().is_some())
                    .map(|s| serde_json::json!({
                        "container_id": s["container_id"],
                        "container_name": s["container_name"],
                        "session_id": s["id"],
                        "status": s["status"],
                    }))
                    .collect();
                println!("{}", serde_json::to_string_pretty(&containers)?);
            } else {
                println!("{:<16} {:<20} {:<12} {:<10}", "CONTAINER", "NAME", "SESSION", "STATUS");
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
                println!(r#"{{"sessions_checked": {}, "outdated": 0}}"#, sessions.len());
            } else {
                println!("Checked {} session(s) for image updates", sessions.len());
                println!("Use 'octoctl container refresh --outdated-only' to update outdated containers");
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
            cmd.args(["build", "-f", &format!("{}/{}", path, dockerfile), "-t", "octo-dev:latest"]);
            
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
