//! Disposable headless carrier for native Durable qualification, NOT an Oqto
//! authentication route. The trusted test supervisor creates separate pipes
//! for tools and operator; tools cannot select the control role over the wire.
//! History is a bounded fixture, not direct access to real history stores.
use anyhow::{Context, Result, bail, ensure};
use oq_permissions::*;
use oqto_sandbox::{
    SandboxConfig, SandboxStdin, SeccompMode, build_sandbox_command, configure_bwrap_pre_exec,
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    io::{self, BufRead, Write},
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Deserialize)]
#[serde(tag = "command", rename_all = "snake_case", deny_unknown_fields)]
enum Command {
    Prepare {
        operation: Operation,
    },
    Check {
        operation: Operation,
    },
    Finish {
        id: String,
    },
    Invoke {
        operation: Operation,
    },
    Inspect {
        id: String,
    },
    Decide {
        id: String,
        digest: String,
        revision: u64,
        lifetime: GrantLifetime,
        allow: bool,
    },
    Revoke {
        grant_id: String,
    },
    RevokeAll,
    Recover,
    OperatorBegin {
        operation: Operation,
    },
    OperatorSeal {
        id: String,
        digest: String,
    },
    OperatorFinish {
        id: String,
        digest: String,
    },
    AcknowledgeInterrupted,
    Resources,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CeilingSource {
    /// The owner's sandbox.toml (absent file: defaults).
    user_config: PathBuf,
    home: PathBuf,
    workspace: PathBuf,
    hard_deny: Vec<PathBuf>,
    #[serde(default)]
    remembered: Remembered,
}

fn load_ceiling(source: CeilingSource) -> Result<Policy> {
    let config = SandboxConfig::load_user_config_at(&source.user_config)?;
    let workspace = SandboxConfig::try_load_from_workspace(&source.workspace, &config.profiles)?;
    compile_ceiling(&CeilingInput {
        owner: config.guard.clone(),
        workspace_guard: workspace.and_then(|workspace| workspace.workspace_guard),
        home: source.home,
        workspace: source.workspace,
        hard_deny: source.hard_deny,
        remembered: source.remembered,
    })
}

fn now() -> Result<u64> {
    Ok(u64::try_from(
        SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis(),
    )?)
}

fn confined_read(directory: &std::path::Path) -> Result<String> {
    let extra = directory.join("extra").canonicalize()?;
    let file = extra.join("token").canonicalize()?;
    ensure!(
        file.starts_with(&extra),
        "file resource escaped its registered root"
    );
    let mut config = SandboxConfig::strict();
    config.allow_read.clear();
    config.allow_write.clear();
    config.extra_ro_bind = vec![extra.display().to_string()];
    config.extra_rw_bind.clear();
    config.workspace_cache_enabled = false;
    // oqt executors are not Pi/Claude harness sessions: never create or bind
    // history shards inside the owner's ~/.pi or ~/.claude (preserve-first).
    config.scoped_paths.clear();
    config.seccomp_mode = SeccompMode::Enforce;
    config.seccomp_bpf_path = Some(std::env::var("OQ_ADAPTIVE_SECCOMP_BPF")?);
    let args = vec!["-c".into(), "4096".into(), file.display().to_string()];
    let workdir = directory.join("workdir");
    let mut command = build_sandbox_command(
        &config,
        &workdir,
        std::path::Path::new("/usr/bin/head"),
        &args,
        SandboxStdin::Redirected,
    )?;
    command.env_clear().stdin(std::process::Stdio::null());
    configure_bwrap_pre_exec(&mut command, &config, &workdir, None)?;
    // Fresh read-only hands per invocation: wait confirms completion/teardown
    // before the permission carrier can admit another operation.
    let output = command.output()?;
    ensure!(
        output.status.success(),
        "confined fixture read failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(String::from_utf8(output.stdout)?)
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let role = args.next().context("role: tools or operator")?;
    ensure!(
        role == "tools" || role == "operator",
        "unsupported carrier role"
    );
    let directory = PathBuf::from(
        args.next()
            .context("private fixture authority directory required")?,
    );
    ensure!(args.next().is_none(), "unexpected arguments");
    let default_binding = Binding {
        account_id: "fixture-owner".into(),
        principal_id: "fixture-principal".into(),
        workspace_id: "fixture-workspace".into(),
        work_directory_id: "fixture-workdir".into(),
        session_id: "fixture-public-session".into(),
        audience: "fixture-owner-only".into(),
    };
    // This environment is set only by the trusted demo runner, never read
    // from a tool/control frame. It is not a production identity assertion.
    let binding = match std::env::var("OQ_DEMO_BINDING") {
        Ok(encoded) => serde_json::from_str::<Binding>(&encoded)?,
        Err(std::env::VarError::NotPresent) => default_binding,
        Err(error) => return Err(error.into()),
    };
    let operator = binding.account_id.clone();
    // Coding mode: the ceiling comes from the one sandbox config (ADR-0060):
    // the owner's sandbox.toml (profile + [guard]) and the workspace's
    // .oqto/sandbox.toml [guard], both loaded strictly through oqto-sandbox.
    // The trusted guard supplies only locations and protected roots.
    let ceiling = match std::env::var("OQT_CEILING_SOURCE") {
        Ok(encoded) => Some(load_ceiling(serde_json::from_str(&encoded)?)?),
        Err(std::env::VarError::NotPresent) => None,
        Err(error) => return Err(error.into()),
    };
    let coding = ceiling.is_some();
    let mut policy = Policy {
        paths: vec![],
        revision: "fixture-v1".into(),
        rules: vec![
            Rule {
                target: Target {
                    capability: Capability::HistorySearch,
                    resource_id: "own-history".into(),
                },
                mode: RuleMode::Allow,
            },
            Rule {
                target: Target {
                    capability: Capability::HistorySearch,
                    resource_id: "account-history".into(),
                },
                mode: RuleMode::Ask,
            },
            Rule {
                target: Target {
                    capability: Capability::FileRead,
                    resource_id: "extra-resource".into(),
                },
                mode: RuleMode::Ask,
            },
        ],
    };
    if let Some(ceiling) = ceiling {
        policy = ceiling;
    }
    let ledger = directory.join("permissions.sqlite");
    // Only the owner's operator channel may adopt a changed ceiling (which
    // revokes every grant); the tool channel always opens strictly.
    let authority = if coding && role == "operator" {
        Authority::open_replacing_policy(&ledger, binding, policy.clone()).await?
    } else {
        Authority::open(&ledger, binding, policy.clone()).await?
    };
    // Permits never cross the wire and cannot be reconstructed from tool input.
    // The bounded map holds only this process's live invocations.
    let mut permits: HashMap<String, (Permit, Operation)> = HashMap::new();
    let stdin = io::stdin();
    let mut input = stdin.lock();
    let mut stdout = io::stdout().lock();
    let mut reads = 0;
    loop {
        // Hard frame limit, including malformed/unterminated input. Do not
        // allocate an unbounded string before checking the protocol limit.
        let mut frame = Vec::new();
        loop {
            let buffer = input.fill_buf()?;
            if buffer.is_empty() {
                break;
            }
            let count = buffer
                .iter()
                .position(|b| *b == b'\n')
                .map_or(buffer.len(), |i| i + 1);
            ensure!(frame.len() + count <= 16_384, "carrier frame too large");
            let newline = buffer[count - 1] == b'\n';
            frame.extend_from_slice(&buffer[..count]);
            input.consume(count);
            if newline {
                break;
            }
        }
        if frame.is_empty() {
            break;
        }
        let response: Result<Value> = async {
            let command: Command = serde_json::from_slice(&frame)?;
            match command {
                Command::OperatorBegin { operation } if role == "operator" && coding => {
                    authority.begin_operator(&operator, &operation, now()?).await?;
                    Ok(json!({"journaled":true}))
                }
                Command::OperatorSeal { id, digest } if role == "operator" && coding => {
                    authority.seal_operator_result(&operator, &id, &digest).await?;
                    Ok(json!({"sealed":true}))
                }
                Command::OperatorFinish { id, digest } if role == "operator" && coding => {
                    authority.complete_operator_result(&operator, &id, &digest).await?;
                    Ok(json!({"finished":true}))
                }
                Command::Prepare { operation } if role == "tools" && coding => {
                    ensure!(permits.len() < 64, "too many live invocations");
                    ensure!(
                        !permits.contains_key(&operation.request_id),
                        "invocation already live"
                    );
                    let admission = authority.admit(operation.clone(), now()?).await?;
                    match admission {
                        Admission::Allowed(permit) => {
                            // Journal before any effect: an owner crash after this
                            // point is reported as interrupted, never replayed.
                            authority.begin(&permit, &operation, now()?).await?;
                            let expires_at = permit.expires_at();
                            permits.insert(operation.request_id.clone(), (permit, operation));
                            Ok(json!({"status":"allowed", "expires_at":expires_at}))
                        }
                        other => Ok(json!({"admission":other})),
                    }
                }
                Command::Check { operation } if role == "tools" && coding => {
                    let (permit, admitted) = permits
                        .get(&operation.request_id)
                        .context("no live invocation permit")?;
                    ensure!(admitted == &operation, "invocation arguments changed");
                    authority.check(permit, &operation, now()?).await?;
                    Ok(json!({"checked":true}))
                }
                Command::Finish { id } if role == "tools" && coding => {
                    permits.remove(&id).context("no live invocation permit")?;
                    authority.end(&id).await?;
                    Ok(json!({"finished":true}))
                }
                Command::Invoke { operation } if role == "tools" && !coding => {
                    let admission = authority.admit(operation.clone(), now()?).await?;
                    match &admission {
                        Admission::Allowed(permit) => {
                            authority.check(permit, &operation, now()?).await?;
                            let text = match operation.target.capability {
                                Capability::HistorySearch => {
                                    ensure!(
                                        operation.arguments == json!({"query":"fixture-token"}),
                                        "unsupported fixture query"
                                    );
                                    match operation.target.resource_id.as_str() {
                                        "own-history" => "own fixture-token".to_string(),
                                        "account-history" => "broader fixture-token".to_string(),
                                        _ => bail!("unknown fixture history resource"),
                                    }
                                }
                                Capability::FileRead => {
                                    ensure!(
                                        operation.target.resource_id == "extra-resource"
                                            && operation.arguments
                                                == json!({"relative_path":"token"}),
                                        "unsupported fixture file resource/arguments"
                                    );
                                    confined_read(&directory)?
                                }
                                _ => bail!("fixture does not implement this provider"),
                            };
                            reads += 1;
                            authority.check(permit, &operation, now()?).await?;
                            Ok(json!({"status":"allowed", "text":text, "fixture_reads":reads}))
                        }
                        _ => Ok(json!({"admission":admission, "fixture_reads":reads})),
                    }
                }
                Command::Inspect { id } if role == "operator" => {
                    Ok(json!({"request":authority.inspect(&id).await?}))
                }
                Command::Decide {
                    id,
                    digest,
                    revision,
                    lifetime,
                    allow,
                } if role == "operator" => {
                    let time = now()?;
                    // External resources become executor mounts: Session-scoped
                    // for one hour. Workspace tool grants stay short.
                    let resource = coding
                        && matches!(
                            authority.inspect(&id).await?.operation.target.capability,
                            Capability::FileRead | Capability::FileWrite | Capability::Network
                        );
                    ensure!(
                        !(resource && allow && matches!(lifetime, GrantLifetime::Once)),
                        "resource access is granted per Session, not once"
                    );
                    // Session grants for workspace work last the working day; the
                    // ledger caps every grant at 24 h and revocation is immediate.
                    let ttl = if resource { 3_600_000 } else { 8 * 3_600_000 };
                    let request = authority
                        .decide(
                            &operator,
                            &id,
                            &digest,
                            revision,
                            lifetime,
                            allow,
                            time.checked_add(ttl).context("expiry overflow")?,
                            time,
                        )
                        .await?;
                    Ok(json!({"request":request}))
                }
                Command::Revoke { grant_id } if role == "operator" => {
                    authority.revoke(&operator, &grant_id).await?;
                    Ok(json!({"revoked":true}))
                }
                Command::RevokeAll if role == "operator" => {
                    Ok(json!({"revoked":authority.revoke_all(&operator).await?}))
                }
                Command::Recover if role == "operator" => {
                    Ok(json!({"interrupted":authority.recover(&operator).await?}))
                }
                Command::Resources if role == "operator" && coding => {
                    let grants = authority.active_resource_grants(now()?).await?;
                    // Baseline pre-approvals (global allow_paths / network) need
                    // mounts too, although they never appear as grants.
                    let mut read_paths = Vec::new();
                    let mut write_paths = Vec::new();
                    for layer in &policy.paths {
                        write_paths.extend(layer.allow.iter().cloned());
                        read_paths.extend(layer.allow.iter().cloned());
                    }
                    read_paths.sort();
                    write_paths.sort();
                    read_paths.dedup();
                    write_paths.dedup();
                    let network = policy.mode(&Target {
                        capability: Capability::Network,
                        resource_id: "egress".into(),
                    }) == RuleMode::Allow;
                    // Deny roots must be invisible to the tools themselves:
                    // tool calls are admitted by kind (read/edit/shell), not path, so a
                    // deny inside any mounted tree is a mount mask, not just
                    // an admission answer.
                    let mut denied_paths = Vec::new();
                    for layer in &policy.paths {
                        if layer.builtin {
                            continue;
                        }
                        denied_paths.extend(layer.deny.iter().cloned());
                    }
                    denied_paths.sort();
                    denied_paths.dedup();
                    Ok(json!({
                        "resources": grants
                            .into_iter()
                            .map(|(target, expires)| json!({"target":target, "expires_at":expires}))
                            .collect::<Vec<_>>(),
                        "baseline": {"read_paths":read_paths, "write_paths":write_paths, "network":network,
                            "denied_paths":denied_paths}
                    }))
                }
                Command::AcknowledgeInterrupted if role == "operator" => Ok(json!({
                    "acknowledged":authority.acknowledge_interrupted(&operator).await?
                })),
                _ => bail!("command unavailable on this role/channel"),
            }
        }
        .await;
        let encoded = match response {
            Ok(value) => json!({"ok":true, "result":value}),
            Err(error) => json!({"ok":false, "error":error.to_string()}),
        };
        writeln!(stdout, "{encoded}")?;
        stdout.flush()?;
    }
    authority.close().await;
    Ok(())
}
