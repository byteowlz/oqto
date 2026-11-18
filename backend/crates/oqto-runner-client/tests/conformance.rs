//! Conformance: the shared corpus, executed against this consumer's codecs.
//!
//! The bundle is read from `OQTO_RUNTIME_INTERACTIONS_BUNDLE` when set, so a
//! vendored consumer runs the same assertions against its own copy.

use oqto_runner_client::contract::{
    MAX_RUNTIME_FRAME_BYTES, RuntimeEvent, RuntimeRequest, RuntimeResponse, decode_runtime_event,
    decode_runtime_request, decode_runtime_response, decode_runtime_source_frame,
};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

fn bundle_dir() -> PathBuf {
    std::env::var_os(oqto_runner_client::BUNDLE_ENV)
        .map(PathBuf::from)
        .unwrap_or_else(oqto_runner_client::default_bundle_dir)
}

fn read_json(path: &Path) -> Value {
    let bytes = std::fs::read(path).expect("read bundle file");
    serde_json::from_slice(&bytes).expect("parse bundle json")
}

fn fixtures() -> Value {
    read_json(&bundle_dir().join("fixtures.json"))
}

fn source_fixtures() -> Value {
    read_json(&bundle_dir().join("source-fixtures.json"))
}

fn bytes(value: &Value) -> Vec<u8> {
    serde_json::to_vec(value).expect("serialize fixture")
}

#[test]
fn bundle_fixture_bytes_match_the_manifest() {
    let manifest = read_json(&bundle_dir().join("manifest.json"));
    for name in ["fixtures.json", "source-fixtures.json"] {
        let expected = manifest["files"][name]
            .as_str()
            .unwrap_or_else(|| panic!("manifest has no hash for {name}"));
        let digest = format!(
            "{:x}",
            Sha256::digest(std::fs::read(bundle_dir().join(name)).unwrap())
        );
        assert_eq!(digest, expected, "{name} drifted from the manifest");
    }
}

#[test]
fn every_request_roundtrips_and_covers_every_operation() {
    let fixtures = fixtures();
    let requests = fixtures["requests"].as_array().expect("requests");
    assert_eq!(requests.len(), 11);
    let mut operations = std::collections::BTreeSet::new();
    for case in requests {
        let request: RuntimeRequest =
            decode_runtime_request(&bytes(&case["value"])).expect("decode");
        assert_eq!(serde_json::to_value(&request).unwrap(), case["value"]);
        operations.insert(
            case["value"]["operation"]["op"]
                .as_str()
                .unwrap()
                .to_owned(),
        );
    }
    assert_eq!(operations.len(), 11, "corpus must cover all operations");
}

#[test]
fn every_response_and_event_decodes() {
    let fixtures = fixtures();
    for case in fixtures["responses"].as_array().expect("responses") {
        let response: RuntimeResponse =
            decode_runtime_response(&bytes(&case["value"])).expect("decode response");
        assert_eq!(serde_json::to_value(&response).unwrap(), case["value"]);
    }
    for case in fixtures["events"].as_array().expect("events") {
        let event: RuntimeEvent =
            decode_runtime_event(&bytes(&case["value"])).expect("decode event");
        assert_eq!(serde_json::to_value(&event).unwrap(), case["value"]);
    }
}

#[test]
fn the_rejection_corpus_is_refused() {
    let fixtures = fixtures();
    let invalid = fixtures["invalid"].as_array().expect("invalid");
    assert_eq!(invalid.len(), 20);
    for case in invalid {
        let payload = bytes(&case["value"]);
        let refused = match case["frame"].as_str().expect("frame") {
            "request" => decode_runtime_request(&payload).is_err(),
            "response" => decode_runtime_response(&payload).is_err(),
            "event" => decode_runtime_event(&payload).is_err(),
            other => panic!("unknown frame {other}"),
        };
        assert!(refused, "{} was accepted", case["name"]);
    }
}

#[test]
fn source_frames_correlate_and_reject_mismatches() {
    let fixtures = source_fixtures();
    let mut matched = 0;
    for case in fixtures["pairs"].as_array().expect("pairs") {
        let request: oqto_runner_client::contract::RuntimeSourceRequest =
            serde_json::from_value(case["request"].clone()).expect("request");
        let response: oqto_runner_client::contract::RuntimeSourceResponse =
            serde_json::from_value(case["response"].clone()).expect("response");
        assert!(response.matches_request(&request), "{}", case["name"]);
        matched += 1;
    }
    assert_eq!(matched, 11);
    for case in fixtures["invalid"].as_array().expect("invalid") {
        assert!(
            decode_runtime_source_frame(&bytes(&case["value"])).is_err(),
            "{} was accepted",
            case["name"]
        );
    }
    let mut mismatched = 0;
    for case in fixtures["mismatches"].as_array().expect("mismatches") {
        let request: oqto_runner_client::contract::RuntimeSourceRequest =
            serde_json::from_value(case["request"].clone()).expect("request");
        let response: oqto_runner_client::contract::RuntimeSourceResponse =
            serde_json::from_value(case["response"].clone()).expect("response");
        assert!(!response.matches_request(&request), "{}", case["name"]);
        mismatched += 1;
    }
    assert_eq!(mismatched, 9);
}

#[test]
fn frames_beyond_the_contract_bound_are_refused() {
    let oversized = vec![b' '; MAX_RUNTIME_FRAME_BYTES + 1];
    assert!(decode_runtime_request(&oversized).is_err());
    assert!(decode_runtime_response(&oversized).is_err());
    assert!(decode_runtime_event(&oversized).is_err());
    assert!(decode_runtime_source_frame(&oversized).is_err());
}
