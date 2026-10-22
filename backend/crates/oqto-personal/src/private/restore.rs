//! Explicit, singleton-locked undo of an approved legacy marker migration.
use super::*;
use sha2::{Digest, Sha256};
use std::io::Read;

pub(crate) fn restore_owner(path: &Path, expected: &str) -> Result<()> {
    ensure!(
        expected.len() == 64 && expected.bytes().all(|b| b.is_ascii_hexdigit()),
        "expected backup SHA256 required"
    );
    let (root, database, _lock) = open_state(path)?;
    let marker = root.join("store-owner.json");
    let old = read_private(&root.join(format!("store-owner-{expected}.backup")))?;
    ensure!(
        format!("{:x}", Sha256::digest(old.as_bytes())) == expected,
        "backup digest mismatch"
    );
    let legacy: identity::LegacyIdentity =
        serde_json::from_str(&old).context("backup is not a legacy marker")?;
    ensure!(
        legacy.purpose == "oqto.personal-store.v1" && legacy.ino == database.metadata()?.ino(),
        "backup belongs to another database inode"
    );
    let current = read_private(&marker)?;
    let _: identity::StoreIdentity =
        serde_json::from_str(&current).context("rollback requires a current v2 marker")?;
    let saved = root.join(format!(
        "store-owner-before-restore-{}.backup",
        uuid::Uuid::new_v4()
    ));
    fs::hard_link(&marker, &saved)?;
    fs::File::open(&root)?.sync_all()?;
    let staged = root.join(format!("store-owner-restore-{}.new", uuid::Uuid::new_v4()));
    fs::hard_link(root.join(format!("store-owner-{expected}.backup")), &staged)?;
    let result = (|| -> Result<()> {
        ensure!(
            read_private(&marker)? == current && read_private(&staged)? == old,
            "ownership marker changed during rollback"
        );
        fs::rename(&staged, &marker)?;
        fs::File::open(&root)?.sync_all()?;
        Ok(())
    })();
    if staged.try_exists()? {
        fs::remove_file(staged)?;
    }
    result
}
fn read_private(path: &Path) -> Result<String> {
    let mut file = fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)?;
    let meta = file.metadata()?;
    ensure!(
        meta.is_file()
            && meta.uid() == rustix::process::geteuid().as_raw()
            && meta.mode() & 0o077 == 0
            && meta.len() <= 256,
        "invalid private ownership marker backup"
    );
    let mut text = String::new();
    file.by_ref().take(257).read_to_string(&mut text)?;
    ensure!(text.len() <= 256, "ownership marker backup too large");
    Ok(text)
}
