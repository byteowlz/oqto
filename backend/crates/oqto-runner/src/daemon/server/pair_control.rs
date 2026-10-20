//! Same-UID local pairing administration. Never exposed over runner/network RPC.
//! Enables another listener on the existing Runner; does not create a Pi manager.
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::io::Read as _;
use std::{
    fs::{File, OpenOptions},
    net::SocketAddr,
    os::unix::fs::{FileTypeExt, MetadataExt, OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    net::{UnixListener, UnixStream},
};

const LIMIT: u64 = 32768;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ListenerPlan {
    pub address: SocketAddr,
    pub origin: String,
    pub workspaces: Vec<PathBuf>,
    pub native_home: PathBuf,
}
impl ListenerPlan {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            !self.address.ip().is_unspecified()
                && !self.address.ip().is_multicast()
                && self.address.port() != 0,
            "select a reachable interface address"
        );
        let origin = oqto_personal_wire::validate_origin(&self.origin)?;
        ensure!(
            origin == format!("https://{}/", self.address),
            "origin must match the selected listener address"
        );
        ensure!(
            !self.workspaces.is_empty() && self.workspaces.len() <= 32,
            "select 1–32 Workspaces"
        );
        for workspace in &self.workspaces {
            for private in [
                root(&self.native_home),
                self.native_home.join(".local/share/oqto/personal"),
            ] {
                ensure!(
                    !private.starts_with(workspace) && !workspace.starts_with(&private),
                    "Workspace overlaps private pairing administration state"
                );
            }
        }
        for p in self
            .workspaces
            .iter()
            .chain(std::iter::once(&self.native_home))
        {
            ensure!(
                p.is_absolute() && p.is_dir() && p.canonicalize()? == *p,
                "directories must be canonical, absolute and exist"
            );
        }
        Ok(())
    }
    pub fn options(&self, state: PathBuf) -> crate::personal_cli::ServerOptions {
        crate::personal_cli::ServerOptions {
            personal_listen: Some(self.address),
            personal_origin: Some(self.origin.clone()),
            personal_state: Some(state),
            workspace_roots: self.workspaces.clone(),
            personal_no_pair: true,
            ..Default::default()
        }
    }
}
#[derive(Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case", deny_unknown_fields)]
pub enum Request {
    Status,
    Enable { plan: ListenerPlan },
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Status {
    pub schema: u32,
    pub pid: u32,
    pub active: bool,
    pub state: PathBuf,
    pub plan: Option<ListenerPlan>,
}

pub fn root(home: &Path) -> PathBuf {
    home.join(".local/share/oqto/pairing")
}
pub fn validate_socket_path(path: &Path) -> Result<()> {
    use std::os::unix::ffi::OsStrExt;
    // Darwin sun_path is104 bytes including NUL; use the portable bound even on Linux.
    ensure!(
        path.as_os_str().as_bytes().len() <= 103,
        "pairing socket path exceeds the portable Unix limit; select a shorter managed profile path"
    );
    Ok(())
}
pub fn private_dir(path: &Path) -> Result<()> {
    use std::fs::DirBuilder;
    use std::os::unix::fs::DirBuilderExt;
    // Validate existing ancestors before creating anything, including dangling links.
    for ancestor in path.ancestors() {
        match std::fs::symlink_metadata(ancestor) {
            Ok(meta) => ensure!(
                !meta.file_type().is_symlink(),
                "symlinked pairing state is unsupported"
            ),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    if !path.exists() {
        DirBuilder::new().recursive(true).mode(0o700).create(path)?;
    }
    let m = std::fs::symlink_metadata(path)?;
    ensure!(
        m.is_dir() && m.uid() == rustix::process::geteuid().as_raw() && m.mode() & 0o077 == 0,
        "untrusted pairing directory"
    );
    Ok(())
}
pub fn lock(path: &Path) -> Result<File> {
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)?;
    let m = file.metadata()?;
    ensure!(
        m.is_file()
            && m.uid() == rustix::process::geteuid().as_raw()
            && m.mode() & 0o077 == 0
            && m.nlink() == 1,
        "untrusted pairing lock"
    );
    file.try_lock()
        .context("another pairing operation or Runner owns this lock")?;
    Ok(file)
}
fn owned_socket(path: &Path) -> Result<()> {
    let m = std::fs::symlink_metadata(path)?;
    ensure!(
        m.file_type().is_socket()
            && m.uid() == rustix::process::geteuid().as_raw()
            && m.mode() & 0o077 == 0,
        "untrusted pairing socket"
    );
    Ok(())
}
pub async fn request(path: &Path, command: &Request) -> Result<Status> {
    private_existing_dir(path)?;
    let socket = path.join("control.sock");
    owned_socket(&socket)?;
    tokio::time::timeout(std::time::Duration::from_secs(15), async {
        let mut stream = UnixStream::connect(socket).await?;
        ensure!(
            stream.peer_cred()?.uid() == rustix::process::geteuid().as_raw(),
            "pairing controller identity denied"
        );
        let mut bytes = serde_json::to_vec(command)?;
        ensure!(bytes.len() < (LIMIT as usize), "pairing request too large");
        bytes.push(b'\n');
        stream.write_all(&bytes).await?;
        let mut data = Vec::new();
        BufReader::new(stream)
            .take(LIMIT + 1)
            .read_until(b'\n', &mut data)
            .await?;
        ensure!(
            data.len() as u64 <= LIMIT && data.last() == Some(&b'\n'),
            "invalid pairing controller response"
        );
        let reply: serde_json::Value = serde_json::from_slice(&data)?;
        ensure!(
            reply["ok"] == true,
            "{}",
            reply["error"].as_str().unwrap_or("pairing request refused")
        );
        Ok(serde_json::from_value(reply["data"].clone())?)
    })
    .await
    .context("pairing controller timed out; inspect status before retrying")?
}
fn private_existing_dir(path: &Path) -> Result<()> {
    ensure!(path.exists(), "pairing controller is not running");
    private_dir(path)
}
struct ListenerTask(Option<tokio::task::JoinHandle<Result<()>>>);
impl ListenerTask {
    fn is_finished(&self) -> bool {
        self.0
            .as_ref()
            .is_none_or(tokio::task::JoinHandle::is_finished)
    }
    async fn stop(mut self) {
        if let Some(task) = self.0.take() {
            task.abort();
            let _ = task.await;
        }
    }
}
impl Drop for ListenerTask {
    fn drop(&mut self) {
        if let Some(task) = &self.0 {
            task.abort();
        }
    }
}

fn publish_settings(path: &Path, plan: &ListenerPlan) -> Result<()> {
    use std::io::Write;
    let stage = path.with_extension(format!("{}.stage", uuid::Uuid::new_v4()));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&stage)?;
    let meta = file.metadata()?;
    let _cleanup = SocketLease {
        path: stage.clone(),
        dev: meta.dev(),
        ino: meta.ino(),
    };
    file.write_all(&serde_json::to_vec(plan)?)?;
    file.sync_all()?;
    // Create-only atomic publication: never replace unknown configuration.
    std::fs::hard_link(stage, path)?;
    File::open(path.parent().context("settings parent unavailable")?)?.sync_all()?;
    Ok(())
}
struct SocketLease {
    path: PathBuf,
    dev: u64,
    ino: u64,
}
impl Drop for SocketLease {
    fn drop(&mut self) {
        if let Ok(m) = std::fs::symlink_metadata(&self.path)
            && (m.dev(), m.ino()) == (self.dev, self.ino)
        {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

impl super::Runner {
    /// Runtime owner control is separate from Device pairing and Account/MFA.
    /// Legacy explicitly configured listeners retain their existing lifecycle.
    pub async fn with_pairing_control<F: std::future::Future<Output = Result<()>>>(
        self: std::sync::Arc<Self>,
        transport: F,
    ) -> Result<()> {
        if !self.user_config.single_user || self.user_config.linux_users_enabled {
            return transport.await;
        }
        let home = dirs::home_dir()
            .context("host home unavailable")?
            .canonicalize()?;
        let path = root(&home);
        validate_socket_path(&path.join("control.sock"))?;
        validate_socket_path(&path.join("host/owner.sock"))?;
        private_dir(&path)?;
        let _lock = lock(&path.join("runner.lock"))?;
        let socket = path.join("control.sock");
        if std::fs::symlink_metadata(&socket).is_ok() {
            owned_socket(&socket)?;
            std::fs::remove_file(&socket)?;
        }
        let listener = UnixListener::bind(&socket)?;
        std::fs::set_permissions(&socket, std::fs::Permissions::from_mode(0o600))?;
        let meta = std::fs::symlink_metadata(&socket)?;
        let _lease = SocketLease {
            path: socket,
            dev: meta.dev(),
            ino: meta.ino(),
        };
        let state = path.join("host");
        let mut active: Option<(ListenerPlan, ListenerTask)> = None;
        let settings = path.join("listener.json");
        if std::fs::symlink_metadata(&settings).is_ok() {
            let mut input = OpenOptions::new()
                .read(true)
                .custom_flags(libc::O_NOFOLLOW)
                .open(&settings)?;
            let meta = input.metadata()?;
            ensure!(
                meta.is_file()
                    && meta.uid() == rustix::process::geteuid().as_raw()
                    && meta.mode() & 0o077 == 0,
                "untrusted listener settings"
            );
            let mut bytes = Vec::new();
            std::io::Read::take(&mut input, LIMIT + 1).read_to_end(&mut bytes)?;
            ensure!(bytes.len() as u64 <= LIMIT, "listener settings too large");
            let plan: ListenerPlan = serde_json::from_slice(&bytes)?;
            plan.validate()?;
            let task = match self
                .clone()
                .start_pair_listener(plan.clone(), &home, &state)
                .await
            {
                Ok(task) => task,
                Err(error) => {
                    log::error!("Stored pairing listener unavailable: {error}");
                    ListenerTask(None)
                }
            };
            active = Some((plan, task));
        }
        tokio::pin!(transport);
        let result = {
            let control = async {
                loop {
                    let (stream, _) = listener.accept().await?;
                    if stream.peer_cred()?.uid() != rustix::process::geteuid().as_raw() {
                        continue;
                    }
                    let operation = async {
                        let mut reader = BufReader::new(stream);
                        let mut bytes = Vec::new();
                        (&mut reader)
                            .take(LIMIT + 1)
                            .read_until(b'\n', &mut bytes)
                            .await?;
                        ensure!(
                            bytes.len() as u64 <= LIMIT && bytes.last() == Some(&b'\n'),
                            "invalid pairing request"
                        );
                        let request: Request = serde_json::from_slice(&bytes)?;
                        let outcome:Result<Status>=async {
                            if let Request::Enable{plan}=request {
                                plan.validate()?;
                                if let Some((current,task))=&active {
                                    ensure!(!task.is_finished(),"personal listener stopped; inspect runner logs");
                                    ensure!(current==&plan,"listener already configured differently; refusing to widen existing Device scope");
                                }else{
                                    ensure!(!home.join(".local/share/oqto/personal").try_exists()?,"existing manual personal identity preserved; explicit migration required before managed pairing");
                                    ensure!(!state.try_exists()?,"uncommitted personal identity preserved; inspect prior activation failure before retrying");
                                    let task=self.clone().start_pair_listener(plan.clone(),&home,&state).await?;
                                    if let Err(error)=publish_settings(&settings,&plan) {task.stop().await;return Err(error);}
                                    active=Some((plan,task));
                                }
                            }
                            Ok(Status{schema:1,pid:std::process::id(),active:active.as_ref().is_some_and(|(_,task)|!task.is_finished()),state:state.clone(),plan:active.as_ref().map(|(plan,_)|plan.clone())})
                        }.await;
                        let response = match outcome {
                            Ok(status) => serde_json::json!({"ok":true,"data":status}),
                            Err(error) => serde_json::json!({"ok":false,"error":error.to_string()}),
                        };
                        let mut bytes = serde_json::to_vec(&response)?;
                        bytes.push(b'\n');
                        reader.get_mut().write_all(&bytes).await?;
                        Ok::<(), anyhow::Error>(())
                    };
                    let _ =
                        tokio::time::timeout(std::time::Duration::from_secs(12), operation).await;
                }
            };
            tokio::select! { result=&mut transport=>result, result=control=>result }
        };
        if let Some((_, task)) = active {
            task.stop().await;
        }
        result
    }
    async fn start_pair_listener(
        self: std::sync::Arc<Self>,
        plan: ListenerPlan,
        home: &Path,
        state: &Path,
    ) -> Result<ListenerTask> {
        ensure!(
            plan.native_home == home,
            "Runner native source differs from your home; source-binding upgrade required before pairing (oqto-8x3p.10.7.5)"
        );
        let (ready, receive) = tokio::sync::oneshot::channel();
        let options = plan.options(state.to_owned());
        let task = ListenerTask(Some(tokio::spawn(self.run_personal_hot(options, ready))));
        if tokio::time::timeout(std::time::Duration::from_secs(8), receive)
            .await
            .is_ok_and(|r| r.is_ok())
        {
            return Ok(task);
        }
        task.stop().await;
        anyhow::bail!(
            "could not enable pairing listener; check selected address, port and private host state"
        )
    }
}
