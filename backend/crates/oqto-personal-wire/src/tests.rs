use super::*;
const HOST: &str = "00000000-0000-4000-8000-000000000001";
fn qr() -> Result<PairingQr> {
    PairingQr::new(
        HOST,
        "https://studio.local:7443",
        b"fixture-leaf",
        "a".repeat(64),
        1300,
        1000,
    )
}

#[test]
fn personal_qr_roundtrip_binds_origin_host_leaf_and_expiry() -> Result<()> {
    let qr = qr()?;
    let parsed = PairingQr::parse(&qr.to_json()?, 1001)?;
    PairingQr::parse(&qr.to_json()?, 940)?; // Up to 60 seconds behind the issuer.
    assert_eq!(parsed.host_id(), HOST);
    assert_eq!(parsed.origin(), "https://studio.local:7443/");
    assert_eq!(parsed.expires_at(), 1300);
    assert_eq!(parsed.enrollment_token(), "a".repeat(64));
    assert!(parsed.matches_leaf(b"fixture-leaf"));
    assert!(!parsed.matches_leaf(b"different-leaf"));
    assert!(PairingQr::parse(&qr.to_json()?, 1300).is_err());
    Ok(())
}

#[test]
fn bad_origins_are_not_silently_reinterpreted() {
    for origin in [
        "http://studio.local",
        "file:///tmp/socket",
        "https://user:password@studio.local",
        "https://studio.local/api",
        "https://studio.local/?token=secret",
        "https://studio.local/#secret",
        "https://studio.local/\n",
        " https://studio.local",
        "https://",
        "https://studio.local:invalid",
    ] {
        assert!(
            validate_origin(origin).is_err(),
            "unexpected origin acceptance"
        );
    }
    assert!(validate_origin(&format!("https://{}.local", "a".repeat(1024))).is_err());
    assert!(validate_origin("https://[::1]:7443/").is_ok());
}

#[test]
fn wrong_purpose_version_identity_pin_token_clock_and_size_fail_closed() -> Result<()> {
    let original: serde_json::Value = serde_json::from_slice(&qr()?.to_json()?)?;
    for (field, value) in [
        ("purpose", serde_json::json!("xlatch")),
        ("version", serde_json::json!(2)),
        ("host_id", serde_json::json!("not-a-host-id")),
        ("pin", serde_json::json!("f".repeat(63))),
        ("pin", serde_json::json!("F".repeat(64))),
        ("token", serde_json::json!("g".repeat(64))),
        ("expires_at", serde_json::json!(1000)),
        ("expires_at", serde_json::json!(1361)),
        ("expires_at", serde_json::json!(i64::MAX)),
        ("grants", serde_json::json!({"terminal":true})),
    ] {
        let mut altered = original.clone();
        altered[field] = value;
        assert!(PairingQr::parse(&serde_json::to_vec(&altered)?, 1000).is_err());
    }
    assert!(PairingQr::parse(&vec![b' '; MAX_QR_BYTES + 1], 1000).is_err());
    assert!(
        PairingQr::new(
            HOST,
            "https://studio.local",
            b"",
            "a".repeat(64),
            1300,
            1000
        )
        .is_err()
    );
    assert!(PairingQr::parse(&qr()?.to_json()?, -1).is_err());
    Ok(())
}

#[test]
fn enrollment_input_is_bounded_and_rejects_client_grants() -> Result<()> {
    let enrollment = serde_json::json!({"token":"a".repeat(64),"name":"Phone","public_key":"fixture","signature":"fixture"});
    decode_enrollment(&serde_json::to_vec(&enrollment)?)?;
    let mut altered = enrollment;
    altered["grants"] = serde_json::json!({"full_user_terminal":true});
    assert!(decode_enrollment(&serde_json::to_vec(&altered)?).is_err());
    assert!(decode_enrollment(&vec![b' '; MAX_ENROLLMENT_BYTES + 1]).is_err());
    Ok(())
}

#[cfg(feature = "tls")]
#[test]
fn tls_pin_configuration_has_no_early_data_or_invalid_pin_fallback() -> Result<()> {
    for invalid in ["", "not-a-pin", &"A".repeat(64)] {
        assert!(crate::tls::pinned_client_config(invalid).is_err());
    }
    let config = crate::tls::pinned_client_config(&"a".repeat(64))?;
    assert!(!config.enable_early_data);
    assert_eq!(config.alpn_protocols, vec![b"http/1.1".to_vec()]);
    Ok(())
}

#[cfg(feature = "qr")]
#[test]
fn renders_terminal_and_svg_without_image_dependencies() -> Result<()> {
    let qr = qr()?;
    assert!(!qr.render_terminal()?.is_empty());
    assert!(qr.render_svg()?.contains("<svg"));
    Ok(())
}
