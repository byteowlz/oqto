use super::*;
use std::time::Duration;

async fn wait_for_no_flight(home: &std::path::Path) -> Result<()> {
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if !CATALOG_CACHE
                .lock()
                .await
                .flights
                .keys()
                .any(|(source, _)| source == home)
            {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await?;
    Ok(())
}

#[tokio::test]
async fn cancelling_one_waiter_keeps_the_shared_build_but_last_waiter_stops_it() -> Result<()> {
    let fixture = Fixture::new().await?;
    let first = fixture.request();
    let second = fixture.request();
    tokio::time::timeout(Duration::from_secs(2), fixture.probe.entered.notified()).await?;
    first.abort();
    let _ = first.await;
    for _ in 0..8 {
        tokio::task::yield_now().await;
    }
    assert_eq!(fixture.probe.scans.load(Ordering::SeqCst), 1);
    fixture.probe.gate.add_permits(1);
    assert_eq!(second.await??.platform_id, fixture.id);
    invalidate_catalog(fixture.home.path()).await;
    let abandoned = fixture.request();
    tokio::time::timeout(Duration::from_secs(2), fixture.probe.entered.notified()).await?;
    abandoned.abort();
    let _ = abandoned.await;
    wait_for_no_flight(fixture.home.path()).await?;
    assert!(
        !CATALOG_CACHE
            .lock()
            .await
            .rows
            .keys()
            .any(|(home, _)| home == fixture.home.path())
    );
    fixture.probe.gate.add_permits(1);
    assert_eq!(fixture.lookup().await?.platform_id, fixture.id);
    assert_eq!(fixture.probe.scans.load(Ordering::SeqCst), 3);
    Ok(())
}

#[tokio::test]
async fn invalidation_fences_inflight_and_already_published_but_undelivered_results() -> Result<()>
{
    for before_publication in [true, false] {
        let fixture = Fixture::new().await?;
        if !before_publication {
            fixture.probe.delivery_pause.store(true, Ordering::SeqCst);
            fixture.probe.gate.add_permits(1);
        }
        let home = fixture.home.path().to_owned();
        let request = tokio::spawn(async move {
            catalog_work::read_once(&home, std::slice::from_ref(&home)).await
        });
        let entered = if before_publication {
            &fixture.probe.entered
        } else {
            &fixture.probe.delivery_entered
        };
        tokio::time::timeout(Duration::from_secs(2), entered.notified()).await?;
        invalidate_catalog(fixture.home.path()).await;
        fixture.probe.delivery_gate.add_permits(1);
        let error = request.await?.unwrap_err();
        assert!(catalog_unavailable(&error));
        assert!(
            !catalog_source_fault(&error),
            "invalidation poisoned source health"
        );
        wait_for_no_flight(fixture.home.path()).await?;
        assert!(
            !CATALOG_CACHE
                .lock()
                .await
                .rows
                .keys()
                .any(|(home, _)| home == fixture.home.path())
        );
        fixture.probe.delivery_pause.store(false, Ordering::SeqCst);
        fixture.probe.gate.add_permits(1);
        assert_eq!(fixture.lookup().await?.platform_id, fixture.id);
        assert_eq!(fixture.probe.scans.load(Ordering::SeqCst), 2);
    }
    Ok(())
}

#[tokio::test]
async fn one_source_epoch_change_retries_only_a_fresh_complete_observation() -> Result<()> {
    let fixture = Fixture::new().await?;
    let request = fixture.request();
    tokio::time::timeout(Duration::from_secs(2), fixture.probe.entered.notified()).await?;
    invalidate_catalog(fixture.home.path()).await;
    tokio::time::timeout(Duration::from_secs(2), fixture.probe.entered.notified()).await?;
    assert_eq!(fixture.probe.scans.load(Ordering::SeqCst), 2);
    assert!(
        !request.is_finished(),
        "invalidated result was delivered before the fresh fill"
    );
    fixture.probe.gate.add_permits(1);
    assert_eq!(request.await??.platform_id, fixture.id);
    Ok(())
}

#[tokio::test]
async fn blocked_filesystem_keeps_its_permit_after_response_deadline_without_blocking_reactor()
-> Result<()> {
    let fixture = Fixture::new().await?;
    fixture.probe.fs_pause.store(true, Ordering::SeqCst);
    fixture.probe.deadline_ms.store(100, Ordering::SeqCst);
    let request = fixture.request();
    tokio::time::timeout(Duration::from_secs(2), fixture.probe.fs_entered.notified()).await?;
    // This is a current-thread runtime: this timer cannot progress if traversal
    // is executed on the reactor. The blocking probe holds the actual scan permit.
    tokio::time::sleep(Duration::from_millis(5)).await;
    let error = tokio::time::timeout(Duration::from_secs(1), request)
        .await??
        .unwrap_err();
    assert!(catalog_unavailable(&error));
    assert!(
        !catalog_source_fault(&error),
        "deadline poisoned source health"
    );
    assert_eq!(
        catalog_work::SCANNER.available_permits(),
        0,
        "timed-out blocking work lost its permit"
    );
    assert!(
        fixture.probe.operations.lock().unwrap().is_empty(),
        "scan continued past a paused operation"
    );
    *fixture.probe.fs_release.0.lock().unwrap() = true;
    fixture.probe.fs_release.1.notify_all();
    tokio::time::timeout(Duration::from_secs(2), async {
        while catalog_work::SCANNER.available_permits() == 0 {
            tokio::task::yield_now().await;
        }
    })
    .await?;
    assert!(
        fixture.probe.operations.lock().unwrap().is_empty(),
        "cancelled orphan performed more filesystem operations"
    );
    eprintln!(
        "synthetic paused filesystem: reactor timer progressed; response unavailable at test100ms deadline; permit retained until worker exited; 0 post-cancel FS operations"
    );
    Ok(())
}
