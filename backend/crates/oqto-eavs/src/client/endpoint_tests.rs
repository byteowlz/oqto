use super::*;

#[test]
fn omitted_inference_endpoint_preserves_the_control_endpoint() -> EavsResult<()> {
    let client = EavsClient::new("http://127.0.0.1:3033/", "synthetic-operator")?;
    assert_eq!(client.base_url, "http://127.0.0.1:3033");
    assert_eq!(client.inference_url(), client.base_url);
    Ok(())
}

#[test]
fn split_endpoints_keep_operator_credentials_out_of_generated_models() -> EavsResult<()> {
    let client = EavsClient::with_endpoints(
        "http://127.0.0.1:3044/",
        "http://127.0.0.1:3033/",
        "synthetic-operator-secret",
    )?;
    let provider = serde_json::from_value(serde_json::json!({
        "name":"fixture", "type":"openai-compatible", "pi_api":"openai-completions",
        "oauth":false, "has_api_key":true, "models":[{"id":"fixture-model"}]
    }))
    .map_err(|error| EavsError::ParseError(error.to_string()))?;
    let generated = crate::generate_pi_models_json(
        &[provider],
        client.inference_url(),
        Some("eavs-synthetic-scoped"),
    );
    assert_eq!(client.base_url, "http://127.0.0.1:3044");
    assert_eq!(
        generated["providers"]["eavs-fixture"]["baseUrl"],
        "http://127.0.0.1:3033/fixture/v1"
    );
    assert!(!generated.to_string().contains("synthetic-operator-secret"));
    assert!(!format!("{client:?}").contains("synthetic-operator-secret"));
    Ok(())
}

#[test]
fn malformed_or_credential_bearing_endpoints_fail_without_echoing_secrets() {
    for invalid in [
        "",
        "relative",
        "ftp://gateway.example",
        "https://user:fixture-secret@gateway.example",
        "https://gateway.example/?token=fixture-secret",
        "https://gateway.example/#fixture-secret",
    ] {
        for result in [
            EavsClient::with_endpoints(invalid, "https://inference.example", "synthetic"),
            EavsClient::with_endpoints("https://operator.example", invalid, "synthetic"),
        ] {
            assert!(result.is_err());
            if let Err(error) = result {
                assert!(!error.to_string().contains("fixture-secret"));
            }
        }
    }
}

#[test]
fn configured_inference_endpoint_deserializes_and_validates() -> Result<(), serde_json::Error> {
    #[derive(serde::Deserialize)]
    struct Config {
        #[serde(default, deserialize_with = "crate::deserialize_optional_endpoint")]
        inference_url: Option<String>,
    }
    let absent: Config = serde_json::from_value(serde_json::json!({}))?;
    assert_eq!(absent.inference_url, None);
    let present: Config =
        serde_json::from_value(serde_json::json!({"inference_url":"https://gateway.example/"}))?;
    assert_eq!(
        present.inference_url.as_deref(),
        Some("https://gateway.example")
    );
    for invalid in [
        "",
        "relative",
        "https://user:fixture-secret@gateway.example",
    ] {
        assert!(
            serde_json::from_value::<Config>(serde_json::json!({"inference_url":invalid})).is_err()
        );
    }
    Ok(())
}
