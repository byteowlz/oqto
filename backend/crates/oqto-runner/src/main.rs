use anyhow::{Result, ensure};
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
    ensure_control_socket_isolated, get_default_socket_path, inherited_unix_listener,
    load_env_file, load_sandbox_config, log_sandbox_state,
};
use oqto_runner::daemon::config::RunnerUserConfig;
use oqto_runner::daemon::scoped_files::ScopedFiles;
use oqto_runner::daemon::server::{ConnectionAccess, Runner, SessionBinaries};
use oqto_runner::pi_manager::{PiManagerConfig, PiSessionManager};

#[derive(Parser, Debug)]
#[command(
    name = "oqto-runner",
    about = "Process runner daemon for multi-user isolation"
)]
struct Args {
    #[command(subcommand)]
    command: Option<MaintenanceCommand>,
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
    /// Grant one canonical absolute Files root on the mTLS endpoint. Repeat
    /// for additional roots. This is supervisor policy, never agent-editable
    /// config.toml; omit to keep the network endpoint inventory-only.
    #[arg(
        long = "remote-file-root",
        requires = "listen_tls",
        value_name = "ABSOLUTE_PATH"
    )]
    remote_file_roots: Vec<PathBuf>,
    /// Explicit personal Pi access as this runner's full OS principal. The
    /// operator must also grant '/' to Files; narrower process roots are NOT
    /// inferred from cwd and remain fail-closed.
    #[arg(long, requires = "listen_tls")]
    remote_full_principal_pi: bool,
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
    /// Endpoint bridges (NAME=PORT): listen on 127.0.0.1:PORT and forward to
    /// the bind-mounted Unix socket /run/oqto/endpoints/NAME.sock.
    #[arg(long = "endpoint", value_name = "NAME=PORT")]
    endpoints: Vec<String>,
    /// Directory shared with the host for exposed-port sockets. Set in
    /// container placements; when unset, ExposePort returns the loopback
    /// address directly (host placements share loopback with the backend).
    #[arg(long, value_name = "DIR")]
    expose_dir: Option<PathBuf>,
}

#[derive(clap::Subcommand, Debug)]
enum MaintenanceCommand {
    /// Preview native Pi history import; --apply updates only canonical oqto-log state.
    /// JSON is emitted on stdout; failures exit nonzero. Back up canonical stores first.
    HistoryImport {
        #[arg(long)]
        apply: bool,
        /// Bound projection attempts for diagnosis or gradual import.
        #[arg(long, value_parser = clap::value_parser!(u32).range(1..))]
        max_imports: Option<u32>,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    if let Some(MaintenanceCommand::HistoryImport { apply, max_imports }) = args.command {
        use std::io::Write;
        let config = args
            .config
            .clone()
            .map(RunnerUserConfig::load_from_path)
            .unwrap_or_else(RunnerUserConfig::load);
        let report = oqto_runner::history_import::run(&config, apply, max_imports.map(|n| n as usize)).await.unwrap_or_else(|_| serde_json::json!({"schema":1,"ok":false,"error":"History import unavailable; check explicit history configuration and source access"}));
        let output = format!("{report}\n");
        if let Err(error) = std::io::stdout().write_all(output.as_bytes()) {
            if error.kind() == std::io::ErrorKind::BrokenPipe {
                return Ok(());
            }
            return Err(error.into());
        }
        anyhow::ensure!(
            report["ok"] == true,
            "History import failed; see the JSON summary"
        );
        return Ok(());
    }

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

    // Endpoint bridges fail closed: a granted endpoint that cannot be parsed
    // aborts startup rather than starting a workspace missing its services.
    let endpoint_socket_dir = PathBuf::from("/run/oqto/endpoints");
    for endpoint in &args.endpoints {
        let spec = oqto_runner::endpoint_bridge::EndpointBridgeSpec::parse(
            endpoint,
            &endpoint_socket_dir,
        )?;
        tokio::spawn(async move {
            if let Err(error) = oqto_runner::endpoint_bridge::serve(spec).await {
                log::error!("endpoint bridge failed: {error:#}");
            }
        });
    }

    let user_config = args
        .config
        .clone()
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

    let mut sandbox_config = load_sandbox_config(
        args.no_sandbox,
        args.sandbox_config.as_ref(),
        allow_user_sandbox_fallback,
    )?;
    // Every sandboxed child, including provider-auth workers, must be unable
    // to read or mutate runner transport capabilities.
    if let Some(policy) = sandbox_config.as_mut() {
        for path in std::iter::once(&socket_path)
            .chain(args.tls_key.iter())
            .chain(args.tls_client_ca.iter())
        {
            let path = path.to_string_lossy().into_owned();
            if !policy.deny_read.contains(&path) {
                policy.deny_read.push(path.clone());
            }
            if !policy.deny_write.contains(&path) {
                policy.deny_write.push(path);
            }
        }
    }
    log_sandbox_state(&sandbox_config);

    // The control socket is an unauthenticated-capability boundary into the
    // runner. If the loaded sandbox policy leaves it visible to workspaces,
    // refuse to start rather than shipping a reachable control plane.
    ensure_control_socket_isolated(&socket_path, sandbox_config.as_ref())?;

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
        extra_deny_read: {
            let mut denied = vec![socket_path.clone()];
            if let Some(key) = args.tls_key.as_ref() {
                denied.push(key.clone());
            }
            if let Some(ca) = args.tls_client_ca.as_ref() {
                denied.push(ca.clone());
            }
            denied
        },
    };
    let pi_manager = PiSessionManager::new(pi_config);

    let pi_manager_cleanup = Arc::clone(&pi_manager);
    tokio::spawn(async move {
        pi_manager_cleanup.cleanup_loop().await;
    });

    let legacy_user_config = oqto_runner::daemon::config::RunnerUserConfig {
        provider_login: user_config.provider_login.clone(),
        history_read: user_config.history_read.clone(),
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
    validate_remote_pi_grant(args.remote_full_principal_pi, &args.remote_file_roots)?;
    let remote_access = if args.remote_file_roots.is_empty() {
        ConnectionAccess::RemoteInventory
    } else {
        ensure!(
            legacy_user_config.single_user && !legacy_user_config.linux_users_enabled,
            "network Files roots require a dedicated single-user runner"
        );
        info!(
            "Enforcing runner-owned network Files roots: {:?}",
            args.remote_file_roots
        );
        if let Some(home) = dirs::home_dir()
            && args
                .remote_file_roots
                .iter()
                .any(|root| home.starts_with(root))
        {
            log::warn!(
                "A configured runner Files root contains the entire home directory, including credentials; use narrower roots to exclude secrets"
            );
        }
        let files = Arc::new(ScopedFiles::new(&args.remote_file_roots)?);
        if args.remote_full_principal_pi {
            log::warn!(
                "Personal network Pi execution enabled with full OS-principal authority; any mTLS client accepted by this listener can control Pi Sessions and run PiBash. Sandbox policy, if present, still applies to Pi children."
            );
            ConnectionAccess::RemotePersonalPi(files)
        } else {
            ConnectionAccess::RemoteFiles(files)
        }
    };
    let runner = Runner::new(
        sandbox_config,
        binaries,
        legacy_user_config,
        pi_manager,
        args.expose_dir.clone(),
    );
    if let Some(inherited) = inherited_unix_listener()? {
        if args.listen_tls.is_some() {
            anyhow::bail!("socket activation (LISTEN_FDS) conflicts with --listen-tls");
        }
        info!("Using socket-activated listener (LISTEN_FDS)");
        return runner.run_inherited(inherited).await;
    }
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
        runner.run_transport(&listener, remote_access).await
    } else {
        runner.run(&socket_path).await
    }
}

fn validate_remote_pi_grant(full_principal: bool, roots: &[PathBuf]) -> Result<()> {
    ensure!(
        !full_principal || roots.iter().any(|root| root == std::path::Path::new("/")),
        "personal Pi execution requires an explicit --remote-file-root /; narrow-root process confinement is not implemented"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_editable_config_cannot_mint_network_files_roots() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let config = temp.path().join("config.toml");
        std::fs::write(
            &config,
            "[local]\nsingle_user = true\n[runner]\nremote_roots = ['/']",
        )?;
        let args = Args::try_parse_from(vec![
            "oqto-runner".to_string(),
            "--listen-tls".to_string(),
            "127.0.0.1:39444".to_string(),
            "--config".to_string(),
            config.to_string_lossy().into_owned(),
        ])?;
        assert!(RunnerUserConfig::load_from_path(config).single_user);
        assert!(args.remote_file_roots.is_empty());
        Ok(())
    }

    #[test]
    fn network_files_roots_require_explicit_supervisor_arguments() -> Result<()> {
        let inventory = Args::try_parse_from(["oqto-runner", "--listen-tls", "127.0.0.1:39444"])?;
        assert!(inventory.remote_file_roots.is_empty());
        assert!(!inventory.remote_full_principal_pi);
        assert!(
            Args::try_parse_from(["oqto-runner", "--remote-file-root", "/home/alice/projects",])
                .is_err()
        );
        let scoped = Args::try_parse_from([
            "oqto-runner",
            "--listen-tls",
            "127.0.0.1:39444",
            "--remote-file-root",
            "/home/alice/projects",
            "--remote-file-root",
            "/home/alice/shared",
        ])?;
        assert_eq!(
            scoped.remote_file_roots,
            vec![
                PathBuf::from("/home/alice/projects"),
                PathBuf::from("/home/alice/shared"),
            ]
        );
        assert!(validate_remote_pi_grant(true, &scoped.remote_file_roots).is_err());
        assert!(validate_remote_pi_grant(false, &scoped.remote_file_roots).is_ok());
        let personal = Args::try_parse_from([
            "oqto-runner",
            "--listen-tls",
            "127.0.0.1:39444",
            "--remote-file-root",
            "/",
            "--remote-full-principal-pi",
        ])?;
        assert!(personal.remote_full_principal_pi);
        assert!(validate_remote_pi_grant(true, &personal.remote_file_roots).is_ok());
        Ok(())
    }
}
