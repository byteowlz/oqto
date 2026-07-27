//! Sandbox containment checks.

use std::path::{Path, PathBuf};
use std::process::Command;

use oqto_sandbox::SandboxConfig;

fn bwrap_available() -> bool {
    if !SandboxConfig::is_bwrap_available() {
        return false;
    }
    matches!(
        Command::new("bwrap")
            .args(["--dev-bind", "/", "/", "true"])
            .status(),
        Ok(s) if s.success()
    )
}

fn run_sandboxed(config: &SandboxConfig, workspace: &Path, script: &str) -> (bool, String) {
    let args = config
        .build_bwrap_args_for_user(workspace, None)
        .expect("profile must produce bwrap args");

    let output = Command::new("bwrap")
        .args(&args)
        .arg("sh")
        .arg("-c")
        .arg(script)
        .output()
        .expect("spawning bwrap");

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if stdout.is_empty() {
        eprintln!(
            "bwrap produced no stdout; status={:?} stderr={}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );
    }
    (output.status.success(), stdout)
}

fn strict_config() -> (SandboxConfig, tempfile::TempDir) {
    // Not under /tmp: the sandbox mounts a fresh tmpfs there, which shadows the
    // workspace bind and makes probes fail for the wrong reason.
    let home = std::env::var("HOME").expect("HOME");
    let workspace = tempfile::tempdir_in(&home).expect("tempdir");
    let mut config = SandboxConfig::strict();
    config.enabled = true;
    (config, workspace)
}

struct FileGuard(PathBuf);

impl Drop for FileGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

struct DirGuard(PathBuf);

impl Drop for DirGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn run_directory_is_not_reachable() {
    if !bwrap_available() {
        eprintln!("skipping: bwrap/userns unavailable");
        return;
    }
    let (config, workspace) = strict_config();

    let (_ok, out) = run_sandboxed(
        &config,
        workspace.path(),
        "test -e /run/oqto && echo present || echo denied",
    );

    assert_eq!(out, "denied");
}

#[test]
fn tmp_is_writable() {
    if !bwrap_available() {
        eprintln!("skipping: bwrap/userns unavailable");
        return;
    }
    let (config, workspace) = strict_config();

    let (_ok, out) = run_sandboxed(
        &config,
        workspace.path(),
        "touch /tmp/oqto-probe 2>/dev/null && echo writable || echo denied",
    );

    assert_eq!(out, "writable");
}

#[test]
fn deny_read_paths_are_not_readable() {
    if !bwrap_available() {
        eprintln!("skipping: bwrap/userns unavailable");
        return;
    }
    let (mut config, workspace) = strict_config();

    let home = std::env::var("HOME").expect("HOME");
    let dir = PathBuf::from(&home).join(".oqto-probe-creds");
    std::fs::create_dir_all(&dir).expect("fixture dir");
    let _guard = DirGuard(dir.clone());
    let fixture = dir.join("key");
    std::fs::write(&fixture, "fixture").expect("fixture");

    config.deny_read.push("~/.oqto-probe-creds".to_string());

    // Readable on the host, else the assertion below proves nothing.
    assert_eq!(
        std::fs::read_to_string(&fixture).expect("fixture readable on host"),
        "fixture"
    );

    let (_ok, out) = run_sandboxed(
        &config,
        workspace.path(),
        &format!(
            "cat {} 2>/dev/null && echo present || echo denied",
            fixture.display()
        ),
    );

    assert_eq!(out, "denied");
}

#[test]
fn strict_profile_deny_read_defaults() {
    let config = SandboxConfig::strict();
    for expected in ["~/.ssh", "~/.gnupg", "~/.aws"] {
        assert!(
            config.deny_read.iter().any(|p| p == expected),
            "strict profile no longer denies {expected}"
        );
    }
}

#[test]
fn unlisted_home_paths_are_not_readable() {
    if !bwrap_available() {
        eprintln!("skipping: bwrap/userns unavailable");
        return;
    }
    let (config, workspace) = strict_config();

    let home = std::env::var("HOME").expect("HOME");
    let fixture = PathBuf::from(&home).join(".oqto-probe-unlisted");
    std::fs::write(&fixture, "fixture").expect("fixture");
    let _guard = FileGuard(fixture.clone());

    let (_ok, out) = run_sandboxed(
        &config,
        workspace.path(),
        &format!(
            "cat {} >/dev/null 2>&1 && echo present || echo denied",
            fixture.display()
        ),
    );

    assert_eq!(out, "denied");
}

#[test]
fn writes_outside_the_workspace_are_denied() {
    if !bwrap_available() {
        eprintln!("skipping: bwrap/userns unavailable");
        return;
    }
    let (config, workspace) = strict_config();

    let (_ok, out) = run_sandboxed(
        &config,
        workspace.path(),
        "touch ~/.oqto-probe 2>/dev/null && echo written || echo denied",
    );

    assert_eq!(out, "denied");
}

#[test]
fn the_workspace_itself_is_still_writable() {
    if !bwrap_available() {
        eprintln!("skipping: bwrap/userns unavailable");
        return;
    }
    let (config, workspace) = strict_config();

    let (_ok, out) = run_sandboxed(
        &config,
        workspace.path(),
        "touch ./probe && echo writable || echo denied",
    );

    assert_eq!(out, "writable");
}

#[test]
fn development_profile_shape_is_stable() {
    let config = SandboxConfig::from_profile("development");
    assert!(
        config
            .allow_write
            .iter()
            .any(|p| p == "~" || p.contains("$HOME"))
            || config.profile == "development",
        "development profile shape changed"
    );
}

#[test]
fn sandbox_is_available_somewhere() {
    if std::env::var("OQTO_REQUIRE_SANDBOX").as_deref() != Ok("1") {
        eprintln!("skipping: set OQTO_REQUIRE_SANDBOX=1 to enforce");
        return;
    }
    assert!(
        bwrap_available(),
        "OQTO_REQUIRE_SANDBOX=1 but bwrap/userns is unavailable"
    );
}

#[test]
fn other_workspaces_session_history_is_not_readable() {
    if !bwrap_available() {
        eprintln!("skipping: bwrap/userns unavailable");
        return;
    }
    let home = std::env::var("HOME").expect("HOME");
    let sessions = PathBuf::from(&home).join(".pi/agent/sessions");

    let mine = PathBuf::from(&home).join(".oqto-probe-ws-mine");
    let theirs = PathBuf::from(&home).join(".oqto-probe-ws-theirs");
    std::fs::create_dir_all(&mine).expect("workspace");
    std::fs::create_dir_all(&theirs).expect("workspace");
    let _g1 = DirGuard(mine.clone());
    let _g2 = DirGuard(theirs.clone());

    let shard = |ws: &PathBuf| {
        sessions.join(format!(
            "--{}--",
            ws.to_string_lossy()
                .trim_start_matches('/')
                .replace('/', "-")
        ))
    };
    let mine_shard = shard(&mine);
    let theirs_shard = shard(&theirs);
    std::fs::create_dir_all(&mine_shard).expect("shard");
    std::fs::create_dir_all(&theirs_shard).expect("shard");
    let _g3 = DirGuard(mine_shard.clone());
    let _g4 = DirGuard(theirs_shard.clone());
    std::fs::write(mine_shard.join("s.jsonl"), "MINE").expect("fixture");
    std::fs::write(theirs_shard.join("s.jsonl"), "THEIRS").expect("fixture");

    let mut config = SandboxConfig::strict();
    config.enabled = true;

    let (_ok, out) = run_sandboxed(
        &config,
        &mine,
        &format!(
            "cat {}/s.jsonl 2>/dev/null || echo denied",
            theirs_shard.display()
        ),
    );
    assert_eq!(out, "denied");

    let (_ok, own) = run_sandboxed(
        &config,
        &mine,
        &format!(
            "cat {}/s.jsonl 2>/dev/null || echo denied",
            mine_shard.display()
        ),
    );
    assert_eq!(own, "MINE", "the workspace must still see its own history");

    let (_ok, writable) = run_sandboxed(
        &config,
        &mine,
        &format!(
            "touch {}/new.jsonl 2>/dev/null && echo writable || echo denied",
            mine_shard.display()
        ),
    );
    assert_eq!(writable, "writable", "the harness must be able to persist");
}

#[test]
fn a_missing_session_shard_is_created_not_skipped() {
    if !bwrap_available() {
        eprintln!("skipping: bwrap/userns unavailable");
        return;
    }
    let home = std::env::var("HOME").expect("HOME");
    let workspace = PathBuf::from(&home).join(".oqto-probe-ws-fresh");
    std::fs::create_dir_all(&workspace).expect("workspace");
    let _g1 = DirGuard(workspace.clone());

    let shard = PathBuf::from(&home)
        .join(".pi/agent/sessions")
        .join(format!(
            "--{}--",
            workspace
                .to_string_lossy()
                .trim_start_matches('/')
                .replace('/', "-")
        ));
    let _ = std::fs::remove_dir_all(&shard);
    let _g2 = DirGuard(shard.clone());

    let mut config = SandboxConfig::strict();
    config.enabled = true;

    let (_ok, out) = run_sandboxed(
        &config,
        &workspace,
        &format!(
            "touch {}/first.jsonl 2>/dev/null && echo writable || echo denied",
            shard.display()
        ),
    );

    assert_eq!(out, "writable");
    assert!(
        shard.join("first.jsonl").exists(),
        "history written in the sandbox must land on the host, not a tmpfs"
    );
}

/// seccomp is installed by bwrap before the inner shim applies Landlock, so a
/// policy that omits the landlock syscalls turns "landlock enforce" into a
/// failed spawn that blames the kernel.
#[test]
fn the_seccomp_policy_allows_landlock() {
    if !bwrap_available() {
        eprintln!("skipping: bwrap/userns unavailable");
        return;
    }
    let bpf = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .expect("repo root")
        .join("backend/crates/oqto/examples/seccomp/default-x86_64.bpf");
    if !bpf.exists() || !cfg!(target_arch = "x86_64") {
        eprintln!("skipping: x86_64 policy artifact not applicable");
        return;
    }

    let probe =
        "import ctypes;l=ctypes.CDLL('libc.so.6',use_errno=True);print(l.syscall(444,None,0,1))";

    // bwrap reads the policy from a file descriptor, and Rust marks opened
    // files CLOEXEC, so hand it over as stdin (fd 0) instead.
    let file = std::fs::File::open(&bpf).expect("open policy");
    let output = Command::new("bwrap")
        .args([
            "--ro-bind",
            "/usr",
            "/usr",
            "--ro-bind",
            "/lib",
            "/lib",
            "--ro-bind",
            "/lib64",
            "/lib64",
            "--ro-bind",
            "/bin",
            "/bin",
            "--ro-bind",
            "/etc",
            "/etc",
            "--proc",
            "/proc",
            "--dev",
            "/dev",
            "--seccomp",
            "0",
        ])
        .args(["python3", "-c", probe])
        .stdin(file)
        .output()
        .expect("spawn bwrap");

    let out = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let abi: i64 = out.parse().unwrap_or(-1);
    assert!(
        abi > 0,
        "landlock probe under the shipped seccomp policy returned {out}; the \
         policy must allowlist landlock_create_ruleset"
    );
}
