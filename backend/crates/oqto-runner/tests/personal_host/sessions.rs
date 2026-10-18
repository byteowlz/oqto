use super::*;
use oqto_personal_wire::actions::SessionInput;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use tokio::net::UnixListener;

#[tokio::test]
async fn actual_cli_signed_https_existing_pi_history_control_and_revocation() -> Result<()> {
    let host = Host::start_mode(true).await?;
    let primary = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Ok(stream) =
                tokio::net::UnixStream::connect(host.home.join("primary.sock")).await
            {
                break stream;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await?;
    let mut primary = BufReader::new(primary);
    oqto_runner::wire::write_json_frame(
        primary.get_mut(),
        &oqto_runner::protocol::RunnerRequest::Ping,
    )
    .await?;
    let pong: Option<oqto_runner::protocol::RunnerResponse> =
        oqto_runner::wire::read_json_frame(&mut primary).await?;
    assert!(
        matches!(pong, Some(oqto_runner::protocol::RunnerResponse::Pong)),
        "Existing Runner transport cohosts pairing in the same process"
    );
    let native = "11111111-2222-3333-4444-555555555555";
    let public = oqto_history::oqto_log::store::platform_id_for_external_id(native);
    let directory = host.home.join(".pi/agent/sessions").join(format!(
        "--{}--",
        host.work
            .to_string_lossy()
            .trim_start_matches('/')
            .replace('/', "-")
    ));
    std::fs::create_dir_all(&directory)?;
    let source = directory.join(format!("2026-10-17_{native}.jsonl"));
    let message = serde_json::json!({"role":"user","content":[{"type":"text","text":"synthetic canonical message"}],"timestamp":1720000000000_i64});
    let sentinel = format!(
        "{}\n{}\n",
        serde_json::json!({"type":"session","version":3,"id":native,"cwd":host.work}),
        serde_json::json!({"type":"message","id":"source-a","parentId":null,"message":message})
    );
    std::fs::write(&source, &sentinel)?;
    oqto_history::oqto_log::store::replace_session_with_pi_jsonl_records(
        &host.home,
        &std::env::var("USER").unwrap_or_else(|_| "fixture".into()),
        host.work.to_str().context("Workspace")?,
        &public,
        &public,
        Some(native),
        native,
        &[oqto_history::oqto_log::store::PiJsonlMessageRecord {
            source_entry_id: "source-a".into(),
            parent_source_entry_id: None,
            source_sequence: 1,
            message: serde_json::from_value(message)?,
        }],
    )
    .await?;
    let socket = host
        .home
        .join("tmp")
        .join(format!("pi-tui-rpc-{}.sock", std::process::id()));
    let listener = UnixListener::bind(&socket)?;
    std::fs::set_permissions(socket, std::fs::Permissions::from_mode(0o600))?;
    let counts = Arc::new(AtomicUsize::new(0));
    let confirmed = Arc::new(AtomicUsize::new(0));
    let writes = counts.clone();
    let leases = confirmed.clone();
    let (path, cwd) = (source.clone(), host.work.clone());
    let server = tokio::spawn(async move {
        let mut children = tokio::task::JoinSet::new();
        loop {
            let Ok((stream, _)) = listener.accept().await else {
                break;
            };
            let (path, cwd, writes, leases) =
                (path.clone(), cwd.clone(), writes.clone(), leases.clone());
            children.spawn(async move {
                let mut reader = BufReader::new(stream);
                let hello = serde_json::json!({"type":"hello","v":1,"pid":std::process::id(),"commands":["get_state","lease","prompt","steer","abort"],"capabilities":["connection_leases_v1"]});
                reader.get_mut().write_all(format!("{hello}\n").as_bytes()).await?;
                let mut line = String::new();
                let mut held = false;
                while reader.read_line(&mut line).await? != 0 {
                    let frame: Value = serde_json::from_str(&line)?;
                    line.clear();
                    let mut response = serde_json::json!({"type":"response","id":frame["id"],"command":frame["type"],"success":true});
                    match frame["type"].as_str() {
                        Some("get_state") => response["data"] = serde_json::json!({"sessionId":native,"sessionFile":path,"cwd":cwd,"leafId":"source-a"}),
                        Some("lease") => { leases.fetch_add(1, Ordering::SeqCst); held=true; response["data"] = serde_json::json!({"owner":"remote","reason":"tui_confirmed"}); }
                        Some("prompt" | "steer" | "abort") if held => { writes.fetch_add(1, Ordering::SeqCst); }
                        _ => response["success"] = false.into(),
                    }
                    reader.get_mut().write_all(format!("{response}\n").as_bytes()).await?;
                }
                Ok::<(),anyhow::Error>(())
            });
        }
    });
    let device = host.enroll(&host.qr, 91).await?;
    let (status, caps) = host.action(&device, 91, HostAction::Capabilities).await?;
    assert_eq!(status, 200);
    assert_eq!(caps["session_observe_supported"], true);
    assert_eq!(caps["session_control_supported"], true);
    let (status, attachment) = host
        .action(
            &device,
            91,
            HostAction::SessionAttach {
                session_id: public.clone(),
            },
        )
        .await?;
    assert_eq!(status, 200, "{attachment}");
    let id = attachment["attachment_id"]
        .as_str()
        .context("attachment")?
        .to_owned();
    assert_eq!(confirmed.load(Ordering::SeqCst), 0);
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let (status, page) = host
                .action(
                    &device,
                    91,
                    HostAction::SessionHistory {
                        session_id: public.clone(),
                        before: None,
                    },
                )
                .await?;
            if status == 200 {
                assert_eq!(page["messages"].as_array().context("messages")?.len(), 1);
                assert_eq!(page["native_graph"]["native_leaf_id"], "source-a");
                break Ok::<(), anyhow::Error>(());
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await??;
    assert_eq!(
        host.action(
            &device,
            91,
            HostAction::SessionRequestControl {
                attachment_id: id.clone()
            }
        )
        .await?
        .0,
        200
    );
    assert_eq!(confirmed.load(Ordering::SeqCst), 1);
    let (status, receipt) = host
        .action(
            &device,
            91,
            HostAction::SessionInput {
                attachment_id: id.clone(),
                input: SessionInput::Prompt {
                    message: "synthetic input".into(),
                },
            },
        )
        .await?;
    assert_eq!(status, 200, "{receipt}");
    assert_eq!(receipt["public_session_id"], public);
    assert_eq!(counts.load(Ordering::SeqCst), 1);
    let revoked = host.owner(&["revoke", &device]).await?;
    assert_eq!(revoked["revoked"], true);
    assert_eq!(
        host.action(
            &device,
            91,
            HostAction::SessionInput {
                attachment_id: id,
                input: SessionInput::Abort
            }
        )
        .await?
        .0,
        401
    );
    assert_eq!(counts.load(Ordering::SeqCst), 1);
    assert_eq!(std::fs::read_to_string(source)?, sentinel);
    server.abort();
    host.stop().await?;
    Ok(())
}
