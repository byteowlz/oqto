//! Bounded Runner-owned execution after exact destination consent. No shell
//! fallback, ambient environment or authority socket is inherited.
use anyhow::{Context, Result, ensure};
use oqto_personal::{PairingAuthority, hands::HandsClaim};
use oqto_sandbox::{SandboxConfig, SandboxStdin, SeccompMode};
use std::{path::Path, process::Stdio, sync::Arc, time::Duration};
use tokio::io::AsyncReadExt;

pub async fn execute(
    authority: Arc<PairingAuthority>,
    claim: HandsClaim,
    bpf: &Path,
) -> Result<()> {
    // Only the qualified Linux lane is offered by this first provider.
    ensure!(
        cfg!(target_os = "linux"),
        "remote hands provider requires Linux"
    );
    claim.root.validate_identity()?;
    let bpf = bpf.canonicalize()?;
    ensure!(
        !bpf.starts_with(claim.root.path()),
        "execution root contains trusted seccomp asset"
    );
    ensure!(
        !std::env::current_exe()?
            .canonicalize()?
            .starts_with(claim.root.path()),
        "execution root contains Runner executable"
    );
    if let Some(shim) = std::env::var_os("OQTO_SANDBOX_SHIM_BIN") {
        ensure!(
            !Path::new(&shim)
                .canonicalize()?
                .starts_with(claim.root.path()),
            "execution root contains sandbox shim"
        );
    }
    let mut policy = SandboxConfig::strict();
    policy.allow_write.clear();
    policy.allow_read.clear();
    policy.seccomp_mode = SeccompMode::Enforce;
    policy.seccomp_bpf_path = Some(bpf.canonicalize()?.to_string_lossy().into_owned());
    let mut command = oqto_sandbox::build_sandbox_command(
        &policy,
        claim.root.path(),
        Path::new("/bin/sh"),
        &["-c".into(), claim.command.clone()],
        SandboxStdin::Redirected,
    )?;
    command
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", "/nonexistent")
        .env("LANG", "C.UTF-8")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    oqto_sandbox::configure_bwrap_pre_exec(&mut command, &policy, claim.root.path(), None)?;
    ensure!(
        authority.hands_active(&claim).await?,
        "invocation revoked before spawn"
    );
    let mut command = tokio::process::Command::from(command);
    command.kill_on_drop(true);
    let mut child = command.spawn().context("spawn confined hands")?;
    let stdout = child.stdout.take().context("missing stdout")?;
    let stderr = child.stderr.take().context("missing stderr")?;
    let out = tokio::spawn(read_bounded(stdout));
    let err = tokio::spawn(read_bounded(stderr));
    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    let status = loop {
        tokio::select! {
            status = child.wait() => break status?,
            _ = tokio::time::sleep(Duration::from_millis(50)) => {
                if tokio::time::Instant::now() >= deadline || !authority.hands_active(&claim).await? {
                    child.kill().await.context("stop revoked hands")?;
                    let _ = child.wait().await;
                    out.abort(); err.abort();
                    authority.finish_hands(&claim, "", None).await?;
                    return Ok(());
                }
            }
        }
    };
    // PID-namespace init exiting kills its descendants; never wait indefinitely
    // for an inherited pipe after a workload exits.
    let output = tokio::time::timeout(Duration::from_secs(2), async {
        Ok::<_, anyhow::Error>((out.await??, err.await??))
    })
    .await
    .context("drain confined output")??;
    let text = format!(
        "stdout:\n{}\nstderr:\n{}",
        bounded_text(&output.0),
        bounded_text(&output.1)
    );
    authority.finish_hands(&claim, &text, status.code()).await
}

fn bounded_text(bytes: &[u8]) -> String {
    let mut text = String::from_utf8_lossy(bytes).into_owned();
    if bytes.len() >= 30_000 || text.len() > 30_000 {
        let mut end = text.len().min(30_000);
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        text.truncate(end);
        text.push_str("\n[output capped at 30000 bytes]");
    }
    text
}

async fn read_bounded(mut input: impl tokio::io::AsyncRead + Unpin) -> Result<Vec<u8>> {
    let mut retained = Vec::new();
    let mut buffer = [0u8; 8192];
    loop {
        let n = input.read(&mut buffer).await?;
        if n == 0 {
            return Ok(retained);
        }
        let keep = n.min(30_000usize.saturating_sub(retained.len()));
        retained.extend_from_slice(&buffer[..keep]);
        // Continue draining after the retention cap so large output cannot
        // deadlock execution or consume unbounded memory.
    }
}
