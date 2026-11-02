use super::*;
use base64::{Engine, engine::general_purpose::STANDARD};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::io::{Cursor, Write};

fn png(size: usize) -> Result<Vec<u8>> {
    let mut data = Cursor::new(Vec::new());
    image::DynamicImage::new_rgb8(1200, 900).write_to(&mut data, image::ImageFormat::Png)?;
    let mut bytes = data.into_inner();
    bytes.resize(size.max(bytes.len()), 0);
    Ok(bytes)
}

#[test]
fn screenshots_project_to_small_portable_refs_without_mutating_source() -> Result<()> {
    let bytes = png(1024 * 1024)?;
    let image = json!({"type":"image","mimeType":"image/png","data":STANDARD.encode(&bytes)});
    let original =
        json!({"messages":[{"parts":[{"tool_output":[image.clone(),image.clone(),image]}]}]});
    let raw = serde_json::to_vec(&original)?;
    assert!(raw.len() > 4 * 1024 * 1024);
    let mut projected = original.clone();
    project(&mut projected)?;
    assert!(
        serde_json::to_vec(&projected)?.len() < 4096,
        "screenshot history exceeds transfer budget"
    );
    assert_eq!(serde_json::to_vec(&original)?, raw);
    let reference = &projected["messages"][0]["parts"][0]["tool_output"][0]["media_ref"];
    assert_eq!(reference["sha256"], hex::encode(Sha256::digest(&bytes)));
    assert_eq!(reference["width"], 1200);
    assert_eq!(reference["height"], 900);
    assert_eq!(reference["byte_size"], bytes.len());
    assert!(reference.get("path").is_none());
    assert!(reference.get("host_id").is_none());
    Ok(())
}

#[test]
fn original_and_rederived_thumbnail_are_bounded_and_foreign_hash_is_refused() -> Result<()> {
    let bytes = png(1024 * 1024)?;
    let hash = hex::encode(Sha256::digest(&bytes));
    let mut file = tempfile::NamedTempFile::new()?;
    writeln!(file, "{}", json!({"type":"session","id":"native"}))?;
    writeln!(
        file,
        "{}",
        json!({"type":"message","message":{"content":[{"type":"image","data":STANDARD.encode(&bytes)}]}})
    )?;
    let before = std::fs::read(file.path())?;
    let full = resolve(file.path(), &hash, false)?;
    assert_eq!(
        STANDARD.decode(full["data_base64"].as_str().context("data")?)?,
        bytes
    );
    let thumb = resolve(file.path(), &hash, true)?;
    assert_eq!(thumb["source_sha256"], hash);
    assert!(thumb["width"].as_u64().context("width")? <= 768);
    assert!(thumb["height"].as_u64().context("height")? <= 768);
    assert!(thumb["byte_size"].as_u64().context("size")? < 64 * 1024);
    assert_eq!(
        resolve(file.path(), &hash, true)?,
        thumb,
        "thumbnail must be rederivable without migrated cache"
    );
    assert!(resolve(file.path(), &"a".repeat(64), false).is_err());
    assert_eq!(std::fs::read(file.path())?, before);
    Ok(())
}

#[test]
fn extension_state_and_private_message_extras_are_not_media_sources() -> Result<()> {
    let bytes = png(1024 * 1024)?;
    let hash = hex::encode(Sha256::digest(&bytes));
    let image = json!({"type":"image","data":STANDARD.encode(&bytes)});
    let mut file = tempfile::NamedTempFile::new()?;
    writeln!(file, "{}", json!({"type":"session","id":"fixture"}))?;
    writeln!(
        file,
        "{}",
        json!({"type":"custom","data":{"private_image":image}})
    )?;
    writeln!(
        file,
        "{}",
        json!({"type":"message","message":{"role":"user","content":[{"type":"text","text":"visible text"}],"private_extra":image}})
    )?;
    assert!(
        resolve(file.path(), &hash, true).is_err(),
        "a hash is not permission to inspect private/control state"
    );
    Ok(())
}

#[test]
fn malformed_or_oversized_images_do_not_leak_inline_bytes() -> Result<()> {
    let mut data = json!({"type":"image","data":"not-base64"});
    project(&mut data)?;
    assert!(data.get("data").is_none());
    assert!(data.get("media_unavailable").is_some());
    let mut text = json!({"type":"text","text":"image data is not an image block"});
    let copy = text.clone();
    project(&mut text)?;
    assert_eq!(text, copy);
    Ok(())
}

#[tokio::test]
async fn media_is_resolved_only_within_the_pinned_admitted_session() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let root = dir.path().canonicalize()?;
    let home = root.join("service");
    let sessions = root.join("native/sessions");
    let work = root.join("work");
    let sockets = root.join("sockets");
    for path in [&home, &sessions, &work, &sockets] {
        std::fs::create_dir_all(path)?;
    }
    let source = crate::personal_source::PersonalSource::new(
        home.clone(),
        sessions.clone(),
        sockets,
        vec![work.clone()],
    )?;
    let bytes = png(1024 * 1024)?;
    let hash = hex::encode(Sha256::digest(&bytes));
    let make = |native: &str, data: &[u8]| -> Result<std::path::PathBuf> {
        let directory = sessions.join(oqto_pi::session_files::cwd_to_safe_dirname(&work));
        std::fs::create_dir_all(&directory)?;
        let path = directory.join(format!("fixture_{native}.jsonl"));
        std::fs::write(
            &path,
            format!(
                "{}\n{}\n",
                json!({"type":"session","id":native,"cwd":work}),
                json!({"type":"message","id":"entry","message":{"role":"user","content":[{"type":"image","data":STANDARD.encode(data)}]}})
            ),
        )?;
        Ok(path)
    };
    let first = make("media-native-a", &bytes)?;
    let second = make("media-native-b", &png(2 * 1024 * 1024)?)?;
    for path in [&first, &second] {
        let header = source.header(path)?;
        oqto_history::oqto_log::ops::native_source::discover(
            &home,
            "fixture",
            work.to_str().context("work")?,
            &header.external_id,
            &header.identity,
        )
        .await?;
    }
    let a = oqto_history::oqto_log::store::platform_id_for_external_id("media-native-a");
    let b = oqto_history::oqto_log::store::platform_id_for_external_id("media-native-b");
    let result = read(
        &home,
        std::slice::from_ref(&work),
        &source,
        &a,
        hash.clone(),
        true,
    )
    .await?;
    assert_eq!(result["source_sha256"], hash);
    assert!(
        read(
            &home,
            std::slice::from_ref(&work),
            &source,
            &b,
            hash.clone(),
            false
        )
        .await
        .is_err(),
        "hash is not a cross-Session capability"
    );
    // Replacement at the same path requires explicit re-admission even when
    // image bytes and the native id have not changed.
    let replacement = first.with_extension("replacement");
    std::fs::copy(&first, &replacement)?;
    std::fs::rename(replacement, &first)?;
    assert!(
        read(&home, std::slice::from_ref(&work), &source, &a, hash, false)
            .await
            .is_err()
    );
    Ok(())
}
