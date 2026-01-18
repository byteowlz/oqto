//! Application state shared across handlers.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::body::Body;
use hyper_util::client::legacy::Client;
use hyper_util::client::legacy::connect::HttpConnector;
use hyper_util::rt::TokioExecutor;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use super::super::agent::AgentService;
use super::a2ui::PendingA2uiRequests;
use crate::agent_rpc::AgentBackend;
use crate::auth::AuthState;
use crate::invite::InviteCodeRepository;
use crate::main_chat::{MainChatPiService, MainChatService};
use crate::session::SessionService;
use crate::session_ui::SessionAutoAttachMode;
use crate::settings::SettingsService;
use crate::user::UserService;
use crate::ws::WsHub;

/// Mmry configuration for the API layer.
#[derive(Clone, Debug)]
pub struct MmryState {
    /// Whether mmry integration is enabled.
    pub enabled: bool,
    /// Whether we're in single-user mode (proxy to local service).
    pub single_user: bool,
    /// URL of the local mmry service (for single-user mode).
    pub local_service_url: String,
    /// URL of the central mmry service (for multi-user mode).
    pub host_service_url: String,
    /// API key for authenticating with host mmry (optional).
    pub host_api_key: Option<String>,
}

impl Default for MmryState {
    fn default() -> Self {
        Self {
            enabled: false,
            single_user: true,
            local_service_url: "http://localhost:8081".to_string(),
            host_service_url: "http://localhost:8081".to_string(),
            host_api_key: None,
        }
    }
}

/// Voice mode configuration for the API layer.
///
/// Frontend clients connect to STT/TTS through backplane WebSocket proxies.
/// This state provides the upstream URLs and default settings.
#[derive(Clone, Debug)]
pub struct VoiceState {
    /// Whether voice mode is enabled.
    pub enabled: bool,
    /// WebSocket URL for the eaRS STT service.
    pub stt_url: String,
    /// WebSocket URL for the kokorox TTS service.
    pub tts_url: String,
    /// VAD timeout in milliseconds.
    pub vad_timeout_ms: u32,
    /// Default kokorox voice ID.
    pub default_voice: String,
    /// Default TTS speed (0.1 - 3.0).
    pub default_speed: f32,
    /// Enable auto language detection.
    pub auto_language_detect: bool,
    /// Whether TTS is muted by default.
    pub tts_muted: bool,
    /// Continuous conversation mode.
    pub continuous_mode: bool,
    /// Default visualizer style ("orb" or "kitt").
    pub default_visualizer: String,
    /// Minimum words spoken to interrupt TTS (0 = disabled).
    pub interrupt_word_count: u32,
    /// Reset interrupt word count after this silence in ms (0 = disabled).
    pub interrupt_backoff_ms: u32,
    /// Per-visualizer voice/speed settings.
    pub visualizer_voices: std::collections::HashMap<String, VisualizerVoiceState>,
}

/// Per-visualizer voice settings.
#[derive(Clone, Debug)]
pub struct VisualizerVoiceState {
    pub voice: String,
    pub speed: f32,
}

impl Default for VoiceState {
    fn default() -> Self {
        Self {
            enabled: false,
            stt_url: "ws://localhost:8765".to_string(),
            tts_url: "ws://localhost:8766".to_string(),
            vad_timeout_ms: 1500,
            default_voice: "af_heart".to_string(),
            default_speed: 1.0,
            auto_language_detect: true,
            tts_muted: false,
            continuous_mode: true,
            default_visualizer: "orb".to_string(),
            interrupt_word_count: 2,
            interrupt_backoff_ms: 5000,
            visualizer_voices: [
                (
                    "orb".to_string(),
                    VisualizerVoiceState {
                        voice: "af_heart".to_string(),
                        speed: 1.0,
                    },
                ),
                (
                    "kitt".to_string(),
                    VisualizerVoiceState {
                        voice: "am_michael".to_string(),
                        speed: 1.1,
                    },
                ),
            ]
            .into_iter()
            .collect(),
        }
    }
}

/// Session UX configuration for the API layer.
#[derive(Clone, Debug)]
pub struct SessionUiState {
    pub auto_attach: SessionAutoAttachMode,
    pub auto_attach_scan: bool,
}

impl Default for SessionUiState {
    fn default() -> Self {
        Self {
            auto_attach: SessionAutoAttachMode::Off,
            auto_attach_scan: false,
        }
    }
}

/// Project template repository configuration/state.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum TemplatesRepoType {
    Remote,
    Local,
}

#[derive(Clone, Debug)]
pub struct TemplatesState {
    pub repo_path: Option<PathBuf>,
    pub repo_type: TemplatesRepoType,
    pub sync_on_list: bool,
    pub sync_interval: Duration,
    pub last_sync: Arc<Mutex<Option<Instant>>>,
}

impl TemplatesState {
    pub fn new(
        repo_path: Option<PathBuf>,
        repo_type: TemplatesRepoType,
        sync_on_list: bool,
        sync_interval: Duration,
    ) -> Self {
        Self {
            repo_path,
            repo_type,
            sync_on_list,
            sync_interval,
            last_sync: Arc::new(Mutex::new(None)),
        }
    }
}

impl Default for TemplatesState {
    fn default() -> Self {
        Self::new(None, TemplatesRepoType::Remote, true, Duration::from_secs(120))
    }
}

/// Application state shared across all handlers.
#[derive(Clone)]
pub struct AppState {
    /// Session service for managing container lifecycles.
    pub sessions: Arc<SessionService>,
    /// Agent service for managing opencode agents within containers.
    pub agents: Arc<AgentService>,
    /// User service for user management.
    pub users: Arc<UserService>,
    /// Invite code repository for registration.
    pub invites: Arc<InviteCodeRepository>,
    /// Authentication state.
    pub auth: AuthState,
    /// HTTP client for proxying requests to per-session services.
    pub http_client: Client<HttpConnector, Body>,
    /// Unified agent backend (optional, for new AgentRPC-based architecture).
    pub agent_backend: Option<Arc<dyn AgentBackend>>,
    /// Mmry (memory service) configuration.
    pub mmry: MmryState,
    /// Voice mode configuration.
    pub voice: VoiceState,
    /// Session UX configuration.
    pub session_ui: SessionUiState,
    /// Project templates configuration.
    pub templates: TemplatesState,
    /// Settings service for octo config.
    pub settings_octo: Option<Arc<SettingsService>>,
    /// Settings service for mmry config.
    pub settings_mmry: Option<Arc<SettingsService>>,
    /// Main Chat service for persistent assistants.
    pub main_chat: Option<Arc<MainChatService>>,
    /// Main Chat Pi service for managing Pi subprocesses.
    pub main_chat_pi: Option<Arc<MainChatPiService>>,
    /// WebSocket hub for real-time communication.
    pub ws_hub: Arc<WsHub>,
    /// Pending A2UI blocking requests (request_id -> response channel).
    pub pending_a2ui_requests: PendingA2uiRequests,
    /// Max proxy body size (bytes) for buffered proxy requests.
    pub max_proxy_body_bytes: usize,
}

impl AppState {
    /// Create new application state.
    pub fn new(
        sessions: SessionService,
        agents: AgentService,
        users: UserService,
        invites: InviteCodeRepository,
        auth: AuthState,
        mmry: MmryState,
        voice: VoiceState,
        session_ui: SessionUiState,
        templates: TemplatesState,
        max_proxy_body_bytes: usize,
    ) -> Self {
        let http_client: Client<HttpConnector, Body> =
            Client::builder(TokioExecutor::new()).build_http();

        Self {
            sessions: Arc::new(sessions),
            agents: Arc::new(agents),
            users: Arc::new(users),
            invites: Arc::new(invites),
            auth,
            http_client,
            agent_backend: None,
            mmry,
            voice,
            session_ui,
            templates,
            settings_octo: None,
            settings_mmry: None,
            main_chat: None,
            main_chat_pi: None,
            ws_hub: Arc::new(WsHub::new()),
            pending_a2ui_requests: super::a2ui::new_pending_requests(),
            max_proxy_body_bytes,
        }
    }

    /// Create new application state with AgentBackend.
    pub fn with_agent_backend(
        sessions: SessionService,
        agents: AgentService,
        users: UserService,
        invites: InviteCodeRepository,
        auth: AuthState,
        backend: Arc<dyn AgentBackend>,
        mmry: MmryState,
        voice: VoiceState,
        session_ui: SessionUiState,
        templates: TemplatesState,
        max_proxy_body_bytes: usize,
    ) -> Self {
        let http_client: Client<HttpConnector, Body> =
            Client::builder(TokioExecutor::new()).build_http();

        Self {
            sessions: Arc::new(sessions),
            agents: Arc::new(agents),
            users: Arc::new(users),
            invites: Arc::new(invites),
            auth,
            http_client,
            agent_backend: Some(backend),
            mmry,
            voice,
            session_ui,
            templates,
            settings_octo: None,
            settings_mmry: None,
            main_chat: None,
            main_chat_pi: None,
            ws_hub: Arc::new(WsHub::new()),
            pending_a2ui_requests: super::a2ui::new_pending_requests(),
            max_proxy_body_bytes,
        }
    }

    /// Set the octo settings service.
    pub fn with_settings_octo(mut self, service: SettingsService) -> Self {
        self.settings_octo = Some(Arc::new(service));
        self
    }

    /// Set the mmry settings service.
    pub fn with_settings_mmry(mut self, service: SettingsService) -> Self {
        self.settings_mmry = Some(Arc::new(service));
        self
    }

    /// Set the main chat service.
    pub fn with_main_chat(mut self, service: MainChatService) -> Self {
        self.main_chat = Some(Arc::new(service));
        self
    }

    /// Set the main chat Pi service.
    pub fn with_main_chat_pi(mut self, service: MainChatPiService) -> Self {
        self.main_chat_pi = Some(Arc::new(service));
        self
    }

    /// Set the main chat Pi service from an existing Arc.
    pub fn with_main_chat_pi_arc(mut self, service: Arc<MainChatPiService>) -> Self {
        self.main_chat_pi = Some(service);
        self
    }
}
