//! Process-tree accounting for confinement backends without PID namespaces.
//!
//! On Linux, bwrap's PID namespace ends every descendant when its init exits,
//! so stopping the launcher stops the tree. macOS has nothing equivalent: a
//! confined child can call setsid(2), let its parent exit and be reparented to
//! launchd, leaving both the process group and the parent tree behind.
//!
//! What such a child cannot leave is its Seatbelt profile. Profiles are
//! inherited across fork and exec, and a confined process cannot apply another
//! one. Each launch therefore carries a [`Marker`]: two private files, one the
//! profile allows reading and one it denies. `sandbox_check` lets the
//! unconfined owner ask, per pid, whether that process's profile makes exactly
//! this pair of decisions. Unconfined processes allow both; other profiles do
//! not allow the private file. That identifies the members of one launch
//! independently of ancestry.
//!
//! The marker is evidence, not authority. [`reap`] confirms that a launch has
//! stopped only when a complete scan finds no live member, and it reports
//! every process it could not kill or could not inspect, so callers fail
//! closed instead of assuming the tree is gone.
//!
//! Limits: a process that a confined member asks a system service (launchd,
//! LaunchServices) to start is neither a descendant nor confined, so no
//! marker exists for it. That is a property of the Seatbelt baseline, which
//! allows Mach IPC, and is documented as a platform exception.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, ensure};

/// File the launch's profile allows reading.
pub const MARKER_ALLOW: &str = "marker-allow";
/// File the launch's profile denies.
pub const MARKER_DENY: &str = "marker-deny";

/// The real user id of this process.
#[must_use]
pub fn current_uid() -> u32 {
    // SAFETY: getuid has no preconditions and cannot fail.
    unsafe { libc::getuid() }
}

/// A launch identity: the pair of private files its Seatbelt profile decides
/// differently from every other profile on the host.
#[derive(Clone, Debug)]
pub struct Marker {
    allow: PathBuf,
    deny: PathBuf,
}

impl Marker {
    /// Open the pair in `directory`. Both must be regular files owned by this
    /// user; the trusted owner creates them before any confined launch.
    pub fn open(directory: &Path) -> Result<Self> {
        use std::os::unix::fs::MetadataExt;
        let directory = directory
            .canonicalize()
            .with_context(|| format!("resolving marker directory {}", directory.display()))?;
        let file = |name: &str| -> Result<PathBuf> {
            let path = directory.join(name);
            let metadata = std::fs::symlink_metadata(&path)
                .with_context(|| format!("reading marker {}", path.display()))?;
            ensure!(
                metadata.file_type().is_file() && metadata.uid() == current_uid(),
                "marker {} must be an owned regular file",
                path.display()
            );
            Ok(path)
        };
        Ok(Self {
            allow: file(MARKER_ALLOW)?,
            deny: file(MARKER_DENY)?,
        })
    }

    /// Seatbelt rules naming this launch. Append them after the compiled
    /// policy: SBPL applies the last matching rule, so these two decisions
    /// hold whatever the policy says about the marker directory.
    #[must_use]
    pub fn profile_rules(&self) -> String {
        let allow = crate::policy_seatbelt::escape(&self.allow.to_string_lossy());
        let deny = crate::policy_seatbelt::escape(&self.deny.to_string_lossy());
        format!(
            "(allow file-read-data (literal \"{allow}\"))\n\
             (deny file-read* file-write* (literal \"{deny}\"))\n"
        )
    }

    fn present(&self) -> bool {
        self.allow.is_file() && self.deny.is_file()
    }
}

/// Outcome of stopping one launch.
#[derive(Debug, Default)]
pub struct ReapReport {
    /// Members that received SIGKILL.
    pub killed: Vec<i32>,
    /// Members still running at the deadline.
    pub remaining: Vec<i32>,
    /// Processes whose membership could not be decided.
    pub unverifiable: Vec<i32>,
}

impl ReapReport {
    /// True only when a complete scan found no live or undecidable process.
    #[must_use]
    pub fn confirmed(&self) -> bool {
        self.remaining.is_empty() && self.unverifiable.is_empty()
    }
}

#[cfg(target_os = "macos")]
pub use darwin::{reap, scan, supervise};

/// Members of a launch found by one complete scan.
#[derive(Debug, Default)]
pub struct Scan {
    pub live: Vec<i32>,
    pub unverifiable: Vec<i32>,
}

#[cfg(target_os = "macos")]
mod darwin {
    use super::{Marker, ReapReport, Scan};
    use anyhow::{Context, Result, bail, ensure};
    use std::collections::BTreeSet;
    use std::ffi::{CString, c_char, c_int};
    use std::os::unix::process::{CommandExt, ExitStatusExt};
    use std::process::{Command, ExitStatus};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::{Duration, Instant};

    // libsystem_sandbox, re-exported by libSystem. Returns 0 when the
    // operation would be allowed for `pid`, 1 when denied, -1 on error.
    unsafe extern "C" {
        fn sandbox_check(pid: libc::pid_t, operation: *const c_char, filter: c_int, ...) -> c_int;
    }
    const SANDBOX_FILTER_PATH: c_int = 1;
    // Asking must not log a denial for the inspected process.
    const SANDBOX_CHECK_NO_REPORT: c_int = 0x4000_0000;

    fn all_pids() -> Result<Vec<libc::pid_t>> {
        for _ in 0..8 {
            // SAFETY: a null buffer only asks for the current number of pids.
            let count = unsafe { libc::proc_listallpids(std::ptr::null_mut(), 0) };
            ensure!(
                count > 0,
                "cannot count processes: {}",
                std::io::Error::last_os_error()
            );
            let capacity = usize::try_from(count)? + 256;
            let mut pids: Vec<libc::pid_t> = vec![0; capacity];
            let bytes = c_int::try_from(capacity * std::mem::size_of::<libc::pid_t>())?;
            // SAFETY: the buffer is live and `bytes` is its exact size.
            let filled = unsafe { libc::proc_listallpids(pids.as_mut_ptr().cast(), bytes) };
            ensure!(
                filled > 0,
                "cannot list processes: {}",
                std::io::Error::last_os_error()
            );
            let filled = usize::try_from(filled)?;
            // A full buffer may be truncated: the table grew; ask again.
            if filled < capacity {
                pids.truncate(filled);
                return Ok(pids);
            }
        }
        bail!("process table kept growing during enumeration")
    }

    enum State {
        Gone,
        Zombie,
        Running,
    }

    fn state(pid: libc::pid_t) -> State {
        // SAFETY: proc_bsdinfo is plain data; all-zero is a valid value.
        let mut info: libc::proc_bsdinfo = unsafe { std::mem::zeroed() };
        let size = std::mem::size_of::<libc::proc_bsdinfo>();
        let Ok(size) = c_int::try_from(size) else {
            return State::Running;
        };
        // SAFETY: `info` is a live buffer of exactly `size` bytes.
        let filled = unsafe {
            libc::proc_pidinfo(pid, libc::PROC_PIDTBSDINFO, 0, (&raw mut info).cast(), size)
        };
        if filled == size {
            return if info.pbi_status == libc::SZOMB {
                State::Zombie
            } else {
                State::Running
            };
        }
        // No information: decide by existence, never by assumption.
        if exists(pid) {
            State::Running
        } else {
            State::Gone
        }
    }

    fn exists(pid: libc::pid_t) -> bool {
        // SAFETY: signal 0 only checks existence and permission.
        let result = unsafe { libc::kill(pid, 0) };
        result == 0 || std::io::Error::last_os_error().raw_os_error() != Some(libc::ESRCH)
    }

    fn allowed(pid: libc::pid_t, path: &CString) -> Option<bool> {
        // SAFETY: both strings are NUL-terminated and outlive the call; the
        // path filter takes exactly one string argument.
        let result = unsafe {
            sandbox_check(
                pid,
                c"file-read-data".as_ptr(),
                SANDBOX_FILTER_PATH | SANDBOX_CHECK_NO_REPORT,
                path.as_ptr(),
            )
        };
        match result {
            0 => Some(true),
            1 => Some(false),
            _ => None,
        }
    }

    /// Find every live process carrying `marker`, whatever its ancestry.
    pub fn scan(marker: &Marker) -> Result<Scan> {
        // Without both files every process looks alike; that is not "none".
        ensure!(
            marker.present(),
            "launch marker is missing; membership cannot be established"
        );
        let allow = CString::new(marker.allow.as_os_str().as_encoded_bytes())
            .context("marker path contains NUL")?;
        let deny = CString::new(marker.deny.as_os_str().as_encoded_bytes())
            .context("marker path contains NUL")?;
        // SAFETY: getpid has no preconditions.
        let own = unsafe { libc::getpid() };
        let mut found = Scan::default();
        for pid in all_pids()? {
            if pid <= 0 || pid == own {
                continue;
            }
            match state(pid) {
                State::Gone | State::Zombie => continue,
                State::Running => {}
            }
            match (allowed(pid, &allow), allowed(pid, &deny)) {
                (Some(true), Some(false)) => found.live.push(pid),
                (Some(_), Some(_)) => {}
                _ if !exists(pid) => {}
                _ => found.unverifiable.push(pid),
            }
        }
        ensure!(
            marker.present(),
            "launch marker disappeared during the scan; membership cannot be established"
        );
        Ok(found)
    }

    /// SIGKILL every member until a complete scan finds none, or the deadline
    /// passes. Repeated scans also catch members forked during the loop.
    pub fn reap(marker: &Marker, deadline: Duration) -> Result<ReapReport> {
        let start = Instant::now();
        let mut killed = BTreeSet::new();
        loop {
            let found = scan(marker)?;
            if found.live.is_empty() && found.unverifiable.is_empty() {
                return Ok(ReapReport {
                    killed: killed.into_iter().collect(),
                    ..ReapReport::default()
                });
            }
            for pid in &found.live {
                // SAFETY: plain signal delivery to a pid that just matched.
                unsafe { libc::kill(*pid, libc::SIGKILL) };
                killed.insert(*pid);
            }
            if start.elapsed() >= deadline {
                return Ok(ReapReport {
                    killed: killed.into_iter().collect(),
                    remaining: found.live,
                    unverifiable: found.unverifiable,
                });
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    static STOP: AtomicBool = AtomicBool::new(false);

    extern "C" fn request_stop(_: c_int) {
        STOP.store(true, Ordering::SeqCst);
    }

    fn install_stop_handlers() -> Result<()> {
        for signal in [libc::SIGTERM, libc::SIGINT, libc::SIGHUP] {
            // SAFETY: zeroed sigaction is valid; the handler only stores an
            // atomic, which is async-signal-safe.
            let mut action: libc::sigaction = unsafe { std::mem::zeroed() };
            action.sa_sigaction = request_stop as extern "C" fn(c_int) as usize;
            // SAFETY: `action` is fully initialised and outlives the call.
            let result =
                unsafe { libc::sigaction(signal, &raw const action, std::ptr::null_mut()) };
            ensure!(
                result == 0,
                "installing stop handler: {}",
                std::io::Error::last_os_error()
            );
        }
        Ok(())
    }

    /// Exit notifications for the child and the launcher's parent, so an
    /// owner's death is seen at once rather than at the next poll.
    struct ExitWatch(c_int);

    impl ExitWatch {
        fn new(pids: [libc::pid_t; 2]) -> Result<Self> {
            // SAFETY: kqueue has no preconditions; the descriptor is owned here.
            let queue = unsafe { libc::kqueue() };
            ensure!(queue >= 0, "kqueue: {}", std::io::Error::last_os_error());
            let watch = Self(queue);
            for pid in pids {
                let change = libc::kevent {
                    ident: usize::try_from(pid)?,
                    filter: libc::EVFILT_PROC,
                    flags: libc::EV_ADD | libc::EV_ONESHOT,
                    fflags: libc::NOTE_EXIT,
                    data: 0,
                    udata: std::ptr::null_mut(),
                };
                // SAFETY: one live change record, no event buffer.
                let result = unsafe {
                    libc::kevent(
                        watch.0,
                        &raw const change,
                        1,
                        std::ptr::null_mut(),
                        0,
                        std::ptr::null(),
                    )
                };
                // ESRCH: already gone; the next poll observes that directly.
                ensure!(
                    result == 0
                        || std::io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH),
                    "watching pid {pid}: {}",
                    std::io::Error::last_os_error()
                );
            }
            Ok(watch)
        }

        /// Block until an exit is reported or `timeout` passes.
        fn wait(&self, timeout: Duration) {
            // SAFETY: kevent is plain data; all-zero is a valid value.
            let mut event: libc::kevent = unsafe { std::mem::zeroed() };
            let timeout = libc::timespec {
                tv_sec: 0,
                tv_nsec: libc::c_long::try_from(timeout.as_nanos()).unwrap_or(50_000_000),
            };
            // SAFETY: one live event slot and a live timeout.
            unsafe {
                libc::kevent(
                    self.0,
                    std::ptr::null(),
                    0,
                    &raw mut event,
                    1,
                    &raw const timeout,
                )
            };
        }
    }

    impl Drop for ExitWatch {
        fn drop(&mut self) {
            // SAFETY: closing the descriptor this value owns.
            unsafe { libc::close(self.0) };
        }
    }

    fn exit_code(status: ExitStatus) -> i32 {
        status
            .code()
            .unwrap_or_else(|| 128 + status.signal().unwrap_or(0))
    }

    /// Run `command` (a `sandbox-exec` invocation carrying `marker`'s rules)
    /// as a supervised child and stay its parent. The child gets its own
    /// process group. A stop signal stops the group (SIGTERM, then SIGKILL
    /// after `grace`); the loss of this launcher's parent kills it at once
    /// (as bwrap's --die-with-parent). In every case
    /// the launch's marked members are reaped before returning. Returns the
    /// child's exit code, or an error when the stop cannot be confirmed.
    pub fn supervise(mut command: Command, marker: &Marker, grace: Duration) -> Result<i32> {
        install_stop_handlers()?;
        // SAFETY: getppid has no preconditions.
        let parent = unsafe { libc::getppid() };
        command.process_group(0);
        let mut child = command.spawn().context("starting confined process")?;
        let group = libc::pid_t::try_from(child.id())?;
        let watch = ExitWatch::new([group, parent])?;
        let mut status = None;
        loop {
            if let Some(exited) = child.try_wait()? {
                status = Some(exited);
                break;
            }
            // The owner died: end the launch at once, as bwrap's
            // --die-with-parent does, so no orphan keeps acting for it.
            // SAFETY: getppid has no preconditions.
            if unsafe { libc::getppid() } != parent {
                break;
            }
            if STOP.load(Ordering::SeqCst) {
                // SAFETY: signal delivery to the child's own process group.
                unsafe { libc::kill(-group, libc::SIGTERM) };
                let until = Instant::now() + grace;
                while Instant::now() < until {
                    if let Some(exited) = child.try_wait()? {
                        status = Some(exited);
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(20));
                }
                break;
            }
            watch.wait(Duration::from_millis(50));
        }
        // Whatever stayed in the group goes now; descendants that left the
        // group are found by the marker below.
        // SAFETY: signal delivery to the child's own process group.
        unsafe { libc::kill(-group, libc::SIGKILL) };
        let status = match status {
            Some(status) => status,
            None => child.wait().context("collecting confined process")?,
        };
        let report = reap(marker, Duration::from_secs(3))?;
        ensure!(
            report.confirmed(),
            "confined descendants not confirmed stopped: remaining {:?}, unverifiable {:?}",
            report.remaining,
            report.unverifiable
        );
        Ok(exit_code(status))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn marker_dir() -> Result<tempfile::TempDir> {
        let dir = tempfile::tempdir()?;
        std::fs::write(dir.path().join(MARKER_ALLOW), b"")?;
        std::fs::write(dir.path().join(MARKER_DENY), b"")?;
        Ok(dir)
    }

    #[test]
    fn marker_requires_both_owned_regular_files() -> Result<()> {
        let dir = tempfile::tempdir()?;
        assert!(Marker::open(dir.path()).is_err());
        std::fs::write(dir.path().join(MARKER_ALLOW), b"")?;
        assert!(Marker::open(dir.path()).is_err());
        std::os::unix::fs::symlink(dir.path().join(MARKER_ALLOW), dir.path().join(MARKER_DENY))?;
        assert!(Marker::open(dir.path()).is_err(), "symlinks are refused");
        Ok(())
    }

    #[test]
    fn profile_rules_allow_one_file_and_deny_the_other_last() -> Result<()> {
        let dir = marker_dir()?;
        let marker = Marker::open(dir.path())?;
        let rules = marker.profile_rules();
        let root = dir.path().canonicalize()?;
        assert!(rules.contains(&format!(
            "(allow file-read-data (literal \"{}\"))",
            root.join(MARKER_ALLOW).display()
        )));
        assert!(rules.contains(&format!(
            "(deny file-read* file-write* (literal \"{}\"))",
            root.join(MARKER_DENY).display()
        )));
        Ok(())
    }

    #[cfg(target_os = "macos")]
    mod darwin {
        use super::*;
        use std::process::{Command, Stdio};
        use std::time::Duration;

        fn profile(marker: &Marker) -> String {
            let mut profile = String::from(
                "(version 1)(deny default)(allow process*)(allow sysctl*)(allow mach*)\
                 (allow signal)(allow file-read-metadata)\
                 (allow file-read* (subpath \"/usr\") (subpath \"/System\") (subpath \"/bin\") \
                 (subpath \"/Library\") (subpath \"/private/var/db\") (subpath \"/dev\") (literal \"/\"))\n",
            );
            profile.push_str(&marker.profile_rules());
            profile
        }

        /// perl: fork; the child calls setsid, forks a sleeper and exits, so
        /// the sleeper is reparented to launchd outside the launch's group.
        const ESCAPE: &str = "use POSIX; my $p = fork(); if ($p == 0) { POSIX::setsid(); \
             if (fork() == 0) { exec '/bin/sleep', '30' } exit 0 } waitpid($p, 0); \
             print \"ready\\n\"; $| = 1; sleep 30";

        fn wait_for(mut done: impl FnMut() -> Result<bool>) -> Result<()> {
            for _ in 0..200 {
                if done()? {
                    return Ok(());
                }
                std::thread::sleep(Duration::from_millis(25));
            }
            anyhow::bail!("condition not reached")
        }

        #[test]
        fn marker_finds_a_setsid_reparented_descendant_and_reap_confirms_stop() -> Result<()> {
            let dir = marker_dir()?;
            let marker = Marker::open(dir.path())?;
            let mut launch = Command::new("/usr/bin/sandbox-exec")
                .arg("-p")
                .arg(profile(&marker))
                .args(["/usr/bin/perl", "-e", ESCAPE])
                .stdout(Stdio::piped())
                .spawn()?;
            // An unconfined process is never a member.
            let mut bystander = Command::new("/bin/sleep").arg("30").spawn()?;
            let bystander_pid = i32::try_from(bystander.id())?;
            wait_for(|| Ok(scan(&marker)?.live.len() >= 2))?;
            let found = scan(&marker)?;
            assert!(found.unverifiable.is_empty(), "{found:?}");
            assert!(!found.live.contains(&bystander_pid));
            let launched = i32::try_from(launch.id())?;
            let escaped: Vec<i32> = found
                .live
                .iter()
                .copied()
                .filter(|pid| *pid != launched)
                .collect();
            assert_eq!(escaped.len(), 1, "{found:?}");
            // Stopping the launch's own process leaves the escaped sleeper.
            launch.kill()?;
            launch.wait()?;
            let survivors = scan(&marker)?;
            assert_eq!(survivors.live, escaped, "a group/tree kill misses it");
            let report = reap(&marker, Duration::from_secs(3))?;
            assert!(report.confirmed(), "{report:?}");
            assert_eq!(report.killed, escaped);
            assert!(scan(&marker)?.live.is_empty());
            // The bystander was never touched.
            assert!(bystander.try_wait()?.is_none());
            bystander.kill()?;
            bystander.wait()?;
            Ok(())
        }

        #[test]
        fn a_missing_marker_fails_closed_instead_of_reporting_none() -> Result<()> {
            let dir = marker_dir()?;
            let marker = Marker::open(dir.path())?;
            std::fs::remove_file(dir.path().join(MARKER_DENY))?;
            assert!(scan(&marker).is_err());
            assert!(reap(&marker, Duration::from_millis(50)).is_err());
            Ok(())
        }

        #[test]
        fn supervise_reaps_escaped_descendants_after_the_child_exits() -> Result<()> {
            let dir = marker_dir()?;
            let marker = Marker::open(dir.path())?;
            let mut command = Command::new("/usr/bin/sandbox-exec");
            command.arg("-p").arg(profile(&marker)).args([
                "/usr/bin/perl",
                "-e",
                "use POSIX; if (fork() == 0) { POSIX::setsid(); exec '/bin/sleep', '30' } exit 7",
            ]);
            let code = supervise(command, &marker, Duration::from_millis(200))?;
            assert_eq!(code, 7);
            assert!(scan(&marker)?.live.is_empty());
            Ok(())
        }
    }
}
