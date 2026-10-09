use std::process::Command;

#[cfg(unix)]
#[test]
fn many_workspaces_import_under_a_bounded_file_descriptor_budget() {
    let root = tempfile::tempdir().unwrap();
    for i in 0..48 {
        let directory = root
            .path()
            .join(format!(".pi/agent/sessions/--workspace-{i:03}--"));
        std::fs::create_dir_all(&directory).unwrap();
        let id = format!("019fae41-0000-7000-a000-{i:012}");
        let file = directory.join(format!("2026-08-18T00-00-00-000Z_{id}.jsonl"));
        std::fs::write(file, format!("{{\"type\":\"session\",\"cwd\":\"/fixture/{i}\"}}\n{{\"type\":\"message\",\"id\":\"entry-{i}\",\"message\":{{\"role\":\"user\",\"content\":\"fixture\"}}}}\n")).unwrap();
    }
    let config = root.path().join("config.toml");
    std::fs::write(&config, format!("[local]\nsingle_user = true\n[local.linux_users]\nenabled = false\n[runner.history_read]\naccount_id = \"fixture\"\nhome = {}\n", serde_json::to_string(root.path()).unwrap())).unwrap();
    let output = Command::new("/bin/sh")
        .args([
            "-c",
            "ulimit -n 64; exec \"$1\" --config \"$2\" history-import --apply",
            "fixture",
        ])
        .arg(env!("CARGO_BIN_EXE_oqto-runner"))
        .arg(config)
        .output()
        .unwrap();
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(output.status.success(), "{report}");
    assert_eq!(report["imported_sessions"], 48, "{report}");
    assert_eq!(report["failed_files"], 0);
}

#[test]
fn maintenance_preview_and_errors_are_json_without_starting_a_daemon() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(root.path().join(".pi/agent/sessions")).unwrap();
    let config = root.path().join("config.toml");
    std::fs::write(&config, format!("[local]\nsingle_user = true\n[local.linux_users]\nenabled = false\n[runner.history_read]\naccount_id = \"fixture\"\nhome = {}\n", serde_json::to_string(root.path()).unwrap())).unwrap();
    let invoke = || {
        Command::new(env!("CARGO_BIN_EXE_oqto-runner"))
            .arg("--config")
            .arg(&config)
            .arg("history-import")
            .output()
            .unwrap()
    };
    let preview = invoke();
    assert!(preview.status.success());
    let report: serde_json::Value = serde_json::from_slice(&preview.stdout).unwrap();
    assert_eq!(report["mode"], "dry_run");
    assert!(!root.path().join(".local").exists());
    std::fs::write(&config, "[local]\nsingle_user = true\n").unwrap();
    let failed = invoke();
    assert!(!failed.status.success());
    let report: serde_json::Value = serde_json::from_slice(&failed.stdout).unwrap();
    assert_eq!(report["ok"], false);
}
