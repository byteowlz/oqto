//! Compatibility projection DTOs derived from canonical timeline.
//!
//! These types are neutral protocol shapes used by storage/projection crates. Runtime
//! wire structs such as `ChatMessageProto` may convert to/from these during migration,
//! but must not be treated as the canonical durable model.

use serde::{Deserialize, Serialize};

/// A controller-supplied ceiling, never permission by itself. A raw Runner
/// connection must already be authorized for the owning OS principal.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MediaWorkspaceScope {
    Owned,
    Exact { path: std::path::PathBuf },
    Within { root: std::path::PathBuf },
}
impl MediaWorkspaceScope {
    pub fn permits(&self, workspace: Option<&str>) -> bool {
        let valid = |path: &std::path::Path| {
            path.is_absolute()
                && !path
                    .components()
                    .any(|part| matches!(part, std::path::Component::ParentDir))
        };
        match self {
            Self::Owned => true,
            Self::Exact { path } => workspace
                .map(std::path::Path::new)
                .is_some_and(|workspace| valid(path) && valid(workspace) && workspace == path),
            Self::Within { root } => workspace
                .map(std::path::Path::new)
                .is_some_and(|workspace| {
                    valid(root) && valid(workspace) && workspace.starts_with(root)
                }),
        }
    }
}

/// One page of projected chat messages, oldest-first, ending at the newest
/// message not excluded by the request cursor.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectedChatMessagePage {
    /// Messages in chronological order.
    pub messages: Vec<ProjectedChatMessage>,
    /// True when older messages exist before this page.
    #[serde(default)]
    pub has_more: bool,
    /// Opaque cursor selecting messages strictly older than this page.
    /// Pass back as `before` to fetch the previous page.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_before: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectedChatMessage {
    pub id: String,
    pub session_id: String,
    pub role: String,
    pub created_at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary_title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tokens_input: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tokens_output: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tokens_reasoning: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_id: Option<String>,
    pub parts: Vec<ProjectedChatMessagePart>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectedChatMessagePart {
    pub id: String,
    pub part_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_html: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_input: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_output: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_status: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_title: Option<String>,
    /// Derived image metadata only; original bytes remain in the selected store.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub media_ref: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectedTurnTreeNode {
    pub turn_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_turn_id: Option<String>,
    pub branch_id: String,
    pub role: String,
    pub turn_version: i64,
}

#[cfg(test)]
mod tests {
    use super::{ProjectedChatMessage, ProjectedChatMessagePart};

    #[test]
    fn media_workspace_scope_is_component_scoped_and_rejects_parent_paths() {
        use super::MediaWorkspaceScope;
        let scope = MediaWorkspaceScope::Within {
            root: "/work/project".into(),
        };
        assert!(scope.permits(Some("/work/project/images")));
        assert!(!scope.permits(Some("/work/project-foreign/images")));
        assert!(!scope.permits(Some("/work/project/../private")));
        assert!(!scope.permits(None));
        assert!(
            !MediaWorkspaceScope::Exact {
                path: "/work/project".into()
            }
            .permits(Some("/work/project/images"))
        );
        assert!(
            !MediaWorkspaceScope::Within {
                root: "relative".into()
            }
            .permits(Some("/work/project"))
        );
    }

    #[test]
    fn projected_chat_part_preserves_structured_tool_output() {
        let part = ProjectedChatMessagePart {
            id: "part-1".to_string(),
            part_type: "tool_result".to_string(),
            text: None,
            text_html: None,
            tool_name: Some("bash".to_string()),
            tool_call_id: Some("call-1".to_string()),
            tool_input: None,
            tool_output: Some(serde_json::json!({ "stdout": "ok", "code": 0 })),
            tool_status: Some("success".to_string()),
            tool_title: None,
            media_ref: None,
        };

        let json = serde_json::to_string(&part).expect("serialize projected part");
        let parsed: ProjectedChatMessagePart =
            serde_json::from_str(&json).expect("deserialize projected part");

        assert_eq!(
            parsed.tool_output,
            Some(serde_json::json!({ "stdout": "ok", "code": 0 }))
        );
    }

    #[test]
    fn projected_chat_message_accepts_legacy_string_tool_output() {
        let message: ProjectedChatMessage = serde_json::from_value(serde_json::json!({
            "id": "msg-1",
            "session_id": "session-1",
            "role": "assistant",
            "created_at": 42,
            "parts": [{
                "id": "part-1",
                "part_type": "tool_result",
                "tool_name": "bash",
                "tool_call_id": "call-1",
                "tool_output": "plain output",
                "tool_status": "success"
            }]
        }))
        .expect("deserialize projected chat message");

        assert_eq!(
            message.parts[0].tool_output,
            Some(serde_json::json!("plain output"))
        );
    }
}
