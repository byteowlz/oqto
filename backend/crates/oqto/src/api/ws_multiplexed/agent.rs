//! Extracted channel handlers from ws_multiplexed.

use super::*;

pub(super) async fn handle_agent_command(
    cmd: oqto_protocol::commands::Command,
    user_id: &str,
    state: &AppState,
    runner_client: Option<&RunnerClient>,
    conn_state: Arc<tokio::sync::Mutex<WsConnectionState>>,
) -> Option<WsEvent> {
    use oqto_protocol::commands::CommandPayload;

    let id = cmd.id.clone();
    let session_id = cmd.session_id.clone();
    let runner_id = cmd.runner_id.clone().unwrap_or_else(|| "local".to_string());
    let agent_response =
        |session_id: &str, id: Option<String>, cmd: &str, result: Result<Option<Value>, String>| {
            agent_response_with_runner(&runner_id, session_id, id, cmd, result)
        };

    // Resolve the effective runner for this command.
    // Priority:
    // 1. Per-session override (for sessions in shared workspaces, stored on session.create)
    // 2. For session.create: resolve from cwd path (may route to shared workspace runner)
    // 3. Personal runner (default)
    let mut resolved_target_for_command: Option<ExecutionTarget> = None;
    let resolved_runner: RunnerClient = {
        // Check stored override first
        let override_runner = {
            let state_guard = conn_state.lock().await;
            state_guard
                .session_runner_overrides
                .get(&session_id)
                .cloned()
        };

        if let Some(ovr) = override_runner {
            tracing::debug!(session_id = %session_id, socket = ?ovr.socket_path(), "using stored runner override");
            ovr
        } else if let CommandPayload::SessionCreate { ref config } = cmd.payload {
            // For session.create, check if cwd is inside a shared workspace
            tracing::info!(session_id = %session_id, cwd = ?config.cwd, "session.create: checking cwd for shared workspace routing");
            let sw_runner = if let Some(ref cwd) = config.cwd {
                runner_client_for_path(state, user_id, Some(cwd.as_str())).await
            } else {
                None
            };

            if let Some((sw, target)) = sw_runner {
                tracing::info!(session_id = %session_id, socket = ?sw.socket_path(), "routing session to shared workspace runner");
                resolved_target_for_command = Some(target.clone());
                // Store override for subsequent commands on this session
                let is_different =
                    runner_client.map_or(true, |r| r.socket_path() != sw.socket_path());
                if is_different {
                    let mut state_guard = conn_state.lock().await;
                    state_guard
                        .session_runner_overrides
                        .insert(session_id.clone(), sw.clone());
                }
                sw
            } else {
                resolved_target_for_command = Some(ExecutionTarget::Personal);
                match runner_client {
                    Some(r) => r.clone(),
                    None => {
                        return Some(agent_response(
                            &session_id,
                            id,
                            "error",
                            Err("Runner not available".into()),
                        ));
                    }
                }
            }
        } else {
            // For non-create commands, resolve runner deterministically in this order:
            // 0) System-scoped commands (`session_id = _system`) -> personal runner
            // 1) Per-connection cached session cwd metadata
            // 2) Durable hstry conversation workspace metadata
            // 3) fail closed (unknown target)

            if session_id == "_system" {
                match runner_client {
                    Some(r) => r.clone(),
                    None => {
                        return Some(agent_response(
                            &session_id,
                            id,
                            "error",
                            Err("Runner not available".into()),
                        ));
                    }
                }
            } else {
                let meta_cwd = {
                    let state_guard = conn_state.lock().await;
                    state_guard
                        .pi_session_meta
                        .get(&session_id)
                        .and_then(|m| m.cwd.as_ref().map(|p| p.to_string_lossy().to_string()))
                };

                if let Some(cwd) = meta_cwd {
                    if let Some((client, target)) =
                        runner_client_for_path(state, user_id, Some(cwd.as_str())).await
                    {
                        let mut state_guard = conn_state.lock().await;
                        state_guard
                            .session_runner_overrides
                            .insert(session_id.clone(), client.clone());
                        let _ = target;
                        client
                    } else {
                        match &cmd.payload {
                            CommandPayload::GetState => {
                                return Some(agent_response(
                                    &session_id,
                                    id,
                                    "get_state",
                                    Ok(None),
                                ));
                            }
                            CommandPayload::GetMessages => {
                                return Some(agent_response(
                                    &session_id,
                                    id,
                                    "get_messages",
                                    Ok(Some(serde_json::json!([]))),
                                ));
                            }
                            CommandPayload::GetStats => {
                                return Some(agent_response(
                                    &session_id,
                                    id,
                                    "get_stats",
                                    Ok(None),
                                ));
                            }
                            CommandPayload::GetForkPoints => {
                                return Some(agent_response(
                                    &session_id,
                                    id,
                                    "get_fork_points",
                                    Ok(Some(serde_json::json!([]))),
                                ));
                            }
                            CommandPayload::GetCommands => {
                                return Some(agent_response(
                                    &session_id,
                                    id,
                                    "get_commands",
                                    Ok(Some(serde_json::json!([]))),
                                ));
                            }
                            CommandPayload::GetModels { workdir } => {
                                if let Some(personal_runner) = runner_client {
                                    let fallback_workdir = workdir.as_deref().or(Some(&cwd));
                                    if let Ok(resp) = personal_runner
                                        .agent_get_available_models("_system", fallback_workdir)
                                        .await
                                    {
                                        return Some(agent_response(
                                            &session_id,
                                            id,
                                            "get_models",
                                            Ok(Some(
                                                serde_json::to_value(&resp.models)
                                                    .unwrap_or_default(),
                                            )),
                                        ));
                                    }
                                }
                                return Some(agent_response(
                                    &session_id,
                                    id,
                                    "get_models",
                                    Ok(Some(serde_json::json!([]))),
                                ));
                            }
                            CommandPayload::SetModel { .. }
                            | CommandPayload::SetThinkingLevel { .. }
                            | CommandPayload::CycleModel
                            | CommandPayload::CycleThinkingLevel
                            | CommandPayload::SetAutoCompaction { .. }
                            | CommandPayload::SetAutoRetry { .. } => {
                                return Some(agent_response(&session_id, id, "ok", Ok(None)));
                            }
                            _ => {
                                return Some(agent_response(
                                    &session_id,
                                    id,
                                    "error",
                                    Err("Session target unknown; reload session metadata".into()),
                                ));
                            }
                        }
                    }
                } else {
                    let target_from_store = match state.session_targets.get(&session_id).await {
                        Ok(Some(record)) => {
                            let mut target = match record.scope {
                                SessionTargetScope::Personal => Some(ExecutionTarget::Personal),
                                SessionTargetScope::SharedWorkspace => {
                                    record.workspace_id.clone().map(|workspace_id| {
                                        ExecutionTarget::SharedWorkspace { workspace_id }
                                    })
                                }
                            };

                            // Self-heal stale target rows: some older sessions were
                            // persisted as personal despite a shared workspace path.
                            if matches!(target, Some(ExecutionTarget::Personal))
                                && let Some(workspace_path) = record.workspace_path.clone()
                                && let Ok(resolved) = resolve_target_for_workspace_path(
                                    state,
                                    user_id,
                                    &workspace_path,
                                )
                                .await
                                && matches!(resolved, ExecutionTarget::SharedWorkspace { .. })
                            {
                                if let ExecutionTarget::SharedWorkspace { workspace_id } = &resolved
                                {
                                    let corrected = SessionTargetRecord {
                                        session_id: session_id.clone(),
                                        owner_user_id: None,
                                        scope: SessionTargetScope::SharedWorkspace,
                                        workspace_id: Some(workspace_id.clone()),
                                        workspace_path: Some(workspace_path),
                                    };
                                    let _ = state.session_targets.upsert(&corrected).await;
                                }
                                target = Some(resolved);
                            }

                            target
                        }
                        Ok(None) => None,
                        Err(_) => None,
                    };

                    let hydrated = if let Some(target) = target_from_store {
                        match resolve_runner_for_target(state, user_id, &target).await {
                            Ok(Some(client)) => {
                                let mut state_guard = conn_state.lock().await;
                                state_guard
                                    .session_runner_overrides
                                    .insert(session_id.clone(), client.clone());
                                if let Ok(Some(record)) =
                                    state.session_targets.get(&session_id).await
                                    && let Some(workspace_path) = record.workspace_path
                                {
                                    state_guard.pi_session_meta.insert(
                                        session_id.clone(),
                                        PiSessionMeta {
                                            scope: None,
                                            cwd: Some(std::path::PathBuf::from(workspace_path)),
                                        },
                                    );
                                }
                                Some(client)
                            }
                            _ => None,
                        }
                    } else {
                        None
                    };

                    if let Some(client) = hydrated {
                        client
                    } else {
                        // Fail closed for mutating commands (prompt/steer/etc), but allow
                        // read-only probes used by frontend reattach/recovery flows.
                        match &cmd.payload {
                            CommandPayload::GetState => {
                                return Some(agent_response(
                                    &session_id,
                                    id,
                                    "get_state",
                                    Ok(None),
                                ));
                            }
                            CommandPayload::GetMessages => {
                                return Some(agent_response(
                                    &session_id,
                                    id,
                                    "get_messages",
                                    Ok(Some(serde_json::json!([]))),
                                ));
                            }
                            CommandPayload::GetStats => {
                                return Some(agent_response(
                                    &session_id,
                                    id,
                                    "get_stats",
                                    Ok(None),
                                ));
                            }
                            CommandPayload::GetForkPoints => {
                                return Some(agent_response(
                                    &session_id,
                                    id,
                                    "get_fork_points",
                                    Ok(Some(serde_json::json!([]))),
                                ));
                            }
                            CommandPayload::GetCommands => {
                                return Some(agent_response(
                                    &session_id,
                                    id,
                                    "get_commands",
                                    Ok(Some(serde_json::json!([]))),
                                ));
                            }
                            CommandPayload::GetModels { workdir } => {
                                if let Some(personal_runner) = runner_client
                                    && let Ok(resp) = personal_runner
                                        .agent_get_available_models("_system", workdir.as_deref())
                                        .await
                                {
                                    return Some(agent_response(
                                        &session_id,
                                        id,
                                        "get_models",
                                        Ok(Some(
                                            serde_json::to_value(&resp.models).unwrap_or_default(),
                                        )),
                                    ));
                                }
                                return Some(agent_response(
                                    &session_id,
                                    id,
                                    "get_models",
                                    Ok(Some(serde_json::json!([]))),
                                ));
                            }
                            _ => {
                                let msg = "Session target could not be resolved. Reload the session list and retry.";
                                return Some(agent_response(
                                    &session_id,
                                    id,
                                    "error",
                                    Err(msg.into()),
                                ));
                            }
                        }
                    }
                }
            }
        }
    };

    let runner = &resolved_runner;

    match cmd.payload {
        CommandPayload::SessionCreate { config } => {
            info!(
                "agent session.create: user={}, session_id={}",
                user_id, session_id
            );

            // If this connection already has an active subscription for
            // this session, return success immediately. This handles the
            // common case of React StrictMode double-invoke or reconnection
            // re-sending session.create for a session that's already alive.
            {
                let state_guard = conn_state.lock().await;
                if state_guard.pi_subscriptions.contains(&session_id) {
                    debug!(
                        "agent session.create: session {} already subscribed, returning success",
                        session_id
                    );
                    return Some(agent_response(
                        &session_id,
                        id,
                        "session.create",
                        Ok(Some(serde_json::json!({ "session_id": session_id }))),
                    ));
                }
            }

            let mut cwd = config
                .cwd
                .as_ref()
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|| std::path::PathBuf::from("/"));

            // Avoid sandboxing sessions at filesystem root; this causes agents to
            // attempt writing state under `/.oqto` and fail with permission errors.
            // Prefer persisted session target workspace path, then user home fallback.
            if cwd == std::path::PathBuf::from("/") {
                if let Ok(Some(target)) = state.session_targets.get(&session_id).await
                    && let Some(workspace_path) = target.workspace_path
                    && !workspace_path.trim().is_empty()
                {
                    cwd = std::path::PathBuf::from(workspace_path);
                } else if let Some(lu) = state.linux_users.as_ref() {
                    cwd = std::path::PathBuf::from(format!("/home/{}", lu.linux_username(user_id)));
                }
            }

            {
                let mut state_guard = conn_state.lock().await;
                state_guard.pi_session_meta.insert(
                    session_id.clone(),
                    PiSessionMeta {
                        scope: Some(config.harness.clone()),
                        cwd: Some(cwd.clone()),
                    },
                );
            }

            let cwd_string = cwd.to_string_lossy().to_string();

            // If no explicit continue_session was provided, try to find an
            // existing Pi JSONL session file for this session ID. This enables
            // resuming external sessions (started in Pi directly, not through
            // Oqto) so the agent has the full conversation context.
            let continue_session = if config.continue_session.is_some() {
                config.continue_session.map(std::path::PathBuf::from)
            } else {
                crate::pi::session_files::find_session_file_async(
                    session_id.clone(),
                    Some(cwd.clone()),
                )
                .await
            };

            if let Some(ref cs) = continue_session {
                debug!(
                    "agent session.create: found session file for {}: {:?}",
                    session_id, cs
                );
            }

            let pi_config = RunnerPiSessionConfig {
                cwd,
                provider: config.provider,
                model: config.model,
                session_file: None,
                continue_session,
                env: std::collections::HashMap::new(),
            };

            let req = PiCreateSessionRequest {
                session_id: session_id.clone(),
                config: pi_config,
            };

            match runner.agent_create_session(req).await {
                Ok(_resp) => {
                    // Session stored under the provisional ID. Pi may
                    // assign a different real ID -- the runner re-keys
                    // its map in the background, and the frontend learns
                    // about it via the get_state response.

                    // Pin this session to the runner that successfully created it.
                    // Without this, a prompt sent immediately after session.create can
                    // race target persistence and fail with "Session target unknown".
                    {
                        let mut state_guard = conn_state.lock().await;
                        state_guard
                            .session_runner_overrides
                            .insert(session_id.clone(), runner.clone());
                    }

                    // Auto-subscribe to events for the session.
                    // We MUST wait for the subscription to be established
                    // before returning the session.create response, otherwise
                    // the frontend may send a prompt before events are being
                    // forwarded, causing streaming to silently fail.
                    let mut state_guard = conn_state.lock().await;
                    if !state_guard.pi_subscriptions.contains(&session_id) {
                        state_guard.subscribed_sessions.insert(session_id.clone());
                        state_guard.pi_subscriptions.insert(session_id.clone());
                        let event_tx = state_guard.event_tx.clone();
                        let runner = runner.clone();
                        let sid = session_id.clone();
                        let uid = user_id.to_string();

                        // Use a oneshot channel to wait for subscription confirmation
                        let (sub_ready_tx, sub_ready_rx) = oneshot::channel::<()>();
                        let runner_id = runner_id.clone();
                        let conn_state_for_fwd = Arc::clone(&conn_state);
                        let forwarder = tokio::spawn(async move {
                            if let Err(e) = forward_pi_events(
                                &runner,
                                &sid,
                                &uid,
                                event_tx,
                                conn_state_for_fwd,
                                Some(sub_ready_tx),
                                runner_id,
                            )
                            .await
                            {
                                error!("Event forwarding error for session {}: {:?}", sid, e);
                            }
                        });
                        state_guard
                            .pi_forwarders
                            .insert(session_id.clone(), forwarder);

                        // Wait for the subscription to be confirmed (with timeout)
                        drop(state_guard);
                        match tokio::time::timeout(Duration::from_secs(5), sub_ready_rx).await {
                            Ok(Ok(())) => {
                                debug!("Event subscription established for session {}", session_id);
                            }
                            Ok(Err(_)) => {
                                warn!(
                                    "Event subscription sender dropped for session {} (forward_pi_events may have failed early)",
                                    session_id
                                );
                            }
                            Err(_) => {
                                warn!(
                                    "Timed out waiting for event subscription for session {}",
                                    session_id
                                );
                            }
                        }
                    } else {
                        drop(state_guard);
                    }

                    let mut target_for_persist = resolved_target_for_command
                        .clone()
                        .unwrap_or(ExecutionTarget::Personal);

                    // Safety net: if cwd is a shared workspace path but routing resolved
                    // as personal, force metadata scope from canonical path resolution.
                    if matches!(target_for_persist, ExecutionTarget::Personal)
                        && (cwd_string.starts_with("/home/oqto_shared_")
                            || cwd_string.starts_with("/home/octo_shared_"))
                    {
                        if let Ok(resolved) =
                            crate::runner::router::resolve_target_for_workspace_path(
                                state,
                                user_id,
                                &cwd_string,
                            )
                            .await
                            && matches!(resolved, ExecutionTarget::SharedWorkspace { .. })
                        {
                            target_for_persist = resolved;
                        }
                    }

                    let (target_scope, target_workspace_id, target_record) =
                        match target_for_persist {
                            ExecutionTarget::Personal => (
                                "personal",
                                None,
                                SessionTargetRecord {
                                    session_id: session_id.clone(),
                                    owner_user_id: Some(user_id.to_string()),
                                    scope: SessionTargetScope::Personal,
                                    workspace_id: None,
                                    workspace_path: Some(cwd_string.clone()),
                                },
                            ),
                            ExecutionTarget::SharedWorkspace { workspace_id } => (
                                "shared_workspace",
                                Some(workspace_id.clone()),
                                SessionTargetRecord {
                                    session_id: session_id.clone(),
                                    owner_user_id: None,
                                    scope: SessionTargetScope::SharedWorkspace,
                                    workspace_id: Some(workspace_id),
                                    workspace_path: Some(cwd_string.clone()),
                                },
                            ),
                        };
                    if let Err(e) = state.session_targets.upsert(&target_record).await {
                        tracing::error!(
                            session_id = %session_id,
                            user_id = %user_id,
                            error = %e,
                            "failed to persist session target metadata during session.create; continuing"
                        );
                    }

                    Some(agent_response(
                        &session_id,
                        id,
                        "session.create",
                        Ok(Some(serde_json::json!({
                            "session_id": session_id,
                            "target_scope": target_scope,
                            "target_workspace_id": target_workspace_id,
                            "workspace_path": cwd_string,
                        }))),
                    ))
                }
                Err(e) => Some(agent_response(
                    &session_id,
                    id,
                    "session.create",
                    Err(format!("Failed to create session: {}", e)),
                )),
            }
        }

        CommandPayload::SessionClose => {
            info!(
                "agent session.close: user={}, session_id={}",
                user_id, session_id
            );

            let mut state_guard = conn_state.lock().await;
            state_guard.subscribed_sessions.remove(&session_id);
            state_guard.pi_subscriptions.remove(&session_id);
            if let Some(handle) = state_guard.pi_forwarders.remove(&session_id) {
                handle.abort();
            }
            if let Some(handle) = state_guard.response_watchdogs.remove(&session_id) {
                handle.abort();
            }
            drop(state_guard);

            match runner.agent_close_session(&session_id).await {
                Ok(()) => {
                    clear_client_ids_for_session(&session_id).await;
                    Some(agent_response(&session_id, id, "session.close", Ok(None)))
                }
                Err(e) => Some(agent_response(
                    &session_id,
                    id,
                    "session.close",
                    Err(e.to_string()),
                )),
            }
        }

        CommandPayload::SessionDelete => {
            info!(
                "agent session.delete: user={}, session_id={}",
                user_id, session_id
            );

            let mut state_guard = conn_state.lock().await;
            state_guard.subscribed_sessions.remove(&session_id);
            state_guard.pi_subscriptions.remove(&session_id);
            if let Some(handle) = state_guard.pi_forwarders.remove(&session_id) {
                handle.abort();
            }
            if let Some(handle) = state_guard.response_watchdogs.remove(&session_id) {
                handle.abort();
            }
            drop(state_guard);

            match runner.agent_delete_session(&session_id).await {
                Ok(()) => {
                    clear_client_ids_for_session(&session_id).await;
                    if let Err(e) = state.session_targets.delete(&session_id).await {
                        return Some(agent_response(
                            &session_id,
                            id,
                            "session.delete",
                            Err(format!("Failed to delete session target metadata: {}", e)),
                        ));
                    }
                    Some(agent_response(&session_id, id, "session.delete", Ok(None)))
                }
                Err(e) => Some(agent_response(
                    &session_id,
                    id,
                    "session.delete",
                    Err(e.to_string()),
                )),
            }
        }

        CommandPayload::SessionNew { parent_session } => {
            debug!(
                "agent session.new: user={}, session_id={}",
                user_id, session_id
            );
            match runner
                .agent_new_session(&session_id, parent_session.as_deref())
                .await
            {
                Ok(()) => Some(agent_response(
                    &session_id,
                    id,
                    "session.new",
                    Ok(Some(serde_json::json!({ "session_id": session_id }))),
                )),
                Err(e) => Some(agent_response(
                    &session_id,
                    id,
                    "session.new",
                    Err(e.to_string()),
                )),
            }
        }

        CommandPayload::SessionSwitch { session_path } => {
            debug!(
                "agent session.switch: user={}, session_id={}, path={}",
                user_id, session_id, session_path
            );
            match runner
                .agent_switch_session(&session_id, &session_path)
                .await
            {
                Ok(()) => Some(agent_response(
                    &session_id,
                    id,
                    "session.switch",
                    Ok(Some(serde_json::json!({ "session_id": session_id }))),
                )),
                Err(e) => Some(agent_response(
                    &session_id,
                    id,
                    "session.switch",
                    Err(e.to_string()),
                )),
            }
        }

        CommandPayload::SessionRestart => {
            info!(
                "agent session.restart: user={}, session_id={}",
                user_id, session_id
            );

            let state_before_restart = runner.agent_get_state(&session_id).await.ok();
            let cwd = {
                let state_guard = conn_state.lock().await;
                state_guard
                    .pi_session_meta
                    .get(&session_id)
                    .and_then(|m| m.cwd.clone())
            };
            let cwd = if let Some(cwd) = cwd {
                cwd
            } else {
                match state.session_targets.get(&session_id).await {
                    Ok(Some(record)) => record
                        .workspace_path
                        .map(std::path::PathBuf::from)
                        .unwrap_or_else(|| std::path::PathBuf::from("/")),
                    _ => std::path::PathBuf::from("/"),
                }
            };

            let continue_session = if let Some(path) = state_before_restart
                .as_ref()
                .and_then(|s| s.state.session_file.clone())
                .map(std::path::PathBuf::from)
            {
                Some(path)
            } else {
                crate::pi::session_files::find_session_file_async(
                    session_id.clone(),
                    Some(cwd.clone()),
                )
                .await
            };

            let provider = state_before_restart
                .as_ref()
                .and_then(|s| s.state.model.as_ref().map(|m| m.provider.clone()));
            let model = state_before_restart
                .as_ref()
                .and_then(|s| s.state.model.as_ref().map(|m| m.id.clone()));

            {
                let mut state_guard = conn_state.lock().await;
                state_guard.subscribed_sessions.remove(&session_id);
                state_guard.pi_subscriptions.remove(&session_id);
                if let Some(handle) = state_guard.pi_forwarders.remove(&session_id) {
                    handle.abort();
                }
                if let Some(handle) = state_guard.response_watchdogs.remove(&session_id) {
                    handle.abort();
                }
                state_guard.pi_session_meta.insert(
                    session_id.clone(),
                    PiSessionMeta {
                        scope: Some("pi".to_string()),
                        cwd: Some(cwd.clone()),
                    },
                );
            }

            let _ = runner.agent_close_session(&session_id).await;

            let req = PiCreateSessionRequest {
                session_id: session_id.clone(),
                config: RunnerPiSessionConfig {
                    cwd,
                    provider,
                    model,
                    session_file: None,
                    continue_session,
                    env: std::collections::HashMap::new(),
                },
            };

            match runner.agent_create_session(req).await {
                Ok(_resp) => {
                    let mut state_guard = conn_state.lock().await;
                    state_guard.subscribed_sessions.insert(session_id.clone());
                    state_guard.pi_subscriptions.insert(session_id.clone());
                    let event_tx = state_guard.event_tx.clone();
                    let runner = runner.clone();
                    let sid = session_id.clone();
                    let uid = user_id.to_string();
                    let (sub_ready_tx, sub_ready_rx) = oneshot::channel::<()>();
                    let runner_id = runner_id.clone();
                    let conn_state_for_fwd = Arc::clone(&conn_state);
                    let forwarder = tokio::spawn(async move {
                        if let Err(e) = forward_pi_events(
                            &runner,
                            &sid,
                            &uid,
                            event_tx,
                            conn_state_for_fwd,
                            Some(sub_ready_tx),
                            runner_id,
                        )
                        .await
                        {
                            error!("Event forwarding error for session {}: {:?}", sid, e);
                        }
                    });
                    state_guard
                        .pi_forwarders
                        .insert(session_id.clone(), forwarder);
                    drop(state_guard);

                    match tokio::time::timeout(Duration::from_secs(5), sub_ready_rx).await {
                        Ok(Ok(())) => {
                            debug!(
                                "Event subscription re-established for session {}",
                                session_id
                            );
                        }
                        Ok(Err(_)) => {
                            warn!(
                                "Event subscription sender dropped for restarted session {}",
                                session_id
                            );
                        }
                        Err(_) => {
                            warn!(
                                "Timed out waiting for event subscription on restarted session {}",
                                session_id
                            );
                        }
                    }

                    clear_client_ids_for_session(&session_id).await;
                    Some(agent_response(
                        &session_id,
                        id,
                        "session.restart",
                        Ok(Some(serde_json::json!({ "session_id": session_id }))),
                    ))
                }
                Err(e) => Some(agent_response(
                    &session_id,
                    id,
                    "session.restart",
                    Err(format!("Failed to restart session: {}", e)),
                )),
            }
        }

        CommandPayload::Prompt {
            message, client_id, ..
        } => {
            if has_accepted_client_id(&session_id, client_id.as_deref()).await {
                return Some(agent_response(&session_id, id, "prompt", Ok(None)));
            }
            if message.trim().is_empty() {
                warn!(
                    "agent prompt rejected empty message: user={}, session_id={}",
                    user_id, session_id
                );
                Some(agent_response(
                    &session_id,
                    id,
                    "prompt",
                    Err("Empty prompt is not allowed".to_string()),
                ))
            } else {
                // For shared workspaces, prepend the user's display name to the message
                // so the agent knows which user is speaking.
                let effective_message = tag_shared_workspace_message(
                    state,
                    &conn_state,
                    &session_id,
                    user_id,
                    &message,
                )
                .await;

                info!(
                    "agent prompt: user={}, session_id={}, len={}, client_id={:?}",
                    user_id,
                    session_id,
                    effective_message.len(),
                    client_id
                );
                let client_id_for_broadcast = client_id.clone();
                let client_id_for_dedupe = client_id.clone();
                match runner
                    .agent_prompt(&session_id, &effective_message, client_id)
                    .await
                {
                    Ok(()) => {
                        mark_client_id_accepted(&session_id, client_id_for_dedupe.as_deref()).await;
                        let event_tx = {
                            let state_guard = conn_state.lock().await;
                            state_guard.event_tx.clone()
                        };
                        arm_response_watchdog(&conn_state, &session_id, &runner_id, event_tx).await;
                        broadcast_user_message(
                            state,
                            &session_id,
                            user_id,
                            &effective_message,
                            client_id_for_broadcast,
                        )
                        .await;
                        Some(agent_response(&session_id, id, "prompt", Ok(None)))
                    }
                    Err(e) => {
                        let error_msg = format!("Failed to send prompt: {}", e);
                        warn!(
                            "agent prompt failed: user={}, session_id={}, error={}",
                            user_id, session_id, error_msg
                        );
                        emit_terminal_send_failure(
                            &conn_state,
                            &session_id,
                            &runner_id,
                            error_msg.clone(),
                        )
                        .await;
                        Some(agent_response(&session_id, id, "prompt", Err(error_msg)))
                    }
                }
            }
        }

        CommandPayload::Steer { message, client_id } => {
            if has_accepted_client_id(&session_id, client_id.as_deref()).await {
                return Some(agent_response(&session_id, id, "steer", Ok(None)));
            }
            if message.trim().is_empty() {
                warn!(
                    "agent steer rejected empty message: user={}, session_id={}",
                    user_id, session_id
                );
                Some(agent_response(
                    &session_id,
                    id,
                    "steer",
                    Err("Empty steer is not allowed".to_string()),
                ))
            } else {
                let effective_message = tag_shared_workspace_message(
                    state,
                    &conn_state,
                    &session_id,
                    user_id,
                    &message,
                )
                .await;
                info!(
                    "agent steer: user={}, session_id={}, len={}, client_id={:?}",
                    user_id,
                    session_id,
                    effective_message.len(),
                    client_id
                );
                let client_id_for_broadcast = client_id.clone();
                let client_id_for_dedupe = client_id.clone();
                match runner
                    .agent_steer(&session_id, &effective_message, client_id)
                    .await
                {
                    Ok(()) => {
                        mark_client_id_accepted(&session_id, client_id_for_dedupe.as_deref()).await;
                        let event_tx = {
                            let state_guard = conn_state.lock().await;
                            state_guard.event_tx.clone()
                        };
                        arm_response_watchdog(&conn_state, &session_id, &runner_id, event_tx).await;
                        broadcast_user_message(
                            state,
                            &session_id,
                            user_id,
                            &effective_message,
                            client_id_for_broadcast,
                        )
                        .await;
                        Some(agent_response(&session_id, id, "steer", Ok(None)))
                    }
                    Err(e) => {
                        let error_msg = format!("Failed to send steer: {}", e);
                        warn!(
                            "agent steer failed: user={}, session_id={}, error={}",
                            user_id, session_id, error_msg
                        );
                        emit_terminal_send_failure(
                            &conn_state,
                            &session_id,
                            &runner_id,
                            error_msg.clone(),
                        )
                        .await;
                        Some(agent_response(&session_id, id, "steer", Err(error_msg)))
                    }
                }
            }
        }

        CommandPayload::FollowUp { message, client_id } => {
            if has_accepted_client_id(&session_id, client_id.as_deref()).await {
                return Some(agent_response(&session_id, id, "follow_up", Ok(None)));
            }
            if message.trim().is_empty() {
                warn!(
                    "agent follow_up rejected empty message: user={}, session_id={}",
                    user_id, session_id
                );
                Some(agent_response(
                    &session_id,
                    id,
                    "follow_up",
                    Err("Empty follow_up is not allowed".to_string()),
                ))
            } else {
                let effective_message = tag_shared_workspace_message(
                    state,
                    &conn_state,
                    &session_id,
                    user_id,
                    &message,
                )
                .await;
                info!(
                    "agent follow_up: user={}, session_id={}, len={}, client_id={:?}",
                    user_id,
                    session_id,
                    effective_message.len(),
                    client_id
                );
                let client_id_for_broadcast = client_id.clone();
                let client_id_for_dedupe = client_id.clone();
                match runner
                    .agent_follow_up(&session_id, &effective_message, client_id)
                    .await
                {
                    Ok(()) => {
                        mark_client_id_accepted(&session_id, client_id_for_dedupe.as_deref()).await;
                        let event_tx = {
                            let state_guard = conn_state.lock().await;
                            state_guard.event_tx.clone()
                        };
                        arm_response_watchdog(&conn_state, &session_id, &runner_id, event_tx).await;
                        broadcast_user_message(
                            state,
                            &session_id,
                            user_id,
                            &effective_message,
                            client_id_for_broadcast,
                        )
                        .await;
                        Some(agent_response(&session_id, id, "follow_up", Ok(None)))
                    }
                    Err(e) => {
                        let error_msg = format!("Failed to send follow_up: {}", e);
                        warn!(
                            "agent follow_up failed: user={}, session_id={}, error={}",
                            user_id, session_id, error_msg
                        );
                        emit_terminal_send_failure(
                            &conn_state,
                            &session_id,
                            &runner_id,
                            error_msg.clone(),
                        )
                        .await;
                        Some(agent_response(&session_id, id, "follow_up", Err(error_msg)))
                    }
                }
            }
        }

        CommandPayload::Abort => {
            info!("agent abort: user={}, session_id={}", user_id, session_id);
            match runner.agent_abort(&session_id).await {
                Ok(()) => None,
                Err(e) => Some(agent_response(
                    &session_id,
                    id,
                    "abort",
                    Err(format!("Failed to abort: {}", e)),
                )),
            }
        }

        CommandPayload::InputResponse {
            request_id,
            value,
            confirmed,
            cancelled,
        } => {
            debug!(
                "agent input_response: user={}, session_id={}, req={}",
                user_id, session_id, request_id
            );
            match runner
                .agent_extension_ui_response(
                    &session_id,
                    &request_id,
                    value.as_deref(),
                    confirmed,
                    cancelled,
                )
                .await
            {
                Ok(()) => Some(agent_response(&session_id, id, "input_response", Ok(None))),
                Err(e) => Some(agent_response(
                    &session_id,
                    id,
                    "input_response",
                    Err(e.to_string()),
                )),
            }
        }

        CommandPayload::GetState => {
            debug!(
                "agent get_state: user={}, session_id={}",
                user_id, session_id
            );
            match tokio::time::timeout(Duration::from_secs(6), runner.agent_get_state(&session_id))
                .await
            {
                Ok(Ok(resp)) => {
                    let state_value = serde_json::to_value(&resp.state).unwrap_or(Value::Null);
                    Some(agent_response(
                        &session_id,
                        id,
                        "get_state",
                        Ok(Some(state_value)),
                    ))
                }
                Ok(Err(e)) => Some(agent_response(
                    &session_id,
                    id,
                    "get_state",
                    Err(e.to_string()),
                )),
                Err(_) => Some(agent_response(&session_id, id, "get_state", Ok(None))),
            }
        }

        CommandPayload::GetMessages => {
            debug!(
                "agent get_messages: user={}, session_id={}",
                user_id, session_id
            );
            handle_get_messages(
                id,
                &session_id,
                user_id,
                state,
                runner,
                conn_state,
                &runner_id,
            )
            .await
        }

        CommandPayload::GetStats => {
            debug!(
                "agent get_stats: user={}, session_id={}",
                user_id, session_id
            );
            match runner.agent_get_session_stats(&session_id).await {
                Ok(resp) => Some(agent_response(
                    &session_id,
                    id,
                    "get_stats",
                    Ok(Some(serde_json::to_value(&resp.stats).unwrap_or_default())),
                )),
                Err(e) => Some(agent_response(
                    &session_id,
                    id,
                    "get_stats",
                    Err(e.to_string()),
                )),
            }
        }

        CommandPayload::GetModels { workdir } => {
            debug!(
                "agent get_models: user={}, session_id={}, workdir={:?}",
                user_id, session_id, workdir
            );
            match runner
                .agent_get_available_models(&session_id, workdir.as_deref())
                .await
            {
                Ok(resp) => Some(agent_response(
                    &session_id,
                    id,
                    "get_models",
                    Ok(Some(serde_json::to_value(&resp.models).unwrap_or_default())),
                )),
                Err(e) => Some(agent_response(
                    &session_id,
                    id,
                    "get_models",
                    Err(e.to_string()),
                )),
            }
        }

        CommandPayload::GetCommands => {
            debug!(
                "agent get_commands: user={}, session_id={}",
                user_id, session_id
            );
            match runner.agent_get_commands(&session_id).await {
                Ok(resp) => {
                    let commands: Vec<Value> = resp
                        .commands
                        .into_iter()
                        .map(|c| {
                            serde_json::json!({
                                "name": c.name,
                                "description": c.description,
                                "type": c.source,
                            })
                        })
                        .collect();
                    Some(agent_response(
                        &session_id,
                        id,
                        "get_commands",
                        Ok(Some(Value::Array(commands))),
                    ))
                }
                Err(e) => {
                    let msg = e.to_string();
                    if msg.contains("PiSessionNotFound")
                        || msg.contains("SessionNotFound")
                        || (msg.contains("unknown variant") && msg.contains("agent_get_commands"))
                    {
                        // Compatibility fallback: older runners may not implement
                        // AgentGetCommands yet. Treat as "no commands" instead of
                        // surfacing an error banner in new/empty sessions.
                        return Some(agent_response(
                            &session_id,
                            id,
                            "get_commands",
                            Ok(Some(Value::Array(Vec::new()))),
                        ));
                    }
                    Some(agent_response(&session_id, id, "get_commands", Err(msg)))
                }
            }
        }

        CommandPayload::GetForkPoints => {
            debug!(
                "agent get_fork_points: user={}, session_id={}",
                user_id, session_id
            );
            match runner.agent_get_fork_messages(&session_id).await {
                Ok(resp) => {
                    let messages: Vec<Value> = resp
                        .messages
                        .into_iter()
                        .map(|m| {
                            serde_json::json!({
                                "entry_id": m.entry_id,
                                "role": "user",
                                "preview": m.text,
                            })
                        })
                        .collect();
                    Some(agent_response(
                        &session_id,
                        id,
                        "get_fork_points",
                        Ok(Some(Value::Array(messages))),
                    ))
                }
                Err(e) => Some(agent_response(
                    &session_id,
                    id,
                    "get_fork_points",
                    Err(e.to_string()),
                )),
            }
        }

        CommandPayload::SetModel { provider, model_id } => {
            debug!(
                "agent set_model: user={}, session_id={}, {}:{}",
                user_id, session_id, provider, model_id
            );
            match runner
                .agent_set_model(&session_id, &provider, &model_id)
                .await
            {
                Ok(resp) => {
                    // Emit ConfigModelChanged event so the frontend UI updates.
                    let config_event = WsEvent::Agent(oqto_protocol::events::Event {
                        session_id: session_id.clone(),
                        runner_id: runner_id.clone(),
                        ts: Utc::now().timestamp_millis(),
                        payload: oqto_protocol::events::EventPayload::ConfigModelChanged {
                            provider: resp.model.provider.clone(),
                            model_id: resp.model.id.clone(),
                        },
                    });
                    let state_guard = conn_state.lock().await;
                    let _ = state_guard.event_tx.send(config_event);
                    drop(state_guard);

                    // Update hstry conversation with new model/provider
                    if let Some(hstry) = state.hstry.as_ref() {
                        let model_id_clone = resp.model.id.clone();
                        let provider_clone = resp.model.provider.clone();
                        let sid = session_id.clone();
                        let hstry = hstry.clone();
                        tokio::spawn(async move {
                            if let Err(e) = hstry
                                .update_conversation(
                                    &sid,
                                    None,
                                    None,
                                    Some(model_id_clone),
                                    Some(provider_clone),
                                    None,
                                    None,
                                    None,
                                    None,
                                )
                                .await
                            {
                                debug!("Failed to update hstry model on set_model: {}", e);
                            }
                        });
                    }

                    Some(agent_response(
                        &session_id,
                        id,
                        "set_model",
                        Ok(Some(serde_json::json!({
                            "provider": resp.model.provider,
                            "model_id": resp.model.id,
                        }))),
                    ))
                }
                Err(e) => Some(agent_response(
                    &session_id,
                    id,
                    "set_model",
                    Err(e.to_string()),
                )),
            }
        }

        CommandPayload::CycleModel => {
            debug!(
                "agent cycle_model: user={}, session_id={}",
                user_id, session_id
            );
            match runner.agent_cycle_model(&session_id).await {
                Ok(resp) => {
                    // Emit ConfigModelChanged event so the frontend UI updates.
                    let config_event = WsEvent::Agent(oqto_protocol::events::Event {
                        session_id: session_id.clone(),
                        runner_id: runner_id.clone(),
                        ts: Utc::now().timestamp_millis(),
                        payload: oqto_protocol::events::EventPayload::ConfigModelChanged {
                            provider: resp.model.provider.clone(),
                            model_id: resp.model.id.clone(),
                        },
                    });
                    let state_guard = conn_state.lock().await;
                    let _ = state_guard.event_tx.send(config_event);
                    drop(state_guard);

                    Some(agent_response(
                        &session_id,
                        id,
                        "cycle_model",
                        Ok(Some(serde_json::json!({
                            "provider": resp.model.provider,
                            "model_id": resp.model.id,
                        }))),
                    ))
                }
                Err(e) => Some(agent_response(
                    &session_id,
                    id,
                    "cycle_model",
                    Err(e.to_string()),
                )),
            }
        }

        CommandPayload::SetThinkingLevel { level } => {
            debug!(
                "agent set_thinking_level: user={}, session_id={}, level={}",
                user_id, session_id, level
            );
            match runner.agent_set_thinking_level(&session_id, &level).await {
                Ok(resp) => Some(agent_response(
                    &session_id,
                    id,
                    "set_thinking_level",
                    Ok(Some(serde_json::json!({ "level": resp.level }))),
                )),
                Err(e) => Some(agent_response(
                    &session_id,
                    id,
                    "set_thinking_level",
                    Err(e.to_string()),
                )),
            }
        }

        CommandPayload::CycleThinkingLevel => {
            debug!(
                "agent cycle_thinking_level: user={}, session_id={}",
                user_id, session_id
            );
            match runner.agent_cycle_thinking_level(&session_id).await {
                Ok(resp) => Some(agent_response(
                    &session_id,
                    id,
                    "cycle_thinking_level",
                    Ok(Some(serde_json::json!({ "level": resp.level }))),
                )),
                Err(e) => Some(agent_response(
                    &session_id,
                    id,
                    "cycle_thinking_level",
                    Err(e.to_string()),
                )),
            }
        }

        CommandPayload::SetAutoCompaction { enabled } => {
            debug!(
                "agent set_auto_compaction: user={}, session_id={}, enabled={}",
                user_id, session_id, enabled
            );
            match runner.agent_set_auto_compaction(&session_id, enabled).await {
                Ok(()) => Some(agent_response(
                    &session_id,
                    id,
                    "set_auto_compaction",
                    Ok(None),
                )),
                Err(e) => Some(agent_response(
                    &session_id,
                    id,
                    "set_auto_compaction",
                    Err(e.to_string()),
                )),
            }
        }

        CommandPayload::SetAutoRetry { enabled } => {
            debug!(
                "agent set_auto_retry: user={}, session_id={}, enabled={}",
                user_id, session_id, enabled
            );
            match runner.agent_set_auto_retry(&session_id, enabled).await {
                Ok(()) => Some(agent_response(&session_id, id, "set_auto_retry", Ok(None))),
                Err(e) => Some(agent_response(
                    &session_id,
                    id,
                    "set_auto_retry",
                    Err(e.to_string()),
                )),
            }
        }

        CommandPayload::Compact { instructions } => {
            info!("agent compact: user={}, session_id={}", user_id, session_id);
            match runner
                .agent_compact(&session_id, instructions.as_deref())
                .await
            {
                Ok(()) => None, // Compaction events stream via subscription
                Err(e) => Some(agent_response(
                    &session_id,
                    id,
                    "compact",
                    Err(format!("Failed to compact: {}", e)),
                )),
            }
        }

        CommandPayload::AbortRetry => {
            debug!(
                "agent abort_retry: user={}, session_id={}",
                user_id, session_id
            );
            match runner.agent_abort_retry(&session_id).await {
                Ok(()) => Some(agent_response(&session_id, id, "abort_retry", Ok(None))),
                Err(e) => Some(agent_response(
                    &session_id,
                    id,
                    "abort_retry",
                    Err(e.to_string()),
                )),
            }
        }

        CommandPayload::SetSessionName { name } => {
            debug!(
                "agent set_session_name: user={}, session_id={}, name={}",
                user_id, session_id, name
            );
            match runner.agent_set_session_name(&session_id, &name).await {
                Ok(()) => Some(agent_response(
                    &session_id,
                    id,
                    "set_session_name",
                    Ok(None),
                )),
                Err(e) => Some(agent_response(
                    &session_id,
                    id,
                    "set_session_name",
                    Err(e.to_string()),
                )),
            }
        }

        CommandPayload::Fork { entry_id } => {
            debug!(
                "agent fork: user={}, session_id={}, entry_id={}",
                user_id, session_id, entry_id
            );
            match runner.agent_fork(&session_id, &entry_id).await {
                Ok(resp) => Some(agent_response(
                    &session_id,
                    id,
                    "fork",
                    Ok(Some(serde_json::json!({
                        "text": resp.text,
                        "cancelled": resp.cancelled,
                    }))),
                )),
                Err(e) => Some(agent_response(&session_id, id, "fork", Err(e.to_string()))),
            }
        }

        CommandPayload::ListSessions => {
            debug!("agent list_sessions: user={}", user_id);

            // Collect sessions from the user's personal runner
            let mut all_sessions: Vec<Value> = match runner.agent_list_sessions().await {
                Ok(sessions) => sessions
                    .iter()
                    .map(|s| {
                        let mut obj = serde_json::json!({
                            "session_id": s.session_id,
                            "state": s.state,
                            "cwd": s.cwd,
                            "provider": s.provider,
                            "model": s.model,
                            "last_activity": s.last_activity,
                            "subscriber_count": s.subscriber_count,
                        });
                        if let Some(ref hid) = s.hstry_id {
                            obj["hstry_id"] = serde_json::Value::String(hid.clone());
                        }
                        obj
                    })
                    .collect(),
                Err(e) => {
                    warn!("list_sessions failed for personal runner: {}", e);
                    Vec::new()
                }
            };

            // Also query shared workspace runners the user has access to.
            // Pre-store runner overrides so subsequent commands (get_messages,
            // get_state, prompt, etc.) route to the correct runner without
            // requiring the user to have sent session.create.
            if let Some(sw_service) = state.shared_workspaces.as_ref() {
                if let Ok(workspaces) = sw_service.list_for_user(user_id).await {
                    for ws in &workspaces {
                        if let Some(sw_runner) =
                            runner_client_for_linux_user(state, user_id, Some(&ws.linux_user))
                        {
                            match sw_runner.agent_list_sessions().await {
                                Ok(sessions) => {
                                    // Store runner overrides for all discovered shared sessions
                                    if !sessions.is_empty() {
                                        let mut state_guard = conn_state.lock().await;
                                        for s in &sessions {
                                            state_guard
                                                .session_runner_overrides
                                                .insert(s.session_id.clone(), sw_runner.clone());
                                            // Pre-populate session meta with cwd so
                                            // prompt/steer handlers can resolve the
                                            // shared workspace for username tagging
                                            // without requiring session.create.
                                            if !state_guard
                                                .pi_session_meta
                                                .contains_key(&s.session_id)
                                            {
                                                state_guard.pi_session_meta.insert(
                                                    s.session_id.clone(),
                                                    PiSessionMeta {
                                                        scope: Some("pi".to_string()),
                                                        cwd: Some(std::path::PathBuf::from(&s.cwd)),
                                                    },
                                                );
                                            }
                                        }
                                        drop(state_guard);
                                    }
                                    for s in &sessions {
                                        let mut obj = serde_json::json!({
                                            "session_id": s.session_id,
                                            "state": s.state,
                                            "cwd": s.cwd,
                                            "provider": s.provider,
                                            "model": s.model,
                                            "last_activity": s.last_activity,
                                            "subscriber_count": s.subscriber_count,
                                            "shared_workspace_id": ws.id,
                                        });
                                        if let Some(ref hid) = s.hstry_id {
                                            obj["hstry_id"] =
                                                serde_json::Value::String(hid.clone());
                                        }
                                        all_sessions.push(obj);
                                    }
                                }
                                Err(e) => {
                                    debug!(
                                        "list_sessions failed for shared workspace {}: {}",
                                        ws.id, e
                                    );
                                }
                            }
                        }
                    }
                }
            }

            Some(agent_response(
                &session_id,
                id,
                "list_sessions",
                Ok(Some(serde_json::json!({ "sessions": all_sessions }))),
            ))
        }

        CommandPayload::Delegate(_) | CommandPayload::DelegateCancel(_) => {
            // Delegation not yet implemented
            Some(agent_response(
                &session_id,
                id,
                "delegate",
                Err("Delegation not yet implemented".into()),
            ))
        }
    }
}
