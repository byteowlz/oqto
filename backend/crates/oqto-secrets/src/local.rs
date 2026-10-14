use crate::storage::{MAX_VAULT_BYTES, Storage};
use crate::{
    ProviderError, ProviderStatus, SecretMetadata, SecretProvider, SecretString, SecretValue,
    valid_id,
};
use age::secrecy::ExposeSecret;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::path::Path;
use zeroize::Zeroizing;

const MAX_ENTRIES: usize = 256;
// Bound decryption resource use independently of untrusted vault headers.
const SCRYPT_WORK_FACTOR: u8 = 16;

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Entry {
    revision: u64,
    #[serde(
        serialize_with = "serialize_value",
        deserialize_with = "deserialize_value"
    )]
    value: SecretValue,
}

fn serialize_value<S: serde::Serializer>(
    value: &SecretValue,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(value.expose_to_mediator())
}

fn deserialize_value<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<SecretValue, D::Error> {
    SecretValue::new(String::deserialize(deserializer)?)
        .map_err(|_| serde::de::Error::custom("invalid credential"))
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Vault {
    schema: u32,
    account: String,
    generation: u64,
    entries: BTreeMap<String, Entry>,
}

struct Unlocked {
    passphrase: SecretString,
    vault: Vault,
}

pub struct LocalProvider {
    account: String,
    storage: Storage,
    initialized: bool,
    unlocked: Option<Unlocked>,
}

impl LocalProvider {
    /// The trusted runner config chooses a dedicated private directory and
    /// Account. Does not inspect/import arbitrary workspace or .env paths.
    pub fn open(root: &Path, account: &str) -> Result<Self, ProviderError> {
        if !valid_id(account) {
            return Err(ProviderError::InvalidInput);
        }
        let storage = Storage::open(root)?;
        let initialized = storage.exists()?;
        Ok(Self {
            account: account.to_owned(),
            storage,
            initialized,
            unlocked: None,
        })
    }

    pub fn initialize(
        &mut self,
        account: &str,
        passphrase: SecretString,
    ) -> Result<(), ProviderError> {
        self.check_owner(account)?;
        if self.initialized || self.storage.exists()? {
            return Err(ProviderError::AlreadyInitialized);
        }
        check_passphrase(&passphrase)?;
        let vault = Vault {
            schema: 1,
            account: self.account.clone(),
            generation: 0,
            entries: BTreeMap::new(),
        };
        let ciphertext = encrypt(&vault, &passphrase)?;
        if let Err(error) = self.storage.replace(&ciphertext) {
            self.unlocked = None;
            self.initialized = self.storage.exists().unwrap_or(true);
            return Err(error);
        }
        self.initialized = true;
        self.unlocked = Some(Unlocked { passphrase, vault });
        Ok(())
    }

    /// Failed unlock clears any previously unlocked state, including on wrong
    /// passphrase, corruption or owner mismatch. Restart always starts locked.
    pub fn unlock(&mut self, account: &str, passphrase: SecretString) -> Result<(), ProviderError> {
        self.unlocked = None;
        self.check_owner(account)?;
        if !self.initialized {
            return Err(ProviderError::Uninitialized);
        }
        check_passphrase(&passphrase)?;
        let bytes = self.storage.read()?;
        let vault = decrypt(&bytes, &passphrase)?;
        if vault.account != self.account {
            return Err(ProviderError::WrongOwner);
        }
        if vault.schema != 1
            || vault.entries.len() > MAX_ENTRIES
            || vault.entries.iter().any(|(id, entry)| {
                !valid_id(id) || entry.revision == 0 || entry.revision > vault.generation
            })
        {
            return Err(ProviderError::Integrity);
        }
        self.unlocked = Some(Unlocked { passphrase, vault });
        Ok(())
    }

    pub fn lock(&mut self) {
        self.unlocked = None;
    }

    fn check_owner(&self, account: &str) -> Result<(), ProviderError> {
        if account != self.account {
            return Err(ProviderError::WrongOwner);
        }
        Ok(())
    }

    fn vault(&self, account: &str) -> Result<&Vault, ProviderError> {
        self.check_owner(account)?;
        if !self.initialized {
            return Err(ProviderError::Uninitialized);
        }
        let state = self.unlocked.as_ref().ok_or(ProviderError::Locked)?;
        self.storage.ensure_anchor()?;
        Ok(&state.vault)
    }

    fn commit(&mut self, vault: Vault) -> Result<(), ProviderError> {
        let state = self.unlocked.as_ref().ok_or(ProviderError::Locked)?;
        let ciphertext = encrypt(&vault, &state.passphrase)?;
        if let Err(error) = self.storage.replace(&ciphertext) {
            // Rename/fsync can have an uncertain result. Force re-unlock from
            // disk instead of serving stale in-memory state or retrying blindly.
            self.lock();
            return Err(error);
        }
        self.unlocked.as_mut().ok_or(ProviderError::Locked)?.vault = vault;
        Ok(())
    }
}

impl SecretProvider for LocalProvider {
    fn status(&self) -> ProviderStatus {
        if self.storage.ensure_anchor().is_err() {
            return ProviderStatus::Unavailable;
        }
        if !self.initialized {
            ProviderStatus::Uninitialized
        } else if self.unlocked.is_some() {
            ProviderStatus::Ready
        } else {
            ProviderStatus::Locked
        }
    }

    fn list(&self, account: &str) -> Result<Vec<SecretMetadata>, ProviderError> {
        Ok(self
            .vault(account)?
            .entries
            .iter()
            .map(|(id, entry)| SecretMetadata {
                id: id.clone(),
                revision: entry.revision,
            })
            .collect())
    }

    fn put(
        &mut self,
        account: &str,
        id: &str,
        value: SecretValue,
        expected_revision: Option<u64>,
    ) -> Result<SecretMetadata, ProviderError> {
        let current = self.vault(account)?;
        if !valid_id(id) {
            return Err(ProviderError::InvalidInput);
        }
        match (current.entries.get(id), expected_revision) {
            (None, None) => {}
            (Some(entry), Some(revision)) if entry.revision == revision => {}
            _ => return Err(ProviderError::Conflict),
        }
        if !current.entries.contains_key(id) && current.entries.len() >= MAX_ENTRIES {
            return Err(ProviderError::InvalidInput);
        }
        let mut next = current.clone();
        next.generation = next
            .generation
            .checked_add(1)
            .ok_or(ProviderError::InvalidInput)?;
        let metadata = SecretMetadata {
            id: id.to_owned(),
            revision: next.generation,
        };
        next.entries.insert(
            id.to_owned(),
            Entry {
                revision: metadata.revision,
                value,
            },
        );
        self.commit(next)?;
        Ok(metadata)
    }

    fn delete(
        &mut self,
        account: &str,
        id: &str,
        expected_revision: u64,
    ) -> Result<(), ProviderError> {
        let current = self.vault(account)?;
        let entry = current.entries.get(id).ok_or(ProviderError::NotFound)?;
        if entry.revision != expected_revision {
            return Err(ProviderError::Conflict);
        }
        let mut next = current.clone();
        next.entries.remove(id);
        next.generation = next
            .generation
            .checked_add(1)
            .ok_or(ProviderError::InvalidInput)?;
        self.commit(next)
    }

    fn resolve_for_mediator(&self, account: &str, id: &str) -> Result<SecretValue, ProviderError> {
        self.vault(account)?
            .entries
            .get(id)
            .map(|entry| entry.value.clone())
            .ok_or(ProviderError::NotFound)
    }
}

fn check_passphrase(passphrase: &SecretString) -> Result<(), ProviderError> {
    if passphrase.expose_secret().len() < 16 || passphrase.expose_secret().len() > 4096 {
        return Err(ProviderError::InvalidInput);
    }
    Ok(())
}

fn encrypt(vault: &Vault, passphrase: &SecretString) -> Result<Vec<u8>, ProviderError> {
    let plaintext =
        Zeroizing::new(serde_json::to_vec(vault).map_err(|_| ProviderError::Integrity)?);
    if plaintext.len() > MAX_VAULT_BYTES - 1024 {
        return Err(ProviderError::InvalidInput);
    }
    let mut recipient = age::scrypt::Recipient::new(passphrase.clone());
    recipient.set_work_factor(SCRYPT_WORK_FACTOR);
    let encryptor =
        age::Encryptor::with_recipients(std::iter::once(&recipient as &dyn age::Recipient))
            .map_err(|_| ProviderError::Integrity)?;
    let mut ciphertext = Vec::new();
    let mut writer = encryptor
        .wrap_output(&mut ciphertext)
        .map_err(|_| ProviderError::Integrity)?;
    writer
        .write_all(&plaintext)
        .map_err(|_| ProviderError::Integrity)?;
    writer.finish().map_err(|_| ProviderError::Integrity)?;
    Ok(ciphertext)
}

fn decrypt(ciphertext: &[u8], passphrase: &SecretString) -> Result<Vault, ProviderError> {
    let decryptor = age::Decryptor::new(ciphertext).map_err(|_| ProviderError::Integrity)?;
    if !decryptor.is_scrypt() {
        return Err(ProviderError::Integrity);
    }
    let mut identity = age::scrypt::Identity::new(passphrase.clone());
    identity.set_max_work_factor(SCRYPT_WORK_FACTOR);
    let reader = decryptor
        .decrypt(std::iter::once(&identity as &dyn age::Identity))
        .map_err(|_| ProviderError::Integrity)?;
    let mut plaintext = Zeroizing::new(Vec::new());
    reader
        .take((MAX_VAULT_BYTES + 1) as u64)
        .read_to_end(&mut plaintext)
        .map_err(|_| ProviderError::Integrity)?;
    if plaintext.len() > MAX_VAULT_BYTES {
        return Err(ProviderError::Integrity);
    }
    serde_json::from_slice(&plaintext).map_err(|_| ProviderError::Integrity)
}
