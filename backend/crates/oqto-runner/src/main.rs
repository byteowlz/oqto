use anyhow::{Context, Result};
use clap::Parser;
use log::{info, warn};
use std::path::PathBuf;
use std::sync::Arc;

use oqto::runner::daemon::bootstrap::{
    get_default_socket_path, load_env_file, load_sandbox_config, log_sandbox_state,
};
use oqto::runner::daemon::config::RunnerUserConfig;
use oqto::runner::daemon::server::{Runner, SessionBinaries};
use oqto::runner::pi_manager::{PiManagerConfig, PiSessionManager};

#[derive(Parser, Debug)]
#[command(
    name = "oqto-runner",
    about = "Process runner daemon for multi-user isolation"
)]
struct Args {
    #[arg(short, long)]
    config: Option<PathBuf>,
    #[arg(short, long, conflicts_with = "listen")]
    socket: Option<PathBuf>,
    #[arg(long, value_name = "HOST:PORT", conflicts_with = "socket")]
    listen: Option<String>,
    #[arg(long, env = "RUNNER_AUTH_TOKEN")]
    auth_token: Option<String>,
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

    let sandbox_config = load_sandbox_config(args.no_sandbox, args.sandbox_config.as_ref())?;
    log_sandbox_state(&sandbox_config);

    load_env_file();

    let user_config = args
        .config
        .map(RunnerUserConfig::load_from_path)
        .unwrap_or_else(RunnerUserConfig::load);

    info!(
        "User config: workspace_dir={:?}, pi_sessions={:?}, memories={:?}",
        user_config.workspace_dir, user_config.pi_sessions_dir, user_config.memories_dir
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
        hstry_db_path: {
            let db_path = oqto::history::hstry_db_path();
            match &db_path {
                Some(p) => info!("hstry DB found: {}", p.display()),
                None => warn!("hstry DB not found -- chat history persistence disabled"),
            }
            db_path
        },
        sandbox_config: sandbox_config.clone(),
        runner_id: user_config.runner_id.clone(),
        model_cache_dir: Some(state_dir.join("oqto").join("model-cache")),
    };
    let pi_manager = PiSessionManager::new(pi_config);

    let pi_manager_cleanup = Arc::clone(&pi_manager);
    tokio::spawn(async move {
        pi_manager_cleanup.cleanup_loop().await;
    });

    let runner = Runner::new(sandbox_config, binaries, user_config, pi_manager);

    if let Some(listen_addr) = args.listen {
        let auth_token = args.auth_token.with_context(
            || "RUNNER_AUTH_TOKEN (or --auth-token) is required when --listen is used",
        )?;
        info!(
            "Starting oqto-runner (user={}, listen={})",
            std::env::var("USER").unwrap_or_else(|_| "unknown".to_string()),
            listen_addr
        );
        runner.run_tcp(&listen_addr, auth_token).await
    } else {
        let socket_path = args.socket.unwrap_or_else(get_default_socket_path);
        info!(
            "Starting oqto-runner (user={}, socket={:?})",
            std::env::var("USER").unwrap_or_else(|_| "unknown".to_string()),
            socket_path
        );
        runner.run(&socket_path).await
    }
}
