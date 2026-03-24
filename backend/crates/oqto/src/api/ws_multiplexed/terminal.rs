//! Extracted channel handlers from ws_multiplexed.

use super::*;

fn terminal_binding_matches(existing: &TerminalSession, user_id: &str, session_id: &str) -> bool {
    existing.owner_user_id == user_id && existing.session_id == session_id
}

pub(super) async fn handle_terminal_command(
    cmd: TerminalWsCommand,
    user_id: &str,
    state: &AppState,
    conn_state: Arc<tokio::sync::Mutex<WsConnectionState>>,
) -> Option<WsEvent> {
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

            let (command_tx, task) = match files::start_terminal_task(
                terminal_id.clone(),
                session_id,
                ttyd_port,
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
}
