#![cfg(target_os = "linux")]

use oqto_secrets::{
    LocalProvider, ProviderError, ProviderStatus, SecretProvider, SecretString, SecretValue,
};
use std::fs;
use std::os::unix::fs::PermissionsExt;

// fork briefly inherits sibling tests' open file descriptions before exec.
// Serialize provider fixtures with the subprocess test to avoid interpreting
// that transient inheritance as a leaked writer lock after a sibling's drop.
static FORK_FIXTURES: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[test]
fn deleting_a_lock_named_file_cannot_create_a_second_writer()
-> Result<(), Box<dyn std::error::Error>> {
    let _guard = FORK_FIXTURES.lock().unwrap();
    let temp = tempfile::tempdir()?;
    let root = temp.path().join("private");
    let first = LocalProvider::open(&root, "account")?;
    // Maintenance may delete a file with this conventional name. Exclusion
    // must depend on the stable directory inode, not on a removable artifact.
    fs::write(root.join("writer.lock"), b"synthetic maintenance marker")?;
    fs::remove_file(root.join("writer.lock"))?;
    assert!(matches!(
        LocalProvider::open(&root, "account"),
        Err(ProviderError::Busy)
    ));
    drop(first);
    assert!(LocalProvider::open(&root, "account").is_ok());
    Ok(())
}

#[test]
fn replaced_directory_is_not_written_through_a_stale_provider()
-> Result<(), Box<dyn std::error::Error>> {
    let _guard = FORK_FIXTURES.lock().unwrap();
    let temp = tempfile::tempdir()?;
    let root = temp.path().join("private");
    let mut first = LocalProvider::open(&root, "account")?;
    first.initialize(
        "account",
        SecretString::from("synthetic old vault passphrase".to_owned()),
    )?;
    let old_ciphertext = fs::read(root.join("vault.age"))?;
    fs::rename(&root, temp.path().join("previous"))?;
    fs::create_dir(&root)?;
    fs::set_permissions(&root, fs::Permissions::from_mode(0o700))?;
    let mut replacement = LocalProvider::open(&root, "account")?;
    let password = || SecretString::from("synthetic new vault passphrase".to_owned());
    replacement.initialize("account", password())?;
    let current_ciphertext = fs::read(root.join("vault.age"))?;
    assert_eq!(first.status(), ProviderStatus::Unavailable);
    assert_eq!(first.list("account"), Err(ProviderError::UnsafeStorage));
    assert!(matches!(
        first.resolve_for_mediator("account", "token"),
        Err(ProviderError::UnsafeStorage)
    ));
    assert!(matches!(
        first.put(
            "account",
            "token",
            SecretValue::new("synthetic stale write".to_owned())?,
            None
        ),
        Err(ProviderError::UnsafeStorage)
    ));
    assert_eq!(fs::read(root.join("vault.age"))?, current_ciphertext);
    assert_eq!(
        fs::read(temp.path().join("previous/vault.age"))?,
        old_ciphertext
    );
    replacement.unlock("account", password())?;
    Ok(())
}

#[test]
fn aggregate_limit_preserves_committed_values_and_private_permissions()
-> Result<(), Box<dyn std::error::Error>> {
    let _guard = FORK_FIXTURES.lock().unwrap();
    let temp = tempfile::tempdir()?;
    let root = temp.path().join("private");
    let mut provider = LocalProvider::open(&root, "account")?;
    provider.initialize(
        "account",
        SecretString::from("synthetic aggregate passphrase".to_owned()),
    )?;
    // JSON escaping makes these valid individual values exhaust the whole-vault
    // limit quickly, without hundreds of expensive KDF operations.
    for index in 0..5 {
        provider.put(
            "account",
            &format!("token-{index}"),
            SecretValue::new("\0".repeat(oqto_secrets::MAX_SECRET_BYTES))?,
            None,
        )?;
    }
    let path = root.join("vault.age");
    let committed = fs::read(&path)?;
    assert_eq!(fs::metadata(&root)?.permissions().mode() & 0o777, 0o700);
    assert_eq!(fs::metadata(&path)?.permissions().mode() & 0o777, 0o600);
    assert!(committed.len() <= 2 * 1024 * 1024);
    assert_eq!(
        provider.put(
            "account",
            "overflow",
            SecretValue::new("\0".repeat(oqto_secrets::MAX_SECRET_BYTES))?,
            None
        ),
        Err(ProviderError::InvalidInput)
    );
    assert_eq!(provider.status(), ProviderStatus::Ready);
    assert_eq!(provider.list("account")?.len(), 5);
    assert_eq!(fs::read(path)?, committed);
    Ok(())
}

#[test]
fn cross_process_exclusion() -> Result<(), Box<dyn std::error::Error>> {
    const CHILD_ROOT: &str = "OQTO_SECRETS_SYNTHETIC_WRITER_CHILD_ROOT";
    if let Some(root) = std::env::var_os(CHILD_ROOT) {
        assert!(matches!(
            LocalProvider::open(std::path::Path::new(&root), "account"),
            Err(ProviderError::Busy)
        ));
        return Ok(());
    }
    let _guard = FORK_FIXTURES.lock().unwrap();
    let temp = tempfile::tempdir()?;
    let root = temp.path().join("private");
    let _provider = LocalProvider::open(&root, "account")?;
    let output = std::process::Command::new(std::env::current_exe()?)
        .env_clear()
        .args(["--exact", "cross_process_exclusion", "--nocapture"])
        .env(CHILD_ROOT, &root)
        .output()?;
    assert!(
        output.status.success(),
        "synthetic child failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("1 passed"));
    Ok(())
}
