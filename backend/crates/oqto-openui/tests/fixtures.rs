//! The Oqto OpenUI profile's shared conformance fixtures (`fixtures/`, see PROVENANCE.md)
//! against this renderer's decision: core-only parts render natively, everything else, and
//! unknown components, fall back to the text.

use std::fs;
use std::path::Path;

use oqto_openui::{Bounds, Decision, Fallback, decide_body};

fn body(name: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures")
        .join(name)
        .join("input.oui");
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

#[test]
fn every_fixture_gets_the_expected_decision() {
    let expected: [(&str, Option<&str>); 8] = [
        ("compare-options", None),
        ("core-rollout", None),
        ("core-ui-part", None),
        ("dashboard", Some("OverviewCardBlock")),
        ("editable-table", Some("EditableTable")),
        ("gallery-followups", Some("ImageGallery")),
        ("plan-approve", Some("Accordion")),
        ("invalid-unknown", Some("Hologram")),
    ];
    for (name, fallback) in expected {
        let decision = decide_body(&body(name), "0.2.2", &Bounds::default());
        match (fallback, &decision) {
            (None, Decision::Native(root)) => assert_eq!(root.name, "Stack", "{name}"),
            (Some(component), Decision::Fallback(Fallback::Component(c))) => {
                assert_eq!(c, component, "{name}")
            }
            _ => panic!("{name}: unexpected {decision:?}"),
        }
    }
    println!("openui fixtures: 8 decided (3 native, 5 fallback)");
}

#[test]
fn every_fixture_has_a_fallback_text() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures");
    let mut count = 0;
    for entry in fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("{e}"))
        .flatten()
    {
        if entry.path().is_dir() {
            let text = fs::read_to_string(entry.path().join("fallback.txt")).unwrap_or_default();
            assert!(!text.trim().is_empty(), "{}", entry.path().display());
            count += 1;
        }
    }
    assert_eq!(count, 8);
}
