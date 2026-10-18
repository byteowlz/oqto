use super::*;
use std::{os::unix::fs::PermissionsExt, sync::atomic::AtomicUsize};
use tokio::net::UnixListener;

struct Gate {
    binding: ObservationBinding,
    live: AtomicBool,
    control_live: AtomicBool,
    mfa_at: Instant,
    expires: Instant,
}
impl ObservationAuthority for Gate {
    fn resolve(&self, account: &str, session: &str) -> Result<ObservationBinding> {
        ensure!(
            account == self.binding.authority_id && session == self.binding.public_session_id,
            "denied"
        );
        self.revalidate(&self.binding)?;
        Ok(self.binding.clone())
    }
    fn revalidate(&self, _: &ObservationBinding) -> Result<()> {
        ensure!(self.live.load(Ordering::SeqCst), "revoked");
        Ok(())
    }
}
impl ControlAuthority for Gate {
    fn authorize_control(
        &self,
        binding: &ObservationBinding,
        controller: &str,
    ) -> Result<ControlPermit> {
        ensure!(controller == "device-a", "controller denied");
        self.revalidate_control(
            binding,
            &ControlPermit {
                authority_id: binding.authority_id.clone(),
                public_session_id: binding.public_session_id.clone(),
                controller_id: controller.into(),
                generation: "g1".into(),
                assurance: Assurance::VerifiedAccountMfa {
                    verified_at: self.mfa_at,
                },
                expires_at: self.expires,
            },
        )?;
        ControlPermit::from_authenticated_host(
            binding.authority_id.clone(),
            binding.public_session_id.clone(),
            controller.into(),
            "g1".into(),
            self.mfa_at,
            self.expires,
        )
    }
    fn revalidate_control(
        &self,
        binding: &ObservationBinding,
        permit: &ControlPermit,
    ) -> Result<()> {
        ensure!(
            self.control_live.load(Ordering::SeqCst)
                && binding.authority_id == "account-a"
                && permit.controller_id() == "device-a"
                && permit.generation() == "g1",
            "control revoked"
        );
        Ok(())
    }
}
struct Fixture {
    _root: tempfile::TempDir,
    gate: Arc<Gate>,
    server: tokio::task::JoinHandle<()>,
    actions: Arc<AtomicUsize>,
    leases: Arc<AtomicUsize>,
    closed: Arc<AtomicUsize>,
    behavior: Arc<AtomicUsize>,
    sentinel: Vec<u8>,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.server.abort();
    }
}
impl Fixture {
    fn new(capable: bool, confirmation: Option<bool>) -> Result<Self> {
        let root = tempfile::tempdir()?;
        let base = root.path().canonicalize()?;
        for name in ["sessions", "work", "sockets"] {
            std::fs::create_dir(base.join(name))?;
        }
        let binding = ObservationBinding {
            authority_id: "account-a".into(),
            public_session_id: "public-a".into(),
            external_session_id: "native-a".into(),
            session_file: base.join("sessions/native.jsonl"),
            session_root: base.join("sessions"),
            workspace_root: base.join("work"),
            work_directory: base.join("work"),
            socket_root: base.join("sockets"),
            grant_generation: "g1".into(),
        };
        let sentinel = format!(
            "{}\n",
            json!({"type":"session","id":"native-a","cwd":binding.work_directory})
        )
        .into_bytes();
        std::fs::write(&binding.session_file, &sentinel)?;
        let gate = Arc::new(Gate {
            binding: binding.clone(),
            live: AtomicBool::new(true),
            control_live: AtomicBool::new(true),
            mfa_at: Instant::now(),
            expires: Instant::now() + Duration::from_secs(30),
        });
        let path = binding
            .socket_root
            .join(format!("pi-tui-rpc-{}.sock", std::process::id()));
        let listener = UnixListener::bind(&path)?;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
        let actions = Arc::new(AtomicUsize::new(0));
        let leases = Arc::new(AtomicUsize::new(0));
        let closed = Arc::new(AtomicUsize::new(0));
        let behavior = Arc::new(AtomicUsize::new(0));
        let mode = behavior.clone();
        let (a, l, c) = (actions.clone(), leases.clone(), closed.clone());
        let server = tokio::spawn(async move {
            loop {
                let Ok((stream, _)) = listener.accept().await else {
                    break;
                };
                let mut reader = BufReader::new(stream);
                let caps = if capable {
                    vec!["connection_leases_v1"]
                } else {
                    vec![]
                };
                let _ = send(&mut reader, json!({"type":"hello","v":1,"pid":std::process::id(),"commands":["get_state","lease","prompt","steer","abort"],"capabilities":caps})).await;
                let mut held = false;
                while let Ok(frame) = read_frame(&mut reader).await {
                    let mut response = json!({"type":"response","command":frame["type"],"id":frame["id"],"success":true});
                    match frame["type"].as_str() {
                        Some("get_state") => {
                            response["data"] = json!({"sessionId":"native-a","sessionFile":binding.session_file,"cwd":binding.work_directory})
                        }
                        Some("lease") => {
                            l.fetch_add(1, Ordering::SeqCst);
                            if mode.load(Ordering::SeqCst) == 3 {
                                for _ in 0..32 {
                                    let _ = send(&mut reader, json!({"type":"event","event":"message_update","data":{"synthetic":true}})).await;
                                }
                            }
                            if confirmation.is_none() {
                                continue;
                            }
                            held = confirmation == Some(true);
                            response["success"] = held.into();
                            response["data"] = json!({"owner":"remote","reason":"tui_confirmed"});
                        }
                        Some("prompt" | "steer" | "abort") if held => {
                            a.fetch_add(1, Ordering::SeqCst);
                            let behavior = mode.load(Ordering::SeqCst);
                            if behavior == 1 {
                                continue; // Dispatch accepted; reply deliberately withheld.
                            }
                            if behavior == 2
                                && send(
                                    &mut reader,
                                    json!({"type":"lease","owner":"tui","reason":"tui_input"}),
                                )
                                .await
                                .is_err()
                            {
                                break;
                            }
                        }
                        _ => response["success"] = false.into(),
                    }
                    if send(&mut reader, response).await.is_err() {
                        break;
                    }
                }
                c.fetch_add(1, Ordering::SeqCst);
            }
        });
        Ok(Self {
            _root: root,
            gate,
            server,
            actions,
            leases,
            closed,
            behavior,
            sentinel,
        })
    }
    async fn observer(&self) -> Result<TuiObserver> {
        TuiObserver::attach(self.gate.clone(), "account-a", "public-a").await
    }
    async fn disconnected(&self) -> Result<()> {
        tokio::time::timeout(Duration::from_secs(1), async {
            while self.closed.load(Ordering::SeqCst) == 0 {
                tokio::task::yield_now().await;
            }
        })
        .await?;
        ensure!(
            std::fs::read(&self.gate.binding.session_file)? == self.sentinel,
            "source mutated"
        );
        Ok(())
    }
}
async fn send(reader: &mut BufReader<UnixStream>, frame: Value) -> Result<()> {
    reader
        .get_mut()
        .write_all(&serde_json::to_vec(&frame)?)
        .await?;
    reader.get_mut().write_all(b"\n").await?;
    Ok(())
}

#[tokio::test]
async fn genuine_pairing_is_not_mfa_and_cannot_upgrade_observation_to_control() -> Result<()> {
    use base64::{Engine, engine::general_purpose::STANDARD};
    use ed25519_dalek::{Signer, SigningKey};
    use oqto_personal::{Access, Grants, PairingAuthority, SessionAccess};
    use oqto_personal_wire::{Enrollment, OQTO_PERSONAL_V1, SignedRequest};
    for access in [SessionAccess::Observe, SessionAccess::Control] {
        let root = tempfile::tempdir()?;
        let authority = PairingAuthority::open_private(&root.path().join("authority")).await?;
        let ticket = authority
            .issue_ticket(&Grants {
                sessions: access,
                ..Grants::default()
            })
            .await?;
        let key = SigningKey::from_bytes(&[71; 32]);
        let mut enrollment = Enrollment {
            token: ticket.token,
            name: "Disposable paired Device".into(),
            public_key: STANDARD.encode(key.verifying_key().to_bytes()),
            signature: String::new(),
        };
        enrollment.signature = STANDARD.encode(
            key.sign(&enrollment.signing_bytes_for(OQTO_PERSONAL_V1))
                .to_bytes(),
        );
        let id = authority.enroll(&enrollment).await?;
        let timestamp = i64::try_from(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_secs(),
        )?;
        let mut request = SignedRequest {
            device_id: id.clone(),
            timestamp,
            nonce: uuid::Uuid::new_v4().to_string(),
            payload: "fixture".into(),
            signature: String::new(),
        };
        request.signature = STANDARD.encode(
            key.sign(&request.signing_bytes_for(OQTO_PERSONAL_V1))
                .to_bytes(),
        );
        let device = authority.authenticate(&request).await?;
        let admission = authority
            .require_current(&device, Access::SessionObserve)
            .await?;
        let result =
            ControlPermit::from_paired_device(&admission, "oqto-disposable-public-session".into());
        if access == SessionAccess::Observe {
            assert!(result.is_err());
        } else {
            let permit = result?;
            assert!(matches!(&permit.assurance, Assurance::PairedDevice(_)));
            assert_eq!(permit.controller_id(), id);
            assert_eq!(
                permit.authority_id,
                format!("personal-host:{}", authority.host_id())
            );
            assert!(permit.check_time().is_ok());
            drop(admission);
            assert!(authority.revoke(&id).await?);
            assert!(permit.check_time().is_err());
        }
    }
    Ok(())
}

#[tokio::test]
async fn verified_controller_can_prompt_steer_abort_without_mutating_source() -> Result<()> {
    let fixture = Fixture::new(true, Some(true))?;
    let mut observer = fixture.observer().await?;
    assert!(observer.control_supported());
    assert!(observer.input(InputAction::Abort).await.is_err());
    observer
        .acquire_input(fixture.gate.clone(), "device-a")
        .await?;
    for action in [
        InputAction::Prompt {
            message: "synthetic".into(),
        },
        InputAction::Steer {
            message: "synthetic steer".into(),
        },
        InputAction::Abort,
    ] {
        assert_eq!(observer.input(action).await?.public_session_id, "public-a");
    }
    observer.release_input();
    fixture.disconnected().await?;
    assert_eq!(fixture.actions.load(Ordering::SeqCst), 3);
    Ok(())
}

#[tokio::test]
async fn snapshot_control_drains_busy_native_events_without_claiming_replay() -> Result<()> {
    for snapshot in [false, true] {
        let fixture = Fixture::new(true, Some(true))?;
        fixture.behavior.store(3, Ordering::SeqCst);
        let mut observer = if snapshot {
            TuiObserver::attach_snapshots(fixture.gate.clone(), "account-a", "public-a").await?
        } else {
            fixture.observer().await?
        };
        let result = observer
            .acquire_input(fixture.gate.clone(), "device-a")
            .await;
        assert_eq!(result.is_ok(), snapshot);
        assert_eq!(fixture.actions.load(Ordering::SeqCst), 0);
        if snapshot {
            assert!(observer.pending.is_empty());
            assert!(observer.next_event(Duration::from_millis(1)).await.is_err());
            assert!(observer.input_held());
            assert!(!observer.snapshot_state().await?.streaming);
        }
        drop(observer);
        fixture.disconnected().await?;
    }
    Ok(())
}

#[tokio::test]
async fn canonical_snapshot_mode_refreshes_without_requesting_or_losing_control() -> Result<()> {
    let fixture = Fixture::new(true, Some(true))?;
    let mut observer = fixture.observer().await?;
    assert!(!observer.snapshot_state().await?.streaming);
    assert_eq!(fixture.leases.load(Ordering::SeqCst), 0);
    assert!(!observer.input_held());
    observer
        .acquire_input(fixture.gate.clone(), "device-a")
        .await?;
    assert!(observer.input_held());
    assert!(!observer.snapshot_state().await?.compacting);
    assert!(observer.input_held());
    observer.release_input();
    fixture.disconnected().await?;
    assert_eq!(fixture.actions.load(Ordering::SeqCst), 0);
    Ok(())
}

#[tokio::test]
async fn legacy_capability_and_missing_fresh_mfa_never_request_native_control() -> Result<()> {
    for cause in ["legacy", "mfa", "expired", "controller", "revoked"] {
        let mut fixture = Fixture::new(cause != "legacy", Some(true))?;
        if cause == "mfa" {
            Arc::get_mut(&mut fixture.gate).unwrap().mfa_at =
                Instant::now() - Duration::from_secs(301);
        }
        if cause == "expired" {
            Arc::get_mut(&mut fixture.gate).unwrap().expires =
                Instant::now() - Duration::from_secs(1);
        }
        if cause == "revoked" {
            fixture.gate.control_live.store(false, Ordering::SeqCst);
        }
        let mut observer = fixture.observer().await?;
        assert!(
            observer
                .acquire_input(
                    fixture.gate.clone(),
                    if cause == "controller" {
                        "device-b"
                    } else {
                        "device-a"
                    }
                )
                .await
                .is_err(),
            "{cause}"
        );
        assert_eq!(fixture.leases.load(Ordering::SeqCst), 0, "{cause}");
        drop(observer);
        fixture.disconnected().await?;
    }
    Ok(())
}

#[tokio::test]
async fn terminal_denial_and_cancelled_confirmation_do_not_dispatch_input() -> Result<()> {
    for confirmation in [Some(false), None] {
        let fixture = Fixture::new(true, confirmation)?;
        let mut observer = fixture.observer().await?;
        let result = tokio::time::timeout(
            Duration::from_millis(20),
            observer.acquire_input(fixture.gate.clone(), "device-a"),
        )
        .await;
        assert!(result.is_err() || result.unwrap().is_err());
        assert!(observer.input(InputAction::Abort).await.is_err());
        fixture.disconnected().await?;
        assert_eq!(fixture.actions.load(Ordering::SeqCst), 0);
    }
    Ok(())
}

#[tokio::test]
async fn idle_revocation_closes_native_socket_without_another_request() -> Result<()> {
    let fixture = Fixture::new(true, Some(true))?;
    let mut observer = fixture.observer().await?;
    observer
        .acquire_input(fixture.gate.clone(), "device-a")
        .await?;
    fixture.gate.control_live.store(false, Ordering::SeqCst);
    fixture.disconnected().await?;
    assert!(observer.input(InputAction::Abort).await.is_err());
    assert_eq!(fixture.actions.load(Ordering::SeqCst), 0);
    Ok(())
}

#[tokio::test]
async fn cancelled_controlled_observation_drops_watchdog_and_lease() -> Result<()> {
    let fixture = Fixture::new(true, Some(true))?;
    let mut observer = fixture.observer().await?;
    observer
        .acquire_input(fixture.gate.clone(), "device-a")
        .await?;
    assert!(
        tokio::time::timeout(
            Duration::from_millis(10),
            observer.next_event(Duration::from_secs(5))
        )
        .await
        .is_err()
    );
    fixture.disconnected().await?;
    assert!(observer.input_lease.is_none());
    Ok(())
}

async fn wait_for_dispatch(counter: &AtomicUsize) -> Result<()> {
    tokio::time::timeout(Duration::from_secs(1), async {
        while counter.load(Ordering::SeqCst) == 0 {
            tokio::task::yield_now().await;
        }
    })
    .await?;
    Ok(())
}

#[tokio::test]
async fn cancellation_after_input_dispatch_closes_lease_without_retry() -> Result<()> {
    let fixture = Fixture::new(true, Some(true))?;
    fixture.behavior.store(1, Ordering::SeqCst);
    let mut observer = fixture.observer().await?;
    observer
        .acquire_input(fixture.gate.clone(), "device-a")
        .await?;
    let mut input = Box::pin(observer.input(InputAction::Prompt {
        message: "synthetic uncertain input".into(),
    }));
    tokio::select! {
        result = &mut input => bail!("withheld response unexpectedly completed: {result:?}"),
        seen = wait_for_dispatch(&fixture.actions) => seen?,
    }
    drop(input);
    fixture.disconnected().await?;
    assert!(observer.input(InputAction::Abort).await.is_err());
    assert_eq!(fixture.actions.load(Ordering::SeqCst), 1);
    assert!(observer.input_lease.is_none());
    assert!(observer.reader.is_none());
    Ok(())
}

#[tokio::test]
async fn terminal_reclaim_before_response_poisons_control_not_terminal_process() -> Result<()> {
    let fixture = Fixture::new(true, Some(true))?;
    fixture.behavior.store(2, Ordering::SeqCst);
    let mut observer = fixture.observer().await?;
    observer
        .acquire_input(fixture.gate.clone(), "device-a")
        .await?;
    let result = observer
        .input(InputAction::Steer {
            message: "synthetic steer".into(),
        })
        .await;
    assert!(result.is_err());
    fixture.disconnected().await?;
    assert!(observer.input(InputAction::Abort).await.is_err());
    assert_eq!(fixture.actions.load(Ordering::SeqCst), 1);
    assert!(observer.reader.is_none());
    Ok(())
}

#[tokio::test]
async fn host_revocation_during_terminal_confirmation_closes_without_input() -> Result<()> {
    let fixture = Fixture::new(true, None)?;
    let mut observer = fixture.observer().await?;
    let revoke = async {
        wait_for_dispatch(&fixture.leases).await?;
        fixture.gate.control_live.store(false, Ordering::SeqCst);
        Ok::<(), anyhow::Error>(())
    };
    let (result, revoked) = tokio::time::timeout(Duration::from_secs(1), async {
        tokio::join!(
            observer.acquire_input(fixture.gate.clone(), "device-a"),
            revoke
        )
    })
    .await?;
    revoked?;
    assert!(result.is_err());
    fixture.disconnected().await?;
    assert_eq!(fixture.actions.load(Ordering::SeqCst), 0);
    assert!(observer.reader.is_none());
    Ok(())
}

#[test]
fn input_wire_cannot_approve_tools_select_paths_or_self_attest_mfa() {
    for value in [
        json!({"command":"approve","request_id":"tool"}),
        json!({"command":"abort","mfa":true}),
        json!({"command":"bash","command_line":"echo bypass"}),
        json!({"command":"switch_session","path":"/secret"}),
    ] {
        assert!(serde_json::from_value::<InputAction>(value).is_err());
    }
}
