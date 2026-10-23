use super::*;

#[test]
fn native_head_requires_explicit_runtime_evidence_including_empty_head() -> Result<()> {
    let base = json!({"sessionId":"native","sessionFile":"/synthetic/source","cwd":"/synthetic"});
    assert_eq!(
        serde_json::from_value::<NativeState>(base.clone())?.head,
        NativeHead::Unavailable
    );
    let mut state = base;
    state["leafId"] = Value::Null;
    assert_eq!(
        serde_json::from_value::<NativeState>(state.clone())?.head,
        NativeHead::Empty
    );
    state["leafId"] = "actual-entry".into();
    assert_eq!(
        serde_json::from_value::<NativeState>(state.clone())?.head,
        NativeHead::Entry("actual-entry".into())
    );
    for invalid in [
        json!(true),
        json!(""),
        json!("../entry"),
        json!({"entry":"untrusted"}),
    ] {
        state["leafId"] = invalid;
        assert!(serde_json::from_value::<NativeState>(state.clone()).is_err());
    }
    Ok(())
}
use std::os::unix::fs::PermissionsExt;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use tokio::net::UnixListener;

struct Authority {
    binding: ObservationBinding,
    active: AtomicBool,
}
impl ObservationAuthority for Authority {
    fn resolve(&self, account: &str, session: &str) -> Result<ObservationBinding> {
        ensure!(
            account == self.binding.authority_id && session == self.binding.public_session_id,
            "denied"
        );
        self.revalidate(&self.binding)?;
        Ok(self.binding.clone())
    }
    fn revalidate(&self, binding: &ObservationBinding) -> Result<()> {
        ensure!(
            self.active.load(Ordering::SeqCst)
                && binding.grant_generation == self.binding.grant_generation,
            "revoked"
        );
        Ok(())
    }
}
struct Fixture {
    _temp: tempfile::TempDir,
    authority: Arc<Authority>,
    tasks: Vec<tokio::task::JoinHandle<()>>,
    connected: Arc<AtomicUsize>,
    disconnected: Arc<AtomicUsize>,
    commands: Arc<std::sync::Mutex<Vec<Value>>>,
    sentinel: Vec<u8>,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        for task in &self.tasks {
            task.abort();
        }
    }
}
impl Fixture {
    fn new() -> Result<Self> {
        // macOS's normal TMPDIR can exceed sockaddr_un's length once the
        // per-UID socket layout is appended. Keep the actual native proof short.
        let temp = tempfile::Builder::new()
            .prefix("oqtui-")
            .tempdir_in("/tmp")?;
        let root = temp.path().canonicalize()?;
        for name in ["sessions", "work", "sockets"] {
            std::fs::create_dir(root.join(name))?;
        }
        let file = root.join("sessions/native.jsonl");
        let sentinel = format!(
            "{}\n",
            json!({"type":"session","version":3,"id":"native-id","cwd":root.join("work")})
        )
        .into_bytes();
        std::fs::write(&file, &sentinel)?;
        let binding = ObservationBinding {
            authority_id: "alice".into(),
            public_session_id: "public-id".into(),
            external_session_id: "native-id".into(),
            session_file: file,
            session_root: root.join("sessions"),
            work_directory: root.join("work"),
            workspace_root: root.join("work"),
            socket_root: root.join("sockets"),
            grant_generation: "generation-1".into(),
        };
        Ok(Self {
            _temp: temp,
            authority: Arc::new(Authority {
                binding,
                active: AtomicBool::new(true),
            }),
            tasks: Vec::new(),
            connected: Arc::new(AtomicUsize::new(0)),
            disconnected: Arc::new(AtomicUsize::new(0)),
            commands: Arc::new(std::sync::Mutex::new(Vec::new())),
            sentinel,
        })
    }
    fn native_state(&self) -> Value {
        json!({"sessionId":"native-id","sessionFile":self.authority.binding.session_file,
            "cwd":self.authority.binding.work_directory,"isStreaming":true})
    }
    fn listen(
        &mut self,
        directory: bool,
        version: u64,
        state: Value,
        before: Vec<Value>,
        after: Vec<Value>,
    ) -> Result<()> {
        let root = &self.authority.binding.socket_root;
        let pid = std::process::id();
        let path = if directory {
            let dir = root.join(format!(
                "pi-tui-rpc-{}-{pid}",
                rustix::process::geteuid().as_raw()
            ));
            std::fs::create_dir(&dir)?;
            dir.join("control.sock")
        } else {
            root.join(format!("pi-tui-rpc-{pid}.sock"))
        };
        let listener = UnixListener::bind(&path)?;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
        let connected = self.connected.clone();
        let disconnected = self.disconnected.clone();
        let commands = self.commands.clone();
        self.tasks.push(tokio::spawn(async move {
            loop {
                let Ok((stream, _)) = listener.accept().await else { break; };
                connected.fetch_add(1, Ordering::SeqCst);
                let mut reader = BufReader::new(stream);
                let hello = json!({"type":"hello","v":version,"pid":pid,"lease":"tui","commands":["get_state","prompt","lease"]});
                let _ = send(&mut reader, &hello).await;
                if let Ok(command) = read_frame(&mut reader).await {
                    commands.lock().unwrap().push(command.clone());
                    for frame in &before { let _ = send(&mut reader, frame).await; }
                    let reply = json!({"type":"response","id":command["id"],"command":"get_state","success":true,"data":state});
                    let _ = send(&mut reader, &reply).await;
                    for frame in &after { let _ = send(&mut reader, frame).await; }
                    let mut line = String::new();
                    let _ = reader.read_line(&mut line).await;
                    assert!(line.is_empty(), "observer attempted a second command");
                }
                disconnected.fetch_add(1, Ordering::SeqCst);
            }
        }));
        Ok(())
    }
    async fn attach(&self) -> Result<TuiObserver> {
        TuiObserver::attach(self.authority.clone(), "alice", "public-id").await
    }
    fn unchanged(&self) -> Result<()> {
        ensure!(
            std::fs::read(&self.authority.binding.session_file)? == self.sentinel,
            "Pi file changed"
        );
        ensure!(
            self.commands
                .lock()
                .unwrap()
                .iter()
                .all(|c| c["type"] == "get_state"),
            "non-observation command issued"
        );
        Ok(())
    }
}
async fn send(reader: &mut BufReader<UnixStream>, frame: &Value) -> Result<()> {
    reader
        .get_mut()
        .write_all(&serde_json::to_vec(frame)?)
        .await?;
    reader.get_mut().write_all(b"\n").await?;
    Ok(())
}
fn delta() -> Value {
    json!({"type":"event","event":"message_update","data":{"private":"synthetic-transcript"},"ts":1})
}

#[tokio::test]
async fn both_socket_layouts_attach_without_history_lease_or_lifecycle_commands() -> Result<()> {
    for directory in [false, true] {
        let mut fixture = Fixture::new()?;
        fixture.listen(directory, 1, fixture.native_state(), vec![delta()], vec![])?;
        let mut observer = fixture.attach().await?;
        assert_eq!(observer.public_session_id(), "public-id");
        assert!(observer.initial_state().streaming);
        assert!(!observer.containment_verified());
        let event = observer.next_event(Duration::from_secs(1)).await?;
        assert_eq!(event.name, "message_update");
        assert_eq!(event.data["private"], "synthetic-transcript");
        assert!(!format!("{event:?}").contains("synthetic-transcript"));
        drop(observer);
        fixture.unchanged()?;
    }
    Ok(())
}

#[tokio::test]
async fn wrong_account_public_id_and_revoked_permit_deny_before_connect() -> Result<()> {
    let mut fixture = Fixture::new()?;
    fixture.listen(false, 1, fixture.native_state(), vec![], vec![])?;
    for (account, public) in [
        ("bob", "public-id"),
        ("alice", "native-id"),
        ("alice", "pending-x"),
    ] {
        assert!(
            TuiObserver::attach(fixture.authority.clone(), account, public)
                .await
                .is_err()
        );
    }
    fixture.authority.active.store(false, Ordering::SeqCst);
    assert!(fixture.attach().await.is_err());
    assert_eq!(fixture.connected.load(Ordering::SeqCst), 0);
    fixture.unchanged()
}

#[tokio::test]
async fn wrong_protocol_native_id_file_or_work_directory_never_adopt() -> Result<()> {
    for change in ["version", "native", "file", "cwd"] {
        let mut fixture = Fixture::new()?;
        let mut state = fixture.native_state();
        match change {
            "native" => state["sessionId"] = "different".into(),
            "file" => state["sessionFile"] = "/wrong".into(),
            "cwd" => state["cwd"] = "/wrong".into(),
            _ => {}
        }
        fixture.listen(
            false,
            if change == "version" { 0 } else { 1 },
            state,
            vec![],
            vec![],
        )?;
        assert!(fixture.attach().await.is_err(), "{change}");
        fixture.unchanged()?;
    }
    Ok(())
}

#[tokio::test]
async fn unauthorized_source_roots_and_wrong_headers_deny_before_socket_access() -> Result<()> {
    for wrong_header in [false, true] {
        let mut fixture = Fixture::new()?;
        if wrong_header {
            std::fs::write(
                &fixture.authority.binding.session_file,
                b"{\"type\":\"session\",\"id\":\"other\"}\n",
            )?;
            fixture.sentinel = std::fs::read(&fixture.authority.binding.session_file)?;
        } else {
            let authority = Arc::get_mut(&mut fixture.authority).unwrap();
            authority.binding.session_root = authority.binding.workspace_root.clone();
        }
        fixture.listen(false, 1, fixture.native_state(), vec![], vec![])?;
        assert!(fixture.attach().await.is_err());
        assert_eq!(fixture.connected.load(Ordering::SeqCst), 0);
        fixture.unchanged()?;
    }
    Ok(())
}

#[tokio::test]
async fn symlink_socket_and_directory_escape_never_connect() -> Result<()> {
    for directory in [false, true] {
        let fixture = Fixture::new()?;
        let outside = fixture._temp.path().join("outside");
        std::fs::create_dir(&outside)?;
        let path = outside.join("control.sock");
        let listener = UnixListener::bind(&path)?;
        let root = &fixture.authority.binding.socket_root;
        let pid = std::process::id();
        let (target, link) = if directory {
            (
                outside,
                root.join(format!(
                    "pi-tui-rpc-{}-{pid}",
                    rustix::process::geteuid().as_raw()
                )),
            )
        } else {
            (path, root.join(format!("pi-tui-rpc-{pid}.sock")))
        };
        std::os::unix::fs::symlink(target, link)?;
        assert!(fixture.attach().await.is_err());
        assert!(
            tokio::time::timeout(Duration::from_millis(20), listener.accept())
                .await
                .is_err()
        );
        fixture.unchanged()?;
    }
    Ok(())
}

#[tokio::test]
async fn duplicate_live_identity_is_not_resolved_by_visible_order() -> Result<()> {
    let mut fixture = Fixture::new()?;
    fixture.listen(false, 1, fixture.native_state(), vec![], vec![])?;
    fixture.listen(true, 1, fixture.native_state(), vec![], vec![])?;
    assert!(fixture.attach().await.is_err());
    fixture.unchanged()
}

#[tokio::test]
async fn revocation_during_silent_read_closes_attachment() -> Result<()> {
    let mut fixture = Fixture::new()?;
    fixture.listen(false, 1, fixture.native_state(), vec![], vec![])?;
    let mut observer = fixture.attach().await?;
    let authority = fixture.authority.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(20)).await;
        authority.active.store(false, Ordering::SeqCst);
    });
    assert!(
        tokio::time::timeout(
            Duration::from_secs(1),
            observer.next_event(Duration::from_secs(10))
        )
        .await?
        .is_err()
    );
    assert!(observer.reader.is_none());
    fixture.unchanged()
}

#[tokio::test]
async fn eof_and_read_deadline_close_without_automatic_reconnect() -> Result<()> {
    for eof in [false, true] {
        let mut fixture = Fixture::new()?;
        fixture.listen(false, 1, fixture.native_state(), vec![], vec![])?;
        let mut observer = fixture.attach().await?;
        if eof {
            fixture.tasks[0].abort();
        }
        assert!(
            observer
                .next_event(Duration::from_millis(10))
                .await
                .is_err()
        );
        assert!(observer.reader.is_none());
        assert!(
            observer
                .next_event(Duration::from_millis(10))
                .await
                .is_err()
        );
        assert_eq!(fixture.connected.load(Ordering::SeqCst), 1);
        fixture.unchanged()?;
    }
    Ok(())
}

#[tokio::test]
async fn cancelled_read_cannot_reuse_a_partially_consumed_connection() -> Result<()> {
    let mut fixture = Fixture::new()?;
    fixture.listen(false, 1, fixture.native_state(), vec![], vec![])?;
    let mut observer = fixture.attach().await?;
    assert!(
        tokio::time::timeout(
            Duration::from_millis(10),
            observer.next_event(Duration::from_secs(10))
        )
        .await
        .is_err()
    );
    assert!(
        observer.reader.is_none(),
        "cancelled observation kept its socket"
    );
    assert!(observer.next_event(Duration::from_secs(1)).await.is_err());
    fixture.unchanged()
}

#[tokio::test]
async fn session_switch_and_pending_overflow_require_readmission() -> Result<()> {
    for (before, after) in [
        (
            vec![],
            vec![json!({"type":"event","event":"session_start","data":{}})],
        ),
        (vec![delta(); MAX_PENDING + 1], vec![]),
    ] {
        let mut fixture = Fixture::new()?;
        fixture.listen(false, 1, fixture.native_state(), before, after)?;
        match fixture.attach().await {
            Ok(mut observer) => {
                assert!(observer.next_event(Duration::from_secs(1)).await.is_err());
                assert!(observer.reader.is_none());
            }
            Err(_) => assert!(fixture.commands.lock().unwrap().len() <= 1),
        }
        fixture.unchanged()?;
    }
    Ok(())
}

#[tokio::test]
async fn repeated_attach_drop_reaps_connections_without_touching_external_process() -> Result<()> {
    let mut fixture = Fixture::new()?;
    fixture.listen(false, 1, fixture.native_state(), vec![], vec![])?;
    for _ in 0..40 {
        drop(fixture.attach().await?);
    }
    tokio::time::timeout(Duration::from_secs(2), async {
        while fixture.disconnected.load(Ordering::SeqCst) != 40 {
            tokio::task::yield_now().await;
        }
    })
    .await?;
    assert_eq!(fixture.connected.load(Ordering::SeqCst), 40);
    assert_eq!(fixture.commands.lock().unwrap().len(), 40);
    fixture.unchanged()
}

#[tokio::test]
async fn oversized_and_unterminated_frames_are_bounded() -> Result<()> {
    for bytes in [
        vec![b'x'; FRAME_LIMIT as usize + 1],
        b"{\"type\":\"hello\"}".to_vec(),
    ] {
        let mut reader = BufReader::new(bytes.as_slice());
        assert!(read_frame(&mut reader).await.is_err());
    }
    Ok(())
}

#[test]
fn socket_names_and_peer_credentials_are_not_authority_shortcuts() {
    let uid = rustix::process::geteuid().as_raw();
    assert!(validate_peer(uid, uid.wrapping_add(1), None, 123).is_err());
    assert!(validate_peer(uid, uid, Some(124), 123).is_err());
    for name in [
        "pi-tui-rpc-0.sock",
        "pi-tui-rpc-2147483648.sock",
        "pi-tui-rpc-x.sock",
        "pi-tui-rpc-1-2-3",
    ] {
        assert!(socket_candidate(name, Path::new("/tmp/x"), uid).is_none());
    }
    assert!(
        socket_candidate(
            &format!("pi-tui-rpc-{}-123", uid.wrapping_add(1)),
            Path::new("/tmp/x"),
            uid
        )
        .is_none()
    );
}

#[tokio::test]
async fn catalog_liveness_reads_state_only_and_ignores_foreign_files() -> Result<()> {
    let mut fixture = Fixture::new()?;
    let mut state = fixture.native_state();
    state["sessionName"] = json!("  Release prep [red-fox-den] ");
    fixture.listen(false, 1, state, vec![delta()], vec![])?;
    let binding = &fixture.authority.binding;
    let live = live_sessions(&binding.socket_root, &binding.session_root).await;
    let session = live.get("native-id").context("live Pi missing")?;
    assert_eq!(live.len(), 1);
    assert_eq!(session.session_file, binding.session_file);
    assert_eq!(session.name.as_deref(), Some("Release prep [red-fox-den]"));
    assert!(session.streaming);
    fixture.unchanged()?;
    // A Pi whose file is outside the designated Session root is not listed.
    let elsewhere = live_sessions(&binding.socket_root, &binding.work_directory).await;
    assert!(elsewhere.is_empty());
    // No socket root, no live Sessions, no error.
    assert!(
        live_sessions(&binding.socket_root.join("missing"), &binding.session_root)
            .await
            .is_empty()
    );
    Ok(())
}

#[tokio::test]
async fn snapshot_drains_a_long_streamed_reply_and_keeps_only_a_display_preview() -> Result<()> {
    let fixture = Fixture::new()?;
    let path = fixture
        .authority
        .binding
        .socket_root
        .join(format!("pi-tui-rpc-{}.sock", std::process::id()));
    let listener = UnixListener::bind(&path)?;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
    let streaming = fixture.native_state();
    let mut idle = streaming.clone();
    idle["isStreaming"] = false.into();
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await?;
        let mut reader = BufReader::new(stream);
        send(&mut reader, &json!({"type":"hello","v":1,"pid":std::process::id(),"lease":"tui","commands":["get_state"]})).await?;
        answer(&mut reader, streaming.clone(), vec![]).await?;
        // Far more than the old 1024-frame drain, each re-sending the whole message
        // so far, the way Pi does. A tool runs in between.
        let mut backlog = vec![
            json!({"type":"event","event":"message_start","data":{"message":{"role":"assistant"}}}),
        ];
        let mut text = String::new();
        for n in 0..3000 {
            text.push_str(&format!("w{n} "));
            backlog.push(json!({"type":"event","event":"message_update","data":{"message":{"role":"assistant",
                "content":[{"type":"thinking","thinking":"hidden"},{"type":"text","text":text}]}}}));
        }
        backlog.push(
            json!({"type":"event","event":"tool_execution_start","data":{"toolName":"bash"}}),
        );
        answer(&mut reader, streaming.clone(), backlog).await?;
        answer(
            &mut reader,
            idle,
            vec![json!({"type":"event","event":"message_end","data":{}})],
        )
        .await?;
        anyhow::Ok(())
    });
    let mut observer =
        TuiObserver::attach_snapshots(fixture.authority.clone(), "alice", "public-id").await?;
    assert_eq!(observer.preview(), &LivePreview::default());
    let state = observer.snapshot_state().await?;
    assert!(state.streaming);
    let preview = observer.preview().clone();
    let text = preview.text.context("preview text")?;
    assert!(
        text.starts_with("w0 ") && text.ends_with("w2999 "),
        "newest full text, text parts only"
    );
    assert!(!text.contains("hidden"));
    assert_eq!(preview.tool.as_deref(), Some("bash"));
    let state = observer.snapshot_state().await?;
    assert!(!state.streaming);
    assert_eq!(
        observer.preview(),
        &LivePreview::default(),
        "cleared once the reply is finished"
    );
    server.await??;
    fixture.unchanged()
}

async fn answer(
    reader: &mut BufReader<UnixStream>,
    state: Value,
    before: Vec<Value>,
) -> Result<()> {
    let command = read_frame(reader).await?;
    for frame in &before {
        send(reader, frame).await?;
    }
    send(reader, &json!({"type":"response","id":command["id"],"command":"get_state","success":true,"data":state})).await
}

#[test]
fn preview_keeps_the_tail_of_a_very_long_reply_on_a_char_boundary() {
    let long = "é".repeat(PREVIEW_LIMIT);
    let frame =
        json!({"data":{"message":{"role":"assistant","content":[{"type":"text","text":long}]}}});
    let text = preview_text(&frame).unwrap();
    assert!(text.starts_with('…'));
    assert!(text.len() <= PREVIEW_LIMIT + '…'.len_utf8());
    assert!(
        preview_text(
            &json!({"data":{"message":{"role":"user","content":[{"type":"text","text":"x"}]}}})
        )
        .is_none()
    );
}
