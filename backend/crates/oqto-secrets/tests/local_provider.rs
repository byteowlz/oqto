#![cfg(target_os = "linux")]

use oqto_secrets::{LocalProvider, ProviderStatus, SecretProvider, SecretString, SecretValue};

#[test]
fn credential_survives_lock_and_reopen_without_plaintext_on_disk()
-> Result<(), Box<dyn std::error::Error>> {
    let root = tempfile::tempdir()?;
    let path = root.path().join("credentials");
    let passphrase = || SecretString::from("synthetic vault unlock phrase".to_owned());
    let mut provider = LocalProvider::open(&path, "account-a")?;
    assert_eq!(provider.status(), ProviderStatus::Uninitialized);
    provider.initialize("account-a", passphrase())?;
    let metadata = provider.put(
        "account-a",
        "podigee",
        SecretValue::new("synthetic-api-token-only".to_owned())?,
        None,
    )?;
    assert_eq!(metadata.revision, 1);
    let ciphertext = std::fs::read(path.join("vault.age"))?;
    assert!(
        !ciphertext
            .windows(b"synthetic-api-token-only".len())
            .any(|w| w == b"synthetic-api-token-only")
    );
    assert!(
        !ciphertext
            .windows(b"account-a".len())
            .any(|w| w == b"account-a")
    );
    provider.lock();
    assert_eq!(provider.status(), ProviderStatus::Locked);
    drop(provider);
    let mut reopened = LocalProvider::open(&path, "account-a")?;
    reopened.unlock("account-a", passphrase())?;
    assert_eq!(reopened.list("account-a")?, vec![metadata]);
    assert_eq!(
        reopened
            .resolve_for_mediator("account-a", "podigee")?
            .expose_to_mediator(),
        "synthetic-api-token-only"
    );
    Ok(())
}
