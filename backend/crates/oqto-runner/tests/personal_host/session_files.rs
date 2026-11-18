//! Real Runner/signed HTTPS, disposable source/grants only. No real Sessions.
use super::*;

async fn seed(
    host: &Host,
    workspace: &Path,
    session: &str,
    public: &str,
    native: &str,
) -> Result<()> {
    std::fs::create_dir_all(workspace)?;
    let message: oqto_pi::AgentMessage = serde_json::from_value(
        serde_json::json!({"role":"user","content":"synthetic Files fixture","timestamp":1700000000000_i64}),
    )?;
    oqto_history::oqto_log::store::replace_session_with_snapshot(
        &host.home,
        "fixture",
        workspace.to_str().context("workspace")?,
        session,
        public,
        Some(native),
        native,
        &[message],
    )
    .await?;
    Ok(())
}
async fn ready(host: &Host, device: &str, seed: u8) -> Result<()> {
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if host.action(device, seed, HostAction::SessionsList).await?.0 == 200 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        Ok::<_, anyhow::Error>(())
    })
    .await??;
    Ok(())
}

#[tokio::test]
async fn signed_session_files_retains_complete_binding_and_fresh_root_inode_checks() -> Result<()> {
    let host = Host::start_mode(true).await?;
    let cwd = host.work.join("project");
    let public = oqto_history::oqto_log::store::platform_id_for_external_id("synthetic-files");
    seed(&host, &cwd, &public, &public, "synthetic-files").await?;
    let other = host.work.join("outside-narrow-grant");
    let other_id = oqto_history::oqto_log::store::platform_id_for_external_id("synthetic-outside");
    seed(&host, &other, &other_id, &other_id, "synthetic-outside").await?;
    let duplicate =
        oqto_history::oqto_log::store::platform_id_for_external_id("synthetic-duplicate");
    seed(
        &host,
        &host.work.join("dup-first"),
        "dup-a",
        &duplicate,
        "native-a",
    )
    .await?;
    seed(
        &host,
        &host.work.join("zzz-dup-late"),
        "dup-b",
        &duplicate,
        "native-b",
    )
    .await?;
    let device = host.enroll(&host.qr, 74).await?;
    ready(&host, &device, 74).await?;
    let action = || HostAction::SessionFiles {
        session_id: public.clone(),
    };
    let (status, context) = host.action(&device, 74, action()).await?;
    assert_eq!(status, 200, "{context}");
    assert_eq!(context["path"], "project");
    assert_eq!(context["session_id"], public);
    let root = context["root"]["id"].as_str().context("root")?;
    // Keep the existing list-before-mkdir contract: already-exists is refused.
    assert_eq!(
        host.action(
            &device,
            74,
            HostAction::DirectoryCreate {
                root_id: root.into(),
                path: "project".into()
            }
        )
        .await?
        .0,
        403
    );
    for id in [&duplicate, "missing", "pending-unstable", "tmp:unstable"] {
        assert_eq!(
            host.action(
                &device,
                74,
                HostAction::SessionFiles {
                    session_id: id.into()
                }
            )
            .await?
            .0,
            403
        );
    }
    let ticket = host
        .owner(&[
            "pair",
            "--sessions",
            "observe",
            "--read-root",
            cwd.to_str().context("cwd")?,
            "--output",
            "json",
        ])
        .await?;
    let qr = PairingQr::parse(&serde_json::to_vec(&ticket)?, now()?)?;
    let narrow = host.enroll(&qr, 75).await?;
    assert_eq!(host.action(&narrow, 75, action()).await?.0, 200);
    assert_eq!(
        host.action(
            &narrow,
            75,
            HostAction::SessionFiles {
                session_id: other_id
            }
        )
        .await?
        .0,
        403
    );
    // Cache contains a valid canonical row; the actual Files descriptor must
    // nevertheless be opened/checked on every request, not held as row authority.
    std::fs::rename(&cwd, host.work.join("old-project"))?;
    std::fs::create_dir(&cwd)?;
    let (status, error) = host.action(&narrow, 75, action()).await?;
    assert_eq!(status, 403, "replaced grant root must require re-approval");
    assert_eq!(error["error"], "file_grant_reapproval");
    assert_eq!(
        host.action(&device, 74, action()).await?.0,
        200,
        "unchanged containing grant remains valid"
    );
    std::fs::remove_dir(&cwd)?;
    assert_eq!(
        host.action(&device, 74, action()).await?.0,
        404,
        "cached binding must not fabricate a present cwd"
    );
    assert!(
        !host.home.join("pi-invocations").exists(),
        "Files must never invoke Pi or obtain an input lease"
    );
    host.stop().await?;
    Ok(())
}

#[tokio::test]
async fn signed_session_files_incomplete_catalog_is_not_ready_not_a_grant_denial() -> Result<()> {
    let host = Host::start_mode(true).await?;
    let public = oqto_history::oqto_log::store::platform_id_for_external_id("synthetic-ready");
    seed(
        &host,
        &host.work.join("project"),
        &public,
        &public,
        "synthetic-ready",
    )
    .await?;
    let bad_work = host.work.join("zzz-corrupt");
    std::fs::create_dir(&bad_work)?;
    let db = oqto_history::oqto_log::paths::existing_user_home_workspace_db_path(
        &host.home,
        bad_work.to_str().context("workspace")?,
    );
    std::fs::create_dir_all(db.parent().context("parent")?)?;
    std::fs::write(db, "synthetic corrupt shard")?;
    let device = host.enroll(&host.qr, 76).await?;
    let (status, value) = host
        .action(&device, 76, HostAction::SessionFiles { session_id: public })
        .await?;
    assert_eq!(status, 503, "{value}");
    assert_eq!(value["error"], "unavailable");
    assert_eq!(
        host.action(&device, 76, HostAction::Capabilities).await?.0,
        200
    );
    assert!(!host.home.join("pi-invocations").exists());
    host.stop().await?;
    Ok(())
}
