//! Guard-owned origin adapter for remote hands (ADR-0086). `serve` runs as a
//! foreground child of the oqt guard: requests arrive as JSON lines on the
//! inherited stdin, replies and events leave on stdout, and per-call pi-env
//! bytes cross a private Unix socket. The brain never reaches this process.
use anyhow::{Context, Result, ensure};
use clap::{Parser, Subcommand};
use oqto_hands_link::{
    client::{ClaimResult, LinkState, Links, MachineLink},
    config::{OriginConfig, origin_schema},
    wire::{Operation, is_hex, random_hex},
};
use serde::Deserialize;
use std::{
    collections::HashMap,
    os::unix::fs::PermissionsExt,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    net::UnixListener,
    sync::mpsc,
};

#[derive(Parser)]
#[command(name = "oqt_link", about = "oqt guard adapter for paired remote hands")]
struct Cli {
    /// Config (else OQT_HANDS_LINK_CONFIG, else $XDG_CONFIG_HOME/oqt/hands-link.toml).
    #[arg(long, global = true)]
    config: Option<PathBuf>,
    /// Private link state (else OQT_HANDS_LINK_STATE, else $XDG_STATE_HOME/oqt/hands-link).
    #[arg(long, global = true)]
    state: Option<PathBuf>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Pair this guard with a hands host using its one-use QR file.
    Pair {
        #[arg(long)]
        name: String,
        #[arg(long)]
        qr_file: PathBuf,
        #[arg(long, default_value = "oqt guard")]
        label: String,
    },
    /// Show configured machines, limits and pairings (no network).
    Status,
    /// Validate the config and print it.
    CheckConfig,
    /// Print the config JSON schema.
    Schema,
    /// Guard child mode.
    Serve {
        #[arg(long)]
        data_socket: PathBuf,
        /// Stable 32-hex controller namespace chosen by the guard.
        #[arg(long)]
        namespace: String,
    },
}

fn xdg(var: &str, fallback: &str) -> Result<PathBuf> {
    std::env::var_os(var)
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(fallback)))
        .context("no XDG location")
}

fn config_path(cli: Option<PathBuf>) -> Result<PathBuf> {
    match cli.or_else(|| std::env::var_os("OQT_HANDS_LINK_CONFIG").map(PathBuf::from)) {
        Some(path) => Ok(path),
        None => Ok(xdg("XDG_CONFIG_HOME", ".config")?.join("oqt/hands-link.toml")),
    }
}

fn state_path(cli: Option<PathBuf>) -> Result<PathBuf> {
    match cli.or_else(|| std::env::var_os("OQT_HANDS_LINK_STATE").map(PathBuf::from)) {
        Some(path) => Ok(path),
        None => Ok(xdg("XDG_STATE_HOME", ".local/state")?.join("oqt/hands-link")),
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn"))
        .target(env_logger::Target::Stderr)
        .init();
    let cli = Cli::parse();
    match cli.command {
        Command::Schema => {
            println!("{}", origin_schema()?);
            Ok(())
        }
        Command::CheckConfig => {
            let config = OriginConfig::load(&config_path(cli.config)?)?;
            println!("{}", serde_json::to_string_pretty(&config)?);
            Ok(())
        }
        Command::Pair {
            name,
            qr_file,
            label,
        } => {
            let state = LinkState::open(&state_path(cli.state)?)?;
            let qr = std::fs::read(&qr_file)?;
            let pairing = state.pair(&name, &qr, &label).await?;
            // The QR is one-use and already consumed; remove the secret copy.
            let _ = std::fs::remove_file(&qr_file);
            println!(
                "{}",
                serde_json::json!({"paired": name, "host_id": pairing.host_id, "device_id": pairing.device_id})
            );
            Ok(())
        }
        Command::Status => {
            let path = config_path(cli.config)?;
            let config = OriginConfig::load(&path)?;
            let state = LinkState::open(&state_path(cli.state)?)?;
            let machines: Vec<_> = config
                .machines
                .iter()
                .map(|(name, machine)| {
                    let pairing = state.pairing(&machine.pairing);
                    serde_json::json!({
                        "machine": name,
                        "binding": machine.binding,
                        "lease_ms": machine.limits.lease_ms,
                        "renew_ms": machine.limits.renew_ms,
                        "max_concurrent_calls": machine.limits.max_concurrent_calls,
                        "paired": pairing.is_ok(),
                        "host_id": pairing.as_ref().ok().map(|p| p.host_id.clone()),
                        "policy_revision": state.policy_revision(name).ok(),
                    })
                })
                .collect();
            println!(
                "{}",
                serde_json::to_string_pretty(
                    &serde_json::json!({"config": path, "machines": machines})
                )?
            );
            Ok(())
        }
        Command::Serve {
            data_socket,
            namespace,
        } => serve(cli.config, cli.state, data_socket, namespace).await,
    }
}

#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
enum Op {
    Machines {},
    Probe {
        machine: String,
    },
    Claim {
        machine: String,
        call_id: String,
        digest: String,
        tool: Operation,
    },
    Release {
        call_id: String,
    },
    Cancel {
        call_id: String,
    },
    Inspect {
        machine: String,
        call_id: String,
    },
    Freeze {
        machine: Option<String>,
    },
    ClearAttention {
        machine: String,
    },
}

struct Server {
    links: Links,
    config_error: Option<String>,
    /// call_id -> (machine, local one-use attach token)
    calls: Mutex<HashMap<String, (Arc<MachineLink>, Option<String>)>>,
    out: mpsc::UnboundedSender<serde_json::Value>,
}

impl Server {
    fn calls(
        &self,
    ) -> std::sync::MutexGuard<'_, HashMap<String, (Arc<MachineLink>, Option<String>)>> {
        match self.calls.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    async fn handle(self: &Arc<Self>, op: Op) -> serde_json::Value {
        match op {
            Op::Machines {} => {
                let mut machines: Vec<_> =
                    self.links.machines.values().map(|m| m.status()).collect();
                machines.sort_by(|a, b| a["machine"].as_str().cmp(&b["machine"].as_str()));
                serde_json::json!({"ok": true, "machines": machines, "refused": self.links.refused, "config_error": self.config_error})
            }
            Op::Probe { machine } => match self.links.machines.get(&machine) {
                Some(link) => match link.probe().await {
                    Ok(status) => serde_json::json!({"ok": true, "machine": status}),
                    Err(error) => {
                        serde_json::json!({"ok": false, "status": "not_started", "code": format!("{error:#}")})
                    }
                },
                None => {
                    serde_json::json!({"ok": false, "status": "not_started", "code": "unknown_machine"})
                }
            },
            Op::Claim {
                machine,
                call_id,
                digest,
                tool,
            } => {
                let Some(link) = self.links.machines.get(&machine).cloned() else {
                    let code = if self.links.refused.contains_key(&machine) {
                        "unpaired"
                    } else {
                        "unknown_machine"
                    };
                    return serde_json::json!({"ok": false, "status": "not_started", "code": code});
                };
                if !is_hex(&call_id, 32) || !is_hex(&digest, 64) {
                    return serde_json::json!({"ok": false, "status": "not_started", "code": "invalid_request"});
                }
                match link.claim(&call_id, &digest, tool).await {
                    ClaimResult::Claimed {
                        call_id,
                        generation,
                        binding,
                        cwd,
                        platform,
                    } => {
                        let attach = random_hex(16);
                        self.calls()
                            .insert(call_id.clone(), (Arc::clone(&link), Some(attach.clone())));
                        serde_json::json!({"ok": true, "status": "claimed", "call_id": call_id, "generation": generation, "binding": binding, "cwd": cwd, "platform": platform, "attach": attach})
                    }
                    other => {
                        let mut value = serde_json::to_value(&other).unwrap_or_default();
                        value["ok"] = false.into();
                        value
                    }
                }
            }
            Op::Release { call_id } => self.finish(&call_id, false).await,
            Op::Cancel { call_id } => self.finish(&call_id, true).await,
            Op::Inspect { machine, call_id } => match self.links.machines.get(&machine) {
                Some(link) => match link.inspect(&call_id).await {
                    Ok(Some(receipt)) => serde_json::json!({"ok": true, "receipt": receipt}),
                    Ok(None) => serde_json::json!({"ok": false, "code": "not_found"}),
                    Err(error) => {
                        serde_json::json!({"ok": false, "code": "offline", "error": format!("{error:#}")})
                    }
                },
                None => serde_json::json!({"ok": false, "code": "unknown_machine"}),
            },
            Op::Freeze { machine } => {
                let mut results = Vec::new();
                for (name, link) in &self.links.machines {
                    if machine.as_ref().is_some_and(|m| m != name) {
                        continue;
                    }
                    results.push(link.freeze().await);
                }
                let confirmed = results.iter().all(|r| r["confirmed"] == true);
                serde_json::json!({"ok": true, "confirmed": confirmed, "machines": results})
            }
            Op::ClearAttention { machine } => match self.links.machines.get(&machine) {
                Some(link) => {
                    link.clear_attention();
                    serde_json::json!({"ok": true})
                }
                None => serde_json::json!({"ok": false, "code": "unknown_machine"}),
            },
        }
    }

    async fn finish(&self, call_id: &str, cancel: bool) -> serde_json::Value {
        let link = self.calls().remove(call_id).map(|(link, _)| link);
        let Some(link) = link else {
            return serde_json::json!({"ok": false, "code": "no_call"});
        };
        let result = if cancel {
            link.cancel(call_id).await
        } else {
            link.release(call_id).await
        };
        match result {
            Ok(receipt) => serde_json::json!({"ok": true, "receipt": receipt}),
            Err(error) => {
                serde_json::json!({"ok": false, "status": "unknown", "error": format!("{error:#}")})
            }
        }
    }

    async fn data(self: Arc<Self>, mut socket: tokio::net::UnixStream) -> Result<()> {
        ensure!(
            socket.peer_cred()?.uid() == rustix::process::geteuid().as_raw(),
            "data peer denied"
        );
        let mut header = Vec::new();
        loop {
            let byte = socket.read_u8().await?;
            if byte == b'\n' {
                break;
            }
            header.push(byte);
            ensure!(header.len() <= 1024, "attach header too long");
        }
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Attach {
            call_id: String,
            attach: String,
            sync_token: String,
        }
        let attach: Attach = serde_json::from_slice(&header)?;
        let link = {
            let mut calls = self.calls();
            let entry = calls.get_mut(&attach.call_id).context("no claimed call")?;
            // One-use: a second attach (lazy reconnect) is refused.
            let token = entry.1.take().context("call already attached")?;
            ensure!(token == attach.attach, "attach token mismatch");
            Arc::clone(&entry.0)
        };
        let stream = link.attach(&attach.call_id, &attach.sync_token).await?;
        let (sent, received) = stream.pump(socket).await?;
        let _ = self.out.send(serde_json::json!({"event": "stream_closed", "call_id": attach.call_id, "sent": sent, "received": received}));
        Ok(())
    }
}

async fn serve(
    config: Option<PathBuf>,
    state: Option<PathBuf>,
    data_socket: PathBuf,
    namespace: String,
) -> Result<()> {
    ensure!(
        is_hex(&namespace, 32),
        "namespace must be 32 hex characters"
    );
    let (out, mut out_rx) = mpsc::unbounded_channel::<serde_json::Value>();
    let path = config_path(config)?;
    let (config, config_error) = if path.exists() {
        match OriginConfig::load(&path) {
            Ok(config) => (config, None),
            // Fail closed: an invalid config offers no machines at all.
            Err(error) => (
                OriginConfig {
                    version: 1,
                    machines: Default::default(),
                },
                Some(format!("{error:#}")),
            ),
        }
    } else {
        (
            OriginConfig {
                version: 1,
                machines: Default::default(),
            },
            None,
        )
    };
    let state = LinkState::open(&state_path(state)?)?;
    let links = Links::open(&config, &state, &namespace, out.clone());
    let server = Arc::new(Server {
        links,
        config_error,
        calls: Mutex::new(HashMap::new()),
        out: out.clone(),
    });
    let _ = std::fs::remove_file(&data_socket);
    let listener = UnixListener::bind(&data_socket)?;
    std::fs::set_permissions(&data_socket, std::fs::Permissions::from_mode(0o600))?;
    let accept_server = Arc::clone(&server);
    tokio::spawn(async move {
        while let Ok((socket, _)) = listener.accept().await {
            let server = Arc::clone(&accept_server);
            tokio::spawn(async move {
                if let Err(error) = server.data(socket).await {
                    log::info!("data stream ended: {error:#}");
                }
            });
        }
    });
    let writer = tokio::spawn(async move {
        let mut stdout = tokio::io::stdout();
        while let Some(value) = out_rx.recv().await {
            let mut line = serde_json::to_vec(&value).unwrap_or_default();
            line.push(b'\n');
            if stdout.write_all(&line).await.is_err() || stdout.flush().await.is_err() {
                break;
            }
        }
    });
    let _ = out.send(serde_json::json!({"event": "ready"}));
    let mut lines = BufReader::new(tokio::io::stdin()).lines();
    // Guard gone (stdin EOF): exit; the destination sees the control drop,
    // stops our generations and marks unfinished calls unknown.
    while let Ok(Some(line)) = lines.next_line().await {
        if line.len() > 64 * 1024 {
            break;
        }
        // `{"id": N, "op": ...}`: the id is split off, the rest must be one
        // strictly known operation.
        let parsed = serde_json::from_str::<serde_json::Value>(&line)
            .ok()
            .and_then(|mut value| {
                let id = value.as_object_mut()?.remove("id")?.as_u64()?;
                let op = serde_json::from_value::<Op>(value).ok()?;
                Some((id, op))
            });
        let Some((id, op)) = parsed else {
            let _ = out.send(serde_json::json!({"ok": false, "code": "invalid_request"}));
            continue;
        };
        let server = Arc::clone(&server);
        let out = out.clone();
        tokio::spawn(async move {
            let mut reply = server.handle(op).await;
            reply["id"] = id.into();
            let _ = out.send(reply);
        });
    }
    drop(out);
    let _ = tokio::time::timeout(Duration::from_secs(1), writer).await;
    let _ = std::fs::remove_file(&data_socket);
    Ok(())
}
