use super::*;
#[test]
fn managed_projection_is_small_scoped_and_does_not_touch_original() -> Result<()> {
    let mut image = Cursor::new(Vec::new());
    image::DynamicImage::new_rgb8(1200, 900).write_to(&mut image, image::ImageFormat::Png)?;
    let mut bytes = image.into_inner();
    bytes.resize(1024 * 1024, 0);
    let hash = hex::encode(Sha256::digest(&bytes));
    let block = json!({"type":"image","source":{"type":"base64","data":STANDARD.encode(&bytes),"mediaType":"image/png"}});
    let original = json!({"tool_output":[{"type":"text","text":"leave this text alone"},block.clone(),block.clone(),block]});
    let mut projected = original.clone();
    project_for_session(&mut projected, "public-a")?;
    assert!(serde_json::to_vec(&projected)?.len() < 16 * 1024);
    assert_eq!(projected["tool_output"][0]["text"], "leave this text alone");
    assert_eq!(
        projected["tool_output"][1]["media_ref"]["session_id"],
        "public-a"
    );
    assert_eq!(projected["tool_output"][1]["media_ref"]["sha256"], hash);
    assert_eq!(
        projected["tool_output"].as_array().context("blocks")?.len(),
        2
    );
    assert_eq!(find_image(&original, &hash)?.context("original")?, bytes);
    assert!(find_image(&projected, &hash)?.is_none());
    projected["tool_output"][1]["media_ref"]["session_id"] = "foreign".into();
    project_for_session(&mut projected, "public-a")?;
    assert_eq!(
        projected["tool_output"][1]["media_ref"]["session_id"],
        "public-a"
    );
    Ok(())
}
