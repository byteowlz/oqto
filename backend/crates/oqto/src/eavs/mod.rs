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
///
/// `api_key` is the user's eavs virtual key. It is embedded directly in
/// models.json so Pi can authenticate against the local eavs proxy without
/// needing an environment variable. The key is only valid on 127.0.0.1 so
/// there is no security concern embedding it in the file.
pub fn generate_pi_models_json(
    providers: &[ProviderDetail],
    eavs_base_url: &str,
    api_key: Option<&str>,
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
                    "cacheWrite": m.cost.input  // default cacheWrite = input cost
                });
                let mut model_obj = serde_json::json!({
                    "id": m.id,
                    "name": if m.name.is_empty() { &m.id } else { &m.name },
                    "reasoning": m.reasoning,
                    "input": if m.input.is_empty() { vec!["text".to_string()] } else { m.input.clone() },
                    "contextWindow": m.context_window,
                    "maxTokens": m.max_tokens,
                    "cost": cost_obj
                });
                // Add compat flags if present (e.g., supportsDeveloperRole)
                if !m.compat.is_empty() {
                    model_obj["compat"] = serde_json::json!(m.compat);
                }
                model_obj
            })
            .collect();

        // Build provider entry with all fields Pi needs.
        // Use the actual eavs virtual key if provided, otherwise use "not-needed"
        // as a passthrough value. Keys are embedded directly in models.json so
        // Pi can use them without env file indirection.
        let key_value = api_key.unwrap_or("not-needed");
        let mut pi_provider = serde_json::json!({
            "baseUrl": base_url,
            "api": pi_api,
            "apiKey": key_value,
            "models": models,
        });

        // Add custom headers if the provider requires them (e.g., Azure api-key).
        // The eavs proxy injects the real values; here we use the virtual key
        // placeholder so Pi sends it and eavs can authenticate the request.
        if !provider.headers.is_empty() {
            pi_provider["headers"] = serde_json::json!(provider.headers);
        }

        let pi_name = format!("eavs-{}", provider.name);
        pi_providers.insert(pi_name, pi_provider);
    }

    serde_json::json!({
        "providers": pi_providers,
    })
}
