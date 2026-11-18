use super::*;

#[tokio::test]
async fn atomic_rename_before_read_is_allowed_but_snapshot_replacement_is_not() -> Result<()> {
    let _serial = SERIAL.lock().await;
    let fixture = Fixture::new().await?;
    let target = fixture.sidecar(0);
    let temporary = target.with_extension("new");
    std::fs::write(
        &temporary,
        serde_json::to_vec(&fixture.envelope(0, vec![fixture.record(0, 2)]))?,
    )?;
    std::fs::rename(&temporary, &target)?;
    fixture.expect(&fixture.output().await, 2, 0);
    let bytes = std::fs::read(&target)?;
    let replaced = target.clone();
    hook(move || {
        std::fs::write(&temporary, bytes)?;
        std::fs::rename(temporary, replaced)?;
        Ok(())
    });
    empty(&fixture.output().await);
    fixture.expect(&fixture.output().await, 2, 0);
    Ok(())
}

#[tokio::test]
async fn late_appearing_conflicting_author_and_in_place_mutation_discard_entire_batch() -> Result<()>
{
    let _serial = SERIAL.lock().await;
    let fixture = Fixture::new().await?;
    fixture.write(0, vec![fixture.record(0, 2)])?;
    let target = fixture.sidecar(1);
    let bytes = serde_json::to_vec(&fixture.envelope(1, vec![fixture.record(1, 2)]))?;
    hook(move || {
        std::fs::write(target, bytes)?;
        Ok(())
    });
    empty(&fixture.output().await); // missing-entry snapshot is part of the commit fence
    std::fs::remove_file(fixture.sidecar(1))?;
    let target = fixture.sidecar(0);
    hook(move || {
        std::fs::write(target, b"changed")?;
        Ok(())
    });
    empty(&fixture.output().await);
    Ok(())
}

#[tokio::test]
async fn native_replacement_during_optional_read_cannot_reuse_old_author_pin() -> Result<()> {
    let _serial = SERIAL.lock().await;
    let fixture = Fixture::new().await?;
    fixture.write(0, vec![fixture.record(0, 2)])?;
    let target = fixture.bindings[0].session_file.clone();
    let bytes = std::fs::read(&target)?;
    hook(move || {
        let replacement = target.with_extension("changed");
        std::fs::write(&replacement, bytes)?;
        std::fs::rename(replacement, target)?;
        Ok(())
    });
    empty(&fixture.output().await);
    Ok(())
}

#[tokio::test]
async fn cancelled_blocking_validation_keeps_global_permit_until_worker_retires() -> Result<()> {
    let _serial = SERIAL.lock().await;
    let fixture = Fixture::new().await?;
    fixture.write(0, vec![fixture.record(0, 2)])?;
    let (entered, resume) = fixture.source.pause_next_header_for_test()?;
    let source = fixture.source.clone();
    let bindings = fixture.bindings.clone();
    let data = fixture.data.clone();
    let task = tokio::spawn(async move {
        read(
            &data,
            Context {
                source: &source,
                bindings: &bindings,
            },
        )
        .await
    });
    tokio::time::timeout(Duration::from_secs(2), entered).await??;
    task.abort();
    assert!(matches!(task.await, Err(error) if error.is_cancelled()));
    assert_eq!(WORKER.available_permits(), 0);
    empty(&fixture.output().await); // busy => no new worker, not another scan
    resume.send(())?;
    tokio::time::timeout(Duration::from_secs(2), async {
        while WORKER.available_permits() != 1 {
            tokio::task::yield_now().await;
        }
    })
    .await?;
    fixture.expect(&fixture.output().await, 2, 0);
    Ok(())
}
