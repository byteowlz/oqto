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

/// Generate Pi models.json content from eavs provider details.
///
/// Creates one Pi provider per eavs provider, each pointing at eavs
/// with the correct path-prefix routing and Pi API type.
pub fn generate_pi_models_json(
    providers: &[ProviderDetail],
    eavs_base_url: &str,
) -> serde_json::Value {
    let mut pi_providers = serde_json::Map::new();
    let base = eavs_base_url.trim_end_matches('/');

    for provider in providers {
        let pi_api = match provider.pi_api.as_deref() {
            Some(a) => a,
            None => continue,
        };

        // Skip "default" provider (alias)
        if provider.name == "default" {
            continue;
        }

        let base_url = format!("{}/{}/v1", base, provider.name);

        let models: Vec<serde_json::Value> = provider
            .models
            .iter()
            .map(|m| {
                let cost_obj = serde_json::json!({
                    "input": m.cost.input,
                    "output": m.cost.output,
                    "cacheRead": m.cost.cache_read,
                    "cacheWrite": 0
                });
                serde_json::json!({
                    "id": m.id,
                    "name": if m.name.is_empty() { &m.id } else { &m.name },
                    "reasoning": m.reasoning,
                    "input": if m.input.is_empty() { vec!["text".to_string()] } else { m.input.clone() },
                    "contextWindow": m.context_window,
                    "maxTokens": m.max_tokens,
                    "cost": cost_obj
                })
            })
            .collect();

        let pi_provider = serde_json::json!({
            "baseUrl": base_url,
            "api": pi_api,
            "apiKey": "EAVS_API_KEY",
            "models": models,
        });

        let pi_name = format!("eavs-{}", provider.name);
        pi_providers.insert(pi_name, pi_provider);
    }

    serde_json::json!({
        "providers": pi_providers,
    })
}
