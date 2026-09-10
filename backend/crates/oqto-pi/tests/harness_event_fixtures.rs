//! ADR-0049: `AgentEvent` is the harness-event contract and `PiEvent` is one
//! mirror of it. Mirrors are bound by recorded fixtures rather than discipline,
//! so a real captured stream must decode without losing events.

use std::path::Path;

/// A dropped event is invisible at runtime: the turn simply never completes.
/// Pi 0.85.1 declares `partial` on assistant message events but does not emit
/// it, which silently discarded every `message_update` until this was caught.
#[test]
fn recorded_pi_streams_decode_without_dropping_events() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let mut checked = 0;

    for entry in std::fs::read_dir(&dir).expect("fixture corpus").flatten() {
        let path = entry.path();
        if path.extension().is_none_or(|ext| ext != "jsonl") {
            continue;
        }
        let capture = std::fs::read_to_string(&path).expect("fixture readable");

        for (index, line) in capture.lines().enumerate() {
            if line.trim().is_empty() {
                continue;
            }
            let raw: serde_json::Value = serde_json::from_str(line).expect("fixture is JSON");
            // Only harness events are mirrored; RPC command replies are not.
            if raw.get("type").and_then(|t| t.as_str()) == Some("response") {
                continue;
            }

            let event: oqto_pi::PiEvent = serde_json::from_str(line).unwrap_or_else(|err| {
                panic!(
                    "{}:{} failed to decode: {err}\n{line}",
                    path.display(),
                    index + 1
                )
            });
            assert!(
                !matches!(event, oqto_pi::PiEvent::Unknown),
                "{}:{} decoded as Unknown, so the mirror silently drops it: {line}",
                path.display(),
                index + 1
            );
            checked += 1;
        }
    }

    assert!(checked > 0, "no fixture events were checked");
}
