//! One-command pairing. Terminal interaction is a view over the same explicit plan
//! used by flags. No authority/configuration mutation happens before confirmation.
mod runtime;
mod wizard;
use crate::{
    pair_control::ListenerPlan,
    personal_cli::{GrantSelection, Output, Sessions},
};
use anyhow::{Context, Result, ensure};
use clap::{Args, ValueEnum};
use std::{
    io::IsTerminal,
    path::{Path, PathBuf},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum Network {
    Tailscale,
    Lan,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum Access {
    Full,
    ReadOnly,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum Files {
    None,
    Read,
    Write,
}
#[derive(Debug, Args)]
pub struct PairOptions {
    /// Print non-secret controller status as JSON; never start or change a Runner.
    #[arg(long, conflicts_with_all = ["workspaces", "access", "network", "interactive", "non_interactive", "yes", "sessions", "files", "terminal", "port"])]
    pub status: bool,
    /// Explicitly migrate a stopped listener's legacy store marker, pinning its SHA256.
    /// No ticket, key rotation or database reset. A private backup is retained.
    #[arg(long, conflicts_with_all = ["status", "recover_listener", "workspaces", "access", "network", "interactive", "non_interactive", "yes", "sessions", "files", "terminal", "port"])]
    pub repair_owner_marker: Option<String>,
    /// Restart only the saved stopped listener, with exactly its existing scope. No ticket.
    #[arg(long, conflicts_with_all = ["status", "workspaces", "access", "network", "interactive", "non_interactive", "yes", "sessions", "files", "terminal", "port"])]
    pub recover_listener: bool,
    /// Select Workspaces; repeat for each directory.
    #[arg(long = "workspace")]
    pub workspaces: Vec<PathBuf>,
    /// Full = session control, writable Files, unrestricted OS-user terminal.
    #[arg(long, value_enum)]
    pub access: Option<Access>,
    #[arg(long, value_enum)]
    pub network: Option<Network>,
    /// Force the selection wizard. Requires an actual terminal.
    #[arg(long, conflicts_with = "non_interactive")]
    pub interactive: bool,
    /// Never prompt. Requires Workspaces, access, network and --yes.
    #[arg(long)]
    pub non_interactive: bool,
    /// Confirm exactly the selected scopes, including unrestricted shell if selected.
    #[arg(long, short = 'y', conflicts_with = "interactive")]
    pub yes: bool,
    #[arg(long, value_enum)]
    pub sessions: Option<Sessions>,
    #[arg(long, value_enum)]
    pub files: Option<Files>,
    #[arg(long,num_args=0..=1,default_missing_value="true")]
    pub terminal: Option<bool>,
    #[arg(long,default_value_t=39444,value_parser=clap::value_parser!(u16).range(1..))]
    pub port: u16,
    /// Explicit already-running Runner home (never an instruction to change HOME).
    #[arg(long)]
    pub runner_home: Option<PathBuf>,
    #[arg(long, value_enum, default_value = "terminal")]
    pub output: Output,
}
impl PairOptions {
    pub fn mode(&self, tty: bool) -> std::result::Result<bool, String> {
        if self.status || self.repair_owner_marker.is_some() || self.recover_listener {
            return Ok(false);
        }
        if self.interactive && !tty {
            return Err("--interactive requires terminal stdin and stderr".into());
        }
        let wizard = self.interactive
            || (tty
                && !self.non_interactive
                && (self.workspaces.is_empty() || self.access.is_none() || self.network.is_none()));
        if wizard && self.yes {
            return Err(
                "--yes requires complete flags; interactive choices need final confirmation".into(),
            );
        }
        if !wizard
            && (self.workspaces.is_empty() || self.access.is_none() || self.network.is_none())
        {
            return Err("pair requires --workspace, --access and --network; run in a terminal for the wizard".into());
        }
        if (!tty || self.non_interactive) && !self.yes {
            return Err(
                "headless pairing requires --yes to confirm the explicit access grant".into(),
            );
        }
        Ok(wizard)
    }
}
pub struct PairPlan {
    pub listener: ListenerPlan,
    pub grants: GrantSelection,
}
pub fn canonical_roots(paths: &[PathBuf], home: &Path) -> Result<Vec<PathBuf>> {
    ensure!(
        !paths.is_empty() && paths.len() <= 32,
        "select 1–32 Workspaces"
    );
    let mut roots = Vec::new();
    for input in paths {
        let expanded = if input == Path::new("~") {
            home.to_owned()
        } else if let Ok(rest) = input.strip_prefix("~/") {
            home.join(rest)
        } else {
            input.clone()
        };
        let root = expanded
            .canonicalize()
            .with_context(|| format!("Workspace does not exist: {}", input.display()))?;
        ensure!(root.is_dir(), "Workspace must be a directory");
        ensure!(
            !root.to_string_lossy().chars().any(char::is_control),
            "control characters in directory names are unsupported"
        );
        if !roots.contains(&root) {
            roots.push(root);
        }
    }
    roots.sort();
    Ok(roots)
}
pub fn grants(options: &PairOptions, roots: &[PathBuf]) -> GrantSelection {
    let full = options.access == Some(Access::Full);
    let sessions = options.sessions.unwrap_or(if full {
        Sessions::Control
    } else {
        Sessions::Observe
    });
    let files = options
        .files
        .unwrap_or(if full { Files::Write } else { Files::Read });
    GrantSelection {
        sessions,
        read_roots: if files == Files::Read {
            roots.to_vec()
        } else {
            vec![]
        },
        write_roots: if files == Files::Write {
            roots.to_vec()
        } else {
            vec![]
        },
        full_user_terminal: options.terminal.unwrap_or(full),
    }
}
/// Common CLI entry: both public `oqto pair` and low-level runner administration
/// use the same validation, interaction, grant gate and output contract.
pub async fn execute(options: PairOptions) -> Result<()> {
    let tty = std::io::stdin().is_terminal() && std::io::stderr().is_terminal();
    let output = if options.status {
        Output::Json
    } else {
        options.output
    };
    let wizard = options.mode(tty).unwrap_or_else(|error| {
        let _ = json_error(output, "usage", &error);
        clap::Error::raw(clap::error::ErrorKind::MissingRequiredArgument, error).exit()
    });
    let result = run(options, wizard).await;
    if let Err(error) = &result {
        if error.chain().any(|cause| {
            cause
                .downcast_ref::<std::io::Error>()
                .is_some_and(|e| e.kind() == std::io::ErrorKind::BrokenPipe)
        }) {
            return Ok(());
        }
        json_error(output, "pairing_failed", &error.to_string())?;
    }
    result
}
pub async fn run(mut options: PairOptions, wizard: bool) -> Result<()> {
    let home = dirs::home_dir()
        .context("home unavailable")?
        .canonicalize()?;
    if let Some(expected) = &options.repair_owner_marker {
        let status = runtime::status(&home, options.runner_home.as_deref()).await?;
        ensure!(
            !status.active && status.plan.is_some(),
            "repair requires the saved listener to be stopped"
        );
        let backup = oqto_personal::repair_store_owner(&status.state, expected)?;
        println!(
            "{}",
            serde_json::json!({"schema":1,"repaired":true,"backup":backup,"ticket_issued":false})
        );
        return Ok(());
    }
    if options.recover_listener {
        let before = runtime::status(&home, options.runner_home.as_deref()).await?;
        let plan = before.plan.context("no saved listener to recover")?;
        let status = runtime::enable(&home, options.runner_home.as_deref(), &plan).await?;
        println!("{}", serde_json::to_string(&status)?);
        return Ok(());
    }
    if options.status {
        use std::io::Write;
        let status = runtime::status(&home, options.runner_home.as_deref()).await?;
        writeln!(
            std::io::stdout().lock(),
            "{}",
            serde_json::to_string(&status)?
        )?;
        return Ok(());
    }
    if wizard && !wizard::select(&mut options, &home).await? {
        return cancelled(options.output);
    }
    let roots = canonical_roots(&options.workspaces, &home)?;
    let grants = grants(&options, &roots);
    let ip = runtime::address(options.network.context("network required")?).await?;
    let address = std::net::SocketAddr::new(ip, options.port);
    let listener = ListenerPlan {
        address,
        origin: format!("https://{address}/"),
        workspaces: roots,
        native_home: home.clone(),
    };
    listener.validate()?;
    let plan = PairPlan { listener, grants };
    summary(&plan);
    if !options.yes {
        ensure!(
            std::io::stdin().is_terminal() && std::io::stderr().is_terminal(),
            "confirmation requires a terminal or --yes"
        );
        if !dialoguer::Confirm::new()
            .with_prompt("Enable pairing and show QR?")
            .default(false)
            .interact()?
        {
            return cancelled(options.output);
        }
    }
    let status = runtime::enable(&home, options.runner_home.as_deref(), &plan.listener).await?;
    // Exactly one ticket request. Failure after dispatch is uncertain, never replayed.
    let qr = oqto_personal::owner::request_owner(&status.state, &plan.grants.command())
        .await
        .map_err(owner_issue_error)?;
    crate::personal_cli::emit_pair(qr, options.output)
}
fn owner_issue_error(error: anyhow::Error) -> anyhow::Error {
    if error
        .downcast_ref::<oqto_personal::owner::OwnerRequestRejected>()
        .is_some()
    {
        error
    } else {
        error.context("pairing outcome uncertain on transport failure; do not automatically replay ticket issuance")
    }
}
pub fn json_error(output: Output, code: &str, message: &str) -> Result<()> {
    if matches!(output, Output::Json) {
        use std::io::Write;
        writeln!(
            std::io::stdout().lock(),
            "{}",
            serde_json::json!({"ok":false,"error":{"code":code,"message":message}})
        )?;
    }
    Ok(())
}
fn cancelled(output: Output) -> Result<()> {
    if matches!(output, Output::Json) {
        use std::io::Write;
        writeln!(
            std::io::stdout().lock(),
            "{}",
            serde_json::json!({"ok":true,"cancelled":true})
        )?;
    }
    eprintln!("Pairing cancelled. No changes made.");
    Ok(())
}
fn summary(plan: &PairPlan) {
    eprintln!(
        "\nPhone pairing\n  Sessions: {:?}\n  Files: {}\n  Terminal: {}\n  HTTPS: {}",
        plan.grants.sessions,
        if !plan.grants.write_roots.is_empty() {
            "read/write"
        } else if !plan.grants.read_roots.is_empty() {
            "read-only"
        } else {
            "none"
        },
        if plan.grants.full_user_terminal {
            "full OS-user shell"
        } else {
            "none"
        },
        plan.listener.origin
    );
    for root in &plan.listener.workspaces {
        eprintln!("  Workspace: {}", root.display());
    }
    if plan.grants.full_user_terminal {
        eprintln!(
            "  WARNING: Terminal runs as your OS user and is NOT restricted to Workspace directories."
        );
    }
    eprintln!("  QR: single use, expires in five minutes. Keep it private.");
}

#[cfg(test)]
mod refusal_tests {
    #[test]
    fn preissuance_rejection_is_definitive_but_unknown_failure_is_not() {
        let rejected = super::owner_issue_error(
            oqto_personal::owner::OwnerRequestRejected::TerminalUnavailable.into(),
        );
        assert!(rejected.to_string().contains("no pairing ticket issued"));
        assert!(!rejected.to_string().contains("uncertain"));
        let unknown = super::owner_issue_error(anyhow::anyhow!("owner_command_refused"));
        assert!(unknown.to_string().contains("uncertain"));
    }
}
