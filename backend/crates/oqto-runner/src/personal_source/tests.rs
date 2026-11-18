use super::*;
use std::os::unix::fs::symlink;

use super::test_support::{fixture, write};
#[test]
fn unchanged_inventory_reads_no_headers() -> Result<()> {
    let (_dir, source, work) = fixture()?;
    for id in ["first", "second"] {
        write(&source, &work, id)?;
    }
    let first = source.headers()?;
    assert_eq!(
        source
            .header_reads
            .load(std::sync::atomic::Ordering::SeqCst),
        2
    );
    source
        .header_reads
        .store(0, std::sync::atomic::Ordering::SeqCst);
    assert_eq!(source.headers()?, first);
    assert_eq!(
        source
            .header_reads
            .load(std::sync::atomic::Ordering::SeqCst),
        0
    );
    Ok(())
}

#[test]
fn cache_invalidates_only_changed_entries() -> Result<()> {
    use std::sync::atomic::Ordering::SeqCst;
    let (_dir, source, work) = fixture()?;
    let first = write(&source, &work, "first")?;
    let second = write(&source, &work, "second")?;
    source.headers()?;
    let reads = || source.header_reads.swap(0, SeqCst);
    reads();
    // Size change, then mtime-only change with nanosecond precision.
    use std::io::Write;
    std::fs::OpenOptions::new()
        .append(true)
        .open(&first)?
        .write_all(b"body\n")?;
    assert_eq!(source.headers()?.len(), 2);
    assert_eq!(reads(), 1);
    let file = std::fs::File::open(&second)?;
    file.set_modified(file.metadata()?.modified()? + std::time::Duration::from_nanos(1000))?;
    source.headers()?;
    assert_eq!(reads(), 1);
    // Same size and mtime but another inode.
    let old = std::fs::metadata(&first)?;
    let replacement = first.with_extension("replacement");
    std::fs::copy(&first, &replacement)?;
    std::fs::File::open(&replacement)?.set_modified(old.modified()?)?;
    std::fs::rename(&replacement, &first)?;
    source.headers()?;
    assert_eq!(reads(), 1);
    write(&source, &work, "third")?;
    assert_eq!(source.headers()?.len(), 3);
    assert_eq!(reads(), 1);
    std::fs::remove_file(&second)?;
    assert_eq!(source.headers()?.len(), 2);
    assert_eq!(reads(), 0);
    assert!(
        !source
            .inventory
            .lock()
            .unwrap()
            .files_contains_for_test(&second)
    );
    Ok(())
}

#[test]
fn warmed_cache_still_refuses_replaced_directories_files_and_cwd_aliases() -> Result<()> {
    let (_dir, source, work) = fixture()?;
    let path = write(&source, &work, "native")?;
    source.headers()?;
    let parent = path.parent().context("parent")?;
    let saved = parent.with_extension("saved");
    std::fs::rename(parent, &saved)?;
    std::fs::create_dir(parent)?;
    std::fs::rename(saved.join(path.file_name().context("name")?), &path)?;
    source
        .header_reads
        .store(0, std::sync::atomic::Ordering::SeqCst);
    assert_eq!(source.headers()?.len(), 1);
    assert_eq!(
        source
            .header_reads
            .load(std::sync::atomic::Ordering::SeqCst),
        1,
        "directory replacement must invalidate children"
    );
    let saved_file = source.home.join("saved-native");
    std::fs::rename(&path, &saved_file)?;
    symlink(&saved_file, &path)?;
    assert!(source.headers()?.is_empty());
    std::fs::remove_file(&path)?;
    std::fs::rename(saved_file, &path)?;
    source.headers()?;
    let saved_work = work.with_extension("saved");
    std::fs::rename(&work, &saved_work)?;
    symlink(&saved_work, &work)?;
    assert!(source.headers()?.is_empty());
    Ok(())
}

#[test]
fn one_directory_chunks_retain_every_header_until_complete() -> Result<()> {
    let (_dir, source, work) = fixture()?;
    for n in 0..4 {
        let cwd = work.join(format!("repo-{n}"));
        std::fs::create_dir(&cwd)?;
        write(&source, &cwd, &format!("native-{n}"))?;
    }
    let mut scan = inventory::HeaderScan::default();
    for n in 1..=4 {
        let ready = scan.advance(&source, 1, std::time::Duration::from_secs(5))?;
        assert_eq!(ready, n == 4);
        assert_eq!(scan.headers.len(), n);
    }
    let mut ids: Vec<_> = scan
        .headers
        .iter()
        .map(|header| header.external_id.as_str())
        .collect();
    ids.sort_unstable();
    assert_eq!(ids, ["native-0", "native-1", "native-2", "native-3"]);
    Ok(())
}

#[tokio::test]
async fn fresh_header_only_split_home_identity_is_immutable_and_preserves_owner() -> Result<()> {
    let (_dir, source, work) = fixture()?;
    let path = write(&source, &work, "fresh-native")?;
    let header = source.header(&path)?;
    let user_home = source
        .session_root()
        .ancestors()
        .nth(3)
        .context("native home")?;
    assert!(!user_home.join(".local/share/oqto").exists());
    let ws = work.to_str().context("workspace")?;
    assert!(
        oqto_history::oqto_log::ops::native_source::discover(
            &source.home,
            "original-owner",
            ws,
            &header.external_id,
            &header.identity
        )
        .await?
    );
    assert!(
        !oqto_history::oqto_log::ops::native_source::discover(
            &source.home,
            "other-owner",
            ws,
            &header.external_id,
            &header.identity
        )
        .await?
    );
    let row = oqto_history::oqto_log::ops::list_sessions(&source.home, None)
        .await?
        .remove(0);
    assert_ne!(row.platform_id, header.external_id);
    assert_eq!(row.user_id, "original-owner");
    assert_eq!(row.messages, 0);
    let old_bytes = std::fs::read(&path)?;
    // A different configured root, even with equal native id/bytes/stat cursor,
    // cannot silently inherit a previously bound canonical Session.
    let alternate_root = source.home.join("alternate-sessions");
    std::fs::create_dir(&alternate_root)?;
    let alternate = PersonalSource::new(
        source.home.clone(),
        alternate_root,
        source.socket_root().to_owned(),
        source.workspace_roots().to_vec(),
    )?;
    let alternate_file = write(&alternate, &work, &header.external_id)?;
    std::fs::File::open(&alternate_file)?.set_modified(std::fs::metadata(&path)?.modified()?)?;
    assert_eq!(
        std::fs::metadata(&alternate_file)?.len(),
        std::fs::metadata(&path)?.len()
    );
    let alternate_header = alternate.header(&alternate_file)?;
    assert!(
        oqto_history::oqto_log::ops::native_source::discover(
            &source.home,
            "original-owner",
            ws,
            &header.external_id,
            &alternate_header.identity
        )
        .await
        .is_err()
    );
    let replacement = path.with_extension("replacement");
    std::fs::write(&replacement, &old_bytes)?;
    std::fs::rename(replacement, &path)?;
    let changed = source.header(&path)?;
    assert!(
        oqto_history::oqto_log::ops::native_source::discover(
            &source.home,
            "original-owner",
            ws,
            &changed.external_id,
            &changed.identity
        )
        .await
        .is_err()
    );
    assert!(
        oqto_history::oqto_log::ops::native_source::validate(
            &source.home,
            &row.platform_id,
            &header.external_id,
            ws,
            &[changed.identity.as_str()]
        )
        .await
        .is_err()
    );
    assert_eq!(std::fs::read(&path)?, old_bytes);
    assert!(!user_home.join(".local/share/oqto").exists());
    Ok(())
}
#[tokio::test]
async fn conflicting_public_native_bindings_are_never_repaired() -> Result<()> {
    let (_dir, source, work) = fixture()?;
    let header = source.header(&write(&source, &work, "conflicting-native")?)?;
    let public = oqto_history::oqto_log::store::platform_id_for_external_id(&header.external_id);
    for workspace in [&work, &source.home] {
        oqto_history::oqto_log::ops::upsert_session_identity(
            &source.home,
            "retained-owner",
            workspace.to_str().context("Workspace")?,
            &public,
            Some(&public),
            Some(&header.external_id),
        )
        .await?;
    }
    assert!(
        oqto_history::oqto_log::ops::native_source::discover(
            &source.home,
            "new-owner",
            work.to_str().context("Workspace")?,
            &header.external_id,
            &header.identity
        )
        .await
        .is_err()
    );
    assert!(
        oqto_history::oqto_log::ops::native_source::validate(
            &source.home,
            &public,
            &header.external_id,
            work.to_str().context("Workspace")?,
            &[header.identity.as_str()]
        )
        .await
        .is_err()
    );
    Ok(())
}
#[test]
fn strict_roots_headers_symlinks_and_discovery_bounds() -> Result<()> {
    let (_dir, source, work) = fixture()?;
    let path = write(&source, &work, "native")?;
    assert_eq!(source.headers()?.len(), 1);
    let outside = source.home.join("outside");
    std::fs::create_dir_all(&outside)?;
    let denied = write(&source, &outside, "outside")?;
    // Invalid body must not even be parsed by cold discovery.
    let mut file = std::fs::OpenOptions::new().append(true).open(&denied)?;
    use std::io::Write;
    file.write_all(b"this is not JSON and must never be read as a body")?;
    assert!(source.header(&denied).is_err());
    assert_eq!(source.headers()?.len(), 1);
    let original = std::fs::read(&path)?;
    for header in [
        serde_json::json!({"type":"session","id":"wrong-id","cwd":work}),
        serde_json::json!({"type":"session","id":"native","cwd":source.workspace_roots()[0]}),
    ] {
        std::fs::write(&path, format!("{header}\n"))?;
        assert!(source.header(&path).is_err());
    }
    std::fs::write(&path, &original)?;
    let alias = source.home.join("alias");
    symlink(source.session_root(), &alias)?;
    for root in [alias, PathBuf::from("relative")] {
        assert!(
            PersonalSource::new(
                source.home.clone(),
                root,
                source.socket_root().to_owned(),
                source.workspace_roots().to_vec()
            )
            .is_err()
        );
    }
    let socket_alias = source.home.join("socket-alias");
    symlink(source.socket_root(), &socket_alias)?;
    assert!(
        PersonalSource::new(
            source.home.clone(),
            source.session_root().to_owned(),
            socket_alias,
            source.workspace_roots().to_vec()
        )
        .is_err()
    );
    let duplicate = path.with_file_name("other_native.jsonl");
    std::fs::copy(&path, &duplicate)?;
    assert!(source.header(&path).is_err());
    std::fs::remove_file(&duplicate)?;
    let bytes = std::fs::read(&path)?;
    std::fs::remove_file(&path)?;
    symlink(&denied, &path)?;
    assert!(source.header(&path).is_err());
    std::fs::remove_file(&path)?;
    std::fs::write(&path, bytes)?;
    let old = source.session_root().with_extension("old");
    std::fs::rename(source.session_root(), &old)?;
    std::fs::create_dir_all(source.session_root())?;
    assert!(source.validate().is_err());
    Ok(())
}
#[tokio::test]
async fn remounted_pins_are_accepted_today_and_repaired_only_for_the_same_source() -> Result<()> {
    use oqto_history::oqto_log::ops::native_source::{discover, repin, validate};
    let (_dir, source, work) = fixture()?;
    let ws = work.to_str().context("workspace")?;
    let mut sessions = Vec::new();
    for native in ["remounted", "replaced"] {
        let header = source.header(&write(&source, &work, native)?)?;
        assert!(
            discover(
                &source.home,
                "owner",
                ws,
                &header.external_id,
                &header.identity
            )
            .await?
        );
        let public =
            oqto_history::oqto_log::store::platform_id_for_external_id(&header.external_id);
        // A version 1 pin with today's device number keeps working.
        assert!(
            repin(
                &source.home,
                ws,
                &public,
                &header.external_id,
                &header.identity,
                &header.legacy_identity
            )
            .await?
        );
        validate(
            &source.home,
            &public,
            &header.external_id,
            ws,
            &header.accepted_pins(),
        )
        .await?;
        // A remount assigns a new device number; "replaced" also has another inode.
        let mut stale: serde_json::Value = serde_json::from_str(&header.legacy_identity)?;
        let bump = |value: &serde_json::Value| {
            serde_json::Value::from(value.as_u64().unwrap_or_default() + 1)
        };
        stale["dev"] = bump(&stale["dev"]);
        stale["root"]["dev"] = bump(&stale["root"]["dev"]);
        if native == "replaced" {
            stale["ino"] = bump(&stale["ino"]);
        }
        let stale = serde_json::to_string(&stale)?;
        assert!(
            repin(
                &source.home,
                ws,
                &public,
                &header.external_id,
                &header.legacy_identity,
                &stale
            )
            .await?
        );
        assert!(
            validate(
                &source.home,
                &public,
                &header.external_id,
                ws,
                &header.accepted_pins()
            )
            .await
            .is_err()
        );
        sessions.push((public, header));
    }
    let dry = source.repair_pins(false).await?;
    assert_eq!(
        (dry.examined, dry.upgradable, dry.mismatched, dry.upgraded),
        (2, 1, 1, 0)
    );
    // The dry run wrote nothing.
    let (public, header) = &sessions[0];
    assert!(
        validate(
            &source.home,
            public,
            &header.external_id,
            ws,
            &header.accepted_pins()
        )
        .await
        .is_err()
    );
    let applied = source.repair_pins(true).await?;
    assert_eq!(
        (applied.upgraded, applied.mismatched, applied.unrecognized),
        (1, 1, 0)
    );
    validate(
        &source.home,
        public,
        &header.external_id,
        ws,
        &[header.identity.as_str()],
    )
    .await?;
    let (public, header) = &sessions[1];
    assert!(
        validate(
            &source.home,
            public,
            &header.external_id,
            ws,
            &header.accepted_pins()
        )
        .await
        .is_err()
    );
    let again = source.repair_pins(true).await?;
    assert_eq!((again.current, again.upgraded, again.mismatched), (1, 0, 1));
    Ok(())
}

/// oqto-a9v9: a version 1 pin that still matches today's mount is upgraded to
/// version 2 without approval, so a later remount no longer strands it. A pin
/// that already stopped matching is left for the owner-approved repair.
#[tokio::test]
async fn current_v1_pins_upgrade_silently_and_drifted_ones_wait_for_the_owner() -> Result<()> {
    use oqto_history::oqto_log::ops::native_source::{discover, repin, validate};
    let (_dir, source, work) = fixture()?;
    let ws = work.to_str().context("workspace")?;
    let mut sessions = Vec::new();
    for native in ["still-current", "already-drifted"] {
        let header = source.header(&write(&source, &work, native)?)?;
        assert!(
            discover(
                &source.home,
                "owner",
                ws,
                &header.external_id,
                &header.identity
            )
            .await?
        );
        let public =
            oqto_history::oqto_log::store::platform_id_for_external_id(&header.external_id);
        // Stored as version 1, as the old Runner wrote it.
        assert!(
            repin(
                &source.home,
                ws,
                &public,
                &header.external_id,
                &header.identity,
                &header.legacy_identity
            )
            .await?
        );
        sessions.push((public, header));
    }
    // The second one was written before a remount: its device number is stale.
    let (public, header) = &sessions[1];
    let mut stale: serde_json::Value = serde_json::from_str(&header.legacy_identity)?;
    stale["dev"] = serde_json::Value::from(stale["dev"].as_u64().unwrap_or_default() + 1);
    stale["root"]["dev"] =
        serde_json::Value::from(stale["root"]["dev"].as_u64().unwrap_or_default() + 1);
    let stale = serde_json::to_string(&stale)?;
    assert!(
        repin(
            &source.home,
            ws,
            public,
            &header.external_id,
            &header.legacy_identity,
            &stale
        )
        .await?
    );

    assert_eq!(
        source.upgrade_current_pins().await?,
        1,
        "only the provably current pin"
    );
    let (public, header) = &sessions[0];
    // Version 2 only: a remount (device change) can no longer break it.
    validate(
        &source.home,
        public,
        &header.external_id,
        ws,
        &[header.identity.as_str()],
    )
    .await?;
    let (public, header) = &sessions[1];
    assert!(
        validate(
            &source.home,
            public,
            &header.external_id,
            ws,
            &header.accepted_pins()
        )
        .await
        .is_err(),
        "a drifted pin is never trusted silently"
    );
    assert_eq!(source.upgrade_current_pins().await?, 0, "idempotent");
    let owner = source.repair_pins(false).await?;
    assert_eq!(
        owner.upgradable, 1,
        "the drifted one stays visible to the owner repair"
    );
    Ok(())
}

/// A Session whose working folder was deleted (a removed checkout or worktree)
/// stays readable history: still root-contained, still plain, never redirected.
#[test]
fn sessions_of_deleted_folders_stay_admissible_but_never_redirected() -> Result<()> {
    let (_dir, source, work) = fixture()?;
    let gone = work.join("elox/extracted/elox-vendor");
    std::fs::create_dir_all(&gone)?;
    let path = write(&source, &gone, "vanished")?;
    std::fs::remove_dir_all(work.join("elox"))?;
    assert!(!gone.exists());
    let header = source.header(&path)?;
    assert_eq!(header.workspace, gone, "the recorded folder, not a guess");

    // Plain-path rules for a missing folder.
    assert!(canonical_or_vanished(&gone)?);
    assert!(
        !canonical_or_vanished(&work.join("elox/../elox/x"))?,
        "no dot-dot"
    );
    assert!(!canonical_or_vanished(std::path::Path::new("relative/x"))?);
    // An existing ancestor reached through a symlink redirects; refuse it.
    let elsewhere = work.join("elsewhere");
    std::fs::create_dir_all(&elsewhere)?;
    std::os::unix::fs::symlink(&elsewhere, work.join("link"))?;
    assert!(!canonical_or_vanished(&work.join("link/missing/x"))?);
    // Outside the roots it is still denied, deleted or not.
    let outside = source.home.join("outside");
    std::fs::create_dir_all(&outside)?;
    let path = write(&source, &outside, "outside")?;
    std::fs::remove_dir_all(&outside)?;
    assert!(source.header(&path).is_err());
    Ok(())
}

/// The owner may trust rewritten Session files in one Workspace; the native
/// session id read from the file must still match, and other Workspaces are
/// untouched.
#[tokio::test]
async fn owner_trusted_rewrites_need_the_same_session_id_and_workspace() -> Result<()> {
    use oqto_history::oqto_log::ops::native_source::{discover, repin, validate};
    let (_dir, source, work) = fixture()?;
    let ws = work.to_str().context("workspace")?;
    let header = source.header(&write(&source, &work, "rewritten")?)?;
    assert!(
        discover(
            &source.home,
            "owner",
            ws,
            &header.external_id,
            &header.identity
        )
        .await?
    );
    let public = oqto_history::oqto_log::store::platform_id_for_external_id(&header.external_id);
    // A version 1 pin from before the rewrite: another inode, another mount number.
    let mut old: serde_json::Value = serde_json::from_str(&header.legacy_identity)?;
    old["ino"] = serde_json::Value::from(old["ino"].as_u64().unwrap_or_default() + 7);
    old["dev"] = serde_json::Value::from(old["dev"].as_u64().unwrap_or_default() + 1);
    old["root"]["dev"] =
        serde_json::Value::from(old["root"]["dev"].as_u64().unwrap_or_default() + 1);
    let old = serde_json::to_string(&old)?;
    assert!(
        repin(
            &source.home,
            ws,
            &public,
            &header.external_id,
            &header.identity,
            &old
        )
        .await?
    );

    let plain = source.repair_pins(true).await?;
    assert_eq!(
        (plain.upgraded, plain.mismatched),
        (0, 1),
        "without owner trust: refused"
    );
    let other = source
        .repair_pins_with(true, Some(&work.join("other")))
        .await?;
    assert_eq!(other.upgraded, 0, "trust is per Workspace");
    let dry = source.repair_pins_with(false, Some(&work)).await?;
    assert_eq!((dry.upgradable, dry.rewritten, dry.upgraded), (1, 1, 0));
    let applied = source.repair_pins_with(true, Some(&work)).await?;
    assert_eq!((applied.upgraded, applied.rewritten), (1, 1));
    validate(
        &source.home,
        &public,
        &header.external_id,
        ws,
        &[header.identity.as_str()],
    )
    .await?;
    Ok(())
}

/// The deployed repair command only understood version 1 pins. Version 2 pins
/// (persistent filesystem IDs) require the same explicit per-Workspace approval
/// for an inode-only rewrite, without relaxing any other identity fact.
#[tokio::test]
async fn owner_trusted_version2_rewrites_preserve_every_other_identity_fact() -> Result<()> {
    use oqto_history::oqto_log::ops::native_source::{discover, repin, validate};
    for changed in [
        "inode",
        "filesystem",
        "root-filesystem",
        "root-inode",
        "path",
        "workspace",
        "native-id",
        "version",
    ] {
        let (_dir, source, work) = fixture()?;
        let ws = work.to_str().context("workspace")?;
        let header = source.header(&write(&source, &work, "rewritten-v2")?)?;
        discover(
            &source.home,
            "owner",
            ws,
            &header.external_id,
            &header.identity,
        )
        .await?;
        let public =
            oqto_history::oqto_log::store::platform_id_for_external_id(&header.external_id);
        let mut old: serde_json::Value = serde_json::from_str(&header.identity)?;
        old["ino"] = serde_json::Value::from(header.ino + 7);
        match changed {
            "filesystem" => old["filesystem"] = "different-volume".into(),
            "root-filesystem" => old["root"]["filesystem"] = "different-volume".into(),
            "root-inode" => old["root"]["ino"] = 0.into(),
            "path" => old["file"] = "/another/session.jsonl".into(),
            "workspace" => old["workspace"] = "/another/workspace".into(),
            "native-id" => old["native_id"] = "another-session".into(),
            "version" => old["pin"] = 3.into(),
            _ => {}
        }
        let old = serde_json::to_string(&old)?;
        assert!(
            repin(
                &source.home,
                ws,
                &public,
                &header.external_id,
                &header.identity,
                &old
            )
            .await?
        );
        assert_eq!(
            source.repair_pins(true).await?.upgraded,
            0,
            "no implicit rewrite trust: {changed}"
        );
        assert_eq!(
            source
                .repair_pins_with(true, Some(&work.join("other")))
                .await?
                .upgraded,
            0,
            "trust cannot escape its Workspace: {changed}"
        );
        let dry = source.repair_pins_with(false, Some(&work)).await?;
        let applied = source.repair_pins_with(true, Some(&work)).await?;
        let expected = usize::from(changed == "inode");
        assert_eq!(dry.upgradable, expected, "dry {changed}: {dry:?}");
        assert_eq!(
            (applied.upgraded, applied.rewritten),
            (expected, expected),
            "{changed}: {applied:?}"
        );
        if expected == 1 {
            validate(
                &source.home,
                &public,
                &header.external_id,
                ws,
                &[header.identity.as_str()],
            )
            .await?;
        }
    }
    Ok(())
}

/// A rewrite that changed the session id is another Session; trust never covers it.
#[tokio::test]
async fn owner_trust_never_adopts_a_different_session() -> Result<()> {
    use oqto_history::oqto_log::ops::native_source::{discover, repin};
    let (_dir, source, work) = fixture()?;
    let ws = work.to_str().context("workspace")?;
    let header = source.header(&write(&source, &work, "original")?)?;
    assert!(
        discover(
            &source.home,
            "owner",
            ws,
            &header.external_id,
            &header.identity
        )
        .await?
    );
    let public = oqto_history::oqto_log::store::platform_id_for_external_id(&header.external_id);
    let mut old: serde_json::Value = serde_json::from_str(&header.legacy_identity)?;
    old["ino"] = serde_json::Value::from(old["ino"].as_u64().unwrap_or_default() + 7);
    old["native_id"] = "someone-else".into();
    let old = serde_json::to_string(&old)?;
    assert!(
        repin(
            &source.home,
            ws,
            &public,
            &header.external_id,
            &header.identity,
            &old
        )
        .await?
    );
    let applied = source.repair_pins_with(true, Some(&work)).await?;
    assert_eq!((applied.upgraded, applied.mismatched), (0, 1));
    Ok(())
}

/// oqto-6qff: header inventory over a Mac-sized native tree (4219 files in 424
/// Workspace directories) stays inside the discovery bounds and reports its
/// cold/warm cost; chunking must converge even when a cold tree is slower than
/// one wall-clock budget. Header opens, not timing alone, prove cache reuse.
#[test]
fn header_inventory_at_personal_host_scale() -> Result<()> {
    const DIRECTORIES: usize = 424;
    const FILES: usize = 4219;
    let (_dir, source, work) = fixture()?;
    let workspaces: Vec<PathBuf> = (0..DIRECTORIES)
        .map(|n| {
            let path = work.join(format!("repo-{n}"));
            std::fs::create_dir_all(&path)?;
            Ok(path)
        })
        .collect::<Result<_>>()?;
    let body = format!("{}\n", "x".repeat(4096));
    for n in 0..FILES {
        let path = write(
            &source,
            &workspaces[n % DIRECTORIES],
            &format!("native-{n}"),
        )?;
        let mut file = std::fs::OpenOptions::new().append(true).open(path)?;
        use std::io::Write;
        file.write_all(body.as_bytes())?;
    }
    let started = std::time::Instant::now();
    let mut scan = inventory::HeaderScan::default();
    for chunk in 1..=DIRECTORIES {
        assert_eq!(
            scan.advance(&source, 1, std::time::Duration::from_secs(5))?,
            chunk == DIRECTORIES
        );
    }
    let elapsed = started.elapsed();
    let headers = scan.headers;
    assert_eq!(headers.len(), FILES);
    assert_eq!(
        source
            .header_reads
            .swap(0, std::sync::atomic::Ordering::SeqCst),
        FILES
    );
    let started = std::time::Instant::now();
    let warm_headers = source.headers()?;
    let warm = started.elapsed();
    assert_eq!(warm_headers, headers);
    assert_eq!(
        source
            .header_reads
            .load(std::sync::atomic::Ordering::SeqCst),
        0
    );
    eprintln!(
        "oqto-6qff header inventory: {FILES} files in {DIRECTORIES} directories, {DIRECTORIES} cold chunks {elapsed:?}, warm {warm:?}, header reads {FILES} -> 0"
    );
    assert!(
        warm < elapsed / 3,
        "steady scan {warm:?} must be a small fraction of cold {elapsed:?}"
    );
    Ok(())
}

#[test]
fn bounded_header_read_keeps_incomplete_and_oversized_distinct() -> Result<()> {
    let (_dir, source, work) = fixture()?;
    let path = write(&source, &work, "native")?;
    let header = std::fs::read(&path)?;
    // Mid-write: the header line is not terminated yet (announce retries this).
    std::fs::write(&path, &header[..header.len() - 1])?;
    let error = source.header(&path).unwrap_err().to_string();
    assert!(error.contains("Incomplete native header"), "{error}");
    std::fs::write(&path, vec![b' '; MAX_HEADER_BYTES + 1])?;
    let error = source.header(&path).unwrap_err().to_string();
    assert!(error.contains("Oversized native header"), "{error}");
    // A terminated header followed by a body parses from the header line only.
    let mut full = header.clone();
    full.extend_from_slice(b"not JSON body that must never be parsed\n");
    std::fs::write(&path, &full)?;
    assert_eq!(source.header(&path)?.external_id, "native");
    Ok(())
}
