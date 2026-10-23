use super::*;
use std::os::unix::fs::symlink;

fn fixture() -> Result<(tempfile::TempDir, PersonalSource, PathBuf)> {
    let dir = tempfile::tempdir()?;
    let root = dir.path().canonicalize()?;
    let home = root.join("service");
    let sessions = root.join("native/.pi/agent/sessions");
    let work = root.join("byteowlz/work");
    let sockets = root.join("terminal-tmp");
    for path in [&home, &sessions, &work, &sockets] {
        std::fs::create_dir_all(path)?;
    }
    let source = PersonalSource::new(home, sessions, sockets, vec![root.join("byteowlz")])?;
    Ok((dir, source, work))
}
fn write(source: &PersonalSource, work: &Path, native: &str) -> Result<PathBuf> {
    let dir = source
        .session_root()
        .join(oqto_pi::session_files::cwd_to_safe_dirname(work));
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(format!("2026-10-19_{native}.jsonl"));
    std::fs::write(
        &path,
        format!(
            "{}\n",
            serde_json::json!({"type":"session","id":native,"cwd":work})
        ),
    )?;
    Ok(path)
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
