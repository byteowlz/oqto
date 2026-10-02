//! Behavioral tests of the trusted provider boundary using synthetic credentials only.
//! Storage assurance is exercised on Linux; this suite makes no other-OS claim.

use oqto_secrets::{MAX_SECRET_BYTES, ProviderError, SecretValue};

const VALUE: &str = "synthetic-secret-never-log-42";

#[test]
fn secret_values_are_bounded_and_debug_is_redacted() {
    assert_eq!(
        SecretValue::new(String::new()).unwrap_err(),
        ProviderError::InvalidInput
    );
    assert!(SecretValue::new("x".repeat(MAX_SECRET_BYTES)).is_ok());
    assert_eq!(
        SecretValue::new("x".repeat(MAX_SECRET_BYTES + 1)).unwrap_err(),
        ProviderError::InvalidInput
    );
    // Bounds are bytes, not Unicode scalar count.
    assert_eq!(
        SecretValue::new("é".repeat(MAX_SECRET_BYTES / 2 + 1)).unwrap_err(),
        ProviderError::InvalidInput
    );
    let value = SecretValue::new(VALUE.to_owned()).unwrap();
    assert_eq!(format!("{value:?}"), "SecretValue([REDACTED])");
    assert_eq!(value.expose_to_mediator(), VALUE);
}

#[cfg(target_os = "linux")]
mod linux {
    use super::*;
    use oqto_secrets::{LocalProvider, ProviderStatus, SecretProvider, SecretString};
    use std::fs;
    use std::os::unix::fs::{PermissionsExt, symlink};
    use std::path::Path;
    use tempfile::TempDir;

    const OWNER: &str = "synthetic-account";
    const PASS: &str = "synthetic-passphrase-12345";

    fn pass() -> SecretString {
        SecretString::from(PASS.to_owned())
    }
    fn value() -> SecretValue {
        SecretValue::new(VALUE.to_owned()).unwrap()
    }
    fn fixture() -> (TempDir, LocalProvider) {
        let dir = tempfile::tempdir().unwrap();
        let provider = LocalProvider::open(&dir.path().join("private"), OWNER).unwrap();
        (dir, provider)
    }
    fn inaccessible(provider: &mut LocalProvider, account: &str, error: ProviderError) {
        assert_eq!(provider.list(account).unwrap_err(), error);
        assert_eq!(
            provider.resolve_for_mediator(account, "token").unwrap_err(),
            error
        );
        assert_eq!(
            provider.put(account, "token", value(), None).unwrap_err(),
            error
        );
        assert_eq!(provider.delete(account, "token", 1).unwrap_err(), error);
    }
    fn assert_open_error(root: &Path, error: ProviderError) {
        assert!(matches!(LocalProvider::open(root, OWNER), Err(actual) if actual == error));
    }

    #[test]
    fn lifecycle_restarts_locked_and_owner_checks_cover_all_operations() {
        let (dir, mut provider) = fixture();
        assert_eq!(provider.status(), ProviderStatus::Uninitialized);
        inaccessible(&mut provider, OWNER, ProviderError::Uninitialized);
        inaccessible(&mut provider, "other", ProviderError::WrongOwner);
        assert_eq!(
            provider.initialize("other", pass()),
            Err(ProviderError::WrongOwner)
        );
        assert_eq!(
            provider.unlock(OWNER, pass()),
            Err(ProviderError::Uninitialized)
        );
        provider.initialize(OWNER, pass()).unwrap();
        provider.put(OWNER, "token", value(), None).unwrap();
        inaccessible(&mut provider, "other", ProviderError::WrongOwner);
        assert_eq!(
            provider.initialize("other", pass()),
            Err(ProviderError::WrongOwner)
        );
        assert_eq!(
            provider.unlock("other", pass()),
            Err(ProviderError::WrongOwner)
        );
        assert_eq!(provider.status(), ProviderStatus::Locked);
        inaccessible(&mut provider, OWNER, ProviderError::Locked);
        provider.unlock(OWNER, pass()).unwrap();
        provider.lock();
        inaccessible(&mut provider, OWNER, ProviderError::Locked);
        drop(provider);
        let mut reopened = LocalProvider::open(&dir.path().join("private"), OWNER).unwrap();
        assert_eq!(reopened.status(), ProviderStatus::Locked);
        inaccessible(&mut reopened, OWNER, ProviderError::Locked);
        reopened.unlock(OWNER, pass()).unwrap();
        assert_eq!(
            reopened
                .resolve_for_mediator(OWNER, "token")
                .unwrap()
                .expose_to_mediator(),
            VALUE
        );
        drop(reopened);
        let mut wrong_owner = LocalProvider::open(&dir.path().join("private"), "other").unwrap();
        assert_eq!(
            wrong_owner.unlock("other", pass()),
            Err(ProviderError::WrongOwner)
        );
        inaccessible(&mut wrong_owner, "other", ProviderError::Locked);
    }

    #[test]
    fn authentication_and_ciphertext_failures_clear_ready_state_without_leaking() {
        let (dir, mut provider) = fixture();
        provider.initialize(OWNER, pass()).unwrap();
        provider.put(OWNER, "token", value(), None).unwrap();
        let path = dir.path().join("private/vault.age");
        let original = fs::read(&path).unwrap();
        let error = provider
            .unlock(
                OWNER,
                SecretString::from("wrong-synthetic-passphrase".to_owned()),
            )
            .unwrap_err();
        assert_eq!(error, ProviderError::Integrity);
        for rendered in [format!("{error}"), format!("{error:?}")] {
            for private in [
                VALUE,
                PASS,
                "wrong-synthetic-passphrase",
                path.to_str().unwrap(),
            ] {
                assert!(!rendered.contains(private));
            }
        }
        inaccessible(&mut provider, OWNER, ProviderError::Locked);
        provider.unlock(OWNER, pass()).unwrap();
        assert_eq!(
            provider.unlock(OWNER, SecretString::from("short".to_owned())),
            Err(ProviderError::InvalidInput)
        );
        inaccessible(&mut provider, OWNER, ProviderError::Locked);
        let mut tampered = original.clone();
        let last = tampered.len() - 1;
        tampered[last] ^= 1;
        for corrupt in [original[..original.len() / 2].to_vec(), tampered] {
            fs::write(&path, &original).unwrap();
            provider.unlock(OWNER, pass()).unwrap();
            fs::write(&path, corrupt).unwrap();
            assert_eq!(
                provider.unlock(OWNER, pass()),
                Err(ProviderError::Integrity)
            );
            assert_eq!(provider.status(), ProviderStatus::Locked);
            inaccessible(&mut provider, OWNER, ProviderError::Locked);
        }
        fs::write(&path, original).unwrap();
        provider.unlock(OWNER, pass()).unwrap();
        assert_eq!(
            provider
                .resolve_for_mediator(OWNER, "token")
                .unwrap()
                .expose_to_mediator(),
            VALUE
        );
    }

    #[test]
    fn cas_conflicts_and_delete_recreate_aba_survive_restart() {
        let (dir, mut provider) = fixture();
        provider.initialize(OWNER, pass()).unwrap();
        assert_eq!(
            provider.put(OWNER, "token", value(), Some(1)).unwrap_err(),
            ProviderError::Conflict
        );
        let first = provider.put(OWNER, "token", value(), None).unwrap();
        assert_eq!(
            provider.put(OWNER, "token", value(), None).unwrap_err(),
            ProviderError::Conflict
        );
        assert_eq!(
            provider
                .put(OWNER, "token", value(), Some(first.revision + 1))
                .unwrap_err(),
            ProviderError::Conflict
        );
        assert_eq!(
            provider.delete(OWNER, "token", first.revision + 1),
            Err(ProviderError::Conflict)
        );
        assert_eq!(provider.list(OWNER).unwrap(), vec![first.clone()]);
        let updated = provider
            .put(OWNER, "token", value(), Some(first.revision))
            .unwrap();
        assert!(updated.revision > first.revision);
        provider.delete(OWNER, "token", updated.revision).unwrap();
        assert_eq!(
            provider.resolve_for_mediator(OWNER, "token").unwrap_err(),
            ProviderError::NotFound
        );
        assert_eq!(
            provider.delete(OWNER, "token", updated.revision),
            Err(ProviderError::NotFound)
        );
        drop(provider);
        let mut provider = LocalProvider::open(&dir.path().join("private"), OWNER).unwrap();
        provider.unlock(OWNER, pass()).unwrap();
        let recreated = provider.put(OWNER, "token", value(), None).unwrap();
        assert!(recreated.revision > updated.revision);
        for stale in [first.revision, updated.revision] {
            assert_eq!(
                provider
                    .put(OWNER, "token", value(), Some(stale))
                    .unwrap_err(),
                ProviderError::Conflict
            );
            assert_eq!(
                provider.delete(OWNER, "token", stale),
                Err(ProviderError::Conflict)
            );
        }
        assert_eq!(provider.list(OWNER).unwrap(), vec![recreated]);
    }

    #[test]
    fn duplicate_initialization_preserves_ciphertext_and_original_passphrase() {
        let (dir, mut provider) = fixture();
        provider.initialize(OWNER, pass()).unwrap();
        let metadata = provider.put(OWNER, "token", value(), None).unwrap();
        let path = dir.path().join("private/vault.age");
        let before = fs::read(&path).unwrap();
        let alternate = || SecretString::from("alternate-synthetic-passphrase".to_owned());
        assert_eq!(
            provider.initialize(OWNER, alternate()),
            Err(ProviderError::AlreadyInitialized)
        );
        assert_eq!(fs::read(&path).unwrap(), before);
        drop(provider);
        let mut provider = LocalProvider::open(&dir.path().join("private"), OWNER).unwrap();
        assert_eq!(
            provider.initialize(OWNER, alternate()),
            Err(ProviderError::AlreadyInitialized)
        );
        assert_eq!(fs::read(&path).unwrap(), before);
        assert_eq!(
            provider.unlock(OWNER, alternate()),
            Err(ProviderError::Integrity)
        );
        provider.unlock(OWNER, pass()).unwrap();
        assert_eq!(provider.list(OWNER).unwrap(), vec![metadata]);
    }

    #[test]
    fn writer_lock_is_held_while_locked_and_released_on_drop() {
        let (dir, mut provider) = fixture();
        let root = dir.path().join("private");
        assert_open_error(&root, ProviderError::Busy);
        provider.initialize(OWNER, pass()).unwrap();
        provider.lock();
        assert_open_error(&root, ProviderError::Busy);
        drop(provider);
        assert!(LocalProvider::open(&root, OWNER).is_ok());
    }

    #[test]
    fn unsafe_storage_is_rejected_without_repair_or_target_mutation() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("private");
        fs::create_dir(&root).unwrap();
        fs::set_permissions(&root, fs::Permissions::from_mode(0o755)).unwrap();
        assert_open_error(&root, ProviderError::UnsafeStorage);
        assert_eq!(
            fs::metadata(&root).unwrap().permissions().mode() & 0o777,
            0o755
        );
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
        let alias = dir.path().join("alias");
        symlink(&root, &alias).unwrap();
        assert_open_error(&alias, ProviderError::UnsafeStorage);
        assert_open_error(&alias.join("nested"), ProviderError::UnsafeStorage);
        let target = dir.path().join("synthetic-target");
        fs::write(&target, b"synthetic untouched bytes").unwrap();
        fs::set_permissions(&target, fs::Permissions::from_mode(0o600)).unwrap();
        let entry = root.join("vault.age");
        if entry.exists() {
            fs::remove_file(&entry).unwrap();
        }
        symlink(&target, &entry).unwrap();
        assert_open_error(&root, ProviderError::UnsafeStorage);
        fs::remove_file(&entry).unwrap();
        fs::hard_link(&target, &entry).unwrap();
        assert_open_error(&root, ProviderError::UnsafeStorage);
        fs::remove_file(&entry).unwrap();
        fs::write(&entry, b"synthetic insecure storage").unwrap();
        fs::set_permissions(&entry, fs::Permissions::from_mode(0o644)).unwrap();
        assert_open_error(&root, ProviderError::UnsafeStorage);
        assert_eq!(
            fs::metadata(&entry).unwrap().permissions().mode() & 0o777,
            0o644
        );
        fs::remove_file(&entry).unwrap();
        fs::create_dir(&entry).unwrap();
        assert_open_error(&root, ProviderError::UnsafeStorage);
        fs::remove_dir(&entry).unwrap();
        assert_eq!(fs::read(target).unwrap(), b"synthetic untouched bytes");
    }

    #[test]
    fn unsafe_storage_after_open_fails_closed_on_unlock_and_commit() {
        let (dir, mut provider) = fixture();
        provider.initialize(OWNER, pass()).unwrap();
        let path = dir.path().join("private/vault.age");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
        assert_eq!(
            provider.unlock(OWNER, pass()),
            Err(ProviderError::UnsafeStorage)
        );
        inaccessible(&mut provider, OWNER, ProviderError::Locked);
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        provider.unlock(OWNER, pass()).unwrap();
        let alias = dir.path().join("hardlink");
        fs::hard_link(&path, &alias).unwrap();
        let before = fs::read(&path).unwrap();
        assert_eq!(
            provider.put(OWNER, "token", value(), None).unwrap_err(),
            ProviderError::UnsafeStorage
        );
        inaccessible(&mut provider, OWNER, ProviderError::Locked);
        assert_eq!(fs::read(&path).unwrap(), before);
    }

    #[test]
    fn bounded_public_inputs_and_oversized_ciphertext_fail_closed() {
        let (dir, mut provider) = fixture();
        for invalid in [String::new(), "x".repeat(15), "x".repeat(4097)] {
            assert_eq!(
                provider.initialize(OWNER, SecretString::from(invalid)),
                Err(ProviderError::InvalidInput)
            );
            assert_eq!(provider.status(), ProviderStatus::Uninitialized);
        }
        assert!(!dir.path().join("private/vault.age").exists());
        assert!(matches!(
            LocalProvider::open(&dir.path().join("unused"), "../invalid"),
            Err(ProviderError::InvalidInput)
        ));
        provider.initialize(OWNER, pass()).unwrap();
        for id in [
            "",
            ".",
            "..",
            "../token",
            "with space",
            "é",
            &"x".repeat(129),
        ] {
            assert_eq!(
                provider.put(OWNER, id, value(), None).unwrap_err(),
                ProviderError::InvalidInput
            );
        }
        assert!(provider.list(OWNER).unwrap().is_empty());
        let path = dir.path().join("private/vault.age");
        let file = fs::OpenOptions::new().write(true).open(&path).unwrap();
        file.set_len(2 * 1024 * 1024 + 1).unwrap();
        assert_eq!(
            provider.unlock(OWNER, pass()),
            Err(ProviderError::Integrity)
        );
        inaccessible(&mut provider, OWNER, ProviderError::Locked);
    }
}
