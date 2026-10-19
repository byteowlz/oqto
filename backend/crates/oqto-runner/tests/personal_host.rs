//! Disposable actual-binary/HTTPS proof. No production HOME, Pi, credentials,
//! service, phone, profile, or external network is used.
#![cfg(unix)]
use anyhow::{Context, Result, ensure};
use base64::{Engine, engine::general_purpose::STANDARD};
use ed25519_dalek::{Signer, SigningKey};
use oqto_personal_wire::{
    ENROLL_PATH, Enrollment, OQTO_PERSONAL_V1, PairingQr, SignedRequest,
    actions::{ACTION_PATH, HostAction},
};
use serde_json::Value;
use std::{
    os::unix::fs::{PermissionsExt, symlink},
    path::{Path, PathBuf},
    process::Stdio,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    net::TcpStream,
    process::{Child, Command},
};
use tokio_rustls::{TlsConnector, rustls::pki_types::ServerName};

fn now() -> Result<i64> {
    Ok(i64::try_from(
        SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs(),
    )?)
}
fn command(home: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_oqto-runner"));
    command
        .env_clear()
        .env("HOME", home)
        .env("XDG_CONFIG_HOME", home.join(".config"))
        .env("XDG_STATE_HOME", home.join(".local/state"))
        .env("TMPDIR", home.join("tmp"))
        .env(
            "USER",
            std::env::var("USER").unwrap_or_else(|_| "fixture".into()),
        )
        .env("PATH", "/usr/bin:/bin")
        .env("SHELL", "/bin/sh")
        .current_dir(home)
        .stdin(Stdio::null())
        .kill_on_drop(true);
    command
}
struct Host {
    _dir: tempfile::TempDir,
    home: PathBuf,
    work: PathBuf,
    state: PathBuf,
    child: Child,
    qr: PairingQr,
}
impl Host {
    async fn start() -> Result<Self> {
        Self::start_mode(false).await
    }
    async fn start_mode(sessions: bool) -> Result<Self> {
        let dir = tempfile::Builder::new()
            .prefix("oqpp-")
            .tempdir_in("/tmp")?;
        let root = dir.path().canonicalize()?;
        let home = root.join("home");
        let work = root.join("work");
        let state = root.join("state");
        std::fs::create_dir_all(home.join("tmp"))?;
        std::fs::create_dir_all(&work)?;
        let config = root.join("config.toml");
        let sentinel = home.join("pi-sentinel");
        std::fs::write(
            &sentinel,
            format!(
                "#!/bin/sh\nprintf invoked >> '{}'\nexit 97\n",
                home.join("pi-invocations").display()
            ),
        )?;
        std::fs::set_permissions(&sentinel, std::fs::Permissions::from_mode(0o700))?;
        std::fs::write(
            &config,
            format!(
                "[local]\nsingle_user=true\nterminal_enabled=true\nworkspace_dir={}\n[local.linux_users]\nenabled=false\n[runner]\nrunner_id=\"personal-disposable-fixture\"\n[pi]\nexecutable={}\n",
                serde_json::to_string(&work)?,
                serde_json::to_string(&sentinel)?
            ),
        )?;
        std::fs::set_permissions(&config, std::fs::Permissions::from_mode(0o600))?;
        let reserved = std::net::TcpListener::bind("127.0.0.1:0")?;
        let address = reserved.local_addr()?;
        drop(reserved);
        let log = std::fs::File::create(root.join("host.stderr"))?;
        let mut process = command(&home);
        if sessions {
            process
                .args(["--sessions", "control", "--personal-workspace-root"])
                .arg(&work)
                .arg("--socket")
                .arg(home.join("primary.sock"));
        }
        let mut child = process
            .args(["--config"])
            .arg(&config)
            .args([
                "--no-sandbox",
                "--personal-listen",
                &address.to_string(),
                "--personal-origin",
                &format!("https://{address}"),
                "--personal-state",
            ])
            .arg(&state)
            .args(["--pair-output", "json", "--write-root"])
            .arg(&work)
            .arg("--full-user-terminal")
            .stdout(Stdio::piped())
            .stderr(log)
            .spawn()?;
        let stdout = child.stdout.take().context("fixture stdout missing")?;
        let mut lines = BufReader::new(stdout).lines();
        let line = tokio::time::timeout(Duration::from_secs(20), lines.next_line())
            .await??
            .context("host did not emit pairing QR")?;
        let qr = PairingQr::parse(line.as_bytes(), now()?)?;
        Ok(Self {
            _dir: dir,
            home,
            work,
            state,
            child,
            qr,
        })
    }
    async fn owner(&self, args: &[&str]) -> Result<Value> {
        let output = command(&self.home)
            .args(["personal"])
            .args(args)
            .arg("--state")
            .arg(&self.state)
            .output()
            .await?;
        ensure!(
            output.status.success(),
            "owner CLI failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        Ok(serde_json::from_slice(&output.stdout)?)
    }
    async fn post(&self, path: &str, bytes: &[u8]) -> Result<(u16, Value)> {
        let config = oqto_personal_wire::tls::pinned_client_config(self.qr.certificate_sha256())?;
        let origin = self
            .qr
            .origin()
            .strip_prefix("https://")
            .context("HTTPS origin required")?;
        let socket = TcpStream::connect(origin.trim_end_matches('/')).await?;
        let mut tls = TlsConnector::from(std::sync::Arc::new(config))
            .connect(ServerName::try_from("localhost")?.to_owned(), socket)
            .await?;
        let header = format!(
            "POST {path} HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            bytes.len()
        );
        tls.write_all(header.as_bytes()).await?;
        tls.write_all(bytes).await?;
        let mut response = Vec::new();
        tokio::time::timeout(Duration::from_secs(10), tls.read_to_end(&mut response)).await??;
        let response = String::from_utf8(response)?;
        let (headers, body) = response
            .split_once("\r\n\r\n")
            .context("missing response headers")?;
        let status = headers
            .split_whitespace()
            .nth(1)
            .context("missing status")?
            .parse()?;
        Ok((status, serde_json::from_str(body)?))
    }
    async fn enroll(&self, qr: &PairingQr, seed: u8) -> Result<String> {
        let key = SigningKey::from_bytes(&[seed; 32]);
        let mut proof = Enrollment {
            token: qr.enrollment_token().into(),
            name: "Disposable phone".into(),
            public_key: STANDARD.encode(key.verifying_key().to_bytes()),
            signature: String::new(),
        };
        proof.signature = STANDARD.encode(
            key.sign(&proof.signing_bytes_for(OQTO_PERSONAL_V1))
                .to_bytes(),
        );
        let (status, reply) = self.post(ENROLL_PATH, &serde_json::to_vec(&proof)?).await?;
        assert_eq!(status, 200);
        assert_eq!(reply["host_id"], qr.host_id());
        Ok(reply["device_id"]
            .as_str()
            .context("device ID missing")?
            .into())
    }
    fn proof(id: &str, seed: u8, action: &HostAction) -> Result<SignedRequest> {
        let key = SigningKey::from_bytes(&[seed; 32]);
        let mut proof = SignedRequest {
            device_id: id.into(),
            timestamp: now()?,
            nonce: uuid::Uuid::new_v4().to_string(),
            payload: serde_json::to_string(action)?,
            signature: String::new(),
        };
        proof.signature = STANDARD.encode(
            key.sign(&proof.signing_bytes_for(OQTO_PERSONAL_V1))
                .to_bytes(),
        );
        Ok(proof)
    }
    async fn action(&self, id: &str, seed: u8, action: HostAction) -> Result<(u16, Value)> {
        self.post(
            ACTION_PATH,
            &serde_json::to_vec(&Self::proof(id, seed, &action)?)?,
        )
        .await
    }
    async fn stop(mut self) -> Result<()> {
        self.child.start_kill()?;
        self.child.wait().await?;
        assert!(
            !self.home.join("pi-invocations").exists(),
            "Startup/owner/native actions must execute zero Pi candidates"
        );
        Ok(())
    }
}

#[path = "personal_host/sessions.rs"]
mod sessions;
#[path = "personal_host/split_source.rs"]
mod split_source;

#[tokio::test]
async fn actual_cli_https_files_pty_independent_grants_and_revoke() -> Result<()> {
    let host = Host::start().await?;
    let device = host.enroll(&host.qr, 1).await?;
    let (status, caps) = host.action(&device, 1, HostAction::Capabilities).await?;
    assert_eq!(status, 200);
    assert_eq!(caps["host_id"], host.qr.host_id());
    assert_eq!(caps["full_user_terminal_granted"], true);
    let root = caps["roots"][0]["id"]
        .as_str()
        .context("missing root")?
        .to_owned();
    let write = HostAction::FileWrite {
        root_id: root.clone(),
        path: "note.txt".into(),
        data_base64: STANDARD.encode(b"first"),
        expected_sha256: None,
    };
    let proof = serde_json::to_vec(&Host::proof(&device, 1, &write)?)?;
    let first_write = host.post(ACTION_PATH, &proof).await?;
    assert_eq!(first_write.0, 200, "write response: {}", first_write.1);
    assert_eq!(host.post(ACTION_PATH, &proof).await?.0, 401);
    assert_eq!(std::fs::read(host.work.join("note.txt"))?, b"first");
    let (_, read) = host
        .action(
            &device,
            1,
            HostAction::FileRead {
                root_id: root.clone(),
                path: "note.txt".into(),
                offset: 0,
                limit: 128,
            },
        )
        .await?;
    assert_eq!(
        STANDARD.decode(read["data_base64"].as_str().context("missing data")?)?,
        b"first"
    );
    let etag = read["sha256"]
        .as_str()
        .context("missing baseline")?
        .to_owned();
    std::fs::write(host.work.join("note.txt"), b"native edit")?;
    assert_eq!(
        host.action(
            &device,
            1,
            HostAction::FileWrite {
                root_id: root.clone(),
                path: "note.txt".into(),
                data_base64: STANDARD.encode(b"must not overwrite"),
                expected_sha256: Some(etag)
            }
        )
        .await?
        .0,
        409
    );
    assert_eq!(std::fs::read(host.work.join("note.txt"))?, b"native edit");
    let outside = host.home.join("private.txt");
    std::fs::write(&outside, b"outside fixture")?;
    symlink(&outside, host.work.join("escape"))?;
    for path in ["escape", "../home/private.txt", "/etc/passwd"] {
        assert_ne!(
            host.action(
                &device,
                1,
                HostAction::FileRead {
                    root_id: root.clone(),
                    path: path.into(),
                    offset: 0,
                    limit: 128
                }
            )
            .await?
            .0,
            200
        );
    }
    // A second real owner-issued enrollment is read-only, not a clone of peer grants.
    let qr = host
        .owner(&[
            "pair",
            "--output",
            "json",
            "--read-root",
            host.work.to_str().context("UTF8 fixture path")?,
        ])
        .await?;
    let qr = PairingQr::parse(&serde_json::to_vec(&qr)?, now()?)?;
    let reader = host.enroll(&qr, 2).await?;
    let (_, reader_caps) = host.action(&reader, 2, HostAction::Capabilities).await?;
    let reader_root = reader_caps["roots"][0]["id"]
        .as_str()
        .context("reader root")?
        .to_owned();
    assert_eq!(
        host.action(
            &reader,
            2,
            HostAction::FileRead {
                root_id: reader_root.clone(),
                path: "note.txt".into(),
                offset: 0,
                limit: 128
            }
        )
        .await?
        .0,
        200
    );
    assert_eq!(
        host.action(
            &reader,
            2,
            HostAction::FileWrite {
                root_id: reader_root,
                path: "other.txt".into(),
                data_base64: STANDARD.encode(b"denied"),
                expected_sha256: None
            }
        )
        .await?
        .0,
        403
    );
    assert_eq!(
        host.action(
            &reader,
            2,
            HostAction::TerminalOpen {
                cwd: host.work.to_string_lossy().into(),
                rows: 24,
                cols: 80
            }
        )
        .await?
        .0,
        403
    );
    assert_eq!(
        host.action(&reader, 2, HostAction::SessionsList).await?.0,
        403
    );
    // Actual OS PTY: resize, output, reconnect-by-ID, no retransmission of input.
    let (status, terminal) = host
        .action(
            &device,
            1,
            HostAction::TerminalOpen {
                cwd: host.work.to_string_lossy().into(),
                rows: 24,
                cols: 80,
            },
        )
        .await?;
    assert_eq!(status, 200);
    let terminal = terminal["terminal_id"]
        .as_str()
        .context("terminal id")?
        .to_owned();
    assert_eq!(
        host.action(
            &device,
            1,
            HostAction::TerminalResize {
                terminal_id: terminal.clone(),
                rows: 31,
                cols: 91
            }
        )
        .await?
        .0,
        200
    );
    assert_eq!(
        host.action(
            &reader,
            2,
            HostAction::TerminalRead {
                terminal_id: terminal.clone(),
                after: 0
            }
        )
        .await?
        .0,
        403
    );
    // Publish the fixture-owned PID atomically. Existence of a redirection
    // target alone is not readiness: it can still be empty before printf runs.
    // Delaying the temporary write exercises polling before publication.
    let input = "stty size; printf 'oqto-pty-fixture\\n'; : > shell.pid.tmp; sleep 0.05; printf '%s' \"$$\" > shell.pid.tmp; mv shell.pid.tmp shell.pid\n";
    assert_eq!(
        host.action(
            &device,
            1,
            HostAction::TerminalWrite {
                terminal_id: terminal.clone(),
                data_base64: STANDARD.encode(input)
            }
        )
        .await?
        .0,
        200
    );
    let mut offset = 0;
    let mut output = Vec::new();
    for _ in 0..100 {
        let (status, page) = host
            .action(
                &device,
                1,
                HostAction::TerminalRead {
                    terminal_id: terminal.clone(),
                    after: offset,
                },
            )
            .await?;
        assert_eq!(status, 200);
        output.extend(STANDARD.decode(page["data_base64"].as_str().context("terminal bytes")?)?);
        offset = page["next"].as_u64().context("terminal cursor")?;
        if String::from_utf8_lossy(&output).contains("31 91")
            && host.work.join("shell.pid").exists()
        {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(String::from_utf8_lossy(&output).contains("31 91"));
    let pid: i32 = std::fs::read_to_string(host.work.join("shell.pid"))?
        .parse()
        .context("atomically published fixture shell PID")?;
    let pid = rustix::process::Pid::from_raw(pid).context("owned shell PID")?;
    assert_eq!(host.owner(&["revoke", &device]).await?["revoked"], true);
    assert_eq!(
        host.action(
            &device,
            1,
            HostAction::TerminalRead {
                terminal_id: terminal,
                after: offset
            }
        )
        .await?
        .0,
        401
    );
    // Revocation removes access, not the shell. The PID came only from our own fixture.
    rustix::process::test_kill_process(pid)?;
    assert_eq!(
        host.action(
            &reader,
            2,
            HostAction::FileRead {
                root_id: reader_caps["roots"][0]["id"]
                    .as_str()
                    .context("root")?
                    .into(),
                path: "note.txt".into(),
                offset: 0,
                limit: 128
            }
        )
        .await?
        .0,
        200
    );
    host.stop().await
}
