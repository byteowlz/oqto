mod support;
use oqto_personal_client::*;
use serde_json::{Value, json};
use std::sync::{Arc, atomic::Ordering};
use support::*;

#[tokio::test]
async fn local_preview_enroll_once_custody_and_public_json_are_separate() {
    let peer = Peer::start().await;
    let stores = Arc::new(Stores::default());
    let client = PersonalClient::new(stores.clone(), stores.clone());
    let preview = pairing_preview(&peer.qr, now()).unwrap();
    assert_eq!(preview.host_id, HOST);
    assert_eq!(peer.count("enroll"), 0);
    assert!(stores.hosts.lock().unwrap().is_empty());
    assert!(stores.secrets.lock().unwrap().is_empty());
    assert!(!serde_json::to_string(&preview).unwrap().contains("token"));
    let host = client
        .enroll_once(&peer.qr, "Synthetic", "Desktop")
        .await
        .unwrap();
    assert!(host.paired);
    assert_eq!(peer.count("enroll"), 1);
    assert!(
        stores
            .secrets
            .lock()
            .unwrap()
            .contains_key(&host.credential_storage_key())
    );
    assert_eq!(
        client
            .enroll_once(&peer.qr, "Other", "Desktop")
            .await
            .unwrap_err(),
        ClientError::AlreadyRecorded
    );
    assert_eq!(peer.count("enroll"), 1);
    let public = serde_json::to_string(&host).unwrap();
    assert!(!public.contains("seed"));
    assert!(!public.contains("device_id"));
    assert!(!public.contains(&"a".repeat(64)));
    assert_eq!(
        serde_json::to_value(ClientError::OutcomeUnknown).unwrap(),
        "outcome_unknown"
    );
    assert_eq!(
        serde_json::to_value(ObservationState::Synchronizing).unwrap(),
        "synchronizing"
    );
}

#[tokio::test]
async fn signed_catalog_history_observe_control_prompt_abort_detach_use_one_substrate() {
    let peer = Peer::start().await;
    let (client, _, host) = peer.enrolled().await;
    let caps = client.capabilities(&host.id).await.unwrap();
    assert!(caps.session_observe_granted && caps.session_control_granted);
    assert!(!caps.full_user_terminal_granted);
    let page = client.history(HOST, SESSION, None).await.unwrap();
    assert_eq!(serde_json::to_value(&page).unwrap(), page_fixture());
    assert_eq!(page.messages[0].parts[0].part_type, "error");
    assert_eq!(
        page.messages[0].parts[0].text.as_deref(),
        Some("Synthetic provider refused.")
    );
    let observation = client.observe(HOST, SESSION).await.unwrap();
    assert_eq!(observation.state, ObservationState::Connected);
    assert!(observation.reconnected && observation.history.is_some());
    let attachment = observation.attachment.unwrap();
    assert!(
        !attachment.input_held,
        "server read must not establish control intent"
    );
    assert_eq!(attachment.context_usage.as_ref().unwrap().tokens, Some(0));
    assert_eq!(
        client
            .input(HOST, &attachment.id, SessionInput::Abort)
            .await
            .unwrap_err(),
        ClientError::Denied
    );
    assert_eq!(peer.count("input"), 0);
    let held = client.request_control(HOST, &attachment.id).await.unwrap();
    assert!(held.input_held);
    let receipt = client
        .input(
            HOST,
            &held.id,
            SessionInput::Prompt {
                message: "Synthetic prompt".into(),
            },
        )
        .await
        .unwrap();
    assert_eq!(receipt.session_id, SESSION);
    client
        .input(HOST, &held.id, SessionInput::Abort)
        .await
        .unwrap();
    assert_eq!(peer.count("input"), 2);
    client.detach(HOST, &held.id).await.unwrap();
    assert_eq!(
        client.read(HOST, &held.id).await.unwrap_err(),
        ClientError::Denied
    );
    assert_eq!(peer.count("detach"), 1);
}

fn page_fixture() -> Value {
    support::page()
}

#[tokio::test]
async fn duplicate_catalog_unknown_session_missing_leaf_and_replaced_identity_refuse() {
    let peer = Peer::start().await;
    let (client, _, _) = peer.enrolled().await;
    assert_eq!(
        client.attach(HOST, "unknown").await.unwrap_err(),
        ClientError::Denied
    );
    peer.state.lock().unwrap().duplicate_catalog = true;
    assert_eq!(
        client.catalog(HOST).await.unwrap_err(),
        ClientError::InvalidResponse
    );
    peer.state.lock().unwrap().duplicate_catalog = false;
    let attachment = client.attach(HOST, SESSION).await.unwrap();
    peer.state
        .lock()
        .unwrap()
        .read
        .as_object_mut()
        .unwrap()
        .remove("native_leaf_id");
    assert_eq!(
        client.read(HOST, &attachment.id).await.unwrap_err(),
        ClientError::InvalidResponse
    );
    let before = peer.count("read");
    assert_eq!(
        client.read(HOST, &attachment.id).await.unwrap_err(),
        ClientError::Denied
    );
    assert_eq!(peer.count("read"), before);
    peer.state.lock().unwrap().read["native_leaf_id"] = Value::Null;
    let attachment = client.attach(HOST, SESSION).await.unwrap();
    peer.state.lock().unwrap().read["session_id"] = json!("other-canonical-session");
    assert_eq!(
        client.read(HOST, &attachment.id).await.unwrap_err(),
        ClientError::InvalidResponse
    );
}

#[tokio::test]
async fn denial_invalidates_local_catalog_handles_and_input_admission() {
    let peer = Peer::start().await;
    let (client, _, _) = peer.enrolled().await;
    let attachment = client.attach(HOST, SESSION).await.unwrap();
    client.request_control(HOST, &attachment.id).await.unwrap();
    peer.state.lock().unwrap().denied = true;
    assert_eq!(
        client.capabilities(HOST).await.unwrap_err(),
        ClientError::Denied
    );
    peer.state.lock().unwrap().denied = false;
    assert_eq!(
        client
            .input(HOST, &attachment.id, SessionInput::Abort)
            .await
            .unwrap_err(),
        ClientError::Denied
    );
    assert_eq!(
        client.attach(HOST, SESSION).await.unwrap_err(),
        ClientError::Denied
    );
    assert_eq!(peer.count("input"), 0);
}

#[tokio::test]
async fn pin_change_is_typed_and_never_falls_back_or_sends_a_signed_action() {
    let peer = Peer::start().await;
    let (client, stores, host) = peer.enrolled().await;
    client.invalidate().unwrap();
    stores
        .hosts
        .lock()
        .unwrap()
        .get_mut(HOST)
        .unwrap()
        .certificate_sha256 = "0".repeat(64);
    assert_eq!(
        client.capabilities(HOST).await.unwrap_err(),
        ClientError::PinMismatch
    );
    assert_eq!(peer.count("capabilities"), 0);
    assert_ne!(host.certificate_sha256, "0".repeat(64));
}

#[tokio::test]
async fn offline_read_and_unknown_input_outcomes_are_distinct_and_never_replayed() {
    let peer = Peer::start().await;
    let (client, stores, _) = peer.enrolled().await;
    let attachment = client.attach(HOST, SESSION).await.unwrap();
    client.request_control(HOST, &attachment.id).await.unwrap();
    peer.state.lock().unwrap().input_disconnect = true;
    assert_eq!(
        client
            .input(
                HOST,
                &attachment.id,
                SessionInput::Prompt {
                    message: "Synthetic uncertain prompt".into()
                }
            )
            .await
            .unwrap_err(),
        ClientError::OutcomeUnknown
    );
    assert_eq!(peer.count("input"), 1);
    assert_eq!(
        client
            .input(HOST, &attachment.id, SessionInput::Abort)
            .await
            .unwrap_err(),
        ClientError::Denied
    );
    assert_eq!(peer.count("input"), 1);
    let reserved = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let offline = format!("https://{}/", reserved.local_addr().unwrap());
    drop(reserved);
    client.invalidate().unwrap();
    stores.hosts.lock().unwrap().get_mut(HOST).unwrap().origin = offline;
    assert_eq!(
        client.capabilities(HOST).await.unwrap_err(),
        ClientError::Offline
    );
}

#[tokio::test]
async fn request_receipt_mismatch_is_unknown_not_success_or_a_retry() {
    let peer = Peer::start().await;
    let (client, _, _) = peer.enrolled().await;
    let attachment = client.attach(HOST, SESSION).await.unwrap();
    client.request_control(HOST, &attachment.id).await.unwrap();
    peer.state.lock().unwrap().bad_receipt = true;
    assert_eq!(
        client
            .input(HOST, &attachment.id, SessionInput::Abort)
            .await
            .unwrap_err(),
        ClientError::OutcomeUnknown
    );
    assert_eq!(peer.count("input"), 1);
}

#[tokio::test]
async fn cancellation_fences_slow_read_and_cannot_restore_old_input() {
    let peer = Peer::start().await;
    let (client, _, _) = peer.enrolled().await;
    let attachment = client.attach(HOST, SESSION).await.unwrap();
    client.request_control(HOST, &attachment.id).await.unwrap();
    peer.state.lock().unwrap().pause = Some("read");
    let task_client = client.clone();
    let id = attachment.id.clone();
    let task = tokio::spawn(async move { task_client.read(HOST, &id).await });
    peer.entered.notified().await;
    client.set_active(false).unwrap();
    assert_eq!(task.await.unwrap().unwrap_err(), ClientError::Cancelled);
    peer.release.notify_one();
    client.set_active(true).unwrap();
    assert_eq!(
        client
            .input(HOST, &attachment.id, SessionInput::Abort)
            .await
            .unwrap_err(),
        ClientError::Denied
    );
    assert_eq!(
        client.attach(HOST, SESSION).await.unwrap_err(),
        ClientError::Denied
    );
    assert_eq!(peer.count("input"), 0);
}

#[tokio::test]
async fn cancelled_enrollment_stays_pending_and_secret_custody_never_replays() {
    let peer = Peer::start().await;
    peer.state.lock().unwrap().pause = Some("enroll");
    let stores = Arc::new(Stores::default());
    let client = Arc::new(PersonalClient::new(stores.clone(), stores.clone()));
    let task_client = client.clone();
    let qr = peer.qr.clone();
    let task =
        tokio::spawn(async move { task_client.enroll_once(&qr, "Synthetic", "Desktop").await });
    peer.entered.notified().await;
    client.invalidate().unwrap();
    assert_eq!(
        task.await.unwrap().unwrap_err(),
        ClientError::OutcomeUnknown
    );
    peer.release.notify_one();
    let pending = stores.hosts.lock().unwrap().get(HOST).unwrap().clone();
    assert!(!pending.paired);
    assert!(
        stores
            .secrets
            .lock()
            .unwrap()
            .contains_key(&pending.credential_storage_key())
    );
    assert_eq!(
        client
            .enroll_once(&peer.qr, "Again", "Desktop")
            .await
            .unwrap_err(),
        ClientError::AlreadyRecorded
    );
    assert_eq!(peer.count("enroll"), 1);
    assert_eq!(
        client.capabilities(HOST).await.unwrap_err(),
        ClientError::Denied
    );
}

#[tokio::test]
async fn keychain_failure_blocks_enrollment_before_network_and_preserves_pending_record() {
    let peer = Peer::start().await;
    let stores = Arc::new(Stores::default());
    stores.fail_secrets.store(true, Ordering::SeqCst);
    let client = PersonalClient::new(stores.clone(), stores.clone());
    assert_eq!(
        client
            .enroll_once(&peer.qr, "Synthetic", "Desktop")
            .await
            .unwrap_err(),
        ClientError::Storage
    );
    assert_eq!(peer.count("enroll"), 0);
    assert!(!stores.hosts.lock().unwrap().get(HOST).unwrap().paired);
}

#[tokio::test]
async fn reconnect_resync_and_history_unavailability_do_not_replay_input_or_kill_observation() {
    let peer = Peer::start().await;
    let (client, _, _) = peer.enrolled().await;
    peer.state.lock().unwrap().history_unavailable = true;
    let observation = client.observe(HOST, SESSION).await.unwrap();
    assert_eq!(observation.state, ObservationState::Connected);
    assert!(observation.history_pending && observation.history.is_none());
    let json = serde_json::to_value(observation).unwrap();
    assert_eq!(json["state"], "connected");
    assert_eq!(peer.count("input"), 0);
    client.invalidate().unwrap();
    client.catalog(HOST).await.unwrap();
    peer.state.lock().unwrap().history_unavailable = false;
    let observation = client.observe(HOST, SESSION).await.unwrap();
    assert!(observation.reconnected && observation.history.is_some());
    assert!(!observation.attachment.unwrap().input_held);
    assert_eq!(peer.count("input"), 0);
}

#[tokio::test]
async fn not_ready_observation_has_backoff_and_no_handle_or_false_connection() {
    let peer = Peer::start().await;
    let (client, _, _) = peer.enrolled().await;
    peer.state.lock().unwrap().not_ready = true;
    let observation = client.observe(HOST, SESSION).await.unwrap();
    assert_eq!(observation.state, ObservationState::Synchronizing);
    assert!(observation.attachment.is_none());
    assert!(observation.retry_after_ms > 0 && observation.retry_after_ms <= 2000);
    let count = peer.count("attach");
    let waiting = client.observe(HOST, SESSION).await.unwrap();
    assert_eq!(waiting.state, ObservationState::Synchronizing);
    assert_eq!(peer.count("attach"), count);
}

#[tokio::test]
async fn opaque_paging_cursors_are_admitted_once_for_exact_current_branch() {
    let peer = Peer::start().await;
    let (client, _, _) = peer.enrolled().await;
    assert_eq!(
        client
            .history(HOST, SESSION, Some("invented".into()))
            .await
            .err(),
        Some(ClientError::InvalidInput)
    );
    {
        let mut state = peer.state.lock().unwrap();
        state.history["has_more"] = json!(true);
        state.history["next_before"] = json!("native:opaque");
    }
    client.history(HOST, SESSION, None).await.unwrap();
    {
        let mut state = peer.state.lock().unwrap();
        state.history["has_more"] = json!(false);
        state.history["next_before"] = Value::Null;
        state.history["native_graph"]["revision"] = json!(2);
    }
    client
        .history(HOST, SESSION, Some("native:opaque".into()))
        .await
        .unwrap();
    assert_eq!(
        client
            .history(HOST, SESSION, Some("native:opaque".into()))
            .await
            .err(),
        Some(ClientError::InvalidInput)
    );
    client.invalidate().unwrap();
    client.catalog(HOST).await.unwrap();
    assert_eq!(
        client
            .history(HOST, SESSION, Some("native:opaque".into()))
            .await
            .err(),
        Some(ClientError::InvalidInput)
    );
}

#[tokio::test]
async fn malformed_history_never_becomes_a_canonical_page() {
    for mutation in [
        "missing_head",
        "missing_leaf",
        "wrong_session",
        "duplicate_id",
        "bad_cursor",
    ] {
        let peer = Peer::start().await;
        let (client, _, _) = peer.enrolled().await;
        {
            let mut state = peer.state.lock().unwrap();
            match mutation {
                "missing_head" => {
                    state
                        .history
                        .as_object_mut()
                        .unwrap()
                        .remove("native_graph");
                }
                "missing_leaf" => {
                    state.history["native_graph"]
                        .as_object_mut()
                        .unwrap()
                        .remove("native_leaf_id");
                }
                "wrong_session" => {
                    state.history["messages"][0]["session_id"] = json!("other-session")
                }
                "duplicate_id" => {
                    let message = state.history["messages"][0].clone();
                    state.history["messages"]
                        .as_array_mut()
                        .unwrap()
                        .push(message);
                }
                "bad_cursor" => state.history["has_more"] = json!(true),
                _ => unreachable!(),
            }
        }
        assert!(
            client.history(HOST, SESSION, None).await.is_err(),
            "{mutation}"
        );
    }
}
