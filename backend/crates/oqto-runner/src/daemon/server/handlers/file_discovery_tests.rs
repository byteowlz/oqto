use super::*;
use crate::protocol::{FileSearchMode, PreviewFileRequest, SearchFilesRequest};
use std::path::{Path, PathBuf};

fn search(root: &Path, path: &str, query: &str, mode: FileSearchMode) -> SearchFilesRequest {
    SearchFilesRequest { root: root.to_path_buf(), path: PathBuf::from(path), query: query.into(), mode, include_hidden: false }
}
fn preview(root: &Path, path: &str, limit: u32, expected_version: Option<String>) -> PreviewFileRequest {
    PreviewFileRequest { root: root.to_path_buf(), path: PathBuf::from(path), offset: 0, limit, expected_version }
}

#[test]
fn canonical_scope_rejects_traversal_and_symlink_escape() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let outside = tempfile::tempdir()?;
    std::fs::write(outside.path().join("secret.txt"), "private")?;
    let configured = tempfile::tempdir()?;
    assert!(authorized_root(root.path(), configured.path()).is_err());
    assert!(authorized_root(root.path(), root.path()).is_ok());
    #[cfg(unix)]
    std::os::unix::fs::symlink(outside.path(), root.path().join("escape"))?;
    assert!(checked_path(root.path(), Path::new("../secret.txt")).is_err());
    assert!(checked_path(root.path(), outside.path()).is_err());
    #[cfg(unix)]
    assert!(checked_path(root.path(), Path::new("escape/secret.txt")).is_err());
    #[cfg(unix)]
    assert!(authorized_root(&root.path().join("escape"), root.path()).is_err());
    Ok(())
}

#[tokio::test]
async fn preview_is_bounded_and_rejects_stale_and_media() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let large = root.path().join("large.txt");
    let mut file = std::fs::File::create(&large)?;
    file.set_len(1024 * 1024 * 1024)?; // sparse GiB file; reading it whole would OOM
    use std::io::Write;
    file.write_all(&vec![b'x'; 1024])?;
    let first = run_preview(preview(root.path(), "large.txt", 1024, None)).await?;
    assert_eq!(first.content.as_ref().map(String::len), Some(1024));
    assert!(first.truncated);
    assert!(run_preview(preview(root.path(), "large.txt", 16385, None)).await.is_err());
    assert!(run_preview(preview(root.path(), "large.txt", 32, Some("stale".into()))).await.is_err());
    file.set_len(1024)?;
    assert!(run_preview(preview(root.path(), "large.txt", 32, Some(first.version))).await.is_err());
    let image = root.path().join("image.png");
    std::fs::write(&image, b"png")?;
    let result = run_preview(preview(root.path(), "image.png", 32, None)).await?;
    assert!(result.content.is_none());
    assert_eq!(result.unavailable.as_deref(), Some("unsupported preview type; metadata only"));
    Ok(())
}

#[tokio::test]
async fn odd_filenames_are_arguments_not_shell() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let name = "odd ; $(touch oqto-search-unsafe) ' file.txt";
    std::fs::write(root.path().join(name), "unique literal")?;
    let result = run_search(search(root.path(), ".", "odd", FileSearchMode::Name)).await?;
    assert_eq!(result.matches.len(), 1);
    assert_eq!(result.matches[0].path, name);
    let content = run_search(search(root.path(), ".", "unique literal", FileSearchMode::Content)).await?;
    assert_eq!(content.matches.len(), 1);
    assert_eq!(content.matches[0].line, Some(1));
    assert!(!root.path().join("oqto-search-unsafe").exists());
    Ok(())
}

#[tokio::test]
async fn name_search_returns_navigable_directories() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    std::fs::create_dir(root.path().join("folder-hit"))?;
    let results = run_search(search(root.path(), ".", "folder-hit", FileSearchMode::Name)).await?;
    assert!(results.matches.iter().any(|item| item.path == "folder-hit" && item.is_dir));
    Ok(())
}

#[cfg(unix)]
#[tokio::test]
async fn missing_tools_and_deadline_fail_explicitly() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let absent = root.path().join("no-such-tool");
    let missing = absent.to_str().ok_or_else(|| anyhow::anyhow!("non UTF-8 temp path"))?;
    let name = search(root.path(), ".", "needle", FileSearchMode::Name);
    assert!(run_search_with_tools(name.clone(), SearchTools { fd: missing, fzf: "fzf", rg: "rg" }).await
        .is_err());
    assert!(run_search_with_tools(name, SearchTools { fd: "fd", fzf: missing, rg: "rg" }).await
        .is_err());
    assert!(run_search_with_tools(search(root.path(), ".", "needle", FileSearchMode::Content),
        SearchTools { fd: "fd", fzf: "fzf", rg: missing }).await.is_err());
    let sleeper = root.path().join("slow-tool");
    std::fs::write(&sleeper, "#!/bin/sh\nexec sleep 10\n")?;
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&sleeper, std::fs::Permissions::from_mode(0o700))?;
    let tool = sleeper.to_str().ok_or_else(|| anyhow::anyhow!("non UTF-8 temp path"))?;
    let timed = tokio::time::timeout(std::time::Duration::from_millis(50),
        run_search_with_tools(search(root.path(), ".", "needle", FileSearchMode::Content),
            SearchTools { fd: "fd", fzf: "fzf", rg: tool })).await;
    assert!(timed.is_err(), "hung tool must be cancellable by deadline");
    Ok(())
}

#[tokio::test]
async fn hidden_files_require_explicit_opt_in() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    std::fs::write(root.path().join(".hidden.txt"), "invisible needle")?;
    let normal = run_search(search(root.path(), ".", "hidden", FileSearchMode::Name)).await?;
    assert!(normal.matches.is_empty());
    let mut enabled = search(root.path(), ".", "hidden", FileSearchMode::Name);
    enabled.include_hidden = true;
    assert_eq!(run_search(enabled).await?.matches.len(), 1);
    let mut content = search(root.path(), ".", "invisible needle", FileSearchMode::Content);
    content.include_hidden = true;
    assert_eq!(run_search(content).await?.matches.len(), 1);
    Ok(())
}

#[tokio::test]
async fn search_respects_runner_concurrency_slots() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let first = discovery_slots().acquire().await?;
    let second = discovery_slots().acquire().await?;
    let blocked = tokio::time::timeout(std::time::Duration::from_millis(50),
        run_search(search(root.path(), ".", "item", FileSearchMode::Name))).await;
    assert!(blocked.is_err(), "third search must wait for an available slot");
    drop(first);
    drop(second);
    Ok(())
}

#[tokio::test]
async fn invalid_queries_and_out_of_root_preview_fail() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let outside = tempfile::tempdir()?;
    std::fs::write(outside.path().join("secret.txt"), b"secret")?;
    assert!(run_search(search(root.path(), "../", "secret", FileSearchMode::Name)).await.is_err());
    assert!(run_search(search(root.path(), ".", &"x".repeat(257), FileSearchMode::Content)).await.is_err());
    assert!(run_preview(preview(root.path(), "../secret.txt", 32, None)).await.is_err());
    Ok(())
}
