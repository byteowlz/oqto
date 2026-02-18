//! Prompt manager for handling security approval requests.

use crate::prompts::models::{
    Prompt, PromptAction, PromptMessage, PromptRequest, PromptResponse, PromptStatus,
};
use anyhow::{Result, anyhow};
use log::{debug, info, warn};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{RwLock, broadcast, oneshot};
use tokio::time::{Duration, interval};

/// Channel capacity for prompt broadcasts.
const BROADCAST_CAPACITY: usize = 64;

/// How often to check for expired prompts.
const CLEANUP_INTERVAL_SECS: u64 = 5;

/// Pending prompt with response channel.
struct PendingPrompt {
    prompt: Prompt,
    response_tx: Option<oneshot::Sender<PromptResponse>>,
}

/// Manages security prompts and broadcasts to connected clients.
pub struct PromptManager {
    /// Pending prompts by ID
    pending: Arc<RwLock<HashMap<String, PendingPrompt>>>,

    /// Broadcast channel for prompt updates
    broadcast_tx: broadcast::Sender<PromptMessage>,

    /// Session-based approval cache: (source, resource) -> expiry
    approval_cache: Arc<RwLock<HashMap<(String, String), chrono::DateTime<chrono::Utc>>>>,

    /// Default timeout for prompts
    default_timeout_secs: u64,

    /// Whether desktop notifications are enabled
    desktop_notifications: bool,
}

impl PromptManager {
    /// Create a new prompt manager.
    pub fn new() -> Self {
        let (broadcast_tx, _) = broadcast::channel(BROADCAST_CAPACITY);

        let manager = Self {
            pending: Arc::new(RwLock::new(HashMap::new())),
            broadcast_tx,
            approval_cache: Arc::new(RwLock::new(HashMap::new())),
            default_timeout_secs: 60,
            desktop_notifications: true,
        };

        // Start background cleanup task
        manager.start_cleanup_task();

        manager
    }

    /// Create with custom configuration.
    pub fn with_config(timeout_secs: u64, desktop_notifications: bool) -> Self {
        let mut manager = Self::new();
        manager.default_timeout_secs = timeout_secs;
        manager.desktop_notifications = desktop_notifications;
        manager
    }

    /// Subscribe to prompt updates.
    pub fn subscribe(&self) -> broadcast::Receiver<PromptMessage> {
        self.broadcast_tx.subscribe()
    }

    /// Get all pending prompts.
    pub async fn list_pending(&self) -> Vec<Prompt> {
        let pending = self.pending.read().await;
        pending
            .values()
            .filter(|p| p.prompt.status == PromptStatus::Pending && !p.prompt.is_expired())
            .map(|p| p.prompt.clone())
            .collect()
    }

    /// Get a specific prompt by ID.
    pub async fn get(&self, id: &str) -> Option<Prompt> {
        let pending = self.pending.read().await;
        pending.get(id).map(|p| p.prompt.clone())
    }

    /// Check if access is already approved in the session cache.
    pub async fn is_approved(&self, source: &str, resource: &str) -> bool {
        let cache = self.approval_cache.read().await;
        if let Some(expiry) = cache.get(&(source.to_string(), resource.to_string()))
            && *expiry > chrono::Utc::now()
        {
            debug!(
                "Cache hit: {} access to {} is approved until {}",
                source, resource, expiry
            );
            return true;
        }
        false
    }

    /// Request approval from the user.
    ///
    /// This will:
    /// 1. Check the session cache for existing approval
    /// 2. Create a prompt and broadcast to connected UIs
    /// 3. Optionally show a desktop notification
    /// 4. Wait for response or timeout
    ///
    /// Returns the user's response, or an error on timeout/cancellation.
    pub async fn request(&self, mut req: PromptRequest) -> Result<PromptResponse> {
        let source = req.source.to_string();
        let resource = req.resource.clone();

        // Check cache first
        if self.is_approved(&source, &resource).await {
            return Ok(PromptResponse {
                action: PromptAction::AllowSession,
                responded_at: chrono::Utc::now(),
            });
        }

        // Apply default timeout if not set
        if req.timeout_secs == 0 {
            req.timeout_secs = self.default_timeout_secs;
        }

        let prompt = Prompt::new(req);
        let prompt_id = prompt.id.clone();
        let timeout = prompt.remaining();

        info!(
            "Creating prompt {}: {} wants {} access to {}",
            prompt_id, prompt.request.source, prompt.request.prompt_type, prompt.request.resource
        );

        // Create response channel
        let (response_tx, response_rx) = oneshot::channel();

        // Store pending prompt
        {
            let mut pending = self.pending.write().await;
            pending.insert(
                prompt_id.clone(),
                PendingPrompt {
                    prompt: prompt.clone(),
                    response_tx: Some(response_tx),
                },
            );
        }

        // Broadcast to connected clients
        let _ = self.broadcast_tx.send(PromptMessage::Created {
            prompt: prompt.clone(),
        });

        // Show desktop notification if enabled and no UI connected
        if self.desktop_notifications && self.broadcast_tx.receiver_count() == 0 {
            self.show_desktop_notification(&prompt);
        }

        // Wait for response with timeout
        let result = tokio::time::timeout(timeout, response_rx).await;

        match result {
            Ok(Ok(response)) => {
                info!("Prompt {} responded: {:?}", prompt_id, response.action);

                // Cache session approvals
                if response.action == PromptAction::AllowSession {
                    self.cache_approval(&source, &resource).await;
                }

                Ok(response)
            }
            Ok(Err(_)) => {
                // Channel closed (cancelled)
                warn!("Prompt {} was cancelled", prompt_id);
                self.mark_cancelled(&prompt_id).await;
                Err(anyhow!("Prompt was cancelled"))
            }
            Err(_) => {
                // Timeout
                warn!("Prompt {} timed out", prompt_id);
                self.mark_timed_out(&prompt_id).await;
                Err(anyhow!("Prompt timed out"))
            }
        }
    }

    /// Respond to a prompt.
    ///
    /// Called by the API when user responds via UI.
    pub async fn respond(&self, prompt_id: &str, action: PromptAction) -> Result<()> {
        let response_tx = {
            let mut pending = self.pending.write().await;
            let pending_prompt = pending
                .get_mut(prompt_id)
                .ok_or_else(|| anyhow!("Prompt not found: {}", prompt_id))?;

            if pending_prompt.prompt.status != PromptStatus::Pending {
                return Err(anyhow!("Prompt already handled: {}", prompt_id));
            }

            pending_prompt.prompt.respond(action.clone());
            pending_prompt.response_tx.take()
        };

        // Send response to waiting request
        if let Some(tx) = response_tx {
            let response = PromptResponse {
                action: action.clone(),
                responded_at: chrono::Utc::now(),
            };
            let _ = tx.send(response);
        }

        // Broadcast update
        let _ = self.broadcast_tx.send(PromptMessage::Responded {
            prompt_id: prompt_id.to_string(),
            action,
        });

        info!("Prompt {} responded", prompt_id);
        Ok(())
    }

    /// Cancel a prompt.
    pub async fn cancel(&self, prompt_id: &str) -> Result<()> {
        self.mark_cancelled(prompt_id).await;
        Ok(())
    }

    /// Cache a session approval.
    async fn cache_approval(&self, source: &str, resource: &str) {
        let expiry = chrono::Utc::now() + chrono::Duration::hours(8); // Session = 8 hours
        let mut cache = self.approval_cache.write().await;
        cache.insert((source.to_string(), resource.to_string()), expiry);
        debug!(
            "Cached approval: {} access to {} until {}",
            source, resource, expiry
        );
    }

    /// Clear the approval cache.
    pub async fn clear_cache(&self) {
        let mut cache = self.approval_cache.write().await;
        cache.clear();
        info!("Cleared approval cache");
    }

    /// Mark a prompt as timed out.
    async fn mark_timed_out(&self, prompt_id: &str) {
        let mut pending = self.pending.write().await;
        if let Some(p) = pending.get_mut(prompt_id) {
            p.prompt.timeout();
            // Drop the response channel to signal cancellation
            p.response_tx.take();
        }

        let _ = self.broadcast_tx.send(PromptMessage::TimedOut {
            prompt_id: prompt_id.to_string(),
        });
    }

    /// Mark a prompt as cancelled.
    async fn mark_cancelled(&self, prompt_id: &str) {
        let mut pending = self.pending.write().await;
        if let Some(p) = pending.get_mut(prompt_id) {
            p.prompt.cancel();
            p.response_tx.take();
        }

        let _ = self.broadcast_tx.send(PromptMessage::Cancelled {
            prompt_id: prompt_id.to_string(),
        });
    }

    /// Show a desktop notification for a prompt.
    fn show_desktop_notification(&self, prompt: &Prompt) {
        #[cfg(feature = "desktop-notifications")]
        {
            use notify_rust::Notification;

            let title = match &prompt.request.source {
                crate::prompts::PromptSource::OctoGuard => "File Access Request",
                crate::prompts::PromptSource::OctoSshProxy => "SSH Access Request",
                crate::prompts::PromptSource::Network => "Network Access Request",
                crate::prompts::PromptSource::Other(_) => "Access Request",
            };

            let body = prompt
                .request
                .description
                .clone()
                .unwrap_or_else(|| format!("Access to {}", prompt.request.resource));

            if let Err(e) = Notification::new()
                .summary(title)
                .body(&body)
                .timeout(prompt.remaining().as_millis() as i32)
                .show()
            {
                warn!("Failed to show desktop notification: {}", e);
            }
        }

        #[cfg(not(feature = "desktop-notifications"))]
        {
            debug!(
                "Desktop notifications disabled, prompt {} requires UI response",
                prompt.id
            );
        }
    }

    /// Start background task to clean up expired prompts.
    fn start_cleanup_task(&self) {
        let pending = Arc::clone(&self.pending);
        let broadcast_tx = self.broadcast_tx.clone();
        let approval_cache = Arc::clone(&self.approval_cache);

        tokio::spawn(async move {
            let mut ticker = interval(Duration::from_secs(CLEANUP_INTERVAL_SECS));

            loop {
                ticker.tick().await;

                // Clean up expired prompts
                let expired: Vec<String> = {
                    let pending = pending.read().await;
                    pending
                        .iter()
                        .filter(|(_, p)| {
                            p.prompt.status == PromptStatus::Pending && p.prompt.is_expired()
                        })
                        .map(|(id, _)| id.clone())
                        .collect()
                };

                for id in expired {
                    let mut pending = pending.write().await;
                    if let Some(p) = pending.get_mut(&id)
                        && p.prompt.status == PromptStatus::Pending
                        && p.prompt.is_expired()
                    {
                        p.prompt.timeout();
                        p.response_tx.take();
                        let _ = broadcast_tx.send(PromptMessage::TimedOut {
                            prompt_id: id.clone(),
                        });
                        debug!("Prompt {} expired", id);
                    }
                }

                // Clean up old entries from pending map (keep for 1 hour for audit)
                {
                    let mut pending = pending.write().await;
                    let cutoff = chrono::Utc::now() - chrono::Duration::hours(1);
                    pending.retain(|_, p| p.prompt.created_at > cutoff);
                }

                // Clean up expired approval cache entries
                {
                    let mut cache = approval_cache.write().await;
                    let now = chrono::Utc::now();
                    cache.retain(|_, expiry| *expiry > now);
                }
            }
        });
    }
}

impl Default for PromptManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_prompt_manager_basic() {
        let manager = PromptManager::new();

        // Should start with no pending prompts
        let pending = manager.list_pending().await;
        assert!(pending.is_empty());
    }

    #[tokio::test]
    async fn test_prompt_respond() {
        let manager = PromptManager::new();

        // Create a prompt in background
        let manager_clone = Arc::new(manager);
        let manager_ref = Arc::clone(&manager_clone);

        let handle = tokio::spawn(async move {
            let req = PromptRequest::file_access("/test", "read").with_timeout(5);
            manager_ref.request(req).await
        });

        // Wait a bit for prompt to be created
        tokio::time::sleep(Duration::from_millis(50)).await;

        // Get pending prompts
        let pending = manager_clone.list_pending().await;
        assert_eq!(pending.len(), 1);

        let prompt_id = pending[0].id.clone();

        // Respond to the prompt
        manager_clone
            .respond(&prompt_id, PromptAction::AllowOnce)
            .await
            .unwrap();

        // The request should complete
        let result = handle.await.unwrap();
        assert!(result.is_ok());
        assert_eq!(result.unwrap().action, PromptAction::AllowOnce);
    }

    #[tokio::test]
    async fn test_session_approval_cache() {
        let manager = PromptManager::new();

        // Not approved initially
        assert!(!manager.is_approved("oqto-guard", "/test").await);

        // Cache an approval
        manager.cache_approval("oqto-guard", "/test").await;

        // Now it should be approved
        assert!(manager.is_approved("oqto-guard", "/test").await);

        // Clear cache
        manager.clear_cache().await;

        // No longer approved
        assert!(!manager.is_approved("oqto-guard", "/test").await);
    }
}
