use super::*;
use sha2::{Digest, Sha256};

fn release(dir: &Path, files: &[(&str, &[u8])], hashes: &[(&str, &str)]) -> PathBuf {
    std::fs::create_dir_all(dir).unwrap();
    for (name, bytes) in files {
        std::fs::write(dir.join(name), bytes).unwrap();
    }
    let mut entries = String::new();
    for (key, file) in hashes {
        let sum = std::fs::read(dir.join(file))
            .map(|b| hex::encode(Sha256::digest(b)))
            .unwrap_or_else(|_| key.to_string());
        entries.push_str(&format!(
            "\"{key}\": {{\"file\": \"{file}\", \"sha256\": \"{sum}\"}},"
        ));
    }
    std::fs::write(
        dir.join("manifest.json"),
        format!(
            "{{\"release\":\"fixture-r1\",\"threshold\":0.5,\"classifiers\":[\"hey_oqto\"],\"files\":{{{}}}}}",
            entries.trim_end_matches(',')
        ),
    )
    .unwrap();
    dir.to_path_buf()
}

fn good(dir: &Path) -> PathBuf {
    release(
        dir,
        &[
            ("mel.onnx", b"mel"),
            ("embedding.onnx", b"emb"),
            ("hey_oqto.onnx", b"head"),
        ],
        &[
            ("mel", "mel.onnx"),
            ("embedding", "embedding.onnx"),
            ("hey_oqto", "hey_oqto.onnx"),
        ],
    )
}

#[test]
fn a_verified_release_exposes_names_and_hashes_but_no_paths() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = good(tmp.path());
    let manifest = load_verified(&dir).unwrap();
    let public = public(&manifest);
    assert_eq!(public.release, "fixture-r1");
    assert_eq!(public.classifiers, vec!["hey_oqto".to_string()]);
    assert!(public.files.contains_key("mel") && public.files.contains_key("hey_oqto"));
    let json = serde_json::to_string(&public).unwrap();
    assert!(
        !json.contains(".onnx"),
        "public manifest leaks file names: {json}"
    );
    assert!(!json.contains(tmp.path().to_str().unwrap()));
    assert_eq!(
        read_verified(&dir, &manifest, "hey_oqto").unwrap(),
        b"head".to_vec()
    );
}

#[test]
fn a_tampered_file_refuses_the_whole_release() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = good(tmp.path());
    std::fs::write(dir.join("hey_oqto.onnx"), b"tampered").unwrap();
    let error = load_verified(&dir).unwrap_err().to_string();
    assert!(
        error.contains("does not match its manifest sha256"),
        "{error}"
    );
}

#[test]
fn unlisted_keys_and_unsafe_names_are_refused() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = good(tmp.path());
    let manifest = load_verified(&dir).unwrap();
    assert!(read_verified(&dir, &manifest, "unknown").is_err());

    let escaped = tempfile::tempdir().unwrap();
    let outside = escaped.path().join("secret.onnx");
    std::fs::write(&outside, b"x").unwrap();
    let dir = escaped.path().join("release");
    std::fs::create_dir(&dir).unwrap();
    let mut bad = manifest.clone();
    bad.files.get_mut("mel").unwrap().file = "../secret.onnx".into();
    assert!(read_verified(&dir, &bad, "mel").is_err());

    let mut symlinked = manifest.clone();
    symlinked.files.get_mut("mel").unwrap().file = "link.onnx".into();
    std::os::unix::fs::symlink(&outside, dir.join("link.onnx")).unwrap();
    assert!(read_verified(&dir, &symlinked, "mel").is_err());
}

#[test]
fn a_manifest_without_a_front_end_or_ranges_is_refused() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("release");
    std::fs::create_dir(&dir).unwrap();
    std::fs::write(
        dir.join("manifest.json"),
        "{\"release\":\"bad\",\"threshold\":1.5,\"classifiers\":[],\"files\":{}}",
    )
    .unwrap();
    assert!(load_verified(&dir).is_err());
    std::fs::write(
        dir.join("manifest.json"),
        "{\"release\":\"bad\",\"threshold\":0.5,\"classifiers\":[],\"files\":{}}",
    )
    .unwrap();
    assert!(load_verified(&dir).is_err());
}
