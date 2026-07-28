//! Extracted channel handlers from ws_multiplexed.

use super::*;

fn terminal_binding_matches(existing: &TerminalSession, user_id: &str, session_id: &str) -> bool {
    existing.owner_user_id == user_id && existing.session_id == session_id
}

/// Single decision point for terminal authorization.
///
/// Returns the refusal event when the connection's principal may not use a
/// terminal, for every command variant, so nothing is spawned or attached.
fn refuse_terminal(allowed: bool, cmd: &TerminalWsCommand) -> Option<WsEvent> {
    if allowed {
        return None;
    }
    let (id, terminal_id) = match cmd {
        TerminalWsCommand::Open {
            id, terminal_id, ..
        } => (id.clone(), terminal_id.clone()),
        TerminalWsCommand::Input {
            id, terminal_id, ..
        }
        | TerminalWsCommand::Resize {
            id, terminal_id, ..
        }
        | TerminalWsCommand::Close { id, terminal_id } => (id.clone(), Some(terminal_id.clone())),
    };
    Some(WsEvent::Terminal(TerminalWsEvent::Error {
        id,
        terminal_id,
        error: "Terminal access is restricted to administrators".to_string(),
    }))
}

pub(super) async fn handle_terminal_command(
    cmd: TerminalWsCommand,
    user_id: &str,
    state: &AppState,
    conn_state: Arc<tokio::sync::Mutex<WsConnectionState>>,
) -> Option<WsEvent> {
    let allowed = conn_state.lock().await.terminal_allowed;
    if let Some(refusal) = refuse_terminal(allowed, &cmd) {
        warn!("Terminal refused for non-operator user {}", user_id);
        return Some(refusal);
    }

    match cmd {
        TerminalWsCommand::Open {
            id,
            terminal_id,
            workspace_path,
            session_id,
            cols,
            rows,
        } => {
            info!(
                "Terminal open: user={}, workspace_path={:?}, session_id={:?}, terminal_id={:?}",
                user_id, workspace_path, session_id, terminal_id
            );
            let session = match files::resolve_terminal_session(
                user_id,
                state,
                workspace_path.as_deref(),
                session_id.as_deref(),
            )
            .await
            {
                Ok(session) => session,
                Err(err) => {
                    return Some(WsEvent::Terminal(TerminalWsEvent::Error {
                        id,
                        terminal_id,
                        error: err,
                    }));
                }
            };

            info!(
                "Terminal session resolved: id={}, workspace_path={:?}, ttyd_port={}",
                session.id, session.workspace_path, session.ttyd_port
            );

            if session.ttyd_port == 0 {
                warn!(
                    "Terminal not available: ttyd_port=0 for session {}",
                    session.id
                );
                return Some(WsEvent::Terminal(TerminalWsEvent::Error {
                    id,
                    terminal_id,
                    error: "Terminal is not available for this session".into(),
                }));
            }

            let terminal_id = terminal_id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
            let state_guard = conn_state.lock().await;
            if let Some(existing) = state_guard.terminal_sessions.get(&terminal_id) {
                if terminal_binding_matches(existing, user_id, &session.id) {
                    info!(
                        "Terminal already exists and matches binding: {}",
                        terminal_id
                    );
                    return Some(WsEvent::Terminal(TerminalWsEvent::Opened {
                        id,
                        terminal_id,
                    }));
                }

                tracing::warn!(
                    terminal_id = %terminal_id,
                    requested_session_id = %session.id,
                    existing_session_id = %existing.session_id,
                    existing_workspace = ?existing.workspace_path,
                    "terminal.open rejected due to terminal binding mismatch"
                );
                return Some(WsEvent::Terminal(TerminalWsEvent::Error {
                    id,
                    terminal_id: Some(terminal_id),
                    error: "Terminal ID already bound to a different session".into(),
                }));
            }

            let event_tx = state_guard.event_tx.clone();
            let session_id = session.id.clone();
            let session_workspace_path = session.workspace_path.clone();
            let ttyd_port = session.ttyd_port as u16;
            drop(state_guard);
            info!(
                "Starting terminal task: terminal_id={}, session_id={}, ttyd_port={}",
                terminal_id, session_id, ttyd_port
            );

            // No credential means the terminal is disabled or not running.
            let ttyd_password =
                match files::terminal_credential_for_session(state, user_id, &session).await {
                    Ok(Some(password)) => password,
                    Ok(None) => {
                        warn!(
                            "Terminal unavailable: no credential for session {}",
                            session_id
                        );
                        return Some(WsEvent::Terminal(TerminalWsEvent::Error {
                            id,
                            terminal_id: Some(terminal_id),
                            error: "Terminal is not available for this session".into(),
                        }));
                    }
                    Err(err) => {
                        tracing::error!(
                            session_id = %session_id,
                            error = %err,
                            "failed to fetch terminal credential"
                        );
                        return Some(WsEvent::Terminal(TerminalWsEvent::Error {
                            id,
                            terminal_id: Some(terminal_id),
                            error: "Terminal is not available for this session".into(),
                        }));
                    }
                };

            let (command_tx, task) = match files::start_terminal_task(
                terminal_id.clone(),
                session_id,
                ttyd_port,
                ttyd_password,
                cols,
                rows,
                event_tx.clone(),
            )
            .await
            {
                Ok(result) => result,
                Err(err) => {
                    return Some(WsEvent::Terminal(TerminalWsEvent::Error {
                        id,
                        terminal_id: Some(terminal_id),
                        error: err,
                    }));
                }
            };

            let mut state_guard = conn_state.lock().await;
            state_guard.terminal_sessions.insert(
                terminal_id.clone(),
                TerminalSession {
                    owner_user_id: user_id.to_string(),
                    session_id: session.id.clone(),
                    workspace_path: Some(session_workspace_path),
                    command_tx,
                    task,
                },
            );

            Some(WsEvent::Terminal(TerminalWsEvent::Opened {
                id,
                terminal_id,
            }))
        }
        TerminalWsCommand::Input {
            id,
            terminal_id,
            data,
        } => {
            let state_guard = conn_state.lock().await;
            if let Some(session) = state_guard.terminal_sessions.get(&terminal_id) {
                if session.owner_user_id != user_id {
                    return Some(WsEvent::Terminal(TerminalWsEvent::Error {
                        id,
                        terminal_id: Some(terminal_id),
                        error: "Terminal ownership mismatch".into(),
                    }));
                }
                let _ = session.command_tx.send(TerminalSessionCommand::Input(data));
                None
            } else {
                Some(WsEvent::Terminal(TerminalWsEvent::Error {
                    id,
                    terminal_id: Some(terminal_id),
                    error: "Terminal session not found".into(),
                }))
            }
        }
        TerminalWsCommand::Resize {
            id,
            terminal_id,
            cols,
            rows,
        } => {
            let state_guard = conn_state.lock().await;
            if let Some(session) = state_guard.terminal_sessions.get(&terminal_id) {
                if session.owner_user_id != user_id {
                    return Some(WsEvent::Terminal(TerminalWsEvent::Error {
                        id,
                        terminal_id: Some(terminal_id),
                        error: "Terminal ownership mismatch".into(),
                    }));
                }
                let _ = session
                    .command_tx
                    .send(TerminalSessionCommand::Resize { cols, rows });
                None
            } else {
                Some(WsEvent::Terminal(TerminalWsEvent::Error {
                    id,
                    terminal_id: Some(terminal_id),
                    error: "Terminal session not found".into(),
                }))
            }
        }
        TerminalWsCommand::Close { id, terminal_id } => {
            let mut state_guard = conn_state.lock().await;
            if let Some(session) = state_guard.terminal_sessions.get(&terminal_id)
                && session.owner_user_id != user_id
            {
                return Some(WsEvent::Terminal(TerminalWsEvent::Error {
                    id,
                    terminal_id: Some(terminal_id),
                    error: "Terminal ownership mismatch".into(),
                }));
            }
            if let Some(session) = state_guard.terminal_sessions.remove(&terminal_id) {
                let _ = session.command_tx.send(TerminalSessionCommand::Close);
                session.task.abort();
                None
            } else {
                Some(WsEvent::Terminal(TerminalWsEvent::Error {
                    id,
                    terminal_id: Some(terminal_id),
                    error: "Terminal session not found".into(),
                }))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn terminal_binding_matches_accepts_same_user_and_session() {
        let (tx, _rx) = mpsc::unbounded_channel();
        let session = TerminalSession {
            owner_user_id: "user-a".to_string(),
            session_id: "ses-1".to_string(),
            workspace_path: Some("/home/user-a/ws".to_string()),
            command_tx: tx,
            task: tokio::spawn(async {}),
        };
        assert!(terminal_binding_matches(&session, "user-a", "ses-1"));
    }

    #[tokio::test]
    async fn terminal_binding_matches_rejects_different_session_or_user() {
        let (tx, _rx) = mpsc::unbounded_channel();
        let session = TerminalSession {
            owner_user_id: "user-a".to_string(),
            session_id: "ses-1".to_string(),
            workspace_path: Some("/home/user-a/ws".to_string()),
            command_tx: tx,
            task: tokio::spawn(async {}),
        };
        assert!(!terminal_binding_matches(&session, "user-a", "ses-2"));
        assert!(!terminal_binding_matches(&session, "user-b", "ses-1"));
    }

    fn all_command_variants() -> Vec<TerminalWsCommand> {
        vec![
            TerminalWsCommand::Open {
                id: Some("req-1".to_string()),
                terminal_id: Some("term-1".to_string()),
                workspace_path: Some("/home/user-a/ws".to_string()),
                session_id: Some("ses-1".to_string()),
                cols: 80,
                rows: 24,
            },
            TerminalWsCommand::Input {
                id: Some("req-1".to_string()),
                terminal_id: "term-1".to_string(),
                data: "whoami\n".to_string(),
            },
            TerminalWsCommand::Resize {
                id: Some("req-1".to_string()),
                terminal_id: "term-1".to_string(),
                cols: 100,
                rows: 40,
            },
            TerminalWsCommand::Close {
                id: Some("req-1".to_string()),
                terminal_id: "term-1".to_string(),
            },
        ]
    }

    #[test]
    fn disallowed_principal_is_refused_for_every_terminal_command() {
        for cmd in all_command_variants() {
            let refusal = refuse_terminal(false, &cmd)
                .unwrap_or_else(|| panic!("expected refusal for {cmd:?}"));
            match refusal {
                WsEvent::Terminal(TerminalWsEvent::Error {
                    id,
                    terminal_id,
                    error,
                }) => {
                    assert_eq!(id.as_deref(), Some("req-1"));
                    assert_eq!(terminal_id.as_deref(), Some("term-1"));
                    assert!(error.contains("restricted"));
                }
                other => panic!("expected terminal error, got {other:?}"),
            }
        }
    }

    #[test]
    fn allowed_principal_is_not_refused() {
        for cmd in all_command_variants() {
            assert!(refuse_terminal(true, &cmd).is_none());
        }
    }
}
