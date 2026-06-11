//! HTTP and WebSocket proxy for container services.
//!
//! This module provides generic proxy infrastructure and specific handlers
//! for proxying requests to session services (fileserver, ttyd, etc.).

pub mod builder;
mod handlers;
mod mmry;
mod websocket;

// Re-export public handler functions for routes
pub use handlers::{
    proxy_browser_stream_ws, proxy_fileserver_for_workspace, proxy_fileserver_for_workspace_root,
    proxy_sldr, proxy_sldr_root, proxy_voice_stt_ws, proxy_voice_tts_ws,
};
pub use mmry::{
    proxy_mmry_add_for_workspace, proxy_mmry_list_for_workspace, proxy_mmry_memory_for_workspace,
    proxy_mmry_search_for_workspace,
};

// Re-export tests module
#[cfg(test)]
pub(crate) mod tests {
    #[allow(unused_imports)]
    pub use super::builder::tests::*;
}
