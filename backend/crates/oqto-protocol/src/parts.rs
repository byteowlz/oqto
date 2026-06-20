//! Oqto-owned structured message parts.
//!
//! These are the canonical content units used by Oqto protocol messages and
//! oqto-log projections. The JSON wire shape intentionally remains stable, but
//! Oqto-owned canonical message part types.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Tool execution status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ToolStatus {
    #[default]
    Pending,
    Running,
    Success,
    Error,
}

impl ToolStatus {
    /// Parse from common status strings.
    pub fn parse(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "running" | "in_progress" | "executing" => Self::Running,
            "success" | "completed" | "done" | "ok" => Self::Success,
            "error" | "failed" | "failure" => Self::Error,
            _ => Self::Pending,
        }
    }
}

/// Source for media content (images, audio, video, files).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "source", rename_all = "camelCase")]
pub enum MediaSource {
    Url {
        url: String,
        #[serde(rename = "mimeType", default, skip_serializing_if = "Option::is_none")]
        mime_type: Option<String>,
    },
    AttachmentRef {
        #[serde(rename = "attachmentId")]
        attachment_id: String,
        #[serde(rename = "mimeType", default, skip_serializing_if = "Option::is_none")]
        mime_type: Option<String>,
    },
    Base64 {
        data: String,
        #[serde(rename = "mimeType")]
        mime_type: String,
    },
}

impl MediaSource {
    pub fn url(url: impl Into<String>) -> Self {
        Self::Url {
            url: url.into(),
            mime_type: None,
        }
    }

    pub fn attachment_ref(attachment_id: impl Into<String>, mime_type: Option<String>) -> Self {
        Self::AttachmentRef {
            attachment_id: attachment_id.into(),
            mime_type,
        }
    }

    pub fn base64(data: impl Into<String>, mime_type: impl Into<String>) -> Self {
        Self::Base64 {
            data: data.into(),
            mime_type: mime_type.into(),
        }
    }

    pub fn mime_type(&self) -> Option<&str> {
        match self {
            Self::Url { mime_type, .. } | Self::AttachmentRef { mime_type, .. } => {
                mime_type.as_deref()
            }
            Self::Base64 { mime_type, .. } => Some(mime_type),
        }
    }

    pub fn is_attachment_ref(&self) -> bool {
        matches!(self, Self::AttachmentRef { .. })
    }

    pub fn attachment_id(&self) -> Option<&str> {
        match self {
            Self::AttachmentRef { attachment_id, .. } => Some(attachment_id),
            _ => None,
        }
    }
}

/// A range within a file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileRange {
    #[serde(rename = "startLine", skip_serializing_if = "Option::is_none")]
    pub start_line: Option<u32>,
    #[serde(rename = "endLine", skip_serializing_if = "Option::is_none")]
    pub end_line: Option<u32>,
}

/// A content part within a message.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Part {
    Text {
        id: String,
        text: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        format: Option<String>,
    },
    Thinking {
        id: String,
        text: String,
    },
    ToolCall {
        id: String,
        #[serde(rename = "toolCallId")]
        tool_call_id: String,
        name: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        input: Option<Value>,
        #[serde(default, skip_serializing_if = "is_default_status")]
        status: ToolStatus,
    },
    ToolResult {
        id: String,
        #[serde(rename = "toolCallId")]
        tool_call_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        name: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        output: Option<Value>,
        #[serde(rename = "isError", default)]
        is_error: bool,
        #[serde(
            rename = "durationMs",
            default,
            skip_serializing_if = "Option::is_none"
        )]
        duration_ms: Option<u64>,
    },
    FileRef {
        id: String,
        uri: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        label: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        range: Option<FileRange>,
    },
    Image {
        id: String,
        #[serde(flatten)]
        source: MediaSource,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        alt: Option<String>,
    },
    Audio {
        id: String,
        #[serde(flatten)]
        source: MediaSource,
        #[serde(
            rename = "durationSec",
            default,
            skip_serializing_if = "Option::is_none"
        )]
        duration_sec: Option<f64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        transcript: Option<String>,
    },
    Video {
        id: String,
        #[serde(flatten)]
        source: MediaSource,
        #[serde(
            rename = "durationSec",
            default,
            skip_serializing_if = "Option::is_none"
        )]
        duration_sec: Option<f64>,
    },
    Attachment {
        id: String,
        #[serde(flatten)]
        source: MediaSource,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        filename: Option<String>,
        #[serde(rename = "sizeBytes", default, skip_serializing_if = "Option::is_none")]
        size_bytes: Option<u64>,
    },
}

#[expect(
    clippy::trivially_copy_pass_by_ref,
    reason = "serde skip_serializing_if requires fn(&T) for field predicates"
)]
fn is_default_status(s: &ToolStatus) -> bool {
    *s == ToolStatus::Pending
}

impl Part {
    pub fn text(text: impl Into<String>) -> Self {
        Self::Text {
            id: generate_id(),
            text: text.into(),
            format: None,
        }
    }

    pub fn thinking(text: impl Into<String>) -> Self {
        Self::Thinking {
            id: generate_id(),
            text: text.into(),
        }
    }

    pub fn tool_call(
        tool_call_id: impl Into<String>,
        name: impl Into<String>,
        input: Option<Value>,
    ) -> Self {
        Self::ToolCall {
            id: generate_id(),
            tool_call_id: tool_call_id.into(),
            name: name.into(),
            input,
            status: ToolStatus::Pending,
        }
    }

    pub fn tool_result(
        tool_call_id: impl Into<String>,
        output: Option<Value>,
        is_error: bool,
    ) -> Self {
        Self::ToolResult {
            id: generate_id(),
            tool_call_id: tool_call_id.into(),
            name: None,
            output,
            is_error,
            duration_ms: None,
        }
    }

    pub fn id(&self) -> &str {
        match self {
            Self::Text { id, .. }
            | Self::Thinking { id, .. }
            | Self::ToolCall { id, .. }
            | Self::ToolResult { id, .. }
            | Self::FileRef { id, .. }
            | Self::Image { id, .. }
            | Self::Audio { id, .. }
            | Self::Video { id, .. }
            | Self::Attachment { id, .. } => id,
        }
    }

    pub fn text_content(&self) -> Option<&str> {
        match self {
            Self::Text { text, .. } | Self::Thinking { text, .. } => Some(text),
            Self::ToolResult { output, .. } => output.as_ref().and_then(|v| v.as_str()),
            Self::Audio { transcript, .. } => transcript.as_deref(),
            _ => None,
        }
    }
}

fn generate_id() -> String {
    format!("part_{}", uuid::Uuid::new_v4().simple())
}

/// The type of entity that sent a message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SenderType {
    User,
    Agent,
    System,
}

impl std::fmt::Display for SenderType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::User => write!(f, "user"),
            Self::Agent => write!(f, "agent"),
            Self::System => write!(f, "system"),
        }
    }
}

impl From<&str> for SenderType {
    fn from(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "agent" | "assistant" | "ai" | "bot" => Self::Agent,
            "system" => Self::System,
            _ => Self::User,
        }
    }
}

/// Attribution for a message sender.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sender {
    #[serde(rename = "type")]
    pub sender_type: SenderType,
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runner_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
}
