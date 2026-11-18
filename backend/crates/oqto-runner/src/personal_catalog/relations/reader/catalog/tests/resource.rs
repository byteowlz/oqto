use super::*;

#[tokio::test]
async fn raw_envelope_guard_rejects_oversized_genuine_pin_and_malformed_without_false_absence()
-> Result<()> {
    let _serial = super::super::super::tests::SERIAL.lock().await;
    let fixture = Fixture::new(3).await?;
    fixture.edge()?;
    fixture.positive().await?;
    let row = fixture
        .rows
        .iter()
        .find(|row| row.external_id.as_deref() == Some(fixture.native[2].0.as_str()))
        .unwrap();
    let db = oqto_history::oqto_log::paths::existing_user_home_workspace_db_path(
        &fixture.source.home,
        fixture.workspace.to_str().unwrap(),
    );
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(sqlx::sqlite::SqliteConnectOptions::new().filename(db))
        .await?;
    let original: String =
        sqlx::query_scalar("SELECT extensions_json FROM oqto_log_sessions WHERE platform_id=?")
            .bind(&row.platform_id)
            .fetch_one(&pool)
            .await?;
    let mut envelope: Value = serde_json::from_str(&original)?;
    assert_eq!(
        envelope["personal_native_source"],
        fixture.source.header(&fixture.native[2].1)?.identity
    );
    envelope["unrelated_padding"] = "".into();
    let overhead = serde_json::to_vec(&envelope)?.len();
    envelope["unrelated_padding"] = "x".repeat(65536 - overhead).into();
    let boundary = serde_json::to_string(&envelope)?;
    assert_eq!(boundary.len(), 65536);
    // Canonical synthetic database fault injection only; not a production
    // writer, repair, migration or change to the existing validator.
    sqlx::query("UPDATE oqto_log_sessions SET extensions_json=? WHERE platform_id=?")
        .bind(&boundary)
        .bind(&row.platform_id)
        .execute(&pool)
        .await?;
    fixture.positive().await?; // exact raw64KiB positive, genuine pin unchanged
    std::fs::write(
        sidecar(&fixture.native[2].1),
        serde_json::to_vec(&self::envelope(
            &fixture.native[2].0,
            vec![record(&fixture.native[2].0, &fixture.native[1].0)],
        ))?,
    )?;
    envelope["unrelated_padding"] = "x".repeat(65537 - overhead).into();
    let oversized = serde_json::to_string(&envelope)?;
    assert_eq!(oversized.len(), 65537);
    let malformed = format!("{{{}", "x".repeat(65536));
    // Deliberate invalid-store fixture; the toggle is connection-local and no
    // constraint/schema is changed. It permits the malformed-envelope probe.
    sqlx::query("PRAGMA ignore_check_constraints=ON")
        .execute(&pool)
        .await?;
    for raw in [oversized, malformed, "[]".into(), "{ malformed".into()] {
        sqlx::query("UPDATE oqto_log_sessions SET extensions_json=? WHERE platform_id=?")
            .bind(raw)
            .bind(&row.platform_id)
            .execute(&pool)
            .await?;
        assert!(
            native_source::catalog_pins(&fixture.source.home)
                .await
                .is_err()
        );
        let (facts, status, counts) = read(&fixture.data, fixture.context()).await;
        assert!(facts.is_empty());
        assert_eq!(status, ReadStatus::SourceUnavailable);
        assert_eq!(counts.validations, 0);
        let mut output = fixture.data.clone();
        super::super::super::super::project(&mut output, &facts);
        assert_eq!(output, fixture.data); // cannot hide the bad author and admit A->C
    }
    pool.close().await;
    Ok(())
}

#[tokio::test]
async fn actual_catalog_cancellation_retains_physical_permit_and_busy_never_starts_another_job()
-> Result<()> {
    let _serial = super::super::super::tests::SERIAL.lock().await;
    let fixture = Fixture::new(2).await?;
    fixture.edge()?;
    let (entered, resume) = fixture.source.pause_next_header_for_test()?;
    let source = fixture.source.clone();
    let rows = fixture.rows.clone();
    let data = fixture.data.clone();
    let task = tokio::spawn(async move {
        read(
            &data,
            Context {
                source: &source,
                home: &source.home,
                roots: source.workspace_roots(),
                rows: &rows,
            },
        )
        .await
    });
    tokio::time::timeout(Duration::from_secs(2), entered).await??;
    task.abort();
    assert!(matches!(task.await,Err(error) if error.is_cancelled()));
    assert_eq!(super::super::super::WORKER.available_permits(), 0);
    let (facts, status, _) = read(&fixture.data, fixture.context()).await;
    assert!(facts.is_empty());
    assert_eq!(status, ReadStatus::Busy);
    resume.send(())?;
    tokio::time::timeout(Duration::from_secs(2), async {
        while super::super::super::WORKER.available_permits() != 1 {
            tokio::task::yield_now().await;
        }
    })
    .await?;
    fixture.positive().await?;
    Ok(())
}

fn sidecar(path: &Path) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(".relations.json");
    PathBuf::from(name)
}
fn record(parent: &str, child: &str) -> Value {
    json!({"kind":"delegation","parent_session_id":parent,"child_session_id":child,"evidence":"launch","confirmed_at_ms":1762560000000_u64})
}
fn envelope(parent: &str, records: Vec<Value>) -> Value {
    json!({"v":1,"kind":"pi_session_relations","parent_session_id":parent,"updated_at":"2026-11-08T00:00:00Z","complete":false,"relations":records})
}

#[tokio::test]
async fn present_file_author_boundary_is_not_a_row_prefix_and_one_more_discards_whole_batch()
-> Result<()> {
    let _serial = super::super::super::tests::SERIAL.lock().await;
    let limit = super::super::CATALOG_AUTHORS;
    let fixture = Fixture::new(limit + 1).await?;
    for index in 0..limit {
        std::fs::write(
            sidecar(&fixture.native[index].1),
            serde_json::to_vec(&envelope(
                &fixture.native[index].0,
                vec![record(
                    &fixture.native[index].0,
                    &fixture.native[index + 1].0,
                )],
            ))?,
        )?;
    }
    let (facts, status, counts) = read(&fixture.data, fixture.context()).await;
    assert_eq!(status, ReadStatus::Read, "counts={counts:?}");
    assert_eq!(facts.len(), limit);
    assert_eq!(counts.files, limit, "authors only");
    assert_eq!(counts.records, limit);
    assert!(counts.retained_fds + 32 <= 512);
    std::fs::write(
        sidecar(&fixture.native[limit].1),
        serde_json::to_vec(&envelope(&fixture.native[limit].0, vec![]))?,
    )?;
    let (facts, status, counts) = read(&fixture.data, fixture.context()).await;
    assert!(facts.is_empty());
    assert_ne!(status, ReadStatus::Read);
    let mut output = fixture.data.clone();
    super::super::super::super::project(&mut output, &facts);
    assert_eq!(output, fixture.data);
    println!(
        "MEASURE present{}-unavailable rows={} counts={counts:?}",
        limit + 1,
        limit + 1
    );
    Ok(())
}

#[tokio::test]
async fn late_unpinned_author_becoming_pinned_invalidates_complete_global_snapshot() -> Result<()> {
    let _serial = super::super::super::tests::SERIAL.lock().await;
    let mut fixture = Fixture::new(2).await?;
    fixture.edge()?;
    let native = "00000000-0000-4000-8000-000000000099";
    let file =
        crate::personal_source::test_support::write(&fixture.source, &fixture.workspace, native)?;
    let public = oqto_history::oqto_log::store::platform_id_for_external_id(native);
    oqto_history::oqto_log::ops::upsert_session_identity(
        &fixture.source.home,
        "synthetic-owner",
        fixture.workspace.to_str().unwrap(),
        &public,
        Some(&public),
        Some(native),
    )
    .await?;
    let parent = fixture.rows[0].session_id.clone();
    oqto_history::oqto_log::store::record_session_fork(
        &fixture.source.home,
        fixture.workspace.to_str().unwrap(),
        &public,
        &parent,
        "synthetic-entry",
        None,
        None,
    )
    .await?;
    fixture.rows = oqto_history::oqto_log::ops::list_sessions(&fixture.source.home, None).await?;
    fixture.data = json!({"sessions":fixture.rows.iter().map(|row|json!({"id":row.platform_id,"workspace":row.workspace_id,"title":"synthetic"})).collect::<Vec<_>>(),"read_only":true});
    std::fs::write(
        sidecar(&file),
        serde_json::to_vec(&envelope(
            native,
            vec![record(native, &fixture.native[1].0)],
        ))?,
    )?;
    fixture.positive().await?; // unpinned sidecar is NOT a legitimate author yet
    let (entered, ready) = tokio::sync::oneshot::channel();
    let (resume, paused) = std::sync::mpsc::channel();
    super::super::super::tests::hook(move || {
        let _ = entered.send(());
        paused.recv_timeout(Duration::from_secs(2))?;
        Ok(())
    });
    let request = read(&fixture.data, fixture.context());
    tokio::pin!(request);
    tokio::select! {_=&mut request=>panic!("reader completed before synthetic phase fence"),result=ready=>{result?;}}
    let header = fixture.source.header(&file)?;
    native_source::discover(
        &fixture.source.home,
        "synthetic-owner",
        fixture.workspace.to_str().unwrap(),
        native,
        &header.identity,
    )
    .await?;
    resume.send(())?;
    let (facts, status, _) = request.await;
    assert!(facts.is_empty());
    assert_ne!(status, ReadStatus::Read);
    Ok(())
}

#[tokio::test]
async fn malformed_pinned_source_is_not_skipped_and_nonselected_metadata_never_leaks() -> Result<()>
{
    let _serial = super::super::super::tests::SERIAL.lock().await;
    let fixture = Fixture::new(2).await?;
    fixture.edge()?;
    let outside = fixture.source.home.join("synthetic-nonselected-workspace");
    std::fs::create_dir(&outside)?;
    native_source::discover(
        &fixture.source.home,
        "synthetic-nonselected-owner",
        outside.to_str().unwrap(),
        "synthetic-nonselected-id",
        "synthetic-private-pin-value",
    )
    .await?;
    let (facts, status, _) = read(&fixture.data, fixture.context()).await;
    assert_eq!(status, ReadStatus::Read);
    assert_eq!(facts.len(), 1);
    let mut output = fixture.data.clone();
    super::super::super::super::project(&mut output, &facts);
    let response = output.to_string();
    for private in ["synthetic-nonselected", "synthetic-private-pin-value"] {
        assert!(!response.contains(private));
    }
    let native = &fixture.native[0];
    let header = fixture.source.header(&native.1)?;
    let public = fixture
        .rows
        .iter()
        .find(|row| row.external_id.as_deref() == Some(native.0.as_str()))
        .unwrap()
        .platform_id
        .clone();
    assert!(
        native_source::repin(
            &fixture.source.home,
            fixture.workspace.to_str().unwrap(),
            &public,
            &native.0,
            &header.identity,
            "malformed-synthetic-pin"
        )
        .await?
    );
    let (facts, status, counts) = read(&fixture.data, fixture.context()).await;
    // Superseded 2026-11-17 by a measured whole-catalog outage: 44 of 60 authors on a
    // real home held pins that no longer proved them, and a whole-batch refusal turned
    // that into "no Session nests anywhere", silently. An endpoint that cannot be
    // proven now keeps ONLY its own Session flat and is counted; it still emits
    // nothing, and the non-selected leak assertions above are unchanged.
    assert!(facts.is_empty(), "an unproven pin emits no edge");
    assert_eq!(status, ReadStatus::Read);
    assert!(counts.skipped >= 1, "counts={counts:?}");
    Ok(())
}

/// Owner report 2026-11-17, "still no nesting" with the Runner already deploying the
/// sidecar-author fix: seven pins carry an older Pi session name
/// (`pi-1770403246506-31fd802b06a26`) instead of a native UUID, and that one class
/// refused the whole batch, so no Session nested anywhere on the machine.
#[tokio::test]
async fn a_legacy_pi_named_pin_is_skipped_without_losing_the_proven_relation() -> Result<()> {
    let _serial = super::super::super::tests::SERIAL.lock().await;
    let mut fixture = Fixture::new(2).await?;
    fixture.edge()?;
    native_source::discover(
        &fixture.source.home,
        "synthetic-owner",
        fixture.workspace.to_str().unwrap(),
        "pi-1770403246506-31fd802b06a26",
        "synthetic-legacy-pin",
    )
    .await?;
    // Re-list: the foreign-format pin is part of the selected catalog now.
    fixture.rows = oqto_history::oqto_log::ops::list_sessions(&fixture.source.home, None).await?;
    fixture.data = serde_json::json!({
        "sessions": fixture.rows.iter().map(|row| serde_json::json!({
            "id": row.platform_id, "workspace": row.workspace_id, "title": "synthetic",
        })).collect::<Vec<_>>(),
        "read_only": true,
    });
    let (facts, status, counts) = read(&fixture.data, fixture.context()).await;
    assert_eq!(status, ReadStatus::Read, "counts={counts:?}");
    assert_eq!(
        facts.len(),
        1,
        "the proven edge survives a foreign-format pin"
    );
    assert_eq!(counts.skipped, 1, "counts={counts:?}");
    Ok(())
}

/// Owner report 2026-11-16 ("nesting still doesn't work"): the real catalog holds
/// thousands of native Sessions, so a reader that opened every one blew the
/// descriptor budget and no relation reached the phone. Only Sessions that
/// publish a sidecar are authors; the rest are matched through the pins.
#[tokio::test]
async fn all_native5000_catalog_still_emits_the_proven_relation_from_its_one_author() -> Result<()>
{
    let _serial = super::super::super::tests::SERIAL.lock().await;
    let fixture = Fixture::new(5000).await?;
    fixture.edge()?;
    assert_eq!(fixture.rows.len(), 5000);
    let (facts, status, counts) = read(&fixture.data, fixture.context()).await;
    assert_eq!(status, ReadStatus::Read, "counts={counts:?}");
    assert_eq!(facts.len(), 1);
    assert_eq!(counts.native, 5000);
    assert_eq!(counts.files, 1, "only the author's file is opened");
    assert!(counts.retained_fds + 32 <= 512);
    let mut output = fixture.data.clone();
    super::super::super::super::project(&mut output, &facts);
    assert_eq!(output["sessions"].as_array().unwrap().len(), 5000);
    println!("MEASURE allnative5000 counts={counts:?}");
    Ok(())
}
