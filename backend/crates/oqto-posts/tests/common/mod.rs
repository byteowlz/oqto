use anyhow::{Context, Result};
use oqto_posts::{
    server::{Limits, Registration, Role, Server},
    store::{Policy, Store},
    wire::{Route, Stamp},
};
use serde_json::{Value, json};
use std::path::PathBuf;
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::UnixStream,
};

pub fn registration(name: &str, role: Role) -> Registration {
    Registration {
        name: name.into(),
        role,
        stamp: Stamp {
            agent: format!("{name}-agent"),
            workspace: "work".into(),
            session: Some("public-session-1".into()),
            task: Some("task-1".into()),
            route: Route::Local,
        },
    }
}
pub struct Harness {
    pub dir: tempfile::TempDir,
    pub sockets: PathBuf,
    pub server: Server,
}
impl Harness {
    pub async fn new(limits: Limits) -> Result<Self> {
        let dir = tempfile::tempdir()?;
        let sockets = dir.path().join("s");
        let store = Store::open(&dir.path().join("p.db"), Policy::default()).await?;
        let server = Server::bind(
            store,
            &sockets,
            &[
                registration("agent", Role::Agent),
                registration("owner", Role::Owner),
            ],
            limits,
        )
        .await?;
        Ok(Self {
            dir,
            sockets,
            server,
        })
    }
    pub fn path(&self, name: &str) -> PathBuf {
        self.sockets.join(format!("{name}.sock"))
    }
}
pub fn post(id: &str) -> Value {
    json!({"v":1,"request_id":id,"op":"post","post":{"title":"T","body":"secret","task":"forged","project":"acme/tool"},"unauthenticated_hints":{"agent_ctx":{"agent_id":"owner","workspace_id":"other"}}})
}
pub async fn line(reader: &mut BufReader<UnixStream>) -> Result<Value> {
    let mut text = String::new();
    tokio::time::timeout(
        std::time::Duration::from_secs(3),
        reader.read_line(&mut text),
    )
    .await
    .context("test read timeout")??;
    serde_json::from_str(&text).context("parsing test response")
}
pub async fn request(path: &std::path::Path, value: Value) -> Result<Value> {
    let mut stream = UnixStream::connect(path).await?;
    stream.write_all(format!("{value}\n").as_bytes()).await?;
    line(&mut BufReader::new(stream)).await
}
