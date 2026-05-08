//! Temporary runner compatibility adapter for oqto-log projections.
//!
//! Authoritative projection logic lives in `oqto_history::oqto_log::projector` and
//! returns neutral protocol DTOs. This module only converts those DTOs to runner
//! wire structs while call sites migrate away from `ChatMessageProto`.

use std::path::Path;

use anyhow::Result;
use oqto_protocol::events::MessageVersion;
use oqto_protocol::projection::{
    ProjectedChatMessage, ProjectedChatMessagePart, ProjectedTurnTreeNode,
};

use crate::protocol::{ChatMessagePartProto, ChatMessageProto};

pub type TurnTreeNode = ProjectedTurnTreeNode;

pub async fn project_session_messages_auto(
    user_home: &Path,
    session_id: &str,
    limit: Option<usize>,
) -> Result<Option<Vec<ChatMessageProto>>> {
    Ok(
        oqto_history::oqto_log::projector::project_session_messages_auto(
            user_home, session_id, limit,
        )
        .await?
        .map(|messages| messages.into_iter().map(projected_to_chat_proto).collect()),
    )
}

pub async fn project_session_tree_auto(
    user_home: &Path,
    session_id: &str,
) -> Result<Option<Vec<TurnTreeNode>>> {
    oqto_history::oqto_log::projector::project_session_tree_auto(user_home, session_id).await
}

pub async fn read_message_version_auto(
    user_home: &Path,
    session_id: &str,
) -> Result<Option<MessageVersion>> {
    oqto_history::oqto_log::projector::read_message_version_auto(user_home, session_id).await
}

#[allow(dead_code)]
pub async fn project_session_messages_for_workspace(
    user_home: &Path,
    workspace_id: &str,
    session_id: &str,
    limit: Option<usize>,
) -> Result<Vec<ChatMessageProto>> {
    Ok(
        oqto_history::oqto_log::projector::project_session_messages_for_workspace(
            user_home,
            workspace_id,
            session_id,
            limit,
        )
        .await?
        .into_iter()
        .map(projected_to_chat_proto)
        .collect(),
    )
}

fn projected_to_chat_proto(message: ProjectedChatMessage) -> ChatMessageProto {
    ChatMessageProto {
        id: message.id,
        session_id: message.session_id,
        role: message.role,
        created_at: message.created_at,
        completed_at: message.completed_at,
        parent_id: message.parent_id,
        model_id: message.model_id,
        provider_id: message.provider_id,
        agent: message.agent,
        summary_title: message.summary_title,
        tokens_input: message.tokens_input,
        tokens_output: message.tokens_output,
        tokens_reasoning: message.tokens_reasoning,
        cost: message.cost,
        client_id: message.client_id,
        parts: message
            .parts
            .into_iter()
            .map(projected_part_to_proto)
            .collect(),
    }
}

fn projected_part_to_proto(part: ProjectedChatMessagePart) -> ChatMessagePartProto {
    ChatMessagePartProto {
        id: part.id,
        part_type: part.part_type,
        text: part.text,
        text_html: part.text_html,
        tool_name: part.tool_name,
        tool_call_id: part.tool_call_id,
        tool_input: part.tool_input,
        tool_output: part.tool_output.map(|value| match value {
            serde_json::Value::String(s) => s,
            other => other.to_string(),
        }),
        tool_status: part.tool_status,
        tool_title: part.tool_title,
    }
}
