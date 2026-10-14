//! Trusted control-plane management only; not an agent capability or resolver.
use oqto_protocol::credentials::*;
use oqto_secrets::{
    LocalProvider, ProviderError, ProviderStatus, SecretProvider, SecretString, SecretValue,
};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

type AccountProvider = Arc<Mutex<Option<LocalProvider>>>;
/// Local is the only supported provider. No environment-value or kyz fallback.
pub struct CredentialManager {
    data_home: Option<PathBuf>,
    accounts: Mutex<HashMap<String, AccountProvider>>,
    work: Arc<tokio::sync::Semaphore>,
}
impl Default for CredentialManager {
    fn default() -> Self {
        // Standard XDG location when unset; this chooses a storage directory,
        // never an ambient secret/provider fallback. Invalid explicit roots
        // remain unavailable rather than silently selecting another location.
        Self::new(host_data_home(
            std::env::var_os("XDG_DATA_HOME"),
            std::env::var_os("HOME"),
        ))
    }
}
fn host_data_home(
    xdg: Option<std::ffi::OsString>,
    home: Option<std::ffi::OsString>,
) -> Option<PathBuf> {
    xdg.map(PathBuf::from)
        .or_else(|| home.map(|home| PathBuf::from(home).join(".local/share")))
}

impl CredentialManager {
    /// The runner host supplies this root, never a workload/request path.
    pub fn new(data_home: Option<PathBuf>) -> Self {
        Self {
            data_home,
            accounts: Mutex::new(HashMap::new()),
            work: Arc::new(tokio::sync::Semaphore::new(4)),
        }
    }
    pub async fn manage(
        self: &Arc<Self>,
        request: CredentialManageRequest,
    ) -> CredentialManageResult {
        let permit = match Arc::clone(&self.work).try_acquire_owned() {
            Ok(permit) => permit,
            Err(_) => return failure(CredentialProviderStatus::Unavailable, "busy"),
        };
        let manager = Arc::clone(self);
        match tokio::task::spawn_blocking(move || {
            let _permit = permit;
            manager.manage_blocking(request)
        })
        .await
        {
            Ok(result) => result,
            Err(_) => failure(CredentialProviderStatus::Unavailable, "unavailable"),
        }
    }
    fn manage_blocking(&self, request: CredentialManageRequest) -> CredentialManageResult {
        let account = &request.account_id;
        if !valid_account(account) {
            return failure(CredentialProviderStatus::Unavailable, "invalid_input");
        }
        let slot = match self.accounts.lock() {
            Ok(mut accounts) => {
                if accounts.len() >= 256 && !accounts.contains_key(account) {
                    return failure(CredentialProviderStatus::Unavailable, "busy");
                }
                Arc::clone(
                    accounts
                        .entry(account.clone())
                        .or_insert_with(|| Arc::new(Mutex::new(None))),
                )
            }
            Err(_) => return failure(CredentialProviderStatus::Unavailable, "unavailable"),
        };
        let mut guard = match slot.lock() {
            Ok(guard) => guard,
            Err(_) => return failure(CredentialProviderStatus::Unavailable, "unavailable"),
        };
        if guard.is_none() {
            let opened = self.open(account);
            match opened {
                Ok(provider) => *guard = Some(provider),
                Err(error) => return failure(CredentialProviderStatus::Unavailable, code(&error)),
            }
        }
        let Some(provider) = guard.as_mut() else {
            return failure(CredentialProviderStatus::Unavailable, "unavailable");
        };
        let operation = match request.command {
            CredentialCommand::Status => Ok(()),
            CredentialCommand::List => provider.list(account).map(|_| ()),
            CredentialCommand::Initialize { passphrase } => provider.initialize(
                account,
                SecretString::from(passphrase.expose_for_protected_transport().to_owned()),
            ),
            CredentialCommand::Unlock { passphrase } => provider.unlock(
                account,
                SecretString::from(passphrase.expose_for_protected_transport().to_owned()),
            ),
            CredentialCommand::Lock => {
                provider.lock();
                Ok(())
            }
            CredentialCommand::Put {
                id,
                value,
                expected_revision,
            } => SecretValue::new(value.expose_for_protected_transport().to_owned()).and_then(
                |value| {
                    provider
                        .put(account, &id, value, expected_revision)
                        .map(|_| ())
                },
            ),
            CredentialCommand::Delete {
                id,
                expected_revision,
            } => provider.delete(account, &id, expected_revision),
        };
        let status = status(provider.status());
        if let Err(error) = operation {
            return failure(status, code(&error));
        }
        let entries = if status == CredentialProviderStatus::Ready {
            match provider.list(account) {
                Ok(entries) => entries
                    .into_iter()
                    .map(|entry| CredentialEntryMetadata {
                        id: entry.id,
                        revision: entry.revision,
                    })
                    .collect(),
                Err(error) => return failure(status, code(&error)),
            }
        } else {
            Vec::new()
        };
        CredentialManageResult {
            status,
            entries,
            code: None,
        }
    }
    fn open(&self, account: &str) -> Result<LocalProvider, ProviderError> {
        let data_home = self
            .data_home
            .as_ref()
            .filter(|path| path.is_absolute())
            .ok_or(ProviderError::Storage)?;
        for ancestor in data_home.ancestors() {
            let metadata =
                std::fs::symlink_metadata(ancestor).map_err(|_| ProviderError::Storage)?;
            if !metadata.is_dir() || metadata.file_type().is_symlink() {
                return Err(ProviderError::UnsafeStorage);
            }
        }
        let mut root = data_home.join("oqto");
        owned_parent_directory(&root)?;
        for component in ["credentials", "secrets"] {
            root.push(component);
            private_directory(&root)?;
        }
        LocalProvider::open(&root.join(account), account)
    }
}
#[cfg(unix)]
fn owned_parent_directory(path: &Path) -> Result<(), ProviderError> {
    use std::os::unix::fs::{DirBuilderExt, MetadataExt};
    match std::fs::symlink_metadata(path) {
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            match std::fs::DirBuilder::new().mode(0o700).create(path) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(_) => return Err(ProviderError::Storage),
            }
        }
        Err(_) => return Err(ProviderError::Storage),
    }
    let metadata = std::fs::symlink_metadata(path).map_err(|_| ProviderError::Storage)?;
    // Existing general Oqto data may be 0755. Private credential descendants
    // are 0700; never chmod unrelated data or accept an externally writable parent.
    if !metadata.is_dir()
        || metadata.file_type().is_symlink()
        || metadata.uid() != rustix::process::geteuid().as_raw()
        || metadata.mode() & 0o022 != 0
    {
        return Err(ProviderError::UnsafeStorage);
    }
    Ok(())
}
#[cfg(not(unix))]
fn owned_parent_directory(_path: &Path) -> Result<(), ProviderError> {
    Err(ProviderError::UnsupportedPlatform)
}

fn valid_account(account: &str) -> bool {
    !account.is_empty()
        && account.len() <= 128
        && account != "."
        && account != ".."
        && account
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
}
#[cfg(unix)]
fn private_directory(path: &Path) -> Result<(), ProviderError> {
    use std::os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt};
    match std::fs::symlink_metadata(path) {
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            match std::fs::DirBuilder::new().mode(0o700).create(path) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(_) => return Err(ProviderError::Storage),
            }
        }
        Err(_) => return Err(ProviderError::Storage),
    }
    let metadata = std::fs::symlink_metadata(path).map_err(|_| ProviderError::Storage)?;
    if !metadata.is_dir()
        || metadata.file_type().is_symlink()
        || metadata.permissions().mode() & 0o777 != 0o700
        || metadata.uid() != rustix::process::geteuid().as_raw()
    {
        return Err(ProviderError::UnsafeStorage);
    }
    Ok(())
}
#[cfg(not(unix))]
fn private_directory(_path: &Path) -> Result<(), ProviderError> {
    Err(ProviderError::UnsupportedPlatform)
}
fn failure(status: CredentialProviderStatus, code: &str) -> CredentialManageResult {
    CredentialManageResult {
        status,
        entries: Vec::new(),
        code: Some(code.to_owned()),
    }
}
fn status(value: ProviderStatus) -> CredentialProviderStatus {
    match value {
        ProviderStatus::Uninitialized => CredentialProviderStatus::Uninitialized,
        ProviderStatus::Locked => CredentialProviderStatus::Locked,
        ProviderStatus::Ready => CredentialProviderStatus::Ready,
        ProviderStatus::Unavailable => CredentialProviderStatus::Unavailable,
    }
}
fn code(error: &ProviderError) -> &'static str {
    match error {
        ProviderError::Locked => "locked",
        ProviderError::Uninitialized => "uninitialized",
        ProviderError::AlreadyInitialized => "already_initialized",
        ProviderError::WrongOwner => "wrong_owner",
        ProviderError::NotFound => "not_found",
        ProviderError::Conflict => "conflict",
        ProviderError::InvalidInput => "invalid_input",
        ProviderError::Integrity => "integrity",
        ProviderError::UnsafeStorage => "unsafe_storage",
        ProviderError::Busy => "busy",
        ProviderError::Storage => "unavailable",
        ProviderError::UnsupportedPlatform => "unsupported_platform",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    async fn run(
        manager: &Arc<CredentialManager>,
        account: &str,
        command: CredentialCommand,
    ) -> CredentialManageResult {
        manager
            .manage(CredentialManageRequest {
                account_id: account.into(),
                command,
            })
            .await
    }
    fn input(value: &str) -> CredentialSecretInput {
        CredentialSecretInput::new(value.into())
    }
    #[tokio::test]
    async fn synthetic_lifecycle_owner_restart_and_sanitization() -> anyhow::Result<()> {
        let root = tempfile::tempdir()?;
        let manager = Arc::new(CredentialManager::new(Some(root.path().to_owned())));
        assert_eq!(
            run(&manager, "alice", CredentialCommand::Status)
                .await
                .status,
            CredentialProviderStatus::Uninitialized
        );
        assert_eq!(
            run(&manager, "alice", CredentialCommand::List)
                .await
                .code
                .as_deref(),
            Some("uninitialized")
        );
        assert_eq!(
            run(
                &manager,
                "alice",
                CredentialCommand::Initialize {
                    passphrase: input("synthetic-strong-passphrase")
                }
            )
            .await
            .status,
            CredentialProviderStatus::Ready
        );
        let result = run(
            &manager,
            "alice",
            CredentialCommand::Put {
                id: "service".into(),
                value: input("synthetic-value"),
                expected_revision: None,
            },
        )
        .await;
        assert!(result.code.is_none());
        let revision = result.entries[0].revision;
        let serialized = serde_json::to_string(&result)?;
        assert!(!serialized.contains("synthetic-value"));
        assert!(!serialized.contains("passphrase"));
        assert!(!serialized.contains(root.path().to_string_lossy().as_ref()));
        assert_eq!(
            run(&manager, "bob", CredentialCommand::List)
                .await
                .code
                .as_deref(),
            Some("uninitialized")
        );
        assert_eq!(
            run(&manager, "../alice", CredentialCommand::Status)
                .await
                .code
                .as_deref(),
            Some("invalid_input")
        );
        assert_eq!(
            run(
                &manager,
                "alice",
                CredentialCommand::Delete {
                    id: "service".into(),
                    expected_revision: revision + 1
                }
            )
            .await
            .code
            .as_deref(),
            Some("conflict")
        );
        assert_eq!(
            run(&manager, "alice", CredentialCommand::Lock).await.status,
            CredentialProviderStatus::Locked
        );
        assert_eq!(
            run(&manager, "alice", CredentialCommand::List)
                .await
                .code
                .as_deref(),
            Some("locked")
        );
        drop(manager);
        let manager = Arc::new(CredentialManager::new(Some(root.path().to_owned())));
        assert_eq!(
            run(&manager, "alice", CredentialCommand::Status)
                .await
                .status,
            CredentialProviderStatus::Locked
        );
        assert_eq!(
            run(
                &manager,
                "alice",
                CredentialCommand::Unlock {
                    passphrase: input("synthetic-wrong-passphrase")
                }
            )
            .await
            .code
            .as_deref(),
            Some("integrity")
        );
        let result = run(
            &manager,
            "alice",
            CredentialCommand::Unlock {
                passphrase: input("synthetic-strong-passphrase"),
            },
        )
        .await;
        assert_eq!(result.status, CredentialProviderStatus::Ready);
        assert_eq!(result.entries[0].revision, revision);
        Ok(())
    }
    #[tokio::test]
    async fn missing_root_symlinks_permissions_and_writer_fail_closed() -> anyhow::Result<()> {
        use std::os::unix::{fs::PermissionsExt, fs::symlink};
        let absent = Arc::new(CredentialManager::new(None));
        assert_eq!(
            run(&absent, "alice", CredentialCommand::Status)
                .await
                .code
                .as_deref(),
            Some("unavailable")
        );
        let root = tempfile::tempdir()?;
        let manager = Arc::new(CredentialManager::new(Some(root.path().to_owned())));
        assert!(
            run(&manager, "alice", CredentialCommand::Status)
                .await
                .code
                .is_none()
        );
        let other = Arc::new(CredentialManager::new(Some(root.path().to_owned())));
        assert_eq!(
            run(&other, "alice", CredentialCommand::Status)
                .await
                .code
                .as_deref(),
            Some("busy")
        );
        symlink(
            root.path().join("oqto/credentials/secrets/alice"),
            root.path().join("oqto/credentials/secrets/bob"),
        )?;
        assert_eq!(
            run(&manager, "bob", CredentialCommand::Status)
                .await
                .code
                .as_deref(),
            Some("unsafe_storage")
        );
        let unsafe_root = tempfile::tempdir()?;
        std::fs::create_dir(unsafe_root.path().join("oqto"))?;
        std::fs::set_permissions(
            unsafe_root.path().join("oqto"),
            std::fs::Permissions::from_mode(0o777),
        )?;
        let unsafe_manager = Arc::new(CredentialManager::new(Some(unsafe_root.path().to_owned())));
        assert_eq!(
            run(&unsafe_manager, "alice", CredentialCommand::Status)
                .await
                .code
                .as_deref(),
            Some("unsafe_storage")
        );
        assert_eq!(
            std::fs::metadata(unsafe_root.path().join("oqto"))?
                .permissions()
                .mode()
                & 0o777,
            0o777
        );
        Ok(())
    }

    #[tokio::test]
    async fn normal_data_parent_is_preserved_and_credentials_remain_private() -> anyhow::Result<()>
    {
        use std::os::unix::fs::PermissionsExt;
        let root = tempfile::tempdir()?;
        std::fs::create_dir(root.path().join("oqto"))?;
        std::fs::set_permissions(
            root.path().join("oqto"),
            std::fs::Permissions::from_mode(0o755),
        )?;
        let manager = Arc::new(CredentialManager::new(Some(root.path().to_owned())));
        let result = run(&manager, "alice", CredentialCommand::Status).await;
        assert_eq!(result.status, CredentialProviderStatus::Uninitialized);
        assert_eq!(result.code, None);
        assert_eq!(
            std::fs::metadata(root.path().join("oqto"))?
                .permissions()
                .mode()
                & 0o777,
            0o755
        );
        for suffix in [
            "oqto/credentials",
            "oqto/credentials/secrets",
            "oqto/credentials/secrets/alice",
        ] {
            assert_eq!(
                std::fs::metadata(root.path().join(suffix))?
                    .permissions()
                    .mode()
                    & 0o777,
                0o700
            );
        }
        assert_eq!(
            host_data_home(None, Some("/synthetic/home".into())),
            Some(PathBuf::from("/synthetic/home/.local/share"))
        );
        assert_eq!(
            host_data_home(
                Some("relative-explicit".into()),
                Some("/synthetic/home".into())
            ),
            Some(PathBuf::from("relative-explicit"))
        );
        Ok(())
    }
}
