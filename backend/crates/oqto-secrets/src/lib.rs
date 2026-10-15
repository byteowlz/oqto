//! Trusted credential-provider internals, not an App/agent secret-reading API.
//!
//! Callers must authenticate provisioning and independently authorize credential use.
//! No method in this crate creates a grant or permits handing values to a workload.

mod kyz;
mod local;
pub use kyz::KyzProvider;
mod storage;

pub use age::secrecy::SecretString;
pub use local::LocalProvider;
use std::fmt;

pub const MAX_SECRET_BYTES: usize = 65_536;

/// Secret-bearing input/output confined to the trusted provider boundary.
/// Deliberately has no Display or transport serialization implementation.
///
/// Ordinary response serialization must remain impossible:
/// ```compile_fail
/// let value = oqto_secrets::SecretValue::new("synthetic-only".to_owned()).unwrap();
/// let _ = serde_json::to_string(&value);
/// ```
#[derive(Clone)]
pub struct SecretValue(SecretString);

impl SecretValue {
    pub fn new(value: String) -> Result<Self, ProviderError> {
        let secret = SecretString::from(value);
        use age::secrecy::ExposeSecret;
        if secret.expose_secret().is_empty() || secret.expose_secret().len() > MAX_SECRET_BYTES {
            return Err(ProviderError::InvalidInput);
        }
        Ok(Self(secret))
    }

    /// Only trusted credential mediation may consume these bytes. Never expose
    /// this method through a Host Bridge, MCP resource/tool, or ordinary API.
    pub fn expose_to_mediator(&self) -> &str {
        use age::secrecy::ExposeSecret;
        self.0.expose_secret()
    }
}

impl fmt::Debug for SecretValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SecretValue([REDACTED])")
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SecretMetadata {
    pub id: String,
    pub revision: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProviderStatus {
    Unavailable,
    Uninitialized,
    Locked,
    Ready,
}

/// Internal provider seam. The authenticated host supplies the acting Account;
/// provider ownership checks do not replace connection/action/destination grants.
pub trait SecretProvider {
    fn status(&self) -> ProviderStatus;
    fn list(&self, account: &str) -> Result<Vec<SecretMetadata>, ProviderError>;
    fn put(
        &mut self,
        account: &str,
        id: &str,
        value: SecretValue,
        expected_revision: Option<u64>,
    ) -> Result<SecretMetadata, ProviderError>;
    fn delete(
        &mut self,
        account: &str,
        id: &str,
        expected_revision: u64,
    ) -> Result<(), ProviderError>;
    fn resolve_for_mediator(&self, account: &str, id: &str) -> Result<SecretValue, ProviderError>;
    /// Trusted callers serialize local-provider access. Providers permitting
    /// external writers must override this with a single atomic snapshot read.
    fn snapshot_for_mediator(
        &self,
        account: &str,
        id: &str,
    ) -> Result<(SecretValue, u64), ProviderError> {
        let metadata = self
            .list(account)?
            .into_iter()
            .find(|entry| entry.id == id)
            .ok_or(ProviderError::NotFound)?;
        Ok((self.resolve_for_mediator(account, id)?, metadata.revision))
    }
}

/// Stable sanitized failures: underlying parser/I/O/provider diagnostics may
/// contain secret values or private paths and must not cross this boundary.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ProviderError {
    #[error("credential provider is locked")]
    Locked,
    #[error("credential provider is not initialized")]
    Uninitialized,
    #[error("credential provider already exists")]
    AlreadyInitialized,
    #[error("credential provider belongs to another Account")]
    WrongOwner,
    #[error("credential does not exist")]
    NotFound,
    #[error("credential revision conflict")]
    Conflict,
    #[error("invalid or oversized credential input")]
    InvalidInput,
    #[error("credential provider authentication or integrity check failed")]
    Integrity,
    #[error("credential storage permissions or file type are unsafe")]
    UnsafeStorage,
    #[error("credential provider has another writer")]
    Busy,
    #[error("credential storage is unavailable")]
    Storage,
    #[error("credential storage assurance is unsupported on this platform")]
    UnsupportedPlatform,
}

fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
        && id != "."
        && id != ".."
}
