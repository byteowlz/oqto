use super::*;

#[tokio::test]
async fn entry_and_row_exhaustion_are_unavailable_never_partial_and_do_not_rescan_each_request()
-> Result<()> {
    for rows in [false, true] {
        let fixture = Fixture::new().await?;
        fixture.probe.gate.add_permits(8);
        if rows {
            fixture.probe.row_limit.store(2, Ordering::SeqCst);
        } else {
            fixture.probe.entry_limit.store(2, Ordering::SeqCst);
        }
        for _ in 0..3 {
            let error = fixture.lookup().await.unwrap_err();
            assert!(catalog_unavailable(&error), "{error:#}");
            assert!(
                !catalog_source_fault(&error),
                "resource exhaustion poisoned source health"
            );
        }
        assert_eq!(
            fixture.probe.scans.load(Ordering::SeqCst),
            1,
            "negative cache did not bound repeated failures"
        );
        let key = (
            fixture.home.path().to_owned(),
            vec![fixture.home.path().to_owned()],
        );
        assert!(
            !CATALOG_CACHE.lock().await.rows.contains_key(&key),
            "partial result became authority"
        );
    }
    Ok(())
}

#[tokio::test]
async fn a_late_cross_shard_duplicate_is_denied_and_out_of_scope_rows_are_not_authority()
-> Result<()> {
    let fixture = Fixture::new().await?;
    let late = fixture.home.path().join("zzz-late");
    std::fs::create_dir(&late)?;
    let message: oqto_pi::AgentMessage = serde_json::from_value(
        serde_json::json!({"role":"user","content":"late synthetic duplicate","timestamp":1700000000000_i64}),
    )?;
    oqto_history::oqto_log::store::replace_session_with_snapshot(
        fixture.home.path(),
        "fixture",
        late.to_str().context("workspace")?,
        "other-session",
        &fixture.id,
        Some("different-native"),
        "different-native",
        &[message],
    )
    .await?;
    fixture.probe.gate.add_permits(8);
    let error = fixture.lookup().await.unwrap_err();
    assert!(
        error.to_string().contains("Ambiguous public Session"),
        "{error:#}"
    );
    assert!(
        !catalog_unavailable(&error),
        "duplicate is denied, not transient"
    );
    let narrow = vec![fixture.home.path().join("work-0")];
    assert_eq!(
        resolve_authorized_session(fixture.home.path(), &narrow, &fixture.id)
            .await?
            .platform_id,
        fixture.id
    );
    let missing = resolve_authorized_session(fixture.home.path(), &narrow, "missing")
        .await
        .unwrap_err();
    assert!(!catalog_unavailable(&missing));
    Ok(())
}

#[tokio::test]
async fn corrupt_or_missing_source_never_returns_an_earlier_valid_row() -> Result<()> {
    let fixture = Fixture::new().await?;
    let workspace = fixture.home.path().join("zzz-broken");
    std::fs::create_dir(&workspace)?;
    let db = oqto_history::oqto_log::paths::existing_user_home_workspace_db_path(
        fixture.home.path(),
        workspace.to_str().context("workspace")?,
    );
    std::fs::create_dir_all(db.parent().context("shard parent")?)?;
    std::fs::write(db, "not a SQLite database")?;
    fixture.probe.gate.add_permits(8);
    for _ in 0..2 {
        let error = fixture.lookup().await.unwrap_err();
        assert!(catalog_unavailable(&error));
        assert!(
            catalog_source_fault(&error),
            "corrupt source was downgraded through flight/backoff"
        );
    }
    assert_eq!(fixture.probe.scans.load(Ordering::SeqCst), 1);
    let absent = fixture.home.path().join("missing-root");
    let error = resolve_authorized_session(fixture.home.path(), &[absent], &fixture.id)
        .await
        .unwrap_err();
    assert!(
        catalog_unavailable(&error),
        "missing selected root must not become an empty complete source"
    );
    Ok(())
}

#[tokio::test]
async fn missing_canonical_shard_database_refuses_the_entire_source() -> Result<()> {
    let fixture = Fixture::new().await?;
    let workspace = fixture.home.path().join("work-3");
    let db = oqto_history::oqto_log::paths::existing_user_home_workspace_db_path(
        fixture.home.path(),
        workspace.to_str().context("workspace")?,
    );
    std::fs::remove_file(db)?;
    fixture.probe.gate.add_permits(1);
    let error = fixture.lookup().await.unwrap_err();
    assert!(catalog_unavailable(&error), "{error:#}");
    assert!(
        catalog_source_fault(&error),
        "missing canonical database is an authority fault"
    );
    assert_eq!(
        fixture.probe.loads.load(Ordering::SeqCst),
        0,
        "incomplete inventory reached shard reads"
    );
    Ok(())
}

#[test]
fn excluded_names_and_symlink_targets_are_not_stat_followed_or_descended() -> Result<()> {
    let home = tempfile::tempdir()?;
    let outside = tempfile::tempdir()?;
    let probe = Arc::new(Probe::default());
    PROBES
        .lock()
        .unwrap()
        .insert(home.path().to_owned(), probe.clone());
    for name in [".hidden", "node_modules", "target", "vendor"] {
        std::fs::create_dir(home.path().join(name))?;
    }
    std::fs::create_dir(home.path().join("allowed"))?;
    std::fs::create_dir(home.path().join("target/selected-child"))?;
    std::os::unix::fs::symlink(outside.path(), home.path().join("outside-link"))?;
    std::os::unix::fs::symlink(home.path().join("cycle"), home.path().join("cycle"))?;
    let roots = vec![home.path().to_owned()];
    let found = workspaces_under(&roots, &AtomicBool::new(true), &AtomicBool::new(false))?;
    PROBES.lock().unwrap().remove(home.path());
    let calls = probe.operations.lock().unwrap();
    assert_eq!(
        calls.iter().filter(|(_, kind)| *kind == "read_dir").count(),
        2
    );
    assert_eq!(
        calls
            .iter()
            .filter(|(_, kind)| *kind == "file_type_no_follow")
            .count(),
        3
    );
    assert!(calls.iter().all(|(path, _)| {
        ![".hidden", "node_modules", "target", "vendor"]
            .iter()
            .any(|name| path.ends_with(name))
    }));
    assert!(
        !found
            .iter()
            .any(|path| path.contains("outside-link") || path.contains("cycle"))
    );
    assert!(
        calls
            .iter()
            .all(|(path, _)| !path.starts_with(outside.path()))
    );
    let selected = home.path().join("target");
    let explicit = workspaces_under(
        std::slice::from_ref(&selected),
        &AtomicBool::new(true),
        &AtomicBool::new(false),
    )?;
    assert!(explicit.contains(&selected.to_string_lossy().into_owned()));
    assert_eq!(
        explicit.len(),
        2,
        "name exclusions must not erase an explicitly selected root"
    );
    eprintln!(
        "excluded scan: 7 entries; 2 directory opens, 3 no-follow entry types; excluded names 0 metadata calls; symlink targets 0 opens"
    );
    Ok(())
}
