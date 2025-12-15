//! Storage abstraction layer.
//!
//! Provides a trait-based storage abstraction with implementations for:
//! - Local filesystem storage (development)
//! - Azure Blob Storage (production)

mod error;
mod local;
mod traits;

pub use error::{StorageError, StorageResult};
pub use local::LocalStorage;
pub use traits::{Storage, StorageMetadata};

/// Create storage based on configuration.
pub fn create_storage(config: StorageConfig) -> Box<dyn Storage> {
    match config {
        StorageConfig::Local(path) => Box::new(LocalStorage::new(path)),
        // Azure blob storage can be added later when azure_storage crate is added
    }
}

/// Storage configuration.
#[derive(Debug, Clone)]
pub enum StorageConfig {
    /// Local filesystem storage.
    Local(String),
    // AzureBlob { account: String, container: String, key: String },
}

impl Default for StorageConfig {
    fn default() -> Self {
        StorageConfig::Local("/tmp/workspace-storage".to_string())
    }
}
