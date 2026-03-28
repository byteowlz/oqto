use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use tokio::process::Child;
use tokio::sync::{Mutex, RwLock, broadcast};

/// Session state tracked by the runner.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct SessionState {
    pub id: String,
    pub workspace_path: PathBuf,
    pub fileserver_id: String,
    pub ttyd_id: String,
    pub fileserver_port: u16,
    pub ttyd_port: u16,
    pub agent: Option<String>,
    pub started_at: std::time::Instant,
}

/// Stdout buffer shared between the reader task and the main runner.
#[derive(Debug)]
pub struct StdoutBuffer {
    pub lines: Vec<String>,
    pub closed: bool,
    pub exit_code: Option<i32>,
}

impl StdoutBuffer {
    pub fn new() -> Self {
        Self {
            lines: Vec::new(),
            closed: false,
            exit_code: None,
        }
    }
}

impl Default for StdoutBuffer {
    fn default() -> Self {
        Self::new()
    }
}

/// Message sent on the stdout broadcast channel.
#[derive(Debug, Clone)]
pub enum StdoutEvent {
    Line(String),
    Closed { exit_code: Option<i32> },
}

/// Managed process with optional RPC pipes.
pub struct ManagedProcess {
    pub id: String,
    pub pid: u32,
    pub binary: String,
    pub cwd: PathBuf,
    pub child: Child,
    pub is_rpc: bool,
    pub stdout_buffer: Option<Arc<Mutex<StdoutBuffer>>>,
    pub stdout_tx: Option<broadcast::Sender<StdoutEvent>>,
    pub _reader_handle: Option<tokio::task::JoinHandle<()>>,
}

impl ManagedProcess {
    pub fn is_running(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }

    pub fn exit_code(&mut self) -> Option<i32> {
        match self.child.try_wait() {
            Ok(Some(status)) => status.code(),
            _ => None,
        }
    }
}

/// Runner daemon state.
pub struct RunnerState {
    pub processes: HashMap<String, ManagedProcess>,
    pub sessions: HashMap<String, SessionState>,
}

impl RunnerState {
    pub fn new() -> Self {
        Self {
            processes: HashMap::new(),
            sessions: HashMap::new(),
        }
    }
}

impl Default for RunnerState {
    fn default() -> Self {
        Self::new()
    }
}

pub type SharedRunnerState = Arc<RwLock<RunnerState>>;
