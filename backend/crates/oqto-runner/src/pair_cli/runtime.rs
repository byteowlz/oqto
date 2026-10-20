use super::*;
use crate::pair_control::{self, Request, Status};
use std::{
    net::{IpAddr, Ipv4Addr, UdpSocket},
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    process::Stdio,
    time::Duration,
};
use tokio::{io::AsyncReadExt, process::Command};

/// Bound both subprocess lifetime and captured output; no shell or credentials.
async fn output(program: &str, args: &[&str]) -> Result<(std::process::ExitStatus, Vec<u8>)> {
    let mut command = Command::new(program);
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    let mut child = command
        .spawn()
        .with_context(|| format!("Cannot run {program}"))?;
    tokio::time::timeout(Duration::from_secs(5), async {
        let mut bytes = Vec::new();
        child
            .stdout
            .take()
            .context("discovery pipe unavailable")?
            .take(32769)
            .read_to_end(&mut bytes)
            .await?;
        ensure!(bytes.len() <= 32768, "discovery output too large");
        let status = child.wait().await?;
        Ok((status, bytes))
    })
    .await
    .context("network/service discovery timed out")?
}
pub(super) async fn address(network: Network) -> Result<IpAddr> {
    let ip = match network {
        Network::Tailscale => {
            let cli = if Path::new("/Applications/Tailscale.app/Contents/MacOS/Tailscale").is_file()
            {
                "/Applications/Tailscale.app/Contents/MacOS/Tailscale"
            } else {
                "tailscale"
            };
            let (status, bytes) = output(cli, &["ip", "-4"]).await.context(
                "Tailscale unavailable; start/sign into Tailscale or select --network lan",
            )?;
            ensure!(status.success(), "Tailscale is not connected");
            let ip: Ipv4Addr = std::str::from_utf8(&bytes)?
                .trim()
                .parse()
                .context("Tailscale did not return one IPv4 address")?;
            ensure!(
                u32::from(ip) >> 22 == u32::from(Ipv4Addr::new(100, 64, 0, 0)) >> 22,
                "Tailscale returned a non-tailnet address"
            );
            IpAddr::V4(ip)
        }
        Network::Lan => {
            // UDP connect chooses a route; no datagram is sent.
            let socket = UdpSocket::bind("0.0.0.0:0")?;
            socket.connect("192.0.2.1:9")?;
            let ip = socket.local_addr()?.ip();
            ensure!(
                matches!(ip,IpAddr::V4(v) if v.is_private()),
                "no private LAN address detected; check your network"
            );
            ip
        }
    };
    Ok(ip)
}
async fn managed_home(home: &Path) -> Result<Option<PathBuf>> {
    let plist = home.join("Library/LaunchAgents/dev.oqto.mac-runner.plist");
    if !plist.try_exists()? {
        return Ok(None);
    }
    let meta = std::fs::symlink_metadata(&plist)?;
    ensure!(
        meta.is_file()
            && !meta.file_type().is_symlink()
            && meta.uid() == rustix::process::geteuid().as_raw(),
        "untrusted Runner service definition"
    );
    let name = plist.to_str().context("invalid service path")?;
    let (status, bytes) = output(
        "/usr/bin/plutil",
        &[
            "-extract",
            "EnvironmentVariables.HOME",
            "raw",
            "-o",
            "-",
            name,
        ],
    )
    .await?;
    ensure!(
        status.success(),
        "cannot inspect existing Runner service; refusing a duplicate"
    );
    let home = PathBuf::from(std::str::from_utf8(&bytes)?.trim_end_matches('\n'));
    ensure!(home.is_absolute(), "Runner service home must be absolute");
    Ok(Some(home.canonicalize()?))
}
// Resolve within the installed artifact; never trust ambient PATH for a daemon.
// `oqto pair` must not recursively launch the public/server executable.
fn runner_executable(current: &Path) -> Result<PathBuf> {
    let candidate = if current
        .file_name()
        .is_some_and(|name| name == "oqto-runner")
    {
        current.to_owned()
    } else {
        current
            .parent()
            .context("executable has no installation directory")?
            .join("oqto-runner")
    };
    let executable = candidate
        .canonicalize()
        .context("install the matching oqto-runner beside oqto before pairing")?;
    use std::os::unix::fs::PermissionsExt;
    let metadata = std::fs::metadata(&executable)?;
    ensure!(
        metadata.is_file() && metadata.permissions().mode() & 0o111 != 0,
        "matching Runner is not executable"
    );
    Ok(executable)
}

struct Starting(Option<tokio::process::Child>);
impl Drop for Starting {
    fn drop(&mut self) {
        if let Some(mut child) = self.0.take() {
            let _ = child.start_kill();
            tokio::spawn(async move {
                let _ = child.wait().await;
            });
        }
    }
}

pub(super) async fn status(home: &Path, selected: Option<&Path>) -> Result<Status> {
    let runner_home = selected
        .map(Path::canonicalize)
        .transpose()?
        .or(managed_home(home).await?)
        .unwrap_or_else(|| home.to_owned());
    pair_control::request(&pair_control::root(&runner_home), &Request::Status).await
}

pub(super) async fn enable(
    home: &Path,
    selected: Option<&Path>,
    plan: &ListenerPlan,
) -> Result<Status> {
    let service = managed_home(home).await?;
    let runner_home = selected
        .map(Path::canonicalize)
        .transpose()?
        .or(service.clone())
        .unwrap_or_else(|| home.to_owned());
    let control = pair_control::root(&runner_home);
    if control.join("control.sock").try_exists()? {
        let status = pair_control::request(&control, &Request::Status)
            .await
            .context("existing pairing controller unavailable; no second Runner will be started")?;
        ensure!(status.schema == 1, "unsupported pairing controller version");
    } else {
        ensure!(
            selected.is_none() && service.is_none(),
            "selected/existing service needs the pairing-control upgrade; no duplicate Runner will be started"
        );
        pair_control::private_dir(&control)?;
        let _start_lock = pair_control::lock(&control.join("start.lock"))?;
        if !control.join("control.sock").try_exists()? {
            start(home, &control).await?;
        }
    }
    let status = pair_control::request(&control, &Request::Enable { plan: plan.clone() }).await?;
    ensure!(
        status.active,
        "personal listener is not active; inspect oqto-runner pair --status and runner logs"
    );
    Ok(status)
}
async fn start(home: &Path, control: &Path) -> Result<()> {
    let executable = runner_executable(&std::env::current_exe()?)?;
    // Old/custom transports may not publish a controller. Never silently launch
    // another process over their histories. Discovery failure is not absence.
    let uid = rustix::process::geteuid().as_raw().to_string();
    let (status, bytes) = output("pgrep", &["-u", &uid, "-x", "oqto-runner"]).await?;
    ensure!(
        status.success() || status.code() == Some(1),
        "cannot determine whether a Runner already exists"
    );
    for line in std::str::from_utf8(&bytes)?.lines() {
        let pid: u32 = line.parse()?;
        ensure!(
            pid == std::process::id(),
            "an existing Runner has no discoverable controller; upgrade/select it instead of starting a second Runner"
        );
    }
    ensure!(
        !crate::runtime_guard::default_socket_path().try_exists()?,
        "a Runner socket already exists; inspect its owner before starting another process"
    );
    let existing = crate::daemon::config::RunnerUserConfig::default_config_path();
    let config = if existing.try_exists()? {
        let raw = std::fs::read_to_string(&existing)?;
        let parsed: toml::Value = toml::from_str(&raw)?;
        ensure!(
            parsed
                .get("local")
                .and_then(|v| v.get("single_user"))
                .and_then(toml::Value::as_bool)
                == Some(true),
            "existing config is not a dedicated single-user Runner; preserve it and select the correct Runner"
        );
        let typed = crate::daemon::config::RunnerUserConfig::load_from_path(existing.clone());
        ensure!(
            typed.single_user && !typed.linux_users_enabled,
            "existing config is not a dedicated personal Runner; preserved unchanged"
        );
        existing
    } else {
        let path = control.join("personal-runner.toml");
        const CONTENT: &str = "[local]\nsingle_user = true\n[local.linux_users]\nenabled = false\n";
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&path)
        {
            Ok(mut file) => {
                use std::io::Write;
                file.write_all(CONTENT.as_bytes())?;
                file.sync_all()?;
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                let m = std::fs::symlink_metadata(&path)?;
                ensure!(
                    !m.file_type().is_symlink()
                        && m.uid() == rustix::process::geteuid().as_raw()
                        && m.mode() & 0o077 == 0
                        && std::fs::read_to_string(&path)? == CONTENT,
                    "unrecognized generated Runner config; preserving it"
                );
            }
            Err(e) => return Err(e.into()),
        }
        path
    };
    let log = std::fs::OpenOptions::new()
        .append(true)
        .create(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(control.join("runner.log"))?;
    let m = log.metadata()?;
    ensure!(
        m.is_file()
            && m.uid() == rustix::process::geteuid().as_raw()
            && m.mode() & 0o077 == 0
            && m.nlink() == 1,
        "untrusted Runner log"
    );
    let mut cmd = Command::new(executable);
    cmd.arg("--socket")
        .arg(control.join("native.sock"))
        .arg("--config")
        .arg(config)
        .current_dir(home)
        .stdin(Stdio::null())
        .stdout(log.try_clone()?)
        .stderr(log);
    cmd.process_group(0);
    let mut owned = Starting(Some(cmd.spawn()?));
    for _ in 0..100 {
        if let Some(child) = owned.0.as_mut() {
            ensure!(
                child.try_wait()?.is_none(),
                "Runner startup failed; see {}/runner.log",
                control.display()
            );
        }
        if let Ok(status) = pair_control::request(control, &Request::Status).await {
            ensure!(
                owned.0.as_ref().and_then(tokio::process::Child::id) == Some(status.pid),
                "another Runner acquired control; preserving it"
            );
            let _detached = owned.0.take();
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    anyhow::bail!("Runner readiness timed out; only the newly started child is stopped")
}

#[cfg(test)]
mod executable_tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    #[test]
    fn public_entry_resolves_only_packaged_sibling() -> Result<()> {
        let home = tempfile::tempdir()?;
        let public = home.path().join("oqto");
        ensure!(runner_executable(&public).is_err());
        let runner = home.path().join("oqto-runner");
        std::fs::write(&runner, b"fixture")?;
        ensure!(runner_executable(&public).is_err());
        std::fs::set_permissions(&runner, std::fs::Permissions::from_mode(0o700))?;
        ensure!(runner_executable(&public)? == runner.canonicalize()?);
        ensure!(runner_executable(&runner)? == runner.canonicalize()?);
        Ok(())
    }
}
