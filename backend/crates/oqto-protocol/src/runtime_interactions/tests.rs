use super::*;
use serde_json::Value;

const GENERAL: &str = include_str!("fixtures.json");
const SOURCE: &str = include_str!("source-fixtures.json");

fn corpus(raw: &str) -> Value {
    serde_json::from_str(raw).expect("synthetic contract corpus")
}

fn roundtrip<T: Serialize>(case: &Value, decode: fn(&[u8]) -> Result<T, String>) -> T {
    let bytes = serde_json::to_vec(&case["value"]).expect("fixture bytes");
    let value = decode(&bytes).expect("valid fixture");
    assert_eq!(
        serde_json::to_value(&value).expect("roundtrip"),
        case["value"],
        "{}",
        case["name"]
    );
    value
}

#[test]
fn native_source_frames_roundtrip_and_preserve_all_operation_correlations() {
    let fixtures = corpus(SOURCE);
    for case in fixtures["frames"].as_array().expect("frames") {
        roundtrip(case, decode_runtime_source_frame);
    }
    for case in fixtures["pairs"].as_array().expect("pairs") {
        let request: RuntimeSourceRequest =
            serde_json::from_value(case["request"].clone()).expect("request");
        let response: RuntimeSourceResponse =
            serde_json::from_value(case["response"].clone()).expect("response");
        assert!(response.matches_request(&request), "{}", case["name"]);
    }
}

#[test]
fn source_frame_rejects_host_claims_and_valid_but_mismatched_results() {
    let fixtures = corpus(SOURCE);
    for case in fixtures["invalid"].as_array().expect("invalid") {
        let raw = serde_json::to_vec(&case["value"]).expect("bytes");
        assert!(
            decode_runtime_source_frame(&raw).is_err(),
            "{}",
            case["name"]
        );
    }
    for case in fixtures["mismatches"].as_array().expect("mismatches") {
        let request: RuntimeSourceRequest =
            serde_json::from_value(case["request"].clone()).expect("request");
        let response: RuntimeSourceResponse =
            serde_json::from_value(case["response"].clone()).expect("response");
        assert!(!response.matches_request(&request), "{}", case["name"]);
    }
    assert!(decode_runtime_source_frame(&vec![b' '; MAX_RUNTIME_FRAME_BYTES + 1]).is_err());
}

#[test]
fn canonical_requests_roundtrip_and_cover_cli_contract_paths() {
    let fixtures = corpus(GENERAL);
    let requests = fixtures["requests"].as_array().expect("requests");
    assert_eq!(requests.len(), 11);
    for case in requests {
        let request = roundtrip(case, decode_runtime_request);
        assert_eq!(
            serde_json::to_value(request.operation.cli_path()).expect("CLI mapping"),
            case["cli"]
        );
    }
}

#[test]
fn canonical_source_responses_and_events_roundtrip_without_private_payloads() {
    let fixtures = corpus(GENERAL);
    for case in fixtures["responses"].as_array().expect("responses") {
        roundtrip(case, decode_runtime_response);
    }
    for case in fixtures["events"].as_array().expect("events") {
        roundtrip(case, decode_runtime_event);
    }
}

#[test]
fn malformed_coerced_forged_and_cross_domain_frames_are_rejected() {
    let fixtures = corpus(GENERAL);
    for case in fixtures["invalid"].as_array().expect("invalid") {
        let value = case["value"].clone();
        let rejected = match case["frame"].as_str().expect("frame") {
            "request" => serde_json::from_value::<RuntimeRequest>(value).is_err(),
            "response" => serde_json::from_value::<RuntimeResponse>(value).is_err(),
            "event" => serde_json::from_value::<RuntimeEvent>(value).is_err(),
            other => panic!("unknown fixture frame {other}"),
        };
        assert!(rejected, "{}", case["name"]);
    }
}

#[test]
fn raw_arguments_are_not_parsed_trimmed_or_forwarded_as_a_prompt_operation() {
    let fixtures = corpus(GENERAL);
    let request: RuntimeRequest =
        serde_json::from_value(fixtures["requests"][2]["value"].clone()).expect("dispatch");
    let RuntimeOperation::CommandDispatch { args, .. } = request.operation else {
        panic!("dispatch fixture");
    };
    assert_eq!(args, "  --value='literal /name' \n");
    let oversized = vec![b' '; MAX_RUNTIME_FRAME_BYTES + 1];
    assert!(decode_runtime_request(&oversized).is_err());
    assert!(decode_runtime_response(&oversized).is_err());
    assert!(decode_runtime_event(&oversized).is_err());
}

#[test]
fn generated_types_use_the_actual_camelcase_wire_fields_and_literal_version() {
    let operation = RuntimeOperation::decl();
    assert!(operation.contains("catalogGeneration"), "{operation}");
    assert!(operation.contains("controllerGeneration"), "{operation}");
    let action = RuntimeQueueAction::decl();
    assert!(action.contains("itemId"), "{action}");
    assert!(
        RuntimeVersion::decl().contains("= 1"),
        "{}",
        RuntimeVersion::decl()
    );
    assert!(RuntimeRevision::decl().contains("number"));
    assert!(RuntimeGeneration::decl().contains("RuntimeRevision"));
    assert!(RuntimeGeneration::decl().contains("RuntimeId"));
}

#[test]
fn safe_revision_and_id_constructors_fail_closed() {
    assert!(RuntimeRevision::try_from(MAX_SAFE_REVISION).is_ok());
    assert!(RuntimeRevision::try_from(MAX_SAFE_REVISION + 1).is_err());
    for id in ["", "bad id", "../path", "λ"] {
        assert!(RuntimeId::try_from(id.to_owned()).is_err());
    }
    assert!(RuntimeId::try_from("fixture-runtime:one".to_owned()).is_ok());
    assert!(RuntimeDigest::try_from("a".repeat(64)).is_ok());
    assert!(RuntimeDigest::try_from("not-a-digest".to_owned()).is_err());
    assert!(RuntimeDigest::try_from("A".repeat(64)).is_err());
}
