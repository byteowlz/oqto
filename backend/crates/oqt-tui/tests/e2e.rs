//! End to end: `oqt-tui` in a real PTY (a private tmux server) against a real byoagnt chat
//! host (Pi Durable 1.5.2, scripted faux model). Checks the rendered screen for a streamed
//! answer, a tool card, a part rendered natively, a part shown as its fallback and a
//! cache-first restart while the host is down, and measures start-up.
//!
//! Needs `OQT_BYOAGNT_DIR` (a byoagnt checkout with dependencies installed), `bun` and
//! `tmux`; without them the test says so and does nothing. The host secret is random per
//! run, reaches the host through its environment and the client through a 0600 file; it is
//! never in argv.

use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

type Result<T> = std::result::Result<T, String>;

struct Host(Child);

impl Drop for Host {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

struct Tmux {
    socket: String,
}

impl Tmux {
    fn run(&self, args: &[&str]) -> Result<String> {
        let out = Command::new("tmux")
            .arg("-L")
            .arg(&self.socket)
            .args(args)
            .output()
            .map_err(|e| format!("tmux: {e}"))?;
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    }

    fn screen(&self) -> Result<String> {
        self.run(&["capture-pane", "-p", "-t", "e2e"])
    }

    fn keys(&self, keys: &[&str]) -> Result<()> {
        let mut args = vec!["send-keys", "-t", "e2e"];
        args.extend_from_slice(keys);
        self.run(&args).map(|_| ())
    }

    fn wait(&self, what: &str, check: impl Fn(&str) -> bool) -> Result<String> {
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            let screen = self.screen()?;
            if check(&screen) {
                return Ok(screen);
            }
            if Instant::now() > deadline {
                return Err(format!("timed out waiting for {what}; screen:\n{screen}"));
            }
            std::thread::sleep(Duration::from_millis(40));
        }
    }

    fn start(&self, command: &str, env: &[(&str, &Path)]) -> Result<()> {
        let mut args: Vec<String> = vec![
            "-f".into(),
            "/dev/null".into(),
            "new-session".into(),
            "-d".into(),
            "-s".into(),
            "e2e".into(),
            "-x".into(),
            "110".into(),
            "-y".into(),
            "50".into(),
        ];
        for (name, value) in env {
            args.push("-e".into());
            args.push(format!("{name}={}", value.display()));
        }
        args.push(command.into());
        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
        self.run(&refs).map(|_| ())
    }
}

impl Drop for Tmux {
    fn drop(&mut self) {
        let _ = Command::new("tmux")
            .args(["-L", &self.socket, "kill-server"])
            .output();
    }
}

fn free_port() -> Result<u16> {
    let listener = TcpListener::bind("127.0.0.1:0").map_err(|e| e.to_string())?;
    listener
        .local_addr()
        .map(|a| a.port())
        .map_err(|e| e.to_string())
}

fn start_host(dir: &str, port: u16, storage: &Path, secret: &str) -> Result<Host> {
    let child = Command::new("bun")
        .arg("examples/chat/main.ts")
        .current_dir(dir)
        .env("BYOAGNT_SECRET", secret)
        .env("BYOAGNT_PORT", port.to_string())
        .env("BYOAGNT_STORAGE", storage)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("bun: {e}"))?;
    let deadline = Instant::now() + Duration::from_secs(15);
    while std::net::TcpStream::connect(("127.0.0.1", port)).is_err() {
        if Instant::now() > deadline {
            return Err("host did not start".into());
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    Ok(Host(child))
}

fn random_secret() -> Result<String> {
    let mut buf = [0u8; 32];
    let file = std::fs::File::open("/dev/urandom").map_err(|e| e.to_string())?;
    std::io::Read::read_exact(&mut &file, &mut buf).map_err(|e| e.to_string())?;
    Ok(buf.iter().map(|b| format!("{b:02x}")).collect())
}

fn write_private(path: &Path, text: &str) -> Result<()> {
    use std::os::unix::fs::OpenOptionsExt;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)
        .map_err(|e| e.to_string())?;
    std::io::Write::write_all(&mut file, text.as_bytes()).map_err(|e| e.to_string())
}

fn timing(path: &Path) -> Vec<String> {
    std::fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .map(str::to_owned)
        .collect()
}

#[test]
fn pty_chat_tools_parts_and_cache_first_restart() -> Result<()> {
    let Ok(dir) = std::env::var("OQT_BYOAGNT_DIR") else {
        println!("OQT_BYOAGNT_DIR not set: end-to-end test not run");
        return Ok(());
    };
    if Command::new("tmux").arg("-V").output().is_err() {
        println!("tmux not found: end-to-end test not run");
        return Ok(());
    }
    let temp = tempfile::tempdir().map_err(|e| e.to_string())?;
    let root: PathBuf = temp.path().to_path_buf();
    let secret = random_secret()?;
    write_private(&root.join("secret"), &secret)?;
    let port = free_port()?;
    let config = root.join("config.toml");
    std::fs::write(
        &config,
        format!(
            "host = \"e2e\"\n[[hosts]]\nname = \"e2e\"\nurl = \"ws://127.0.0.1:{port}/session\"\nsession = \"ses_chat\"\nsecret_file = \"{}\"\n[cache]\nkey = \"file\"\ndir = \"{}\"\n",
            root.join("secret").display(),
            root.join("cache").display()
        ),
    )
    .map_err(|e| e.to_string())?;
    let mut host = Some(start_host(&dir, port, &root.join("store"), &secret)?);
    let tmux = Tmux {
        socket: format!("oqt-e2e-{}", std::process::id()),
    };
    let bin = env!("CARGO_BIN_EXE_oqt-tui");
    let timing_file = root.join("timing.txt");
    let command = format!(
        "{bin} --config {} --timing-file {}",
        config.display(),
        timing_file.display()
    );
    let env = [
        ("XDG_CONFIG_HOME", root.join("xdg")),
        ("XDG_CACHE_HOME", root.join("xdg-cache")),
    ];
    let env_refs: Vec<(&str, &Path)> = env.iter().map(|(k, v)| (*k, v.as_path())).collect();
    tmux.start(&command, &env_refs)?;
    tmux.wait("empty state", |s| s.contains("No messages yet."))?;

    // A tool card with live output, then a streamed answer (the caret shows while it streams).
    tmux.keys(&["count to 3", "Enter"])?;
    tmux.wait("running tool card", |s| {
        s.contains("▸  count") && s.contains("running")
    })?;
    let streaming = tmux.wait("streamed answer", |s| {
        s.lines()
            .any(|l| l.contains("agent    Th") && l.contains('▍'))
    })?;
    let partial = streaming
        .lines()
        .find(|l| l.contains('▍'))
        .unwrap_or_default()
        .to_owned();
    println!("while streaming: {}", partial.trim());
    tmux.wait("finished answer and receipt", |s| {
        s.contains("The count tool returned: 1 2 3")
            && s.contains("done ·")
            && !s.contains('▍')
            && s.contains("you      count to 3  · answered")
    })?;

    // A part in the core set renders natively.
    tmux.keys(&["show the rollout form", "Enter"])?;
    tmux.wait("native part and attention line", |s| {
        s.contains("(•) Canary")
            && s.contains("[ Roll out ]")
            && s.contains("gamma  0.5.9   degraded")
            && s.contains("! the agent shows \"Release 0.5.12\":")
    })?;

    // A part with a chart shows its fallback and why.
    tmux.keys(&["dashboard please", "Enter"])?;
    tmux.wait("fallback part", |s| {
        s.contains("Mon 4 · Tue 9") && s.contains("shown as text: AreaChart is not in the core set")
    })?;
    let first = timing(&timing_file);
    println!("first start: {first:?}");

    // Detach (ctrl+c twice), stop the host, start again: the cache shows at once, offline.
    tmux.keys(&["C-c"])?;
    std::thread::sleep(Duration::from_millis(100));
    tmux.keys(&["C-c"])?;
    std::thread::sleep(Duration::from_millis(300));
    drop(host.take());
    let _ = std::fs::remove_file(&timing_file);
    tmux.run(&["kill-server"])?;
    let started = Instant::now();
    tmux.start(&command, &env_refs)?;
    tmux.wait("cached transcript", |s| {
        s.contains("(•) Canary") && s.contains("offline") && s.contains("cached")
    })?;
    let shown_after = started.elapsed();
    let restart = timing(&timing_file);
    println!(
        "cache-first restart: {restart:?}, screen showed it after {shown_after:?} (including tmux start)"
    );
    let first_frame: u64 = restart
        .iter()
        .find_map(|l| {
            l.strip_prefix("first_frame_ms=")
                .and_then(|r| r.split_whitespace().next())
                .and_then(|n| n.parse().ok())
        })
        .ok_or("no first frame timing")?;
    assert!(
        first_frame < 500,
        "first frame with cache took {first_frame} ms"
    );
    assert!(
        restart
            .iter()
            .any(|l| l.contains("from_cache=true") && !l.contains(" entries=0 "))
    );

    // The host comes back: the client reconnects and leaves the cached state.
    let _host = start_host(&dir, port, &root.join("store"), &secret)?;
    tmux.wait("live again", |s| {
        !s.contains("offline") && !s.contains("cached") && s.contains("(•) Canary")
    })?;
    Ok(())
}
