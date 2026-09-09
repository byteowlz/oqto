//! Account-bound, read-only presentation of an explicitly selected oqto-log home.
//! No Pi process is created, no credentials are loaded, and Pi JSONL is never written.
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HistoryReadConfig {
    pub account_id: String,
    pub home: PathBuf,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case", deny_unknown_fields)]
pub enum HistoryReadOperation {
    List {},
    Messages {
        session_id: String,
        before: Option<String>,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HistoryReadRequest {
    pub account_id: String,
    pub operation: HistoryReadOperation,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct HistoryReadResponse {
    pub data: serde_json::Value,
}
impl std::fmt::Debug for HistoryReadResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("HistoryReadResponse(<private history>)")
    }
}

pub async fn read(
    config: Option<&HistoryReadConfig>,
    dedicated: bool,
    request: HistoryReadRequest,
) -> Result<HistoryReadResponse> {
    let config = config.context("History access disabled")?;
    ensure!(
        dedicated && !config.account_id.is_empty() && config.account_id == request.account_id,
        "History access denied"
    );
    ensure!(
        config.home.is_absolute() && config.home.is_dir(),
        "History home unavailable"
    );
    let rows = oqto_history::oqto_log::ops::list_sessions(&config.home, None).await?;
    let mut identities = std::collections::HashMap::new();
    for row in &rows {
        *identities.entry(row.platform_id.clone()).or_insert(0_usize) += 1;
    }
    let data = match request.operation {
        HistoryReadOperation::List {} => {
            let sessions: Vec<_> = rows
                .into_iter()
                .filter(|row| {
                    !row.platform_id.is_empty() && identities.get(&row.platform_id) == Some(&1)
                })
                .take(5000)
                .map(|row| {
                    serde_json::json!({
                        "id": row.platform_id, "title": row.title, "workspace": row.workspace_id,
                        "updated_at": row.updated_at,
                    })
                })
                .collect();
            serde_json::json!({"sessions": sessions, "read_only": true})
        }
        HistoryReadOperation::Messages { session_id, before } => {
            ensure!(
                !session_id.is_empty()
                    && session_id.len() <= 256
                    && before.as_ref().is_none_or(|s| s.len() <= 2048),
                "Invalid history request"
            );
            ensure!(
                identities.get(&session_id) == Some(&1),
                "Unambiguous public Session ID required"
            );
            let page = oqto_history::oqto_log::projector::project_session_messages_page_auto(
                &config.home,
                &session_id,
                50,
                before.as_deref(),
            )
            .await?
            .context("History unavailable")?;
            serde_json::to_value(page)?
        }
    };
    Ok(HistoryReadResponse { data })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn authorization_precedes_source_access_and_rejects_shared_mode() {
        let config = HistoryReadConfig {
            account_id: "alice".into(),
            home: "/not/a/history/home".into(),
        };
        for (account, dedicated) in [("bob", true), ("alice", false)] {
            let result = read(
                Some(&config),
                dedicated,
                HistoryReadRequest {
                    account_id: account.into(),
                    operation: HistoryReadOperation::List {},
                },
            )
            .await;
            assert_eq!(result.unwrap_err().to_string(), "History access denied");
        }
    }
    #[tokio::test]
    async fn reads_canonical_history_without_touching_pi_or_accepting_harness_ids() {
        let home = tempfile::tempdir().unwrap();
        let pi = home.path().join(".pi/agent/sessions");
        std::fs::create_dir_all(&pi).unwrap();
        let sentinel = pi.join("fixture.jsonl");
        std::fs::write(&sentinel, "Pi-owned sentinel\n").unwrap();
        let public_id =
            oqto_history::oqto_log::store::platform_id_for_external_id("native-fixture");
        let message: oqto_pi::AgentMessage = serde_json::from_value(serde_json::json!({"role":"user","content":"History fixture","timestamp":1700000000000_i64})).unwrap();
        oqto_history::oqto_log::store::replace_session_with_snapshot(
            home.path(),
            "native-user",
            "/workspace",
            &public_id,
            &public_id,
            Some("native-fixture"),
            "native-fixture",
            &[message],
        )
        .await
        .unwrap();
        let config = HistoryReadConfig {
            account_id: "alice".into(),
            home: home.path().to_owned(),
        };
        let request = |operation| HistoryReadRequest {
            account_id: "alice".into(),
            operation,
        };
        let catalog = read(Some(&config), true, request(HistoryReadOperation::List {}))
            .await
            .unwrap();
        assert_eq!(catalog.data["sessions"][0]["id"], public_id);
        let page = read(
            Some(&config),
            true,
            request(HistoryReadOperation::Messages {
                session_id: public_id,
                before: None,
            }),
        )
        .await
        .unwrap();
        assert_eq!(page.data["messages"].as_array().unwrap().len(), 1);
        assert!(
            read(
                Some(&config),
                true,
                request(HistoryReadOperation::Messages {
                    session_id: "native-fixture".into(),
                    before: None
                })
            )
            .await
            .is_err()
        );
        assert_eq!(
            std::fs::read_to_string(sentinel).unwrap(),
            "Pi-owned sentinel\n"
        );
    }
    #[test]
    fn wire_is_read_only_and_does_not_accept_source_paths() {
        assert!(
            serde_json::from_value::<HistoryReadOperation>(
                serde_json::json!({"command":"list","home":"/other"})
            )
            .is_err()
        );
        assert!(
            serde_json::from_value::<HistoryReadOperation>(
                serde_json::json!({"command":"prompt","message":"no"})
            )
            .is_err()
        );
        let response = crate::protocol::RunnerResponse::HistoryRead(HistoryReadResponse {
            data: serde_json::json!({"sessions":[]}),
        });
        let encoded = serde_json::to_value(&response).unwrap();
        assert!(serde_json::from_value::<crate::protocol::RunnerResponse>(encoded).is_ok());
        assert!(
            !format!(
                "{:?}",
                HistoryReadResponse {
                    data: serde_json::json!("private-message")
                }
            )
            .contains("private-message")
        );
    }
}
