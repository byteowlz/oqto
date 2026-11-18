use super::*;
use oqto_history::oqto_log::ops::OqtoLogSessionRow;
use oqto_session_relations::RelationKind;
use serde_json::json;

fn row(public: &str, stored: &str, parent: Option<&str>) -> OqtoLogSessionRow {
    OqtoLogSessionRow {
        session_id: stored.into(),
        platform_id: public.into(),
        external_id: Some(format!("native-{stored}")),
        user_id: "fixture-owner".into(),
        workspace_id: Some("/synthetic/work".into()),
        parent_session_id: parent.map(str::to_owned),
        forked_from_entry_id: parent.map(|_| "entry".into()),
        forked_at: None,
        created_at: String::new(),
        updated_at: String::new(),
        messages: 0,
        title: None,
        readable_id: None,
    }
}
fn data(ids: &[&str]) -> Value {
    json!({"sessions":ids.iter().map(|id|json!({"id":id,"workspace":"/synthetic/work","title":"[sub] is not evidence"})).collect::<Vec<_>>()})
}
fn fact(child: &str, kind: RelationKind, parent: &str) -> ValidatedRelation {
    // Synthetic already-admitted facts only, NOT a producer/source admission test.
    ValidatedRelation {
        child_public: child.into(),
        relation: Relation {
            kind,
            parent_session_id: parent.into(),
        },
    }
}
async fn enrich_legacy(output: &mut Value, rows: &[OqtoLogSessionRow]) {
    let root = tempfile::tempdir().unwrap();
    let session_root = root.path().join("native");
    let socket_root = root.path().join("sockets");
    std::fs::create_dir_all(&session_root).unwrap();
    std::fs::create_dir_all(&socket_root).unwrap();
    super::super::enrich(
        output,
        rows,
        &super::super::NativeRoots {
            session_root,
            socket_root,
        },
        &[],
    )
    .await;
}

#[test]
fn explicit_validated_public_facts_keep_both_kinds_without_legacy_inference() {
    let mut output = data(&["parent", "child", "fork"]);
    project(
        &mut output,
        &[
            fact("child", RelationKind::Delegation, "parent"),
            fact("child", RelationKind::Fork, "fork"),
        ],
    );
    assert_eq!(
        output["sessions"][1]["relations"],
        json!([
            {"kind":"delegation","parent_session_id":"parent"},{"kind":"fork","parent_session_id":"fork"}
        ])
    );
    assert_eq!(output["sessions"].as_array().unwrap().len(), 3);
}

#[test]
fn absent_filtered_parent_never_leaks_or_hides_child() {
    let mut output = data(&["child", "unrelated"]);
    let before = output.clone();
    project(
        &mut output,
        &[fact("child", RelationKind::Delegation, "denied-parent")],
    );
    assert_eq!(output, before);
    assert!(!output.to_string().contains("denied-parent"));
}

#[test]
fn bad_delegation_conflict_and_cycle_preserve_independent_validated_fork() {
    let mut output = data(&["root", "a", "b"]);
    project(
        &mut output,
        &[
            fact("a", RelationKind::Delegation, "b"),
            fact("b", RelationKind::Delegation, "a"),
            fact("a", RelationKind::Fork, "root"),
        ],
    );
    assert_eq!(
        output["sessions"][1]["relations"],
        json!([{"kind":"fork","parent_session_id":"root"}])
    );
    assert!(output["sessions"][2].get("relations").is_none());
    project(
        &mut output,
        &[
            fact("a", RelationKind::Delegation, "root"),
            fact("a", RelationKind::Delegation, "b"),
            fact("a", RelationKind::Fork, "root"),
        ],
    );
    assert_eq!(
        output["sessions"][1]["relations"],
        json!([{"kind":"fork","parent_session_id":"root"}])
    );
}

#[test]
fn duplicate_or_self_endpoints_change_only_optional_facts() {
    let mut output = data(&["duplicate", "duplicate", "child"]);
    let before = output.clone();
    project(
        &mut output,
        &[
            fact("child", RelationKind::Fork, "duplicate"),
            fact("child", RelationKind::Delegation, "child"),
        ],
    );
    assert_eq!(output, before);
}

#[test]
fn fresh_absence_never_carries_stale_or_peer_supplied_relation_authority() {
    let mut output = data(&["parent", "child"]);
    output["sessions"][1]["relations"] =
        json!([{"kind":"delegation","parent_session_id":"parent"}]);
    project(&mut output, &[]);
    assert!(output["sessions"][1].get("relations").is_none());
    let once = output.clone();
    project(&mut output, &[]);
    assert_eq!(output, once);
}

#[tokio::test]
async fn hidden_actual_parent_public_alias_cannot_bind_an_admitted_coincident_row() {
    let actual_parent = row("public-x", "raw-y", None);
    let coincident = row("public-x", "public-x", None);
    let child = row("child", "child", Some("public-x"));
    // COALESCE projected public-x from actual raw-y. Authorization/partial
    // selection omitted that parent. A visible coincident key is not evidence.
    let full = vec![actual_parent, coincident, child];
    let admitted: Vec<_> = full
        .into_iter()
        .filter(|row| row.session_id != "raw-y")
        .collect();
    let mut output = data(&["public-x", "child"]);
    enrich_legacy(&mut output, &admitted).await;
    assert!(
        output["sessions"]
            .as_array()
            .unwrap()
            .iter()
            .all(|row| row.get("relations").is_none())
    );
    assert_eq!(output["sessions"].as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn even_coincident_legacy_lineage_remains_unknown_without_tagged_mapping() {
    let rows = vec![
        row("parent", "parent", None),
        row("child", "child", Some("parent")),
    ];
    let mut output = data(&["parent", "child"]);
    enrich_legacy(&mut output, &rows).await;
    assert!(
        output["sessions"]
            .as_array()
            .unwrap()
            .iter()
            .all(|row| row.get("relations").is_none())
    );
    assert_eq!(output["sessions"].as_array().unwrap().len(), 2);
}
