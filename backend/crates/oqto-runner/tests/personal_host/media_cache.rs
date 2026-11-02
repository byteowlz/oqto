//! Invalid cache configuration refuses startup before binding/pairing.
use super::*;
#[tokio::test]
async fn media_cache_configuration_fails_closed() -> Result<()> {
    for (name, value, expected) in [
        ("OQTO_MEDIA_CACHE_ENABLED", "yes", "must be true or false"),
        ("OQTO_MEDIA_CACHE_MAX_BYTES", "0", "limits must be positive"),
        (
            "OQTO_MEDIA_CACHE_IDLE_TTL_SECS",
            "bad",
            "invalid media cache limit",
        ),
        (
            "OQTO_MEDIA_CACHE_DIR",
            "relative",
            "directory must be absolute",
        ),
    ] {
        let dir = tempfile::tempdir()?;
        let home = dir.path().join("home");
        let work = dir.path().join("work");
        std::fs::create_dir_all(home.join("tmp"))?;
        std::fs::create_dir_all(&work)?;
        let config = dir.path().join("config.toml");
        std::fs::write(
            &config,
            format!(
                "[local]\nsingle_user=true\nworkspace_dir={}\n[local.linux_users]\nenabled=false\n[pi]\nexecutable=\"/usr/bin/false\"\n",
                serde_json::to_string(&work)?
            ),
        )?;
        std::fs::set_permissions(&config, std::fs::Permissions::from_mode(0o600))?;
        let state = dir.path().join("state");
        let output = tokio::time::timeout(
            Duration::from_secs(10),
            command(&home)
                .env(name, value)
                .arg("--config")
                .arg(config)
                .args([
                    "--no-sandbox",
                    "--personal-listen",
                    "127.0.0.1:0",
                    "--personal-origin",
                    "https://127.0.0.1:39444",
                    "--personal-no-pair",
                    "--personal-state",
                ])
                .arg(&state)
                .output(),
        )
        .await??;
        assert!(!output.status.success());
        assert!(
            String::from_utf8_lossy(&output.stderr).contains(expected),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            !state.exists(),
            "invalid configuration must not create pairing authority"
        );
    }
    Ok(())
}
