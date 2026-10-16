//! Opt-in fixture only. The launcher owns a fresh offline Pi process/profile;
//! this test never discovers the operator's sockets, profile or sessions.
use super::*;

#[derive(Deserialize)]
struct NativeFixture {
    kind: String,
    external_id: String,
    session_file: PathBuf,
    pid: u32,
}

fn fixture_authority() -> Result<Arc<Authority>> {
    let root = PathBuf::from(std::env::var("OQTO_NATIVE_TUI_FIXTURE")?)
        .canonicalize()
        .context("native fixture root unavailable")?;
    ensure!(
        root.file_name()
            .is_some_and(|name| name.to_string_lossy().starts_with("oqto-pi-smoke-")),
        "Not a disposable native fixture"
    );
    let meta = std::fs::metadata(&root)?;
    ensure!(
        meta.uid() == rustix::process::geteuid().as_raw() && meta.mode() & 0o077 == 0,
        "Native fixture must be private and same-Principal"
    );
    let fixture: NativeFixture =
        serde_json::from_slice(&std::fs::read(root.join("fixture.json"))?)?;
    ensure!(
        fixture.kind == "oqto-native-observation-v1",
        "Wrong fixture kind"
    );
    ensure!(
        root.join("sockets")
            .join(format!("pi-tui-rpc-{}.sock", fixture.pid))
            .exists(),
        "Fixture-owned Pi socket unavailable"
    );
    Ok(Arc::new(Authority {
        binding: ObservationBinding {
            account_id: "fixture-account".into(),
            public_session_id: "fixture-public-session".into(),
            external_session_id: fixture.external_id,
            session_file: fixture.session_file,
            session_root: root.join("sessions"),
            work_directory: root.join("work"),
            workspace_root: root.join("work"),
            socket_root: root.join("sockets"),
            grant_generation: "fixture-generation".into(),
        },
        active: AtomicBool::new(true),
    }))
}

#[tokio::test]
#[ignore = "run just test-pi-tui-rpc-native with an explicit extension path"]
async fn disposable_real_tui_observation_is_read_only_and_reconnect_is_explicit() -> Result<()> {
    let authority = fixture_authority()?;
    let before = std::fs::read(&authority.binding.session_file)?;
    let mut observer = TuiObserver::attach(
        authority.clone(),
        "fixture-account",
        "fixture-public-session",
    )
    .await?;
    assert_eq!(observer.public_session_id(), "fixture-public-session");
    assert!(!observer.initial_state().streaming);
    assert!(!observer.initial_state().compacting);
    assert!(!observer.containment_verified());
    // No prompts/events are produced. A deadline poisons only our connection.
    assert!(
        observer
            .next_event(Duration::from_millis(50))
            .await
            .is_err()
    );
    assert!(
        observer
            .next_event(Duration::from_millis(50))
            .await
            .is_err()
    );
    drop(observer);
    let observer = TuiObserver::attach(
        authority.clone(),
        "fixture-account",
        "fixture-public-session",
    )
    .await?;
    drop(observer);
    assert_eq!(std::fs::read(&authority.binding.session_file)?, before);
    Ok(())
}
