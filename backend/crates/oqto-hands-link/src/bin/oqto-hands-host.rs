//! Standalone foreground hands host (ADR-0086, destination side). No
//! daemonizing, service unit or autostart: run it in a terminal or a
//! supervisor of your choosing and stop it with Ctrl-C.
use anyhow::{Context, Result, ensure};
use clap::{Parser, Subcommand};
use oqto_hands_link::{
    clock::SuspendAwareClock,
    config::{HostConfig, host_schema},
    host::HandsHost,
    journal::Journal,
    launcher::OqtHandsLauncher,
    server::{self, HostOwnerCommand},
};
use oqto_personal::PairingAuthority;
use std::{
    io::Write,
    os::unix::fs::{DirBuilderExt, OpenOptionsExt},
    path::PathBuf,
    sync::Arc,
};

#[derive(Parser)]
#[command(
    name = "oqto-hands-host",
    about = "Foreground destination for paired remote hands"
)]
struct Cli {
    /// Config file (else OQTO_HANDS_HOST_CONFIG, else $XDG_CONFIG_HOME/oqto/hands-host.toml).
    #[arg(long, global = true)]
    config: Option<PathBuf>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Serve in the foreground until interrupted.
    Serve,
    /// Validate the config and print the effective values.
    CheckConfig,
    /// Print the config JSON schema.
    Schema,
    /// Issue a one-use pairing QR (no Sessions, Files or terminal) into a 0600 file.
    Pair {
        #[arg(long)]
        out: PathBuf,
    },
    Devices,
    /// Install a standing hands delegation of one binding to one device.
    Delegate {
        #[arg(long)]
        device: String,
        #[arg(long)]
        binding: String,
    },
    Undelegate {
        #[arg(long)]
        device: String,
        #[arg(long)]
        binding: String,
    },
    /// Revoke a paired device and stop its hands.
    Revoke {
        #[arg(long)]
        device: String,
    },
    Status,
    Recent {
        #[arg(long, default_value_t = 16)]
        limit: u32,
    },
    /// Unfreeze a binding after inspecting an unconfirmed stop.
    ClearAttention {
        #[arg(long)]
        binding: String,
    },
}

fn config_path(cli: Option<PathBuf>) -> Result<PathBuf> {
    if let Some(path) = cli {
        return Ok(path);
    }
    if let Some(path) = std::env::var_os("OQTO_HANDS_HOST_CONFIG") {
        return Ok(PathBuf::from(path));
    }
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .context("no config location")?;
    Ok(base.join("oqto").join("hands-host.toml"))
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .target(env_logger::Target::Stderr)
        .init();
    let cli = Cli::parse();
    if let Command::Schema = cli.command {
        println!("{}", host_schema()?);
        return Ok(());
    }
    let path = config_path(cli.config)?;
    let config = HostConfig::load(&path)?;
    let owner = server::owner_socket(&config);
    let command = match cli.command {
        Command::Serve => return serve(config).await,
        Command::Schema => return Ok(()),
        Command::CheckConfig => {
            println!("{}", serde_json::to_string_pretty(&config)?);
            return Ok(());
        }
        Command::Pair { out } => {
            let qr = server::request_owner(&owner, &HostOwnerCommand::Pair).await?;
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&out)
                .with_context(|| format!("creating {}", out.display()))?;
            file.write_all(serde_json::to_string(&qr)?.as_bytes())?;
            eprintln!(
                "one-use pairing QR written to {} (valid 5 minutes; it is a secret)",
                out.display()
            );
            return Ok(());
        }
        Command::Devices => HostOwnerCommand::Devices,
        Command::Delegate { device, binding } => HostOwnerCommand::Delegate {
            device_id: device,
            binding_id: binding,
        },
        Command::Undelegate { device, binding } => HostOwnerCommand::Undelegate {
            device_id: device,
            binding_id: binding,
        },
        Command::Revoke { device } => HostOwnerCommand::Revoke { device_id: device },
        Command::Status => HostOwnerCommand::Status,
        Command::Recent { limit } => HostOwnerCommand::Recent { limit },
        Command::ClearAttention { binding } => HostOwnerCommand::ClearAttention {
            binding_id: binding,
        },
    };
    let reply = server::request_owner(&owner, &command).await?;
    println!("{}", serde_json::to_string_pretty(&reply)?);
    Ok(())
}

async fn serve(config: HostConfig) -> Result<()> {
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(&config.state_dir)?;
    let state_dir = config.state_dir.canonicalize()?;
    let authority = Arc::new(PairingAuthority::open_private(&state_dir.join("personal")).await?);
    let (journal, recovery) = Journal::open(&state_dir.join("hands.sqlite3")).await?;
    if !recovery.unknown.is_empty() {
        log::warn!(
            "{} claims from a previous host are now unknown (never replayed)",
            recovery.unknown.len()
        );
    }
    let launcher = OqtHandsLauncher::new(config.launcher.clone(), &state_dir)?;
    let leftovers = launcher.reap_leftovers().await?;
    if !leftovers.is_empty() {
        for binding in config.bindings.keys() {
            journal
                .set_attention(binding, "previous host left unconfirmed generations")
                .await?;
        }
        log::warn!(
            "{} unconfirmed leftover generations; bindings frozen",
            leftovers.len()
        );
    }
    let host = HandsHost::new(
        &config,
        Arc::clone(&authority),
        journal,
        Arc::new(launcher),
        Arc::new(SuspendAwareClock),
    )?;
    let identity = authority.transport_identity().await?;
    let address: std::net::SocketAddr = config.listen.parse()?;
    ensure!(!address.ip().is_unspecified(), "wildcard listen refused");
    let listener = tokio::net::TcpListener::bind(address)
        .await
        .with_context(|| format!("binding {address}"))?;
    let owner = server::bind_owner(&server::owner_socket(&config))?;
    let enforcer = host.spawn_enforcer();
    log::info!(
        "hands host {} serving {} with bindings [{}]; owner socket {}",
        authority.host_id(),
        address,
        host.bindings.keys().cloned().collect::<Vec<_>>().join(", "),
        server::owner_socket(&config).display()
    );
    let shutdown = tokio_util::sync::CancellationToken::new();
    let signal = shutdown.clone();
    tokio::spawn(async move {
        let mut term =
            match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
                Ok(term) => term,
                Err(_) => return,
            };
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {},
            _ = term.recv() => {},
        }
        signal.cancel();
    });
    let tls = server::serve_tls(
        listener,
        identity.server_config()?,
        server::router(Arc::clone(&host)),
        shutdown.clone().cancelled_owned(),
    );
    let owner_task = server::serve_owner(
        owner,
        Arc::clone(&host),
        config.origin.clone(),
        identity.leaf_der().to_vec(),
        shutdown.clone().cancelled_owned(),
    );
    let (tls, owner_result) = tokio::join!(tls, owner_task);
    enforcer.abort();
    // Stop and confirm every generation before exiting.
    let stopped = host.shutdown().await;
    log::info!("stopped {} generations on shutdown", stopped);
    tls?;
    owner_result?;
    Ok(())
}
