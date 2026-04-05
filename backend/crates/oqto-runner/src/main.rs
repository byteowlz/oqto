use anyhow::{Result, bail};
use clap::Parser;
use log::{info, warn};
use std::path::PathBuf;
use std::sync::Arc;

use oqto::history::HstryEndpoint;
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
    #[arg(short, long)]
    socket: Option<PathBuf>,
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

    let socket_path = args.socket.unwrap_or_else(get_default_socket_path);

    info!(
        "Starting oqto-runner (user={}, socket={:?})",
        std::env::var("USER").unwrap_or_else(|_| "unknown".to_string()),
        socket_path
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
    // Resolve hstry endpoint: in single-user mode, read the port file written by
    // systemd/hstry-service and use an explicit Tcp endpoint. This avoids stale
    // port-file races by resolving the port once at startup.
    // In multi-user mode the runner would receive a per-user endpoint; for now
    // single-user runners always use Discover which probes socket then port file.
    let hstry_endpoint = {
        let socket_path = hstry_core::paths::service_socket_path();
        if socket_path.exists() {
            info!("hstry endpoint: Unix socket {:?}", socket_path);
            HstryEndpoint::UnixSocket(socket_path)
        } else {
            let port_path = hstry_core::paths::service_port_path();
            match std::fs::read_to_string(&port_path)
                .ok()
                .and_then(|s| s.trim().parse::<u16>().ok())
            {
                Some(port) => {
                    // Verify connectivity at startup so stale ports fail fast.
                    let addr = std::net::SocketAddr::new(
                        std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
                        port,
                    );
                    if std::net::TcpStream::connect_timeout(
                        &addr,
                        std::time::Duration::from_millis(500),
                    )
                    .is_ok()
                    {
                        info!("hstry endpoint: TCP port {} (verified reachable)", port);
                        HstryEndpoint::Tcp(port)
                    } else {
                        warn!(
                            "hstry port file says {} but endpoint unreachable; falling back to auto-discover",
                            port
                        );
                        HstryEndpoint::Discover
                    }
                }
                None => {
                    warn!("hstry port file not found; using auto-discover");
                    HstryEndpoint::Discover
                }
            }
        }
    };

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
        hstry_endpoint,
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
    runner.run(&socket_path).await
}
