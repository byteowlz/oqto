//! Focused disposable transport regressions; no model credentials/services/stores.
// Keep the test-only boundary visible to scanners of external module files.
#[cfg(test)]
use super::*;
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, BufReader};

fn request() -> crate::protocol::AgentControlRequest {
    crate::protocol::AgentControlRequest {
        session_id: "oqto-platform".into(),
        request_id: "client-request".into(),
        binding_id: "authenticated-connection".into(),
        operation: oqto_protocol::control::ControlOperation::SelectionGet,
        arguments: json!({"id":"review"}),
    }
}

async fn deliver(pending: &PendingResponses, id: &str, data: Value) {
    let sender = pending
        .write()
        .await
        .remove(id)
        .expect("registered before write");
    sender
        .send(PiResponse {
            id: Some(id.into()),
            success: true,
            data: Some(data),
            error: None,
        })
        .expect("live waiter");
}

async fn status(pending: &PendingResponses, request_id: &str, scope: &str) {
    let event: PiEvent = serde_json::from_value(json!({
        "type":"extension_ui_request", "id":"unrelated-ui-id", "method":"setStatus",
        "statusKey":"pi-selection:reply/v1",
        "statusText": json!({"version":1,"requestId":request_id,"command":"selection-get","ok":true,"result":{"scopeId":scope,"id":"review"}}).to_string()
    })).expect("status");
    route_status(&event, pending).await;
}

async fn harness<R: tokio::io::AsyncRead + Unpin>(
    reader: R,
    pending: PendingResponses,
    native: &str,
    source: &str,
) -> Vec<Value> {
    let mut lines = BufReader::new(reader).lines();
    let mut writes = vec![];
    while let Some(line) = lines.next_line().await.expect("line") {
        let command: Value = serde_json::from_str(&line).expect("RPC JSON");
        let id = command["id"].as_str().expect("unique RPC id");
        match command["type"].as_str().expect("type") {
            "get_state" => deliver(&pending, id, json!({"sessionId":native})).await,
            "get_commands" => {
                let commands = if source == "absent" {
                    json!({"commands":[]})
                } else {
                    let catalog_source =
                        if matches!(source, "missing-ack" | "unhandled" | "wrong-ack-scope") {
                            "extension"
                        } else {
                            source
                        };
                    json!({"commands":[{"name":"selection-get","source":catalog_source}]})
                };
                deliver(&pending, id, commands).await
            }
            "prompt" => {
                let message = command["message"].as_str().expect("message");
                let (name, args) = message.split_once(' ').expect("slash command JSON");
                assert_eq!(name, "/selection-get");
                assert!(!message.contains("oqto_meta"));
                assert!(command.get("streamingBehavior").is_none());
                let args: Value = serde_json::from_str(args).expect("strict JSONv1");
                let correlation = args["requestId"].as_str().expect("metadata correlation");
                assert_ne!(correlation, "client-request");
                assert_ne!(correlation, id);
                // Wrong request cannot consume the registered waiter. Status precedes RPC ack.
                status(&pending, "other-request", native).await;
                if source != "missing-ack" {
                    let scope = if source == "wrong-ack-scope" {
                        "pi-other"
                    } else {
                        native
                    };
                    status(&pending, correlation, scope).await;
                }
                let disposition = if source == "unhandled" {
                    "accepted"
                } else {
                    "handled"
                };
                deliver(&pending, id, json!({"disposition":disposition})).await;
            }
            other => panic!("unexpected chat/lifecycle operation: {other}"),
        }
        writes.push(command);
    }
    writes
}

#[tokio::test]
async fn absent_or_template_commands_and_wrong_native_scope_never_dispatch() {
    for (native, source, expected_writes) in [
        ("pi-native", "template", 2),
        ("pi-other", "extension", 1),
        ("pi-native", "skill", 2),
        ("pi-native", "absent", 2),
    ] {
        let pending = Arc::new(RwLock::new(HashMap::new()));
        let (mut writer, reader) = tokio::io::duplex(1024 * 1024);
        let task = tokio::spawn(harness(reader, pending.clone(), native, source));
        let mut ready = true;
        assert!(
            execute(&mut writer, &pending, "pi-native", request(), &mut ready)
                .await
                .is_err()
        );
        drop(writer);
        let writes = task.await.expect("harness");
        assert_eq!(writes.len(), expected_writes);
        assert!(writes.iter().all(|value| value["type"] != "prompt"));
        assert!(pending.read().await.is_empty());
        assert!(
            ready,
            "Failure before dispatch must not poison supported controls"
        );
    }
}

#[test]
fn public_discovery_and_control_share_the_exact_pi_one_catalog_shape() {
    let data = json!({"commands":[{"name":"selection-get","source":"extension"}]});
    assert_eq!(
        serde_json::Value::Array(catalog(&data).expect("Pi 1 wrapper").to_vec()),
        data["commands"]
    );
    assert!(discovered(&data, "selection-get"));
    for malformed in [
        json!([]),
        json!({"commands":null}),
        json!({"commands":{}}),
        json!({}),
    ] {
        assert!(catalog(&malformed).is_err());
        assert!(!discovered(&malformed, "selection-get"));
    }
}

#[tokio::test]
async fn timeout_or_unhandled_dispatch_cleans_waiters_and_cannot_be_retried_as_text() {
    for mode in ["missing-ack", "unhandled", "wrong-ack-scope"] {
        let pending = Arc::new(RwLock::new(HashMap::new()));
        let (mut writer, reader) = tokio::io::duplex(1024 * 1024);
        let task = tokio::spawn(harness(reader, pending.clone(), "pi-native", mode));
        let mut ready = true;
        assert!(
            execute(&mut writer, &pending, "pi-native", request(), &mut ready)
                .await
                .is_err()
        );
        assert!(!ready);
        assert!(pending.read().await.is_empty());
        assert!(
            execute(&mut writer, &pending, "pi-native", request(), &mut ready)
                .await
                .is_err()
        );
        drop(writer);
        assert_eq!(
            task.await.expect("harness").len(),
            3,
            "No retry or model fallback"
        );
    }
}

#[tokio::test]
async fn correlated_status_before_rpc_ack_preserves_both_identities() {
    let pending = Arc::new(RwLock::new(HashMap::new()));
    let (mut writer, reader) = tokio::io::duplex(1024 * 1024);
    let task = tokio::spawn(harness(reader, pending.clone(), "pi-native", "extension"));
    let mut ready = true;
    let result = execute(&mut writer, &pending, "pi-native", request(), &mut ready)
        .await
        .expect("handled control");
    assert_eq!(result.native_session_id, "pi-native");
    assert_eq!(
        result.binding_id, "authenticated-connection",
        "Connection identity must come from the transport, not a status snapshot"
    );
    assert_eq!(result.request_id, "client-request");
    assert_eq!(result.reply["result"]["scopeId"], "pi-native");
    drop(writer);
    assert_eq!(task.await.expect("harness").len(), 3);
    assert!(pending.read().await.is_empty());
}

#[tokio::test]
async fn command_processor_control_never_touches_prompt_bookkeeping_or_session_state() {
    let mut child = tokio::process::Command::new("cat")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .expect("disposable echo process");
    let stdin = child.stdin.take().expect("stdin");
    let stdout = child.stdout.take().expect("stdout");
    let pending = Arc::new(RwLock::new(HashMap::new()));
    let fake = tokio::spawn(harness(stdout, pending.clone(), "pi-native", "extension"));
    let (tx, rx) = mpsc::channel(4);
    let state = Arc::new(RwLock::new(PiSessionState::Streaming));
    let client_ids = Arc::new(Mutex::new(VecDeque::new()));
    let processor = tokio::spawn(PiSessionManager::command_processor_task(
        "oqto-platform".into(),
        stdin,
        rx,
        state.clone(),
        Arc::new(RwLock::new(Instant::now())),
        client_ids.clone(),
        Runtime {
            pending: pending.clone(),
            external_id: Arc::new(RwLock::new("pi-native".into())),
        },
        Arc::new(RwLock::new(
            crate::runtime_activity::PendingInputs::default(),
        )),
    ));
    let mut catalogs = Vec::new();
    for request_id in ["catalog-first", "catalog-second"] {
        let (reply, response) = oneshot::channel();
        pending.write().await.insert(request_id.into(), reply);
        tx.send(PiSessionCommand::GetCommands {
            request_id: request_id.into(),
        })
        .await
        .expect("query");
        catalogs.push(response);
    }
    for response in catalogs {
        let data = response
            .await
            .expect("independent discovery correlation")
            .data
            .expect("catalog");
        assert_eq!(
            catalog(&data).expect("exact Pi wrapper")[0]["name"],
            "selection-get"
        );
    }
    let (reply, result) = oneshot::channel();
    tx.send(PiSessionCommand::Control {
        request: request(),
        reply,
    })
    .await
    .expect("queue");
    result.await.expect("response channel").expect("control");
    assert_eq!(*state.read().await, PiSessionState::Streaming);
    assert!(client_ids.lock().await.is_empty());
    assert!(pending.read().await.is_empty());
    drop(tx);
    processor.await.expect("processor");
    child.wait().await.expect("echo exit");
    assert_eq!(fake.await.expect("fake harness").len(), 5);
}

#[tokio::test]
async fn concurrent_transactions_have_disjoint_rpc_and_metadata_correlations() {
    let pending = Arc::new(RwLock::new(HashMap::new()));
    let (mut first_writer, first_reader) = tokio::io::duplex(1024 * 1024);
    let (mut second_writer, second_reader) = tokio::io::duplex(1024 * 1024);
    let first_harness = tokio::spawn(harness(
        first_reader,
        pending.clone(),
        "pi-native",
        "extension",
    ));
    let second_harness = tokio::spawn(harness(
        second_reader,
        pending.clone(),
        "pi-native",
        "extension",
    ));
    let mut first_ready = true;
    let mut second_ready = true;
    let (first, second) = tokio::join!(
        execute(
            &mut first_writer,
            &pending,
            "pi-native",
            request(),
            &mut first_ready
        ),
        execute(
            &mut second_writer,
            &pending,
            "pi-native",
            request(),
            &mut second_ready
        ),
    );
    assert_ne!(
        first.expect("first").reply["requestId"],
        second.expect("second").reply["requestId"]
    );
    drop(first_writer);
    drop(second_writer);
    first_harness.await.expect("first harness");
    second_harness.await.expect("second harness");
    assert!(pending.read().await.is_empty());
}

#[tokio::test]
async fn session_replacement_invalidates_control_before_discovery_or_dispatch() {
    let mut child = tokio::process::Command::new("cat")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .expect("echo process");
    let stdin = child.stdin.take().expect("stdin");
    let stdout = child.stdout.take().expect("stdout");
    let pending = Arc::new(RwLock::new(HashMap::new()));
    let (tx, rx) = mpsc::channel(4);
    let state = Arc::new(RwLock::new(PiSessionState::Idle));
    let client_ids = Arc::new(Mutex::new(VecDeque::new()));
    let processor = tokio::spawn(PiSessionManager::command_processor_task(
        "oqto-platform".into(),
        stdin,
        rx,
        state.clone(),
        Arc::new(RwLock::new(Instant::now())),
        client_ids.clone(),
        Runtime {
            pending: pending.clone(),
            external_id: Arc::new(RwLock::new("pi-native".into())),
        },
        Arc::new(RwLock::new(
            crate::runtime_activity::PendingInputs::default(),
        )),
    ));
    tx.send(PiSessionCommand::NewSession(None))
        .await
        .expect("session replacement");
    let (reply, result) = oneshot::channel();
    tx.send(PiSessionCommand::Control {
        request: request(),
        reply,
    })
    .await
    .expect("queue");
    assert!(result.await.expect("response").is_err());
    drop(tx);
    processor.await.expect("processor");
    let mut lines = BufReader::new(stdout).lines();
    let line = lines
        .next_line()
        .await
        .expect("read")
        .expect("replacement command");
    assert_eq!(
        serde_json::from_str::<Value>(&line).expect("JSON")["type"],
        "new_session"
    );
    assert!(lines.next_line().await.expect("EOF").is_none());
    child.wait().await.expect("exit");
    assert!(client_ids.lock().await.is_empty());
    assert!(pending.read().await.is_empty());
}

#[test]
fn control_status_is_ephemeral_only_and_bounded_with_clear_support() {
    let mut translator = PiTranslator::new();
    for text in [
        Some(
            json!({"version":1,"bindingIds":["connection"],"review":{"scopeId":"pi-native"}})
                .to_string(),
        ),
        None,
    ] {
        let event: PiEvent = serde_json::from_value(json!({"type":"extension_ui_request","id":"ui","method":"setStatus","statusKey":"pi-selection:present/v1","statusText":text})).expect("UI");
        let events = translator.translate(&event);
        assert_eq!(events.len(), 1);
        assert!(
            matches!(&events[0], EventPayload::Status { key, .. } if key == "pi-selection:present/v1")
        );
    }
    for text in [
        "x".repeat(CONTROL_MAX_BYTES + 1),
        json!({"version":2}).to_string(),
        "not-json".into(),
    ] {
        let event = serde_json::from_value(json!({"type":"extension_ui_request","id":"ui","method":"setStatus","statusKey":"pi-selection:present/v1","statusText":text})).expect("UI");
        assert!(translator.translate(&event).is_empty());
    }
}

#[tokio::test]
async fn oversized_wrong_version_and_wrong_correlation_statuses_are_ignored() {
    let pending = Arc::new(RwLock::new(HashMap::new()));
    let (tx, mut rx) = oneshot::channel();
    pending
        .write()
        .await
        .insert(status_waiter("pi-selection:reply/v1", "expected"), tx);
    for text in [
        json!({"version":2,"requestId":"expected"}).to_string(),
        "x".repeat(CONTROL_MAX_BYTES + 1),
        json!({"version":1,"requestId":"wrong"}).to_string(),
    ] {
        let event = serde_json::from_value(json!({"type":"extension_ui_request","id":"ui","method":"setStatus","statusKey":"pi-selection:reply/v1","statusText":text})).expect("UI");
        route_status(&event, &pending).await;
        assert!(matches!(
            rx.try_recv(),
            Err(oneshot::error::TryRecvError::Empty)
        ));
    }
}
