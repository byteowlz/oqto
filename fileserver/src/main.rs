use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use axum::Router;
use clap::Parser;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod config;
mod error;
mod handlers;
mod routes;

use config::Config;

/// Application state shared across handlers
#[derive(Clone)]
pub struct AppState {
    /// Root directory to serve files from
    pub root_dir: PathBuf,
    /// Configuration
    pub config: Arc<Config>,
}

#[derive(Parser, Debug)]
#[command(name = "fileserver")]
#[command(about = "Lightweight file server for container workspace access")]
#[command(version)]
struct Cli {
    /// Port to listen on
    #[arg(short, long, env = "FILESERVER_PORT", default_value = "41821")]
    port: u16,

    /// Address to bind to
    #[arg(short, long, env = "FILESERVER_BIND", default_value = "0.0.0.0")]
    bind: String,

    /// Root directory to serve files from
    #[arg(short, long, env = "FILESERVER_ROOT", default_value = ".")]
    root: PathBuf,

    /// Enable verbose logging
    #[arg(short, long, env = "FILESERVER_VERBOSE")]
    verbose: bool,

    /// Config file path (optional)
    #[arg(short, long, env = "FILESERVER_CONFIG")]
    config: Option<PathBuf>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    // Initialize tracing
    let filter = if cli.verbose {
        "fileserver=debug,tower_http=debug"
    } else {
        "fileserver=info,tower_http=info"
    };

    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| filter.into()))
        .with(tracing_subscriber::fmt::layer())
        .init();

    // Load config from file if provided, otherwise use defaults
    let config = if let Some(config_path) = &cli.config {
        Config::from_file(config_path)?
    } else {
        Config::default()
    };

    // Resolve root directory to absolute path
    let root_dir = cli.root.canonicalize().unwrap_or_else(|_| cli.root.clone());

    if !root_dir.exists() {
        return Err(format!("Root directory does not exist: {}", root_dir.display()).into());
    }

    if !root_dir.is_dir() {
        return Err(format!("Root path is not a directory: {}", root_dir.display()).into());
    }

    info!("Serving files from: {}", root_dir.display());

    let state = AppState {
        root_dir,
        config: Arc::new(config),
    };

    // Build CORS layer
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    // Build router
    let app = Router::new()
        .merge(routes::file_routes())
        .layer(cors)
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    // Start server
    let addr: SocketAddr = format!("{}:{}", cli.bind, cli.port).parse()?;
    info!("Starting fileserver on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
