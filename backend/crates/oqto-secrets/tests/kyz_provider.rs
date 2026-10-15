use kyz_core::{VaultSession, VaultStore};
use oqto_secrets::{
    KyzProvider, ProviderError, ProviderStatus, SecretProvider, SecretString, SecretValue,
};

const PASS: &str = "synthetic-oqto-kyz-fixture-passphrase-only-123456";
#[test]
fn owner_lifecycle_rotation_restart_and_no_session_material() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let path = root.path().join("vault.json");
    let store = VaultStore::new(path.clone());
    store.init(PASS, false)?;
    let mut provider = KyzProvider::open(path.clone(), "alice")?;
    assert_eq!(provider.status(), ProviderStatus::Locked);
    assert_eq!(provider.list("alice"), Err(ProviderError::Locked));
    assert_eq!(provider.list("bob"), Err(ProviderError::WrongOwner));
    provider.unlock("alice", SecretString::from(PASS.to_owned()))?;
    let first = provider.put(
        "alice",
        "api",
        SecretValue::new("synthetic-first".into())?,
        None,
    )?;
    assert_eq!(
        provider
            .resolve_for_mediator("alice", "api")?
            .expose_to_mediator(),
        "synthetic-first"
    );
    let (snapshot, snapshot_revision) = provider.snapshot_for_mediator("alice", "api")?;
    assert_eq!(snapshot.expose_to_mediator(), "synthetic-first");
    assert_eq!(snapshot_revision, first.revision);
    assert!(provider.snapshot_for_mediator("bob", "api").is_err());
    let second = provider.put(
        "alice",
        "api",
        SecretValue::new("synthetic-second".into())?,
        Some(first.revision),
    )?;
    assert_ne!(first.revision, second.revision);
    assert_eq!(
        provider.delete("alice", "api", first.revision),
        Err(ProviderError::Conflict)
    );
    assert_eq!(
        provider.resolve_for_mediator("bob", "api").unwrap_err(),
        ProviderError::WrongOwner
    );
    provider.lock();
    assert_eq!(
        provider.resolve_for_mediator("alice", "api").unwrap_err(),
        ProviderError::Locked
    );
    drop(provider);
    let mut restored = KyzProvider::open(path, "alice")?;
    assert_eq!(restored.status(), ProviderStatus::Locked);
    restored.unlock("alice", SecretString::from(PASS.to_owned()))?;
    assert_eq!(restored.list("alice")?, vec![second.clone()]);
    restored.delete("alice", "api", second.revision)?;
    assert!(!VaultSession::session_file_for(store.vault_path())?.exists());
    assert!(!store.status()?.unlocked);
    Ok(())
}
