use super::*;
use std::collections::HashSet;
use std::sync::Mutex;

/// Distinct accepted socket names can share this fixture process's PID. This
/// exercises real same-UID peer checks without spawning 71 disposable processes.
fn crowded_live_sockets(matches: usize) -> Result<Fixture> {
    let mut fixture = Fixture::new()?;
    let root = fixture.authority.binding.socket_root.clone();
    let selected = Arc::new(Mutex::new(HashSet::<PathBuf>::new()));
    let uid = rustix::process::geteuid().as_raw();
    let pid = std::process::id();
    for index in 0..71 {
        let owner_width = uid.to_string().len() + index / 8;
        let pid_width = pid.to_string().len() + index % 8;
        let directory = root.join(format!("pi-tui-rpc-{uid:0owner_width$}-{pid:0pid_width$}"));
        std::fs::create_dir(&directory)?;
        let path = directory.join("control.sock");
        let listener = UnixListener::bind(&path)?;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
        let selected = selected.clone();
        let connected = fixture.connected.clone();
        let disconnected = fixture.disconnected.clone();
        let commands = fixture.commands.clone();
        let mut state = fixture.native_state();
        state["leafId"] = Value::Null;
        fixture.tasks.push(tokio::spawn(async move {
            loop {
                let Ok((stream, _)) = listener.accept().await else {
                    break;
                };
                connected.fetch_add(1, Ordering::SeqCst);
                let mut reader = BufReader::new(stream);
                let hello = json!({"type":"hello","v":1,"pid":pid,"commands":["get_state"]});
                if send(&mut reader, &hello).await.is_err() {
                    continue;
                }
                if let Ok(command) = read_frame(&mut reader).await {
                    commands.lock().expect("fixture commands").push(command.clone());
                    let mut reply_state = state.clone();
                    if !selected.lock().expect("fixture selection").contains(&path) {
                        reply_state["sessionId"] = format!("unrelated-{index}").into();
                    }
                    let reply = json!({"type":"response","id":command["id"],"command":"get_state","success":true,"data":reply_state});
                    let _ = send(&mut reader, &reply).await;
                    let mut line = String::new();
                    let _ = reader.read_line(&mut line).await;
                    assert!(line.is_empty(), "discovery sent a non-observation command");
                }
                disconnected.fetch_add(1, Ordering::SeqCst);
            }
        }));
    }
    // Select from the actual directory order, never from creation/name order.
    // Every selected peer is beyond the old global 64-candidate cutoff.
    let paths = std::fs::read_dir(&root)?
        .map(|entry| entry.map(|entry| entry.path().join("control.sock")))
        .collect::<std::io::Result<Vec<_>>>()?;
    assert_eq!(paths.len(), 71);
    selected
        .lock()
        .expect("fixture selection")
        .extend(paths.into_iter().skip(71 - matches));
    Ok(fixture)
}

struct ActiveProbe(Arc<AtomicUsize>);
impl Drop for ActiveProbe {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

#[tokio::test]
async fn probe_window_is_bounded_and_cancellation_releases_all_workers() -> Result<()> {
    let fixture = crowded_live_sockets(1)?;
    let active = Arc::new(AtomicUsize::new(0));
    let peak = Arc::new(AtomicUsize::new(0));
    let visited = Arc::new(AtomicUsize::new(0));
    probe_sockets(
        &fixture.authority.binding.socket_root,
        |_, _| {
            let (active, peak, visited) = (active.clone(), peak.clone(), visited.clone());
            async move {
                let current = active.fetch_add(1, Ordering::SeqCst) + 1;
                let _guard = ActiveProbe(active);
                peak.fetch_max(current, Ordering::SeqCst);
                visited.fetch_add(1, Ordering::SeqCst);
                tokio::task::yield_now().await;
                Ok(None::<()>)
            }
        },
        |()| Ok(()),
    )
    .await?;
    assert_eq!(visited.load(Ordering::SeqCst), 71);
    assert!(peak.load(Ordering::SeqCst) <= MAX_CONCURRENT_PROBES);
    assert_eq!(active.load(Ordering::SeqCst), 0);

    let entered = Arc::new(tokio::sync::Notify::new());
    let root = fixture.authority.binding.socket_root.clone();
    let task_active = active.clone();
    let task_entered = entered.clone();
    let task = tokio::spawn(async move {
        probe_sockets(
            &root,
            |_, _| {
                let active = task_active.clone();
                let entered = task_entered.clone();
                async move {
                    active.fetch_add(1, Ordering::SeqCst);
                    let _guard = ActiveProbe(active);
                    entered.notify_one();
                    std::future::pending::<()>().await;
                    Ok(None::<()>)
                }
            },
            |()| Ok(()),
        )
        .await
    });
    tokio::time::timeout(Duration::from_secs(2), entered.notified()).await?;
    assert!(active.load(Ordering::SeqCst) <= MAX_CONCURRENT_PROBES);
    task.abort();
    assert!(task.await.is_err_and(|error| error.is_cancelled()));
    tokio::time::timeout(Duration::from_secs(2), async {
        while active.load(Ordering::SeqCst) != 0 {
            tokio::task::yield_now().await;
        }
    })
    .await?;
    fixture.unchanged()
}

#[tokio::test]
async fn target_beyond_64_live_peers_attaches_and_is_not_truncated_from_catalog() -> Result<()> {
    let fixture = crowded_live_sockets(1)?;
    let observer = fixture.attach().await?;
    assert_eq!(fixture.connected.load(Ordering::SeqCst), 71);
    drop(observer);
    let binding = &fixture.authority.binding;
    let live = live_sessions(&binding.socket_root, &binding.session_root).await;
    assert_eq!(
        live.len(),
        71,
        "catalog must not silently truncate live peers"
    );
    assert!(live.contains_key("native-id"));
    fixture.unchanged()
}

#[tokio::test]
async fn duplicate_target_beyond_64_live_peers_still_refuses_ambiguous_identity() -> Result<()> {
    let fixture = crowded_live_sockets(2)?;
    let error = fixture
        .attach()
        .await
        .err()
        .context("duplicate must fail")?;
    assert!(
        error.to_string().contains("Ambiguous live TUI identity"),
        "wrong refusal: {error:#}"
    );
    let binding = &fixture.authority.binding;
    let live = live_sessions(&binding.socket_root, &binding.session_root).await;
    assert_eq!(live.len(), 69);
    assert!(
        !live.contains_key("native-id"),
        "ambiguous peer is never live"
    );
    fixture.unchanged()
}
