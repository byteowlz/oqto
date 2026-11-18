//! Destination hands generations: one fresh confined launch per admitted
//! call through the platform's existing launcher (`oqt_hands`: Linux bwrap
//! with seccomp and Landlock, macOS Seatbelt with launch markers, ADR-0084).
//! A stop is confirmed only by the platform supervisor: on Linux the bwrap
//! PID namespace ends with its init; on macOS a complete marker scan
//! (`oqt_hands reap`) found no live or undecidable member.
use crate::{config::LauncherConfig, wire::StopState};
use anyhow::{Context, Result, ensure};
use async_trait::async_trait;
use std::{
    os::unix::fs::{DirBuilderExt, OpenOptionsExt},
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};
use tokio::{
    io::{AsyncRead, AsyncWrite},
    process::{Child, Command},
};

pub type Input = Box<dyn AsyncWrite + Send + Unpin>;
pub type Output = Box<dyn AsyncRead + Send + Unpin>;

/// Destination binding as compiled for a launch.
#[derive(Clone, Debug)]
pub struct LaunchBinding {
    pub root: PathBuf,
}

#[async_trait]
pub trait Launcher: Send + Sync {
    async fn launch(
        &self,
        binding: &LaunchBinding,
        generation: u64,
        sync_token: &str,
    ) -> Result<Box<dyn Generation>>;
}

#[async_trait]
pub trait Generation: Send {
    /// The daemon's stdin and stdout, taken once.
    fn take_io(&mut self) -> Option<(Input, Output)>;
    /// Stop the generation and every descendant; report what is confirmed.
    async fn stop(&mut self) -> StopState;
}

pub struct OqtHandsLauncher {
    config: LauncherConfig,
    generations: PathBuf,
    protected: Vec<PathBuf>,
}

impl OqtHandsLauncher {
    pub fn new(config: LauncherConfig, state_dir: &Path) -> Result<Self> {
        let generations = state_dir.join("generations");
        private_dir(&generations)?;
        let mut protected = config.protected_roots.clone();
        protected.push(state_dir.to_path_buf());
        Ok(Self {
            config,
            generations,
            protected,
        })
    }

    /// macOS: a dead host's launches carry markers; reap them before serving.
    /// Returns the generation directories that could not be confirmed.
    pub async fn reap_leftovers(&self) -> Result<Vec<PathBuf>> {
        let mut unconfirmed = Vec::new();
        for entry in std::fs::read_dir(&self.generations)? {
            let path = entry?.path();
            if cfg!(target_os = "macos") && !reap(&self.config.oqt_hands, &path).await {
                unconfirmed.push(path);
                continue;
            }
            std::fs::remove_dir_all(&path)
                .with_context(|| format!("removing {}", path.display()))?;
        }
        Ok(unconfirmed)
    }
}

fn private_dir(path: &Path) -> Result<()> {
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(path)
        .with_context(|| format!("creating {}", path.display()))?;
    use std::os::unix::fs::MetadataExt;
    let meta = std::fs::symlink_metadata(path)?;
    ensure!(
        meta.is_dir()
            && meta.mode() & 0o077 == 0
            && meta.uid() == rustix::process::geteuid().as_raw(),
        "{} must be a private owned directory",
        path.display()
    );
    Ok(())
}

fn private_file(path: &Path, bytes: &[u8]) -> Result<()> {
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .with_context(|| format!("creating {}", path.display()))?;
    file.write_all(bytes)?;
    Ok(())
}

#[async_trait]
impl Launcher for OqtHandsLauncher {
    async fn launch(
        &self,
        binding: &LaunchBinding,
        generation: u64,
        sync_token: &str,
    ) -> Result<Box<dyn Generation>> {
        ensure!(
            sync_token.len() == 32 && sync_token.bytes().all(|b| b.is_ascii_hexdigit()),
            "invalid daemon sync token"
        );
        let dir = self.generations.join(format!("g{generation}"));
        ensure!(!dir.exists(), "generation directory already exists");
        private_dir(&dir)?;
        let mut spec = serde_json::json!({
            "workspace": binding.root,
            "daemon": self.config.daemon,
            "protected_roots": self.protected,
            "read_paths": [],
            "write_paths": [],
            "network": false,
            "mask_paths": [],
        });
        let mut scratch = None;
        if cfg!(target_os = "macos") {
            // Launch markers: one readable and one denied private file.
            private_file(&dir.join("marker-allow"), b"")?;
            private_file(&dir.join("marker-deny"), b"")?;
            let path = tempfile_dir()?;
            spec["marker"] = serde_json::json!(dir);
            spec["scratch"] = serde_json::json!(path);
            scratch = Some(path);
        } else {
            spec["bpf"] = serde_json::json!(self.config.bpf);
        }
        let spec_path = dir.join("hands.json");
        private_file(&spec_path, &serde_json::to_vec(&spec)?)?;
        let mut command = Command::new(&self.config.oqt_hands);
        command
            .arg(&spec_path)
            .args(["serve", "--token", sync_token])
            .env_clear()
            .env("PATH", "/usr/local/bin:/usr/bin:/bin")
            .env("HOME", std::env::var_os("HOME").unwrap_or_default())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        if let Some(shim) = &self.config.shim {
            command.env("OQTO_SANDBOX_SHIM_BIN", shim);
        }
        let mut child = command.spawn().context("starting confined hands")?;
        let stdin = child.stdin.take().context("hands stdin")?;
        let stdout = child.stdout.take().context("hands stdout")?;
        Ok(Box::new(OqtGeneration {
            child,
            io: Some((Box::new(stdin), Box::new(stdout))),
            dir,
            scratch,
            launcher: self.config.oqt_hands.clone(),
            stopped: None,
        }))
    }
}

fn tempfile_dir() -> Result<PathBuf> {
    // Seatbelt has no private /tmp: a fresh owned scratch outside every
    // protected root, wiped after the generation.
    let path = PathBuf::from("/var/tmp").join(format!("oqhs-{}", crate::wire::random_hex(8)));
    private_dir(&path)?;
    Ok(path)
}

struct OqtGeneration {
    child: Child,
    io: Option<(Input, Output)>,
    dir: PathBuf,
    scratch: Option<PathBuf>,
    launcher: PathBuf,
    stopped: Option<StopState>,
}

#[async_trait]
impl Generation for OqtGeneration {
    fn take_io(&mut self) -> Option<(Input, Output)> {
        self.io.take()
    }
    async fn stop(&mut self) -> StopState {
        if let Some(stopped) = self.stopped {
            return stopped;
        }
        self.io = None;
        let state = stop_child(&mut self.child).await;
        let state = if state == StopState::Confirmed && cfg!(target_os = "macos") {
            if reap(&self.launcher, &self.dir).await {
                StopState::Confirmed
            } else {
                StopState::Unconfirmed
            }
        } else {
            state
        };
        if state == StopState::Confirmed {
            // Keep the directory (and its markers) when the stop is unconfirmed.
            let _ = std::fs::remove_dir_all(&self.dir);
            if let Some(scratch) = &self.scratch {
                let _ = std::fs::remove_dir_all(scratch);
            }
        }
        self.stopped = Some(state);
        state
    }
}

async fn stop_child(child: &mut Child) -> StopState {
    if matches!(child.try_wait(), Ok(Some(_))) {
        return StopState::Confirmed;
    }
    if let Some(pid) = child
        .id()
        .and_then(|pid| rustix::process::Pid::from_raw(pid as i32))
    {
        let _ = rustix::process::kill_process(pid, rustix::process::Signal::TERM);
    }
    if let Ok(Ok(_)) = tokio::time::timeout(Duration::from_secs(1), child.wait()).await {
        return StopState::Confirmed;
    }
    let _ = child.start_kill();
    match tokio::time::timeout(Duration::from_secs(3), child.wait()).await {
        Ok(Ok(_)) => StopState::Confirmed,
        _ => StopState::Unconfirmed,
    }
}

/// macOS: confirm a launch's marked processes are gone (ADR-0084).
async fn reap(launcher: &Path, dir: &Path) -> bool {
    let output = tokio::time::timeout(
        Duration::from_secs(10),
        Command::new(launcher)
            .arg("reap")
            .arg(dir)
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output(),
    )
    .await;
    let Ok(Ok(output)) = output else {
        return false;
    };
    output.status.success()
        && serde_json::from_slice::<serde_json::Value>(&output.stdout)
            .is_ok_and(|report| report["confirmed"] == true)
}
