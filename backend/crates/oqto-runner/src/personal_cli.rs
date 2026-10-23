//! Owner-facing Runner flags and local administration. No database/key access
//! in the client: all commands go to the single live authority's private socket.
use anyhow::{Context, Result};
use clap::{Args, Subcommand, ValueEnum};
use oqto_personal::{
    SessionAccess,
    owner::{OwnerCommand, SelectedRoot},
};
use std::{net::SocketAddr, path::PathBuf};

#[derive(Clone, Copy, Debug, Default, ValueEnum)]
pub enum Sessions {
    #[default]
    None,
    Observe,
    Control,
}
impl From<Sessions> for SessionAccess {
    fn from(value: Sessions) -> Self {
        match value {
            Sessions::None => Self::None,
            Sessions::Observe => Self::Observe,
            Sessions::Control => Self::Control,
        }
    }
}
#[derive(Clone, Copy, Debug, Default, ValueEnum)]
pub enum Output {
    #[default]
    Terminal,
    /// Copyable full pairing payload on stdout; readable expiry hints on stderr.
    Code,
    Json,
    Svg,
}
#[derive(Clone, Debug, Default, Args)]
pub struct GrantSelection {
    /// Observe or control designated Sessions. Does not grant Files or a shell.
    #[arg(long, value_enum, default_value = "none")]
    pub sessions: Sessions,
    #[arg(long = "read-root", value_name = "CANONICAL_ABSOLUTE_DIRECTORY")]
    pub read_roots: Vec<PathBuf>,
    #[arg(long = "write-root", value_name = "CANONICAL_ABSOLUTE_DIRECTORY")]
    pub write_roots: Vec<PathBuf>,
    /// Full OS-user shell execution, NOT confined by the selected Files roots.
    #[arg(long)]
    pub full_user_terminal: bool,
}
impl GrantSelection {
    pub fn command(&self) -> OwnerCommand {
        let files = self
            .read_roots
            .iter()
            .map(|path| SelectedRoot {
                path: path.clone(),
                writable: false,
            })
            .chain(self.write_roots.iter().map(|path| SelectedRoot {
                path: path.clone(),
                writable: true,
            }))
            .collect();
        OwnerCommand::Pair {
            sessions: self.sessions.into(),
            files,
            full_user_terminal: self.full_user_terminal,
        }
    }
}
#[derive(Debug, Default, Args)]
pub struct ServerOptions {
    /// Personal HTTPS: standalone, or cohost with an explicit --socket/--listen-tls.
    #[arg(long, requires = "personal_origin")]
    pub personal_listen: Option<SocketAddr>,
    /// Canonical root HTTPS URL reachable from the phone, e.g. https://host.local:39444.
    #[arg(long, requires = "personal_listen")]
    pub personal_origin: Option<String>,
    #[arg(long, requires = "personal_listen")]
    pub personal_state: Option<PathBuf>,
    /// Explicit canonical Workspace roots for stored/live Session admission.
    #[arg(long = "personal-workspace-root", requires = "personal_listen")]
    pub workspace_roots: Vec<PathBuf>,
    /// Additional native JSONL source; canonical history remains in service HOME.
    #[arg(long, requires = "personal_listen")]
    pub personal_native_session_root: Option<PathBuf>,
    /// Trusted terminal Pi socket directory, independent of service TMPDIR.
    #[arg(long, requires = "personal_native_session_root")]
    pub personal_native_socket_root: Option<PathBuf>,
    /// Start without issuing a ticket; use `personal pair` later.
    #[arg(long, requires = "personal_listen")]
    pub personal_no_pair: bool,
    #[arg(long = "pair-output", value_enum, requires = "personal_listen")]
    pub output: Option<Output>,
    #[command(flatten)]
    pub grants: GrantSelection,
}
#[derive(Debug, Subcommand)]
pub enum OwnerCli {
    /// Issue one owner-selected five-minute QR ticket through the running host.
    Pair {
        #[arg(long)]
        state: Option<PathBuf>,
        #[command(flatten)]
        grants: GrantSelection,
        #[arg(long, value_enum, default_value = "terminal")]
        output: Output,
    },
    /// List enrolled Devices (64 per page); no ticket/key material is returned.
    Devices {
        #[arg(long)]
        state: Option<PathBuf>,
        #[arg(long)]
        after: Option<String>,
    },
    /// Revoke access without killing terminal-owned work.
    Revoke {
        #[arg(long)]
        state: Option<PathBuf>,
        device_id: String,
    },
    /// Re-pin native Sessions whose pin stopped matching only because macOS gave the
    /// volume a new device number (for example after a reboot). Reports counts; writes
    /// only with --apply. Safe while the Runner runs: each pin is compare-and-swapped.
    RepairNativePins {
        /// The Runner service HOME holding the canonical history and native-source binding.
        #[arg(long)]
        home: PathBuf,
        #[arg(long)]
        apply: bool,
    },
}
pub fn state_path(selected: Option<&PathBuf>) -> Result<PathBuf> {
    selected
        .cloned()
        .or_else(|| dirs::home_dir().map(|home| home.join(".local/share/oqto/personal")))
        .context("No home directory; specify personal state explicitly")
}
pub fn emit_pair(value: serde_json::Value, output: Output) -> Result<()> {
    use std::io::Write;
    let json = serde_json::to_vec(&value)?;
    let now = i64::try_from(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs(),
    )?;
    let qr = oqto_personal_wire::PairingQr::parse(&json, now)?;
    let text = match output {
        Output::Json => String::from_utf8(qr.to_json()?)?,
        Output::Code => {
            writeln!(
                std::io::stderr().lock(),
                "Copy the complete pairing payload. Keep it private.\n{}\nSingle use. Pairing is not a tool approval.",
                human_expiry(qr.expires_at(), now)?
            )?;
            String::from_utf8(qr.to_json()?)?
        }
        Output::Svg => qr.render_svg()?,
        Output::Terminal => format!(
            "Personal host: {}\n{}\n{}\nSingle use. Pairing is not a tool approval.\n",
            qr.origin(),
            qr.render_terminal()?,
            human_expiry(qr.expires_at(), now)?
        ),
    };
    std::io::stdout()
        .lock()
        .write_all(format!("{text}\n").as_bytes())?;
    Ok(())
}
fn human_expiry(expires: i64, now: i64) -> Result<String> {
    use chrono::TimeZone;
    let local = chrono::Local
        .timestamp_opt(expires, 0)
        .single()
        .context("pairing expiry is outside supported local time")?;
    let remaining = expires.saturating_sub(now).max(0);
    Ok(format!(
        "Expires: {} ({}m {:02}s remaining).",
        local.format("%Y-%m-%d %H:%M:%S %:z"),
        remaining / 60,
        remaining % 60
    ))
}

pub async fn owner(command: OwnerCli) -> Result<()> {
    if let OwnerCli::RepairNativePins { home, apply } = command {
        let home = home
            .canonicalize()
            .context("Runner service HOME unavailable")?;
        let source = crate::personal_source::binding::load(&home)?
            .context("No native-source binding in this HOME")?;
        let report = source.repair_pins(apply).await?;
        println!(
            "{}",
            serde_json::to_string(&serde_json::json!({"applied": apply, "report": report}))?
        );
        return Ok(());
    }
    let (state, command, output) = match command {
        OwnerCli::Pair {
            state,
            grants,
            output,
        } => (state, grants.command(), Some(output)),
        OwnerCli::Devices { state, after } => (state, OwnerCommand::Devices { after }, None),
        OwnerCli::Revoke { state, device_id } => (state, OwnerCommand::Revoke { device_id }, None),
        OwnerCli::RepairNativePins { .. } => unreachable!("handled above"),
    };
    let value = oqto_personal::owner::request_owner(&state_path(state.as_ref())?, &command).await?;
    if let Some(output) = output {
        emit_pair(value, output)
    } else {
        use std::io::Write;
        std::io::stdout()
            .lock()
            .write_all(format!("{}\n", serde_json::to_string(&value)?).as_bytes())?;
        Ok(())
    }
}
