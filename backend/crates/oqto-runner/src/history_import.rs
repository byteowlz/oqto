//! Local operator maintenance, deliberately absent from the read-only RPC surface.
use crate::daemon::config::RunnerUserConfig;
use anyhow::{Context, Result, ensure};

pub async fn run(
    config: &RunnerUserConfig,
    apply: bool,
    max_imports: Option<usize>,
) -> Result<serde_json::Value> {
    ensure!(
        config.single_user && !config.linux_users_enabled,
        "Dedicated runner required"
    );
    let history = config
        .history_read
        .as_ref()
        .context("Configure runner.history_read first")?;
    ensure!(
        !history.account_id.is_empty() && history.home.is_absolute(),
        "Explicit history owner and home required"
    );
    let source = history.home.join(".pi/agent/sessions");
    std::fs::read_dir(&source).context("Native Pi session source unavailable")?;
    if !apply {
        return Ok(
            serde_json::json!({"schema":1,"ok":true,"mode":"dry_run","source":source,"destination_home":history.home,"pi_files_modified":false,"next":"Back up canonical stores, then repeat with --apply"}),
        );
    }
    let stats = oqto_history::oqto_log::importer::bootstrap_import_limited(
        &history.home,
        &history.account_id,
        max_imports,
    )
    .await?;
    let identities = if max_imports.is_none() && stats.failed_files == 0 {
        oqto_history::oqto_log::importer::fast_import_identities_from_pi_jsonl(
            &history.home,
            &history.account_id,
            None,
        )
        .await?
    } else {
        Default::default()
    };
    Ok(
        serde_json::json!({"schema":1,"ok":stats.failed_files == 0 && identities.failed_files == 0,"mode":"apply","scanned_files":stats.scanned_files,"imported_sessions":stats.imported_sessions,"imported_messages":stats.imported_messages,"skipped_files":stats.skipped_files,"failed_files":stats.failed_files + identities.failed_files,"failure_categories":stats.failure_categories,"pi_files_modified":false}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::history_read::HistoryReadConfig;
    #[tokio::test]
    async fn applying_twice_preserves_native_files_and_public_identity() {
        let home = tempfile::tempdir().unwrap();
        let source = home.path().join(".pi/agent/sessions/--workspace--");
        std::fs::create_dir_all(&source).unwrap();
        let id = "019fae41-0000-7000-a000-000000000099";
        let file = source.join(format!("2026-08-18T00-00-00-000Z_{id}.jsonl"));
        let content = "{\"type\":\"session\",\"cwd\":\"/workspace\"}\n{\"type\":\"message\",\"id\":\"entry-one\",\"message\":{\"role\":\"user\",\"content\":\"Native fixture\"}}\n";
        std::fs::write(&file, content).unwrap();
        let credentials = home.path().join(".pi/agent/auth.json");
        std::fs::write(&credentials, "untouched fixture").unwrap();
        let config = RunnerUserConfig {
            single_user: true,
            history_read: Some(HistoryReadConfig {
                account_id: "owner".into(),
                home: home.path().to_owned(),
            }),
            ..Default::default()
        };
        assert_eq!(
            run(&config, true, None).await.unwrap()["imported_sessions"],
            1
        );
        assert_eq!(
            run(&config, true, None).await.unwrap()["imported_sessions"],
            0
        );
        let rows = oqto_history::oqto_log::ops::list_sessions(home.path(), None)
            .await
            .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(
            rows[0].platform_id,
            oqto_history::oqto_log::store::platform_id_for_external_id(id)
        );
        assert_eq!(std::fs::read_to_string(file).unwrap(), content);
        assert_eq!(
            std::fs::read_to_string(credentials).unwrap(),
            "untouched fixture"
        );
    }

    #[tokio::test]
    async fn preview_does_not_create_canonical_state() {
        let home = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(home.path().join(".pi/agent/sessions")).unwrap();
        let config = RunnerUserConfig {
            single_user: true,
            history_read: Some(HistoryReadConfig {
                account_id: "owner".into(),
                home: home.path().to_owned(),
            }),
            ..Default::default()
        };
        let report = run(&config, false, None).await.unwrap();
        assert_eq!(report["mode"], "dry_run");
        assert!(!home.path().join(".local").exists());
    }
}
