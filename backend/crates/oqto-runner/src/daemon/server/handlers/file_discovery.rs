//! Runner-local discovery and preview. The control plane authorizes `root`;
//! this module independently confines every returned/read path to that root.
use super::super::*;
use anyhow::{Context, Result, bail};
use std::path::{Component, Path};
use std::sync::OnceLock;
use std::time::{Duration, UNIX_EPOCH};
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};
use tokio::sync::Semaphore;

const MAX_QUERY: usize = 256;
const MAX_INPUT: u64 = 1024 * 1024;
const MAX_OUTPUT: u64 = 256 * 1024;
const MAX_MATCHES: usize = 100;
const MAX_PREVIEW: u32 = 16 * 1024;
const DEADLINE: Duration = Duration::from_secs(5);

fn discovery_slots() -> &'static Semaphore {
    static SLOTS: OnceLock<Semaphore> = OnceLock::new();
    SLOTS.get_or_init(|| Semaphore::new(2))
}

fn authorized_root(root: &Path, configured_root: &Path) -> Result<std::path::PathBuf> {
    if !root.is_absolute() || !configured_root.is_absolute() {
        bail!("unconfigured or relative runner workspace root");
    }
    let allowed = configured_root
        .canonicalize()
        .context("runner workspace root unavailable")?;
    let requested = root.canonicalize().context("work directory unavailable")?;
    if !requested.starts_with(&allowed) {
        bail!("work directory outside runner workspace root");
    }
    Ok(requested)
}

fn checked_path(root: &Path, relative: &Path) -> Result<(std::path::PathBuf, std::path::PathBuf)> {
    if root.as_os_str().len() > 4096 || relative.as_os_str().len() > 1024 {
        bail!("file path exceeds limit");
    }
    if !root.is_absolute()
        || relative.is_absolute()
        || relative.as_os_str().is_empty()
        || relative
            .components()
            .any(|part| !matches!(part, Component::Normal(_) | Component::CurDir))
    {
        bail!("invalid work directory or relative path");
    }
    let root = root.canonicalize().context("work directory unavailable")?;
    if !root.is_dir() {
        bail!("work directory is not a directory");
    }
    let path = root
        .join(relative)
        .canonicalize()
        .context("path unavailable")?;
    if !path.starts_with(&root) {
        bail!("path outside work directory");
    }
    Ok((root, path))
}

fn version(metadata: &std::fs::Metadata) -> String {
    let nanos = metadata
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_nanos());
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        format!(
            "{}:{nanos}:{}:{}:{}:{}",
            metadata.len(),
            metadata.dev(),
            metadata.ino(),
            metadata.ctime(),
            metadata.ctime_nsec()
        )
    }
    #[cfg(not(unix))]
    {
        format!("{}:{nanos}", metadata.len())
    }
}

fn search_match(
    root: &Path,
    name: &str,
    line: Option<u64>,
    snippet: Option<String>,
) -> Option<FileSearchMatch> {
    let rel = Path::new(name)
        .strip_prefix("./")
        .unwrap_or(Path::new(name));
    if rel.is_absolute()
        || rel
            .components()
            .any(|c| !matches!(c, Component::Normal(_) | Component::CurDir))
    {
        return None;
    }
    let (_, path) = checked_path(root, rel).ok()?;
    let metadata = path.metadata().ok()?;
    if !metadata.is_file() && (line.is_some() || !metadata.is_dir()) {
        return None;
    }
    let relative = path.strip_prefix(root).ok()?.to_str()?.to_string();
    if relative.len() > 1024 {
        return None;
    }
    Some(FileSearchMatch {
        path: relative,
        is_dir: metadata.is_dir(),
        line,
        snippet,
        size: metadata.len(),
        version: version(&metadata),
    })
}

async fn bounded_output(mut child: tokio::process::Child, cap: u64) -> Result<Vec<u8>> {
    let stdout = child.stdout.take().context("tool stdout unavailable")?;
    let mut output = Vec::new();
    stdout.take(cap + 1).read_to_end(&mut output).await?;
    if output.len() as u64 > cap {
        bail!("search output limit exceeded");
    }
    let status = child.wait().await?;
    // rg/fzf use 1 for no matches.
    if !status.success() && status.code() != Some(1) {
        bail!("search tool failed: {status}");
    }
    Ok(output)
}

struct SearchTools<'a> {
    fd: &'a str,
    fzf: &'a str,
    rg: &'a str,
}

async fn run_search(req: SearchFilesRequest) -> Result<FileSearchResponse> {
    run_search_with_tools(
        req,
        SearchTools {
            fd: "fd",
            fzf: "fzf",
            rg: "rg",
        },
    )
    .await
}

async fn run_search_with_tools(
    req: SearchFilesRequest,
    tools: SearchTools<'_>,
) -> Result<FileSearchResponse> {
    let _slot = discovery_slots().acquire().await?;
    if req.query.is_empty() || req.query.len() > MAX_QUERY || req.query.contains('\0') {
        bail!("query must be 1..256 bytes without NUL");
    }
    let (root, directory) = checked_path(&req.root, &req.path)?;
    if !directory.is_dir() {
        bail!("search path is not a directory");
    }
    let rel = directory.strip_prefix(&root)?;
    let rel = if rel.as_os_str().is_empty() {
        Path::new(".")
    } else {
        rel
    };
    let bytes = match req.mode {
        FileSearchMode::Name => {
            let mut fd = Command::new(tools.fd);
            fd.current_dir(&root)
                .args([
                    "--print0", "--type", "f", "--type", "d", "--type", "l", "--color", "never",
                ])
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .kill_on_drop(true);
            if req.include_hidden {
                fd.arg("--hidden");
            }
            fd.arg(".").arg(rel);
            let input = bounded_output(fd.spawn().context("fd unavailable")?, MAX_INPUT).await?;
            let mut fzf = Command::new(tools.fzf);
            fzf.env_remove("FZF_DEFAULT_OPTS")
                .env_remove("FZF_DEFAULT_COMMAND")
                .args(["--read0", "--print0", "--filter"])
                .arg(&req.query)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .kill_on_drop(true);
            let mut child = fzf.spawn().context("fzf unavailable")?;
            child
                .stdin
                .take()
                .context("fzf stdin unavailable")?
                .write_all(&input)
                .await?;
            bounded_output(child, MAX_OUTPUT).await?
        }
        FileSearchMode::Content => {
            let mut rg = Command::new(tools.rg);
            rg.current_dir(&root)
                .args([
                    "--json",
                    "--fixed-strings",
                    "--no-config",
                    "--no-messages",
                    "--max-filesize",
                    "2M",
                    "--max-count",
                    "8",
                    "-e",
                ])
                .arg(&req.query)
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .kill_on_drop(true);
            if req.include_hidden {
                rg.arg("--hidden");
            }
            rg.arg("--").arg(rel);
            bounded_output(rg.spawn().context("rg unavailable")?, MAX_OUTPUT).await?
        }
    };
    let mut matches = Vec::new();
    let mut truncated = false;
    match req.mode {
        FileSearchMode::Name => {
            for raw in bytes.split(|b| *b == 0).filter(|b| !b.is_empty()) {
                if matches.len() == MAX_MATCHES {
                    truncated = true;
                    break;
                }
                if let Ok(name) = std::str::from_utf8(raw)
                    && let Some(item) = search_match(&root, name, None, None)
                {
                    matches.push(item);
                }
            }
        }
        FileSearchMode::Content => {
            for raw in bytes.split(|b| *b == b'\n') {
                if matches.len() == MAX_MATCHES {
                    truncated = true;
                    break;
                }
                let Ok(value) = serde_json::from_slice::<serde_json::Value>(raw) else {
                    continue;
                };
                if value.get("type").and_then(|v| v.as_str()) != Some("match") {
                    continue;
                }
                let Some(name) = value.pointer("/data/path/text").and_then(|v| v.as_str()) else {
                    continue;
                };
                let line = value.pointer("/data/line_number").and_then(|v| v.as_u64());
                let snippet = value
                    .pointer("/data/lines/text")
                    .and_then(|v| v.as_str())
                    .map(|s| s.chars().take(240).collect::<String>());
                if let Some(item) = search_match(&root, name, line, snippet) {
                    matches.push(item);
                }
            }
        }
    }
    Ok(FileSearchResponse { matches, truncated })
}

async fn run_preview(req: PreviewFileRequest) -> Result<FilePreviewResponse> {
    let _slot = discovery_slots().acquire().await?;
    if req
        .expected_version
        .as_ref()
        .is_some_and(|version| version.len() > 128)
    {
        bail!("preview version exceeds limit");
    }
    if req.limit == 0 || req.limit > MAX_PREVIEW || req.offset > 1024 * 1024 * 1024 {
        bail!("preview byte range exceeds limit");
    }
    let (root, path) = checked_path(&req.root, &req.path)?;
    let relative = path
        .strip_prefix(&root)?
        .to_str()
        .context("non-UTF8 path unsupported")?
        .to_string();
    // Canonical containment is rechecked after open, but concurrent directory
    // replacement remains a TOCTOU risk; this is not a sandbox boundary.
    let mut file = tokio::fs::File::open(&path).await?;
    let (_, after_open) = checked_path(&req.root, &req.path)?;
    if after_open != path {
        bail!("path changed during preview");
    }
    let metadata = file.metadata().await?;
    if !metadata.is_file() {
        bail!("preview requires a regular file");
    }
    let current = version(&metadata);
    if req
        .expected_version
        .as_deref()
        .is_some_and(|expected| expected != current)
    {
        bail!("stale preview version");
    }
    let modified_at_ms = metadata
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_millis() as i64);
    // No remote media reads: only known text extensions are eligible for bounded bytes.
    let text = path.extension().and_then(|e| e.to_str()).is_some_and(|e| {
        matches!(
            e.to_ascii_lowercase().as_str(),
            "txt"
                | "md"
                | "markdown"
                | "json"
                | "jsonl"
                | "toml"
                | "yaml"
                | "yml"
                | "rs"
                | "ts"
                | "tsx"
                | "js"
                | "jsx"
                | "py"
                | "go"
                | "sh"
                | "css"
                | "html"
                | "xml"
                | "log"
                | "csv"
        )
    });
    let (content, unavailable) = if text {
        file.seek(std::io::SeekFrom::Start(req.offset)).await?;
        let mut bytes = Vec::with_capacity(req.limit as usize);
        (&mut file)
            .take(req.limit as u64)
            .read_to_end(&mut bytes)
            .await?;
        if version(&file.metadata().await?) != current
            || version(&tokio::fs::metadata(&path).await?) != current
            || checked_path(&req.root, &req.path)?.1 != path
        {
            bail!("stale preview version");
        }
        // A bounded read may stop inside the final UTF-8 code point. Return the
        // complete prefix, but never conceal an invalid byte inside the range.
        let valid_len = match std::str::from_utf8(&bytes) {
            Ok(_) => Some(bytes.len()),
            Err(error) if error.error_len().is_none() => Some(error.valid_up_to()),
            Err(_) => None,
        };
        match valid_len.and_then(|len| std::str::from_utf8(&bytes[..len]).ok()) {
            Some(text) if !text.contains('\0') => (Some(text.to_string()), None),
            _ => (None, Some("binary or invalid UTF-8 content".to_string())),
        }
    } else {
        (
            None,
            Some("unsupported preview type; metadata only".to_string()),
        )
    };
    Ok(FilePreviewResponse {
        path: relative,
        size: metadata.len(),
        version: current,
        modified_at_ms,
        content,
        unavailable,
        truncated: req.offset.saturating_add(req.limit as u64) < metadata.len(),
    })
}

pub(crate) async fn search(runner: &Runner, mut req: SearchFilesRequest) -> RunnerResponse {
    req.root = match authorized_root(&req.root, &runner.user_config.workspace_dir) {
        Ok(root) => root,
        Err(err) => {
            return error_response(
                ErrorCode::PermissionDenied,
                format!("file search denied: {err:#}"),
            );
        }
    };
    match tokio::time::timeout(DEADLINE, run_search(req)).await {
        Ok(Ok(result)) => RunnerResponse::FileSearch(result),
        Ok(Err(err)) => error_response(ErrorCode::InvalidRequest, format!("file search: {err:#}")),
        Err(_) => error_response(ErrorCode::InvalidRequest, "file search timed out"),
    }
}

pub(crate) async fn preview(runner: &Runner, mut req: PreviewFileRequest) -> RunnerResponse {
    req.root = match authorized_root(&req.root, &runner.user_config.workspace_dir) {
        Ok(root) => root,
        Err(err) => {
            return error_response(
                ErrorCode::PermissionDenied,
                format!("file preview denied: {err:#}"),
            );
        }
    };
    match tokio::time::timeout(DEADLINE, run_preview(req)).await {
        Ok(Ok(result)) => RunnerResponse::FilePreview(result),
        Ok(Err(err)) => error_response(ErrorCode::InvalidRequest, format!("file preview: {err:#}")),
        Err(_) => error_response(ErrorCode::InvalidRequest, "file preview timed out"),
    }
}

#[cfg(test)]
mod tests {
    include!("file_discovery_tests.rs");
}
