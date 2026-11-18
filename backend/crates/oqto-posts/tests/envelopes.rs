use anyhow::{Result, ensure};
use oqto_posts::wire::{Envelope, MAX_ENVELOPE, Request};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

#[test]
fn pinned_dingz_fixtures_are_unchanged_and_valid() -> Result<()> {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/dingz");
    for line in std::fs::read_to_string(dir.join("SHA256SUMS"))?.lines() {
        let (hash, name) = line
            .split_once("  ")
            .ok_or_else(|| anyhow::anyhow!("bad hash line"))?;
        let bytes = std::fs::read(dir.join(name))?;
        ensure!(
            hex::encode(Sha256::digest(&bytes)) == hash,
            "fixture changed: {name}"
        );
        if name != "post.schema.json" {
            let post: Envelope = serde_json::from_slice(&bytes)?;
            post.validate()?;
            let canonical = format!("{}\n", serde_json::to_string_pretty(&post)?);
            ensure!(canonical.as_bytes() == bytes, "golden differs: {name}");
        }
    }
    Ok(())
}

fn accepts(value: Value) -> bool {
    serde_json::from_value::<Envelope>(value).is_ok_and(|p| p.validate().is_ok())
}

#[test]
fn unknowns_and_bounds_are_rejected() {
    for value in [
        json!({"title":"T","kind":"urgent"}),
        json!({"title":"T","approve":true}),
        json!({"title":" "}),
        json!({"title":"x".repeat(201)}),
        json!({"title":"T","body":"x".repeat(4001)}),
        json!({"title":"T","body":"\u{1b}"}),
        json!({"title":"T","links":vec![json!({"url":"https://x"});9]}),
        json!({"title":"T","links":[{"url":"relative"}]}),
        json!({"title":"T","labels":["route:local","route:cloud"]}),
        json!({"title":"T","labels":["route:moon"]}),
        json!({"title":"T","labels":["a","a"]}),
        json!({"title":"T","labels":["A"]}),
        json!({"title":"T","task":"x".repeat(129)}),
        json!({"title":"T","links":[{"url":"https://x","label":"x".repeat(121)}]}),
        json!({"title":"T","labels":vec!["a";17]}),
    ] {
        assert!(!accepts(value.clone()), "accepted {value}");
    }
    assert!(accepts(
        json!({"title":"x".repeat(200),"body":"ä".repeat(4000)})
    ));
    assert!(accepts(json!({"title":"T","kind":"routine","task":null})));
    let huge = json!({"title":"T","body":"x".repeat(MAX_ENVELOPE)});
    assert!(!accepts(huge));
}

#[test]
fn schema_bounds_and_closed_kind_set_match() -> Result<()> {
    let schema: Value = serde_json::from_str(include_str!("fixtures/dingz/post.schema.json"))?;
    for (pointer, count) in [
        ("/properties/title/maxLength", 200),
        ("/properties/body/maxLength", 4000),
        ("/properties/links/maxItems", 8),
        ("/properties/labels/maxItems", 16),
        ("/x-max-serialized-bytes", 32768),
    ] {
        ensure!(schema.pointer(pointer) == Some(&json!(count)), "{pointer}");
    }
    for kind in [
        "done",
        "blocked",
        "delegation",
        "board",
        "wiki",
        "routine",
        "info",
    ] {
        ensure!(accepts(json!({"title":"T","kind":kind})), "kind {kind}");
    }
    Ok(())
}

#[test]
fn only_three_operations_and_hints_have_no_identity_type() -> Result<()> {
    for op in ["post", "watch", "reply"] {
        let value = json!({"v":1,"request_id":"r","op":op,"post":{"title":"T"},"reply":{"post_id":"p","mode":"steer","text":"approve nothing"},"unauthenticated_hints":{"agent_ctx":{"agent_id":"forged"}},"client":{"name":"dingz","version":"0.1.0"}});
        let request: Request = serde_json::from_value(value)?;
        ensure!(request.request_id == "r", "id");
    }
    for op in ["approve", "grant", "allow"] {
        assert!(
            serde_json::from_value::<Request>(json!({"v":1,"request_id":"r","op":op})).is_err()
        );
    }
    Ok(())
}
