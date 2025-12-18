//! EAVS (LLM Proxy) client module.
//!
//! Provides an async client for managing virtual API keys in EAVS.

#![allow(dead_code)]

mod client;
mod error;
mod types;

use async_trait::async_trait;

pub use client::EavsClient;
#[allow(unused_imports)]
pub use error::{EavsError, EavsResult};
pub use types::*;

/// Minimal EAVS API abstraction for testability.
#[async_trait]
pub trait EavsApi: Send + Sync {
    async fn create_key(&self, request: CreateKeyRequest) -> EavsResult<CreateKeyResponse>;
    async fn revoke_key(&self, key_id_or_hash: &str) -> EavsResult<()>;
}

#[async_trait]
impl EavsApi for EavsClient {
    async fn create_key(&self, request: CreateKeyRequest) -> EavsResult<CreateKeyResponse> {
        self.create_key(request).await
    }

    async fn revoke_key(&self, key_id_or_hash: &str) -> EavsResult<()> {
        self.revoke_key(key_id_or_hash).await
    }
}
