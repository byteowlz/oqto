use super::*;
use serde_json::{Value, json};

const FIXTURES: &str = include_str!("../../contracts/session-announce/v1/fixtures.json");

fn fixtures() -> Value {
    serde_json::from_str(FIXTURES).expect("fixtures parse")
}

fn line(value: &Value) -> Vec<u8> {
    let mut text = serde_json::to_string(value).expect("serialize");
    text.push('\n');
    text.into_bytes()
}

fn refusal(raw: &[u8], name: &str) -> AnnounceError {
    match decode_session_announce(raw) {
        Ok(_) => panic!("{name}: accepted"),
        Err(error) => error,
    }
}

#[test]
fn every_valid_fixture_decodes_and_reencodes() {
    for case in fixtures()["valid"].as_array().expect("valid array") {
        let name = case["name"].as_str().expect("name");
        let announce = decode_session_announce(&line(&case["announce"]))
            .unwrap_or_else(|error| panic!("{name}: {error}"));
        assert_eq!(announce.kind, SESSION_ANNOUNCE_KIND, "{name}");
        let round = encode_session_announce(&announce).expect("encode");
        let again = decode_session_announce(round.as_bytes()).expect("re-decode");
        assert_eq!(announce, again, "{name}");
    }
}

#[test]
fn every_valid_fixture_matches_its_ack() {
    for case in fixtures()["valid"].as_array().expect("valid array") {
        let name = case["name"].as_str().expect("name");
        let ack = decode_session_announce_ack(&line(&case["ack"]))
            .unwrap_or_else(|error| panic!("{name}: {error}"));
        let expected = match case["ack"]["result"].as_str().expect("result") {
            "listed" => AnnounceOutcome::Listed,
            "duplicate" => AnnounceOutcome::Duplicate,
            other => panic!("unexpected ack result {other}"),
        };
        assert_eq!(ack.result, expected, "{name}");
    }
}

#[test]
fn every_invalid_fixture_is_refused_with_its_code() {
    for case in fixtures()["invalid"].as_array().expect("invalid array") {
        let name = case["name"].as_str().expect("name");
        let raw = match case.get("raw").and_then(Value::as_str) {
            Some(raw) => raw.as_bytes().to_vec(),
            None => line(&case["announce"]),
        };
        let error = refusal(&raw, name);
        assert_eq!(error.code(), case["error"], "{name}");
    }
}

#[test]
fn oversized_announces_are_refused_before_parsing() {
    let raw = vec![b' '; MAX_SESSION_ANNOUNCE_BYTES + 1];
    assert_eq!(
        decode_session_announce(&raw).unwrap_err(),
        AnnounceError::TooLarge
    );
}

#[test]
fn announce_never_carries_authority() {
    let ack = SessionAnnounceAck::new(AnnounceOutcome::Listed, "");
    assert!(ack.ok);
    let denied = SessionAnnounceAck::new(AnnounceOutcome::Denied, "Native Workspace denied");
    assert!(!denied.ok);
    let parsed = decode_session_announce_ack(encode_session_announce_ack(&denied).as_bytes())
        .expect("denied ack decodes");
    assert_eq!(parsed, denied);
}

#[test]
fn ack_ok_must_match_its_result() {
    let lying = json!({"v":1,"ok":true,"result":"denied","detail":""});
    assert_eq!(
        decode_session_announce_ack(line(&lying).as_slice()).unwrap_err(),
        AnnounceError::Malformed
    );
}

#[test]
fn ack_detail_is_bounded() {
    let ack = SessionAnnounceAck::new(AnnounceOutcome::Denied, "x".repeat(4096));
    assert!(ack.detail.len() <= MAX_SESSION_ANNOUNCE_DETAIL);
}
