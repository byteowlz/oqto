mod support;
use oqto_personal_client::*;
use serde_json::{Value, json};
use support::*;

#[tokio::test]
async fn current_context_is_source_owned_optional_and_clears_stale_values() {
    let peer = Peer::start().await;
    let (client, _, _) = peer.enrolled().await;
    let attachment = client.attach(HOST, SESSION).await.unwrap();
    for (wire, expected) in [
        (
            json!({"tokens":42,"context_window":100}),
            Some((Some(42), 100)),
        ),
        (
            json!({"tokens":42,"context_window":200}),
            Some((Some(42), 200)),
        ),
        (
            json!({"tokens":0,"context_window":200}),
            Some((Some(0), 200)),
        ),
        (
            json!({"tokens":null,"context_window":200}),
            Some((None, 200)),
        ),
        (
            json!({"tokens":120,"context_window":100}),
            Some((Some(120), 100)),
        ),
        (json!({"tokens":-1,"context_window":100}), None),
        (json!({"tokens":1.5,"context_window":100}), None),
        (json!({"tokens":42,"context_window":0}), None),
        (json!({"tokens":"42","context_window":100}), None),
        (Value::Null, None),
    ] {
        peer.state.lock().unwrap().read["details"]["context_usage"] = wire;
        let read = client.read(HOST, &attachment.id).await.unwrap();
        assert_eq!(
            read.context_usage
                .map(|usage| (usage.tokens, usage.context_window)),
            expected
        );
        assert!(!read.input_held, "observation never acquires input");
    }
    peer.state.lock().unwrap().read["details"]
        .as_object_mut()
        .unwrap()
        .remove("context_usage");
    assert!(
        client
            .read(HOST, &attachment.id)
            .await
            .unwrap()
            .context_usage
            .is_none()
    );
    assert_eq!(peer.count("input"), 0);
}

#[tokio::test]
async fn malformed_platform_credential_fails_before_network_not_by_reenrollment() {
    let peer = Peer::start().await;
    let (client, stores, host) = peer.enrolled().await;
    let old = peer.count("capabilities");
    CredentialStore::save(
        &*stores,
        &host.credential_storage_key(),
        "malformed-synthetic-value",
    )
    .unwrap();
    assert_eq!(
        client.capabilities(HOST).await.unwrap_err(),
        ClientError::Storage
    );
    assert_eq!(peer.count("capabilities"), old);
    assert_eq!(peer.count("enroll"), 1);
}
