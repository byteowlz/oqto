#![cfg(unix)]

use oq_permissions::*;
use serde_json::json;
use tempfile::TempDir;

fn binding() -> Binding {
    Binding {
        account_id: "owner".into(),
        principal_id: "principal".into(),
        workspace_id: "workspace".into(),
        work_directory_id: "workdir".into(),
        session_id: "public-session".into(),
        audience: "owner-only-v1".into(),
    }
}
fn policy() -> Policy {
    Policy {
        paths: vec![],
        revision: "policy-v1".into(),
        rules: vec![
            Rule {
                target: target(Capability::HistorySearch, "own-history"),
                mode: RuleMode::Allow,
            },
            Rule {
                target: target(Capability::HistorySearch, "authorized-account-history"),
                mode: RuleMode::Ask,
            },
            Rule {
                target: target(Capability::HistorySearch, "other-account-history"),
                mode: RuleMode::Deny,
            },
            Rule {
                target: target(Capability::MemoryRead, "project-memory"),
                mode: RuleMode::Allow,
            },
            Rule {
                target: target(Capability::MemoryWrite, "project-memory"),
                mode: RuleMode::Ask,
            },
            Rule {
                target: target(Capability::MemoryRead, "personal-memory"),
                mode: RuleMode::Ask,
            },
            Rule {
                target: target(Capability::IssueRead, "project-issues"),
                mode: RuleMode::Allow,
            },
            Rule {
                target: target(Capability::IssueUpdate, "project-issues"),
                mode: RuleMode::Ask,
            },
            Rule {
                target: target(Capability::FileRead, "extra-directory"),
                mode: RuleMode::Ask,
            },
        ],
    }
}
fn target(capability: Capability, resource: &str) -> Target {
    Target {
        capability,
        resource_id: resource.into(),
    }
}
fn op(id: &str, capability: Capability, resource: &str) -> Operation {
    Operation {
        request_id: id.into(),
        target: target(capability, resource),
        arguments: json!({"query":"token"}),
    }
}
fn private_dir() -> TempDir {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    dir
}

async fn setup() -> (TempDir, Authority) {
    let dir = private_dir();
    let authority = Authority::open(&dir.path().join("permissions.sqlite"), binding(), policy())
        .await
        .unwrap();
    (dir, authority)
}
async fn pending(a: &Authority, operation: &Operation) -> Request {
    match a.admit(operation.clone(), 100).await.unwrap() {
        Admission::Pending(request) => *request,
        other => panic!("expected pending, got {other:?}"),
    }
}
async fn approve(a: &Authority, request: &Request, lifetime: GrantLifetime) -> Request {
    a.decide(
        "owner",
        &request.operation.request_id,
        &request.digest,
        request.revision,
        lifetime,
        true,
        1000,
        101,
    )
    .await
    .unwrap()
}
async fn permit(a: &Authority, operation: &Operation, now: u64) -> Permit {
    match a.admit(operation.clone(), now).await.unwrap() {
        Admission::Allowed(p) => p,
        other => panic!("expected allowed, got {other:?}"),
    }
}

#[tokio::test]
async fn own_history_allowed_broader_history_waits_without_provider_access() {
    let (_dir, a) = setup().await;
    let own = op("own", Capability::HistorySearch, "own-history");
    let p = permit(&a, &own, 100).await;
    a.check(&p, &own, 100).await.unwrap();
    let global = op(
        "broader",
        Capability::HistorySearch,
        "authorized-account-history",
    );
    let request = pending(&a, &global).await;
    assert_eq!(request.binding, binding());
    assert_eq!(request.policy_revision, "policy-v1");
    approve(&a, &request, GrantLifetime::Once).await;
    let p = permit(&a, &global, 102).await;
    a.check(&p, &global, 103).await.unwrap();
}

#[tokio::test]
async fn baseline_also_binds_exact_arguments_and_stable_request_identity() {
    let (_dir, a) = setup().await;
    let own = op("baseline", Capability::HistorySearch, "own-history");
    let p = permit(&a, &own, 100).await;
    let mut changed = own.clone();
    changed.arguments = json!({"query":"different"});
    assert!(a.admit(changed.clone(), 101).await.is_err());
    assert!(a.check(&p, &changed, 101).await.is_err());
    assert!(matches!(
        a.admit(own, 101).await.unwrap(),
        Admission::Allowed(_)
    ));
}

#[tokio::test]
async fn denial_and_unknown_resources_never_enter_provider() {
    let (_dir, a) = setup().await;
    let mut reads = 0;
    for operation in [
        op("hard", Capability::HistorySearch, "other-account-history"),
        op("unknown", Capability::Execute, "unregistered-shell"),
        op(
            "ask",
            Capability::HistorySearch,
            "authorized-account-history",
        ),
    ] {
        if let Admission::Allowed(p) = a.admit(operation.clone(), 100).await.unwrap() {
            a.check(&p, &operation, 100).await.unwrap();
            reads += 1;
        }
    }
    assert_eq!(reads, 0);
    assert!(a.inspect("hard").await.is_err());
}

#[tokio::test]
async fn once_does_not_authorize_another_logical_operation() {
    let (_dir, a) = setup().await;
    let first = op(
        "one",
        Capability::HistorySearch,
        "authorized-account-history",
    );
    approve(&a, &pending(&a, &first).await, GrantLifetime::Once).await;
    permit(&a, &first, 102).await;
    let second = op(
        "two",
        Capability::HistorySearch,
        "authorized-account-history",
    );
    assert!(matches!(
        a.admit(second, 102).await.unwrap(),
        Admission::Pending(_)
    ));
}

#[tokio::test]
async fn session_grant_is_target_and_operation_specific_not_all_tools() {
    let (_dir, a) = setup().await;
    let first = op(
        "one",
        Capability::HistorySearch,
        "authorized-account-history",
    );
    approve(&a, &pending(&a, &first).await, GrantLifetime::Session).await;
    permit(
        &a,
        &op(
            "two",
            Capability::HistorySearch,
            "authorized-account-history",
        ),
        102,
    )
    .await;
    assert!(matches!(
        a.admit(op("memory", Capability::MemoryRead, "personal-memory"), 102)
            .await
            .unwrap(),
        Admission::Pending(_)
    ));
    assert!(matches!(
        a.admit(
            op(
                "foreign",
                Capability::HistorySearch,
                "other-account-history"
            ),
            102
        )
        .await
        .unwrap(),
        Admission::Denied
    ));
}

#[tokio::test]
async fn memories_and_issues_use_same_gate_but_read_does_not_grant_write() {
    let (_dir, a) = setup().await;
    for (read, write, resource) in [
        (
            Capability::MemoryRead,
            Capability::MemoryWrite,
            "project-memory",
        ),
        (
            Capability::IssueRead,
            Capability::IssueUpdate,
            "project-issues",
        ),
    ] {
        permit(&a, &op(&format!("read-{resource}"), read, resource), 100).await;
        let operation = op(&format!("write-{resource}"), write, resource);
        let request = pending(&a, &operation).await;
        approve(&a, &request, GrantLifetime::Once).await;
        permit(&a, &operation, 102).await;
    }
}

#[tokio::test]
async fn pending_and_approved_authority_survive_reopen() {
    let (dir, a) = setup().await;
    let operation = op(
        "native-store:task-42",
        Capability::HistorySearch,
        "authorized-account-history",
    );
    let original = pending(&a, &operation).await;
    a.close().await;
    let b = Authority::open(&dir.path().join("permissions.sqlite"), binding(), policy())
        .await
        .unwrap();
    let restored = pending(&b, &operation).await;
    assert_eq!(original.digest, restored.digest);
    assert_eq!(original.revision, restored.revision);
    approve(&b, &restored, GrantLifetime::Once).await;
    b.close().await;
    let c = Authority::open(&dir.path().join("permissions.sqlite"), binding(), policy())
        .await
        .unwrap();
    permit(&c, &operation, 102).await;
}

#[tokio::test]
async fn two_connections_only_one_approval_wins() {
    let (dir, a) = setup().await;
    let b = Authority::open(&dir.path().join("permissions.sqlite"), binding(), policy())
        .await
        .unwrap();
    let operation = op(
        "cas",
        Capability::HistorySearch,
        "authorized-account-history",
    );
    let r = pending(&a, &operation).await;
    let (one, two) = tokio::join!(
        a.decide(
            "owner",
            &operation.request_id,
            &r.digest,
            r.revision,
            GrantLifetime::Once,
            true,
            1000,
            101
        ),
        b.decide(
            "owner",
            &operation.request_id,
            &r.digest,
            r.revision,
            GrantLifetime::Once,
            false,
            1000,
            101
        )
    );
    assert_ne!(one.is_ok(), two.is_ok());
    assert_eq!(a.inspect(&operation.request_id).await.unwrap().revision, 2);
}

#[tokio::test]
async fn wrong_operator_digest_or_revision_cannot_approve() {
    let (_dir, a) = setup().await;
    let operation = op("review", Capability::MemoryRead, "personal-memory");
    let r = pending(&a, &operation).await;
    for (actor, hash, revision) in [
        ("other", r.digest.as_str(), 1),
        ("owner", "wrong", 1),
        ("owner", r.digest.as_str(), 2),
    ] {
        assert!(
            a.decide(
                actor,
                &operation.request_id,
                hash,
                revision,
                GrantLifetime::Once,
                true,
                1000,
                101
            )
            .await
            .is_err()
        );
    }
    assert_eq!(
        a.inspect(&operation.request_id).await.unwrap().state,
        RequestState::Pending
    );
    approve(&a, &r, GrantLifetime::Once).await;
}

#[tokio::test]
async fn deny_is_a_durable_disposition_not_a_repeated_prompt() {
    let (_dir, a) = setup().await;
    let operation = op(
        "deny",
        Capability::HistorySearch,
        "authorized-account-history",
    );
    let r = pending(&a, &operation).await;
    a.decide(
        "owner",
        &operation.request_id,
        &r.digest,
        1,
        GrantLifetime::Once,
        false,
        0,
        101,
    )
    .await
    .unwrap();
    assert!(matches!(
        a.admit(operation, 102).await.unwrap(),
        Admission::Denied
    ));
}

#[tokio::test]
async fn revoke_all_is_owner_only_durable_and_preserves_baseline() {
    let (directory, authority) = setup().await;
    let baseline = op("baseline", Capability::HistorySearch, "own-history");
    let Admission::Allowed(baseline_permit) = authority.admit(baseline.clone(), 100).await.unwrap()
    else {
        panic!("baseline must be allowed");
    };
    let expanded = op("expanded", Capability::FileRead, "extra-directory");
    let request = pending(&authority, &expanded).await;
    approve(&authority, &request, GrantLifetime::Session).await;
    let Admission::Allowed(permit) = authority.admit(expanded.clone(), 100).await.unwrap() else {
        panic!("approved overlay must be allowed");
    };
    assert!(authority.revoke_all("agent").await.is_err());
    authority.check(&permit, &expanded, 100).await.unwrap();
    authority.close().await;
    let reopened = Authority::open(
        &directory.path().join("permissions.sqlite"),
        binding(),
        policy(),
    )
    .await
    .unwrap();
    assert_eq!(reopened.revoke_all("owner").await.unwrap(), 1);
    assert_eq!(reopened.revoke_all("owner").await.unwrap(), 0);
    assert!(reopened.check(&permit, &expanded, 100).await.is_err());
    // Ledger revisions fence *all* in-flight permits, including baseline.
    // The immutable baseline remains available on fresh admission.
    assert!(
        reopened
            .check(&baseline_permit, &baseline, 100)
            .await
            .is_err()
    );
    let Admission::Allowed(fresh_baseline) = reopened.admit(baseline.clone(), 100).await.unwrap()
    else {
        panic!("baseline must not need approval after overlay revocation");
    };
    reopened
        .check(&fresh_baseline, &baseline, 100)
        .await
        .unwrap();
    assert!(matches!(
        reopened
            .admit(op("next", Capability::FileRead, "extra-directory"), 100)
            .await
            .unwrap(),
        Admission::Pending(_)
    ));
}

#[tokio::test]
async fn revoke_blocks_inflight_delivery_and_future_calls() {
    let (_dir, a) = setup().await;
    let operation = op("revoke", Capability::MemoryRead, "personal-memory");
    let r = approve(&a, &pending(&a, &operation).await, GrantLifetime::Session).await;
    let p = permit(&a, &operation, 102).await;
    a.check(&p, &operation, 102).await.unwrap();
    a.revoke("owner", r.grant_id.as_ref().unwrap())
        .await
        .unwrap();
    assert!(a.check(&p, &operation, 103).await.is_err());
    assert!(matches!(
        a.admit(operation, 103).await.unwrap(),
        Admission::Denied
    ));
    assert!(matches!(
        a.admit(op("next", Capability::MemoryRead, "personal-memory"), 103)
            .await
            .unwrap(),
        Admission::Pending(_)
    ));
}

#[tokio::test]
async fn expiry_cannot_be_resurrected_by_a_backward_clock() {
    let (_dir, a) = setup().await;
    let operation = op("expiry", Capability::IssueUpdate, "project-issues");
    approve(&a, &pending(&a, &operation).await, GrantLifetime::Once).await;
    let p = permit(&a, &operation, 102).await;
    assert!(a.check(&p, &operation, 1000).await.is_err());
    assert!(a.check(&p, &operation, 103).await.is_err());
    assert!(matches!(
        a.admit(operation, 103).await.unwrap(),
        Admission::Denied
    ));
}

#[tokio::test]
async fn permits_cannot_cross_sessions_audiences_or_actual_arguments() {
    let (_dir, a) = setup().await;
    let operation = op("scope", Capability::HistorySearch, "own-history");
    let p = permit(&a, &operation, 100).await;
    let other_dir = private_dir();
    let mut other = binding();
    other.session_id = "other-session".into();
    let b = Authority::open(&other_dir.path().join("ledger"), other, policy())
        .await
        .unwrap();
    assert!(b.check(&p, &operation, 100).await.is_err());
    let mut changed = operation.clone();
    changed.target.resource_id = "authorized-account-history".into();
    assert!(a.check(&p, &changed, 100).await.is_err());
}

#[tokio::test]
async fn authority_reopen_cannot_change_binding_audience_or_policy() {
    let (dir, a) = setup().await;
    a.close().await;
    let path = dir.path().join("permissions.sqlite");
    let mut foreign = binding();
    foreign.account_id = "foreign".into();
    assert!(Authority::open(&path, foreign, policy()).await.is_err());
    let mut audience = binding();
    audience.audience = "shared-with-other-account".into();
    assert!(Authority::open(&path, audience, policy()).await.is_err());
    let mut new_policy = policy();
    new_policy.rules[2].mode = RuleMode::Allow;
    assert!(Authority::open(&path, binding(), new_policy).await.is_err());
}

#[tokio::test]
async fn requests_and_policy_are_bounded_and_unknown_input_fails_closed() {
    let (_dir, a) = setup().await;
    let mut huge = op("huge", Capability::HistorySearch, "own-history");
    huge.arguments = json!({"query":"x".repeat(8192)});
    assert!(a.admit(huge, 100).await.is_err());
    let mut duplicate = policy();
    duplicate.rules.push(duplicate.rules[0].clone());
    assert!(duplicate.validate().is_err());
    assert!(serde_json::from_value::<Operation>(json!({"request_id":"x", "target":{"capability":"execute", "resource_id":"x"}, "arguments":{}, "approve":true})).is_err());
    let mut temporary = binding();
    temporary.session_id = "pending-1".into();
    let d = private_dir();
    assert!(
        Authority::open(&d.path().join("ledger"), temporary, policy())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn concurrent_first_open_has_one_bound_authority() {
    let d = private_dir();
    let path = d.path().join("ledger");
    let (a, b) = tokio::join!(
        Authority::open(&path, binding(), policy()),
        Authority::open(&path, binding(), policy())
    );
    let a = a.unwrap();
    let b = b.unwrap();
    let operation = op(
        "concurrent",
        Capability::HistorySearch,
        "authorized-account-history",
    );
    let first = pending(&a, &operation).await;
    assert_eq!(pending(&b, &operation).await.digest, first.digest);
}

#[tokio::test]
async fn ledger_refuses_world_readable_directories_and_symlinks() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    let d = private_dir();
    std::fs::set_permissions(d.path(), std::fs::Permissions::from_mode(0o755)).unwrap();
    assert!(
        Authority::open(&d.path().join("ledger"), binding(), policy())
            .await
            .is_err()
    );
    std::fs::set_permissions(d.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    let source = d.path().join("user-file");
    std::fs::write(&source, "preserve me").unwrap();
    symlink(&source, d.path().join("ledger")).unwrap();
    assert!(
        Authority::open(&d.path().join("ledger"), binding(), policy())
            .await
            .is_err()
    );
    assert_eq!(std::fs::read_to_string(source).unwrap(), "preserve me");
}

#[tokio::test]
async fn object_key_order_is_not_a_new_request() {
    let (_d, a) = setup().await;
    let mut operation = op("canonical", Capability::HistorySearch, "own-history");
    operation.arguments = serde_json::from_str(r#"{"a":1,"b":{"c":2,"d":3}}"#).unwrap();
    permit(&a, &operation, 100).await;
    operation.arguments = serde_json::from_str(r#"{"b":{"d":3,"c":2},"a":1}"#).unwrap();
    permit(&a, &operation, 101).await;
}

#[tokio::test]
async fn session_grant_resolves_already_pending_sibling_requests() {
    let (_d, a) = setup().await;
    let one = op(
        "first-pending",
        Capability::HistorySearch,
        "authorized-account-history",
    );
    let two = op(
        "second-pending",
        Capability::HistorySearch,
        "authorized-account-history",
    );
    let r1 = pending(&a, &one).await;
    let r2 = pending(&a, &two).await;
    approve(&a, &r1, GrantLifetime::Session).await;
    permit(&a, &two, 102).await;
    assert_eq!(
        a.inspect(&two.request_id).await.unwrap().state,
        RequestState::Approved
    );
    assert!(
        a.decide(
            "owner",
            &two.request_id,
            &r2.digest,
            r2.revision,
            GrantLifetime::Once,
            true,
            1000,
            103
        )
        .await
        .is_err()
    );
}

#[tokio::test]
async fn overlapping_grants_do_not_invalidate_a_still_active_once_approval() {
    let (_d, a) = setup().await;
    let one = op(
        "once-existing",
        Capability::HistorySearch,
        "authorized-account-history",
    );
    let two = op(
        "session-new",
        Capability::HistorySearch,
        "authorized-account-history",
    );
    approve(&a, &pending(&a, &one).await, GrantLifetime::Once).await;
    let r = pending(&a, &two).await;
    a.decide(
        "owner",
        &two.request_id,
        &r.digest,
        r.revision,
        GrantLifetime::Session,
        true,
        2000,
        101,
    )
    .await
    .unwrap();
    permit(&a, &one, 102).await;
}

#[tokio::test]
async fn identical_bindings_in_different_ledgers_cannot_replay_permits_or_reviews() {
    let (_d1, a) = setup().await;
    let (_d2, b) = setup().await;
    let own = op("same-native-id", Capability::HistorySearch, "own-history");
    let p = permit(&a, &own, 100).await;
    assert!(b.check(&p, &own, 100).await.is_err());
    let global = op(
        "same-pending-id",
        Capability::HistorySearch,
        "authorized-account-history",
    );
    let old = pending(&a, &global).await;
    let current = pending(&b, &global).await;
    assert_ne!(old.digest, current.digest);
    assert!(
        b.decide(
            "owner",
            &global.request_id,
            &old.digest,
            1,
            GrantLifetime::Once,
            true,
            1000,
            101
        )
        .await
        .is_err()
    );
}

#[test]
fn executor_generations_fail_closed_until_confirmed_install() {
    let mut fence = ExecutorFence::default();
    assert!(fence.admit(0).is_err());
    assert_eq!(fence.install_after_confirmed_stop(0).unwrap(), 1);
    fence.admit(1).unwrap();
    assert!(fence.install_after_confirmed_stop(1).is_err());
    fence.freeze();
    assert!(fence.admit(1).is_err());
    assert!(fence.install_after_confirmed_stop(0).is_err());
    assert_eq!(fence.install_after_confirmed_stop(1).unwrap(), 2);
    assert!(fence.admit(1).is_err());
    fence.admit(2).unwrap();
}

#[tokio::test]
async fn owner_death_marks_live_invocations_interrupted_and_fences_session_grants() {
    let (dir, a) = setup().await;
    let first = op("store:task-1", Capability::MemoryWrite, "project-memory");
    let request = pending(&a, &first).await;
    approve(&a, &request, GrantLifetime::Session).await;
    let p = permit(&a, &first, 102).await;
    a.begin(&p, &first, 103).await.unwrap();
    // Re-entry of the same logical invocation is refused, not double-run.
    assert!(a.begin(&p, &first, 103).await.is_err());
    let finished = op("store:task-2", Capability::MemoryWrite, "project-memory");
    let q = permit(&a, &finished, 104).await;
    a.begin(&q, &finished, 104).await.unwrap();
    a.end("store:task-2").await.unwrap();
    // Owner dies without `end` for task-1.
    a.close().await;
    let b = Authority::open(&dir.path().join("permissions.sqlite"), binding(), policy())
        .await
        .unwrap();
    assert!(b.recover("intruder").await.is_err());
    let interrupted = b.recover("owner").await.unwrap();
    assert_eq!(interrupted.len(), 1);
    assert_eq!(interrupted[0].operation, first);
    assert_eq!(interrupted[0].started_at, 103);
    // The still-unexpired Session grant no longer covers a retry.
    let retry = op("store:task-3", Capability::MemoryWrite, "project-memory");
    pending(&b, &retry).await;
    // Old permits are generation-fenced.
    assert!(b.check(&p, &first, 105).await.is_err());
    // Recovery is idempotent and reporting persists until acknowledged.
    assert_eq!(b.recover("owner").await.unwrap().len(), 1);
    assert!(b.acknowledge_interrupted("intruder").await.is_err());
    assert_eq!(b.acknowledge_interrupted("owner").await.unwrap(), 1);
    assert!(b.recover("owner").await.unwrap().is_empty());
}

#[tokio::test]
async fn operator_journal_is_bound_deduplicated_and_recovered_without_replay() {
    let (_dir, a) = setup().await;
    let id = format!("{}:operator:effect-1", binding().session_id);
    let effect = op(&id, Capability::Execute, "operator-shell");
    assert!(a.begin_operator("intruder", &effect, 100).await.is_err());
    let wrong = op(
        "other:operator:effect",
        Capability::Execute,
        "operator-shell",
    );
    assert!(a.begin_operator("owner", &wrong, 100).await.is_err());
    a.begin_operator("owner", &effect, 101).await.unwrap();
    assert!(a.begin_operator("owner", &effect, 102).await.is_err());
    let interrupted = a.recover("owner").await.unwrap();
    assert_eq!(interrupted.len(), 1);
    assert_eq!(interrupted[0].operation, effect);
    assert_eq!(interrupted[0].started_at, 101);
    assert_eq!(a.recover("owner").await.unwrap().len(), 1);
}

#[tokio::test]
async fn clean_recovery_keeps_active_grants() {
    let (_dir, a) = setup().await;
    let first = op("store:task-1", Capability::MemoryWrite, "project-memory");
    let request = pending(&a, &first).await;
    approve(&a, &request, GrantLifetime::Session).await;
    assert!(a.recover("owner").await.unwrap().is_empty());
    permit(
        &a,
        &op("store:task-2", Capability::MemoryWrite, "project-memory"),
        103,
    )
    .await;
}
