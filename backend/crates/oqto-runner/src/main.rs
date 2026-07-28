use anyhow::Result;
// Used only by the non-Linux guards below, which are compiled out on Linux.
#[cfg(not(target_os = "linux"))]
use anyhow::bail;
use clap::Parser;
use log::info;
#[cfg(not(target_os = "linux"))]
use log::warn;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use oqto_runner::daemon::bootstrap::{
    get_default_socket_path, load_env_file, load_sandbox_config, log_sandbox_state,
};
use oqto_runner::daemon::config::RunnerUserConfig;
use oqto_runner::daemon::server::{Runner, SessionBinaries};
use oqto_runner::pi_manager::{PiManagerConfig, PiSessionManager};

#[derive(Parser, Debug)]
#[command(
    name = "oqto-runner",
    about = "Process runner daemon for multi-user isolation"
)]
struct Args {
    #[arg(short, long)]
    config: Option<PathBuf>,
    #[arg(short, long, conflicts_with = "listen_tls")]
    socket: Option<PathBuf>,
    /// Listen on TCP with mandatory mutual TLS instead of a Unix socket.
    #[arg(long, value_name = "ADDRESS")]
    listen_tls: Option<SocketAddr>,
    #[arg(long, requires = "listen_tls")]
    tls_cert: Option<PathBuf>,
    #[arg(long, requires = "listen_tls")]
    tls_key: Option<PathBuf>,
    #[arg(long, requires = "listen_tls")]
    tls_client_ca: Option<PathBuf>,
    #[arg(long)]
    sandbox_config: Option<PathBuf>,
    #[arg(long)]
    no_sandbox: bool,
    #[arg(short, long)]
    verbose: bool,
    #[arg(long)]
    fileserver_binary: Option<String>,
    #[arg(long)]
    ttyd_binary: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    let log_level = if args.verbose { "debug" } else { "info" };
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or(log_level)).init();

    let socket_path = args.socket.clone().unwrap_or_else(get_default_socket_path);

    info!(
        "Starting oqto-runner (user={}, endpoint={})",
        std::env::var("USER").unwrap_or_else(|_| "unknown".to_string()),
        args.listen_tls.map_or_else(
            || format!("unix:{}", socket_path.display()),
            |address| format!("tcp+mtls://{address}")
        )
    );

    load_env_file();

    let user_config = args
        .config
        .map(RunnerUserConfig::load_from_path)
        .unwrap_or_else(RunnerUserConfig::load);

    let allow_user_sandbox_fallback = user_config.single_user && !user_config.linux_users_enabled;

    #[cfg(not(target_os = "linux"))]
    if !args.no_sandbox && !allow_user_sandbox_fallback {
        bail!(
            "Sandbox v2 hardened runner mode requires Linux. \
             Non-Linux platforms are supported only for single-user/dev fallback mode."
        );
    }

    let sandbox_config = load_sandbox_config(
        args.no_sandbox,
        args.sandbox_config.as_ref(),
        allow_user_sandbox_fallback,
    )?;
    log_sandbox_state(&sandbox_config);

    #[cfg(not(target_os = "linux"))]
    if sandbox_config.is_some() {
        warn!(
            "Sandbox enabled on non-Linux platform: running reduced-security mode \
             (no bwrap/seccomp/landlock/cgroups parity)."
        );
    }

    info!(
        "User config: workspace_dir={:?}, pi_sessions={:?}, memories={:?}, single_user={}, linux_users_enabled={}",
        user_config.workspace_dir,
        user_config.pi_sessions_dir,
        user_config.memories_dir,
        user_config.single_user,
        user_config.linux_users_enabled
    );

    let binaries = SessionBinaries {
        fileserver: args
            .fileserver_binary
            .unwrap_or(user_config.fileserver_binary.clone()),
        ttyd: args.ttyd_binary.unwrap_or(user_config.ttyd_binary.clone()),
    };

    let state_dir = std::env::var("XDG_STATE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
            PathBuf::from(home).join(".local").join("state")
        });
    let pi_config = PiManagerConfig {
        pi_binary: PathBuf::from(&user_config.pi_binary),
        default_cwd: user_config.workspace_dir.clone(),
        idle_timeout_secs: 300,
        cleanup_interval_secs: 60,
        sandbox_config: sandbox_config.clone(),
        runner_id: user_config.runner_id.clone(),
        model_cache_dir: Some(state_dir.join("oqto").join("model-cache")),
    };
    let pi_manager = PiSessionManager::new(pi_config);

    let pi_manager_cleanup = Arc::clone(&pi_manager);
    tokio::spawn(async move {
        pi_manager_cleanup.cleanup_loop().await;
    });

    let legacy_user_config = oqto_runner::daemon::config::RunnerUserConfig {
        fileserver_binary: user_config.fileserver_binary.clone(),
        ttyd_binary: user_config.ttyd_binary.clone(),
        pi_binary: user_config.pi_binary.clone(),
        runner_id: user_config.runner_id.clone(),
        workspace_dir: user_config.workspace_dir.clone(),
        pi_sessions_dir: user_config.pi_sessions_dir.clone(),
        memories_dir: user_config.memories_dir.clone(),
        single_user: user_config.single_user,
        linux_users_enabled: user_config.linux_users_enabled,
        terminal_enabled: user_config.terminal_enabled,
    };
    let runner = Runner::new(sandbox_config, binaries, legacy_user_config, pi_manager);
    if let Some(address) = args.listen_tls {
        let certificate = args
            .tls_cert
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("--tls-cert is required with --listen-tls"))?;
        let key = args
            .tls_key
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("--tls-key is required with --listen-tls"))?;
        let client_ca = args
            .tls_client_ca
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("--tls-client-ca is required with --listen-tls"))?;
        let config = oqto_runner::tls::server_config(client_ca, certificate, key)?;
        let listener = oqto_runner::tls::TcpTlsRunnerListener::bind(address, config).await?;
        runner.run_transport(&listener).await
    } else {
        runner.run(&socket_path).await
    }
}
