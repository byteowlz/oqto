//! Opt-in real Pi 1 acceptance; launch the entire test process in a private HOME.
#[cfg(test)]
use super::*;
use crate::protocol::AgentControlRequest;
use serde_json::{Value, json};
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

async fn invoke(
    manager: &PiSessionManager,
    session: &str,
    binding: &str,
    operation: oqto_protocol::control::ControlOperation,
    arguments: Value,
) -> Result<Value> {
    let request_id = uuid::Uuid::new_v4().to_string();
    let result = manager
        .control(AgentControlRequest {
            session_id: session.into(),
            binding_id: binding.into(),
            request_id: request_id.clone(),
            operation,
            arguments,
        })
        .await?;
    anyhow::ensure!(
        result.binding_id == binding
            && result.request_id == request_id
            && result.operation == operation,
        "Transport correlation changed"
    );
    anyhow::ensure!(
        result.native_session_id != session && !result.native_session_id.is_empty(),
        "Native/public identity collapsed"
    );
    Ok(result.reply)
}

fn success(reply: &Value) -> Result<()> {
    anyhow::ensure!(reply["ok"] == true, "Core rejected control: {reply}");
    Ok(())
}

fn wrapper(
    root: &Path,
    name: &str,
    binary: &Path,
    extensions: &Path,
    selection: bool,
    capabilities: bool,
) -> Result<std::path::PathBuf> {
    let path = root.join(name);
    // Fixture paths are explicitly owned by this proof, not user-controlled shell input.
    for value in [binary, extensions, root] {
        anyhow::ensure!(
            !value.to_string_lossy().contains('\''),
            "Unsupported fixture path"
        );
    }
    let mut args = String::new();
    if selection {
        args.push_str(&format!(
            " -e '{}/pi-selection/index.ts' --selection-state-dir '{}/reviews'",
            extensions.display(),
            root.display()
        ));
    }
    if capabilities {
        args.push_str(&format!(
            " -e '{}/pi-capabilities/index.ts'",
            extensions.display()
        ));
    }
    std::fs::write(
        &path,
        format!(
            "#!/bin/sh\nprintf '%s\\n' \"$$\" >> '{}/child-pids'\nexec '{}' --offline -ne -ns -np -nc -na --no-tools --no-session --provider fixture --model fixture{} \"$@\"\n",
            root.display(),
            binary.display(),
            args
        ),
    )?;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700))?;
    Ok(path)
}

fn real_manager(binary: std::path::PathBuf, workdir: &Path) -> Arc<PiSessionManager> {
    PiSessionManager::new(PiManagerConfig {
        pi_binary: binary,
        default_cwd: workdir.into(),
        idle_timeout_secs: 300,
        cleanup_interval_secs: 60,
        sandbox_config: None,
        runner_id: "real-selection-proof".into(),
        model_cache_dir: None,
        extra_deny_read: vec![],
    })
}

async fn stats(home: &Path, workdir: &Path, session: &str) -> Result<(usize, usize)> {
    let stats = oqto_history::oqto_log::store::read_session_stats(
        home,
        &workdir.to_string_lossy(),
        session,
    )
    .await?;
    Ok((stats.turns, stats.messages))
}

fn inspect(
    event: &crate::protocol::PiEventWrapper,
    session: &str,
    events: &mut Vec<Value>,
) -> Result<()> {
    anyhow::ensure!(
        event.event.session_id == session,
        "Public event identity changed"
    );
    let value = serde_json::to_value(&event.event)?;
    let kind = value["event"].as_str().context("Event tag missing")?;
    anyhow::ensure!(
        !kind.starts_with("stream.") && !kind.starts_with("tool.") && kind != "agent.working",
        "Unexpected Chat/model event: {kind}"
    );
    events.push(value);
    Ok(())
}

#[tokio::test]
#[ignore = "requires explicitly private whole-process HOME and exact Pi 1/extension fixture"]
async fn real_pi_one_serialized_runner_selection_acceptance() -> Result<()> {
    use oqto_protocol::control::ControlOperation as Op;
    let root =
        std::path::PathBuf::from(std::env::var("OQTO_SELECTION_PROOF_ROOT")?).canonicalize()?;
    anyhow::ensure!(
        root.starts_with("/tmp") && root != Path::new("/tmp"),
        "Proof must be privately temporary"
    );
    let home = std::path::PathBuf::from(std::env::var("HOME")?).canonicalize()?;
    anyhow::ensure!(
        home == root.join("home"),
        "Entire process HOME must be private before manager construction"
    );
    for key in [
        "XDG_STATE_HOME",
        "XDG_CONFIG_HOME",
        "XDG_DATA_HOME",
        "XDG_CACHE_HOME",
        "PI_CODING_AGENT_DIR",
    ] {
        anyhow::ensure!(
            Path::new(&std::env::var(key)?).starts_with(&home),
            "{key} not private"
        );
    }
    let binary = std::path::PathBuf::from(std::env::var("OQTO_SELECTION_PI_BINARY")?);
    let extensions = std::path::PathBuf::from(std::env::var("OQTO_SELECTION_EXTENSION_ROOT")?);
    let version = std::process::Command::new(&binary)
        .arg("--version")
        .output()?;
    anyhow::ensure!(
        version.status.success() && String::from_utf8(version.stdout)?.trim() == "1.0.0",
        "Not exact Pi 1"
    );
    let workdir = root.join("workdir");
    std::fs::create_dir_all(&workdir)?;
    let agent = std::path::PathBuf::from(std::env::var("PI_CODING_AGENT_DIR")?);
    std::fs::create_dir_all(&agent)?;
    std::fs::write(
        agent.join("models.json"),
        serde_json::to_vec(
            &json!({"providers":{"fixture":{"baseUrl":"http://127.0.0.1:9/v1","api":"openai-completions","apiKey":"synthetic","models":[{"id":"fixture","input":["text"],"contextWindow":4096,"maxTokens":256}]}}}),
        )?,
    )?;
    let wrapped = wrapper(&root, "pi-with-selection", &binary, &extensions, true, true)?;
    let manager = real_manager(wrapped, &workdir);
    let session = format!("oqto-{}", uuid::Uuid::new_v4());
    manager
        .create_session(
            session.clone(),
            PiSessionConfig {
                cwd: workdir.clone(),
                env: HashMap::from([(
                    "AGENT_BROWSER_SOCKET_DIR".into(),
                    root.join("browser-sockets").display().to_string(),
                )]),
                ..Default::default()
            },
        )
        .await?;
    manager.get_state(&session).await?;
    let native = manager.session_external_id(&session).await;
    anyhow::ensure!(
        native != session && !native.is_empty(),
        "No real native binding"
    );
    let commands = manager.get_commands(&session).await?;
    for name in [
        "presentation-bind",
        "presentation-list",
        "presentation-unbind",
        "selection-create",
        "selection-get",
        "selection-ack",
        "selection-save",
        "selection-submit",
        "selection-close",
    ] {
        anyhow::ensure!(
            commands
                .as_array()
                .context("Public discovery not an array")?
                .iter()
                .any(|entry| entry["name"] == name && entry["source"] == "extension"),
            "Missing real extension command {name}"
        );
    }
    let before = stats(&home, &workdir, &session).await?;
    anyhow::ensure!(before == (0, 0), "Initial Chat ledger not empty");
    let mut attachment = manager.subscribe(&session, None).await?;
    let first = uuid::Uuid::new_v4().to_string();
    let second = uuid::Uuid::new_v4().to_string();
    let bound = invoke(
        &manager,
        &session,
        &first,
        Op::PresentationBind,
        json!({"clientKind":"oqto-web","capabilities":["selection.questions.v1"],"leaseMs":60000}),
    )
    .await?;
    success(&bound)?;
    anyhow::ensure!(
        bound["snapshot"]["scopeId"] == native,
        "Metadata used platform scope"
    );
    let created = invoke(&manager, &session, &first, Op::SelectionCreate, json!({"presentation":"host","spec":{"version":1,"mode":"questions","title":"Real runner proof","questions":[{"id":"direction","header":"Direction","title":"Choose","kind":"single","required":true,"options":[{"id":"safe","label":"Safe"},{"id":"other","label":"Other"}]}]}})).await?;
    success(&created)?;
    let record = &created["result"]["record"];
    let review = record["id"]
        .as_str()
        .context("Missing review ID")?
        .to_owned();
    anyhow::ensure!(
        record["scopeId"] == native && record["revision"] == 0,
        "Create identity/revision incorrect"
    );
    let mut events = Vec::new();
    let presented = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let event = attachment
                .receiver
                .recv()
                .await
                .context("Event channel closed")?;
            inspect(&event, &session, &mut events)?;
            if let EventPayload::Status {
                key,
                text: Some(text),
            } = event.event.payload
                && key == "pi-selection:present/v1"
            {
                return serde_json::from_str::<Value>(&text).context("Presentation JSON");
            }
        }
    })
    .await
    .context("No canonical present Status")??;
    anyhow::ensure!(
        presented["review"]["id"] == review
            && presented["review"]["scopeId"] == native
            && presented["bindingIds"] == json!([first]),
        "Presentation not bound to owning request"
    );
    success(
        &invoke(
            &manager,
            &session,
            &first,
            Op::SelectionAck,
            json!({"id":review}),
        )
        .await?,
    )?;
    let answers = json!({"direction":{"answered":true,"selectedIds":["safe"]}});
    let saved = invoke(
        &manager,
        &session,
        &first,
        Op::SelectionSave,
        json!({"id":review,"revision":0,"answers":answers}),
    )
    .await?;
    success(&saved)?;
    anyhow::ensure!(
        saved["result"]["revision"] == 1 && saved["result"]["state"] == "draft",
        "Save not CAS draft"
    );
    let stale = invoke(
        &manager,
        &session,
        &first,
        Op::SelectionClose,
        json!({"id":review,"revision":0}),
    )
    .await?;
    anyhow::ensure!(stale["ok"] == false, "Stale view cancelled newer draft");
    let draft = invoke(
        &manager,
        &session,
        &first,
        Op::SelectionGet,
        json!({"id":review}),
    )
    .await?;
    success(&draft)?;
    anyhow::ensure!(
        draft["result"]["record"]["revision"] == 1 && draft["result"]["record"]["state"] == "draft",
        "Stale close mutated draft"
    );
    let submitted = invoke(
        &manager,
        &session,
        &first,
        Op::SelectionSubmit,
        json!({"id":review,"revision":1,"answers":answers}),
    )
    .await?;
    success(&submitted)?;
    let terminal = invoke(
        &manager,
        &session,
        &first,
        Op::SelectionGet,
        json!({"id":review}),
    )
    .await?;
    success(&terminal)?;
    anyhow::ensure!(
        terminal["result"]["record"]["state"] == "submitted"
            && terminal["result"]["record"]["revision"] == 2
            && terminal["result"]["record"]["answers"] == answers,
        "Terminal answer changed"
    );
    let unbound = invoke(
        &manager,
        &session,
        &first,
        Op::PresentationUnbind,
        json!({}),
    )
    .await?;
    success(&unbound)?;
    anyhow::ensure!(
        unbound["snapshot"]["bindings"] == json!([]),
        "Unbind retained owner"
    );
    let other = invoke(
        &manager,
        &session,
        &second,
        Op::PresentationBind,
        json!({"clientKind":"oqto-web","capabilities":["selection.questions.v1"]}),
    )
    .await?;
    success(&other)?;
    let rebound = invoke(
        &manager,
        &session,
        &first,
        Op::PresentationBind,
        json!({"clientKind":"oqto-web","capabilities":["selection.questions.v1"]}),
    )
    .await?;
    success(&rebound)?;
    let bindings = rebound["snapshot"]["bindings"]
        .as_array()
        .context("Bindings array")?;
    anyhow::ensure!(
        first != second
            && bindings.len() == 2
            && bindings.iter().any(|b| b["id"] == first)
            && bindings.iter().any(|b| b["id"] == second),
        "Distinct bindings collapsed"
    );
    let final_state = manager.get_state(&session).await?;
    let native_stats = manager.get_session_stats(&session).await?;
    anyhow::ensure!(
        !final_state.is_streaming
            && final_state.message_count == 0
            && final_state.pending_message_count == 0
            && final_state.session_file.is_none(),
        "Native model/session state changed"
    );
    anyhow::ensure!(
        native_stats.user_messages == 0
            && native_stats.assistant_messages == 0
            && native_stats.total_messages == 0
            && native_stats.tool_calls == 0
            && native_stats.tool_results == 0,
        "Native Chat/model counters changed"
    );
    {
        let sessions = manager.sessions.read().await;
        let active = sessions
            .get(&session)
            .context("Active runner binding lost")?;
        anyhow::ensure!(
            active.pending_client_id.lock().await.is_empty()
                && active.pending_responses.read().await.is_empty(),
            "Runner prompt/correlation bookkeeping left pending"
        );
    }
    let messages = manager.get_messages(&session).await?;
    anyhow::ensure!(
        messages["messages"] == json!([])
            && manager
                .get_message_buffer(&session)
                .await
                .context("Buffer absent")?
                .is_empty(),
        "Native or runner Chat messages added"
    );
    let after = stats(&home, &workdir, &session).await?;
    anyhow::ensure!(after == before, "Chat turn/message ledger writes occurred");
    while let Ok(event) = attachment.receiver.try_recv() {
        inspect(&event, &session, &mut events)?;
    }
    manager.shutdown().await;
    // Same actual Pi binary, no capabilities; then no extensions at all.
    let mut negative = Vec::new();
    for (name, selection, operation) in [
        ("pi-no-capabilities", true, Op::PresentationBind),
        ("pi-no-extensions", false, Op::SelectionGet),
    ] {
        let missing = real_manager(
            wrapper(&root, name, &binary, &extensions, selection, false)?,
            &workdir,
        );
        let id = format!("oqto-{}", uuid::Uuid::new_v4());
        missing
            .create_session(
                id.clone(),
                PiSessionConfig {
                    cwd: workdir.clone(),
                    env: HashMap::from([(
                        "AGENT_BROWSER_SOCKET_DIR".into(),
                        root.join(name)
                            .with_extension("sockets")
                            .display()
                            .to_string(),
                    )]),
                    ..Default::default()
                },
            )
            .await?;
        missing.get_state(&id).await?;
        let missing_catalog = missing.get_commands(&id).await?;
        anyhow::ensure!(!missing_catalog.as_array().context("Missing-case catalog")?.iter().any(|entry| entry["name"] == operation.command() && entry["source"] == "extension"), "Negative command unexpectedly registered");
        let missing_native = missing.session_external_id(&id).await;
        anyhow::ensure!(
            missing_native != id && missing_native != native,
            "Native scope not independent"
        );
        let mut source = missing.subscribe(&id, None).await?;
        let result = invoke(
            &missing,
            &id,
            &first,
            operation,
            if selection {
                json!({"clientKind":"oqto-web","capabilities":[]})
            } else {
                json!({"id":review})
            },
        )
        .await;
        let denial = result
            .err()
            .context("Missing extension fell through")?
            .to_string();
        anyhow::ensure!(
            denial.contains("Extension control unavailable")
                || (!selection
                    && denial
                        .contains("Control unavailable during work or after runtime replacement")),
            "Not an explicit unavailable-control denial: {denial}"
        );
        anyhow::ensure!(
            missing.get_messages(&id).await?["messages"] == json!([])
                && missing
                    .get_message_buffer(&id)
                    .await
                    .context("Buffer absent")?
                    .is_empty()
                && stats(&home, &workdir, &id).await? == (0, 0),
            "Missing control created Chat/ledger"
        );
        let mut denied_events = Vec::new();
        while let Ok(event) = source.receiver.try_recv() {
            inspect(&event, &id, &mut denied_events)?;
        }
        negative.push(json!({"case":name,"platform_id":id,"native_id":missing_native,"catalog":missing_catalog,"denial":denial,"events":denied_events,"model_turns":0,"messages":0,"ledger_turns":0}));
        missing.shutdown().await;
    }
    std::fs::write(
        root.join("evidence.json"),
        serde_json::to_vec_pretty(
            &json!({"pi":"1.0.0","platform_id":session,"native_id":native,"review_id":review,"binding_ids":[first,second],"stale_close_reply":stale,"terminal_record":terminal["result"]["record"],"messages":messages,"native_counts":{"user_messages":native_stats.user_messages,"assistant_messages":native_stats.assistant_messages,"total_messages":native_stats.total_messages,"tool_calls":native_stats.tool_calls,"tool_results":native_stats.tool_results,"pending_messages":final_state.pending_message_count,"streaming":final_state.is_streaming,"session_file":final_state.session_file},"ledger_before":before,"ledger_after":after,"events":events,"negative":negative}),
        )?,
    )?;
    println!(
        "PASS real Pi 1 serialized runner controls, correlated Status/ACK, stale-close CAS, terminal answers, distinct bindings, absent controls, zero Chat/model turns; evidence={}",
        root.join("evidence.json").display()
    );
    Ok(())
}
