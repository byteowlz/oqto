use anyhow::Result;

use crate::hstry::HstryClient;

/// Return the workspace path for a hstry conversation, if present.
pub async fn get_session_workspace_via_grpc(
    client: &HstryClient,
    session_id: &str,
) -> Result<Option<String>> {
    let conv = client.get_conversation(session_id, None).await?;
    Ok(conv.and_then(|c| c.workspace).filter(|w| !w.is_empty()))
}
