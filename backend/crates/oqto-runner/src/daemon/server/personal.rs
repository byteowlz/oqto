//! Personal mode is another Runner entry point, not a separate authority or
//! session supervisor. Native adoption is composed here, never spawned on read.
use oqto_personal::{
    AuthorizedAction, SessionAccess,
    actions::{ActionBackend, ActionFailure},
};
use oqto_personal_wire::actions::{HostAction, HostCapabilities, RootInfo};
use serde_json::Value;
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};

pub struct PersonalBackend {
    home: PathBuf,
    workspace_roots: Vec<PathBuf>,
    terminal_enabled: bool,
    files: Arc<Mutex<()>>,
    terminals: Arc<crate::personal_terminal::Terminals>,
    sessions: Arc<crate::personal_sessions::Sessions>,
    reaper: tokio::task::JoinHandle<()>,
}
impl Drop for PersonalBackend {
    fn drop(&mut self) {
        self.reaper.abort();
    }
}
impl PersonalBackend {
    pub fn new(
        home: PathBuf,
        workspace_roots: Vec<PathBuf>,
        terminal_enabled: bool,
        ingest: super::super::jsonl_ingest::IngestHandle,
    ) -> Self {
        let terminals = Arc::new(crate::personal_terminal::Terminals::default());
        let weak = Arc::downgrade(&terminals);
        let sessions = Arc::new(crate::personal_sessions::Sessions::with_ingest(ingest));
        let weak_sessions = Arc::downgrade(&sessions);
        let reaper = tokio::spawn(async move {
            let mut tick = tokio::time::interval(std::time::Duration::from_millis(500));
            loop {
                tick.tick().await;
                if let Some(sessions) = weak_sessions.upgrade() {
                    sessions.reap();
                }
                let Some(terminals) = weak.upgrade() else {
                    break;
                };
                if tokio::task::spawn_blocking(move || terminals.reap())
                    .await
                    .is_err()
                {
                    break;
                }
            }
        });
        Self {
            home,
            workspace_roots,
            terminal_enabled,
            files: Arc::new(Mutex::new(())),
            terminals,
            sessions,
            reaper,
        }
    }
    pub fn supported(&self) -> oqto_personal::owner::Supported {
        oqto_personal::owner::Supported {
            files: true,
            terminal: self.terminal_enabled,
            sessions: !self.workspace_roots.is_empty(),
            control: !self.workspace_roots.is_empty(),
        }
    }
}
impl super::Runner {
    pub async fn run_personal(
        self: Arc<Self>,
        options: crate::personal_cli::ServerOptions,
    ) -> anyhow::Result<()> {
        self.run_personal_inner(options, true).await
    }
    /// One Runner, one Pi manager and one serialized ingest writer. The existing
    /// authenticated transport keeps its own grants; pairing confers none there.
    pub async fn run_transport_with_personal<L: crate::transport::RunnerListener>(
        self: Arc<Self>,
        listener: &L,
        access: super::ConnectionAccess,
        options: crate::personal_cli::ServerOptions,
    ) -> anyhow::Result<()> {
        self.clone()
            .cohost(self.run_transport(listener, access), options)
            .await
    }
    pub async fn run_socket_with_personal(
        self: Arc<Self>,
        socket: &PathBuf,
        options: crate::personal_cli::ServerOptions,
    ) -> anyhow::Result<()> {
        self.clone().cohost(self.run(socket), options).await
    }
    async fn cohost<F: std::future::Future<Output = anyhow::Result<()>>>(
        self: Arc<Self>,
        transport: F,
        options: crate::personal_cli::ServerOptions,
    ) -> anyhow::Result<()> {
        tokio::pin!(transport);
        let personal = self.clone().run_personal_inner(options, false);
        tokio::pin!(personal);
        tokio::select! {
            result = &mut personal => {
                let _ = self.shutdown_tx.send(());
                let stopped = match tokio::time::timeout(std::time::Duration::from_secs(5), &mut transport).await {
                    Ok(stopped) => stopped,
                    Err(_) => {
                        // Broadcast is not sticky if startup failed before the
                        // primary listener subscribed. Cancellation still owns
                        // its socket lease; perform managed cleanup once here.
                        self.pi_manager.shutdown().await;
                        self.cleanup_managed_processes().await;
                        Err(anyhow::anyhow!("Primary Runner shutdown deadline exceeded"))
                    }
                };
                result.and(stopped)
            }
            result = &mut transport => {
                let _ = self.shutdown_tx.send(());
                // Dropping the personal future closes its listeners, connection
                // JoinSets and owner inode. Terminal-owned Pi is never killed.
                result
            }
        }
    }
    async fn run_personal_inner(
        self: Arc<Self>,
        options: crate::personal_cli::ServerOptions,
        owns_lifecycle: bool,
    ) -> anyhow::Result<()> {
        use anyhow::{Context, ensure};
        use oqto_personal::PairingAuthority;
        use std::os::unix::fs::MetadataExt;
        ensure!(
            self.user_config.single_user && !self.user_config.linux_users_enabled,
            "personal mode requires the dedicated host-user Runner"
        );
        let address = options
            .personal_listen
            .context("personal listen address required")?;
        let origin = oqto_personal_wire::validate_origin(
            options
                .personal_origin
                .as_deref()
                .context("personal origin required")?,
        )?;
        ensure!(
            !options.personal_no_pair
                || (options.grants.read_roots.is_empty()
                    && options.grants.write_roots.is_empty()
                    && !options.grants.full_user_terminal
                    && matches!(options.grants.sessions, crate::personal_cli::Sessions::None)),
            "grant flags require startup pairing; remove --personal-no-pair or use personal pair later"
        );
        for root in &options.workspace_roots {
            ensure!(
                root.is_absolute() && root.is_dir() && root.canonicalize()? == *root,
                "personal Workspace roots must be canonical absolute directories"
            );
        }
        ensure!(
            options.workspace_roots.len() <= 32,
            "too many personal Workspace roots"
        );
        let home = if let Some(source) = &self.user_config.personal_source {
            source.validate()?;
            ensure!(
                source.workspace_roots() == options.workspace_roots,
                "Personal Workspace source mismatch"
            );
            source.home.clone()
        } else {
            ensure!(
                options.personal_native_session_root.is_none()
                    && options.personal_native_socket_root.is_none(),
                "Native source must be resolved before starting Runner workers"
            );
            dirs::home_dir().context("personal history home unavailable")?
        };
        crate::runtime_guard::ensure_history_state_owner(&home)?;
        let authority = Arc::new(
            PairingAuthority::open_private(&crate::personal_cli::state_path(
                options.personal_state.as_ref(),
            )?)
            .await?,
        );
        let identity = authority.transport_identity().await?;
        let config = identity.server_config()?;
        let listener = tokio::net::TcpListener::bind(address).await?;
        let owner_listener = authority.bind_owner_socket()?;
        let owner_path = authority.owner_socket();
        let owner_meta = std::fs::symlink_metadata(&owner_path)?;
        let _owner_cleanup = OwnerCleanup {
            path: owner_path,
            dev: owner_meta.dev(),
            ino: owner_meta.ino(),
        };
        let backend = Arc::new(PersonalBackend::new(
            home,
            options.workspace_roots,
            self.user_config.terminal_enabled,
            self.jsonl_ingest.clone(),
        ));
        let supported = backend.supported();
        let router = oqto_personal::http::enrollment_router(Arc::clone(&authority)).merge(
            oqto_personal::actions::action_router(Arc::clone(&authority), backend),
        );
        let mut network_shutdown = self.shutdown_tx.subscribe();
        let mut owner_shutdown = self.shutdown_tx.subscribe();
        if owns_lifecycle {
            self.install_shutdown_signals();
            self.bootstrap_history_index();
        }
        if !options.personal_no_pair {
            if options.grants.full_user_terminal {
                log::warn!(
                    "Pairing ticket grants full host-user terminal execution; Files roots do not confine that shell"
                );
            }
            let qr = authority
                .owner_command(
                    options.grants.command(),
                    &origin,
                    identity.leaf_der(),
                    supported,
                )
                .await?;
            crate::personal_cli::emit_pair(qr, options.output.unwrap_or_default())?;
        }
        log::info!(
            "Personal Runner HTTPS listening on {}; no centralized Account is required",
            listener.local_addr()?
        );
        if owns_lifecycle {
            super::sd_notify_ready();
        }
        let result = tokio::select! {
            result=oqto_personal::http::serve_personal_tls(listener,config,router,async {let _=network_shutdown.recv().await;})=>result,
            result=oqto_personal::owner::serve_owner(owner_listener,authority,origin,identity.leaf_der().to_vec(),supported,async {let _=owner_shutdown.recv().await;})=>result,
        };
        if owns_lifecycle {
            self.pi_manager.shutdown().await;
            self.cleanup_managed_processes().await;
        }
        result
    }
}

struct OwnerCleanup {
    path: PathBuf,
    dev: u64,
    ino: u64,
}
impl Drop for OwnerCleanup {
    fn drop(&mut self) {
        use std::os::unix::fs::MetadataExt;
        if let Ok(meta) = std::fs::symlink_metadata(&self.path)
            && (meta.dev(), meta.ino()) == (self.dev, self.ino)
        {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

#[async_trait::async_trait]
impl ActionBackend for PersonalBackend {
    async fn dispatch(
        &self,
        action: HostAction,
        authorization: AuthorizedAction,
    ) -> Result<Value, ActionFailure> {
        match action {
            HostAction::Capabilities => {
                let grants = authorization.grants();
                let fence = authorization.fence();
                let supported = self.supported();
                serde_json::to_value(HostCapabilities {
                    version: 1,
                    host_id: fence.host_id().into(),
                    device_id: fence.device_id().into(),
                    roots: grants
                        .files
                        .iter()
                        .map(|root| RootInfo {
                            id: root.id().into(),
                            label: root
                                .path()
                                .file_name()
                                .map(|name| {
                                    name.to_string_lossy()
                                        .chars()
                                        .filter(|character| !character.is_control())
                                        .take(96)
                                        .collect()
                                })
                                .unwrap_or_else(|| "Files".into()),
                            writable: root.writable(),
                        })
                        .collect(),
                    session_observe_granted: grants.sessions != SessionAccess::None,
                    session_control_granted: grants.sessions == SessionAccess::Control,
                    full_user_terminal_granted: grants.full_user_terminal,
                    files_supported: supported.files,
                    terminal_supported: supported.terminal,
                    session_observe_supported: supported.sessions,
                    session_control_supported: supported.control,
                })
                .map_err(|_| ActionFailure::Unavailable)
            }
            action @ (HostAction::FilesList { .. }
            | HostAction::FileRead { .. }
            | HostAction::FileWrite { .. }
            | HostAction::FileRemove { .. }
            | HostAction::DirectoryCreate { .. }) => {
                let serial = Arc::clone(&self.files);
                tokio::task::spawn_blocking(move || {
                    let _serial = serial.lock().map_err(|_| ActionFailure::Unavailable)?;
                    crate::personal_files::execute(action, authorization)
                })
                .await
                .map_err(|_| ActionFailure::OutcomeUnknown)?
            }
            action @ (HostAction::TerminalsList
            | HostAction::TerminalOpen { .. }
            | HostAction::TerminalRead { .. }
            | HostAction::TerminalWrite { .. }
            | HostAction::TerminalResize { .. }
            | HostAction::TerminalClose { .. }) => {
                if !self.terminal_enabled {
                    return Err(ActionFailure::Unavailable);
                }
                let terminals = Arc::clone(&self.terminals);
                tokio::task::spawn_blocking(move || terminals.execute(action, authorization))
                    .await
                    .map_err(|_| ActionFailure::OutcomeUnknown)?
            }
            HostAction::SessionsList => crate::history_read::read_authorized_source(
                &self.home,
                &self.workspace_roots,
                crate::history_read::HistoryReadOperation::List {},
            )
            .await
            .map(|response| response.data)
            .map_err(|_| ActionFailure::Unavailable),
            HostAction::SessionHistory { session_id, before } => {
                crate::history_read::read_native_authorized_history(
                    &self.home,
                    &self.workspace_roots,
                    session_id,
                    before,
                )
                .await
                .map(|response| response.data)
                .map_err(|_| ActionFailure::Unavailable)
            }
            action @ (HostAction::SessionAttach { .. }
            | HostAction::SessionRead { .. }
            | HostAction::SessionRequestControl { .. }
            | HostAction::SessionReleaseControl { .. }
            | HostAction::SessionInput { .. }
            | HostAction::SessionDetach { .. }) => {
                self.sessions
                    .execute(&self.home, &self.workspace_roots, action, authorization)
                    .await
            }
        }
    }
}
