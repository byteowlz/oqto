use super::*;
use serde_json::json;

const NATIVE: &str = "synthetic-provider-errors";
const WORKSPACE: &str = "/synthetic";
const FAILURE: &str = "Synthetic subscription request failed.";

fn source() -> Vec<u8> {
    let messages = [
        json!({"role":"assistant","content":[],"stopReason":"error","errorMessage":FAILURE}),
        json!({"role":"assistant","content":[],"stopReason":"error","errorMessage":FAILURE}),
        json!({"role":"assistant","content":[],"stopReason":"error","errorMessage":FAILURE}),
        json!({"role":"assistant","content":[],"stopReason":"stop","errorMessage":"stale metadata is not a failure"}),
        json!({"role":"assistant","content":null,"stopReason":"stop"}),
        json!({"role":"assistant","content":"[]","stopReason":"stop"}),
        json!({"role":"toolResult","content":[],"toolCallId":"synthetic-call","isError":false}),
        json!({"role":"assistant","content":[{"type":"text","text":"Partial synthetic reply."}],"stopReason":"error","errorMessage":FAILURE}),
    ];
    let mut lines = vec![
        json!({"type":"session","id":NATIVE,"cwd":WORKSPACE}),
        json!({"type":"message","id":"request","parentId":null,"message":{"role":"user","content":"Synthetic request.","timestamp":1_699_999_999_999_u64}}),
    ];
    let mut parent = json!("request");
    for (index, mut message) in messages.into_iter().enumerate() {
        message["timestamp"] = json!(1_700_000_000_000_u64 + index as u64);
        let id = format!("entry-{index}");
        lines.push(json!({"type":"message","id":id,"parentId":parent,"message":message}));
        parent = json!(id);
    }
    lines
        .into_iter()
        .map(|line| format!("{line}\n"))
        .collect::<String>()
        .into_bytes()
}

fn assert_errors(page: &ProjectedChatMessagePage) {
    assert_eq!(
        page.messages.len(),
        8,
        "do not drop records or alter paging identity"
    );
    for message in &page.messages[..3] {
        assert_eq!(message.role, "assistant");
        assert_eq!(message.parts.len(), 1);
        assert_eq!(message.parts[0].part_type, "error");
        assert_eq!(message.parts[0].text.as_deref(), Some(FAILURE));
        assert_eq!(message.parts[0].id, format!("{}:part:0", message.id));
    }
    for message in &page.messages[3..5] {
        assert!(
            message.parts.is_empty(),
            "empty successful content is not JSON text or an error"
        );
    }
    assert_eq!(page.messages[5].parts[0].part_type, "text");
    assert_eq!(page.messages[5].parts[0].text.as_deref(), Some("[]"));
    assert_eq!(page.messages[6].parts[0].part_type, "tool_result");
    assert_eq!(page.messages[6].parts[0].tool_output, Some(json!([])));
    assert_eq!(
        page.messages[6].parts[0].tool_status.as_deref(),
        Some("success")
    );
    let partial = &page.messages[7];
    assert!(
        partial
            .parts
            .iter()
            .any(|p| p.text.as_deref() == Some("Partial synthetic reply."))
    );
    assert!(
        partial
            .parts
            .iter()
            .any(|p| p.part_type == "error" && p.text.as_deref() == Some(FAILURE))
    );
    let ids: std::collections::HashSet<_> = partial.parts.iter().map(|p| &p.id).collect();
    assert_eq!(
        ids.len(),
        partial.parts.len(),
        "error details need a unique stable part id"
    );
}

#[test]
fn native_history_preserves_provider_failure_instead_of_serializing_empty_content() -> Result<()> {
    let bytes = source();
    let mut file = Cursor::new(&bytes);
    let index = NativeFileIndex::read(&mut file, bytes.len() as u64, NATIVE, WORKSPACE)?;
    let page = index.page(&mut file, "public", Some("entry-7"), 8, None)?;
    assert_errors(&page);
    assert!(page.has_more && page.next_before.is_some());
    let repeat = index.page(&mut file, "public", Some("entry-7"), 8, None)?;
    assert_eq!(serde_json::to_value(page)?, serde_json::to_value(repeat)?);
    Ok(())
}

#[tokio::test]
async fn native_and_stored_error_pages_preserve_the_same_typed_failure_and_ids() -> Result<()> {
    let home = tempfile::tempdir()?;
    let bytes = source();
    let public = crate::oqto_log::store::platform_id_for_external_id(NATIVE);
    let graph = NativeGraph::parse(&bytes, NATIVE, WORKSPACE, Some("entry-7"))?;
    crate::oqto_log::store::native_graph::merge(home.path(), "synthetic-owner", &public, &graph)
        .await?;
    let stored = crate::oqto_log::projector::project_session_messages_page_for_workspace(
        home.path(),
        WORKSPACE,
        &public,
        8,
        None,
    )
    .await?
    .context("stored synthetic page")?;
    assert_errors(&stored);
    let mut file = Cursor::new(&bytes);
    let index = NativeFileIndex::read(&mut file, bytes.len() as u64, NATIVE, WORKSPACE)?;
    let native = index.page(&mut file, &public, Some("entry-7"), 8, None)?;
    assert_eq!(
        serde_json::to_value(native.messages)?,
        serde_json::to_value(stored.messages)?
    );
    Ok(())
}

#[test]
fn error_metadata_aliases_and_missing_detail_never_become_empty_json_success() -> Result<()> {
    for metadata in [
        json!({"errorMessage":FAILURE}),
        json!({"errorMessage":" ","message":FAILURE}),
        json!({"error":FAILURE}),
        json!({"error":{"message":FAILURE}}),
        json!({}),
    ] {
        let mut message = json!({"role":"assistant","content":[],"stopReason":"error"});
        message
            .as_object_mut()
            .context("synthetic message")?
            .extend(metadata.as_object().context("synthetic metadata")?.clone());
        let bytes = format!(
            "{}\n{}\n",
            json!({"type":"session","id":NATIVE,"cwd":WORKSPACE}),
            json!({"type":"message","id":"failure","parentId":null,"message":message})
        )
        .into_bytes();
        let mut file = Cursor::new(&bytes);
        let index = NativeFileIndex::read(&mut file, bytes.len() as u64, NATIVE, WORKSPACE)?;
        let page = index.page(&mut file, "public", Some("failure"), 1, None)?;
        assert_eq!(page.messages.len(), 1);
        assert_eq!(page.messages[0].parts.len(), 1);
        let part = &page.messages[0].parts[0];
        assert_eq!(part.part_type, "error");
        assert_eq!(
            part.text.as_deref(),
            Some(if metadata.as_object().context("metadata")?.is_empty() {
                "Assistant response failed."
            } else {
                FAILURE
            })
        );
    }
    Ok(())
}
