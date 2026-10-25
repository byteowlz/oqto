use super::*;
use oqto_history::oqto_log::{projector, store::native_graph::NativeGraph};
use std::io::Write;

fn large_source(fixture: &Fixture) -> Result<Vec<u8>> {
    let mut prefix = format!(
        "{}\n",
        serde_json::json!({"type":"session","version":3,"id":NATIVE,"cwd":fixture.host.work})
    );
    for n in 0..120 {
        let parent = (n > 0).then(|| format!("m{}", n - 1));
        let value = serde_json::json!({"type":"message","id":format!("m{n}"),"parentId":parent,
            "message":{"role":"assistant","content":format!("message-{n}"),"timestamp":n+1}});
        prefix.push_str(&format!("{value}\n"));
    }
    prefix.push_str("{\"type\":\"model_change\",\"id\":\"a\",\"parentId\":\"m119\"}\n");
    prefix.push_str("{\"type\":\"message\",\"id\":\"b\",\"parentId\":\"m99\",\"message\":{\"role\":\"assistant\",\"content\":\"other branch\"}}\n");
    let mut file = std::io::BufWriter::new(
        std::fs::OpenOptions::new()
            .write(true)
            .truncate(true)
            .open(&fixture.source)?,
    );
    file.write_all(prefix.as_bytes())?;
    let padding = "x".repeat(64 * 1024);
    for n in 0..1100 {
        writeln!(
            file,
            "{}",
            serde_json::json!({"type":"message","id":format!("padding-{n}"),"parentId":null,"message":{"role":"assistant","content":padding}})
        )?;
    }
    file.flush()?;
    Ok(prefix.into_bytes())
}

#[tokio::test]
async fn large_native_file_reads_selected_live_head_without_prior_import() -> Result<()> {
    let fixture = Fixture::start().await?;
    let small = large_source(&fixture)?;
    let before = std::fs::metadata(&fixture.source)?;
    assert!(before.len() > 64 * 1024 * 1024);
    let peer = Peer::start(&fixture)?;
    let host = &fixture.host;
    let device = host.enroll(&host.qr, 101).await?;
    let public = oqto_history::oqto_log::store::platform_id_for_external_id(NATIVE);
    tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            let (status, list) = host.action(&device, 101, HostAction::SessionsList).await?;
            if status == 200 && list.to_string().contains(&public) {
                break anyhow::Ok(());
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await??;
    let workspace = host.work.to_str().context("workspace")?;
    assert!(
        projector::native_head_for_workspace(&host.home, workspace, &public)
            .await?
            .is_none()
    );
    // Match the phone's open flow: observe first, then fetch history. The large
    // canonical ingest may be deferred, but must not block this read lane.
    let handle = attach(host, &device, 101, &public).await?;
    let (status, state) = host
        .action(
            &device,
            101,
            HostAction::SessionRead {
                attachment_id: handle.clone(),
            },
        )
        .await?;
    assert_eq!(status, 200);
    assert_eq!(state["native_leaf_id"], "a");
    let branch = NativeGraph::parse(&small, NATIVE, workspace, Some("a"))?.branch_messages(&public);
    let (mut cursor, mut old_cursor) = (None, None);
    let mut ids = std::collections::HashSet::new();
    for rows in [50, 50, 20] {
        let started = std::time::Instant::now();
        let (status, page) = host
            .action(
                &device,
                101,
                HostAction::SessionHistory {
                    session_id: public.clone(),
                    before: cursor,
                },
            )
            .await?;
        eprintln!(
            "synthetic large history: {rows} rows in {:?}",
            started.elapsed()
        );
        assert_eq!(
            status, 200,
            "large live history must not fall back to an absent import: {page}"
        );
        assert_eq!(
            page["native_graph"]["native_leaf_id"], "a",
            "selected head, not the last appended record"
        );
        let expected =
            projector::project_native_branch_page(&public, &branch, 50, old_cursor.as_deref())?;
        assert_eq!(page["messages"], serde_json::to_value(&expected.messages)?);
        assert_eq!(page["messages"].as_array().context("messages")?.len(), rows);
        assert_eq!(page["has_more"], expected.has_more);
        for message in page["messages"].as_array().context("messages")? {
            assert!(ids.insert(message["id"].as_str().context("id")?.to_owned()));
        }
        cursor = page["next_before"].as_str().map(str::to_owned);
        old_cursor = expected.next_before;
    }
    assert_eq!(ids.len(), 120);
    assert!(cursor.is_none());
    assert_eq!(std::fs::metadata(&fixture.source)?.len(), before.len());
    assert!(
        projector::native_head_for_workspace(&host.home, workspace, &public)
            .await?
            .is_none()
    );
    assert_eq!(peer.writes.load(Ordering::SeqCst), 0);
    assert_eq!(peer.leases.load(Ordering::SeqCst), 0);
    assert_eq!(
        host.action(
            &device,
            101,
            HostAction::SessionDetach {
                attachment_id: handle
            }
        )
        .await?
        .0,
        200
    );
    fixture.host.stop().await?;
    Ok(())
}
