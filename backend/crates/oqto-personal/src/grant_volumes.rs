//! Persistent volume identity for file grants.
//!
//! A file grant pins its root's (mount-instance number, inode). On macOS the
//! mount-instance number (`st_dev`) changes whenever mount order changes, so a
//! reboot silently revoked every grant. The persistent identity is the volume
//! UUID, recorded here per (device, grant).
//!
//! Rollback safety: this table is created with `IF NOT EXISTS` outside the sqlx
//! migration list and the grant JSON keeps its v1 shape, so an older Runner can
//! still open the store and read every grant; it merely ignores this table.
use crate::*;
use serde::Deserialize;

/// Why a volume identity was recorded. `same_mount` needs no approval: the
/// stored mount-instance number still matched, so it is provably the same mount.
/// `owner_repair` is the owner's explicit acceptance after a remount.
const SAME_MOUNT: &str = "same_mount";

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    /// Write same-mount facts only (startup, enrollment).
    Record,
    /// Report what Record and Repair would do; write nothing.
    DryRun,
    /// Record same-mount facts and owner-approved remounts.
    Repair,
}
const OWNER_REPAIR: &str = "owner_repair";

/// Counts only; no paths, device ids or volume identities are reported.
#[derive(Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct GrantVolumeReport {
    pub examined: usize,
    /// Already recorded before this run.
    pub current: usize,
    /// Recorded now because the mount was provably unchanged.
    pub recorded: usize,
    /// Remounted roots an owner may approve (dry run).
    pub upgradable: usize,
    /// Remounted roots recorded now with owner approval.
    pub upgraded: usize,
    /// Moved, replaced or unreadable roots: left alone for re-pairing.
    pub mismatched: usize,
}

impl PairingAuthority {
    pub(crate) async fn create_grant_volumes(pool: &SqlitePool) -> Result<()> {
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS personal_grant_volumes (
                device_id TEXT NOT NULL,
                grant_id TEXT NOT NULL,
                filesystem TEXT NOT NULL,
                inode INTEGER NOT NULL,
                approval TEXT NOT NULL CHECK (approval IN ('same_mount', 'owner_repair')),
                recorded_at INTEGER NOT NULL,
                PRIMARY KEY (device_id, grant_id)
            )",
        )
        .execute(pool)
        .await?;
        Ok(())
    }

    /// Attach recorded volume identities to a device's grants (in memory only).
    /// A row only applies to the grant with the same id AND inode it was recorded for.
    pub(crate) async fn attach_grant_volumes(
        &self,
        device_id: &str,
        grants: &mut Grants,
    ) -> Result<()> {
        if grants.files.is_empty() {
            return Ok(());
        }
        let rows = sqlx::query(
            "SELECT grant_id, filesystem, inode FROM personal_grant_volumes WHERE device_id=?",
        )
        .bind(device_id)
        .fetch_all(&self.pool)
        .await?;
        for row in rows {
            let grant_id: String = row.try_get("grant_id")?;
            let inode: i64 = row.try_get("inode")?;
            if let Some(grant) = grants
                .files
                .iter_mut()
                .find(|grant| grant.id() == grant_id && grant.inode() as i64 == inode)
            {
                grant.with_volume(row.try_get("filesystem")?);
            }
        }
        Ok(())
    }

    /// Record the volume identity of every current grant whose mount is provably
    /// unchanged. Runs at store open and after enrollment; needs no approval.
    pub async fn record_grant_volumes(&self) -> Result<GrantVolumeReport> {
        self.grant_volumes(Mode::Record).await
    }

    /// Owner command. `apply` false is a pure dry run that writes nothing;
    /// `apply` true also accepts roots whose only change is a remount (same
    /// canonical path, directory and inode) as the owner's explicit approval.
    pub async fn repair_grant_volumes(&self, apply: bool) -> Result<GrantVolumeReport> {
        self.grant_volumes(if apply { Mode::Repair } else { Mode::DryRun })
            .await
    }

    async fn grant_volumes(&self, mode: Mode) -> Result<GrantVolumeReport> {
        let mut report = GrantVolumeReport::default();
        for (device_id, grants) in self.current_device_grants().await? {
            for grant in &grants.files {
                report.examined += 1;
                if self.has_volume(&device_id, grant).await? {
                    report.current += 1;
                    continue;
                }
                if let Ok(Some(volume)) = grant.same_mount_volume() {
                    if mode != Mode::DryRun {
                        self.insert_volume(&device_id, grant, &volume, SAME_MOUNT)
                            .await?;
                    }
                    report.recorded += 1;
                    continue;
                }
                match (mode, grant.remounted_volume()) {
                    (Mode::Repair, Ok(Some(volume))) => {
                        self.insert_volume(&device_id, grant, &volume, OWNER_REPAIR)
                            .await?;
                        report.upgraded += 1;
                    }
                    (Mode::DryRun, Ok(Some(_))) => report.upgradable += 1,
                    _ => report.mismatched += 1,
                }
            }
        }
        Ok(report)
    }

    async fn current_device_grants(&self) -> Result<Vec<(String, Grants)>> {
        let rows =
            sqlx::query("SELECT id, grants_json FROM personal_devices WHERE revoked_at IS NULL")
                .fetch_all(&self.pool)
                .await?;
        let mut out = Vec::with_capacity(rows.len());
        for row in rows {
            let grants: Grants = serde_json::from_str(&row.try_get::<String, _>("grants_json")?)?;
            out.push((row.try_get("id")?, grants));
        }
        Ok(out)
    }

    async fn has_volume(&self, device_id: &str, grant: &FileGrant) -> Result<bool> {
        Ok(sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM personal_grant_volumes WHERE device_id=? AND grant_id=? AND inode=?",
        )
        .bind(device_id)
        .bind(grant.id())
        .bind(grant.inode() as i64)
        .fetch_one(&self.pool)
        .await?
            > 0)
    }

    async fn insert_volume(
        &self,
        device_id: &str,
        grant: &FileGrant,
        volume: &str,
        approval: &str,
    ) -> Result<()> {
        // OR IGNORE: a concurrent recorder already wrote the same fact; never overwrite one.
        sqlx::query(
            "INSERT OR IGNORE INTO personal_grant_volumes (device_id, grant_id, filesystem, inode, approval, recorded_at) VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(device_id)
        .bind(grant.id())
        .bind(volume)
        .bind(grant.inode() as i64)
        .bind(approval)
        .bind(now()?)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::{enrollment, fixture};

    async fn enrolled(authority: &PairingAuthority, root: &Path) -> Result<String> {
        let grants = Grants {
            files: vec![FileGrant::select(root, true)?],
            ..Grants::default()
        };
        let ticket = authority.issue_ticket(&grants).await?;
        authority.enroll(&enrollment(&ticket, 7)).await
    }
    async fn rows(authority: &PairingAuthority) -> Result<Vec<(String, String)>> {
        Ok(
            sqlx::query_as("SELECT grant_id, approval FROM personal_grant_volumes")
                .fetch_all(&authority.pool)
                .await?,
        )
    }
    /// Simulate the reboot: the stored v1 row keeps the old mount-instance number.
    async fn renumber_mount(authority: &PairingAuthority, device: &str) -> Result<()> {
        let json: String =
            sqlx::query_scalar("SELECT grants_json FROM personal_devices WHERE id=?")
                .bind(device)
                .fetch_one(&authority.pool)
                .await?;
        let mut value: serde_json::Value = serde_json::from_str(&json)?;
        let old = value["files"][0]["device"].as_u64().expect("device");
        value["files"][0]["device"] = (old + 2).into();
        sqlx::query("UPDATE personal_devices SET grants_json=? WHERE id=?")
            .bind(value.to_string())
            .bind(device)
            .execute(&authority.pool)
            .await?;
        Ok(())
    }
    async fn grants(authority: &PairingAuthority, device: &str) -> Result<Grants> {
        let json: String =
            sqlx::query_scalar("SELECT grants_json FROM personal_devices WHERE id=?")
                .bind(device)
                .fetch_one(&authority.pool)
                .await?;
        let mut grants: Grants = serde_json::from_str(&json)?;
        authority.attach_grant_volumes(device, &mut grants).await?;
        Ok(grants)
    }

    #[tokio::test]
    async fn enrollment_records_the_volume_so_a_later_reboot_keeps_the_grant() -> Result<()> {
        let (dir, authority) = fixture().await?;
        let root = dir.path().join("work");
        std::fs::create_dir(&root)?;
        let device = enrolled(&authority, &root).await?;
        assert_eq!(rows(&authority).await?.len(), 1);
        assert_eq!(
            rows(&authority).await?[0].1,
            SAME_MOUNT,
            "no approval needed on the original mount"
        );

        renumber_mount(&authority, &device).await?;
        grants(&authority, &device).await?.files[0].open_root()?;
        Ok(())
    }

    #[tokio::test]
    async fn already_drifted_grants_need_the_owner_and_dry_run_writes_nothing() -> Result<()> {
        let (dir, authority) = fixture().await?;
        let root = dir.path().join("work");
        std::fs::create_dir(&root)?;
        let device = enrolled(&authority, &root).await?;
        // A grant created by the old Runner: no volume row, then a reboot.
        sqlx::query("DELETE FROM personal_grant_volumes")
            .execute(&authority.pool)
            .await?;
        renumber_mount(&authority, &device).await?;
        assert!(
            grants(&authority, &device).await?.files[0]
                .open_root()
                .is_err()
        );

        let startup = authority.record_grant_volumes().await?;
        assert_eq!(
            (startup.recorded, startup.mismatched),
            (0, 1),
            "startup never trusts a remount"
        );
        assert!(rows(&authority).await?.is_empty());

        let dry = authority.repair_grant_volumes(false).await?;
        assert_eq!((dry.upgradable, dry.upgraded, dry.mismatched), (1, 0, 0));
        assert!(rows(&authority).await?.is_empty(), "dry run writes nothing");

        let applied = authority.repair_grant_volumes(true).await?;
        assert_eq!(applied.upgraded, 1);
        assert_eq!(rows(&authority).await?[0].1, OWNER_REPAIR);
        grants(&authority, &device).await?.files[0].open_root()?;

        let again = authority.repair_grant_volumes(true).await?;
        assert_eq!((again.current, again.upgraded), (1, 0), "idempotent");
        Ok(())
    }

    #[tokio::test]
    async fn a_replaced_folder_is_never_repaired() -> Result<()> {
        let (dir, authority) = fixture().await?;
        let root = dir.path().join("work");
        std::fs::create_dir(&root)?;
        let device = enrolled(&authority, &root).await?;
        sqlx::query("DELETE FROM personal_grant_volumes")
            .execute(&authority.pool)
            .await?;
        renumber_mount(&authority, &device).await?;
        std::fs::remove_dir(&root)?;
        std::fs::create_dir(&root)?;
        let applied = authority.repair_grant_volumes(true).await?;
        assert_eq!((applied.upgraded, applied.mismatched), (0, 1));
        assert!(rows(&authority).await?.is_empty());
        Ok(())
    }
}
