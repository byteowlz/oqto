//! Explicit Account-owned kyz v5 provider. Never uses CLI sessions, keyring,
//! ambient vault discovery, environment credentials or automatic migration.
use crate::{
    ProviderError, ProviderStatus, SecretMetadata, SecretProvider, SecretString, SecretValue,
};
use kyz_core::{CoreError, UnlockedVault, VaultStore};
use secrecy::ExposeSecret;
use std::{collections::BTreeMap, path::PathBuf};

pub struct KyzProvider {
    account: String,
    store: VaultStore,
    unlocked: Option<UnlockedVault>,
}
impl KyzProvider {
    pub fn open(path: PathBuf, account: &str) -> Result<Self, ProviderError> {
        if !path.is_absolute() || !crate::valid_id(account) {
            return Err(ProviderError::InvalidInput);
        }
        // Preserve-first: missing vaults must be created explicitly with kyz;
        // never turn a typo into an empty, silently selected new vault.
        let metadata = std::fs::symlink_metadata(&path).map_err(|_| ProviderError::Storage)?;
        if !metadata.is_file() || metadata.file_type().is_symlink() {
            return Err(ProviderError::UnsafeStorage);
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            if metadata.uid() != rustix::process::geteuid().as_raw() || metadata.mode() & 0o077 != 0
            {
                return Err(ProviderError::UnsafeStorage);
            }
        }
        #[cfg(not(unix))]
        return Err(ProviderError::UnsupportedPlatform);
        Ok(Self {
            account: account.to_owned(),
            store: VaultStore::new(path),
            unlocked: None,
        })
    }
    fn owner(&self, account: &str) -> Result<(), ProviderError> {
        if self.account == account {
            Ok(())
        } else {
            Err(ProviderError::WrongOwner)
        }
    }
    fn service(&self) -> String {
        format!("oqto/{}", self.account)
    }
    fn vault(&self) -> Result<&UnlockedVault, ProviderError> {
        self.unlocked.as_ref().ok_or(ProviderError::Locked)
    }
    pub fn unlock(&mut self, account: &str, passphrase: SecretString) -> Result<(), ProviderError> {
        self.owner(account)?;
        use age::secrecy::ExposeSecret;
        let passphrase = secrecy::SecretString::from(passphrase.expose_secret().to_owned());
        let opened = self.store.open_in_memory(&passphrase).map_err(map_error)?;
        self.unlocked = Some(opened);
        Ok(())
    }
    pub fn lock(&mut self) {
        self.unlocked = None;
    }
    pub fn initialize(
        &mut self,
        account: &str,
        _passphrase: SecretString,
    ) -> Result<(), ProviderError> {
        self.owner(account)?;
        Err(ProviderError::AlreadyInitialized)
    }
}
// Wire contract uses u64 revisions. They are opaque change detectors, not
// authorization tokens; the full authenticated revision is checked atomically
// by kyz after the adapter checks this wire representation.
fn revision(value: &str) -> Result<u64, ProviderError> {
    value
        .get(..16)
        .and_then(|v| u64::from_str_radix(v, 16).ok())
        .ok_or(ProviderError::Integrity)
}
fn map_error(error: CoreError) -> ProviderError {
    match error {
        CoreError::RevisionConflict => ProviderError::Conflict,
        CoreError::SecretNotFound(_) => ProviderError::NotFound,
        CoreError::MigrationRequired { .. } => ProviderError::UnsupportedPlatform,
        _ => ProviderError::Integrity,
    }
}
impl SecretProvider for KyzProvider {
    fn snapshot_for_mediator(
        &self,
        account: &str,
        id: &str,
    ) -> Result<(SecretValue, u64), ProviderError> {
        self.owner(account)?;
        let (entry, token) = self
            .vault()?
            .get_versioned(&self.service(), id)
            .map_err(map_error)?;
        let value = entry
            .fields
            .get("token")
            .ok_or(ProviderError::InvalidInput)?;
        Ok((
            SecretValue::new(value.expose_secret().to_owned())?,
            revision(&token)?,
        ))
    }

    fn status(&self) -> ProviderStatus {
        if self.unlocked.is_some() {
            ProviderStatus::Ready
        } else {
            ProviderStatus::Locked
        }
    }
    fn list(&self, account: &str) -> Result<Vec<SecretMetadata>, ProviderError> {
        self.owner(account)?;
        let vault = self.vault()?;
        let service = self.service();
        let entries = vault.list(&service).map_err(map_error)?;
        if entries.len() > 256 {
            return Err(ProviderError::InvalidInput);
        }
        entries
            .into_iter()
            .map(|entry| {
                let (_, token) = vault
                    .get_versioned(&service, &entry.key)
                    .map_err(map_error)?;
                Ok(SecretMetadata {
                    id: entry.key,
                    revision: revision(&token)?,
                })
            })
            .collect()
    }
    fn put(
        &mut self,
        account: &str,
        id: &str,
        value: SecretValue,
        expected: Option<u64>,
    ) -> Result<SecretMetadata, ProviderError> {
        self.owner(account)?;
        if !crate::valid_id(id) {
            return Err(ProviderError::InvalidInput);
        }
        let service = self.service();
        let vault = self.vault()?;
        let current = match expected {
            None => None,
            Some(expected) => {
                let (_, token) = vault.get_versioned(&service, id).map_err(map_error)?;
                if revision(&token)? != expected {
                    return Err(ProviderError::Conflict);
                }
                Some(token)
            }
        };
        let entry = kyz_core::SecretEntry::new(
            &service,
            id,
            BTreeMap::from([(
                "token".to_owned(),
                secrecy::SecretString::from(value.expose_to_mediator().to_owned()),
            )]),
        );
        vault
            .compare_set(&service, id, &entry, current.as_deref())
            .map_err(map_error)?;
        let (_, token) = vault.get_versioned(&service, id).map_err(map_error)?;
        Ok(SecretMetadata {
            id: id.to_owned(),
            revision: revision(&token)?,
        })
    }
    fn delete(&mut self, account: &str, id: &str, expected: u64) -> Result<(), ProviderError> {
        self.owner(account)?;
        let vault = self.vault()?;
        let service = self.service();
        let (_, token) = vault.get_versioned(&service, id).map_err(map_error)?;
        if revision(&token)? != expected {
            return Err(ProviderError::Conflict);
        }
        vault
            .compare_delete(&service, id, &token)
            .map_err(map_error)
    }
    fn resolve_for_mediator(&self, account: &str, id: &str) -> Result<SecretValue, ProviderError> {
        self.owner(account)?;
        let entry = self.vault()?.get(&self.service(), id).map_err(map_error)?;
        let value = entry
            .fields
            .get("token")
            .ok_or(ProviderError::InvalidInput)?;
        SecretValue::new(value.expose_secret().to_owned())
    }
}
