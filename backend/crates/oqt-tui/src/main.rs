//! `oqt-tui`: the terminal client for Durable agents (ADR-0087). A view and an owner device:
//! it never runs tools, calls models or reads stores.

mod app;
mod caps;
mod config;
mod editor;
mod keys;
mod kitty;
mod markdown;
mod model;
mod parts;
mod sanitize;
mod status;
mod ui;

use std::io::IsTerminal;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand, ValueEnum};
use oqto_session_client::{Link, Options as ClientOptions, Secret, SessionConfig, SessionHandle};
use oqto_session_view::frames::InputKind;

use crate::config::{Images, Overrides};

#[derive(Debug, Parser)]
#[command(
    name = "oqt-tui",
    version,
    about = "Terminal client for Durable agents over oqto.session-view"
)]
struct Cli {
    /// Config file (between the command line and the environment in priority).
    #[arg(long, global = true)]
    config: Option<PathBuf>,
    /// Host entry from the config to attach to.
    #[arg(long, global = true)]
    host: Option<String>,
    /// WebSocket URL of a host's /session path (overrides the host entry).
    #[arg(long, global = true)]
    url: Option<String>,
    /// Session id (overrides the host entry).
    #[arg(long, global = true)]
    session: Option<String>,
    /// Image output.
    #[arg(long, global = true, value_enum)]
    images: Option<ImagesArg>,
    /// ASCII glyphs only.
    #[arg(long, global = true)]
    ascii: bool,
    /// Append start-up timings to this file (measurements and tests).
    #[arg(long, hide = true)]
    timing_file: Option<PathBuf>,
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum ImagesArg {
    Auto,
    Kitty,
    Placeholders,
    Off,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Attach to the Session (the default).
    Attach,
    /// Diagnostics.
    Doctor {
        #[command(subcommand)]
        what: Doctor,
    },
    /// Configuration.
    Config {
        #[command(subcommand)]
        what: ConfigCommand,
    },
    /// Send one input without the TUI; with --wait, print the answer.
    Send {
        /// The text.
        text: String,
        /// Queue as a follow-up instead of a prompt.
        #[arg(long)]
        follow_up: bool,
        /// Wait until the input is answered and print the answer.
        #[arg(long)]
        wait: bool,
    },
}

#[derive(Debug, Subcommand)]
enum Doctor {
    /// Probe the terminal by reply and print what it supports.
    Terminal,
}

#[derive(Debug, Subcommand)]
enum ConfigCommand {
    /// Print the config file path (creating defaults on first run).
    Path,
    /// Print the JSON schema of config.toml.
    Schema,
    /// Print the effective configuration.
    Show,
}

fn overrides(cli: &Cli) -> Overrides {
    Overrides {
        config: cli.config.clone(),
        host: cli.host.clone(),
        url: cli.url.clone(),
        session: cli.session.clone(),
        images: cli.images.map(|i| match i {
            ImagesArg::Auto => Images::Auto,
            ImagesArg::Kitty => Images::Kitty,
            ImagesArg::Placeholders => Images::Placeholders,
            ImagesArg::Off => Images::Off,
        }),
    }
}

fn host_and_secret(config: &config::Config) -> Result<(config::HostConfig, Secret)> {
    let host = config
        .selected_host()
        .cloned()
        .with_context(|| format!("no host named {} in the config", config.host))?;
    let secret = Secret::new(host.secret(&|name| std::env::var(name).ok())?)?;
    Ok((host, secret))
}

fn resume_command(cli: &Cli, host: &config::HostConfig) -> Vec<String> {
    let exe = std::env::current_exe().map_or_else(
        |_| "oqt-tui".to_owned(),
        |p| p.to_string_lossy().into_owned(),
    );
    let mut command = vec![exe];
    if let Some(config) = &cli.config {
        command.extend(["--config".to_owned(), config.to_string_lossy().into_owned()]);
    }
    command.extend([
        "--url".to_owned(),
        host.url.clone(),
        "--session".to_owned(),
        host.session.clone(),
        "attach".to_owned(),
    ]);
    command
}

async fn send(config: &config::Config, text: String, follow_up: bool, wait: bool) -> Result<()> {
    let (host, secret) = host_and_secret(config)?;
    let (handle, task) = SessionHandle::start(SessionConfig {
        url: host.url.clone(),
        secret,
        session: host.session.clone(),
        options: ClientOptions::default(),
        cache: None,
        max_cached_entries: config.cache.max_entries,
        connect_timeout: Duration::from_millis(config.connection.connect_timeout_ms),
    });
    let kind = if follow_up {
        InputKind::FollowUp
    } else {
        InputKind::Prompt
    };
    let request = handle.update(|c| c.submit(kind, text));
    let mut changes = handle.changed();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(300);
    loop {
        let (status, answer, link) = handle.read(|s| {
            let receipt = s.client.receipt(&request).cloned();
            let answer = receipt.as_ref().and_then(|r| r.answer).and_then(|id| {
                s.client
                    .transcript()
                    .entries()
                    .iter()
                    .find(|e| e.id() == id)
                    .cloned()
            });
            (receipt, answer, s.link.clone())
        });
        if let Some(receipt) = &status {
            let settled = receipt.status.is_final();
            if !wait || (settled && (answer.is_some() || receipt.answer.is_none())) {
                if let Some(entry) = answer {
                    let text =
                        entry
                            .model()
                            .and_then(|m| m.pointer("/0/content"))
                            .map(|c| match c {
                                serde_json::Value::String(t) => t.clone(),
                                serde_json::Value::Array(blocks) => blocks
                                    .iter()
                                    .filter_map(|b| {
                                        b.get("text").and_then(serde_json::Value::as_str)
                                    })
                                    .collect(),
                                _ => String::new(),
                            });
                    println!("{}", sanitize::clean(&text.unwrap_or_default()));
                } else {
                    println!("{}", serde_json::to_string(receipt)?);
                }
                break;
            }
        }
        if let Link::Offline(reason) = &link
            && tokio::time::Instant::now() > deadline
        {
            bail!("not connected: {reason}");
        }
        if tokio::time::timeout_at(deadline, changes.changed())
            .await
            .is_err()
        {
            bail!("timed out waiting for the receipt");
        }
    }
    task.abort();
    Ok(())
}

fn doctor_terminal(config: &config::Config) -> Result<()> {
    if !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal() {
        bail!("doctor terminal needs a terminal on stdin and stdout");
    }
    let env = caps::Environment::read(&|name| std::env::var(name).ok());
    crossterm::terminal::enable_raw_mode().context("raw mode")?;
    let replies = caps::probe(
        env.tmux,
        Duration::from_millis(config.terminal.probe_timeout_ms.max(300)),
    );
    crossterm::terminal::disable_raw_mode().context("leave raw mode")?;
    let caps = caps::decide(replies?, env, &config.terminal);
    for line in caps::report(&caps) {
        println!("{line}");
    }
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let overrides = overrides(&cli);
    match &cli.command {
        Some(Command::Config {
            what: ConfigCommand::Schema,
        }) => {
            print!("{}", config::schema_json()?);
            return Ok(());
        }
        Some(Command::Config {
            what: ConfigCommand::Path,
        }) => {
            println!("{}", config::ensure_global()?.display());
            return Ok(());
        }
        _ => {}
    }
    let config = config::load(&overrides)?;
    match cli.command {
        Some(Command::Config {
            what: ConfigCommand::Show,
        }) => {
            print!("{}", toml::to_string_pretty(&config)?);
            Ok(())
        }
        Some(Command::Doctor {
            what: Doctor::Terminal,
        }) => doctor_terminal(&config),
        Some(Command::Send {
            text,
            follow_up,
            wait,
        }) => send(&config, text, follow_up, wait).await,
        Some(Command::Config { .. }) => Ok(()),
        None | Some(Command::Attach) => {
            if !std::io::stdout().is_terminal() {
                bail!("oqt-tui needs a terminal; use `oqt-tui send` without one");
            }
            let (host, secret) = host_and_secret(&config)?;
            let resume = resume_command(&cli, &host);
            app::run(app::Launch {
                config,
                host,
                secret,
                timing_file: cli.timing_file,
                resume,
                ascii: cli.ascii,
            })
            .await
        }
    }
}
