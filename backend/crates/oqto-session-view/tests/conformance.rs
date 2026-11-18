//! Client conformance against the vendored contract bundle
//! (`oqto-protocol/contracts/session-view/v0`), as `spec.md` "Conformance" defines it:
//! every op vector as recorded, the invalid fixtures refused, every scenario replayed per
//! connection to the recorded hashes and final value, and the cached transcript of every
//! connection ending at exactly `full_transcript`.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use oqto_session_view::frames::{ClientFrame, HostFrame, Order, valid_frame};
use oqto_session_view::{Entry, Transcript, View, canonical, ops};
use serde_json::Value;

type Result<T> = std::result::Result<T, String>;

fn bundle() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../oqto-protocol/contracts/session-view/v0")
}

fn load(relative: &str) -> Result<Value> {
    let path = bundle().join(relative);
    let text = fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))
}

fn scenarios() -> Result<Vec<Value>> {
    let dir = bundle().join("scenarios");
    let mut names: Vec<PathBuf> = fs::read_dir(&dir)
        .map_err(|e| e.to_string())?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .collect();
    names.sort();
    names
        .iter()
        .map(|p| {
            load(&format!(
                "scenarios/{}",
                p.file_name().and_then(|n| n.to_str()).unwrap_or_default()
            ))
        })
        .collect()
}

fn array<'a>(value: &'a Value, key: &str) -> Result<&'a Vec<Value>> {
    value
        .get(key)
        .and_then(Value::as_array)
        .ok_or_else(|| format!("missing array {key}"))
}

#[test]
fn op_vectors_apply_as_recorded() -> Result<()> {
    let vectors = load("ops-vectors.json")?;
    let mut applied = 0;
    let mut refused = 0;
    for case in array(&vectors, "vectors")? {
        let name = case.get("name").and_then(Value::as_str).unwrap_or("?");
        let mut value = case.get("value").cloned().unwrap_or(Value::Null);
        let result = ops::apply(&mut value, array(case, "ops")?);
        match (case.get("error").and_then(Value::as_str), result) {
            (Some(code), Err(refusal)) if refusal.code() == code => refused += 1,
            (None, Ok(()))
                if canonical::to_string(&value) == canonical::to_string(&case["result"]) =>
            {
                applied += 1
            }
            (want, got) => {
                return Err(format!(
                    "vector {name}: expected {want:?}, got {got:?} -> {value}"
                ));
            }
        }
    }
    println!("op vectors: {applied} applied, {refused} refused");
    assert_eq!((applied, refused), (20, 18));
    Ok(())
}

#[test]
fn fixtures_parse_or_are_refused() -> Result<()> {
    let fixtures = load("fixtures.json")?;
    let valid = array(&fixtures, "valid")?;
    let invalid = array(&fixtures, "invalid")?;
    for case in valid {
        if !valid_frame(&case["frame"]) {
            let client = ClientFrame::parse(&case["frame"]).err();
            let host = HostFrame::parse(&case["frame"]).err();
            return Err(format!(
                "valid fixture refused: {} ({client:?} / {host:?})",
                case["name"]
            ));
        }
    }
    for case in invalid {
        if valid_frame(&case["frame"]) {
            return Err(format!("invalid fixture accepted: {}", case["name"]));
        }
    }
    println!(
        "fixtures: {} valid accepted, {} invalid refused",
        valid.len(),
        invalid.len()
    );
    assert_eq!((valid.len(), invalid.len()), (30, 41));
    Ok(())
}

/// One connection of a replay: its views by attachment and its caching transcript.
#[derive(Default)]
struct Connection {
    views: BTreeMap<String, View>,
    transcript: Transcript,
    requests: BTreeMap<String, ClientFrame>,
    /// Exclusive bound an ascending read continues, by its bounds.
    ascending: BTreeMap<(Option<u64>, Option<u64>), u64>,
    statuses: BTreeMap<String, String>,
}

struct Replay {
    hashes: usize,
    connections: BTreeMap<String, Connection>,
}

fn conn_of(step: &Value) -> String {
    step.get("conn")
        .and_then(Value::as_str)
        .unwrap_or("a")
        .to_owned()
}

fn replay(scenario: &Value) -> Result<Replay> {
    let name = scenario["scenario"].as_str().unwrap_or("?").to_owned();
    let mut connections: BTreeMap<String, Connection> = BTreeMap::new();
    let mut hashes = 0;
    for (index, step) in array(scenario, "steps")?.iter().enumerate() {
        let where_ = format!("{name} step {index}");
        let conn = connections.entry(conn_of(step)).or_default();
        if let Some(client) = step.get("client") {
            // Requests the driver sends on purpose although the schema refuses them are not
            // a client's; only their replies are checked.
            if let Ok(frame) = ClientFrame::parse(client) {
                conn.requests.insert(frame.id().to_owned(), frame);
            }
            continue;
        }
        let Some(raw) = step.get("host") else {
            continue;
        };
        let frame = HostFrame::parse(raw).map_err(|e| format!("{where_}: {e}"))?;
        handle(conn, frame, step, &where_, &mut hashes)?;
    }
    Ok(Replay {
        hashes,
        connections,
    })
}

fn handle(
    conn: &mut Connection,
    frame: HostFrame,
    step: &Value,
    where_: &str,
    hashes: &mut usize,
) -> Result<()> {
    match frame {
        HostFrame::Attached { attachment, .. } => {
            conn.views.insert(attachment, View::new());
        }
        HostFrame::View {
            attachment,
            seq,
            ops,
            ..
        } => {
            let view = conn
                .views
                .get_mut(&attachment)
                .ok_or_else(|| format!("{where_}: view before attached"))?;
            view.apply(seq, &ops)
                .map_err(|e| format!("{where_}: {e}"))?;
            let want = step
                .get("value_sha256")
                .and_then(Value::as_str)
                .unwrap_or_default();
            if canonical::sha256(view.value()) != want {
                return Err(format!("{where_}: view hash differs"));
            }
            *hashes += 1;
            conn.transcript.merge(&view.entries(), view.before());
        }
        HostFrame::Entries {
            id, items, next, ..
        } => page(conn, &id, &items, next.is_some(), where_)?,
        HostFrame::Receipt(receipt) => {
            let status = serde_json::to_value(receipt.status).map_err(|e| e.to_string())?;
            conn.statuses.insert(
                receipt.request_id,
                status.as_str().unwrap_or_default().to_owned(),
            );
        }
        HostFrame::Detached { .. } | HostFrame::Ended { .. } | HostFrame::Error { .. } => {}
    }
    Ok(())
}

/// A recorded `entries` reply joins the transcript as the caching client would: gap pages
/// continue the exclusive bound of the read, earlier pages extend below the oldest held.
fn page(conn: &mut Connection, id: &str, items: &[Entry], next: bool, where_: &str) -> Result<()> {
    let Some(ClientFrame::ReadEntries {
        min,
        max,
        order,
        cursor,
        ..
    }) = conn.requests.get(id).cloned()
    else {
        return Err(format!("{where_}: entries without a read"));
    };
    let more = next && !items.is_empty();
    if order == Some(Order::Ascending) {
        let bounds = (min, max);
        let known = match (cursor, conn.ascending.get(&bounds)) {
            (Some(_), Some(known)) => *known,
            _ => min.map_or(0, |m| m.saturating_sub(1)),
        };
        let top = max.ok_or_else(|| format!("{where_}: gap read without max"))?;
        conn.transcript.cover_ascending(known, top, items, more);
        if let Some(last) = items.last() {
            conn.ascending.insert(bounds, last.id());
        }
    } else {
        let top = conn
            .transcript
            .earlier_bound()
            .or(max)
            .ok_or_else(|| format!("{where_}: earlier read without max"))?;
        conn.transcript.cover_descending(top, items, more);
    }
    Ok(())
}

#[test]
fn scenarios_replay_to_hashes_final_values_and_receipts() -> Result<()> {
    let mut total = 0;
    let all = scenarios()?;
    for scenario in &all {
        let name = scenario["scenario"].as_str().unwrap_or("?");
        let replay = replay(scenario)?;
        let expected = array(scenario, "steps")?
            .iter()
            .filter(|s| s.get("value_sha256").is_some())
            .count();
        assert_eq!(replay.hashes, expected, "{name}: every view hash checked");
        total += replay.hashes;
        let attachment = scenario["attachment"].as_str().unwrap_or_default();
        let first = replay.connections.get("a").ok_or("no connection a")?;
        let view = first
            .views
            .get(attachment)
            .ok_or_else(|| format!("{name}: no view {attachment}"))?;
        if canonical::to_string(view.value()) != canonical::to_string(&scenario["final_value"]) {
            return Err(format!("{name}: final value differs"));
        }
        // Final receipt statuses per connection, as the scenario expects them.
        if let Some(by_conn) = scenario["expect"]
            .get("receipts_by_conn")
            .and_then(Value::as_object)
        {
            for (conn, requests) in by_conn {
                let held = replay
                    .connections
                    .get(conn)
                    .ok_or_else(|| format!("{name}: no connection {conn}"))?;
                for (request, statuses) in requests.as_object().into_iter().flatten() {
                    let want = statuses
                        .as_array()
                        .and_then(|s| s.last())
                        .and_then(Value::as_str);
                    assert_eq!(
                        held.statuses.get(request).map(String::as_str),
                        want,
                        "{name}: {conn} {request}"
                    );
                }
            }
        }
    }
    println!(
        "scenarios: {} replayed, {total} view hashes matched",
        all.len()
    );
    assert_eq!(all.len(), 12);
    Ok(())
}

#[test]
fn cached_transcripts_end_at_the_full_transcript() -> Result<()> {
    let mut connections = 0;
    for scenario in &scenarios()? {
        let name = scenario["scenario"].as_str().unwrap_or("?");
        let full: Vec<String> = array(scenario, "full_transcript")?
            .iter()
            .map(canonical::to_string)
            .collect();
        for (conn, state) in replay(scenario)?.connections {
            let held: Vec<String> = state
                .transcript
                .entries()
                .iter()
                .map(|e| canonical::to_string(e.value()))
                .collect();
            assert_eq!(held, full, "{name} connection {conn}: transcript");
            assert!(
                state.transcript.gaps().is_empty(),
                "{name} connection {conn}: gaps left"
            );
            assert_eq!(
                state.transcript.earlier_bound(),
                None,
                "{name} connection {conn}: start unknown"
            );
            connections += 1;
        }
    }
    println!("cached transcripts: {connections} connections hold exactly full_transcript");
    Ok(())
}

#[test]
fn two_clients_scenario_replays_per_connection() -> Result<()> {
    let scenario = load("scenarios/two-clients.json")?;
    let replay = replay(&scenario)?;
    assert_eq!(
        replay.connections.keys().cloned().collect::<Vec<_>>(),
        vec!["a".to_owned(), "b".to_owned()]
    );
    for (conn, state) in &replay.connections {
        assert!(!state.views.is_empty(), "connection {conn} attached");
    }
    Ok(())
}

#[test]
fn fill_gap_and_load_earlier_page_as_recorded() -> Result<()> {
    for (file, gap_before) in [
        ("scenarios/fill-gap.json", true),
        ("scenarios/load-earlier.json", false),
    ] {
        let scenario = load(file)?;
        let mut conn = Connection::default();
        let mut saw_gap = false;
        let mut hashes = 0;
        for (index, step) in array(&scenario, "steps")?.iter().enumerate() {
            if let Some(client) = step.get("client") {
                if let Ok(frame) = ClientFrame::parse(client) {
                    if matches!(frame, ClientFrame::ReadEntries { .. }) {
                        saw_gap |= !conn.transcript.gaps().is_empty();
                    }
                    conn.requests.insert(frame.id().to_owned(), frame);
                }
                continue;
            }
            if let Some(raw) = step.get("host") {
                let frame = HostFrame::parse(raw).map_err(|e| e.to_string())?;
                handle(
                    &mut conn,
                    frame,
                    step,
                    &format!("{file} {index}"),
                    &mut hashes,
                )?;
            }
        }
        assert_eq!(saw_gap, gap_before, "{file}: gap shown before the reads");
        assert!(conn.transcript.gaps().is_empty());
    }
    Ok(())
}
