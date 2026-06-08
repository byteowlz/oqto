//! Extracted channel handlers from ws_multiplexed.

use super::*;

pub(super) async fn handle_hstry_command(
    cmd: HstryWsCommand,
    user_id: &str,
    state: &AppState,
) -> Option<WsEvent> {
    let HstryWsCommand::Query {
        id,
        session_id,
        query,
        limit,
    } = cmd;

    let query = query.unwrap_or_default();
    if query.trim().is_empty() && session_id.is_none() {
        return Some(WsEvent::Hstry(HstryWsEvent::Result {
            id,
            data: serde_json::json!({"hits":[],"total":0}),
        }));
    }

    if let Some(session_id) = session_id {
        // Tree projection endpoint over ws history channel.
        if query.trim() == ":tree" {
            let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
            match crate::oqto_log::projector::project_session_tree_auto(
                std::path::Path::new(&home),
                &session_id,
            )
            .await
            {
                Ok(Some(tree)) => {
                    let data = serde_json::json!({"session_id":session_id,"tree":tree});
                    return Some(WsEvent::Hstry(HstryWsEvent::Result { id, data }));
                }
                Ok(None) => {
                    // continue to legacy path below
                }
                Err(err) => {
                    return Some(WsEvent::Hstry(HstryWsEvent::Error {
                        id,
                        error: err.to_string(),
                    }));
                }
            }
        }

        // Primary read path: oqto-log projector, fallback legacy hstry.
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
        match crate::oqto_log::projector::project_session_messages_auto(
            std::path::Path::new(&home),
            &session_id,
            limit.map(|v| v as usize),
        )
        .await
        {
            Ok(Some(messages)) => {
                let data = serde_json::to_value(&messages).unwrap_or(Value::Null);
                return Some(WsEvent::Hstry(HstryWsEvent::Result { id, data }));
            }
            Ok(None) => {}
            Err(err) => {
                return Some(WsEvent::Hstry(HstryWsEvent::Error {
                    id,
                    error: err.to_string(),
                }));
            }
        }

        Some(WsEvent::Hstry(HstryWsEvent::Result {
            id,
            data: serde_json::json!([]),
        }))
    } else {
        let effective_user = state.effective_linux_username(user_id);
        let user_home = if state.user_isolation_enabled() {
            std::path::PathBuf::from(format!("/home/{effective_user}"))
        } else {
            dirs::home_dir().unwrap_or_else(|| std::path::PathBuf::from("/tmp"))
        };

        let response = match oqto_history::oqto_log::search::search_timeline(
            &oqto_history::oqto_log::search::TimelineSearchRequest {
                user_home: &user_home,
                query: &query,
                scope: oqto_history::oqto_log::search::TimelineSearchScope::All,
                workspace_id: None,
                cwd: None,
                limit: limit.unwrap_or(50) as usize,
            },
        )
        .await
        {
            Ok(response) => response,
            Err(err) => {
                return Some(WsEvent::Hstry(HstryWsEvent::Error {
                    id,
                    error: err.to_string(),
                }));
            }
        };
        let data = serde_json::to_value(response.results).unwrap_or(Value::Null);
        Some(WsEvent::Hstry(HstryWsEvent::Result { id, data }))
    }
}

/// Handle TRX channel commands.
pub(super) async fn handle_trx_command(
    cmd: TrxWsCommand,
    user_id: &str,
    state: &AppState,
) -> Option<WsEvent> {
    let now = Utc::now().timestamp() + 3600;
    let user = CurrentUser {
        claims: Claims {
            sub: user_id.to_string(),
            iss: None,
            aud: None,
            exp: now,
            iat: None,
            nbf: None,
            jti: None,
            email: None,
            name: None,
            preferred_username: None,
            roles: vec![],
            role: None,
        },
    };

    match cmd {
        TrxWsCommand::List { id, workspace_path } => {
            let query = TrxWorkspaceQuery { workspace_path };
            match crate::api::handlers::list_trx_issues(
                axum::extract::State(state.clone()),
                user,
                axum::extract::Query(query),
            )
            .await
            {
                Ok(axum::Json(issues)) => Some(WsEvent::Trx(TrxWsEvent::ListResult {
                    id,
                    issues: serde_json::to_value(issues).unwrap_or(Value::Null),
                })),
                Err(err) => Some(WsEvent::Trx(TrxWsEvent::Error {
                    id,
                    error: err.to_string(),
                })),
            }
        }
        TrxWsCommand::Create {
            id,
            workspace_path,
            data,
        } => {
            let query = TrxWorkspaceQuery { workspace_path };
            let request = CreateTrxIssueRequest {
                title: data.title,
                description: data.description,
                issue_type: data.issue_type.unwrap_or_else(|| "task".to_string()),
                priority: data.priority.unwrap_or(2),
                parent_id: data.parent_id,
            };
            match crate::api::handlers::create_trx_issue(
                axum::extract::State(state.clone()),
                user,
                axum::extract::Query(query),
                axum::Json(request),
            )
            .await
            {
                Ok(axum::Json(issue)) => Some(WsEvent::Trx(TrxWsEvent::IssueResult {
                    id,
                    issue: serde_json::to_value(issue).unwrap_or(Value::Null),
                })),
                Err(err) => Some(WsEvent::Trx(TrxWsEvent::Error {
                    id,
                    error: err.to_string(),
                })),
            }
        }
        TrxWsCommand::Update {
            id,
            workspace_path,
            issue_id,
            data,
        } => {
            let query = TrxWorkspaceQuery { workspace_path };
            let request = UpdateTrxIssueRequest {
                title: data.title,
                description: data.description,
                status: data.status,
                priority: data.priority,
            };
            match crate::api::handlers::update_trx_issue(
                axum::extract::State(state.clone()),
                user,
                axum::extract::Path(issue_id),
                axum::extract::Query(query),
                axum::Json(request),
            )
            .await
            {
                Ok(axum::Json(issue)) => Some(WsEvent::Trx(TrxWsEvent::IssueResult {
                    id,
                    issue: serde_json::to_value(issue).unwrap_or(Value::Null),
                })),
                Err(err) => Some(WsEvent::Trx(TrxWsEvent::Error {
                    id,
                    error: err.to_string(),
                })),
            }
        }
        TrxWsCommand::Close {
            id,
            workspace_path,
            issue_id,
            reason,
        } => {
            let query = TrxWorkspaceQuery { workspace_path };
            let request = CloseTrxIssueRequest { reason };
            match crate::api::handlers::close_trx_issue(
                axum::extract::State(state.clone()),
                user,
                axum::extract::Path(issue_id),
                axum::extract::Query(query),
                axum::Json(request),
            )
            .await
            {
                Ok(axum::Json(issue)) => Some(WsEvent::Trx(TrxWsEvent::IssueResult {
                    id,
                    issue: serde_json::to_value(issue).unwrap_or(Value::Null),
                })),
                Err(err) => Some(WsEvent::Trx(TrxWsEvent::Error {
                    id,
                    error: err.to_string(),
                })),
            }
        }
        TrxWsCommand::Sync { id, workspace_path } => {
            let query = TrxWorkspaceQuery { workspace_path };
            match crate::api::handlers::sync_trx(
                axum::extract::State(state.clone()),
                user,
                axum::extract::Query(query),
            )
            .await
            {
                Ok(axum::Json(resp)) => Some(WsEvent::Trx(TrxWsEvent::SyncResult {
                    id,
                    success: resp
                        .get("synced")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false),
                })),
                Err(err) => Some(WsEvent::Trx(TrxWsEvent::Error {
                    id,
                    error: err.to_string(),
                })),
            }
        }
    }
}

/// Handle Bus channel commands.
pub(super) async fn handle_session_command(
    cmd: SessionWsCommand,
    _user_id: &str,
    _state: &AppState,
) -> Option<WsEvent> {
    let session_id = extract_legacy_session_id(&cmd.cmd);
    // Legacy Session channel commands targeted the OpenCode HTTP API which has been removed.
    // All agent interaction now flows through the Agent channel.
    Some(WsEvent::System(SystemWsEvent::Error {
        id: None,
        error: match session_id {
            Some(id) => format!(
                "Legacy session channel is deprecated for session {}. Use the agent channel instead.",
                id
            ),
            None => {
                "Legacy session channel is deprecated. Use the agent channel instead.".to_string()
            }
        },
    }))
}

pub(super) fn extract_legacy_session_id(cmd: &LegacyWsCommand) -> Option<String> {
    use crate::ws::types::WsCommand as Legacy;
    match cmd {
        Legacy::Subscribe { session_id }
        | Legacy::Unsubscribe { session_id }
        | Legacy::SendMessage { session_id, .. }
        | Legacy::SendParts { session_id, .. }
        | Legacy::Abort { session_id }
        | Legacy::PermissionReply { session_id, .. }
        | Legacy::QuestionReply { session_id, .. }
        | Legacy::QuestionReject { session_id, .. }
        | Legacy::RefreshSession { session_id }
        | Legacy::GetMessages { session_id, .. } => Some(session_id.clone()),
        Legacy::Pong | Legacy::A2uiAction { .. } => None,
    }
}
