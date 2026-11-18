mod support;
use oqto_personal_client::*;
use std::sync::Arc;
use support::*;

#[tokio::test]
async fn cancelled_input_remains_unknown_and_is_never_resent_on_resume() {
    let peer = Peer::start().await;
    let (client, _, _) = peer.enrolled().await;
    let attachment = client.attach(HOST, SESSION).await.unwrap();
    client.request_control(HOST, &attachment.id).await.unwrap();
    peer.state.lock().unwrap().pause = Some("input");
    let copy = client.clone();
    let id = attachment.id.clone();
    let task = tokio::spawn(async move { copy.input(HOST, &id, SessionInput::Abort).await });
    peer.entered.notified().await;
    client.invalidate().unwrap();
    assert_eq!(
        task.await.unwrap().unwrap_err(),
        ClientError::OutcomeUnknown
    );
    peer.release.notify_one();
    assert_eq!(peer.count("input"), 1);
    assert_eq!(
        client
            .input(HOST, &attachment.id, SessionInput::Abort)
            .await
            .unwrap_err(),
        ClientError::Denied
    );
    assert_eq!(peer.count("input"), 1);
}

#[tokio::test]
async fn cancelled_control_cannot_restore_a_handle_or_input_latch() {
    let peer = Peer::start().await;
    let (client, _, _) = peer.enrolled().await;
    let attachment = client.attach(HOST, SESSION).await.unwrap();
    peer.state.lock().unwrap().pause = Some("control");
    let copy = client.clone();
    let id = attachment.id.clone();
    let task = tokio::spawn(async move { copy.request_control(HOST, &id).await });
    peer.entered.notified().await;
    client.invalidate().unwrap();
    assert_eq!(
        task.await.unwrap().unwrap_err(),
        ClientError::OutcomeUnknown
    );
    peer.release.notify_one();
    assert_eq!(
        client
            .input(HOST, &attachment.id, SessionInput::Abort)
            .await
            .unwrap_err(),
        ClientError::Denied
    );
    assert_eq!(peer.count("input"), 0);
}

#[tokio::test]
async fn simultaneous_clients_reserve_once_without_overwriting_the_winning_key() {
    let peer = Peer::start().await;
    peer.state.lock().unwrap().pause = Some("enroll");
    let stores = Arc::new(Stores::default());
    let first = Arc::new(PersonalClient::new(stores.clone(), stores.clone()));
    let second = PersonalClient::new(stores.clone(), stores.clone());
    let copy = first.clone();
    let qr = peer.qr.clone();
    let task = tokio::spawn(async move { copy.enroll_once(&qr, "Synthetic", "Desktop").await });
    peer.entered.notified().await;
    let before = stores.secrets.lock().unwrap().clone();
    assert_eq!(
        second
            .enroll_once(&peer.qr, "Synthetic", "Desktop")
            .await
            .unwrap_err(),
        ClientError::AlreadyRecorded
    );
    assert_eq!(*stores.secrets.lock().unwrap(), before);
    peer.release.notify_one();
    let paired = task.await.unwrap().unwrap();
    assert!(paired.paired);
    assert_eq!(peer.count("enroll"), 1);
}

#[tokio::test]
async fn pending_generation_replacement_prevents_enrollment_aba_and_secret_overwrite() {
    let peer = Peer::start().await;
    peer.state.lock().unwrap().pause = Some("enroll");
    let stores = Arc::new(Stores::default());
    let client = Arc::new(PersonalClient::new(stores.clone(), stores.clone()));
    let copy = client.clone();
    let qr = peer.qr.clone();
    let task = tokio::spawn(async move { copy.enroll_once(&qr, "Synthetic", "Desktop").await });
    peer.entered.notified().await;
    let old = stores.hosts.lock().unwrap().get(HOST).unwrap().clone();
    let mut replacement = old.clone();
    replacement.registration_id = uuid::Uuid::new_v4().to_string();
    stores
        .hosts
        .lock()
        .unwrap()
        .insert(HOST.into(), replacement.clone());
    CredentialStore::save(
        &*stores,
        &replacement.credential_storage_key(),
        "synthetic-new-generation-value",
    )
    .unwrap();
    peer.release.notify_one();
    assert_eq!(
        task.await.unwrap().unwrap_err(),
        ClientError::OutcomeUnknown
    );
    assert_eq!(
        HostStore::load(&*stores, HOST).unwrap().as_ref(),
        Some(&replacement)
    );
    assert_eq!(
        CredentialStore::load(&*stores, &replacement.credential_storage_key())
            .unwrap()
            .as_deref(),
        Some("synthetic-new-generation-value")
    );
    assert_ne!(
        old.credential_storage_key(),
        replacement.credential_storage_key()
    );
    CredentialStore::remove(&*stores, &old.credential_storage_key()).unwrap();
    assert!(
        CredentialStore::load(&*stores, &replacement.credential_storage_key())
            .unwrap()
            .is_some()
    );
}

#[tokio::test]
async fn restart_has_no_cached_catalog_or_attachment_authority() {
    let peer = Peer::start().await;
    let (client, stores, host) = peer.enrolled().await;
    let attachment = client.attach(HOST, SESSION).await.unwrap();
    client.request_control(HOST, &attachment.id).await.unwrap();
    let restarted = PersonalClient::new(stores.clone(), stores);
    assert_eq!(
        restarted
            .input(HOST, &attachment.id, SessionInput::Abort)
            .await
            .unwrap_err(),
        ClientError::Denied
    );
    assert_eq!(
        restarted.attach(HOST, SESSION).await.unwrap_err(),
        ClientError::Denied
    );
    restarted.catalog(&host.id).await.unwrap();
    let fresh = restarted.attach(HOST, SESSION).await.unwrap();
    assert!(!fresh.input_held);
    assert_eq!(peer.count("input"), 0);
}

#[tokio::test]
async fn cancelled_history_does_not_install_a_cursor_for_the_new_epoch() {
    let peer = Peer::start().await;
    let (client, _, _) = peer.enrolled().await;
    {
        let mut state = peer.state.lock().unwrap();
        state.pause = Some("history");
        state.history["has_more"] = serde_json::json!(true);
        state.history["next_before"] = serde_json::json!("native:stale");
    }
    let copy = client.clone();
    let task = tokio::spawn(async move { copy.history(HOST, SESSION, None).await });
    peer.entered.notified().await;
    client.invalidate().unwrap();
    assert_eq!(task.await.unwrap().err(), Some(ClientError::Cancelled));
    peer.release.notify_one();
    client.catalog(HOST).await.unwrap();
    assert_eq!(
        client
            .history(HOST, SESSION, Some("native:stale".into()))
            .await
            .err(),
        Some(ClientError::InvalidInput)
    );
}
