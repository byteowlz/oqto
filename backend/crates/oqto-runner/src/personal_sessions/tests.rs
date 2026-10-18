use super::*;
use base64::{Engine, engine::general_purpose::STANDARD};
use ed25519_dalek::{Signer, SigningKey};
use oqto_personal::{Access, Grants, PairingAuthority, SessionAccess};
use oqto_personal_wire::{Enrollment, OQTO_PERSONAL_V1, SignedRequest};
use std::{
    os::unix::fs::PermissionsExt,
    sync::atomic::{AtomicUsize, Ordering},
};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::UnixListener,
};

struct Device {
    id: String,
    key: SigningKey,
}
async fn device(authority: &PairingAuthority, access: SessionAccess, seed: u8) -> Result<Device> {
    let ticket = authority
        .issue_ticket(&Grants {
            sessions: access,
            ..Grants::default()
        })
        .await?;
    let key = SigningKey::from_bytes(&[seed; 32]);
    let mut enrollment = Enrollment {
        token: ticket.token,
        name: "Synthetic session Device".into(),
        public_key: STANDARD.encode(key.verifying_key().to_bytes()),
        signature: String::new(),
    };
    enrollment.signature = STANDARD.encode(
        key.sign(&enrollment.signing_bytes_for(OQTO_PERSONAL_V1))
            .to_bytes(),
    );
    Ok(Device {
        id: authority.enroll(&enrollment).await?,
        key,
    })
}
async fn admit(
    authority: &PairingAuthority,
    device: &Device,
    access: Access<'_>,
) -> Result<AuthorizedAction> {
    let mut request = SignedRequest {
        device_id: device.id.clone(),
        timestamp: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs()
            .try_into()?,
        nonce: uuid::Uuid::new_v4().to_string(),
        payload: "synthetic admission".into(),
        signature: String::new(),
    };
    request.signature = STANDARD.encode(
        device
            .key
            .sign(&request.signing_bytes_for(OQTO_PERSONAL_V1))
            .to_bytes(),
    );
    let principal = authority.authenticate(&request).await?;
    authority.require_current(&principal, access).await
}

#[test]
fn attachment_budget_counts_inflight_operations_and_releases_on_cancel() -> Result<()> {
    let budget = Arc::new(Budget::default());
    let key = ("host".into(), "device".into(), "generation".into());
    let reservations = (0..MAX_DEVICE_ATTACHMENTS)
        .map(|_| budget.reserve(key.clone()))
        .collect::<Result<Vec<_>>>()?;
    assert!(budget.reserve(key.clone()).is_err());
    drop(reservations);
    assert!(budget.reserve(key).is_ok());
    assert!(budget.0.lock().unwrap().is_empty());
    let mut total = Vec::new();
    for n in 0..MAX_ATTACHMENTS {
        total.push(budget.reserve(("host".into(), format!("device-{n}"), "generation".into()))?);
    }
    assert!(
        budget
            .reserve(("host".into(), "overflow".into(), "generation".into()))
            .is_err()
    );
    drop(total);
    assert!(budget.0.lock().unwrap().is_empty());
    Ok(())
}

#[tokio::test]
async fn signed_devices_attach_existing_pi_control_detach_revoke_without_spawn_or_file_write()
-> Result<()> {
    let root = tempfile::tempdir()?;
    let home = root.path().canonicalize()?;
    let work = home.join("work");
    let sockets = home.join("sockets");
    std::fs::create_dir_all(&work)?;
    std::fs::create_dir_all(&sockets)?;
    let native = "11111111-2222-3333-4444-555555555555";
    let public = oqto_history::oqto_log::store::platform_id_for_external_id(native);
    let native_dir = home.join(".pi/agent/sessions").join(format!(
        "--{}--",
        work.to_string_lossy()
            .trim_start_matches('/')
            .replace('/', "-")
    ));
    std::fs::create_dir_all(&native_dir)?;
    let source = native_dir.join(format!("2026-10-17_{native}.jsonl"));
    let native_message = json!({"role":"user","content":[{"type":"text","text":"synthetic durable content"}],"timestamp":1720000000000_i64});
    let sentinel = format!(
        "{}\n{}\n{}\n",
        json!({"type":"session","version":3,"id":native,"cwd":work}),
        json!({"type":"message","id":"entry-a","parentId":null,"message":native_message}),
        json!({"type":"message","id":"entry-b","parentId":null,"message":native_message})
    );
    std::fs::write(&source, &sentinel)?;
    let message = serde_json::from_value(native_message)?;
    oqto_history::oqto_log::store::replace_session_with_pi_jsonl_records(
        &home,
        "synthetic",
        work.to_str().context("Workspace")?,
        &public,
        &public,
        Some(native),
        native,
        &[oqto_history::oqto_log::store::PiJsonlMessageRecord {
            source_entry_id: "entry-a".into(),
            parent_source_entry_id: None,
            source_sequence: 1,
            message,
        }],
    )
    .await?;
    let listener =
        UnixListener::bind(sockets.join(format!("pi-tui-rpc-{}.sock", std::process::id())))?;
    std::fs::set_permissions(
        sockets.join(format!("pi-tui-rpc-{}.sock", std::process::id())),
        std::fs::Permissions::from_mode(0o600),
    )?;
    let actions = Arc::new(AtomicUsize::new(0));
    let leases = Arc::new(AtomicUsize::new(0));
    let closed = Arc::new(AtomicUsize::new(0));
    let head = Arc::new(AtomicUsize::new(0));
    let native_head = head.clone();
    let counts = (actions.clone(), leases.clone(), closed.clone());
    let path = source.clone();
    let cwd = work.clone();
    let server = tokio::spawn(async move {
        let mut connections = tokio::task::JoinSet::new();
        loop {
            let Ok((socket, _)) = listener.accept().await else {
                break;
            };
            let (path, cwd, counts, head) = (
                path.clone(),
                cwd.clone(),
                counts.clone(),
                native_head.clone(),
            );
            connections.spawn(async move {
                let mut socket = BufReader::new(socket);
                let hello = json!({"type":"hello","v":1,"pid":std::process::id(),"commands":["get_state","lease","prompt","steer","abort"],"capabilities":["connection_leases_v1"]});
                socket.get_mut().write_all(format!("{hello}\n").as_bytes()).await?;
                let mut line = String::new();
                let mut held = false;
                while socket.read_line(&mut line).await? != 0 {
                    let frame: Value = serde_json::from_str(&line)?;
                    line.clear();
                    let mut response = json!({"type":"response","id":frame["id"],"command":frame["type"],"success":true});
                    match frame["type"].as_str() {
                        Some("get_state") => response["data"] = json!({"sessionId":native,"sessionFile":path,"cwd":cwd,"leafId":if head.load(Ordering::SeqCst) == 0 { "entry-a" } else { "entry-b" }}),
                        Some("lease") => { counts.1.fetch_add(1, Ordering::SeqCst); held = true; response["data"] = json!({"owner":"remote","reason":"tui_confirmed"}); }
                        Some("prompt" | "steer" | "abort") if held => { counts.0.fetch_add(1, Ordering::SeqCst); }
                        _ => response["success"] = false.into(),
                    }
                    socket.get_mut().write_all(format!("{response}\n").as_bytes()).await?;
                }
                counts.2.fetch_add(1, Ordering::SeqCst);
                Ok::<(), anyhow::Error>(())
            });
        }
        Ok::<(), anyhow::Error>(())
    });
    let sessions = Sessions {
        socket_root: sockets,
        ingest: Some(crate::daemon::jsonl_ingest::spawn_for_test(
            home.clone(),
            "synthetic".into(),
        )),
        ..Sessions::default()
    };
    let authority = PairingAuthority::open_private(&home.join("authority")).await?;
    let owner = device(&authority, SessionAccess::Control, 81).await?;
    let other = device(&authority, SessionAccess::Control, 82).await?;
    let observer = device(&authority, SessionAccess::Observe, 83).await?;
    let roots = vec![work];
    let attached = sessions
        .execute(
            &home,
            &roots,
            HostAction::SessionAttach {
                session_id: public.clone(),
            },
            admit(&authority, &owner, Access::SessionObserve).await?,
        )
        .await
        .map_err(|e| anyhow::anyhow!("attach: {e:?}"))?;
    let id = attached["attachment_id"]
        .as_str()
        .context("handle")?
        .to_owned();
    assert_eq!(leases.load(Ordering::SeqCst), 0);
    assert_eq!(attached["sync_requested"], true);
    let page = tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if let Ok(page) = crate::history_read::read_native_authorized_history(
                &home,
                &roots,
                public.clone(),
                None,
            )
            .await
            {
                break page;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await?;
    assert_eq!(
        page.data["messages"].as_array().context("messages")?.len(),
        1
    );
    assert_eq!(page.data["native_graph"]["native_leaf_id"], "entry-a");
    let first_id = page.data["messages"][0]["id"].clone();
    head.store(1, Ordering::SeqCst);
    let state = sessions
        .execute(
            &home,
            &roots,
            HostAction::SessionRead {
                attachment_id: id.clone(),
            },
            admit(&authority, &owner, Access::SessionObserve).await?,
        )
        .await
        .map_err(|e| anyhow::anyhow!("snapshot: {e:?}"))?;
    assert_eq!(state["native_leaf_id"], "entry-b");
    let switched = tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if let Ok(page) = crate::history_read::read_native_authorized_history(
                &home,
                &roots,
                public.clone(),
                None,
            )
            .await
                && page.data["native_graph"]["native_leaf_id"] == "entry-b"
            {
                break page;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await?;
    assert_eq!(
        switched.data["messages"]
            .as_array()
            .context("messages")?
            .len(),
        1
    );
    assert_ne!(
        switched.data["messages"][0]["id"], first_id,
        "Equal-count, unchanged-file Branch switch follows source identity, not visible text"
    );
    assert!(
        switched.data["native_graph"]["revision"].as_u64()
            > page.data["native_graph"]["revision"].as_u64()
    );
    assert!(matches!(
        sessions
            .execute(
                &home,
                &roots,
                HostAction::SessionRead {
                    attachment_id: id.clone()
                },
                admit(&authority, &other, Access::SessionObserve).await?
            )
            .await,
        Err(ActionFailure::Denied)
    ));
    assert!(
        sessions
            .execute(
                &home,
                &roots,
                HostAction::SessionRead {
                    attachment_id: id.clone()
                },
                admit(&authority, &owner, Access::SessionObserve).await?
            )
            .await
            .is_ok()
    );
    assert!(
        admit(&authority, &observer, Access::SessionControl)
            .await
            .is_err()
    );
    assert!(
        sessions
            .execute(
                &home,
                &roots,
                HostAction::SessionRequestControl {
                    attachment_id: id.clone()
                },
                admit(&authority, &owner, Access::SessionControl).await?
            )
            .await
            .is_ok()
    );
    assert_eq!(leases.load(Ordering::SeqCst), 1);
    let receipt = sessions
        .execute(
            &home,
            &roots,
            HostAction::SessionInput {
                attachment_id: id.clone(),
                input: SessionInput::Prompt {
                    message: "synthetic prompt".into(),
                },
            },
            admit(&authority, &owner, Access::SessionControl).await?,
        )
        .await
        .map_err(|e| anyhow::anyhow!("input: {e:?}"))?;
    assert_eq!(receipt["public_session_id"], public);
    assert_eq!(actions.load(Ordering::SeqCst), 1);
    let closed_before = closed.load(Ordering::SeqCst);
    authority.revoke(&owner.id).await?;
    sessions.reap();
    tokio::time::timeout(Duration::from_secs(1), async {
        while closed.load(Ordering::SeqCst) <= closed_before {
            tokio::task::yield_now().await;
        }
    })
    .await?;
    assert!(sessions.handles.lock().unwrap().is_empty());
    assert!(sessions.budget.0.lock().unwrap().is_empty());
    assert_eq!(std::fs::read_to_string(source)?, sentinel);
    assert_eq!(actions.load(Ordering::SeqCst), 1);
    server.abort();
    Ok(())
}
