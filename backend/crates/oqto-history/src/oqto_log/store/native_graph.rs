//! Lossless native-entry graph facts. Only the serialized Runner ingest worker
//! may persist these; a chat read may only nudge that worker (ADR-0025/0063).
use super::*;
use anyhow::ensure;
use sha2::{Digest, Sha256};
use std::collections::HashMap;

const SOURCE: &str = "pi-native-graph-v1";
const MAX_ENTRIES: usize = 100_000;
const MAX_BYTES: usize = 64 * 1024 * 1024;

struct Entry {
    id: String,
    raw: Value,
    message: Option<AgentMessage>,
    parent_message: Option<String>,
}
pub struct NativeGraph {
    native_id: String,
    header: Value,
    workspace: String,
    entries: Vec<Entry>,
    leaf_id: Option<String>,
    head_message: Option<String>,
}
fn token(value: &str) -> Result<()> {
    ensure!(
        !value.is_empty()
            && value.len() <= 256
            && value
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_'),
        "Invalid native entry identity"
    );
    Ok(())
}
impl NativeGraph {
    /// `leaf_id` is explicit runtime evidence, including a known empty head.
    /// Never pass the last parsed id as a substitute for native runtime state.
    pub fn parse(
        bytes: &[u8],
        native_id: &str,
        workspace: &str,
        leaf_id: Option<&str>,
    ) -> Result<Self> {
        ensure!(bytes.len() <= MAX_BYTES, "Native source exceeds bound");
        token(native_id)?;
        if let Some(leaf) = leaf_id {
            token(leaf)?;
        }
        let text = std::str::from_utf8(bytes).context("Invalid native source encoding")?;
        // Pi may currently be appending its final record. Only complete lines
        // are facts; a head referencing that partial record is not admitted.
        let complete = text
            .rsplit_once('\n')
            .map(|(prefix, _)| prefix)
            .context("Incomplete native source header")?;
        let mut lines = complete.lines();
        let header: Value =
            serde_json::from_str(lines.next().context("Missing native source header")?)?;
        ensure!(
            header["type"] == "session" && header["id"] == native_id && header["cwd"] == workspace,
            "Native source identity mismatch"
        );
        let mut entries = Vec::new();
        let mut nearest: HashMap<String, Option<String>> = HashMap::new();
        for line in lines {
            ensure!(entries.len() < MAX_ENTRIES, "Native entry bound exceeded");
            let raw: Value = serde_json::from_str(line)?;
            let id = raw["id"]
                .as_str()
                .context("Native entry identity missing")?
                .to_owned();
            token(&id)?;
            ensure!(
                !nearest.contains_key(&id),
                "Duplicate native entry identity"
            );
            let parent = match raw.get("parentId") {
                Some(Value::Null) => None,
                Some(Value::String(parent)) => Some(parent.as_str()),
                _ => bail!("Native parent evidence missing"),
            };
            let parent_message = match parent {
                None => None,
                Some(parent) => nearest
                    .get(parent)
                    .context("Native parent absent or cyclic")?
                    .clone(),
            };
            let message = if raw["type"] == "message" {
                Some(serde_json::from_value(raw["message"].clone())?)
            } else {
                None
            };
            nearest.insert(
                id.clone(),
                if message.is_some() {
                    Some(id.clone())
                } else {
                    parent_message.clone()
                },
            );
            entries.push(Entry {
                id,
                raw,
                message,
                parent_message,
            });
        }
        let head_message = match leaf_id {
            Some(leaf) => nearest
                .get(leaf)
                .context("Native head is not durably present")?
                .clone(),
            None => None,
        };
        Ok(Self {
            native_id: native_id.into(),
            header,
            workspace: workspace.into(),
            entries,
            leaf_id: leaf_id.map(Into::into),
            head_message,
        })
    }
}
impl NativeGraph {
    /// The messages on the branch ending at the live head, oldest first, in exactly
    /// the form `merge` stores them; empty for a known empty head.
    pub fn branch_messages(
        &self,
        public_id: &str,
    ) -> Vec<crate::oqto_log::projector::NativeBranchMessage> {
        let by_id: HashMap<&str, &Entry> = self
            .entries
            .iter()
            .map(|entry| (entry.id.as_str(), entry))
            .collect();
        let mut chain = Vec::new();
        let mut next = self.head_message.as_deref();
        while let Some(id) = next {
            let Some(entry) = by_id.get(id) else { break };
            chain.push(*entry);
            next = entry.parent_message.as_deref();
            if chain.len() > self.entries.len() {
                break;
            }
        }
        chain.reverse();
        chain
            .into_iter()
            .filter_map(|entry| {
                let message = entry.message.as_ref()?;
                Some(crate::oqto_log::projector::NativeBranchMessage {
                    message_id: stable_id("message", public_id, &entry.id),
                    parent_turn_id: entry
                        .parent_message
                        .as_deref()
                        .map(|id| stable_id("turn", public_id, id)),
                    role: normalize_role(&message.role).to_owned(),
                    content: extract_text(&message.content),
                    json_payload: serde_json::to_string(message).ok()?,
                    created_at_ms: message
                        .timestamp
                        .and_then(|value| i64::try_from(value).ok())
                        .filter(|value| *value > 0)
                        .map(|value| {
                            if value < 1_000_000_000_000 {
                                value * 1000
                            } else {
                                value
                            }
                        })
                        .or_else(|| {
                            entry.raw["timestamp"]
                                .as_str()
                                .and_then(|text| chrono::DateTime::parse_from_rfc3339(text).ok())
                                .map(|time| time.timestamp_millis())
                        })
                        .unwrap_or(0),
                })
            })
            .collect()
    }
}

fn stable_id(kind: &str, session: &str, native_entry: &str) -> String {
    // Length-framed tuple: no text/ordinal/array-order reconciliation.
    let value = serde_json::json!([SOURCE, kind, session, native_entry]).to_string();
    format!("{kind}:{}", hex::encode(Sha256::digest(value.as_bytes())))
}

/// Atomic merge; legacy sources and Fork lineage are never removed or rebound.
/// A source mutation rolls the whole transaction back, including head changes.
pub async fn merge(
    user_home: &Path,
    user_id: &str,
    public_id: &str,
    graph: &NativeGraph,
) -> Result<()> {
    ensure!(
        is_canonical_session_id(public_id) && public_id != graph.native_id,
        "Canonical public identity required"
    );
    let pool = open_workspace_pool(user_home, &graph.workspace).await?;
    let mut tx = pool.begin().await.context("Begin native graph merge")?;
    let (session_id, platform_id) =
        canonicalize_session_identity(&mut tx, public_id, public_id, Some(&graph.native_id), true)
            .await?;
    ensure!(
        session_id == public_id && platform_id == public_id,
        "Native graph identity changed"
    );
    sqlx::query("INSERT INTO oqto_log_sessions(session_id,platform_id,external_id,user_id,workspace_id) VALUES(?,?,?,?,?) ON CONFLICT(session_id) DO NOTHING")
        .bind(public_id).bind(public_id).bind(&graph.native_id).bind(user_id).bind(&graph.workspace).execute(&mut *tx).await?;
    let owner: (String, Option<String>, Option<String>) = sqlx::query_as(
        "SELECT user_id,workspace_id,external_id FROM oqto_log_sessions WHERE session_id=?",
    )
    .bind(public_id)
    .fetch_one(&mut *tx)
    .await?;
    ensure!(
        owner.0 == user_id
            && owner.1.as_deref() == Some(graph.workspace.as_str())
            && owner.2.as_deref() == Some(graph.native_id.as_str()),
        "Native graph owner mismatch"
    );
    append_pi_session_binding(&mut tx, public_id, Some(&graph.native_id), SOURCE).await?;
    let branch = format!("branch:{public_id}:{SOURCE}");
    sqlx::query("INSERT OR IGNORE INTO oqto_log_branches(branch_id,session_id) VALUES(?,?)")
        .bind(&branch)
        .bind(public_id)
        .execute(&mut *tx)
        .await?;
    let previous: Option<String> =
        sqlx::query_scalar("SELECT extensions_json FROM oqto_log_branches WHERE branch_id=?")
            .bind(&branch)
            .fetch_one(&mut *tx)
            .await?;
    let previous: Value = previous
        .map(|value| serde_json::from_str(&value))
        .transpose()?
        .unwrap_or(Value::Null);
    let old_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM oqto_log_raw_envelopes WHERE session_id=? AND source=? AND harness='pi'")
        .bind(public_id).bind(SOURCE).fetch_one(&mut *tx).await?;
    ensure!(
        old_count <= i64::try_from(graph.entries.len() + 1)?,
        "Native source lost established entries"
    );
    let header_id = stable_id("header", public_id, &graph.native_id);
    let header_json = serde_json::to_string(&graph.header)?;
    let old_header: Option<String> =
        sqlx::query_scalar("SELECT payload_json FROM oqto_log_raw_envelopes WHERE raw_id=?")
            .bind(&header_id)
            .fetch_optional(&mut *tx)
            .await?;
    if let Some(old_header) = old_header {
        ensure!(old_header == header_json, "Native header changed");
    } else {
        sqlx::query("INSERT INTO oqto_log_raw_envelopes(raw_id,session_id,source,harness,native_type,source_sequence,received_at,payload_json,payload_sha256) VALUES(?,? ,?,'pi','session',0,datetime('now'),?,?)")
            .bind(header_id).bind(public_id).bind(SOURCE).bind(&header_json).bind(hex::encode(Sha256::digest(header_json.as_bytes()))).execute(&mut *tx).await?;
    }
    let mut version: i64 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(turn_version),0) FROM oqto_log_turns WHERE session_id=?",
    )
    .bind(public_id)
    .fetch_one(&mut *tx)
    .await?;
    for (position, entry) in graph.entries.iter().enumerate() {
        let raw_id = stable_id("raw", public_id, &entry.id);
        let raw_json = serde_json::to_string(&entry.raw)?;
        let old: Option<(String, i64)> = sqlx::query_as(
            "SELECT payload_json,source_sequence FROM oqto_log_raw_envelopes WHERE raw_id=?",
        )
        .bind(&raw_id)
        .fetch_optional(&mut *tx)
        .await?;
        let sequence = i64::try_from(position + 1)?;
        if let Some((payload, old_sequence)) = old {
            ensure!(
                payload == raw_json && old_sequence == sequence,
                "Immutable native source mismatch"
            );
            continue;
        }
        let turn_id = entry
            .message
            .as_ref()
            .map(|_| stable_id("turn", public_id, &entry.id));
        if let Some(message) = &entry.message {
            version += 1;
            let turn = turn_id.as_deref().context("Missing native turn")?;
            let parent = entry
                .parent_message
                .as_deref()
                .map(|id| stable_id("turn", public_id, id));
            let role = normalize_role(&message.role);
            sqlx::query("INSERT INTO oqto_log_turns(turn_id,session_id,branch_id,parent_turn_id,turn_version,role,status,source_kind,source_session_id,source_entry_id,source_hash,source_timestamp,committed_at) VALUES(?,?,?,?,?,?,'committed',?,?,?,?,?,datetime('now'))")
                .bind(turn).bind(public_id).bind(&branch).bind(parent).bind(version).bind(role).bind(SOURCE).bind(&graph.native_id).bind(&entry.id).bind(message_source_hash(message)).bind(message.timestamp.map(|v| v.to_string())).execute(&mut *tx).await?;
            let message_id = stable_id("message", public_id, &entry.id);
            sqlx::query("INSERT INTO oqto_log_messages(message_id,turn_id,seq,kind,role,content,json_payload,source_message_id) VALUES(?,?,0,'message',?,?,?,?)")
                .bind(message_id).bind(turn).bind(role).bind(extract_text(&message.content)).bind(serde_json::to_string(message)?).bind(&entry.id).execute(&mut *tx).await?;
        }
        sqlx::query("INSERT INTO oqto_log_raw_envelopes(raw_id,session_id,turn_id,source,harness,native_type,source_sequence,received_at,payload_json,payload_sha256) VALUES(?,?,?,?,'pi',?,?,datetime('now'),?,?)")
            .bind(raw_id).bind(public_id).bind(turn_id).bind(SOURCE).bind(entry.raw["type"].as_str().context("Native entry type missing")?).bind(sequence).bind(&raw_json).bind(hex::encode(Sha256::digest(raw_json.as_bytes()))).execute(&mut *tx).await?;
    }
    if old_count == i64::try_from(graph.entries.len() + 1)?
        && previous["source"] == SOURCE
        && previous["native_leaf_id"] == serde_json::json!(graph.leaf_id)
    {
        tx.commit().await?;
        return Ok(());
    }
    let revision = previous["revision"]
        .as_u64()
        .unwrap_or(0)
        .checked_add(1)
        .context("Native head revision overflow")?;
    let head = graph
        .head_message
        .as_deref()
        .map(|id| stable_id("turn", public_id, id));
    sqlx::query("UPDATE oqto_log_branches SET head_turn_id=?,extensions_json=? WHERE branch_id=?")
        .bind(head)
        .bind(
            serde_json::json!({"native_leaf_id":graph.leaf_id,"source":SOURCE,"revision":revision})
                .to_string(),
        )
        .bind(&branch)
        .execute(&mut *tx)
        .await?;
    sqlx::query("UPDATE oqto_log_sessions SET extensions_json=json_set(COALESCE(extensions_json,'{}'),'$.pi_native_graph',json(?)),updated_at=datetime('now') WHERE session_id=?")
        .bind(serde_json::json!({"branch_id":branch,"native_leaf_id":graph.leaf_id,"source":SOURCE,"revision":revision}).to_string()).bind(public_id).execute(&mut *tx).await?;
    tx.commit().await.context("Commit native graph merge")?;
    // Best-effort index visibility uses the existing store path, not a second
    // private history authority. The durable transaction above is authoritative.
    Ok(())
}

#[cfg(test)]
mod tests;
