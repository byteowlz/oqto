use super::*;

#[tokio::test]
async fn bounded_streams_and_deadline_reap() {
    for script in ["yes x", "yes private >&2", "sleep 60"] {
        let mut command = tokio::process::Command::new("/bin/sh");
        command
            .args(["-c", script])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        // Test children have no descendants outside this disposable process group.
        command.process_group(0);
        let mut child = command.spawn().expect("fixture spawn");
        let pid = child.id().expect("fixture pid");
        let result = collect_operation(&mut child, None, Duration::from_millis(100)).await;
        assert!(result.is_err());
        assert!(child.try_wait().expect("wait").is_some());
        // Clean the shell fixture's sleep/yes descendants as well.
        unsafe {
            libc::kill(-(pid as i32), libc::SIGKILL);
        }
        assert!(!result.expect_err("bounded failure").contains("private"));
    }
}

#[test]
fn enforced_fixture() {
    let root = tempfile::tempdir_in("/var/tmp").expect("synthetic root outside private /tmp");
    let home = root.path().join("home");
    let data = home.join(".local/share");
    let runtime = root.path().join("runtime");
    let definition = data.join("oqto/app-artifacts/definitions/fixture");
    let work = home.join("work");
    for path in [
        &definition,
        &work,
        &runtime,
        &home.join(".config"),
        &data.join("oqto/credentials"),
        &data.join("kyz"),
    ] {
        std::fs::create_dir_all(path).expect("fixture directory");
        std::fs::write(path.join("marker"), "PRIVATE_FIXTURE_VALUE").expect("fixture marker");
    }
    std::fs::write(runtime.join("oqto-runner.sock"), "PRIVATE_CONTROL").expect("control fixture");
    std::fs::write(definition.join("public"), "PUBLIC_FIXTURE").expect("public fixture");
    let status = std::process::Command::new(std::env::current_exe().expect("test executable"))
        .args([
            "--exact",
            "daemon::server::handlers::apps::tests::enforced_fixture_child",
            "--ignored",
            "--nocapture",
        ])
        .env("HOME", &home)
        .env("XDG_DATA_HOME", &data)
        .env("XDG_CONFIG_HOME", home.join(".config"))
        .env("XDG_RUNTIME_DIR", &runtime)
        .env("APP_FIXTURE_ROOT", root.path())
        .status()
        .expect("fixture subprocess");
    assert!(
        status.success(),
        "enforced sandbox proof must not skip unavailable bwrap"
    );
}

#[test]
#[ignore = "invoked by enforced_fixture in a synthetic environment"]
fn enforced_fixture_child() {
    let root = PathBuf::from(std::env::var_os("APP_FIXTURE_ROOT").expect("fixture only"));
    let home = root.join("home");
    let data = home.join(".local/share");
    let definition = data.join("oqto/app-artifacts/definitions/fixture");
    let work = home.join("work");
    let base = oqto_sandbox::SandboxConfig::from_profile("minimal");
    assert!(operation_sandbox(None, &home, &data, &definition, &work).is_err());
    let mut disabled = base.clone();
    disabled.enabled = false;
    assert!(operation_sandbox(Some(&disabled), &home, &data, &definition, &work).is_err());
    let config =
        operation_sandbox(Some(&base), &home, &data, &definition, &work).expect("confined policy");
    let script = format!(
        "set -eu; test \"$(cat '{}/public')\" = PUBLIC_FIXTURE; ! touch '{}/new'; for p in '{}/.config/marker' '{}/oqto/credentials/marker' '{}/kyz/marker' '{}/runtime/oqto-runner.sock' '{}/marker'; do if cat \"$p\" 2>/dev/null; then exit 91; fi; done; printf 'ok\\n'",
        definition.display(),
        definition.display(),
        home.display(),
        data.display(),
        data.display(),
        root.display(),
        work.display()
    );
    let mut command = oqto_sandbox::build_sandbox_command(
        &config,
        &work,
        Path::new("/bin/sh"),
        &["-c".into(), script],
        oqto_sandbox::SandboxStdin::Redirected,
    )
    .expect("sandbox builder");
    oqto_sandbox::configure_bwrap_pre_exec(&mut command, &config, &work, None)
        .expect("sandbox hooks");
    let output = command.output().expect("enforced probe must execute");
    assert!(
        output.status.success(),
        "enforced positive/negative probe failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.stdout, b"ok\n");
}
