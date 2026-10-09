//! One read-only immutable Node/Pi contract shared by activation and doctor.
use anyhow::{Context, Result, ensure};
use sha2::{Digest, Sha256};
use std::{fs, path::Path};

fn bounded_json(path: &Path) -> Result<serde_json::Value> {
    let metadata = fs::symlink_metadata(path)
        .with_context(|| format!("inspect runtime contract {}", path.display()))?;
    ensure!(
        metadata.is_file() && metadata.len() <= 2_097_152,
        "runtime contract must be a bounded regular file: {}",
        path.display()
    );
    serde_json::from_slice(&fs::read(path)?).context("parse runtime contract JSON")
}

fn stable_version(value: &str) -> bool {
    let components: Vec<_> = value.split('.').collect();
    components.len() == 3
        && components.iter().all(|part| {
            !part.is_empty()
                && (part.len() == 1 || !part.starts_with('0'))
                && part.bytes().all(|byte| byte.is_ascii_digit())
        })
}

/// Checks declarations and required immutable files without executing Pi,
/// opening SQLite, changing ownership, or writing user/config state.
pub fn validate(release_dir: &Path) -> Result<()> {
    let manifest_path = release_dir.join("manifest.toml");
    let metadata = fs::symlink_metadata(&manifest_path)?;
    ensure!(
        metadata.is_file() && metadata.len() <= 1_048_576,
        "release manifest must be a bounded regular file"
    );
    let manifest: toml::Value = fs::read_to_string(&manifest_path)?
        .parse()
        .context("parse runtime manifest")?;
    let release = manifest
        .get("release")
        .context("missing release runtime contract")?;
    let field = |name: &str| -> Result<&str> {
        release
            .get(name)
            .and_then(toml::Value::as_str)
            .with_context(|| format!("missing release.{name}"))
    };
    ensure!(
        field("pi_runtime")? == "bundled-official-node-npm",
        "unsupported full release runtime mode"
    );
    let pi = field("pi_version")?;
    let node = field("node_version")?;
    ensure!(
        stable_version(pi) && stable_version(node),
        "runtime versions must be exact stable pins"
    );
    let root = release_dir.join("immutable/pi-runtime/pi");
    let receipt = bounded_json(&root.join("oqto-runtime.json"))?;
    ensure!(
        receipt["kind"] == "node-npm"
            && receipt["version"] == pi
            && receipt["node_version"] == node,
        "runtime receipt/version disagrees with release manifest"
    );
    for (manifest_name, receipt_name) in [
        ("node_archive_sha256", "node_archive_sha256"),
        ("pi_npm_integrity", "pi_npm_integrity"),
        ("pi_package_lock_sha256", "package_lock_sha256"),
    ] {
        ensure!(
            receipt[receipt_name].as_str() == Some(field(manifest_name)?),
            "runtime provenance disagrees with release.{manifest_name}"
        );
    }
    let lock_path = root.join("package-lock.json");
    let lock = bounded_json(&lock_path)?;
    let digest = format!("{:x}", Sha256::digest(fs::read(&lock_path)?));
    ensure!(
        digest == field("pi_package_lock_sha256")?,
        "runtime package lock hash mismatch"
    );
    let package_name = "@earendil-works/pi-coding-agent";
    ensure!(
        lock["packages"][""]["dependencies"][package_name] == pi,
        "runtime root dependency is not the exact Pi pin"
    );
    let sdk_key = format!("node_modules/{package_name}");
    ensure!(
        lock["packages"][&sdk_key]["version"] == pi
            && lock["packages"][&sdk_key]["integrity"] == field("pi_npm_integrity")?,
        "locked SDK version/integrity mismatch"
    );
    let package = bounded_json(&root.join("package.json"))?;
    let sdk = bounded_json(&root.join(&sdk_key).join("package.json"))?;
    ensure!(
        package["dependencies"][package_name] == pi && sdk["version"] == pi,
        "installed SDK/package declaration disagrees with exact Pi pin"
    );
    for file in [
        "pi",
        "node/bin/node",
        "node_modules/@earendil-works/pi-coding-agent/dist/bundle/cli.js",
        "node_modules/@earendil-works/pi-coding-agent/dist/modes/interactive/theme/dark.json",
        "node_modules/@earendil-works/pi-coding-agent/dist/modes/interactive/theme/light.json",
    ] {
        let path = root.join(file);
        let metadata =
            fs::symlink_metadata(&path).with_context(|| format!("missing runtime file {file}"))?;
        ensure!(
            metadata.is_file() && metadata.len() > 0,
            "runtime file must be nonempty and regular: {file}"
        );
        #[cfg(unix)]
        if file == "pi" || file == "node/bin/node" {
            use std::os::unix::fs::PermissionsExt;
            ensure!(
                metadata.permissions().mode() & 0o111 != 0,
                "runtime entrypoint not executable: {file}"
            );
        }
    }
    Ok(())
}
