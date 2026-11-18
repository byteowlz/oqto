use super::*;
use std::os::unix::fs::{PermissionsExt, symlink};

#[tokio::test]
async fn exact_header_without_persisted_pin_or_wrong_explicit_home_cannot_admit() -> Result<()> {
    let _serial = SERIAL.lock().await;
    let mut fixture = Fixture::new().await?;
    fixture.write(0, vec![fixture.record(0, 2)])?;
    let other_home = fixture.source.home.join("different-owner-home");
    std::fs::create_dir(&other_home)?;
    fixture.source = PersonalSource::new(
        other_home.clone(),
        fixture.source.session_root().to_owned(),
        fixture.source.socket_root().to_owned(),
        fixture.source.workspace_roots().to_vec(),
    )?;
    empty(&fixture.output().await);
    assert!(!other_home.join(".local/share/oqto").exists());
    Ok(())
}

#[tokio::test]
async fn native_public_aliases_duplicate_endpoints_and_filtered_parent_fail_closed() -> Result<()> {
    let _serial = SERIAL.lock().await;
    let mut fixture = Fixture::new().await?;
    fixture.write(0, vec![fixture.record(0, 2)])?;
    let original = fixture.bindings.clone();
    fixture.bindings[0].public_session_id = fixture.bindings[0].external_session_id.clone();
    fixture.data["sessions"][0]["id"] = fixture.bindings[0].public_session_id.clone().into();
    empty(&fixture.output().await);
    fixture.bindings = original.clone();
    fixture.data["sessions"][0]["id"] = original[0].public_session_id.clone().into();
    fixture.bindings[1].public_session_id = original[0].public_session_id.clone();
    empty(&fixture.output().await);
    fixture.bindings = original;
    let mut filtered = fixture.data.clone();
    filtered["sessions"].as_array_mut().unwrap().remove(0);
    super::super::super::enrich(&mut filtered, Some(fixture.context())).await;
    empty(&filtered);
    assert_eq!(filtered["sessions"].as_array().unwrap().len(), 2);
    Ok(())
}

#[tokio::test]
async fn duplicate_native_mapping_and_persisted_cross_shard_conflict_are_not_repaired() -> Result<()>
{
    let _serial = SERIAL.lock().await;
    let mut fixture = Fixture::new().await?;
    fixture.write(0, vec![fixture.record(0, 2)])?;
    let original = fixture.bindings.clone();
    fixture.bindings[1].external_session_id = fixture.bindings[0].external_session_id.clone();
    empty(&fixture.output().await);
    fixture.bindings = original;
    oqto_history::oqto_log::ops::upsert_session_identity(
        &fixture.source.home,
        "synthetic-owner",
        fixture.source.home.to_str().unwrap(),
        "raw-conflicting-row",
        Some(&fixture.bindings[0].public_session_id),
        Some(&fixture.bindings[0].external_session_id),
    )
    .await?;
    empty(&fixture.output().await);
    Ok(())
}

#[tokio::test]
async fn record_author_unknown_evidence_fork_self_and_unadmitted_child_never_grant_edges()
-> Result<()> {
    let _serial = SERIAL.lock().await;
    let fixture = Fixture::new().await?;
    let mut bad_author = fixture.record(0, 2);
    bad_author["parent_session_id"] = fixture.bindings[1].external_session_id.clone().into();
    let mut fork = fixture.record(0, 2);
    fork["kind"] = "fork".into();
    let mut evidence = fixture.record(0, 2);
    evidence["evidence"] = "title".into();
    let mut unknown = fixture.record(0, 2);
    unknown["child_session_id"] = "99999999-9999-4999-8999-999999999999".into();
    fixture.write(
        0,
        vec![
            bad_author,
            fork,
            evidence,
            unknown,
            fixture.record(0, 0),
            json!({"unknown":"record"}),
        ],
    )?;
    empty(&fixture.output().await);
    let mut valid = fixture.record(0, 2);
    valid["unknown_extra"] = "ignored".into();
    fixture.write(0, vec![valid])?;
    fixture.expect(&fixture.output().await, 2, 0);
    Ok(())
}

#[tokio::test]
async fn malformed_envelope_oversized_and_unknown_version_discard_all_optional_not_base_rows()
-> Result<()> {
    let _serial = SERIAL.lock().await;
    let fixture = Fixture::new().await?;
    fixture.write(0, vec![fixture.record(0, 2)])?;
    for invalid in [
        json!({"v":2}),
        json!(null),
        fixture.envelope(1, vec![fixture.record(1, 2)]),
        {
            let mut e = fixture.envelope(0, vec![fixture.record(0, 2)]);
            e["complete"] = true.into();
            e
        },
    ] {
        std::fs::write(fixture.sidecar(1), serde_json::to_vec(&invalid)?)?;
        // One correctly authored valid envelope for1 would conflict, not be invalid.
        let output = fixture.output().await;
        empty(&output);
    }
    std::fs::write(fixture.sidecar(1), b"{ malformed")?;
    empty(&fixture.output().await);
    Ok(())
}

#[tokio::test]
async fn selected_root_and_native_file_replacement_symlinks_are_refused() -> Result<()> {
    let _serial = SERIAL.lock().await;
    let mut fixture = Fixture::new().await?;
    fixture.write(0, vec![fixture.record(0, 2)])?;
    let old = fixture.bindings.clone();
    fixture.bindings[0].session_root = fixture.source.home.clone();
    empty(&fixture.output().await);
    fixture.bindings = old;
    let path = &fixture.bindings[0].session_file;
    let bytes = std::fs::read(path)?;
    let replacement = path.with_extension("replacement");
    std::fs::write(&replacement, &bytes)?;
    std::fs::rename(&replacement, path)?;
    empty(&fixture.output().await); // same header/bytes, new inode, persisted pin unchanged
    std::fs::rename(path, &replacement)?;
    symlink(&replacement, path)?;
    empty(&fixture.output().await);
    Ok(())
}

#[tokio::test]
async fn no_follow_sidecar_directory_and_selected_root_replacement_are_refused() -> Result<()> {
    let _serial = SERIAL.lock().await;
    let fixture = Fixture::new().await?;
    fixture.write(0, vec![fixture.record(0, 2)])?;
    let path = fixture.sidecar(0);
    let target = path.with_extension("saved");
    std::fs::rename(&path, &target)?;
    symlink(&target, &path)?;
    empty(&fixture.output().await);
    std::fs::remove_file(&path)?;
    std::fs::rename(&target, &path)?;
    std::fs::remove_file(&path)?;
    // rustix intentionally has no mkfifoat on Apple; POSIX fixture utility,
    // exact argv and a synthetic path, never a production source operation.
    assert!(
        std::process::Command::new("/usr/bin/mkfifo")
            .arg(&path)
            .status()?
            .success()
    );
    empty(&fixture.output().await); // nonblocking open, reject non-regular file
    std::fs::remove_file(&path)?;
    fixture.write(0, vec![fixture.record(0, 2)])?;
    fixture.expect(&fixture.output().await, 2, 0); // no abandoned FIFO reader/permit
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o666))?;
    empty(&fixture.output().await);
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
    let directory = fixture.bindings[0].session_file.parent().unwrap();
    let saved = directory.with_extension("saved");
    std::fs::rename(directory, &saved)?;
    symlink(&saved, directory)?;
    empty(&fixture.output().await);
    std::fs::remove_file(directory)?;
    std::fs::rename(&saved, directory)?;
    let root = fixture.source.session_root();
    let saved = root.with_extension("saved");
    std::fs::rename(root, &saved)?;
    std::fs::create_dir(root)?;
    empty(&fixture.output().await);
    Ok(())
}
