//! Live macOS proof through the exact command builder used by runner and CLI.
//! No real keys, user config, or services are modified. A failed positive probe
//! is a test failure, never evidence of containment.
#![cfg(all(target_os = "macos", feature = "macos-seatbelt"))]

use oqto_sandbox::{
    NetworkConfig, NetworkMode, SandboxConfig, build_sandbox_command, configure_bwrap_pre_exec,
};
use std::os::unix::{
    fs::symlink,
    net::{UnixListener, UnixStream},
};
use std::path::{Path, PathBuf};
use std::process::Output;

fn policy() -> SandboxConfig {
    let mut config = SandboxConfig::from_profile("minimal");
    config.no_new_privs = false;
    config.read_policy = serde_json::from_str("\"allowlist\"").unwrap();
    // Tests grant only their own workspace. In particular do not inherit /tmp.
    config.allow_write.clear();
    config
}

fn run(config: &SandboxConfig, workspace: &Path, script: &str, args: &[&Path]) -> Output {
    let mut argv = vec!["-c".into(), script.into(), "probe".into()];
    argv.extend(args.iter().map(|p| p.to_string_lossy().into_owned()));
    let mut command = build_sandbox_command(
        config,
        workspace,
        Path::new("/bin/sh"),
        &argv,
        oqto_sandbox::SandboxStdin::Redirected,
    )
    .unwrap();
    command.env("TMPDIR", workspace);
    configure_bwrap_pre_exec(&mut command, config, workspace, None).unwrap();
    command.output().unwrap()
}

fn success(output: Output, expected: &str) {
    assert!(
        output.status.success(),
        "status={:?} stderr={}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&output.stdout), expected);
}

#[test]
fn cwd_file_grants_and_negative_controls_are_enforced() {
    let root = tempfile::tempdir().unwrap();
    let workspace = root.path().join("work");
    std::fs::create_dir(&workspace).unwrap();
    let readable = root.path().join("readable");
    let denied = workspace.join("denied");
    let outside = root.path().join("outside");
    std::fs::write(&readable, "readable\n").unwrap();
    std::fs::write(&denied, "private\n").unwrap();
    std::fs::write(&outside, "outside\n").unwrap();
    let mut config = policy();
    config
        .extra_ro_bind
        .push(readable.to_string_lossy().into_owned());
    config.deny_read.push(denied.to_string_lossy().into_owned());
    success(
        run(&config, &workspace, "pwd -P", &[]),
        &format!("{}\n", workspace.canonicalize().unwrap().display()),
    );
    success(
        run(
            &config,
            &workspace,
            "cat \"$1\"; printf written > local; cat local",
            &[&readable],
        ),
        "readable\nwritten",
    );
    success(
        run(
            &config,
            &workspace,
            "printf READY; if cat \"$1\"; then exit 9; fi; printf DENIED",
            &[&denied],
        ),
        "READYDENIED",
    );
    success(
        run(
            &config,
            &workspace,
            "printf READY; if cat \"$1\"; then exit 9; fi; printf DENIED",
            &[&outside],
        ),
        "READYDENIED",
    );
    success(
        run(
            &config,
            &workspace,
            "printf READY; if printf bad > \"$1\"; then exit 9; fi; printf DENIED",
            &[&readable],
        ),
        "READYDENIED",
    );
    success(
        run(
            &config,
            &workspace,
            "printf READY; if printf bad > \"$1\"; then exit 9; fi; printf DENIED",
            &[&outside],
        ),
        "READYDENIED",
    );
    assert_eq!(std::fs::read_to_string(readable).unwrap(), "readable\n");
    assert_eq!(std::fs::read_to_string(outside).unwrap(), "outside\n");
}

#[test]
fn symlink_and_quoted_workdir_do_not_bypass_policy() {
    let root = tempfile::tempdir().unwrap();
    let workspace = root.path().join("work\"quoted");
    std::fs::create_dir(&workspace).unwrap();
    let alias = root.path().join("alias");
    symlink(&workspace, &alias).unwrap();
    let denied = workspace.join("denied");
    std::fs::write(&denied, "private").unwrap();
    let mut config = policy();
    config
        .deny_read
        .push(alias.join("denied").to_string_lossy().into_owned());
    success(
        run(&config, &alias, "printf good > local; cat local", &[]),
        "good",
    );
    success(
        run(
            &config,
            &alias,
            "printf READY; if cat \"$1\"; then exit 9; fi; printf DENIED",
            &[&denied],
        ),
        "READYDENIED",
    );
}

// Child test mode gives the sandbox a native Unix-socket client without relying
// on Python, Homebrew, shell flags, or a user's installed agent.
#[test]
#[ignore = "helper executed explicitly by socket_boundaries_with_open_and_isolated_network"]
fn unix_socket_probe() {
    let path = std::env::var_os("OQTO_PROBE_SOCKET").expect("probe socket");
    let expected = std::env::var("OQTO_PROBE_EXPECT").expect("expected result") == "allow";
    let connected = UnixStream::connect(PathBuf::from(path)).is_ok();
    assert_eq!(connected, expected);
}

fn socket_probe(config: &SandboxConfig, workspace: &Path, socket: &Path, allowed: bool) {
    let executable = std::env::current_exe().unwrap();
    let mut config = config.clone();
    config
        .extra_ro_bind
        .push(executable.to_string_lossy().into_owned());
    let mut command = build_sandbox_command(
        &config,
        workspace,
        &executable,
        &[
            "--ignored".into(),
            "--exact".into(),
            "unix_socket_probe".into(),
        ],
        oqto_sandbox::SandboxStdin::Redirected,
    )
    .unwrap();
    command
        .env("OQTO_PROBE_SOCKET", socket)
        .env("OQTO_PROBE_EXPECT", if allowed { "allow" } else { "deny" });
    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "{} {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("1 passed"));
}

#[test]
fn shipped_profile_grants_only_system_dns_and_respects_network_isolation() {
    let file: oqto_sandbox::SandboxConfigFile = toml::from_str(include_str!(
        "../../oqto/examples/sandbox.template.macos-host.toml"
    ))
    .unwrap();
    let mut config: SandboxConfig = file.into();
    let workspace = tempfile::Builder::new()
        .prefix("oq-dns-work-")
        .tempdir_in("/tmp")
        .unwrap();
    let outside = tempfile::Builder::new()
        .prefix("oq-dns-other-")
        .tempdir_in("/tmp")
        .unwrap();
    let control = outside.path().join("control.sock");
    let _listener = UnixListener::bind(&control).unwrap();
    let resolver = Path::new("/private/var/run/mDNSResponder");
    assert!(
        resolver.exists(),
        "macOS resolver service is required for this proof"
    );
    socket_probe(&config, workspace.path(), resolver, true);
    socket_probe(&config, workspace.path(), &control, false);
    config
        .deny_read
        .push(resolver.to_string_lossy().into_owned());
    socket_probe(&config, workspace.path(), resolver, false);
    config.deny_read.clear();
    config.network = Some(NetworkConfig {
        mode: NetworkMode::Isolated,
        ..Default::default()
    });
    socket_probe(&config, workspace.path(), resolver, false);
}

#[test]
fn socket_boundaries_with_open_and_isolated_network() {
    // Short /tmp paths avoid Darwin's 104-byte sockaddr_un limit.
    let root = tempfile::Builder::new()
        .prefix("oq-sock-")
        .tempdir_in("/tmp")
        .unwrap();
    let allowed = root.path().join("filtered.sock");
    let denied = root.path().join("upstream.sock");
    let _allowed_listener = UnixListener::bind(&allowed).unwrap();
    let _denied_listener = UnixListener::bind(&denied).unwrap();
    let mut config = policy();
    config.deny_read.push(denied.to_string_lossy().into_owned());
    socket_probe(&config, root.path(), &allowed, true);
    socket_probe(&config, root.path(), &denied, false);
    let sibling = tempfile::Builder::new()
        .prefix("oq-other-")
        .tempdir_in("/tmp")
        .unwrap();
    let sibling_socket = sibling.path().join("control.sock");
    let _sibling_listener = UnixListener::bind(&sibling_socket).unwrap();
    // An allowlist also denies sockets without an explicit deny_read entry.
    socket_probe(&config, root.path(), &sibling_socket, false);
    config.network = Some(NetworkConfig {
        mode: NetworkMode::Isolated,
        ..Default::default()
    });
    socket_probe(&config, root.path(), &allowed, false);
}

#[test]
#[ignore = "helper executed explicitly by redirected_input_never_borrows_the_runner_terminal"]
fn redirected_profile_probe() {
    use std::io::IsTerminal;
    assert!(std::io::stdin().is_terminal(), "positive PTY control");
    let root = tempfile::tempdir().unwrap();
    let command = build_sandbox_command(
        &policy(),
        root.path(),
        Path::new("/bin/true"),
        &[],
        oqto_sandbox::SandboxStdin::Redirected,
    )
    .unwrap();
    assert!(
        !command
            .get_args()
            .any(|arg| arg.to_string_lossy().contains("file-ioctl"))
    );
}

#[test]
fn redirected_input_never_borrows_the_runner_terminal() {
    use std::os::fd::FromRawFd;
    let (mut master, mut slave) = (-1, -1);
    // SAFETY: both output pointers are valid; null options request defaults.
    let status = unsafe {
        libc::openpty(
            &mut master,
            &mut slave,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };
    assert_eq!(status, 0, "openpty: {}", std::io::Error::last_os_error());
    // SAFETY: openpty succeeded and returned two newly owned descriptors.
    let (_master, slave) = unsafe {
        (
            std::fs::File::from_raw_fd(master),
            std::fs::File::from_raw_fd(slave),
        )
    };
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--ignored", "--exact", "redirected_profile_probe"])
        .stdin(slave)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{} {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("1 passed"));
}

#[test]
fn unsupported_policy_errors_before_any_payload_runs() {
    let root = tempfile::tempdir().unwrap();
    let config = SandboxConfig::from_profile("strict");
    let result = build_sandbox_command(
        &config,
        root.path(),
        Path::new("/usr/bin/touch"),
        &[root
            .path()
            .join("should-not-exist")
            .to_string_lossy()
            .into_owned()],
        oqto_sandbox::SandboxStdin::Redirected,
    );
    assert!(result.is_err());
    assert!(!root.path().join("should-not-exist").exists());
}
