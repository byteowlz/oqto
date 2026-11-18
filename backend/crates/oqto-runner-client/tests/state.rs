//! Pure state behaviour: fences, correlation, expiry and unknown-is-not-empty.

use oqto_runner_client::contract::{
    RuntimeArgumentContract, RuntimeAvailability, RuntimeBinding, RuntimeCatalogCoverage,
    RuntimeCommandCatalog, RuntimeCommandDescriptor, RuntimeCommandOrigin, RuntimeCursor,
    RuntimeDigest, RuntimeEffectState, RuntimeError, RuntimeErrorSource, RuntimeEvent,
    RuntimeEventPayload, RuntimeFamily, RuntimeGeneration, RuntimeId, RuntimeOperation,
    RuntimePermissionDecision, RuntimePermissionOption, RuntimePermissionOutcome,
    RuntimePermissionPresentation, RuntimePermissionReceipt, RuntimePermissionReceiptKind,
    RuntimePermissionSnapshot, RuntimePermissionSource, RuntimeQueueAction, RuntimeQueueActionKind,
    RuntimeQueueCompleteness, RuntimeQueueItem, RuntimeQueueSnapshot, RuntimeRequest,
    RuntimeResponse, RuntimeResult, RuntimeRevision, RuntimeSpawnSubject,
};
use oqto_runner_client::{ClientError, InteractionSession};

fn id(text: &str) -> RuntimeId {
    RuntimeId::try_from(text.to_owned()).expect("valid id")
}

fn revision(value: u64) -> RuntimeRevision {
    RuntimeRevision::try_from(value).expect("safe revision")
}

fn digest(byte: char) -> RuntimeDigest {
    RuntimeDigest::try_from(byte.to_string().repeat(64)).expect("digest")
}

fn binding() -> RuntimeBinding {
    RuntimeBinding {
        public_session_id: id("session-1"),
        native_session_id: id("native-1"),
        runtime_generation: RuntimeGeneration::Id(id("runtime-1")),
        controller_id: Some(id("device-1")),
        controller_generation: Some(RuntimeGeneration::Id(id("grant-1"))),
    }
}

fn descriptor(command: &str, availability: RuntimeAvailability) -> RuntimeCommandDescriptor {
    RuntimeCommandDescriptor {
        command_id: id(command),
        invocation: format!("/{command}"),
        description: None,
        origin: RuntimeCommandOrigin::Extension,
        arguments: RuntimeArgumentContract::RawString,
        availability,
        required_interactions: vec![RuntimeFamily::Commands],
    }
}

fn catalog(generation: &str, commands: Vec<RuntimeCommandDescriptor>) -> RuntimeCommandCatalog {
    RuntimeCommandCatalog {
        generation: RuntimeGeneration::Id(id(generation)),
        coverage: RuntimeCatalogCoverage::RegisteredOnly,
        commands,
    }
}

fn queue_snapshot(
    revision_value: u64,
    completeness: RuntimeQueueCompleteness,
    rows: usize,
    supported: Vec<RuntimeQueueActionKind>,
) -> RuntimeQueueSnapshot {
    let items = |prefix: &str| {
        (0..rows)
            .map(|n| RuntimeQueueItem {
                item_id: Some(id(&format!("{prefix}-{n}"))),
                text: format!("prompt {n}"),
            })
            .collect::<Vec<_>>()
    };
    RuntimeQueueSnapshot {
        cursor: RuntimeCursor {
            generation: RuntimeGeneration::Id(id("runtime-1")),
            revision: revision(revision_value),
        },
        completeness,
        steering: items("steer"),
        follow_up: Vec::new(),
        supported_actions: supported,
    }
}

fn presentation(handle: &str, expires_at: u64) -> RuntimePermissionPresentation {
    let subject = RuntimeSpawnSubject {
        kind: "pi".into(),
        model: "model-1".into(),
        loadout: None,
        effect_digest: digest('a'),
    };
    RuntimePermissionPresentation {
        handle_id: id(handle),
        request_id: id("request-1"),
        request_revision: revision(0),
        subject: Some(subject),
        subject_digest: digest('b'),
        operation_digest: digest('c'),
        ceiling_digest: digest('d'),
        expires_at_ms: revision(expires_at),
        options: vec![RuntimePermissionOption {
            option_id: id("allow"),
            model: None,
            effect_digest: digest('e'),
        }],
    }
}

fn response(request: &RuntimeRequest, result: RuntimeResult) -> RuntimeResponse {
    RuntimeResponse {
        version: request.version,
        request_id: request.request_id.clone(),
        binding: binding(),
        result,
    }
}

fn answer(session: &mut InteractionSession, request: &RuntimeRequest, result: RuntimeResult) {
    session
        .apply(response(request, result))
        .expect("correlated response");
}

/// Establish a session with a catalog and a queue snapshot.
fn primed() -> InteractionSession {
    let mut session = InteractionSession::new();
    let request = session.catalog_request().expect("catalog request");
    answer(
        &mut session,
        &request,
        RuntimeResult::Catalog {
            catalog: catalog(
                "catalog-1",
                vec![
                    descriptor("deploy", RuntimeAvailability::Available),
                    descriptor(
                        "legacy",
                        RuntimeAvailability::Unavailable {
                            reason: oqto_runner_client::contract::RuntimeUnavailableReason::SdkEngineAbsent,
                        },
                    ),
                ],
            ),
        },
    );
    let request = session.queue_snapshot_request().expect("queue request");
    answer(
        &mut session,
        &request,
        RuntimeResult::Queue {
            snapshot: queue_snapshot(
                1,
                RuntimeQueueCompleteness::Complete,
                2,
                vec![RuntimeQueueActionKind::Clear],
            ),
        },
    );
    session
}

#[test]
fn unknown_queue_state_is_not_an_empty_queue() {
    let mut session = InteractionSession::new();
    assert!(!session.queue().is_empty(), "no snapshot is not empty");
    assert!(session.queue().is_unknown());

    let request = session.queue_snapshot_request().unwrap();
    answer(
        &mut session,
        &request,
        RuntimeResult::Queue {
            snapshot: queue_snapshot(1, RuntimeQueueCompleteness::Unknown, 0, Vec::new()),
        },
    );
    assert!(session.queue().is_unknown());
    assert!(
        !session.queue().is_empty(),
        "zero rows with unknown coverage"
    );

    let request = session.queue_snapshot_request().unwrap();
    answer(
        &mut session,
        &request,
        RuntimeResult::Queue {
            snapshot: queue_snapshot(2, RuntimeQueueCompleteness::Partial, 0, Vec::new()),
        },
    );
    assert!(!session.queue().is_empty(), "partial coverage");

    let request = session.queue_snapshot_request().unwrap();
    answer(
        &mut session,
        &request,
        RuntimeResult::Queue {
            snapshot: queue_snapshot(3, RuntimeQueueCompleteness::Complete, 0, Vec::new()),
        },
    );
    assert!(session.queue().is_empty(), "complete and empty is empty");
}

#[test]
fn foreign_generation_and_stale_revision_are_refused() {
    let mut session = primed();
    let mut foreign = queue_snapshot(2, RuntimeQueueCompleteness::Complete, 0, Vec::new());
    foreign.cursor.generation = RuntimeGeneration::Id(id("other-runtime"));
    let request = session.queue_snapshot_request().unwrap();
    assert_eq!(
        session.apply(response(
            &request,
            RuntimeResult::Queue { snapshot: foreign }
        )),
        Err(ClientError::ForeignQueueGeneration)
    );

    let request = session.queue_snapshot_request().unwrap();
    assert_eq!(
        session.apply(response(
            &request,
            RuntimeResult::Queue {
                snapshot: queue_snapshot(0, RuntimeQueueCompleteness::Complete, 0, Vec::new()),
            }
        )),
        Err(ClientError::StaleQueueRevision)
    );
}

#[test]
fn dispatch_requires_a_catalog_and_a_fenced_generation() {
    let mut session = InteractionSession::new();
    assert_eq!(
        session.dispatch_request("deploy", "--now"),
        Err(ClientError::CatalogUnknown)
    );

    let mut session = primed();
    assert_eq!(
        session.dispatch_request("missing", ""),
        Err(ClientError::UnknownCommand)
    );
    assert!(matches!(
        session.dispatch_request("legacy", ""),
        Err(ClientError::CommandUnavailable(_))
    ));

    let request = session.dispatch_request("deploy", "  --now  ").unwrap();
    match &request.operation {
        RuntimeOperation::CommandDispatch {
            catalog_generation,
            command_id,
            args,
        } => {
            assert_eq!(catalog_generation, &RuntimeGeneration::Id(id("catalog-1")));
            assert_eq!(command_id, &id("deploy"));
            assert_eq!(args, "  --now  ", "arguments stay verbatim");
        }
        other => panic!("unexpected operation {other:?}"),
    }
}

#[test]
fn uncorrelated_and_mismatched_responses_keep_the_fence() {
    let mut session = primed();
    let request = session.catalog_request().unwrap();
    let mut wrong = response(
        &request,
        RuntimeResult::Capabilities {
            capabilities: Vec::new(),
        },
    );
    wrong.request_id = id("someone-else");
    assert_eq!(session.apply(wrong), Err(ClientError::UncorrelatedResponse));
    assert!(
        session.has_outstanding(),
        "the fence survives a stray response"
    );

    let mismatched = response(
        &request,
        RuntimeResult::Capabilities {
            capabilities: Vec::new(),
        },
    );
    assert_eq!(session.apply(mismatched), Err(ClientError::ResultMismatch));
    assert!(session.has_outstanding());

    session
        .apply(response(
            &request,
            RuntimeResult::Catalog {
                catalog: catalog("catalog-2", Vec::new()),
            },
        ))
        .expect("the rightful response still applies");
    assert!(!session.has_outstanding());
}

#[test]
fn only_one_request_may_be_outstanding() {
    let mut session = InteractionSession::new();
    let _request = session.catalog_request().unwrap();
    assert_eq!(
        session.queue_snapshot_request(),
        Err(ClientError::RequestOutstanding)
    );
}

#[test]
fn binding_drift_requires_readmission() {
    let mut session = primed();
    let request = session.catalog_request().unwrap();
    let mut drifted = response(
        &request,
        RuntimeResult::Catalog {
            catalog: catalog("catalog-2", Vec::new()),
        },
    );
    drifted.binding.runtime_generation = RuntimeGeneration::Id(id("runtime-2"));
    assert_eq!(session.apply(drifted), Err(ClientError::BindingChanged));
}

#[test]
fn decisions_are_fenced_by_expiry_option_and_controller() {
    let mut session = InteractionSession::new();
    let request = session.permission_snapshot_request().unwrap();
    answer(
        &mut session,
        &request,
        RuntimeResult::Permissions {
            snapshot: RuntimePermissionSnapshot {
                cursor: RuntimeCursor {
                    generation: RuntimeGeneration::Id(id("runtime-1")),
                    revision: revision(1),
                },
                active: vec![presentation("handle-1", 1_000)],
                receipts: Vec::new(),
            },
        },
    );

    assert_eq!(
        session.decide_request(
            "handle-1",
            0,
            RuntimePermissionDecision::AllowOnce,
            Some("allow"),
            1_000
        ),
        Err(ClientError::Expired)
    );
    assert_eq!(
        session.decide_request(
            "handle-1",
            0,
            RuntimePermissionDecision::AllowOnce,
            Some("not-issued"),
            10
        ),
        Err(ClientError::UnknownOption)
    );
    assert_eq!(
        session.decide_request(
            "handle-1",
            1,
            RuntimePermissionDecision::AllowOnce,
            Some("allow"),
            10
        ),
        Err(ClientError::NotActive),
        "revision must match the issued request"
    );

    let request = session
        .decide_request(
            "handle-1",
            0,
            RuntimePermissionDecision::AllowOnce,
            Some("allow"),
            10,
        )
        .expect("valid decision");
    match &request.operation {
        RuntimeOperation::PermissionDecide {
            producer_generation,
            controller_id,
            controller_generation,
            handle_id,
            decision,
            option_id,
            ..
        } => {
            assert_eq!(producer_generation, &RuntimeGeneration::Id(id("runtime-1")));
            assert_eq!(controller_id, &id("device-1"));
            assert_eq!(controller_generation, &RuntimeGeneration::Id(id("grant-1")));
            assert_eq!(handle_id, &id("handle-1"));
            assert_eq!(decision, &RuntimePermissionDecision::AllowOnce);
            assert_eq!(option_id, &Some(id("allow")));
        }
        other => panic!("unexpected operation {other:?}"),
    }
}

#[test]
fn decisions_without_a_controller_binding_are_refused() {
    let mut session = InteractionSession::new();
    let mut no_controller = binding();
    no_controller.controller_id = None;
    no_controller.controller_generation = None;
    session
        .adopt_binding(no_controller.clone())
        .expect("binding adopted");

    let request = session.permission_snapshot_request().unwrap();
    session
        .apply(RuntimeResponse {
            version: request.version,
            request_id: request.request_id.clone(),
            binding: no_controller,
            result: RuntimeResult::Permissions {
                snapshot: RuntimePermissionSnapshot {
                    cursor: RuntimeCursor {
                        generation: RuntimeGeneration::Id(id("runtime-1")),
                        revision: revision(1),
                    },
                    active: vec![presentation("handle-1", 1_000)],
                    receipts: Vec::new(),
                },
            },
        })
        .expect("snapshot applies");

    assert_eq!(
        session.decide_request("handle-1", 0, RuntimePermissionDecision::Deny, None, 10),
        Err(ClientError::MissingController)
    );
}

#[test]
fn queue_actions_need_a_cursor_and_a_declared_action() {
    let mut session = InteractionSession::new();
    assert_eq!(
        session.queue_action_request(RuntimeQueueAction::Clear),
        Err(ClientError::UnknownQueueCursor)
    );

    let request = session.queue_snapshot_request().unwrap();
    answer(
        &mut session,
        &request,
        RuntimeResult::Queue {
            snapshot: queue_snapshot(1, RuntimeQueueCompleteness::Complete, 1, vec![]),
        },
    );
    assert_eq!(
        session.queue_action_request(RuntimeQueueAction::Clear),
        Err(ClientError::UnsupportedAction)
    );

    let request = session.queue_snapshot_request().unwrap();
    answer(
        &mut session,
        &request,
        RuntimeResult::Queue {
            snapshot: queue_snapshot(
                2,
                RuntimeQueueCompleteness::Complete,
                1,
                vec![RuntimeQueueActionKind::Clear],
            ),
        },
    );
    let request = session
        .queue_action_request(RuntimeQueueAction::Clear)
        .expect("declared action");
    match &request.operation {
        RuntimeOperation::QueueApply { expected, .. } => {
            assert_eq!(u64::from(expected.revision), 2);
        }
        other => panic!("unexpected operation {other:?}"),
    }
}

#[test]
fn receipts_keep_their_outcome_and_later_polls_do_not_erase_failures() {
    let mut session = primed();
    let request = session.dispatch_request("deploy", "").unwrap();
    answer(
        &mut session,
        &request,
        RuntimeResult::CommandReceipt {
            receipt: oqto_runner_client::contract::RuntimeCommandReceipt {
                operation_id: id("op-1"),
                command_id: id("deploy"),
                disposition: oqto_runner_client::contract::RuntimeCommandDisposition::Started,
                rejection: None,
            },
        },
    );
    assert!(session.receipts().by_operation(&id("op-1")).is_some());

    let event = RuntimeEvent {
        version: request.version,
        event_id: id("event-1"),
        binding: binding(),
        payload: RuntimeEventPayload::CommandOutcome {
            operation_id: id("op-1"),
            state: RuntimeEffectState::Failed,
            error_id: Some(id("err-1")),
        },
    };
    session.apply_event(event).expect("outcome applies");

    let error = RuntimeError {
        event_id: id("err-1"),
        operation_id: Some(id("op-1")),
        source: RuntimeErrorSource::Engine,
        code: "dispatch_failed".into(),
        message: "safe text".into(),
        phase: "dispatch".into(),
        recoverable: false,
        uncertain: true,
    };
    let request = session.error_list_request(None).unwrap();
    answer(
        &mut session,
        &request,
        RuntimeResult::Errors {
            snapshot: oqto_runner_client::contract::RuntimeErrorSnapshot {
                cursor: RuntimeCursor {
                    generation: RuntimeGeneration::Id(id("runtime-1")),
                    revision: revision(5),
                },
                errors: vec![error.clone()],
            },
        },
    );
    assert_eq!(session.errors().len(), 1);

    let request = session.error_list_request(None).unwrap();
    answer(
        &mut session,
        &request,
        RuntimeResult::Errors {
            snapshot: oqto_runner_client::contract::RuntimeErrorSnapshot {
                cursor: RuntimeCursor {
                    generation: RuntimeGeneration::Id(id("runtime-1")),
                    revision: revision(6),
                },
                errors: vec![error.clone()],
            },
        },
    );
    assert_eq!(
        session.errors().len(),
        1,
        "duplicate polls do not duplicate"
    );
    assert_eq!(
        session.errors_cursor().map(|c| u64::from(c.revision)),
        Some(6)
    );
}

#[test]
fn permission_receipts_are_recorded_without_becoming_grants() {
    let mut session = primed();
    let request = session.permission_snapshot_request().unwrap();
    answer(
        &mut session,
        &request,
        RuntimeResult::Permissions {
            snapshot: RuntimePermissionSnapshot {
                cursor: RuntimeCursor {
                    generation: RuntimeGeneration::Id(id("runtime-1")),
                    revision: revision(1),
                },
                active: vec![presentation("handle-1", 1_000)],
                receipts: Vec::new(),
            },
        },
    );

    let request = session
        .decide_request(
            "handle-1",
            0,
            RuntimePermissionDecision::AllowOnce,
            Some("allow"),
            10,
        )
        .expect("valid decision");
    answer(
        &mut session,
        &request,
        RuntimeResult::PermissionReceipt {
            receipt: RuntimePermissionReceipt {
                cursor: RuntimeCursor {
                    generation: RuntimeGeneration::Id(id("runtime-1")),
                    revision: revision(2),
                },
                handle_id: id("handle-1"),
                request_id: id("request-1"),
                request_revision: revision(0),
                kind: RuntimePermissionReceiptKind::Decision,
                outcome: RuntimePermissionOutcome::Allow,
                option_id: Some(id("allow")),
                reason: "allowed".into(),
                source: RuntimePermissionSource::Adapter,
                controller_id: id("device-1"),
                native_session_digest: digest('f'),
                subject_digest: digest('b'),
                operation_digest: digest('c'),
                at_ms: revision(10),
                restored: false,
            },
        },
    );
    assert_eq!(session.receipts().entries().len(), 1);
    assert_eq!(
        session.permissions().active().len(),
        1,
        "a receipt alone does not clear the request; only the producer snapshot does"
    );

    let request = session.permission_snapshot_request().unwrap();
    answer(
        &mut session,
        &request,
        RuntimeResult::Permissions {
            snapshot: RuntimePermissionSnapshot {
                cursor: RuntimeCursor {
                    generation: RuntimeGeneration::Id(id("runtime-1")),
                    revision: revision(3),
                },
                active: Vec::new(),
                receipts: Vec::new(),
            },
        },
    );
    assert!(session.permissions().active().is_empty());
}

#[test]
fn events_from_another_binding_are_refused() {
    let mut session = primed();
    let mut event = RuntimeEvent {
        version: oqto_runner_client::contract::RuntimeVersion::V1,
        event_id: id("event-9"),
        binding: binding(),
        payload: RuntimeEventPayload::CommandOutcome {
            operation_id: id("op-1"),
            state: RuntimeEffectState::Succeeded,
            error_id: None,
        },
    };
    event.binding.runtime_generation = RuntimeGeneration::Id(id("runtime-2"));
    assert_eq!(session.apply_event(event), Err(ClientError::BindingChanged));
}
