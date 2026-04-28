//! Canonical message types aligned with hstry format.
//!
//! See: https://github.com/byteowlz/hstry/blob/main/adapters/types/index.ts

use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

// ============================================================================
// Message Role
// ============================================================================

/// Canonical message role.
///
/// Maps from various agent-specific terms:
/// - `user`, `human` -> `User`
/// - `assistant`, `agent`, `ai`, `bot` -> `Assistant`
/// - `system` -> `System`
/// - `tool`, `function`, `toolResult` -> `Tool`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS, Default)]
#[serde(rename_all = "lowercase")]
#[ts(export, export_to = "../../../../frontend/src/generated/")]
pub enum MessageRole {
    #[default]
    User,
    Assistant,
    System,
    Tool,
}

impl MessageRole {
    /// Parse a role string from any agent format.
    pub fn parse(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "user" | "human" => Self::User,
            "assistant" | "agent" | "ai" | "bot" => Self::Assistant,
            "system" => Self::System,
            "tool" | "function" | "toolresult" | "tool_result" => Self::Tool,
            _ => Self::User, // Default fallback
        }
    }
}

impl std::fmt::Display for MessageRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::User => write!(f, "user"),
            Self::Assistant => write!(f, "assistant"),
            Self::System => write!(f, "system"),
            Self::Tool => write!(f, "tool"),
        }
    }
}

// ============================================================================
// Canonical Parts
// ============================================================================

/// A unique identifier for a content part.
pub type PartId = String;

/// Text format hint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS, Default)]
#[serde(rename_all = "lowercase")]
#[ts(export, export_to = "../../../../frontend/src/generated/")]
pub enum TextFormat {
    #[default]
    Markdown,
    Plain,
}

/// Thinking visibility hint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS, Default)]
#[serde(rename_all = "lowercase")]
#[ts(export, export_to = "../../../../frontend/src/generated/")]
pub enum ThinkingVisibility {
    #[default]
    Ui,
    Hidden,
}

/// Tool call/result status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS, Default)]
#[serde(rename_all = "lowercase")]
#[ts(export, export_to = "../../../../frontend/src/generated/")]
pub enum ToolStatus {
    #[default]
    Pending,
    Running,
    Success,
    Error,
}

impl ToolStatus {
    /// Parse from various agent status strings.
    pub fn parse(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "pending" | "queued" => Self::Pending,
            "running" | "in_progress" | "executing" => Self::Running,
            "success" | "completed" | "done" | "ok" => Self::Success,
            "error" | "failed" | "failure" => Self::Error,
            _ => Self::Pending,
        }
    }
}

/// A range within a file (for citations and file references).
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../../frontend/src/generated/")]
pub struct FileRange {
    /// Start line (1-indexed).
    #[serde(rename = "startLine", skip_serializing_if = "Option::is_none")]
    pub start_line: Option<u32>,
    /// End line (1-indexed, inclusive).
    #[serde(rename = "endLine", skip_serializing_if = "Option::is_none")]
    pub end_line: Option<u32>,
    /// Start column (0-indexed).
    #[serde(rename = "startCol", skip_serializing_if = "Option::is_none")]
    pub start_col: Option<u32>,
    /// End column (0-indexed).
    #[serde(rename = "endCol", skip_serializing_if = "Option::is_none")]
    pub end_col: Option<u32>,
}

// ============================================================================
// Media Types (for binary data: images, audio, video, attachments)
// ============================================================================

/// Source for media content (images, audio, video, files).
///
/// Supports three modes:
/// - **URL**: External or internal URL (http://, https://, file://)
/// - **Data URI**: Inline base64 with MIME type (data:image/png;base64,...)
/// - **Base64**: Raw base64 data with separate MIME type field
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../../frontend/src/generated/")]
#[serde(untagged)]
pub enum MediaSource {
    /// URL reference (most common for large files).
    Url {
        /// URL to the media file.
        url: String,
        /// MIME type (optional, can be inferred from URL or content).
        #[serde(rename = "mimeType", default, skip_serializing_if = "Option::is_none")]
        mime_type: Option<String>,
    },
    /// Base64-encoded inline data.
    Base64 {
        /// Base64-encoded content.
        data: String,
        /// MIME type (required for base64).
        #[serde(rename = "mimeType")]
        mime_type: String,
    },
}

impl MediaSource {
    /// Create a URL source.
    pub fn url(url: impl Into<String>) -> Self {
        Self::Url {
            url: url.into(),
            mime_type: None,
        }
    }

    /// Create a URL source with explicit MIME type.
    pub fn url_with_mime(url: impl Into<String>, mime_type: impl Into<String>) -> Self {
        Self::Url {
            url: url.into(),
            mime_type: Some(mime_type.into()),
        }
    }

    /// Create a base64 source.
    pub fn base64(data: impl Into<String>, mime_type: impl Into<String>) -> Self {
        Self::Base64 {
            data: data.into(),
            mime_type: mime_type.into(),
        }
    }

    /// Create a data URI from base64 data.
    pub fn data_uri(data: impl Into<String>, mime_type: impl Into<String>) -> Self {
        let mime = mime_type.into();
        let data = data.into();
        Self::Url {
            url: format!("data:{};base64,{}", mime, data),
            mime_type: Some(mime),
        }
    }

    /// Get the MIME type if known.
    pub fn mime_type(&self) -> Option<&str> {
        match self {
            Self::Url { mime_type, .. } => mime_type.as_deref(),
            Self::Base64 { mime_type, .. } => Some(mime_type),
        }
    }

    /// Check if this is inline data (base64 or data URI).
    pub fn is_inline(&self) -> bool {
        match self {
            Self::Base64 { .. } => true,
            Self::Url { url, .. } => url.starts_with("data:"),
        }
    }

    /// Estimate size in bytes (for base64 data).
    pub fn estimated_size(&self) -> Option<usize> {
        match self {
            Self::Base64 { data, .. } => Some(data.len() * 3 / 4), // Base64 is ~4/3 of original
            Self::Url { url, .. } if url.starts_with("data:") => {
                // Extract base64 portion from data URI
                url.find(",").map(|i| (url.len() - i - 1) * 3 / 4)
            }
            Self::Url { .. } => None,
        }
    }
}

/// Dimensions for image/video content.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../../frontend/src/generated/")]
pub struct MediaDimensions {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
}

impl MediaDimensions {
    pub fn new(width: u32, height: u32) -> Self {
        Self { width, height }
    }

    /// Aspect ratio (width / height).
    pub fn aspect_ratio(&self) -> f64 {
        self.width as f64 / self.height as f64
    }
}

/// Common MIME types for media content.
pub mod mime_types {
    // Images
    pub const PNG: &str = "image/png";
    pub const JPEG: &str = "image/jpeg";
    pub const GIF: &str = "image/gif";
    pub const WEBP: &str = "image/webp";
    pub const SVG: &str = "image/svg+xml";

    // Audio
    pub const MP3: &str = "audio/mpeg";
    pub const WAV: &str = "audio/wav";
    pub const OGG_AUDIO: &str = "audio/ogg";
    pub const WEBM_AUDIO: &str = "audio/webm";
    pub const AAC: &str = "audio/aac";
    pub const FLAC: &str = "audio/flac";

    // Video
    pub const MP4: &str = "video/mp4";
    pub const WEBM_VIDEO: &str = "video/webm";
    pub const OGG_VIDEO: &str = "video/ogg";
    pub const MOV: &str = "video/quicktime";

    // Documents
    pub const PDF: &str = "application/pdf";
    pub const JSON: &str = "application/json";
    pub const ZIP: &str = "application/zip";
    pub const OCTET_STREAM: &str = "application/octet-stream";

    /// Infer MIME type from file extension.
    pub fn from_extension(ext: &str) -> Option<&'static str> {
        match ext.to_lowercase().as_str() {
            "png" => Some(PNG),
            "jpg" | "jpeg" => Some(JPEG),
            "gif" => Some(GIF),
            "webp" => Some(WEBP),
            "svg" => Some(SVG),
            "mp3" => Some(MP3),
            "wav" => Some(WAV),
            "ogg" => Some(OGG_AUDIO),
            "aac" => Some(AAC),
            "flac" => Some(FLAC),
            "mp4" | "m4v" => Some(MP4),
            "webm" => Some(WEBM_VIDEO),
            "mov" => Some(MOV),
            "pdf" => Some(PDF),
            "json" => Some(JSON),
            "zip" => Some(ZIP),
            _ => None,
        }
    }

    /// Check if MIME type is an image.
    pub fn is_image(mime: &str) -> bool {
        mime.starts_with("image/")
    }

    /// Check if MIME type is audio.
    pub fn is_audio(mime: &str) -> bool {
        mime.starts_with("audio/")
    }

    /// Check if MIME type is video.
    pub fn is_video(mime: &str) -> bool {
        mime.starts_with("video/")
    }
}

/// Canonical content part - the building block of messages.
///
/// This enum covers all content types used by various AI agents:
/// - Text content (plain or markdown)
/// - Thinking/reasoning (chain-of-thought)
/// - Tool calls and results
/// - File references and citations
/// - Extension types for agent-specific content
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "snake_case")]
#[ts(export, export_to = "../../../../frontend/src/generated/")]
pub enum CanonPart {
    /// Plain text or markdown content.
    Text {
        /// Unique part ID.
        id: PartId,
        /// The text content.
        text: String,
        /// Format hint (markdown or plain).
        #[serde(default, skip_serializing_if = "is_default_format")]
        format: TextFormat,
        /// Additional metadata.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        meta: Option<Value>,
    },

    /// Thinking/reasoning content (chain-of-thought).
    Thinking {
        /// Unique part ID.
        id: PartId,
        /// The thinking content.
        text: String,
        /// Whether to show in UI or keep hidden.
        #[serde(default, skip_serializing_if = "is_default_visibility")]
        visibility: ThinkingVisibility,
        /// Additional metadata.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        meta: Option<Value>,
    },

    /// A tool call (request to execute a tool).
    ToolCall {
        /// Unique part ID.
        id: PartId,
        /// Tool call ID for correlation with result.
        #[serde(rename = "toolCallId")]
        tool_call_id: String,
        /// Tool name.
        name: String,
        /// Tool input parameters.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        input: Option<Value>,
        /// Execution status.
        #[serde(default, skip_serializing_if = "is_default_status")]
        status: ToolStatus,
        /// Additional metadata.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        meta: Option<Value>,
    },

    /// A tool result (output from executing a tool).
    ToolResult {
        /// Unique part ID.
        id: PartId,
        /// Tool call ID this result corresponds to.
        #[serde(rename = "toolCallId")]
        tool_call_id: String,
        /// Tool name (may be omitted if obvious from context).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        name: Option<String>,
        /// Tool output.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        output: Option<Value>,
        /// Whether the tool execution resulted in an error.
        #[serde(rename = "isError", default)]
        is_error: bool,
        /// Human-readable summary/title.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        title: Option<String>,
        /// Execution duration in milliseconds.
        #[serde(
            rename = "durationMs",
            default,
            skip_serializing_if = "Option::is_none"
        )]
        duration_ms: Option<u64>,
        /// Additional metadata.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        meta: Option<Value>,
    },

    /// A file reference (pointer to a file in the workspace).
    FileRef {
        /// Unique part ID.
        id: PartId,
        /// URI or path to the file.
        uri: String,
        /// Human-readable label.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        label: Option<String>,
        /// Optional range within the file.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        range: Option<FileRange>,
        /// Original text that was replaced by this reference.
        #[serde(
            rename = "originText",
            default,
            skip_serializing_if = "Option::is_none"
        )]
        origin_text: Option<String>,
        /// Additional metadata.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        meta: Option<Value>,
    },

    /// A citation (reference to external content).
    Citation {
        /// Unique part ID.
        id: PartId,
        /// Citation label.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        label: Option<String>,
        /// Target URI.
        #[serde(rename = "targetUri", default, skip_serializing_if = "Option::is_none")]
        target_uri: Option<String>,
        /// Optional range within the target.
        #[serde(
            rename = "targetRange",
            default,
            skip_serializing_if = "Option::is_none"
        )]
        target_range: Option<FileRange>,
        /// Original text being cited.
        #[serde(
            rename = "originText",
            default,
            skip_serializing_if = "Option::is_none"
        )]
        origin_text: Option<String>,
        /// Additional metadata.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        meta: Option<Value>,
    },

    /// Image content (PNG, JPEG, GIF, WebP, SVG).
    Image {
        /// Unique part ID.
        id: PartId,
        /// Image source - URL, data URI, or base64.
        #[serde(flatten)]
        source: MediaSource,
        /// Alt text for accessibility.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        alt: Option<String>,
        /// Image dimensions.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        dimensions: Option<MediaDimensions>,
        /// Additional metadata.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        meta: Option<Value>,
    },

    /// Audio content (MP3, WAV, OGG, WebM audio, etc.).
    Audio {
        /// Unique part ID.
        id: PartId,
        /// Audio source - URL, data URI, or base64.
        #[serde(flatten)]
        source: MediaSource,
        /// Duration in seconds.
        #[serde(
            rename = "durationSec",
            default,
            skip_serializing_if = "Option::is_none"
        )]
        duration_sec: Option<f64>,
        /// Transcript of the audio (if available).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        transcript: Option<String>,
        /// Additional metadata (sample rate, channels, etc.).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        meta: Option<Value>,
    },

    /// Video content (MP4, WebM, etc.).
    Video {
        /// Unique part ID.
        id: PartId,
        /// Video source - URL, data URI, or base64.
        #[serde(flatten)]
        source: MediaSource,
        /// Duration in seconds.
        #[serde(
            rename = "durationSec",
            default,
            skip_serializing_if = "Option::is_none"
        )]
        duration_sec: Option<f64>,
        /// Video dimensions.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        dimensions: Option<MediaDimensions>,
        /// Thumbnail image URL.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        thumbnail: Option<String>,
        /// Additional metadata.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        meta: Option<Value>,
    },

    /// Generic binary file attachment.
    Attachment {
        /// Unique part ID.
        id: PartId,
        /// File source - URL, data URI, or base64.
        #[serde(flatten)]
        source: MediaSource,
        /// Original filename.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        filename: Option<String>,
        /// File size in bytes.
        #[serde(rename = "sizeBytes", default, skip_serializing_if = "Option::is_none")]
        size_bytes: Option<u64>,
        /// Additional metadata.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        meta: Option<Value>,
    },

    /// Extension part for agent-specific content.
    /// Type must start with "x-" (e.g., "x-pi-compaction", "x-claude-artifact").
    #[serde(untagged)]
    Extension {
        /// Part type (must start with "x-").
        #[serde(rename = "type")]
        part_type: String,
        /// Unique part ID.
        id: PartId,
        /// Extension payload.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        payload: Option<Value>,
        /// Additional metadata.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        meta: Option<Value>,
    },
}

// Helper functions for serde skip_serializing_if
fn is_default_format(f: &TextFormat) -> bool {
    *f == TextFormat::Markdown
}

fn is_default_visibility(v: &ThinkingVisibility) -> bool {
    *v == ThinkingVisibility::Ui
}

fn is_default_status(s: &ToolStatus) -> bool {
    *s == ToolStatus::Pending
}

impl CanonPart {
    /// Create a text part with auto-generated ID.
    pub fn text(text: impl Into<String>) -> Self {
        Self::Text {
            id: generate_part_id(),
            text: text.into(),
            format: TextFormat::Markdown,
            meta: None,
        }
    }

    /// Create a plain text part with auto-generated ID.
    pub fn plain_text(text: impl Into<String>) -> Self {
        Self::Text {
            id: generate_part_id(),
            text: text.into(),
            format: TextFormat::Plain,
            meta: None,
        }
    }

    /// Create a thinking part with auto-generated ID.
    pub fn thinking(text: impl Into<String>) -> Self {
        Self::Thinking {
            id: generate_part_id(),
            text: text.into(),
            visibility: ThinkingVisibility::Ui,
            meta: None,
        }
    }

    /// Create a tool call part.
    pub fn tool_call(
        tool_call_id: impl Into<String>,
        name: impl Into<String>,
        input: Option<Value>,
    ) -> Self {
        Self::ToolCall {
            id: generate_part_id(),
            tool_call_id: tool_call_id.into(),
            name: name.into(),
            input,
            status: ToolStatus::Pending,
            meta: None,
        }
    }

    /// Create a tool result part.
    pub fn tool_result(
        tool_call_id: impl Into<String>,
        name: Option<String>,
        output: Option<Value>,
        is_error: bool,
    ) -> Self {
        Self::ToolResult {
            id: generate_part_id(),
            tool_call_id: tool_call_id.into(),
            name,
            output,
            is_error,
            title: None,
            duration_ms: None,
            meta: None,
        }
    }

    /// Create a file reference part.
    pub fn file_ref(uri: impl Into<String>, label: Option<String>) -> Self {
        Self::FileRef {
            id: generate_part_id(),
            uri: uri.into(),
            label,
            range: None,
            origin_text: None,
            meta: None,
        }
    }

    /// Create an image part from a URL.
    pub fn image_url(url: impl Into<String>, alt: Option<String>) -> Self {
        Self::Image {
            id: generate_part_id(),
            source: MediaSource::url(url),
            alt,
            dimensions: None,
            meta: None,
        }
    }

    /// Create an image part from base64 data.
    pub fn image_base64(
        data: impl Into<String>,
        mime_type: impl Into<String>,
        alt: Option<String>,
    ) -> Self {
        Self::Image {
            id: generate_part_id(),
            source: MediaSource::base64(data, mime_type),
            alt,
            dimensions: None,
            meta: None,
        }
    }

    /// Create an audio part from a URL.
    pub fn audio_url(url: impl Into<String>) -> Self {
        Self::Audio {
            id: generate_part_id(),
            source: MediaSource::url(url),
            duration_sec: None,
            transcript: None,
            meta: None,
        }
    }

    /// Create an audio part from base64 data.
    pub fn audio_base64(data: impl Into<String>, mime_type: impl Into<String>) -> Self {
        Self::Audio {
            id: generate_part_id(),
            source: MediaSource::base64(data, mime_type),
            duration_sec: None,
            transcript: None,
            meta: None,
        }
    }

    /// Create a video part from a URL.
    pub fn video_url(url: impl Into<String>) -> Self {
        Self::Video {
            id: generate_part_id(),
            source: MediaSource::url(url),
            duration_sec: None,
            dimensions: None,
            thumbnail: None,
            meta: None,
        }
    }

    /// Create an attachment part from a URL.
    pub fn attachment_url(url: impl Into<String>, filename: Option<String>) -> Self {
        Self::Attachment {
            id: generate_part_id(),
            source: MediaSource::url(url),
            filename,
            size_bytes: None,
            meta: None,
        }
    }

    /// Create an attachment part from base64 data.
    pub fn attachment_base64(
        data: impl Into<String>,
        mime_type: impl Into<String>,
        filename: Option<String>,
    ) -> Self {
        Self::Attachment {
            id: generate_part_id(),
            source: MediaSource::base64(data, mime_type),
            filename,
            size_bytes: None,
            meta: None,
        }
    }

    /// Get the part ID.
    pub fn id(&self) -> &str {
        match self {
            Self::Text { id, .. }
            | Self::Thinking { id, .. }
            | Self::ToolCall { id, .. }
            | Self::ToolResult { id, .. }
            | Self::FileRef { id, .. }
            | Self::Citation { id, .. }
            | Self::Image { id, .. }
            | Self::Audio { id, .. }
            | Self::Video { id, .. }
            | Self::Attachment { id, .. }
            | Self::Extension { id, .. } => id,
        }
    }

    /// Extract text content from this part (for flattening).
    pub fn text_content(&self) -> Option<&str> {
        match self {
            Self::Text { text, .. } | Self::Thinking { text, .. } => Some(text),
            Self::ToolResult { output, .. } => output.as_ref().and_then(|v| v.as_str()),
            Self::Audio { transcript, .. } => transcript.as_deref(),
            _ => None,
        }
    }

    /// Check if this part contains binary/media content.
    pub fn is_media(&self) -> bool {
        matches!(
            self,
            Self::Image { .. } | Self::Audio { .. } | Self::Video { .. } | Self::Attachment { .. }
        )
    }

    /// Get the media source if this is a media part.
    pub fn media_source(&self) -> Option<&MediaSource> {
        match self {
            Self::Image { source, .. }
            | Self::Audio { source, .. }
            | Self::Video { source, .. }
            | Self::Attachment { source, .. } => Some(source),
            _ => None,
        }
    }
}

/// Generate a unique part ID.
fn generate_part_id() -> PartId {
    format!("part_{}", uuid::Uuid::new_v4().simple())
}

// ============================================================================
// Token Usage
// ============================================================================

/// Token usage statistics.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../../frontend/src/generated/")]
pub struct TokenUsage {
    /// Input tokens.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input: Option<i64>,
    /// Output tokens.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<i64>,
    /// Reasoning tokens (extended thinking).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<i64>,
    /// Cache read tokens.
    #[serde(rename = "cacheRead", default, skip_serializing_if = "Option::is_none")]
    pub cache_read: Option<i64>,
    /// Cache write tokens.
    #[serde(
        rename = "cacheWrite",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub cache_write: Option<i64>,
}

impl TokenUsage {
    /// Total tokens (input + output).
    pub fn total(&self) -> i64 {
        self.input.unwrap_or(0) + self.output.unwrap_or(0)
    }
}

// ============================================================================
// Model Info
// ============================================================================

/// Model identification.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../../frontend/src/generated/")]
pub struct ModelInfo {
    /// Provider ID (e.g., "anthropic", "openai").
    #[serde(rename = "providerId")]
    pub provider_id: String,
    /// Model ID (e.g., "claude-3-5-sonnet-20241022").
    #[serde(rename = "modelId")]
    pub model_id: String,
}

impl ModelInfo {
    pub fn new(provider_id: impl Into<String>, model_id: impl Into<String>) -> Self {
        Self {
            provider_id: provider_id.into(),
            model_id: model_id.into(),
        }
    }

    /// Combined identifier in "provider/model" format.
    pub fn full_id(&self) -> String {
        format!("{}/{}", self.provider_id, self.model_id)
    }
}

// ============================================================================
// Canonical Message
// ============================================================================

/// A canonical message in a conversation.
///
/// This is the unified message format that all agent types convert to.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../../frontend/src/generated/")]
pub struct CanonMessage {
    /// Unique message ID.
    pub id: String,
    /// Conversation/session ID this message belongs to.
    #[serde(rename = "sessionId")]
    pub session_id: String,
    /// Message role.
    pub role: MessageRole,
    /// Flattened text content (for search indexing).
    pub content: String,
    /// Structured content parts.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parts: Vec<CanonPart>,
    /// Creation timestamp (Unix milliseconds).
    #[serde(rename = "createdAt")]
    pub created_at: i64,
    /// Completion timestamp (Unix milliseconds, for assistant messages).
    #[serde(
        rename = "completedAt",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub completed_at: Option<i64>,
    /// Model used for this message.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<ModelInfo>,
    /// Token usage.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tokens: Option<TokenUsage>,
    /// Cost in USD.
    #[serde(rename = "costUsd", default, skip_serializing_if = "Option::is_none")]
    pub cost_usd: Option<f64>,
    /// Parent message ID (for threading/branching).
    #[serde(rename = "parentId", default, skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    /// Agent name/type that generated this message.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
    /// Additional metadata.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Value>,
}

impl Default for CanonMessage {
    fn default() -> Self {
        Self {
            id: format!("msg_{}", uuid::Uuid::new_v4().simple()),
            session_id: String::new(),
            role: MessageRole::User,
            content: String::new(),
            parts: Vec::new(),
            created_at: chrono::Utc::now().timestamp_millis(),
            completed_at: None,
            model: None,
            tokens: None,
            cost_usd: None,
            parent_id: None,
            agent: None,
            metadata: None,
        }
    }
}

impl CanonMessage {
    /// Create a new user message.
    pub fn user(session_id: impl Into<String>, text: impl Into<String>) -> Self {
        let text = text.into();
        Self {
            id: format!("msg_{}", uuid::Uuid::new_v4().simple()),
            session_id: session_id.into(),
            role: MessageRole::User,
            content: text.clone(),
            parts: vec![CanonPart::text(text)],
            created_at: chrono::Utc::now().timestamp_millis(),
            ..Default::default()
        }
    }

    /// Create a new assistant message.
    pub fn assistant(session_id: impl Into<String>) -> Self {
        Self {
            id: format!("msg_{}", uuid::Uuid::new_v4().simple()),
            session_id: session_id.into(),
            role: MessageRole::Assistant,
            created_at: chrono::Utc::now().timestamp_millis(),
            ..Default::default()
        }
    }

    /// Create a new system message.
    pub fn system(session_id: impl Into<String>, text: impl Into<String>) -> Self {
        let text = text.into();
        Self {
            id: format!("msg_{}", uuid::Uuid::new_v4().simple()),
            session_id: session_id.into(),
            role: MessageRole::System,
            content: text.clone(),
            parts: vec![CanonPart::text(text)],
            created_at: chrono::Utc::now().timestamp_millis(),
            ..Default::default()
        }
    }

    /// Flatten parts to content string.
    pub fn flatten_content(&mut self) {
        self.content = self
            .parts
            .iter()
            .filter_map(|p| p.text_content())
            .collect::<Vec<_>>()
            .join("\n\n");
    }

    /// Add a text part and update content.
    pub fn add_text(&mut self, text: impl Into<String>) {
        let text = text.into();
        if !self.content.is_empty() {
            self.content.push_str("\n\n");
        }
        self.content.push_str(&text);
        self.parts.push(CanonPart::text(text));
    }

    /// Add a thinking part.
    pub fn add_thinking(&mut self, text: impl Into<String>) {
        self.parts.push(CanonPart::thinking(text));
    }

    /// Add a tool call.
    pub fn add_tool_call(
        &mut self,
        tool_call_id: impl Into<String>,
        name: impl Into<String>,
        input: Option<Value>,
    ) {
        self.parts
            .push(CanonPart::tool_call(tool_call_id, name, input));
    }

    /// Add a tool result.
    pub fn add_tool_result(
        &mut self,
        tool_call_id: impl Into<String>,
        name: Option<String>,
        output: Option<Value>,
        is_error: bool,
    ) {
        self.parts
            .push(CanonPart::tool_result(tool_call_id, name, output, is_error));
    }
}

// ============================================================================
// Canonical Conversation
// ============================================================================

/// A canonical conversation (chat session).
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../../frontend/src/generated/")]
pub struct CanonConversation {
    /// Internal conversation ID.
    pub id: String,
    /// External ID from source system (e.g., Pi session UUID).
    #[serde(
        rename = "externalId",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub external_id: Option<String>,
    /// Human-readable ID (e.g., "amber-builds-beacon").
    #[serde(
        rename = "readableId",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub readable_id: Option<String>,
    /// Conversation title/summary.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Workspace/project directory.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace: Option<String>,
    /// Project name (derived from workspace).
    #[serde(
        rename = "projectName",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub project_name: Option<String>,
    /// Creation timestamp (Unix milliseconds).
    #[serde(rename = "createdAt")]
    pub created_at: i64,
    /// Last update timestamp (Unix milliseconds).
    #[serde(rename = "updatedAt")]
    pub updated_at: i64,
    /// Primary model used.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Total input tokens.
    #[serde(rename = "tokensIn", default, skip_serializing_if = "Option::is_none")]
    pub tokens_in: Option<i64>,
    /// Total output tokens.
    #[serde(rename = "tokensOut", default, skip_serializing_if = "Option::is_none")]
    pub tokens_out: Option<i64>,
    /// Total cost in USD.
    #[serde(rename = "costUsd", default, skip_serializing_if = "Option::is_none")]
    pub cost_usd: Option<f64>,
    /// Parent conversation ID (for branching).
    #[serde(rename = "parentId", default, skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    /// Source agent type (e.g., "pi", "claude-code").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
    /// Whether this conversation is currently active/running.
    #[serde(rename = "isActive", default)]
    pub is_active: bool,
    /// Messages in this conversation (optional, for full fetch).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub messages: Vec<CanonMessage>,
    /// Additional metadata.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Value>,
}

impl Default for CanonConversation {
    fn default() -> Self {
        let now = chrono::Utc::now().timestamp_millis();
        Self {
            id: format!("conv_{}", uuid::Uuid::new_v4().simple()),
            external_id: None,
            readable_id: None,
            title: None,
            workspace: None,
            project_name: None,
            created_at: now,
            updated_at: now,
            model: None,
            tokens_in: None,
            tokens_out: None,
            cost_usd: None,
            parent_id: None,
            agent: None,
            is_active: false,
            messages: Vec::new(),
            metadata: None,
        }
    }
}

impl CanonConversation {
    /// Create a new conversation for a workspace.
    pub fn new(workspace: impl Into<String>, agent: impl Into<String>) -> Self {
        let workspace = workspace.into();
        let project_name = std::path::Path::new(&workspace)
            .file_name()
            .and_then(|n| n.to_str())
            .map(String::from);

        Self {
            workspace: Some(workspace),
            project_name,
            agent: Some(agent.into()),
            ..Default::default()
        }
    }

    /// Update timestamps based on messages.
    pub fn update_timestamps(&mut self) {
        if let Some(first) = self.messages.first() {
            self.created_at = first.created_at;
        }
        if let Some(last) = self.messages.last() {
            self.updated_at = last.completed_at.unwrap_or(last.created_at);
        }
    }

    /// Calculate total tokens from messages.
    pub fn calculate_totals(&mut self) {
        let mut tokens_in: i64 = 0;
        let mut tokens_out: i64 = 0;
        let mut cost: f64 = 0.0;

        for msg in &self.messages {
            if let Some(ref tokens) = msg.tokens {
                tokens_in += tokens.input.unwrap_or(0);
                tokens_out += tokens.output.unwrap_or(0);
            }
            if let Some(c) = msg.cost_usd {
                cost += c;
            }
        }

        self.tokens_in = if tokens_in > 0 { Some(tokens_in) } else { None };
        self.tokens_out = if tokens_out > 0 {
            Some(tokens_out)
        } else {
            None
        };
        self.cost_usd = if cost > 0.0 { Some(cost) } else { None };
    }
}

// ============================================================================
// Streaming Events
// ============================================================================

/// A streaming event during message generation.
///
/// Used for real-time updates via WebSocket.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "snake_case")]
#[ts(export, export_to = "../../../../frontend/src/generated/")]
pub enum StreamEvent {
    /// Agent started processing.
    AgentStart {
        #[serde(rename = "sessionId")]
        session_id: String,
    },

    /// New message started.
    MessageStart {
        #[serde(rename = "sessionId")]
        session_id: String,
        #[serde(rename = "messageId")]
        message_id: String,
        role: MessageRole,
    },

    /// Text content delta.
    TextDelta {
        #[serde(rename = "sessionId")]
        session_id: String,
        #[serde(rename = "messageId")]
        message_id: String,
        delta: String,
    },

    /// Thinking content delta.
    ThinkingDelta {
        #[serde(rename = "sessionId")]
        session_id: String,
        #[serde(rename = "messageId")]
        message_id: String,
        delta: String,
    },

    /// Tool call started.
    ToolCallStart {
        #[serde(rename = "sessionId")]
        session_id: String,
        #[serde(rename = "messageId")]
        message_id: String,
        #[serde(rename = "toolCallId")]
        tool_call_id: String,
        name: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        input: Option<Value>,
    },

    /// Tool execution update.
    ToolCallUpdate {
        #[serde(rename = "sessionId")]
        session_id: String,
        #[serde(rename = "toolCallId")]
        tool_call_id: String,
        status: ToolStatus,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        title: Option<String>,
    },

    /// Tool call completed.
    ToolCallEnd {
        #[serde(rename = "sessionId")]
        session_id: String,
        #[serde(rename = "toolCallId")]
        tool_call_id: String,
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

    /// Message completed.
    MessageEnd {
        #[serde(rename = "sessionId")]
        session_id: String,
        #[serde(rename = "messageId")]
        message_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tokens: Option<TokenUsage>,
        #[serde(rename = "costUsd", default, skip_serializing_if = "Option::is_none")]
        cost_usd: Option<f64>,
    },

    /// Agent finished processing.
    AgentEnd {
        #[serde(rename = "sessionId")]
        session_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
    },

    /// Error occurred.
    Error {
        #[serde(rename = "sessionId")]
        session_id: String,
        error: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        code: Option<String>,
    },
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_role_parsing() {
        assert_eq!(MessageRole::parse("user"), MessageRole::User);
        assert_eq!(MessageRole::parse("human"), MessageRole::User);
        assert_eq!(MessageRole::parse("assistant"), MessageRole::Assistant);
        assert_eq!(MessageRole::parse("agent"), MessageRole::Assistant);
        assert_eq!(MessageRole::parse("AI"), MessageRole::Assistant);
        assert_eq!(MessageRole::parse("system"), MessageRole::System);
        assert_eq!(MessageRole::parse("tool"), MessageRole::Tool);
        assert_eq!(MessageRole::parse("toolResult"), MessageRole::Tool);
        assert_eq!(MessageRole::parse("function"), MessageRole::Tool);
    }

    #[test]
    fn test_tool_status_parsing() {
        assert_eq!(ToolStatus::parse("pending"), ToolStatus::Pending);
        assert_eq!(ToolStatus::parse("running"), ToolStatus::Running);
        assert_eq!(ToolStatus::parse("in_progress"), ToolStatus::Running);
        assert_eq!(ToolStatus::parse("success"), ToolStatus::Success);
        assert_eq!(ToolStatus::parse("completed"), ToolStatus::Success);
        assert_eq!(ToolStatus::parse("error"), ToolStatus::Error);
        assert_eq!(ToolStatus::parse("failed"), ToolStatus::Error);
    }

    #[test]
    fn test_canon_part_serialization() {
        let part = CanonPart::text("Hello, world!");
        let json = serde_json::to_string(&part).unwrap();
        assert!(json.contains("\"type\":\"text\""));
        assert!(json.contains("Hello, world!"));
    }

    #[test]
    fn test_canon_part_tool_call() {
        let part = CanonPart::tool_call(
            "call_123",
            "bash",
            Some(serde_json::json!({"command": "ls -la"})),
        );
        let json = serde_json::to_string(&part).unwrap();
        assert!(json.contains("\"type\":\"tool_call\""));
        assert!(json.contains("\"toolCallId\":\"call_123\""));
        assert!(json.contains("\"name\":\"bash\""));
    }

    #[test]
    fn test_canon_message_user() {
        let msg = CanonMessage::user("ses_123", "Hello!");
        assert_eq!(msg.role, MessageRole::User);
        assert_eq!(msg.content, "Hello!");
        assert_eq!(msg.session_id, "ses_123");
        assert_eq!(msg.parts.len(), 1);
    }

    #[test]
    fn test_canon_message_add_parts() {
        let mut msg = CanonMessage::assistant("ses_123");
        msg.add_text("Let me help you.");
        msg.add_thinking("I need to consider...");
        msg.add_tool_call(
            "call_1",
            "read",
            Some(serde_json::json!({"path": "foo.rs"})),
        );

        assert_eq!(msg.parts.len(), 3);
        assert_eq!(msg.content, "Let me help you.");
    }

    #[test]
    fn test_canon_conversation_totals() {
        let mut conv = CanonConversation::new("/home/user/project", "pi");

        let mut msg1 = CanonMessage::user("conv_1", "Hello");
        msg1.tokens = Some(TokenUsage {
            input: Some(10),
            output: Some(0),
            ..Default::default()
        });

        let mut msg2 = CanonMessage::assistant("conv_1");
        msg2.add_text("Hi there!");
        msg2.tokens = Some(TokenUsage {
            input: Some(0),
            output: Some(20),
            ..Default::default()
        });
        msg2.cost_usd = Some(0.001);

        conv.messages.push(msg1);
        conv.messages.push(msg2);
        conv.calculate_totals();

        assert_eq!(conv.tokens_in, Some(10));
        assert_eq!(conv.tokens_out, Some(20));
        assert_eq!(conv.cost_usd, Some(0.001));
    }

    #[test]
    fn test_stream_event_serialization() {
        let event = StreamEvent::TextDelta {
            session_id: "ses_123".to_string(),
            message_id: "msg_456".to_string(),
            delta: "Hello".to_string(),
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"type\":\"text_delta\""));
        assert!(json.contains("\"sessionId\":\"ses_123\""));
        assert!(json.contains("\"delta\":\"Hello\""));
    }
}
