//! Store-bound request recovery, without network, models or private data.
use oqto_session_client::{Notice, Options, RequestSource, SessionClient};
use oqto_session_view::frames::{ClientFrame, InputKind};
use serde_json::json;

fn effects(frames: &[ClientFrame]) -> Vec<String> {
    frames
        .iter()
        .filter(|f| {
            matches!(
                f,
                ClientFrame::Submit { .. } | ClientFrame::Write { .. } | ClientFrame::Abort { .. }
            )
        })
        .filter_map(|f| f.request_id().map(str::to_owned))
        .collect()
}

fn accept(client: &mut SessionClient, store: &str) -> Vec<ClientFrame> {
    let sent = client.take_outgoing();
    assert!(
        effects(&sent).is_empty(),
        "effects before attached: {sent:?}"
    );
    let id = sent
        .iter()
        .find(|f| matches!(f, ClientFrame::Attach { .. }))
        .expect("attach")
        .id();
    client.receive(&json!({
        "type": "attached", "id": id, "v": 0, "attachment": "att",
        "session": "ses", "store": store, "window": { "min": 50, "max": 100 },
        "upstream": { "package": "@earendil-works/pi-durable", "version": "1.5.2" },
        "max_frame_bytes": 8_388_608
    }));
    client.take_outgoing()
}

fn fresh() -> SessionClient {
    let mut client = SessionClient::new("ses", Options::default());
    client.connect();
    assert!(accept(&mut client, "st_a").is_empty());
    client
}

fn pending(client: &mut SessionClient) -> Vec<String> {
    vec![
        client.submit(InputKind::Prompt, "fixture action"),
        client.write("app.fixture", Some(json!({"text": "fixture"}))),
        client.abort(),
    ]
}

#[test]
fn same_store_reconnect_keeps_original_request_ids_and_waits_for_attachment() {
    let mut client = fresh();
    let ids = pending(&mut client);
    assert_eq!(effects(&client.take_outgoing()), ids);
    client.disconnect();
    client.connect();
    assert_eq!(effects(&accept(&mut client, "st_a")), ids);
    assert!(client.uncertain_requests().is_empty());
}

#[test]
fn replacement_quarantines_all_effect_kinds_even_if_the_old_store_returns() {
    let mut client = fresh();
    let ids = pending(&mut client);
    client.take_outgoing();
    client.disconnect();
    client.connect();
    assert!(accept(&mut client, "st_b").is_empty());
    assert_eq!(client.uncertain_requests().len(), 3);
    for request in client.requests() {
        assert_eq!(
            request.source,
            Some(RequestSource::Uncertain {
                store: Some("st_a".into())
            })
        );
    }
    client.receive(
        &json!({"type": "receipt", "session": "ses", "request_id": ids[0],
        "kind": "prompt", "status": "done", "entry": 1, "answer": 2}),
    );
    assert!(client.receipt(&ids[0]).is_none());
    client.disconnect();
    client.connect();
    assert!(accept(&mut client, "st_a").is_empty());
    assert_eq!(client.uncertain_requests().len(), 3);
    // A new explicit operation may target the newly observed store; old ones remain quarantined.
    let new_id = client.submit(InputKind::Prompt, "new intent");
    assert_eq!(effects(&client.take_outgoing()), vec![new_id]);
}

#[test]
fn quarantine_survives_cache_restore_and_colliding_entry_ids() {
    let mut client = fresh();
    let id = client.submit(InputKind::Prompt, "original");
    client.take_outgoing();
    client.receive(
        &json!({"type": "receipt", "session": "ses", "request_id": id,
        "kind": "prompt", "status": "placed", "entry": 3}),
    );
    client.disconnect();
    client.connect();
    accept(&mut client, "st_b");
    let mut restored = SessionClient::new("ses", Options::default());
    restored.restore(client.record());
    restored.connect();
    assert!(accept(&mut restored, "st_b").is_empty());
    restored.receive(&json!({"type":"view", "attachment":"att", "seq":1, "ops":[["r",{
        "conversation":{"id":1}, "entries":[{"id":3,"conversationId":1,"kind":"pi.user"}], "docs":{}
    }]]}));
    assert_eq!(restored.uncertain_requests().len(), 1);
    assert!(
        restored
            .items()
            .iter()
            .any(|i| matches!(i, oqto_session_client::Item::Pending { .. }))
    );
    assert_eq!(
        restored.receipt(&id).map(|r| r.status),
        Some(oqto_session_view::ReceiptStatus::Placed)
    );
}

#[test]
fn offline_intent_can_bind_in_memory_but_cached_unknown_source_cannot() {
    let mut offline = SessionClient::new("ses", Options::default());
    let ids = pending(&mut offline);
    let record = offline.record();
    offline.connect();
    assert_eq!(effects(&accept(&mut offline, "st_a")), ids);
    let mut restored = SessionClient::new("ses", Options::default());
    restored.restore(record.clone());
    restored.connect();
    assert!(accept(&mut restored, "st_a").is_empty());
    assert_eq!(restored.uncertain_requests().len(), 3);
    let mut legacy = record;
    for request in &mut legacy.requests {
        request.source = None;
    }
    let mut restored = SessionClient::new("ses", Options::default());
    restored.restore(legacy);
    restored.connect();
    assert!(accept(&mut restored, "st_a").is_empty());
    assert_eq!(restored.uncertain_requests().len(), 3);
}

#[test]
fn legacy_known_store_only_retries_against_that_store() {
    for current in ["st_a", "st_b"] {
        let mut client = fresh();
        let ids = pending(&mut client);
        let mut record = client.record();
        for request in &mut record.requests {
            request.source = None;
        }
        let mut restored = SessionClient::new("ses", Options::default());
        restored.restore(record);
        restored.connect();
        let sent = accept(&mut restored, current);
        if current == "st_a" {
            assert_eq!(effects(&sent), ids);
        } else {
            assert!(sent.is_empty());
            assert_eq!(restored.uncertain_requests().len(), 3);
        }
    }
}

#[test]
fn refused_attachment_emits_no_old_effect_and_purges_without_claiming_cancellation() {
    for code in ["denied", "unknown_session"] {
        let mut client = fresh();
        pending(&mut client);
        client.take_outgoing();
        client.disconnect();
        client.connect();
        let attach = client.take_outgoing();
        assert!(effects(&attach).is_empty());
        client.receive(&json!({"type":"error","id":attach[0].id(),"code":code}));
        assert!(client.take_outgoing().is_empty());
        assert!(client.take_purge());
        assert!(client.requests().is_empty());
        assert!(client.store().is_none());
        assert!(client.take_notices().iter().any(
            |n| matches!(n, Notice::Refused { code } if code == "access_lost_outcome_unknown")
        ));
    }
}

#[test]
fn failed_attachment_rechecks_store_before_any_query_or_effect() {
    let mut client = fresh();
    pending(&mut client);
    let outgoing = client.take_outgoing();
    client.receive(&json!({"type":"ended","attachment":"att","reason":"failed"}));
    assert!(accept(&mut client, "st_b").is_empty());
    client.receive(&json!({"type":"error","id":outgoing[0].id(),"code":"busy"}));
    assert_eq!(client.uncertain_requests().len(), 3);
    assert_eq!(client.error(), Some("store_changed_outcome_unknown"));
}
