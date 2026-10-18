#![cfg(unix)]

use anyhow::Result;
use oq_permissions::{Authority, Binding, Capability, Operation, Policy, Target};
use serde_json::json;

fn binding() -> Binding {
    Binding {
        account_id: "owner".into(),
        principal_id: "principal".into(),
        workspace_id: "workspace".into(),
        work_directory_id: "workdir".into(),
        session_id: "receipt-session".into(),
        audience: "owner-only".into(),
    }
}
fn policy() -> Policy {
    Policy {
        revision: "receipt-v1".into(),
        paths: vec![],
        rules: vec![],
    }
}
fn private_directory() -> Result<tempfile::TempDir> {
    use std::os::unix::fs::PermissionsExt;
    let directory = tempfile::tempdir()?;
    std::fs::set_permissions(directory.path(), std::fs::Permissions::from_mode(0o700))?;
    Ok(directory)
}
fn operation(number: usize) -> Operation {
    Operation {
        request_id: format!("receipt-session:operator:{number}"),
        target: Target {
            capability: Capability::Execute,
            resource_id: "operator-shell".into(),
        },
        arguments: json!({"tool":"bash", "input":{"command":"true"}}),
    }
}
const DIGEST: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

#[tokio::test]
async fn native_ack_tombstone_survives_reopen_and_prevents_effect_reentry() -> Result<()> {
    let directory = private_directory()?;
    let path = directory.path().join("permissions.sqlite");
    let authority = Authority::open(&path, binding(), policy()).await?;
    let op = operation(1);
    authority.begin_operator("owner", &op, 1).await?;
    assert!(
        authority.end(&op.request_id).await.is_err(),
        "generic model finish must not clear operator evidence"
    );
    authority
        .seal_operator_result("owner", &op.request_id, DIGEST)
        .await?;
    assert!(
        authority
            .complete_operator_result("other", &op.request_id, DIGEST)
            .await
            .is_err()
    );
    assert!(
        authority
            .complete_operator_result("owner", &op.request_id, &"b".repeat(64))
            .await
            .is_err()
    );
    authority
        .complete_operator_result("owner", &op.request_id, DIGEST)
        .await?;
    drop(authority);
    let reopened = Authority::open(&path, binding(), policy()).await?;
    assert!(reopened.recover("owner").await?.is_empty());
    reopened
        .complete_operator_result("owner", &op.request_id, DIGEST)
        .await?;
    assert!(
        reopened.begin_operator("owner", &op, 2).await.is_err(),
        "completed IDs are never fresh effect authority"
    );
    Ok(())
}

#[tokio::test]
async fn lost_native_ack_keeps_interruption_until_exact_sealed_receipt_reconciles() -> Result<()> {
    let directory = private_directory()?;
    let path = directory.path().join("permissions.sqlite");
    let authority = Authority::open(&path, binding(), policy()).await?;
    let op = operation(2);
    authority.begin_operator("owner", &op, 1).await?;
    authority
        .seal_operator_result("owner", &op.request_id, DIGEST)
        .await?;
    assert!(
        authority
            .seal_operator_result("owner", &op.request_id, &"b".repeat(64))
            .await
            .is_err()
    );
    drop(authority);
    let reopened = Authority::open(&path, binding(), policy()).await?;
    let interrupted = reopened.recover("owner").await?;
    assert_eq!(interrupted.len(), 1);
    assert_eq!(interrupted[0].operation.request_id, op.request_id);
    assert!(
        reopened
            .complete_operator_result("owner", &op.request_id, &"b".repeat(64))
            .await
            .is_err()
    );
    assert_eq!(reopened.recover("owner").await?.len(), 1);
    reopened
        .complete_operator_result("owner", &op.request_id, DIGEST)
        .await?;
    assert!(reopened.recover("owner").await?.is_empty());
    assert!(
        reopened
            .complete_operator_result("owner", "receipt-session:operator:unknown", DIGEST)
            .await
            .is_err()
    );
    Ok(())
}

#[tokio::test]
async fn bounded_receipt_capacity_denies_before_new_effect_admission() -> Result<()> {
    let directory = private_directory()?;
    let authority = Authority::open(
        &directory.path().join("permissions.sqlite"),
        binding(),
        policy(),
    )
    .await?;
    for index in 0..128 {
        let op = operation(index);
        authority.begin_operator("owner", &op, 1).await?;
        authority
            .seal_operator_result("owner", &op.request_id, DIGEST)
            .await?;
        authority
            .complete_operator_result("owner", &op.request_id, DIGEST)
            .await?;
    }
    assert!(
        authority
            .begin_operator("owner", &operation(128), 2)
            .await
            .is_err()
    );
    Ok(())
}
