mod support;
use oqto_personal_client::*;
use serde_json::{Value, json};
use support::*;

fn session(id: &str, relations: Value) -> Value {
    json!({"id":id,"workspace":"/synthetic/work","relations":relations})
}
fn reply(peer: &Peer, sessions: Vec<Value>) {
    peer.state.lock().unwrap().response_bytes =
        Some(serde_json::to_vec(&json!({"sessions":sessions})).unwrap());
}

#[tokio::test]
async fn authenticated_catalog_retains_typed_optional_kinds_without_changing_rows() {
    let peer = Peer::start().await;
    let (client, _, _) = peer.enrolled().await;
    reply(
        &peer,
        vec![
            session("parent", Value::Null),
            session("fork", Value::Null),
            session(
                "child",
                json!([
                    {"kind":"delegation","parent_session_id":"parent"},
                    {"kind":"fork","parent_session_id":"fork"}
                ]),
            ),
        ],
    );
    let rows = client.catalog(HOST).await.unwrap();
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[2].id, "child");
    assert_eq!(rows[2].relations.len(), 2);
    assert_eq!(rows[2].relations[0].kind, SessionRelationKind::Delegation);
    assert_eq!(rows[2].relations[1].kind, SessionRelationKind::Fork);
    assert_eq!(
        serde_json::to_value(&rows[2].relations).unwrap(),
        json!([
            {"kind":"delegation","parent_session_id":"parent"},{"kind":"fork","parent_session_id":"fork"}
        ])
    );
}

#[tokio::test]
async fn malformed_optional_facts_never_fail_an_otherwise_valid_catalog() {
    let peer = Peer::start().await;
    let (client, _, _) = peer.enrolled().await;
    for relations in [
        Value::Null,
        json!(false),
        json!("future"),
        json!({"kind":"delegation"}),
        json!([{"kind":"future","parent_session_id":"parent"}]),
        json!([{"kind":"delegation","parent_session_id":42}]),
        json!(vec![
            json!({"kind":"delegation","parent_session_id":"parent"});
            201
        ]),
    ] {
        reply(
            &peer,
            vec![session("parent", Value::Null), session("child", relations)],
        );
        let rows = client.catalog(HOST).await.unwrap();
        assert_eq!(rows.len(), 2);
        assert!(rows[1].relations.is_empty());
    }
}

#[tokio::test]
async fn fresh_absence_clears_previous_relation_and_old_host_schema_still_loads() {
    let peer = Peer::start().await;
    let (client, _, _) = peer.enrolled().await;
    reply(
        &peer,
        vec![
            session("parent", Value::Null),
            session(
                "child",
                json!([{"kind":"delegation","parent_session_id":"parent"}]),
            ),
        ],
    );
    assert_eq!(client.catalog(HOST).await.unwrap()[1].relations.len(), 1);
    reply(
        &peer,
        vec![
            json!({"id":"parent","workspace":"/synthetic/work"}),
            json!({"id":"child","workspace":"/synthetic/work"}),
        ],
    );
    let rows = client.catalog(HOST).await.unwrap();
    assert!(rows.iter().all(|row| row.relations.is_empty()));
    assert!(
        serde_json::to_value(&rows[1])
            .unwrap()
            .get("relations")
            .is_none()
    );
}

#[tokio::test]
async fn delegation_cycle_and_conflict_keep_valid_independent_fork() {
    let peer = Peer::start().await;
    let (client, _, _) = peer.enrolled().await;
    reply(
        &peer,
        vec![
            session("root", Value::Null),
            session(
                "a",
                json!([
                    {"kind":"delegation","parent_session_id":"b"},{"kind":"fork","parent_session_id":"root"}
                ]),
            ),
            session("b", json!([{"kind":"delegation","parent_session_id":"a"}])),
        ],
    );
    let rows = client.catalog(HOST).await.unwrap();
    assert_eq!(
        rows[1].relations,
        vec![SessionRelation {
            kind: SessionRelationKind::Fork,
            parent_session_id: "root".into()
        }]
    );
    assert!(rows[2].relations.is_empty());
    reply(
        &peer,
        vec![
            session("root", Value::Null),
            session(
                "a",
                json!([
                    {"kind":"delegation","parent_session_id":"a"},{"kind":"fork","parent_session_id":"root"}
                ]),
            ),
        ],
    );
    assert_eq!(
        client.catalog(HOST).await.unwrap()[1].relations[0].kind,
        SessionRelationKind::Fork
    );
}

#[tokio::test]
async fn out_of_set_parent_never_joins_previous_catalog_or_foreign_source() {
    let peer = Peer::start().await;
    let (client, _, _) = peer.enrolled().await;
    reply(
        &peer,
        vec![
            session("parent", Value::Null),
            session("child", Value::Null),
        ],
    );
    client.catalog(HOST).await.unwrap();
    reply(
        &peer,
        vec![session(
            "child",
            json!([{"kind":"delegation","parent_session_id":"parent","sourceId":"foreign-host"}]),
        )],
    );
    let rows = client.catalog(HOST).await.unwrap();
    assert_eq!(rows.len(), 1);
    assert!(rows[0].relations.is_empty());
}
