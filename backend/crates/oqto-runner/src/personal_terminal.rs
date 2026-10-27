//! Runner-owned PTYs. Client disconnect/revocation only removes access; registry
//! ownership retains the master and child. Never adopt/kill a terminal's Pi PID.
use base64::Engine;
use oqto_personal::{AuthorizedAction, actions::ActionFailure};
use oqto_personal_wire::actions::HostAction;
use portable_pty::{Child, CommandBuilder, MasterPty, PtySize};
use serde_json::{Value, json};
use std::{
    collections::{HashMap, VecDeque},
    io::Write,
    os::fd::BorrowedFd,
    sync::{Arc, Mutex},
};
use tokio::io::unix::AsyncFd;

const OUTPUT_BOUND: usize = 1024 * 1024;
#[derive(Default)]
struct Output {
    bytes: VecDeque<u8>,
    start: u64,
    end: u64,
    ended: bool,
}
struct Control {
    master: Box<dyn MasterPty + Send>,
    child: Box<dyn Child + Send + Sync>,
    writer: Box<dyn Write + Send>,
}
struct Terminal {
    owner: String,
    session: Option<String>,
    control: Control,
    output: Arc<Mutex<Output>>,
    reader: tokio::task::JoinHandle<()>,
}
impl Drop for Terminal {
    fn drop(&mut self) {
        self.reader.abort();
    }
}
#[derive(Default)]
pub(crate) struct Terminals {
    session_opens: Mutex<()>,
    entries: Mutex<HashMap<String, Terminal>>,
}
fn denied<T>(_: T) -> ActionFailure {
    ActionFailure::Denied
}
fn size(rows: u16, cols: u16) -> Result<PtySize, ActionFailure> {
    if rows == 0 || cols == 0 || rows > 1000 || cols > 1000 {
        return Err(ActionFailure::Invalid);
    }
    Ok(PtySize {
        rows,
        cols,
        pixel_width: 0,
        pixel_height: 0,
    })
}
impl Terminals {
    /// A repeated explicit Open reattaches the same live Session PTY, not another shell.
    pub(crate) fn open_session(
        &self,
        session: String,
        cwd: String,
        dimensions: (u16, u16),
        admission: AuthorizedAction,
    ) -> Result<Value, ActionFailure> {
        let (rows, cols) = dimensions;
        let _gate = self.session_opens.lock().map_err(denied)?;
        if !admission.grants().full_user_terminal
            || !admission.fence().is_current()
            || admission.grants().sessions == oqto_personal::SessionAccess::None
        {
            return Err(ActionFailure::Denied);
        }
        size(rows, cols)?;
        let owner = admission.fence().device_id().to_owned();
        {
            let mut entries = self.entries.lock().map_err(denied)?;
            for (id, terminal) in entries.iter_mut() {
                if terminal.owner == owner
                    && terminal.session.as_deref() == Some(&session)
                    && terminal.control.child.try_wait().map_err(denied)?.is_none()
                {
                    return Ok(json!({"terminal_id":id,"session_id":session,"reattached":true}));
                }
            }
        }
        let result = self.execute(HostAction::TerminalOpen { cwd, rows, cols }, admission)?;
        let id = result["terminal_id"]
            .as_str()
            .ok_or(ActionFailure::OutcomeUnknown)?;
        let mut entries = self.entries.lock().map_err(denied)?;
        entries
            .get_mut(id)
            .ok_or(ActionFailure::OutcomeUnknown)?
            .session = Some(session.clone());
        Ok(json!({"terminal_id":id,"session_id":session,"reattached":false}))
    }
    pub(crate) fn reap(&self) {
        if let Ok(mut entries) = self.entries.lock() {
            for terminal in entries.values_mut() {
                let _ = terminal.control.child.try_wait();
            }
        }
    }
    pub(crate) fn execute(
        &self,
        action: HostAction,
        admission: AuthorizedAction,
    ) -> Result<Value, ActionFailure> {
        if !admission.grants().full_user_terminal || !admission.fence().is_current() {
            return Err(ActionFailure::Denied);
        }
        let owner = admission.fence().device_id().to_owned();
        let mut entries = self.entries.lock().map_err(denied)?;
        if matches!(action, HostAction::TerminalsList) {
            return Ok(
                json!({"terminals":entries.iter().filter(|(_,terminal)| terminal.owner==owner).map(|(id,_)| json!({"terminal_id":id})).collect::<Vec<_>>()}),
            );
        }
        if let HostAction::TerminalOpen { cwd, rows, cols } = &action {
            if entries.len() >= 32
                || entries
                    .values()
                    .filter(|terminal| terminal.owner == owner)
                    .count()
                    >= 8
            {
                return Err(ActionFailure::Unavailable);
            }
            let cwd = std::path::Path::new(cwd);
            if !cwd.is_absolute() || !cwd.is_dir() {
                return Err(ActionFailure::Invalid);
            }
            let pair = portable_pty::native_pty_system()
                .openpty(size(*rows, *cols)?)
                .map_err(denied)?;
            let shell = std::env::var_os("SHELL").unwrap_or_else(|| "/bin/sh".into());
            if !std::path::Path::new(&shell).is_absolute() {
                return Err(ActionFailure::Unavailable);
            }
            let mut command = CommandBuilder::new(shell);
            command.cwd(cwd);
            command.env("TERM", "xterm-256color");
            // Prepare capture BEFORE spawning: any preparation error creates no orphan.
            let raw = pair.master.as_raw_fd().ok_or(ActionFailure::Unavailable)?;
            // SAFETY: master is owned here and remains alive until after dup returns.
            let borrowed = unsafe { BorrowedFd::borrow_raw(raw) };
            let read_fd = rustix::io::fcntl_dupfd_cloexec(borrowed, 0).map_err(denied)?;
            let flags = rustix::fs::fcntl_getfl(&read_fd).map_err(denied)?;
            rustix::fs::fcntl_setfl(&read_fd, flags | rustix::fs::OFlags::NONBLOCK)
                .map_err(denied)?;
            let reader = AsyncFd::new(read_fd).map_err(denied)?;
            let writer = pair.master.take_writer().map_err(denied)?;
            let child = pair.slave.spawn_command(command).map_err(denied)?;
            drop(pair.slave);
            let output = Arc::new(Mutex::new(Output::default()));
            let capture = Arc::clone(&output);
            let reader = tokio::spawn(async move {
                let mut bytes = [0u8; 8192];
                loop {
                    let Ok(mut ready) = reader.readable().await else {
                        break;
                    };
                    match ready.try_io(|fd| {
                        rustix::io::read(fd.get_ref(), &mut bytes).map_err(std::io::Error::from)
                    }) {
                        Ok(Ok(0)) | Ok(Err(_)) => break,
                        Err(_) => continue,
                        Ok(Ok(count)) => {
                            let Ok(mut out) = capture.lock() else {
                                break;
                            };
                            for byte in &bytes[..count] {
                                if out.bytes.len() == OUTPUT_BOUND {
                                    out.bytes.pop_front();
                                    out.start += 1;
                                }
                                out.bytes.push_back(*byte);
                                out.end += 1;
                            }
                        }
                    }
                }
                if let Ok(mut out) = capture.lock() {
                    out.ended = true;
                }
            });
            let id = uuid::Uuid::new_v4().to_string();
            entries.insert(
                id.clone(),
                Terminal {
                    owner,
                    session: None,
                    control: Control {
                        master: pair.master,
                        child,
                        writer,
                    },
                    output,
                    reader,
                },
            );
            return Ok(
                json!({"terminal_id":id,"full_user_execution":true,"containment_verified":false}),
            );
        }
        let id = match &action {
            HostAction::TerminalRead { terminal_id, .. }
            | HostAction::TerminalWrite { terminal_id, .. }
            | HostAction::TerminalResize { terminal_id, .. }
            | HostAction::TerminalClose { terminal_id } => terminal_id,
            _ => return Err(ActionFailure::Invalid),
        };
        let terminal = entries
            .get_mut(id)
            .filter(|terminal| terminal.owner == owner)
            .ok_or(ActionFailure::Denied)?;
        match &action {
            HostAction::TerminalRead { after, .. } => {
                let ended = terminal.control.child.try_wait().map_err(denied)?.is_some();
                let out = terminal.output.lock().map_err(denied)?;
                if *after > out.end {
                    return Err(ActionFailure::Invalid);
                }
                let start = (*after).max(out.start);
                let skip = usize::try_from(start - out.start).map_err(denied)?;
                let bytes = out
                    .bytes
                    .iter()
                    .skip(skip)
                    .take(64 * 1024)
                    .copied()
                    .collect::<Vec<_>>();
                Ok(
                    json!({"start":start,"next":start+bytes.len() as u64,"gap":*after<out.start,"data_base64":base64::engine::general_purpose::STANDARD.encode(&bytes),"process_exited":ended,"ended":out.ended && start+bytes.len() as u64==out.end}),
                )
            }
            HostAction::TerminalWrite { data_base64, .. } => {
                let bytes = base64::engine::general_purpose::STANDARD
                    .decode(data_base64)
                    .map_err(|_| ActionFailure::Invalid)?;
                if bytes.is_empty() || bytes.len() > 32 * 1024 {
                    return Err(ActionFailure::Invalid);
                }
                // On partial/error the caller MUST NOT repeat uncertain input.
                let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
                let mut sent = 0;
                while sent < bytes.len() {
                    if std::time::Instant::now() >= deadline {
                        return Err(ActionFailure::OutcomeUnknown);
                    }
                    match terminal.control.writer.write(&bytes[sent..]) {
                        Ok(0) => return Err(ActionFailure::OutcomeUnknown),
                        Ok(count) => sent += count,
                        Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                            std::thread::sleep(std::time::Duration::from_millis(2))
                        }
                        Err(_) => return Err(ActionFailure::OutcomeUnknown),
                    }
                }
                terminal
                    .control
                    .writer
                    .flush()
                    .map_err(|_| ActionFailure::OutcomeUnknown)?;
                Ok(json!({"accepted":bytes.len()}))
            }
            HostAction::TerminalResize { rows, cols, .. } => {
                terminal
                    .control
                    .master
                    .resize(size(*rows, *cols)?)
                    .map_err(denied)?;
                Ok(json!({"resized":true}))
            }
            HostAction::TerminalClose { .. } => {
                if terminal.control.child.try_wait().map_err(denied)?.is_none() {
                    terminal
                        .control
                        .child
                        .kill()
                        .map_err(|_| ActionFailure::OutcomeUnknown)?;
                }
                let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
                while terminal.control.child.try_wait().map_err(denied)?.is_none() {
                    if std::time::Instant::now() >= deadline {
                        return Err(ActionFailure::OutcomeUnknown);
                    }
                    std::thread::sleep(std::time::Duration::from_millis(5));
                }
                // Reap before dropping; no zombie or ambiguous PID ownership.
                // This closes only the new PTY owned by this device/Runner,
                // never an externally attached Pi process.
                entries.remove(id);
                Ok(json!({"closed":true}))
            }
            _ => Err(ActionFailure::Invalid),
        }
    }
}
