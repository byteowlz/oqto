use super::*;

#[test]
fn eavs_endpoint_config_separates_local_and_container_inference() -> anyhow::Result<()> {
    let config: EavsConfig = serde_json::from_value(serde_json::json!({
        "base_url":"http://127.0.0.1:3044/",
        "inference_url":"http://127.0.0.1:3033/",
        "container_url":"http://host.containers.internal:3033/"
    }))?;
    assert_eq!(config.base_url, "http://127.0.0.1:3044");
    assert_eq!(
        config.session_inference_url(true).as_deref(),
        Some("http://127.0.0.1:3033")
    );
    assert_eq!(
        config.session_inference_url(false).as_deref(),
        Some("http://host.containers.internal:3033")
    );
    let client = config.client("synthetic-operator")?;
    assert_eq!(client.inference_url(), config.inference_url());
    Ok(())
}

#[test]
fn eavs_endpoint_config_omission_preserves_existing_placement_semantics() -> anyhow::Result<()> {
    let config: EavsConfig =
        serde_json::from_value(serde_json::json!({"base_url":"http://localhost:3033"}))?;
    assert_eq!(
        config.session_inference_url(true).as_deref(),
        Some("http://localhost:3033")
    );
    assert_eq!(config.session_inference_url(false), None);
    for field in ["base_url", "inference_url", "container_url"] {
        for invalid in [
            "relative",
            "",
            "https://user:fixture-secret@gateway.example",
        ] {
            let mut value = serde_json::json!({"base_url":"http://localhost:3033"});
            value[field] = serde_json::json!(invalid);
            assert!(serde_json::from_value::<EavsConfig>(value).is_err());
        }
    }
    Ok(())
}
