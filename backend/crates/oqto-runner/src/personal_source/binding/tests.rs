use super::*;
use std::os::unix::fs::PermissionsExt;

#[test]
fn startup_binding_is_explicit_immutable_and_preserves_unknown_data() -> Result<()> {
    let temporary = tempfile::tempdir()?;
    let root = temporary.path().canonicalize()?;
    let home = root.join("service");
    let sessions = root.join("native-user/.pi/agent/sessions");
    let sockets = root.join("native-tmp");
    let workspace = root.join("work");
    for directory in [&home, &sessions, &sockets, &workspace] {
        std::fs::create_dir_all(directory)?;
    }
    ensure!(load(&home)?.is_none());
    ensure!(!crate::pair_control::root(&home).exists());
    let binding = Binding {
        schema: 1,
        session_root: sessions,
        socket_root: sockets,
        workspaces: vec![workspace],
    };
    install(&home, &binding)?;
    let bytes = std::fs::read(path(&home))?;
    install(&home, &binding)?;
    ensure!(std::fs::read(path(&home))? == bytes);
    let selected = for_startup(&home, None)?.context("missing binding")?;
    ensure!(selected.home == home && selected.session_root() == binding.session_root);
    ensure!(
        selected.socket_root() == binding.socket_root
            && selected.workspace_roots() == binding.workspaces
    );
    ensure!(for_startup(&home, Some(selected.clone()))?.is_some());
    let other = root.join("other");
    std::fs::create_dir(&other)?;
    let changed = Binding {
        workspaces: vec![other],
        ..binding.clone()
    };
    ensure!(install(&home, &changed).is_err());
    ensure!(for_startup(&home, Some(std::sync::Arc::new(changed.resolve(&home)?))).is_err());
    ensure!(std::fs::read(path(&home))? == bytes);
    std::fs::set_permissions(path(&home), std::fs::Permissions::from_mode(0o644))?;
    ensure!(load(&home).is_err());
    std::fs::set_permissions(path(&home), std::fs::Permissions::from_mode(0o600))?;
    let saved = root.join("saved-source.json");
    std::fs::rename(path(&home), &saved)?;
    std::os::unix::fs::symlink(&saved, path(&home))?;
    ensure!(load(&home).is_err());
    ensure!(std::fs::read(&saved)? == bytes);
    Ok(())
}
