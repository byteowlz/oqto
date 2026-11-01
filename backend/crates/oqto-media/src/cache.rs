//! Disposable derived display products, never a source or authorization store.
use anyhow::{Context, Result, ensure};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    fs::{DirBuilder, File},
    io::{Read, Write},
    os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt},
    path::PathBuf,
    sync::{LazyLock, Mutex},
    time::{Duration, SystemTime},
};

pub const VARIANT: &str = "thumb-jpeg-fit768-budget128k-v1";
const ENTRY_LIMIT: u64 = 192 * 1024;
static LOCK: Mutex<()> = Mutex::new(());
static CONFIG: LazyLock<Result<Config, String>> =
    LazyLock::new(|| Config::from_env().map_err(|e| e.to_string()));

#[derive(Clone)]
pub struct Config {
    pub enabled: bool,
    pub directory: PathBuf,
    pub max_bytes: u64,
    pub idle_ttl: Duration,
}
impl Config {
    fn from_env() -> Result<Self> {
        // Unit readers must never populate an operator's cache, even when its
        // environment is inherited. Explicit Config tests and isolated actual-
        // binary HTTPS tests exercise the enabled cache instead.
        if cfg!(test) {
            return Ok(Self {
                enabled: false,
                directory: PathBuf::from("/unused-unit-media-cache"),
                max_bytes: 256 * 1024 * 1024,
                idle_ttl: Duration::from_secs(7 * 24 * 3600),
            });
        }
        let enabled = match std::env::var("OQTO_MEDIA_CACHE_ENABLED").as_deref() {
            Ok("true") => true,
            Err(std::env::VarError::NotPresent) => true,
            Ok("false") => false,
            _ => anyhow::bail!("OQTO_MEDIA_CACHE_ENABLED must be true or false"),
        };
        let directory = match std::env::var_os("OQTO_MEDIA_CACHE_DIR") {
            Some(p) => PathBuf::from(p),
            None => dirs::cache_dir()
                .context("media cache directory unavailable")?
                .join("oqto/media"),
        };
        ensure!(
            directory.is_absolute(),
            "media cache directory must be absolute"
        );
        let number = |name, default| -> Result<u64> {
            match std::env::var(name) {
                Ok(v) => Ok(v.parse().context("invalid media cache limit")?),
                Err(std::env::VarError::NotPresent) => Ok(default),
                Err(e) => Err(e.into()),
            }
        };
        let max_bytes = number("OQTO_MEDIA_CACHE_MAX_BYTES", 256 * 1024 * 1024)?;
        let ttl = number("OQTO_MEDIA_CACHE_IDLE_TTL_SECS", 7 * 24 * 3600)?;
        ensure!(
            max_bytes > 0 && ttl > 0,
            "media cache limits must be positive"
        );
        Ok(Self {
            enabled,
            directory,
            max_bytes,
            idle_ttl: Duration::from_secs(ttl),
        })
    }
    fn root(&self) -> Result<File> {
        ensure!(
            self.directory.is_absolute(),
            "media cache directory must be absolute"
        );
        ensure!(
            self.max_bytes > 0 && !self.idle_ttl.is_zero(),
            "media cache limits must be positive"
        );
        DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(&self.directory)?;
        let root = std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW)
            .open(&self.directory)?;
        let meta = root.metadata()?;
        ensure!(
            meta.uid() == rustix::process::geteuid().as_raw() && meta.mode() & 0o777 == 0o700,
            "unsafe media cache directory permissions"
        );
        Ok(root)
    }
    fn locked_root(&self) -> Result<File> {
        let root = self.root()?;
        // Another process must not stall a preview indefinitely.
        rustix::fs::flock(&root, rustix::fs::FlockOperation::NonBlockingLockExclusive)?;
        Ok(root)
    }
    fn name_for(hash: &str, variant: &str) -> String {
        format!("{hash}-{variant}.json")
    }
    #[cfg(test)]
    fn name(hash: &str) -> String {
        Self::name_for(hash, VARIANT)
    }
    fn sweep(&self, root: &File, reserve: u64) -> Result<()> {
        let dir = cap_std::fs::Dir::from_std_file(root.try_clone()?);
        let mut entries = Vec::new();
        let mut total = 0u64;
        for (index, entry) in dir.entries()?.enumerate() {
            ensure!(index < 4096, "media cache entry bound exceeded");
            let entry = entry?;
            let name = entry.file_name();
            let name = name.to_str().context("invalid cache entry")?;
            if let Some(id) = name.strip_prefix(".pending-")
                && uuid::Uuid::parse_str(id).is_ok()
            {
                rustix::fs::unlinkat(root, name, rustix::fs::AtFlags::empty())?;
                continue;
            }
            let owned = name.ends_with(".json")
                && name.len() > 70
                && name.len() < 256
                && name.as_bytes()[64] == b'-'
                && name.as_bytes()[65..name.len() - 5]
                    .iter()
                    .all(|b| b.is_ascii_alphanumeric() || *b == b'-')
                && name.as_bytes()[..64]
                    .iter()
                    .copied()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
            ensure!(owned, "unexpected media cache entry");
            let fd = rustix::fs::openat(
                root,
                name,
                rustix::fs::OFlags::RDONLY
                    | rustix::fs::OFlags::NOFOLLOW
                    | rustix::fs::OFlags::NONBLOCK,
                rustix::fs::Mode::empty(),
            )?;
            let file = File::from(fd);
            let meta = file.metadata()?;
            ensure!(
                meta.is_file()
                    && meta.nlink() == 1
                    && meta.mode() & 0o077 == 0
                    && meta.uid() == rustix::process::geteuid().as_raw(),
                "unsafe media cache entry"
            );
            let modified = meta.modified()?;
            let expired = match SystemTime::now().duration_since(modified) {
                Ok(age) => age >= self.idle_ttl,
                Err(_) => true, // A clock rollback must not extend cache retention.
            };
            if expired {
                rustix::fs::unlinkat(root, name, rustix::fs::AtFlags::empty())?;
            } else {
                total = total.saturating_add(meta.len());
                entries.push((modified, name.to_owned(), meta.len()));
            }
        }
        entries.sort_by_key(|e| e.0);
        let mut count = entries.len();
        for (_, name, size) in entries {
            if total.saturating_add(reserve) <= self.max_bytes && (reserve == 0 || count < 4096) {
                break;
            }
            rustix::fs::unlinkat(root, name.as_str(), rustix::fs::AtFlags::empty())?;
            total = total.saturating_sub(size);
            count = count.saturating_sub(1);
        }
        ensure!(
            total.saturating_add(reserve) <= self.max_bytes,
            "cache entry exceeds configured budget"
        );
        Ok(())
    }
    pub fn get(&self, hash: &str) -> Result<Option<Value>> {
        self.get_for(hash, VARIANT, 768)
    }
    pub fn get_for(&self, hash: &str, variant: &str, edge: u32) -> Result<Option<Value>> {
        validate_key(hash, variant, edge)?;
        if !self.enabled {
            return Ok(None);
        }
        let _lock = LOCK
            .lock()
            .map_err(|_| anyhow::anyhow!("media cache lock poisoned"))?;
        let root = self.locked_root()?;
        self.sweep(&root, 0)?;
        let name = Self::name_for(hash, variant);
        let fd = match rustix::fs::openat(
            &root,
            name.as_str(),
            rustix::fs::OFlags::RDONLY
                | rustix::fs::OFlags::NOFOLLOW
                | rustix::fs::OFlags::NONBLOCK,
            rustix::fs::Mode::empty(),
        ) {
            Ok(fd) => fd,
            Err(rustix::io::Errno::NOENT) => return Ok(None),
            Err(e) => return Err(e.into()),
        };
        let mut file = File::from(fd);
        let mut bytes = Vec::new();
        (&mut file).take(ENTRY_LIMIT + 1).read_to_end(&mut bytes)?;
        let value = serde_json::from_slice::<Value>(&bytes)
            .ok()
            .filter(|v| valid(v, hash, variant, edge));
        if bytes.len() as u64 > ENTRY_LIMIT || value.is_none() {
            rustix::fs::unlinkat(&root, name.as_str(), rustix::fs::AtFlags::empty())?;
            return Ok(None);
        }
        file.set_modified(SystemTime::now())?;
        Ok(value)
    }
    pub fn put(&self, hash: &str, value: &Value) -> Result<()> {
        self.put_for(hash, VARIANT, 768, value)
    }
    pub fn put_for(&self, hash: &str, variant: &str, edge: u32, value: &Value) -> Result<()> {
        validate_key(hash, variant, edge)?;
        if !self.enabled {
            return Ok(());
        }
        ensure!(
            valid(value, hash, variant, edge),
            "invalid derived media cache value"
        );
        let bytes = serde_json::to_vec(value)?;
        ensure!(
            bytes.len() as u64 <= ENTRY_LIMIT,
            "cache entry exceeds bound"
        );
        let _lock = LOCK
            .lock()
            .map_err(|_| anyhow::anyhow!("media cache lock poisoned"))?;
        let root = self.locked_root()?;
        self.sweep(&root, bytes.len() as u64)?;
        let temp = format!(".pending-{}", uuid::Uuid::new_v4());
        let fd = rustix::fs::openat(
            &root,
            temp.as_str(),
            rustix::fs::OFlags::WRONLY
                | rustix::fs::OFlags::CREATE
                | rustix::fs::OFlags::EXCL
                | rustix::fs::OFlags::NOFOLLOW,
            rustix::fs::Mode::from_raw_mode(0o600),
        )?;
        let outcome = (|| -> Result<()> {
            let mut file = File::from(fd);
            file.write_all(&bytes)?;
            file.sync_all()?;
            rustix::fs::renameat(
                &root,
                temp.as_str(),
                &root,
                Self::name_for(hash, variant).as_str(),
            )?;
            Ok(())
        })();
        if outcome.is_err() {
            let _ = rustix::fs::unlinkat(&root, temp.as_str(), rustix::fs::AtFlags::empty());
        }
        outcome
    }
}
fn validate_key(hash: &str, variant: &str, edge: u32) -> Result<()> {
    ensure!(
        hash.len() == 64
            && hash
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
        "invalid media hash"
    );
    ensure!(
        !variant.is_empty()
            && variant.len() <= 128
            && variant
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-'),
        "invalid media variant"
    );
    ensure!((1..=768).contains(&edge), "unsupported media preview size");
    Ok(())
}
fn valid(v: &Value, hash: &str, variant: &str, edge: u32) -> bool {
    if v["source_sha256"] != hash || v["variant"] != variant || v["mime_type"] != "image/jpeg" {
        return false;
    }
    let Some(data) = v["data_base64"].as_str().filter(|s| s.len() <= 176 * 1024) else {
        return false;
    };
    let Ok(bytes) = STANDARD.decode(data) else {
        return false;
    };
    crate::dimensions(&bytes, image::ImageFormat::Jpeg).is_ok_and(|(w, h)| {
        v["width"].as_u64() == Some(w as u64) && v["height"].as_u64() == Some(h as u64)
    }) && bytes.len() <= 128 * 1024
        && v["byte_size"].as_u64() == Some(bytes.len() as u64)
        && v["sha256"] == hex::encode(Sha256::digest(&bytes))
        && ["width", "height"]
            .into_iter()
            .all(|k| v[k].as_u64().is_some_and(|n| n > 0 && n <= edge as u64))
}
pub fn validate_config() -> Result<()> {
    let config = config()?;
    if config.enabled {
        config.root()?;
    }
    Ok(())
}
pub fn config() -> Result<&'static Config> {
    CONFIG.as_ref().map_err(|e| anyhow::anyhow!("{e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn config(path: PathBuf) -> Config {
        Config {
            enabled: true,
            directory: path,
            max_bytes: 1024 * 1024,
            idle_ttl: Duration::from_secs(60),
        }
    }
    fn value(hash: &str) -> Value {
        let mut bytes = Vec::new();
        image::codecs::jpeg::JpegEncoder::new(&mut bytes)
            .encode_image(&image::DynamicImage::new_rgb8(20, 10))
            .unwrap_or_else(|error| panic!("fixture JPEG: {error}"));
        serde_json::json!({"source_sha256":hash,"variant":VARIANT,"mime_type":"image/jpeg","sha256":hex::encode(Sha256::digest(&bytes)),"byte_size":bytes.len(),"width":20,"height":10,"data_base64":STANDARD.encode(&bytes)})
    }
    #[test]
    fn cache_hit_disabled_corruption_expiry_and_budget() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let mut c = config(dir.path().join("media"));
        let a = "a".repeat(64);
        let b = "b".repeat(64);
        let v = value(&a);
        assert!(c.get(&a)?.is_none());
        c.put(&a, &v)?;
        assert_eq!(c.get(&a)?, Some(v.clone()));
        c.enabled = false;
        assert!(c.get(&a)?.is_none());
        c.enabled = true;
        std::fs::write(c.directory.join(Config::name(&a)), b"broken")?;
        assert!(c.get(&a)?.is_none());
        c.put(&a, &v)?;
        let old = File::open(c.directory.join(Config::name(&a)))?;
        old.set_modified(SystemTime::UNIX_EPOCH)?;
        assert!(c.get(&a)?.is_none());
        c.put(&a, &v)?;
        File::open(c.directory.join(Config::name(&a)))?
            .set_modified(SystemTime::now() + Duration::from_secs(3600))?;
        assert!(
            c.get(&a)?.is_none(),
            "future timestamps must not extend retention"
        );
        c.max_bytes = serde_json::to_vec(&v)?.len() as u64 + 2;
        c.put(&a, &v)?;
        c.put(&b, &value(&b))?;
        assert!(c.get(&a)?.is_none());
        assert!(c.get(&b)?.is_some());
        Ok(())
    }
    #[test]
    fn warm_reads_refresh_lru_before_byte_budget_eviction() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let mut c = config(dir.path().join("media"));
        let a = "a".repeat(64);
        let b = "b".repeat(64);
        let d = "d".repeat(64);
        c.max_bytes = serde_json::to_vec(&value(&a))?.len() as u64 * 2 + 2;
        c.put(&a, &value(&a))?;
        c.put(&b, &value(&b))?;
        for hash in [&a, &b] {
            File::open(c.directory.join(Config::name(hash)))?
                .set_modified(SystemTime::now() - Duration::from_secs(20))?;
        }
        assert!(c.get(&a)?.is_some());
        c.put(&d, &value(&d))?;
        assert!(c.get(&b)?.is_none());
        assert!(c.get(&a)?.is_some());
        assert!(c.get(&d)?.is_some());
        Ok(())
    }

    #[test]
    fn concurrent_publication_partial_recovery_and_version_miss() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let c = config(dir.path().join("media"));
        let hash = "c".repeat(64);
        let v = value(&hash);
        let mut threads = Vec::new();
        for _ in 0..4 {
            let c = c.clone();
            let hash = hash.clone();
            let v = v.clone();
            threads.push(std::thread::spawn(move || c.put(&hash, &v)));
        }
        for thread in threads {
            thread
                .join()
                .map_err(|_| anyhow::anyhow!("cache writer panicked"))??;
        }
        assert_eq!(c.get(&hash)?, Some(v.clone()));
        let pending = c
            .directory
            .join(format!(".pending-{}", uuid::Uuid::new_v4()));
        std::fs::write(&pending, b"interrupted publication")?;
        assert_eq!(c.get(&hash)?, Some(v));
        assert!(!pending.exists());
        let current = c.directory.join(Config::name(&hash));
        let old = c.directory.join(format!("{hash}-thumb-old-v0.json"));
        std::fs::rename(current, old)?;
        assert!(
            c.get(&hash)?.is_none(),
            "different encoder version is not a cache hit"
        );
        let disabled = Config {
            enabled: false,
            ..config(dir.path().join("disabled"))
        };
        disabled.put(&hash, &value(&hash))?;
        assert!(!disabled.directory.exists());
        Ok(())
    }

    #[test]
    fn private_root_and_symlink_refusal() -> Result<()> {
        use std::os::unix::fs::{PermissionsExt, symlink};
        let dir = tempfile::tempdir()?;
        let c = config(dir.path().join("media"));
        let a = "a".repeat(64);
        c.put(&a, &value(&a))?;
        assert_eq!(std::fs::metadata(&c.directory)?.mode() & 0o777, 0o700);
        assert_eq!(
            std::fs::metadata(c.directory.join(Config::name(&a)))?.mode() & 0o777,
            0o600
        );
        let held = c.locked_root()?;
        assert!(
            c.get(&a).is_err(),
            "busy cache must fail promptly instead of blocking preview work"
        );
        drop(held);
        std::fs::remove_file(c.directory.join(Config::name(&a)))?;
        symlink(
            dir.path().join("outside"),
            c.directory.join(Config::name(&a)),
        )?;
        assert!(c.get(&a).is_err());
        std::fs::set_permissions(&c.directory, std::fs::Permissions::from_mode(0o755))?;
        assert!(c.get(&a).is_err());
        Ok(())
    }
}
