//! What a client keeps of one Session between runs, and an encrypted file store for it.
//!
//! A cache belongs to one host, Session and `store` (contract "Caching" rule 1). Records are
//! sealed with XChaCha20-Poly1305 under a 32-byte key from the OS keychain, or from a 0600
//! key file where no keychain exists; the file name is bound into the seal so records cannot
//! be swapped between Sessions.

use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use oqto_session_view::{CacheKey, Receipt, TranscriptRecord};
use serde::{Deserialize, Serialize};

use crate::client::Tracked;

const MAGIC: &[u8; 5] = b"OQTC1";
const NONCE: usize = 24;

/// One Session's cached state.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CacheRecord {
    /// Record format, 1.
    pub v: u32,
    /// The `store` the transcript belongs to; `None` until the first `attached`.
    pub store: Option<String>,
    /// Held entries and gaps.
    pub transcript: TranscriptRecord,
    /// Requests not yet settled or not yet shown as entries.
    pub requests: Vec<Tracked>,
    /// Latest receipt of every request this client heard of.
    pub receipts: BTreeMap<String, Receipt>,
}

impl CacheRecord {
    /// Keep at most `max_entries` entries, dropping the oldest; the dropped range becomes
    /// "earlier" again and is read back on demand.
    pub fn truncate_oldest(&mut self, max_entries: usize) {
        let transcript = &mut self.transcript;
        let excess = transcript.entries.len().saturating_sub(max_entries);
        if excess == 0 {
            return;
        }
        let dropped: Vec<i64> = transcript
            .entries
            .drain(..excess)
            .map(|e| i64::try_from(e.id()).unwrap_or(i64::MAX))
            .collect();
        let mut low = dropped
            .last()
            .copied()
            .unwrap_or(i64::MIN)
            .max(transcript.low.unwrap_or(i64::MIN));
        // A gap that reaches into the dropped range leaves nothing known below its upper end.
        for gap in &transcript.gaps {
            if gap.after < low && gap.up_to > low {
                low = gap.up_to - 1;
            }
        }
        transcript.gaps.retain(|gap| gap.after >= low);
        transcript.low = Some(low);
    }
}

/// Where the cache key comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeySource {
    /// The OS keychain (macOS Keychain, Linux kernel keyring), falling back to the file.
    Auto(PathBuf),
    /// A 0600 key file (headless servers, tests).
    File(PathBuf),
}

/// Which key store is in use, for `doctor`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyKind {
    /// The OS keychain.
    Keychain,
    /// A 0600 file.
    File,
}

/// Encrypted per-Session cache files in one directory.
pub struct FileCache {
    dir: PathBuf,
    cipher: XChaCha20Poly1305,
    kind: KeyKind,
}

impl std::fmt::Debug for FileCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FileCache")
            .field("dir", &self.dir)
            .field("kind", &self.kind)
            .finish_non_exhaustive()
    }
}

fn random<const N: usize>() -> Result<[u8; N]> {
    let mut bytes = [0u8; N];
    getrandom::getrandom(&mut bytes).map_err(|e| anyhow!("random bytes: {e}"))?;
    Ok(bytes)
}

fn decode_key(text: &str) -> Result<[u8; 32]> {
    let bytes = hex::decode(text.trim()).context("cache key is not hex")?;
    bytes
        .try_into()
        .map_err(|_| anyhow!("cache key is not 32 bytes"))
}

#[cfg(unix)]
fn private_dir(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::create_dir_all(path).with_context(|| format!("create {}", path.display()))?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
        .with_context(|| format!("chmod {}", path.display()))
}

#[cfg(not(unix))]
fn private_dir(path: &Path) -> Result<()> {
    fs::create_dir_all(path).with_context(|| format!("create {}", path.display()))
}

fn write_private(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path.parent().context("cache path without a parent")?;
    private_dir(parent)?;
    let temp = path.with_extension("tmp");
    let mut options = fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(&temp)
        .with_context(|| format!("open {}", temp.display()))?;
    file.write_all(bytes)
        .with_context(|| format!("write {}", temp.display()))?;
    file.sync_all().ok();
    fs::rename(&temp, path).with_context(|| format!("rename to {}", path.display()))
}

fn file_key(path: &Path) -> Result<[u8; 32]> {
    match fs::read_to_string(path) {
        Ok(text) => decode_key(&text),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let key = random::<32>()?;
            write_private(path, hex::encode(key).as_bytes())?;
            Ok(key)
        }
        Err(error) => Err(error).with_context(|| format!("read {}", path.display())),
    }
}

fn keychain_key(account: &str) -> Result<[u8; 32]> {
    let entry =
        keyring::Entry::new("oqt-tui session cache", account).context("open keychain entry")?;
    match entry.get_password() {
        Ok(text) => decode_key(&text),
        Err(keyring::Error::NoEntry) => {
            let key = random::<32>()?;
            entry
                .set_password(&hex::encode(key))
                .context("store cache key in the keychain")?;
            Ok(key)
        }
        Err(error) => Err(anyhow!("keychain: {error}")),
    }
}

impl FileCache {
    /// Open the cache directory with a key from `source`.
    pub fn open(dir: impl Into<PathBuf>, source: &KeySource) -> Result<Self> {
        let dir = dir.into();
        private_dir(&dir)?;
        let (key, kind) = match source {
            KeySource::File(path) => (file_key(path)?, KeyKind::File),
            KeySource::Auto(path) => {
                let account =
                    oqto_session_view::canonical::sha256(&serde_json::json!(dir.to_string_lossy()));
                match keychain_key(&account) {
                    Ok(key) => (key, KeyKind::Keychain),
                    Err(_) => (file_key(path)?, KeyKind::File),
                }
            }
        };
        let cipher =
            XChaCha20Poly1305::new_from_slice(&key).map_err(|_| anyhow!("cache key length"))?;
        Ok(Self { dir, cipher, kind })
    }

    /// Which key store is in use.
    pub const fn key_kind(&self) -> KeyKind {
        self.kind
    }

    /// The file holding `key`'s record.
    pub fn path(&self, key: &CacheKey) -> PathBuf {
        self.dir.join(format!("{}.oqtc", key.file_stem()))
    }

    /// Load a record. A missing file is `None`; a file that does not open is an error.
    pub fn load(&self, key: &CacheKey) -> Result<Option<CacheRecord>> {
        let path = self.path(key);
        let bytes = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error).with_context(|| format!("read {}", path.display())),
        };
        if bytes.len() < MAGIC.len() + NONCE || &bytes[..MAGIC.len()] != MAGIC {
            bail!("{} is not a session cache", path.display());
        }
        let (nonce, sealed) = bytes[MAGIC.len()..].split_at(NONCE);
        let aad = key.file_stem();
        let plain = self
            .cipher
            .decrypt(
                XNonce::from_slice(nonce),
                Payload {
                    msg: sealed,
                    aad: aad.as_bytes(),
                },
            )
            .map_err(|_| anyhow!("{} does not open with this key", path.display()))?;
        let record: CacheRecord = serde_json::from_slice(&plain).context("cache record")?;
        Ok((record.v == 1).then_some(record))
    }

    /// Seal and write a record atomically.
    pub fn save(&self, key: &CacheKey, record: &CacheRecord) -> Result<()> {
        let plain = serde_json::to_vec(record).context("encode cache record")?;
        let nonce = random::<NONCE>()?;
        let aad = key.file_stem();
        let sealed = self
            .cipher
            .encrypt(
                XNonce::from_slice(&nonce),
                Payload {
                    msg: &plain,
                    aad: aad.as_bytes(),
                },
            )
            .map_err(|_| anyhow!("seal cache record"))?;
        let mut bytes = Vec::with_capacity(MAGIC.len() + NONCE + sealed.len());
        bytes.extend_from_slice(MAGIC);
        bytes.extend_from_slice(&nonce);
        bytes.extend_from_slice(&sealed);
        write_private(&self.path(key), &bytes)
    }

    /// Purge: delete everything kept for this host and Session.
    pub fn remove(&self, key: &CacheKey) -> Result<()> {
        match fs::remove_file(self.path(key)) {
            Err(error) if error.kind() != std::io::ErrorKind::NotFound => {
                Err(error).context("delete cache")
            }
            _ => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oqto_session_view::{Entry, Transcript};
    use serde_json::json;

    fn record() -> Result<CacheRecord> {
        let mut transcript = Transcript::new();
        let entries: Vec<Entry> = (1..=5)
            .map(|id| Entry::new(json!({"id": id, "conversationId": 1, "kind": "pi.user", "model": [{"role": "user", "content": "secret words"}]})))
            .collect::<std::result::Result<_, _>>()?;
        transcript.merge(&entries, None);
        Ok(CacheRecord {
            v: 1,
            store: Some("st_1".into()),
            transcript: transcript.to_record(),
            requests: vec![],
            receipts: BTreeMap::new(),
        })
    }

    #[test]
    fn round_trip_is_encrypted_and_bound_to_its_key() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let cache = FileCache::open(
            temp.path().join("c"),
            &KeySource::File(temp.path().join("key")),
        )?;
        let key = CacheKey::new("ws://h/session", "ses_a");
        let saved = record()?;
        cache.save(&key, &saved)?;
        let bytes = fs::read(cache.path(&key))?;
        assert!(!String::from_utf8_lossy(&bytes).contains("secret words"));
        assert_eq!(cache.load(&key)?, Some(saved));
        // A record copied to another Session's name does not open.
        let other = CacheKey::new("ws://h/session", "ses_b");
        fs::copy(cache.path(&key), cache.path(&other))?;
        assert!(cache.load(&other).is_err());
        // Tampering is refused.
        let mut tampered = bytes;
        if let Some(last) = tampered.last_mut() {
            *last ^= 1;
        }
        fs::write(cache.path(&key), tampered)?;
        assert!(cache.load(&key).is_err());
        cache.remove(&key)?;
        assert_eq!(cache.load(&key)?, None);
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn files_are_private() -> Result<()> {
        use std::os::unix::fs::PermissionsExt;
        let temp = tempfile::tempdir()?;
        let cache = FileCache::open(
            temp.path().join("c"),
            &KeySource::File(temp.path().join("k/key")),
        )?;
        let key = CacheKey::new("h", "s");
        cache.save(&key, &record()?)?;
        assert_eq!(
            fs::metadata(cache.path(&key))?.permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(
            fs::metadata(temp.path().join("k/key"))?
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        Ok(())
    }

    #[test]
    fn truncation_keeps_the_newest_and_marks_earlier() -> Result<()> {
        let mut saved = record()?;
        saved.truncate_oldest(2);
        let ids: Vec<u64> = saved.transcript.entries.iter().map(Entry::id).collect();
        assert_eq!(ids, vec![4, 5]);
        assert_eq!(
            Transcript::from_record(saved.transcript).earlier_bound(),
            Some(3)
        );
        Ok(())
    }
}
