use super::*;

#[tokio::test]
async fn saved_listener_retries_exact_plan_after_transient_startup_bind_failure() -> Result<()> {
    let temp = tempfile::Builder::new()
        .prefix("oqretry-")
        .tempdir_in("/tmp")?;
    let home = temp.path().canonicalize()?;
    std::fs::create_dir(home.join("work"))?;
    std::fs::create_dir(home.join("runtime"))?;
    std::fs::set_permissions(home.join("runtime"), std::fs::Permissions::from_mode(0o700))?;
    let mut first = Host::start(&home)?;
    first.ready().await?;
    let control = pair_control::root(&home);
    let selected = plan(&home)?;
    let original = pair_control::request(
        &control,
        &Request::Enable {
            plan: selected.clone(),
        },
    )
    .await?;
    first.stop()?;
    let occupied = std::net::TcpListener::bind(selected.address)?;
    let mut second = Host::start(&home)?;
    second.ready().await?;
    let before = pair_control::request(&control, &Request::Status).await?;
    ensure!(!before.active && before.plan == Some(selected.clone()));
    drop(occupied);
    let recovered = pair_control::request(
        &control,
        &Request::Enable {
            plan: selected.clone(),
        },
    )
    .await?;
    ensure!(
        recovered.active
            && recovered.pid == second.child.id()
            && recovered.state == original.state
            && recovered.plan == Some(selected)
    );
    second.stop()?;
    Ok(())
}
