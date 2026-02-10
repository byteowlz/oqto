//! hstry gRPC client for writing chat history.

use std::path::Path;
use std::sync::Arc;

use anyhow::{Context, Result};
use tokio::sync::RwLock;
use tonic::transport::Channel;

use hstry_core::service::proto::{
    AppendMessagesRequest, AppendMessagesResponse, Conversation, DeleteConversationRequest,
    DeleteConversationResponse, GetConversationRequest, GetMessagesRequest,
    ListConversationsRequest, Message, UpdateConversationRequest, UpdateConversationResponse,
    UploadAttachmentRequest, UploadAttachmentResponse, WriteConversationRequest,
    WriteConversationResponse,
};
use hstry_core::service::{ReadServiceClient, WriteServiceClient};

/// Source ID for Pi sessions (used for deduplication with hstry daemon).
pub const PI_SOURCE_ID: &str = "pi";

/// Client for communicating with hstry daemon's WriteService.
#[derive(Clone)]
pub struct HstryClient {
    write: Arc<RwLock<Option<WriteServiceClient<Channel>>>>,
    read: Arc<RwLock<Option<ReadServiceClient<Channel>>>>,
}

impl HstryClient {
    /// Create a new client (not connected yet).
    pub fn new() -> Self {
        Self {
            write: Arc::new(RwLock::new(None)),
            read: Arc::new(RwLock::new(None)),
        }
    }

    /// Connect to the hstry daemon.
    /// Tries Unix socket first, then falls back to TCP.
    pub async fn connect(&self) -> Result<()> {
        let channel = try_connect_channel().await?;
        *self.write.write().await = Some(WriteServiceClient::new(channel.clone()));
        *self.read.write().await = Some(ReadServiceClient::new(channel));
        Ok(())
    }

    /// Check if connected.
    pub async fn is_connected(&self) -> bool {
        self.write.read().await.is_some()
    }

    /// Ensure connection is established, reconnecting if needed.
    async fn ensure_write_connected(&self) -> Result<WriteServiceClient<Channel>> {
        {
            let guard = self.write.read().await;
            if let Some(client) = guard.as_ref() {
                return Ok(client.clone());
            }
        }

        // Try to connect
        let channel = try_connect_channel().await?;
        let write_client = WriteServiceClient::new(channel.clone());
        *self.write.write().await = Some(write_client.clone());
        if self.read.read().await.is_none() {
            *self.read.write().await = Some(ReadServiceClient::new(channel));
        }
        Ok(write_client)
    }

    async fn ensure_read_connected(&self) -> Result<ReadServiceClient<Channel>> {
        {
            let guard = self.read.read().await;
            if let Some(client) = guard.as_ref() {
                return Ok(client.clone());
            }
        }

        let channel = try_connect_channel().await?;
        let read_client = ReadServiceClient::new(channel.clone());
        *self.read.write().await = Some(read_client.clone());
        if self.write.read().await.is_none() {
            *self.write.write().await = Some(WriteServiceClient::new(channel));
        }
        Ok(read_client)
    }

    /// Write a conversation and its messages to hstry.
    ///
    /// Uses source_id + external_id for deduplication:
    /// - If the conversation exists, it's updated
    /// - If not, it's created
    pub async fn write_conversation(
        &self,
        session_id: &str,
        title: Option<String>,
        workspace: Option<String>,
        model: Option<String>,
        provider: Option<String>,
        metadata_json: Option<String>,
        messages: Vec<Message>,
        created_at_ms: i64,
        updated_at_ms: Option<i64>,
        harness: Option<String>,
        readable_id: Option<String>,
    ) -> Result<WriteConversationResponse> {
        let mut client = self.ensure_write_connected().await?;

        let request = WriteConversationRequest {
            conversation: Some(Conversation {
                source_id: PI_SOURCE_ID.to_string(),
                external_id: session_id.to_string(),
                title,
                created_at_ms,
                updated_at_ms,
                model,
                provider,
                workspace,
                tokens_in: None,
                tokens_out: None,
                cost_usd: None,
                metadata_json: metadata_json.unwrap_or_default(),
                harness,
                readable_id,
            }),
            messages,
        };

        let response = client
            .write_conversation(request)
            .await
            .context("Failed to write conversation to hstry")?;

        Ok(response.into_inner())
    }

    /// Partial metadata update -- only set fields are applied, others preserved.
    pub async fn update_conversation(
        &self,
        session_id: &str,
        title: Option<String>,
        workspace: Option<String>,
        model: Option<String>,
        provider: Option<String>,
        metadata_json: Option<String>,
        readable_id: Option<String>,
        harness: Option<String>,
    ) -> Result<UpdateConversationResponse> {
        let mut client = self.ensure_write_connected().await?;

        let request = UpdateConversationRequest {
            source_id: PI_SOURCE_ID.to_string(),
            external_id: session_id.to_string(),
            title,
            workspace,
            model,
            provider,
            metadata_json,
            readable_id,
            harness,
        };

        let response = client
            .update_conversation(request)
            .await
            .context("Failed to update conversation in hstry")?;

        Ok(response.into_inner())
    }

    /// Delete a conversation and all its messages.
    pub async fn delete_conversation(
        &self,
        session_id: &str,
    ) -> Result<DeleteConversationResponse> {
        let mut client = self.ensure_write_connected().await?;

        let request = DeleteConversationRequest {
            source_id: PI_SOURCE_ID.to_string(),
            external_id: session_id.to_string(),
        };

        let response = client
            .delete_conversation(request)
            .await
            .context("Failed to delete conversation from hstry")?;

        Ok(response.into_inner())
    }

    /// Append messages to an existing conversation.
    pub async fn append_messages(
        &self,
        session_id: &str,
        messages: Vec<Message>,
        updated_at_ms: Option<i64>,
    ) -> Result<AppendMessagesResponse> {
        let mut client = self.ensure_write_connected().await?;

        let request = AppendMessagesRequest {
            source_id: PI_SOURCE_ID.to_string(),
            external_id: session_id.to_string(),
            messages,
            updated_at_ms,
        };

        let response = client
            .append_messages(request)
            .await
            .context("Failed to append messages to hstry")?;

        Ok(response.into_inner())
    }

    /// Upload binary attachment data.
    pub async fn upload_attachment(
        &self,
        message_id: &str,
        mime_type: &str,
        filename: Option<String>,
        data: Vec<u8>,
    ) -> Result<UploadAttachmentResponse> {
        let mut client = self.ensure_write_connected().await?;

        let request = UploadAttachmentRequest {
            message_id: message_id.to_string(),
            mime_type: mime_type.to_string(),
            filename,
            data,
        };

        let response = client
            .upload_attachment(request)
            .await
            .context("Failed to upload attachment to hstry")?;

        Ok(response.into_inner())
    }

    pub async fn get_conversation(
        &self,
        session_id: &str,
        workspace: Option<String>,
    ) -> Result<Option<Conversation>> {
        let mut client = self.ensure_read_connected().await?;
        let request = GetConversationRequest {
            source_id: PI_SOURCE_ID.to_string(),
            external_id: session_id.to_string(),
            readable_id: session_id.to_string(),
            conversation_id: session_id.to_string(),
            workspace: workspace.unwrap_or_default(),
        };
        let response = client
            .get_conversation(request)
            .await
            .context("Failed to get conversation from hstry")?;
        Ok(response.into_inner().conversation)
    }

    pub async fn get_messages(
        &self,
        session_id: &str,
        workspace: Option<String>,
        limit: Option<i64>,
    ) -> Result<Vec<Message>> {
        let mut client = self.ensure_read_connected().await?;
        let request = GetMessagesRequest {
            source_id: PI_SOURCE_ID.to_string(),
            external_id: session_id.to_string(),
            readable_id: session_id.to_string(),
            conversation_id: session_id.to_string(),
            workspace: workspace.unwrap_or_default(),
            limit: limit.unwrap_or(0),
        };
        let response = client
            .get_messages(request)
            .await
            .context("Failed to get messages from hstry")?;
        Ok(response.into_inner().messages)
    }

    pub async fn list_conversations(
        &self,
        workspace: Option<String>,
        limit: Option<i64>,
        offset: Option<i64>,
    ) -> Result<Vec<hstry_core::service::proto::ConversationSummary>> {
        let mut client = self.ensure_read_connected().await?;
        let request = ListConversationsRequest {
            source_id: PI_SOURCE_ID.to_string(),
            workspace: workspace.unwrap_or_default(),
            limit: limit.unwrap_or(0),
            offset: offset.unwrap_or(0),
        };
        let response = client
            .list_conversations(request)
            .await
            .context("Failed to list conversations from hstry")?;
        Ok(response.into_inner().conversations)
    }
}

impl Default for HstryClient {
    fn default() -> Self {
        Self::new()
    }
}

/// Try to connect to hstry daemon.
/// Attempts Unix socket first, then falls back to TCP.
async fn try_connect_channel() -> Result<Channel> {
    // Try Unix socket first (more secure)
    #[cfg(unix)]
    {
        let socket_path = hstry_core::paths::service_socket_path();
        if socket_path.exists() {
            if let Ok(client) = try_connect_unix(&socket_path).await {
                tracing::debug!("Connected to hstry via Unix socket");
                return Ok(client);
            }
        }
    }

    // Fall back to TCP
    let port = read_port().ok_or_else(|| {
        anyhow::anyhow!(
            "hstry daemon not running. Start it with `hstry service start` or ensure it's running."
        )
    })?;

    let endpoint = format!("http://127.0.0.1:{port}");
    let channel = Channel::from_shared(endpoint)?
        .connect()
        .await
        .context("Failed to connect to hstry daemon via TCP")?;

    tracing::debug!("Connected to hstry via TCP on port {port}");
    Ok(channel)
}

#[cfg(unix)]
async fn try_connect_unix(socket_path: &Path) -> Result<Channel> {
    use hyper_util::rt::TokioIo;
    use tokio::net::UnixStream;
    use tonic::transport::Endpoint;

    let socket_path = socket_path.to_path_buf();

    let channel = Endpoint::from_static("http://[::]:0")
        .connect_with_connector(tower::service_fn(move |_: tonic::transport::Uri| {
            let path = socket_path.clone();
            async move {
                let stream = UnixStream::connect(path).await?;
                Ok::<_, std::io::Error>(TokioIo::new(stream))
            }
        }))
        .await?;

    Ok(channel)
}

fn read_port() -> Option<u16> {
    let port_path = hstry_core::paths::service_port_path();
    std::fs::read_to_string(port_path)
        .ok()
        .and_then(|s| s.trim().parse().ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_creation() {
        let client = HstryClient::new();
        // Just verify it can be created
        assert!(!futures::executor::block_on(client.is_connected()));
    }
}
