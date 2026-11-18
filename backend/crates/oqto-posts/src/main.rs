//! Standalone foreground post store; lifecycle belongs to the caller.
#[cfg(unix)]
mod cli {
    use anyhow::{Context, Result, ensure};
    use clap::{Parser, Subcommand};
    use oqto_posts::{
        server::{Registration, Server, private_directory},
        service::{ServiceConfig, add_identity},
        store::Store,
    };
    use std::path::PathBuf;

    #[derive(Debug, Parser)]
    #[command(
        version,
        about = "Runner post store (standalone foreground, no Session delivery)"
    )]
    pub struct Cli {
        #[arg(long, env = "OQTO_POSTS_CONFIG", global = true)]
        config: Option<PathBuf>,
        #[command(subcommand)]
        command: Command,
    }
    #[derive(Debug, Subcommand)]
    enum Command {
        Service {
            #[command(subcommand)]
            command: ServiceCommand,
        },
        Registry {
            #[command(subcommand)]
            command: RegistryCommand,
        },
        Config {
            #[command(subcommand)]
            command: ConfigCommand,
        },
    }
    #[derive(Debug, Subcommand)]
    enum ServiceCommand {
        Run,
        Status,
    }
    #[derive(Debug, Subcommand)]
    enum ConfigCommand {
        Schema,
        Check,
    }
    #[derive(Debug, Subcommand)]
    enum RegistryCommand {
        List,
        /// Create a NEW candidate config; never edits live configuration.
        Add {
            #[arg(long)]
            identity: PathBuf,
            #[arg(long)]
            output: PathBuf,
        },
    }
    pub async fn run() -> Result<()> {
        let cli = Cli::parse();
        if matches!(
            cli.command,
            Command::Config {
                command: ConfigCommand::Schema
            }
        ) {
            println!("{}", ServiceConfig::schema()?);
            return Ok(());
        }
        let config = ServiceConfig::load(
            &cli.config
                .context("--config or OQTO_POSTS_CONFIG required (no default socket)")?,
        )?;
        match cli.command {
            Command::Config { .. } => println!("configuration valid; enabled={}", config.enabled),
            Command::Registry {
                command: RegistryCommand::List,
            } => println!("{}", serde_json::to_string_pretty(&config.identities)?),
            Command::Registry {
                command: RegistryCommand::Add { identity, output },
            } => {
                let identity: Registration = toml::from_str(
                    &std::fs::read_to_string(identity).context("reading identity file")?,
                )
                .context("parsing identity")?;
                add_identity(&config, identity, &output)?;
                println!("new candidate written; no running service changed");
            }
            Command::Service {
                command: ServiceCommand::Status,
            } => {
                // Presence is not authentication, liveness or a readiness probe.
                for identity in &config.identities {
                    let path = config
                        .socket_directory
                        .join(format!("{}.sock", identity.name));
                    println!(
                        "{}",
                        serde_json::json!({"identity":identity.name,"socket_present":path.exists(),"enabled":config.enabled,"liveness":"not-probed"})
                    );
                }
            }
            Command::Service {
                command: ServiceCommand::Run,
            } => run_service(config).await?,
        }
        Ok(())
    }
    async fn run_service(config: ServiceConfig) -> Result<()> {
        ensure!(
            config.enabled,
            "post service disabled; set enabled=true explicitly"
        );
        let parent = config.database.parent().context("database has no parent")?;
        private_directory(parent)?;
        let store = Store::open(&config.database, config.policy).await?;
        let server = Server::bind(
            store.clone(),
            &config.socket_directory,
            &config.identities,
            config.limits,
        )
        .await?;
        println!(
            "{}",
            serde_json::json!({"ready":true,"identities":config.identities.len(),"reply_delivery":"outbox-only"})
        );
        // Foreground: Ctrl-C or SIGTERM; no supervisor or daemon dependency.
        let mut terminate =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
        tokio::select! {_=tokio::signal::ctrl_c()=>{},_=terminate.recv()=>{}}
        server.shutdown().await?;
        store.close().await;
        Ok(())
    }
}
#[cfg(unix)]
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    cli::run().await
}
#[cfg(not(unix))]
fn main() -> anyhow::Result<()> {
    anyhow::bail!("post service requires Unix domain sockets")
}
