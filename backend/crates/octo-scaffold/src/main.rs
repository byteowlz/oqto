//! octo-scaffold - Project scaffolding tool for Octo workspaces.
//!
//! Creates new projects with appropriate configurations based on permission tiers.
//!
//! ## Usage
//!
//! ```bash
//! # Create a new project with default (normal) tier
//! octo-scaffold new my-project
//!
//! # Create with specific tier
//! octo-scaffold new my-project --tier=private
//! octo-scaffold new my-project --tier=normal
//! octo-scaffold new my-project --tier=privileged
//!
//! # Create from a template
//! octo-scaffold new my-project --template=rust-cli
//!
//! # Create with specific skills
//! octo-scaffold new my-project --skills=web-search,code-review
//!
//! # List available templates
//! octo-scaffold templates
//!
//! # List available tiers
//! octo-scaffold tiers
//! ```

use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::Result;
use clap::{Parser, Subcommand, ValueEnum};

mod generators;
mod templates;
mod tiers;

fn main() -> ExitCode {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into()),
        )
        .with_target(false)
        .init();

    if let Err(err) = run() {
        eprintln!("Error: {err:?}");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

fn run() -> Result<()> {
    let cli = Cli::parse();

    // Load scaffold configuration
    let scaffold_config = tiers::ScaffoldConfig::load()?;

    match cli.command {
        Command::New {
            name,
            tier,
            template,
            skills,
            output,
            no_git,
            no_trx,
        } => {
            let output_dir = output.unwrap_or_else(|| PathBuf::from(&name));

            let config = generators::ProjectConfig {
                name: name.clone(),
                tier: tier.unwrap_or(Tier::Normal),
                template,
                skills: skills.unwrap_or_default(),
                output_dir,
                init_git: !no_git,
                init_trx: !no_trx,
            };

            generators::create_project(&config, &scaffold_config)?;
        }
        Command::Templates { path } => {
            templates::list_templates(path.as_deref(), &scaffold_config)?;
        }
        Command::Tiers => {
            tiers::list_tiers(&scaffold_config);
        }
        Command::Config { example } => {
            if example {
                println!("{}", tiers::ScaffoldConfig::example_config());
            } else {
                show_config(&scaffold_config);
            }
        }
    }

    Ok(())
}

fn show_config(config: &tiers::ScaffoldConfig) {
    println!("Current scaffold configuration:\n");

    println!("Config file locations (in priority order):");
    println!("  1. OCTO_SCAFFOLD_CONFIG env var");
    println!("  2. ~/.config/octo/scaffold.toml");
    println!("  3. /etc/octo/scaffold.toml");
    println!("  4. Built-in defaults\n");

    if let Some(path) = config.templates_path() {
        println!("Templates path: {}", path.display());
        if path.exists() {
            println!("  (exists)");
        } else {
            println!("  (not found)");
        }
    } else {
        println!("Templates path: not configured");
    }

    println!("\nTiers: private, normal, privileged");
    println!("\nRun 'octo-scaffold config --example' to see full example config.");
}

#[derive(Parser, Debug)]
#[command(
    name = "octo-scaffold",
    author,
    version,
    about = "Project scaffolding tool for Octo workspaces",
    after_help = "Examples:\n  \
        octo-scaffold new my-project --tier=private\n  \
        octo-scaffold new my-project --template=rust-cli --skills=code-review\n  \
        octo-scaffold templates\n  \
        octo-scaffold tiers"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Create a new project
    New {
        /// Project name
        name: String,

        /// Permission tier (private, normal, privileged)
        #[arg(short, long)]
        tier: Option<Tier>,

        /// Template to use (from templates directory)
        #[arg(short = 'T', long)]
        template: Option<String>,

        /// Skills to enable (comma-separated)
        #[arg(short, long, value_delimiter = ',')]
        skills: Option<Vec<String>>,

        /// Output directory (defaults to project name)
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// Skip git initialization
        #[arg(long)]
        no_git: bool,

        /// Skip trx initialization
        #[arg(long)]
        no_trx: bool,
    },

    /// List available templates
    Templates {
        /// Path to templates directory (defaults to configured path)
        #[arg(short, long)]
        path: Option<PathBuf>,
    },

    /// List available permission tiers
    Tiers,

    /// Show or generate configuration
    Config {
        /// Print example configuration file
        #[arg(long)]
        example: bool,
    },
}

/// Permission tier for the project.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Tier {
    /// Restricted: local models only, network isolation, own directory only
    Private,
    /// Standard: cloud LLMs allowed, own directory only
    Normal,
    /// Full access: all models, workspace read/write
    Privileged,
}

impl std::fmt::Display for Tier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Tier::Private => write!(f, "private"),
            Tier::Normal => write!(f, "normal"),
            Tier::Privileged => write!(f, "privileged"),
        }
    }
}
