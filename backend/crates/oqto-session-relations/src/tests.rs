use super::*;
use serde_json::json;

fn edge(kind: RelationKind, parent: &str) -> Relation {
    Relation {
        kind,
        parent_session_id: parent.into(),
    }
}
fn row(id: &str, relations: Vec<Relation>) -> CatalogRow {
    CatalogRow {
        id: id.into(),
        relations,
    }
}

#[test]
fn legacy_absence_null_and_malformed_optional_values_are_empty_not_row_errors() {
    #[derive(Deserialize)]
    struct Legacy {
        id: String,
        #[serde(default, deserialize_with = "deserialize_optional")]
        relations: Vec<Relation>,
    }
    for value in [
        json!({"id":"kept"}),
        json!({"id":"kept","relations":null}),
        json!({"id":"kept","relations":"unknown"}),
        json!({"id":"kept","relations":{}}),
    ] {
        let row: Legacy = serde_json::from_value(value).unwrap();
        assert_eq!(row.id, "kept");
        assert!(row.relations.is_empty());
    }
}

#[test]
fn tolerant_decode_keeps_valid_fork_when_delegation_is_bad_or_conflicting() {
    for bad in [
        json!([{"kind":"delegation","parent_session_id":false}]),
        json!([{"kind":"delegation","parent_session_id":"a"},{"kind":"delegation","parent_session_id":"b"}]),
        json!([{"kind":"delegation","parent_session_id":"a"},{"kind":"delegation","parent_session_id":"a"}]),
        json!([{"kind":"delegation","parent_session_id":""}]),
    ] {
        let mut records = bad.as_array().unwrap().clone();
        records.push(json!({"kind":"fork","parent_session_id":"fork-parent","future":true}));
        assert_eq!(
            parse_optional(&Value::Array(records)),
            vec![edge(RelationKind::Fork, "fork-parent")]
        );
    }
}

#[test]
fn unknown_kinds_and_source_tags_never_establish_a_different_catalog_source() {
    let facts = parse_optional(&json!([
        {"kind":"future","parent_session_id":"outside"},
        {"kind":"delegation","parent_session_id":"outside","sourceId":"another-host"},
        {"kind":"fork","parent_session_id":"local"}
    ]));
    let mut rows = vec![row("child", facts), row("local", vec![])];
    normalize(&mut rows);
    assert_eq!(rows[0].relations, vec![edge(RelationKind::Fork, "local")]);
    assert_eq!(rows.len(), 2);
}

#[test]
fn decoder_enforces_optional_input_and_identifier_bounds() {
    let many =
        vec![json!({"kind":"fork","parent_session_id":"parent"}); MAX_OPTIONAL_INPUT_RECORDS + 1];
    assert!(parse_optional(&Value::Array(many)).is_empty());
    assert!(
        parse_optional(
            &json!([{"kind":"delegation","parent_session_id":"x".repeat(MAX_PUBLIC_ID_BYTES+1)}])
        )
        .is_empty()
    );
    assert!(parse_optional(&json!([{"kind":"fork","parent_session_id":"bad\nparent"}])).is_empty());
    let facts = parse_optional(
        &json!([{"kind":"fork","parent_session_id":"a"},{"kind":"delegation","parent_session_id":"b"}]),
    );
    assert_eq!(facts.len(), MAX_RELATIONS_PER_ROW);
    assert_eq!(facts[0].kind, RelationKind::Delegation);
    assert_eq!(facts[1].kind, RelationKind::Fork);
}

#[test]
fn both_kinds_survive_and_only_same_set_distinct_unique_endpoints_are_retained() {
    let mut rows = vec![
        row(
            "child",
            vec![
                edge(RelationKind::Fork, "fork"),
                edge(RelationKind::Delegation, "parent"),
            ],
        ),
        row("parent", vec![]),
        row("fork", vec![]),
        row("orphan", vec![edge(RelationKind::Delegation, "missing")]),
        row(
            "self",
            vec![
                edge(RelationKind::Delegation, "self"),
                edge(RelationKind::Fork, "fork"),
            ],
        ),
    ];
    let ids: Vec<_> = rows.iter().map(|row| row.id.clone()).collect();
    normalize(&mut rows);
    assert_eq!(
        rows[0].relations,
        vec![
            edge(RelationKind::Delegation, "parent"),
            edge(RelationKind::Fork, "fork")
        ]
    );
    assert!(rows[3].relations.is_empty());
    assert_eq!(rows[4].relations, vec![edge(RelationKind::Fork, "fork")]);
    assert_eq!(
        rows.iter().map(|row| row.id.clone()).collect::<Vec<_>>(),
        ids
    );
}

#[test]
fn independent_cycles_do_not_erase_another_provenance_kind_or_base_rows() {
    let mut rows = vec![
        row(
            "a",
            vec![
                edge(RelationKind::Delegation, "b"),
                edge(RelationKind::Fork, "root"),
            ],
        ),
        row("b", vec![edge(RelationKind::Delegation, "c")]),
        row("c", vec![edge(RelationKind::Delegation, "a")]),
        row("root", vec![]),
        row("above", vec![edge(RelationKind::Delegation, "a")]),
    ];
    normalize(&mut rows);
    assert_eq!(rows[0].relations, vec![edge(RelationKind::Fork, "root")]);
    assert!(rows[1].relations.is_empty() && rows[2].relations.is_empty());
    assert_eq!(rows[4].relations, vec![edge(RelationKind::Delegation, "a")]);
    assert_eq!(rows.len(), 5);
}

#[test]
fn fork_cycles_and_conflicts_do_not_erase_valid_delegation() {
    let mut rows = vec![
        row(
            "a",
            vec![
                edge(RelationKind::Delegation, "root"),
                edge(RelationKind::Fork, "b"),
            ],
        ),
        row("b", vec![edge(RelationKind::Fork, "a")]),
        row("root", vec![]),
        row(
            "conflict",
            vec![
                edge(RelationKind::Fork, "a"),
                edge(RelationKind::Fork, "b"),
                edge(RelationKind::Delegation, "root"),
            ],
        ),
    ];
    normalize(&mut rows);
    assert_eq!(
        rows[0].relations,
        vec![edge(RelationKind::Delegation, "root")]
    );
    assert!(rows[1].relations.is_empty());
    assert_eq!(
        rows[3].relations,
        vec![edge(RelationKind::Delegation, "root")]
    );
}

#[test]
fn cross_kind_links_are_not_a_singular_overloaded_lineage_graph() {
    let mut rows = vec![
        row("a", vec![edge(RelationKind::Delegation, "b")]),
        row("b", vec![edge(RelationKind::Fork, "a")]),
    ];
    normalize(&mut rows);
    assert_eq!(rows[0].relations.len(), 1);
    assert_eq!(rows[1].relations.len(), 1);
}

#[test]
fn duplicate_public_rows_do_not_pick_a_first_parent_or_delete_any_row() {
    let mut rows = vec![
        row("duplicate", vec![]),
        row("duplicate", vec![]),
        row("child", vec![edge(RelationKind::Delegation, "duplicate")]),
    ];
    normalize(&mut rows);
    assert!(rows.iter().all(|row| row.relations.is_empty()));
    assert_eq!(rows.len(), 3);
}

#[test]
fn maximum_chain_is_iterative_and_oversized_catalog_becomes_flat() {
    let mut rows: Vec<_> = (0..MAX_CATALOG_ROWS)
        .map(|index| {
            row(
                &format!("s{index}"),
                if index == 0 {
                    vec![]
                } else {
                    vec![edge(RelationKind::Delegation, &format!("s{}", index - 1))]
                },
            )
        })
        .collect();
    normalize(&mut rows);
    assert_eq!(
        rows.iter().map(|row| row.relations.len()).sum::<usize>(),
        MAX_CATALOG_ROWS - 1
    );
    rows.push(row("extra", vec![edge(RelationKind::Delegation, "s0")]));
    normalize(&mut rows);
    assert!(rows.iter().all(|row| row.relations.is_empty()));
    assert_eq!(rows.len(), MAX_CATALOG_ROWS + 1);
}

#[test]
fn normalization_is_deterministic_and_idempotent() {
    let mut rows = vec![
        row(
            "a",
            vec![
                edge(RelationKind::Fork, "root"),
                edge(RelationKind::Delegation, "root"),
            ],
        ),
        row("root", vec![]),
    ];
    normalize(&mut rows);
    let once = rows.clone();
    normalize(&mut rows);
    assert_eq!(rows, once);
    assert_eq!(
        serde_json::to_value(&rows[0].relations).unwrap(),
        json!([
            {"kind":"delegation","parent_session_id":"root"},{"kind":"fork","parent_session_id":"root"}
        ])
    );
}
