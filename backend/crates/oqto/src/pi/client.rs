//! Pi RPC client configuration.
//!
//! Shared settings for Pi subprocess communication.

/// Configuration for the Pi client.
#[derive(Debug, Clone)]
pub struct PiClientConfig {
    /// Buffer size for the event broadcast channel.
    pub event_buffer_size: usize,
    /// Buffer size for the command channel.
    pub command_buffer_size: usize,
}

impl Default for PiClientConfig {
    fn default() -> Self {
        Self {
            event_buffer_size: 256,
            command_buffer_size: 64,
        }
    }
}
