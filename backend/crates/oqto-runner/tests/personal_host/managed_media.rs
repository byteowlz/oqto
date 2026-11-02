//! Actual Runner transport with owned canonical snapshots; no Pi/model is run.
use super::*;
use oqto_runner::{
    client::RunnerClient,
    protocol::{
        GetWorkspaceSessionMediaRequest, WorkspaceChatMessagesSource, WorkspaceMediaVariant,
    },
};
#[tokio::test]
async fn managed_history_images_use_refs_and_scoped_canonical_originals() -> Result<()> {
    use sha2::{Digest, Sha256};
    let host = Host::start_mode(true).await?;
    let client = RunnerClient::new(host.home.join("primary.sock"));
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if tokio::net::UnixStream::connect(host.home.join("primary.sock"))
                .await
                .is_ok()
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await?;
    let native = "managed-image-native";
    let public = oqto_history::oqto_log::store::platform_id_for_external_id(native);
    let mut png = std::io::Cursor::new(Vec::new());
    image::DynamicImage::new_rgb8(1200, 900).write_to(&mut png, image::ImageFormat::Png)?;
    let mut original = png.into_inner();
    original.resize(1024 * 1024, 0);
    let hash = hex::encode(Sha256::digest(&original));
    let mut messages = Vec::new();
    for index in 0..3 {
        messages.push(serde_json::from_value::<oqto_pi::AgentMessage>(serde_json::json!({"role":"toolResult","toolCallId":format!("image-{index}"),"toolName":"screenshot","timestamp":1740000000000u64+index,"content":[{"type":"text","text":"screenshot fixture"},{"type":"image","mimeType":"image/png","data":STANDARD.encode(&original)}]}))?);
    }
    messages.push(serde_json::from_value::<oqto_pi::AgentMessage>(serde_json::json!({"role":"user","timestamp":1740000000003u64,"content":[{"type":"text","text":"user image fixture"},{"type":"image","source":{"type":"base64","mediaType":"image/png","data":STANDARD.encode(&original)}}]}))?);
    oqto_history::oqto_log::store::append_agent_end_snapshot(
        &host.home,
        "fixture",
        host.work.to_str().context("workspace")?,
        &public,
        &public,
        Some(native),
        native,
        &messages,
    )
    .await?;
    let history = client
        .get_workspace_chat_session_messages_paged(
            &public,
            false,
            Some(20),
            None,
            WorkspaceChatMessagesSource::Authoritative,
        )
        .await?;
    let json = serde_json::to_value(&history)?;
    assert_eq!(history.messages.len(), 4);
    let user = history
        .messages
        .iter()
        .find(|m| m.role == "user")
        .context("user message")?;
    let image = user
        .parts
        .iter()
        .find(|p| p.part_type == "image")
        .context("user image part")?;
    assert_eq!(
        image.media_ref.as_ref().context("user image ref")?["session_id"],
        public
    );
    assert_eq!(
        image.media_ref.as_ref().context("user image ref")?["sha256"],
        hash
    );
    assert!(serde_json::to_vec(&history)?.len() < 16 * 1024);
    assert!(!serde_json::to_string(&json)?.contains("data_base64"));
    assert_eq!(
        json["messages"][0]["parts"][0]["tool_output"][1]["media_ref"]["session_id"],
        public
    );
    let request = |session: &str, hash: &str, variant| GetWorkspaceSessionMediaRequest {
        session_id: session.into(),
        workspace_scope: oqto_protocol::projection::MediaWorkspaceScope::Owned,
        sha256: hash.into(),
        variant,
    };
    let full = client
        .get_workspace_session_media(request(&public, &hash, WorkspaceMediaVariant::Full))
        .await?;
    assert_eq!(
        STANDARD.decode(
            full.media["data_base64"]
                .as_str()
                .context("original bytes")?
        )?,
        original
    );
    let thumb = client
        .get_workspace_session_media(request(&public, &hash, WorkspaceMediaVariant::Thumb))
        .await?;
    assert!(serde_json::to_vec(&thumb)?.len() < 192 * 1024);
    assert_eq!(
        thumb.media,
        client
            .get_workspace_session_media(request(&public, &hash, WorkspaceMediaVariant::Thumb))
            .await?
            .media
    );
    assert!(
        client
            .get_workspace_session_media(request(native, &hash, WorkspaceMediaVariant::Full))
            .await
            .is_err(),
        "native id must not alias public id"
    );
    assert!(
        client
            .get_workspace_session_media(request(
                "foreign-session",
                &hash,
                WorkspaceMediaVariant::Thumb
            ))
            .await
            .is_err()
    );
    assert!(
        client
            .get_workspace_session_media(request(
                &public,
                &"a".repeat(64),
                WorkspaceMediaVariant::Full
            ))
            .await
            .is_err()
    );
    let mut denied = request(&public, &hash, WorkspaceMediaVariant::Thumb);
    denied.workspace_scope = oqto_protocol::projection::MediaWorkspaceScope::Within {
        root: host.home.join("not-the-workspace"),
    };
    assert!(
        client.get_workspace_session_media(denied).await.is_err(),
        "warm cache must not bypass the Workspace ceiling"
    );
    let persisted = oqto_history::oqto_log::ops::media::original(
        &host.home,
        &public,
        &hash,
        &oqto_protocol::projection::MediaWorkspaceScope::Owned,
    )
    .await?;
    assert_eq!(
        persisted.bytes(),
        original.as_slice(),
        "wire projection must not rewrite canonical originals"
    );
    host.stop().await?;
    Ok(())
}
