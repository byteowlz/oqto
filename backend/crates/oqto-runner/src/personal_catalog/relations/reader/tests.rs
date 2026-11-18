use super::*;
use anyhow::Result;
use serde_json::json;
use std::{path::PathBuf, sync::Mutex};

mod bounds;
mod replacement;
mod security;
mod transactions;

pub(super) static SERIAL: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
type Hook = Box<dyn FnOnce() -> Result<()> + Send>;
static AFTER_READ: Mutex<Option<Hook>> = Mutex::new(None);
pub(super) fn after_read() -> Result<()> {
    let hook = AFTER_READ
        .lock()
        .map_err(|_| anyhow::anyhow!("Synthetic hook poisoned"))?
        .take();
    if let Some(hook) = hook {
        hook()?;
    }
    Ok(())
}
pub(super) fn hook(action: impl FnOnce() -> Result<()> + Send + 'static) {
    *AFTER_READ.lock().unwrap() = Some(Box::new(action));
}
struct Fixture {
    _dir: tempfile::TempDir,
    source: PersonalSource,
    bindings: Vec<ObservationBinding>,
    data: Value,
}
impl Fixture {
    async fn new() -> Result<Self> {
        let (dir, source, workspace) = crate::personal_source::test_support::fixture()?;
        let mut bindings = Vec::new();
        for index in 1..=3 {
            let native = format!("00000000-0000-4000-8000-{index:012}");
            let path = crate::personal_source::test_support::write(&source, &workspace, &native)?;
            let header = source.header(&path)?;
            oqto_history::oqto_log::ops::native_source::discover(
                &source.home,
                "synthetic-owner",
                workspace.to_str().unwrap(),
                &native,
                &header.identity,
            )
            .await?;
            let rows = oqto_history::oqto_log::ops::list_sessions(&source.home, None).await?;
            let public = rows
                .iter()
                .find(|row| row.external_id.as_deref() == Some(&native))
                .unwrap()
                .platform_id
                .clone();
            bindings.push(ObservationBinding {
                authority_id: "synthetic-personal".into(),
                public_session_id: public,
                external_session_id: native,
                session_file: path,
                session_root: source.session_root().to_owned(),
                work_directory: workspace.clone(),
                workspace_root: source.workspace_roots()[0].clone(),
                socket_root: source.socket_root().to_owned(),
                grant_generation: "synthetic-not-a-live-grant".into(),
            });
        }
        let data = json!({"sessions":bindings.iter().map(|b|json!({"id":b.public_session_id,"title":"synthetic","parent_id":"legacy-is-not-evidence"})).collect::<Vec<_>>()});
        Ok(Self {
            _dir: dir,
            source,
            bindings,
            data,
        })
    }
    fn sidecar(&self, parent: usize) -> PathBuf {
        let mut name = self.bindings[parent].session_file.as_os_str().to_owned();
        name.push(".relations.json");
        PathBuf::from(name)
    }
    fn record(&self, parent: usize, child: usize) -> Value {
        json!({"kind":"delegation","parent_session_id":self.bindings[parent].external_session_id,
            "child_session_id":self.bindings[child].external_session_id,"evidence":"launch","confirmed_at_ms":1762560000000_u64})
    }
    fn envelope(&self, parent: usize, records: Vec<Value>) -> Value {
        json!({"v":1,"kind":"pi_session_relations","parent_session_id":self.bindings[parent].external_session_id,
            "updated_at":"2026-11-08T00:00:00Z","complete":false,"relations":records})
    }
    fn write(&self, parent: usize, records: Vec<Value>) -> Result<()> {
        std::fs::write(
            self.sidecar(parent),
            serde_json::to_vec(&self.envelope(parent, records))?,
        )?;
        Ok(())
    }
    fn context(&self) -> Context<'_> {
        Context {
            source: &self.source,
            bindings: &self.bindings,
        }
    }
    async fn output(&self) -> Value {
        let mut output = self.data.clone();
        super::super::enrich(&mut output, Some(self.context())).await;
        assert_eq!(output["sessions"].as_array().unwrap().len(), 3);
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
        output
    }
    async fn limited(&self, limits: Limits) -> Value {
        let facts = read_with_limits(&self.data, self.context(), limits).await;
        let mut output = self.data.clone();
        super::super::project(&mut output, &facts);
        output
    }
    fn expect(&self, output: &Value, child: usize, parent: usize) {
        assert_eq!(
            output["sessions"][child]["relations"],
            json!([{"kind":"delegation","parent_session_id":self.bindings[parent].public_session_id}])
        );
    }
}
fn empty(output: &Value) {
    assert!(
        output["sessions"]
            .as_array()
            .unwrap()
            .iter()
            .all(|row| row.get("relations").is_none())
    );
}

#[tokio::test]
async fn some_context_requires_exact_persisted_source_and_maps_only_public_ids() -> Result<()> {
    let _serial = SERIAL.lock().await;
    let fixture = Fixture::new().await?;
    let before: Vec<_> = fixture
        .bindings
        .iter()
        .map(|b| std::fs::read(&b.session_file).unwrap())
        .collect();
    fixture.write(0, vec![fixture.record(0, 2)])?;
    fixture.expect(&fixture.output().await, 2, 0);
    for (binding, bytes) in fixture.bindings.iter().zip(before) {
        assert_eq!(std::fs::read(&binding.session_file)?, bytes);
    }
    Ok(())
}

#[tokio::test]
async fn production_wrapper_remains_none_even_with_real_synthetic_sidecar() -> Result<()> {
    let _serial = SERIAL.lock().await;
    let fixture = Fixture::new().await?;
    fixture.write(0, vec![fixture.record(0, 2)])?;
    let rows = oqto_history::oqto_log::ops::list_sessions(&fixture.source.home, None).await?;
    let mut output = fixture.data.clone();
    super::super::super::enrich(
        &mut output,
        &rows,
        &crate::personal_catalog::NativeRoots {
            session_root: fixture.source.session_root().to_owned(),
            socket_root: fixture.source.socket_root().to_owned(),
        },
        fixture.source.workspace_roots(),
    )
    .await;
    empty(&output);
    assert_eq!(output["sessions"].as_array().unwrap().len(), 3);
    Ok(())
}

#[tokio::test]
async fn fresh_restart_reuses_durable_pins_but_never_caches_old_edges() -> Result<()> {
    let _serial = SERIAL.lock().await;
    let fixture = Fixture::new().await?;
    fixture.write(0, vec![fixture.record(0, 2)])?;
    let restarted = PersonalSource::new(
        fixture.source.home.clone(),
        fixture.source.session_root().to_owned(),
        fixture.source.socket_root().to_owned(),
        fixture.source.workspace_roots().to_vec(),
    )?;
    let mut output = fixture.data.clone();
    super::super::enrich(
        &mut output,
        Some(Context {
            source: &restarted,
            bindings: &fixture.bindings,
        }),
    )
    .await;
    fixture.expect(&output, 2, 0);
    std::fs::remove_file(fixture.sidecar(0))?;
    super::super::enrich(
        &mut output,
        Some(Context {
            source: &restarted,
            bindings: &fixture.bindings,
        }),
    )
    .await;
    empty(&output);
    Ok(())
}
