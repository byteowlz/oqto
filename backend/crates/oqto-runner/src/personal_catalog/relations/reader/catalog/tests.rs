use super::*;
use serde_json::json;
mod resource;
mod signed;

struct Fixture {
    _dir: tempfile::TempDir,
    source: PersonalSource,
    workspace: PathBuf,
    native: Vec<(String, PathBuf)>,
    rows: Vec<OqtoLogSessionRow>,
    data: Value,
}
impl Fixture {
    async fn new(count: usize) -> Result<Self> {
        let (dir, source, workspace) = crate::personal_source::test_support::fixture()?;
        let mut native = Vec::new();
        for n in 0..count {
            let id = format!("00000000-0000-4000-8000-{n:012}");
            let path = crate::personal_source::test_support::write(&source, &workspace, &id)?;
            native.push((id, path));
        }
        let mut discovery = native_source::DiscoveryBatch::open(&source.home).await?;
        for (id, path) in &native {
            let header = source.header(path)?;
            let prepared = discovery.prepare(id).await?;
            discovery
                .commit(
                    "synthetic-owner",
                    workspace.to_str().unwrap(),
                    &header.identity,
                    prepared,
                )
                .await?;
        }
        let rows = oqto_history::oqto_log::ops::list_sessions(&source.home, None).await?;
        let data = json!({"sessions":rows.iter().map(|row|json!({"id":row.platform_id,"workspace":row.workspace_id,"title":"synthetic"})).collect::<Vec<_>>(),"read_only":true});
        Ok(Self {
            _dir: dir,
            source,
            workspace,
            native,
            rows,
            data,
        })
    }
    fn context(&self) -> Context<'_> {
        Context {
            source: &self.source,
            home: &self.source.home,
            roots: self.source.workspace_roots(),
            rows: &self.rows,
        }
    }
    fn edge(&self) -> Result<()> {
        let mut file = self.native[0].1.as_os_str().to_owned();
        file.push(".relations.json");
        std::fs::write(
            PathBuf::from(file),
            serde_json::to_vec(
                &json!({"v":1,"kind":"pi_session_relations","parent_session_id":self.native[0].0,"updated_at":"2026-11-08T00:00:00Z","complete":false,"relations":[{"kind":"delegation","parent_session_id":self.native[0].0,"child_session_id":self.native[1].0,"evidence":"launch","confirmed_at_ms":1762560000000_u64}]}),
            )?,
        )?;
        Ok(())
    }
    async fn positive(&self) -> Result<Counts> {
        let (facts, status, counts) = read(&self.data, self.context()).await;
        assert_eq!(status, ReadStatus::Read, "counts={counts:?}");
        assert_eq!(facts.len(), 1);
        assert_eq!(counts.native, self.native.len());
        // Only authors (sidecar publishers) are opened and validated, twice each.
        let authors = self
            .native
            .iter()
            .filter(|(_, path)| {
                let mut name = path.as_os_str().to_owned();
                name.push(".relations.json");
                PathBuf::from(name).exists()
            })
            .count();
        assert_eq!(counts.validations, authors * 2);
        assert_eq!(counts.files, authors);
        assert_eq!(counts.pin_inventories, 2);
        assert!(counts.retained_fds + 32 <= 512);
        assert!(counts.elapsed_ms < 4000);
        let mut output = self.data.clone();
        super::super::super::project(&mut output, &facts);
        assert_eq!(
            output["sessions"].as_array().unwrap().len(),
            self.rows.len()
        );
        for (actual, expected) in output["sessions"]
            .as_array()
            .unwrap()
            .iter()
            .zip(self.data["sessions"].as_array().unwrap())
        {
            let mut actual = actual.clone();
            actual.as_object_mut().unwrap().remove("relations");
            assert_eq!(&actual, expected);
        }
        println!(
            "MEASURE native={} rows={} counts={counts:?}",
            self.native.len(),
            self.rows.len()
        );
        Ok(counts)
    }
}

#[tokio::test]
async fn actual_default64_and65_complete_native_catalogs_emit_relations_with_measured_bounds()
-> Result<()> {
    let _serial = super::super::tests::SERIAL.lock().await;
    for count in [64, 65] {
        let fixture = Fixture::new(count).await?;
        fixture.edge()?;
        fixture.positive().await?;
    }
    Ok(())
}

#[tokio::test]
async fn complete5000_mixed_catalog_keeps_all_rows_and_proven_native_relation() -> Result<()> {
    let _serial = super::super::tests::SERIAL.lock().await;
    let mut fixture = Fixture::new(2).await?;
    fixture.edge()?;
    let identities: Vec<_> = (2..5000)
        .map(|n| oqto_history::oqto_log::ops::SessionIdentityInput {
            external_id: format!("synthetic-unpinned-{n}"),
            platform_id: format!("synthetic-public-{n}"),
            title: Some("synthetic".into()),
            readable_id: None,
            created_at: None,
            updated_at: None,
        })
        .collect();
    let outcome = oqto_history::oqto_log::ops::batch_upsert_session_identities(
        &fixture.source.home,
        "synthetic-owner",
        fixture.workspace.to_str().unwrap(),
        &identities,
    )
    .await?;
    assert_eq!(outcome.upserted, 4998);
    assert!(outcome.conflicts.is_empty());
    // Actual catalog visibility requires messages, explicit lineage, or a
    // native pin. Seed real canonical legacy fork records through the existing
    // store API; do NOT fabricate visible rows or infer delegation from them.
    let parent = fixture
        .rows
        .iter()
        .find(|row| row.external_id.as_deref() == Some(&fixture.native[0].0))
        .unwrap()
        .session_id
        .clone();
    for identity in &identities {
        oqto_history::oqto_log::store::record_session_fork(
            &fixture.source.home,
            fixture.workspace.to_str().unwrap(),
            &identity.platform_id,
            &parent,
            "synthetic-entry",
            None,
            None,
        )
        .await?;
    }
    fixture.rows = oqto_history::oqto_log::ops::list_sessions(&fixture.source.home, None).await?;
    assert_eq!(fixture.rows.len(), 5000);
    fixture.data = json!({"sessions":fixture.rows.iter().map(|row|json!({"id":row.platform_id,"workspace":row.workspace_id,"title":"synthetic"})).collect::<Vec<_>>(),"read_only":true});
    fixture.positive().await?;
    Ok(())
}
