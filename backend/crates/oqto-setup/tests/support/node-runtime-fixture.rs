// Synthetic immutable runtime declarations, NOT native runtime acceptance.
pub fn create(release: &std::path::Path) -> anyhow::Result<()> {
    use sha2::{Digest, Sha256};
    use std::os::unix::fs::PermissionsExt;
    let root = release.join("immutable/pi-runtime/pi");
    let sdk = root.join("node_modules/@earendil-works/pi-coding-agent");
    std::fs::create_dir_all(root.join("node/bin"))?;
    std::fs::create_dir_all(sdk.join("dist/bundle"))?;
    std::fs::create_dir_all(sdk.join("dist/modes/interactive/theme"))?;
    for name in ["pi", "node/bin/node"] {
        std::fs::write(root.join(name), "#!/bin/sh\nexit 0\n")?;
        std::fs::set_permissions(root.join(name), std::fs::Permissions::from_mode(0o755))?;
    }
    std::fs::write(sdk.join("dist/bundle/cli.js"), "// synthetic fixture\n")?;
    for name in ["dark.json", "light.json"] {
        std::fs::write(sdk.join("dist/modes/interactive/theme").join(name), "{}")?;
    }
    let package = serde_json::json!({"dependencies":{"@earendil-works/pi-coding-agent":"1.0.3"}});
    let integrity = "sha512-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA==";
    let lock = serde_json::json!({"packages": {"":{"dependencies":package["dependencies"]},
        "node_modules/@earendil-works/pi-coding-agent":{"version":"1.0.3","integrity":integrity}}});
    let bytes = serde_json::to_vec(&lock)?;
    let digest = format!("{:x}", Sha256::digest(&bytes));
    std::fs::write(root.join("package-lock.json"), bytes)?;
    std::fs::write(root.join("package.json"), serde_json::to_vec(&package)?)?;
    std::fs::write(sdk.join("package.json"), "{\"version\":\"1.0.3\"}")?;
    let node_hash = "a".repeat(64);
    let receipt = serde_json::json!({"kind":"node-npm","version":"1.0.3","node_version":"24.14.1",
        "node_archive_sha256":node_hash,"pi_npm_integrity":integrity,"package_lock_sha256":digest});
    std::fs::write(root.join("oqto-runtime.json"), serde_json::to_vec(&receipt)?)?;
    let manifest = release.join("manifest.toml");
    let mut text = std::fs::read_to_string(&manifest)?;
    text.push_str(&format!("pi_runtime = \"bundled-official-node-npm\"\npi_version = \"1.0.3\"\nnode_version = \"24.14.1\"\nnode_archive_sha256 = \"{node_hash}\"\npi_npm_integrity = \"{integrity}\"\npi_package_lock_sha256 = \"{digest}\"\n"));
    std::fs::write(manifest, text)?;
    Ok(())
}
