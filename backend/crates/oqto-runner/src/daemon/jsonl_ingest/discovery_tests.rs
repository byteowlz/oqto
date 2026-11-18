//! Deterministic cancellation and fail-closed recovery at the real scan seam.
use super::*;
use crate::personal_source::test_support::{fixture as source_fixture, write};
use anyhow::Context;
use std::sync::Arc;

struct Fixture {
    _root: tempfile::TempDir,
    source: Arc<crate::personal_source::PersonalSource>,
    home: PathBuf,
    workspace: PathBuf,
}
impl Fixture {
    fn new() -> anyhow::Result<Self> {
        let (root, source, workspace) = source_fixture()?;
        write(&source, &workspace, "first")?;
        Ok(Self {
            _root: root,
            home: source.home.clone(),
            source: Arc::new(source),
            workspace,
        })
    }
}

async fn reaches(handle: &IngestHandle, expected: DiscoveryReadiness) -> anyhow::Result<()> {
    let mut state = handle.readiness.clone();
    tokio::time::timeout(Duration::from_secs(15), async {
        while *state.borrow_and_update() != expected || handle.readiness() != expected {
            state.changed().await?;
        }
        anyhow::Ok(())
    })
    .await??;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cancelled_header_chunk_keeps_permit_until_worker_exits_without_adoption()
-> anyhow::Result<()> {
    let fixture = Fixture::new()?;
    let (entered, resume) = fixture.source.pause_next_header_for_test()?;
    let pass = Discovery::start(&fixture.source).await?.context("scan")?;
    let mut job = pass.next_job(0, None).context("scan job")?;
    tokio::time::timeout(Duration::from_secs(15), entered).await??;
    job.task.abort();
    match (&mut job.task).await {
        Err(error) => assert!(error.is_cancelled()),
        Ok(_) => anyhow::bail!("read completed despite the explicit pause"),
    }
    drop(job);
    assert_eq!(
        HEADER_SCANNER.available_permits(),
        0,
        "cancelled async caller cannot release its still-running filesystem worker permit"
    );
    let replacement = Discovery::start(&fixture.source)
        .await?
        .context("replacement scan")?;
    let mut replacement = replacement.next_job(0, None).context("replacement job")?;
    resume.send(())?;
    let event = tokio::time::timeout(Duration::from_secs(15), replacement.wait()).await?;
    let DiscoveryEvent::Scanned { pass, result } = event else {
        anyhow::bail!("replacement did not complete its scan");
    };
    result?;
    assert!(pass.scan.is_none());
    assert_eq!(pass.headers.len(), 1);
    assert!(
        ops::list_sessions(&fixture.home, None).await?.is_empty(),
        "neither a cancelled scan nor its replacement read may adopt outside the writer"
    );
    discover(&fixture.source, "owner").await?;
    assert_eq!(ops::list_sessions(&fixture.home, None).await?.len(), 1);
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn stale_probe_cannot_clear_new_fault_generation_and_retry_recovers() -> anyhow::Result<()> {
    let fixture = Fixture::new()?;
    let (handle, release) = spawn_paused_inventory_for_test(
        fixture.home.clone(),
        "owner".into(),
        fixture.source.clone(),
    );
    // Model a fault whose best-effort nudge was lost: the generation fence must
    // reject the in-flight generation-zero result without depending on delivery.
    assert_eq!(handle.health.fail(), 1);
    release
        .send(())
        .map_err(|_| anyhow::anyhow!("inventory ended"))?;
    reaches(&handle, DiscoveryReadiness::Failed).await?;
    assert!(!handle.catalog_ready() && !handle.native_available());
    assert!(handle.health.unavailable());
    assert_eq!(
        handle
            .health
            .verified
            .load(std::sync::atomic::Ordering::SeqCst),
        0
    );
    assert!(ops::list_sessions(&fixture.home, None).await?.is_empty());
    reaches(&handle, DiscoveryReadiness::Ready).await?;
    assert!(!handle.health.unavailable());
    assert_eq!(ops::list_sessions(&fixture.home, None).await?.len(), 1);
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn warmed_inventory_fault_refuses_catalog_then_retries_without_nudges() -> anyhow::Result<()>
{
    let fixture = Fixture::new()?;
    discover(&fixture.source, "owner").await?;
    let before = ops::list_sessions(&fixture.home, None).await?;
    assert_eq!(before.len(), 1);
    write(&fixture.source, &fixture.workspace, "second")?;
    let defaults = fixture.home.join(".pi/agent/sessions");
    std::fs::create_dir_all(&defaults)?;
    let broken = defaults.join("unreadable-workspace");
    std::os::unix::fs::symlink(fixture.home.join("missing"), &broken)?;
    let (handle, release) = spawn_paused_inventory_for_test(
        fixture.home.clone(),
        "owner".into(),
        fixture.source.clone(),
    );
    release
        .send(())
        .map_err(|_| anyhow::anyhow!("inventory ended"))?;
    reaches(&handle, DiscoveryReadiness::Failed).await?;
    assert!(!handle.catalog_ready());
    assert!(!handle.native_available());
    let retained = ops::list_sessions(&fixture.home, None).await?;
    assert_eq!(
        retained.len(),
        1,
        "incomplete inventory cannot admit the second source"
    );
    assert_eq!(retained[0].platform_id, before[0].platform_id);
    handle.mark_authority_failed();
    assert!(handle.health.unavailable());
    assert_eq!(handle.readiness(), DiscoveryReadiness::Failed);
    std::fs::remove_file(broken)?;
    // No foreground request, nudge or sweep: the existing fault retry must heal.
    reaches(&handle, DiscoveryReadiness::Ready).await?;
    assert!(handle.catalog_ready() && handle.native_available());
    assert!(!handle.health.unavailable());
    assert_eq!(handle.health.generation(), 1);
    let rows = ops::list_sessions(&fixture.home, None).await?;
    assert_eq!(rows.len(), 2);
    assert!(
        rows.iter()
            .any(|row| row.platform_id == before[0].platform_id)
    );
    Ok(())
}
