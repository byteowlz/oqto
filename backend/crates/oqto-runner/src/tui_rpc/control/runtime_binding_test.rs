//! Real disposable Unix peers and existing host/lease gates; no actual Pi or effect.
use super::*;
use oqto_protocol::runtime_interactions::{RuntimeOperation, RuntimeRequest};

fn dispatch() -> Result<RuntimeOperation> {
    let request: RuntimeRequest = serde_json::from_value(json!({
        "version":1,"requestId":"request-fixture","operation":{
            "op":"command_dispatch","catalogGeneration":"catalog-fixture",
            "commandId":"command-fixture","args":"literal /not-a-prompt"
        }
    }))?;
    Ok(request.operation)
}

fn epoch(fixture: &Fixture, generation: Value) {
    *fixture.source_epoch.lock().unwrap() = Some(generation);
}

async fn source_runtime(generation: Value) -> Result<(Fixture, TuiObserver)> {
    let fixture = Fixture::new(true, Some(true))?;
    epoch(&fixture, generation);
    let observer = fixture.observer().await?;
    Ok((fixture, observer))
}

#[tokio::test]
async fn binding_retains_source_epoch_and_host_owned_public_native_mapping() -> Result<()> {
    for generation in [json!(17), json!("source-runtime-17")] {
        let (fixture, mut observer) = source_runtime(generation.clone()).await?;
        let binding = observer
            .runtime_binding_for(&RuntimeOperation::CommandCatalog)
            .await?;
        let value = serde_json::to_value(binding)?;
        assert_eq!(value["publicSessionId"], "public-a");
        assert_eq!(value["nativeSessionId"], "native-a");
        assert_eq!(value["runtimeGeneration"], generation);
        assert!(value["controllerId"].is_null());
        assert!(value["controllerGeneration"].is_null());
        assert_eq!(fixture.actions.load(Ordering::SeqCst), 0);
        assert_eq!(
            std::fs::read(&fixture.gate.binding.session_file)?,
            fixture.sentinel
        );
    }
    Ok(())
}

#[tokio::test]
async fn legacy_source_does_not_turn_grant_generation_or_pid_into_runtime_epoch() -> Result<()> {
    let fixture = Fixture::new(true, Some(true))?;
    let mut observer = fixture.observer().await?;
    assert!(
        observer
            .runtime_binding_for(&RuntimeOperation::Capabilities)
            .await
            .is_err()
    );
    assert!(
        observer.reader.is_some(),
        "legacy observation remains usable"
    );
    Ok(())
}

#[tokio::test]
async fn missing_and_revoked_control_deny_before_native_io() -> Result<()> {
    let (fixture, mut observer) = source_runtime(json!(17)).await?;
    let before = fixture.state_reads.load(Ordering::SeqCst);
    assert!(observer.runtime_binding_for(&dispatch()?).await.is_err());
    assert_eq!(fixture.state_reads.load(Ordering::SeqCst), before);
    observer
        .acquire_input(fixture.gate.clone(), "device-a")
        .await?;
    fixture.gate.control_live.store(false, Ordering::SeqCst);
    assert!(observer.runtime_binding_for(&dispatch()?).await.is_err());
    assert_eq!(fixture.state_reads.load(Ordering::SeqCst), before);
    assert_eq!(fixture.actions.load(Ordering::SeqCst), 0);
    Ok(())
}

#[tokio::test]
async fn controlled_binding_uses_current_permit_without_performing_an_effect() -> Result<()> {
    let (fixture, mut observer) = source_runtime(json!(17)).await?;
    observer
        .acquire_input(fixture.gate.clone(), "device-a")
        .await?;
    let binding = serde_json::to_value(observer.runtime_binding_for(&dispatch()?).await?)?;
    assert_eq!(binding["controllerId"], "device-a");
    assert_eq!(binding["controllerGeneration"], "g1");
    assert_eq!(binding["runtimeGeneration"], 17);
    assert_eq!(fixture.actions.load(Ordering::SeqCst), 0);
    Ok(())
}

#[tokio::test]
async fn session_control_is_not_semantic_permission_or_owner_write_authority() -> Result<()> {
    let (fixture, mut observer) = source_runtime(json!(17)).await?;
    observer
        .acquire_input(fixture.gate.clone(), "device-a")
        .await?;
    let before = fixture.state_reads.load(Ordering::SeqCst);
    for operation in [
        RuntimeOperation::PermissionSnapshot,
        RuntimeOperation::PolicyRead,
    ] {
        assert!(observer.runtime_binding_for(&operation).await.is_err());
    }
    assert_eq!(fixture.state_reads.load(Ordering::SeqCst), before);
    Ok(())
}

#[tokio::test]
async fn epoch_change_or_disappearance_poison_attachment_without_reconnect() -> Result<()> {
    for next in [Some(json!(18)), None] {
        let (fixture, mut observer) = source_runtime(json!(17)).await?;
        *fixture.source_epoch.lock().unwrap() = next;
        assert!(
            observer
                .runtime_binding_for(&RuntimeOperation::Capabilities)
                .await
                .is_err()
        );
        assert!(observer.reader.is_none());
        let reads = fixture.state_reads.load(Ordering::SeqCst);
        assert!(
            observer
                .runtime_binding_for(&RuntimeOperation::Capabilities)
                .await
                .is_err()
        );
        assert_eq!(fixture.state_reads.load(Ordering::SeqCst), reads);
    }
    Ok(())
}

#[tokio::test]
async fn revocation_before_and_during_native_refresh_never_returns_a_binding() -> Result<()> {
    for during in [false, true] {
        let (fixture, mut observer) = source_runtime(json!(17)).await?;
        if during {
            fixture.revoke_on_state.store(true, Ordering::SeqCst);
        } else {
            fixture.gate.live.store(false, Ordering::SeqCst);
        }
        assert!(
            observer
                .runtime_binding_for(&RuntimeOperation::Capabilities)
                .await
                .is_err()
        );
        assert_eq!(
            fixture.state_reads.load(Ordering::SeqCst),
            if during { 2 } else { 1 }
        );
    }
    Ok(())
}

#[tokio::test]
async fn malformed_source_epoch_is_not_adopted() -> Result<()> {
    for generation in [json!("not an id"), json!(9007199254740992_u64), json!(-1)] {
        let fixture = Fixture::new(true, Some(true))?;
        epoch(&fixture, generation);
        assert!(fixture.observer().await.is_err());
    }
    Ok(())
}
