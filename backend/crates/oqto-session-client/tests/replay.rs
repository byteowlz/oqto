//! The caching client against the contract's captured scenarios, as byoagnt's `replayCached`
//! does for the TypeScript client: one client per connection, each re-attach a fresh client
//! started from the previous one's cache, the client's own `attach` and `read_entries` equal
//! to the recorded ones up to their ids, every view hash matched, and every connection ending
//! with exactly `full_transcript`, no gaps and the start known.

use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};

use oqto_session_client::{Options, SessionClient};
use oqto_session_view::canonical;
use oqto_session_view::frames::ClientFrame;
use serde_json::Value;

type Result<T> = std::result::Result<T, String>;

fn bundle() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../oqto-protocol/contracts/session-view/v0/scenarios")
}

struct Peer {
    client: SessionClient,
    options: Options,
    ids: HashMap<String, String>,
    sent: Vec<ClientFrame>,
    attached: bool,
}

#[derive(Default)]
struct Outcome {
    hashes: usize,
    reads: usize,
    restarts: usize,
}

fn without_id(frame: &Value) -> String {
    let mut copy = frame.clone();
    if let Some(object) = copy.as_object_mut() {
        object.remove("id");
    }
    canonical::to_string(&copy)
}

fn options_for(steps: &[Value], conn: &str) -> Options {
    let mine = |s: &&Value| s.get("conn").and_then(Value::as_str).unwrap_or("a") == conn;
    let attach = steps
        .iter()
        .filter(mine)
        .find_map(|s| s.get("client").filter(|c| c["type"] == "attach"));
    let read = steps
        .iter()
        .find_map(|s| s.get("client").filter(|c| c["type"] == "read_entries"));
    Options {
        window_min: attach
            .and_then(|a| a.pointer("/window/min"))
            .and_then(Value::as_u64),
        window_max: attach
            .and_then(|a| a.pointer("/window/max"))
            .and_then(Value::as_u64),
        page_size: read
            .and_then(|r| r.get("limit"))
            .and_then(Value::as_u64)
            .unwrap_or(50),
    }
}

/// The client's next frame of `kind`, compared to the recorded one up to its id.
fn produced(peer: &mut Peer, kind: &str, recorded: &Value) -> Result<()> {
    peer.sent.extend(peer.client.take_outgoing());
    let index = peer
        .sent
        .iter()
        .position(|f| f.to_value()["type"] == kind)
        .ok_or_else(|| format!("client produced no {kind} for {recorded}"))?;
    let mine = peer.sent.remove(index).to_value();
    if without_id(&mine) != without_id(recorded) {
        return Err(format!("{kind} differs: mine {mine} recorded {recorded}"));
    }
    let recorded_id = recorded["id"].as_str().unwrap_or_default().to_owned();
    peer.ids.insert(
        recorded_id,
        mine["id"].as_str().unwrap_or_default().to_owned(),
    );
    Ok(())
}

fn client_step(peer: &mut Peer, frame: &Value, session: &str, outcome: &mut Outcome) -> Result<()> {
    let Ok(parsed) = ClientFrame::parse(frame) else {
        return Ok(());
    };
    match parsed {
        ClientFrame::Attach {
            v: 0,
            session: ref s,
            ..
        } if s == session => {
            if peer.attached {
                let record = peer.client.record();
                peer.client = SessionClient::new(session, peer.options.clone());
                peer.client.restore(record);
                outcome.restarts += 1;
            }
            peer.client.connect();
            peer.attached = true;
            produced(peer, "attach", frame)
        }
        ClientFrame::Detach { .. } => {
            peer.client.detach();
            produced(peer, "detach", frame)
        }
        ClientFrame::ReadEntries { .. } => {
            peer.sent.extend(peer.client.take_outgoing());
            let pending = peer
                .sent
                .iter()
                .any(|f| matches!(f, ClientFrame::ReadEntries { .. }));
            if !pending {
                let transcript = peer.client.transcript();
                if !transcript.gaps().is_empty() {
                    peer.client.fill_gap();
                } else if transcript.earlier_bound().is_some() {
                    peer.client.load_earlier();
                } else {
                    // Nothing missing and the start known: a probe of the driver's.
                    return Ok(());
                }
            }
            outcome.reads += 1;
            produced(peer, "read_entries", frame)
        }
        // Inputs and deliberate refusals belong to the scenario's driver, not this client.
        _ => Ok(()),
    }
}

fn replay(scenario: &Value) -> Result<(Outcome, BTreeMap<String, Peer>)> {
    let session = scenario["session"].as_str().unwrap_or_default();
    let steps = scenario["steps"].as_array().ok_or("steps")?;
    let mut peers: BTreeMap<String, Peer> = BTreeMap::new();
    let mut outcome = Outcome::default();
    for step in steps {
        let conn = step
            .get("conn")
            .and_then(Value::as_str)
            .unwrap_or("a")
            .to_owned();
        let peer = peers.entry(conn.clone()).or_insert_with(|| {
            let options = options_for(steps, &conn);
            Peer {
                client: SessionClient::new(session, options.clone()),
                options,
                ids: HashMap::new(),
                sent: Vec::new(),
                attached: false,
            }
        });
        if step.get("close").is_some() {
            peer.client.disconnect();
            continue;
        }
        if let Some(frame) = step.get("client") {
            client_step(peer, frame, session, &mut outcome)?;
            continue;
        }
        let Some(host) = step.get("host") else {
            continue;
        };
        let mut frame = host.clone();
        if let Some(mine) = host
            .get("id")
            .and_then(Value::as_str)
            .and_then(|id| peer.ids.get(id))
        {
            frame["id"] = Value::String(mine.clone());
        }
        peer.client.receive(&frame);
        peer.sent.extend(peer.client.take_outgoing());
        if let Some(want) = step.get("value_sha256").and_then(Value::as_str) {
            if canonical::sha256(peer.client.view().value()) != want {
                return Err(format!("view hash differs on {conn}"));
            }
            outcome.hashes += 1;
        }
    }
    Ok((outcome, peers))
}

fn scenarios() -> Result<Vec<Value>> {
    let mut paths: Vec<PathBuf> = fs::read_dir(bundle())
        .map_err(|e| e.to_string())?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .collect();
    paths.sort();
    paths
        .iter()
        .map(|p| {
            fs::read_to_string(p)
                .map_err(|e| e.to_string())
                .and_then(|t| serde_json::from_str(&t).map_err(|e| e.to_string()))
        })
        .collect()
}

#[test]
fn caching_client_replays_every_scenario_to_the_full_transcript() -> Result<()> {
    let mut connections = 0;
    let mut hashes = 0;
    for scenario in scenarios()? {
        let name = scenario["scenario"].as_str().unwrap_or("?").to_owned();
        let (outcome, peers) = replay(&scenario).map_err(|e| format!("{name}: {e}"))?;
        let expected = scenario["steps"].as_array().map_or(0, |s| {
            s.iter().filter(|s| s.get("value_sha256").is_some()).count()
        });
        assert_eq!(outcome.hashes, expected, "{name}: hashes");
        hashes += outcome.hashes;
        let full: Vec<String> = scenario["full_transcript"]
            .as_array()
            .ok_or("full_transcript")?
            .iter()
            .map(canonical::to_string)
            .collect();
        for (conn, peer) in &peers {
            let held: Vec<String> = peer
                .client
                .transcript()
                .entries()
                .iter()
                .map(|e| canonical::to_string(e.value()))
                .collect();
            assert_eq!(held, full, "{name} {conn}: transcript");
            assert!(
                peer.client.transcript().gaps().is_empty(),
                "{name} {conn}: gaps"
            );
            assert_eq!(
                peer.client.transcript().earlier_bound(),
                None,
                "{name} {conn}: earlier"
            );
            connections += 1;
        }
        let first = peers.get("a").ok_or("connection a")?;
        // After a restart the live view is the last attachment's, which is final_value.
        assert_eq!(
            canonical::to_string(first.client.view().value()),
            canonical::to_string(&scenario["final_value"]),
            "{name}: final value"
        );
        match name.as_str() {
            "fill-gap" => assert_eq!((outcome.restarts, outcome.reads), (1, 2)),
            "load-earlier" => assert_eq!((outcome.restarts, outcome.reads), (0, 3)),
            _ => {}
        }
    }
    println!(
        "caching client: {connections} connections end at full_transcript, {hashes} view hashes"
    );
    Ok(())
}
