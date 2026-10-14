#![cfg(target_os = "linux")]

use oq_permissions::*;
use oqto_sandbox::{
    SandboxConfig, SandboxStdin, SeccompMode, build_sandbox_command, configure_bwrap_pre_exec,
};
use serde_json::{Value, json};
use std::{
    fs,
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    process::{Child, ChildStdin, ChildStdout, Stdio},
};

const WORKER: &str = r#"
import json, sys, subprocess
with open('/proc/self/status') as f: status = f.read()
filters = int(next(line.split(':', 1)[1] for line in status.splitlines() if line.startswith('Seccomp_filters:')))
print(json.dumps({'ready':True, 'seccomp_filters':filters}), flush=True)
for line in sys.stdin:
    try:
        request = json.loads(line)
        if request['action'] == 'read':
            with open(request['path']) as f: result = {'ok':True, 'text':f.read(1024)}
        elif request['action'] == 'write':
            with open(request['path'], 'w') as f: f.write('forbidden')
            result = {'ok':True}
        elif request['action'] == 'spawn':
            subprocess.Popen(['/usr/bin/sleep', '60'])
            result = {'ok':True}
        else: result = {'ok':False}
    except Exception as e: result = {'ok':False, 'error':str(e)}
    print(json.dumps(result), flush=True)
"#;

struct Hands {
    child: Child,
    input: ChildStdin,
    output: BufReader<ChildStdout>,
    generation: u64,
}
impl Hands {
    fn start(workdir: &Path, extra: Option<&Path>, generation: u64) -> anyhow::Result<Self> {
        let mut config = SandboxConfig::strict();
        config.allow_read.clear();
        config.allow_write.clear();
        config.extra_ro_bind.clear();
        config.extra_rw_bind.clear();
        config.workspace_cache_enabled = false;
        config.seccomp_mode = SeccompMode::Enforce;
        let bpf = std::env::var("OQ_ADAPTIVE_SECCOMP_BPF")?;
        config.seccomp_bpf_path = Some(bpf);
        if let Some(extra) = extra {
            config.extra_ro_bind.push(extra.display().to_string());
        }
        let args = vec!["-I".into(), "-u".into(), "-c".into(), WORKER.into()];
        let mut command = build_sandbox_command(
            &config,
            workdir,
            Path::new("/usr/bin/python3"),
            &args,
            SandboxStdin::Redirected,
        )?;
        command
            .env_clear()
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit());
        configure_bwrap_pre_exec(&mut command, &config, workdir, None)?;
        let mut child = command.spawn()?;
        let input = child
            .stdin
            .take()
            .ok_or_else(|| anyhow::anyhow!("hands stdin absent"))?;
        let output = BufReader::new(
            child
                .stdout
                .take()
                .ok_or_else(|| anyhow::anyhow!("hands stdout absent"))?,
        );
        let mut hands = Self {
            child,
            input,
            output,
            generation,
        };
        let ready = hands.receive()?;
        anyhow::ensure!(ready["ready"] == true, "hands failed readiness");
        let host_status = fs::read_to_string("/proc/self/status")?;
        let host_filters: u64 = host_status
            .lines()
            .find_map(|line| line.strip_prefix("Seccomp_filters:"))
            .ok_or_else(|| anyhow::anyhow!("kernel cannot report seccomp filter count"))?
            .trim()
            .parse()?;
        anyhow::ensure!(
            ready["seccomp_filters"]
                .as_u64()
                .is_some_and(|count| count > host_filters),
            "hands did not install an additional seccomp filter"
        );
        Ok(hands)
    }
    fn receive(&mut self) -> anyhow::Result<Value> {
        let mut line = String::new();
        anyhow::ensure!(self.output.read_line(&mut line)? > 0, "hands disconnected");
        Ok(serde_json::from_str(&line)?)
    }
    fn call(&mut self, fence: &ExecutorFence, request: Value) -> anyhow::Result<Value> {
        fence.admit(self.generation)?;
        writeln!(self.input, "{request}")?;
        self.input.flush()?;
        self.receive()
    }
    fn stop(&mut self) -> anyhow::Result<()> {
        self.child.kill()?;
        self.child.wait()?;
        Ok(())
    }
}
impl Drop for Hands {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
fn descendants(pid: u32) -> Vec<u32> {
    let children =
        fs::read_to_string(format!("/proc/{pid}/task/{pid}/children")).unwrap_or_default();
    let mut found = Vec::new();
    for child in children
        .split_whitespace()
        .filter_map(|p| p.parse::<u32>().ok())
    {
        found.push(child);
        found.extend(descendants(child));
    }
    found
}
fn private(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
}
fn binding() -> Binding {
    Binding {
        account_id: "owner".into(),
        principal_id: "principal".into(),
        workspace_id: "workspace".into(),
        work_directory_id: "workdir".into(),
        session_id: "public-session".into(),
        audience: "owner-only".into(),
    }
}

#[tokio::test(flavor = "current_thread")]
async fn real_bwrap_seccomp_replacement_and_revocation_keep_brain_identity() {
    // /tmp is deliberately private in existing SandboxConfig. /var/tmp lets
    // the compiled explicit extra mount remain visible without changing it.
    let dir = tempfile::Builder::new()
        .prefix("oq-adaptive-")
        .tempdir_in("/var/tmp")
        .unwrap();
    private(dir.path());
    let workdir = dir.path().join("workdir");
    fs::create_dir(&workdir).unwrap();
    let protected = dir.path().join("protected");
    fs::create_dir(&protected).unwrap();
    private(&protected);
    let extra = dir.path().join("extra");
    fs::create_dir(&extra).unwrap();
    let file = extra.join("token");
    fs::write(&file, "approved extra token").unwrap();
    fs::write(protected.join("private-history"), "must never reach hands").unwrap();
    let policy = Policy {
        paths: vec![],
        revision: "v1".into(),
        rules: vec![Rule {
            target: Target {
                capability: Capability::FileRead,
                resource_id: "extra-resource".into(),
            },
            mode: RuleMode::Ask,
        }],
    };
    let authority = Authority::open(&protected.join("permissions.sqlite"), binding(), policy)
        .await
        .unwrap();
    let operation = Operation {
        request_id: "native-store:read-task-1".into(),
        target: Target {
            capability: Capability::FileRead,
            resource_id: "extra-resource".into(),
        },
        arguments: json!({"relative_path":"token"}),
    };
    let brain_identity = (std::process::id(), binding().session_id);
    let mut fence = ExecutorFence::default();
    let generation = fence.install_after_confirmed_stop(0).unwrap();
    let mut first = Hands::start(&workdir, None, generation).unwrap();
    let first_pid = first.child.id();
    assert_eq!(
        first
            .call(&fence, json!({"action":"read", "path":file}))
            .unwrap()["ok"],
        false
    );
    assert_eq!(
        first
            .call(
                &fence,
                json!({"action":"read", "path":protected.join("private-history")})
            )
            .unwrap()["ok"],
        false
    );
    let request = match authority.admit(operation.clone(), 100).await.unwrap() {
        Admission::Pending(r) => r,
        _ => panic!("file grant should wait"),
    };
    let approved = authority
        .decide(
            "owner",
            &operation.request_id,
            &request.digest,
            1,
            GrantLifetime::Session,
            true,
            1000,
            101,
        )
        .await
        .unwrap();
    let permit = match authority.admit(operation.clone(), 102).await.unwrap() {
        Admission::Allowed(p) => p,
        _ => panic!("file grant should allow"),
    };
    authority.check(&permit, &operation, 102).await.unwrap();
    first.call(&fence, json!({"action":"spawn"})).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(80));
    let old_descendants = descendants(first_pid);
    assert!(!old_descendants.is_empty());
    fence.freeze();
    assert!(
        first
            .call(&fence, json!({"action":"read", "path":file}))
            .is_err()
    );
    first.stop().unwrap();
    let generation = fence.install_after_confirmed_stop(generation).unwrap();
    let mut second = Hands::start(&workdir, Some(&extra), generation).unwrap();
    assert_ne!(second.child.id(), first_pid);
    assert!(fence.admit(first.generation).is_err());
    let result = second
        .call(&fence, json!({"action":"read", "path":file}))
        .unwrap();
    authority.check(&permit, &operation, 103).await.unwrap();
    assert_eq!(result["text"], "approved extra token");
    assert_eq!(
        second
            .call(&fence, json!({"action":"write", "path":file}))
            .unwrap()["ok"],
        false
    );
    assert_eq!(
        second
            .call(
                &fence,
                json!({"action":"read", "path":protected.join("permissions.sqlite")})
            )
            .unwrap()["ok"],
        false
    );
    authority
        .revoke("owner", approved.grant_id.as_ref().unwrap())
        .await
        .unwrap();
    assert!(authority.check(&permit, &operation, 104).await.is_err());
    fence.freeze();
    second.stop().unwrap();
    let generation = fence.install_after_confirmed_stop(generation).unwrap();
    let mut third = Hands::start(&workdir, None, generation).unwrap();
    assert_eq!(
        third
            .call(&fence, json!({"action":"read", "path":file}))
            .unwrap()["ok"],
        false
    );
    assert_eq!(brain_identity, (std::process::id(), binding().session_id));
    for _ in 0..100 {
        if old_descendants
            .iter()
            .all(|p| !PathBuf::from(format!("/proc/{p}")).exists())
        {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    for pid in old_descendants {
        // A dead orphan can briefly be a zombie awaiting host init's reap;
        // it cannot retain mounts or execute an effect.
        if let Ok(status) = fs::read_to_string(format!("/proc/{pid}/status")) {
            assert!(
                status
                    .lines()
                    .any(|l| l.starts_with("State:") && l.contains('Z')),
                "old descendant alive: {status}"
            );
        }
    }
    third.stop().unwrap();
}
