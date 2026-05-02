use std::path::PathBuf;

/// Base directory shared with agent-browser socket/session files.
pub fn agent_browser_base_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("AGENT_BROWSER_SOCKET_DIR_BASE") {
        return PathBuf::from(dir);
    }
    if let Ok(state_dir) = std::env::var("XDG_STATE_HOME") {
        return PathBuf::from(state_dir).join("oqto").join("agent-browser");
    }
    if let Some(home) = dirs::home_dir() {
        return home
            .join(".local")
            .join("state")
            .join("oqto")
            .join("agent-browser");
    }
    std::env::temp_dir().join("oqto").join("agent-browser")
}

/// Compute a short, deterministic agent-browser session name from a chat session ID.
pub fn browser_session_name(chat_session_id: &str) -> String {
    const NAMESPACE_BYTES: [u8; 16] = [
        0x8b, 0x3a, 0x8f, 0x51, 0x90, 0x4c, 0x4a, 0x09, 0x97, 0x7c, 0x83, 0x37, 0x9f, 0x7a, 0x21,
        0x59,
    ];
    let namespace = uuid::Uuid::from_bytes(NAMESPACE_BYTES);
    let uuid = uuid::Uuid::new_v5(&namespace, chat_session_id.as_bytes());
    let simple = uuid.simple().to_string();
    format!("ab-{}", &simple[..16])
}

/// Resolve the agent-browser socket directory for a session.
pub fn agent_browser_session_dir(session_id: &str, override_dir: Option<&str>) -> PathBuf {
    if let Some(dir) = override_dir {
        return PathBuf::from(dir);
    }
    agent_browser_base_dir().join(session_id)
}
