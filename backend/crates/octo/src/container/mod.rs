//! Container runtime management module.
//!
//! Provides an async interface to manage containers via Docker or Podman CLI.
//! The runtime is auto-detected or can be configured explicitly.

mod container;
mod error;

#[allow(unused_imports)]
pub use container::PortMapping;
pub use container::{Container, ContainerConfig, ContainerStats};
pub use error::{ContainerError, ContainerResult};

// Re-export validation function for use in this module
use container::validate_image_name;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::process::Stdio;
use tokio::process::Command;

/// Container runtime type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RuntimeType {
    /// Docker runtime (default for macOS/Windows dev)
    Docker,
    /// Podman runtime (default for Linux prod)
    #[default]
    Podman,
}

impl RuntimeType {
    /// Get the default binary name for this runtime.
    pub fn default_binary(&self) -> &'static str {
        match self {
            RuntimeType::Docker => "docker",
            RuntimeType::Podman => "podman",
        }
    }

    /// Whether this runtime requires SELinux volume labels (:Z suffix).
    pub fn needs_selinux_labels(&self) -> bool {
        match self {
            RuntimeType::Docker => false,
            RuntimeType::Podman => true,
        }
    }
}

impl std::fmt::Display for RuntimeType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RuntimeType::Docker => write!(f, "docker"),
            RuntimeType::Podman => write!(f, "podman"),
        }
    }
}

/// Validate a container ID or name.
///
/// Container IDs are hex strings (12 or 64 chars for docker/podman).
/// Container names follow the same rules as container creation.
fn validate_container_id_or_name(id: &str) -> ContainerResult<()> {
    if id.is_empty() {
        return Err(ContainerError::InvalidInput(
            "container ID or name cannot be empty".to_string(),
        ));
    }

    if id.len() > 128 {
        return Err(ContainerError::InvalidInput(
            "container ID or name exceeds maximum length".to_string(),
        ));
    }

    // Container IDs are hex, container names are alphanumeric with - and _
    let valid_chars = |c: char| c.is_ascii_alphanumeric() || c == '-' || c == '_';
    if !id.chars().all(valid_chars) {
        return Err(ContainerError::InvalidInput(format!(
            "container ID or name '{}' contains invalid characters",
            id
        )));
    }

    Ok(())
}

/// Container runtime client for managing containers.
///
/// Supports both Docker and Podman with automatic detection.
#[derive(Debug, Clone)]
pub struct ContainerRuntime {
    /// The runtime type (docker or podman)
    runtime_type: RuntimeType,
    /// Path to the container binary
    binary: String,
}

/// Container runtime abstraction for testability.
#[async_trait]
pub trait ContainerRuntimeApi: Send + Sync {
    async fn create_container(&self, config: &ContainerConfig) -> ContainerResult<String>;
    async fn stop_container(
        &self,
        container_id: &str,
        timeout_seconds: Option<u32>,
    ) -> ContainerResult<()>;
    async fn start_container(&self, container_id: &str) -> ContainerResult<()>;
    async fn remove_container(&self, container_id: &str, force: bool) -> ContainerResult<()>;
    async fn list_containers(&self, all: bool) -> ContainerResult<Vec<Container>>;
    async fn container_state_status(&self, id_or_name: &str) -> ContainerResult<Option<String>>;
    async fn get_image_digest(&self, image: &str) -> ContainerResult<Option<String>>;
    async fn get_stats(&self, container_id: &str) -> ContainerResult<ContainerStats>;

    /// Execute a command in a container (detached, fire-and-forget).
    async fn exec_detached(&self, container_id: &str, command: &[&str]) -> ContainerResult<()>;

    /// Execute a command in a container and return the output.
    async fn exec_output(&self, container_id: &str, command: &[&str]) -> ContainerResult<String>;
}

#[async_trait]
impl ContainerRuntimeApi for ContainerRuntime {
    async fn create_container(&self, config: &ContainerConfig) -> ContainerResult<String> {
        self.create_container(config).await
    }

    async fn stop_container(
        &self,
        container_id: &str,
        timeout_seconds: Option<u32>,
    ) -> ContainerResult<()> {
        self.stop_container(container_id, timeout_seconds).await
    }

    async fn start_container(&self, container_id: &str) -> ContainerResult<()> {
        self.start_container(container_id).await
    }

    async fn remove_container(&self, container_id: &str, force: bool) -> ContainerResult<()> {
        self.remove_container(container_id, force).await
    }

    async fn list_containers(&self, all: bool) -> ContainerResult<Vec<Container>> {
        self.list_containers(all).await
    }

    async fn container_state_status(&self, id_or_name: &str) -> ContainerResult<Option<String>> {
        self.container_state_status(id_or_name).await
    }

    async fn get_image_digest(&self, image: &str) -> ContainerResult<Option<String>> {
        self.get_image_digest(image).await
    }

    async fn get_stats(&self, container_id: &str) -> ContainerResult<ContainerStats> {
        self.get_stats(container_id).await
    }

    async fn exec_detached(&self, container_id: &str, command: &[&str]) -> ContainerResult<()> {
        self.exec_detached(container_id, command).await
    }

    async fn exec_output(&self, container_id: &str, command: &[&str]) -> ContainerResult<String> {
        self.exec_output(container_id, command).await
    }
}

impl Default for ContainerRuntime {
    fn default() -> Self {
        Self::new()
    }
}

impl ContainerRuntime {
    /// Create a new container runtime with auto-detection.
    ///
    /// Tries Docker first (for macOS dev), then falls back to Podman.
    pub fn new() -> Self {
        // Try to detect which runtime is available
        // Prefer Docker on macOS (dev environment)
        #[cfg(target_os = "macos")]
        {
            if Self::is_binary_available("docker") {
                return Self {
                    runtime_type: RuntimeType::Docker,
                    binary: "docker".to_string(),
                };
            }
        }

        // Default to Podman on Linux or if Docker isn't available
        if Self::is_binary_available("podman") {
            Self {
                runtime_type: RuntimeType::Podman,
                binary: "podman".to_string(),
            }
        } else if Self::is_binary_available("docker") {
            Self {
                runtime_type: RuntimeType::Docker,
                binary: "docker".to_string(),
            }
        } else {
            // Fall back to podman, will fail at runtime
            Self {
                runtime_type: RuntimeType::Podman,
                binary: "podman".to_string(),
            }
        }
    }

    /// Create a container runtime with a specific type.
    pub fn with_type(runtime_type: RuntimeType) -> Self {
        Self {
            binary: runtime_type.default_binary().to_string(),
            runtime_type,
        }
    }

    /// Create a container runtime with a custom binary path.
    #[allow(dead_code)]
    pub fn with_binary(runtime_type: RuntimeType, binary: impl Into<String>) -> Self {
        Self {
            runtime_type,
            binary: binary.into(),
        }
    }

    /// Get the runtime type.
    pub fn runtime_type(&self) -> RuntimeType {
        self.runtime_type
    }

    /// Check if a binary is available in PATH.
    fn is_binary_available(name: &str) -> bool {
        std::process::Command::new("which")
            .arg(name)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }

    /// Check if the container runtime is available and working.
    pub async fn health_check(&self) -> ContainerResult<String> {
        let output = Command::new(&self.binary)
            .args(["version", "--format", "json"])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|e| ContainerError::CommandFailed {
                command: "version".to_string(),
                message: e.to_string(),
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(ContainerError::CommandFailed {
                command: "version".to_string(),
                message: stderr.to_string(),
            });
        }

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    /// Create and start a new container.
    ///
    /// The configuration is validated before creating the container to prevent
    /// injection attacks and ensure all inputs are well-formed.
    pub async fn create_container(&self, config: &ContainerConfig) -> ContainerResult<String> {
        // Validate all inputs before creating the container
        config.validate()?;

        let mut owned_args: Vec<String> = Vec::new();

        owned_args.push("run".to_string());
        owned_args.push("-d".to_string());

        // Container name
        if let Some(ref name) = config.name {
            owned_args.push("--name".to_string());
            owned_args.push(name.clone());
        }

        // Hostname (skip if using host network mode)
        if let Some(ref hostname) = config.hostname
            && config.network_mode.as_deref() != Some("host")
        {
            owned_args.push("--hostname".to_string());
            owned_args.push(hostname.clone());
        }

        // Network mode
        if let Some(ref network_mode) = config.network_mode {
            owned_args.push("--network".to_string());
            owned_args.push(network_mode.clone());
        } else if self.runtime_type == RuntimeType::Podman {
            // For Podman with default pasta networking, set a reasonable MTU.
            // The default pasta MTU of 65520 can cause TLS handshake failures
            // with some CDNs (like Cloudflare/npm) that don't handle large MTUs
            // or Path MTU Discovery properly.
            owned_args.push("--network".to_string());
            owned_args.push("pasta:-m,1500".to_string());
        }

        // Port mappings (skip if using host network mode - ports are directly accessible)
        if config.network_mode.as_deref() != Some("host") {
            for port in &config.ports {
                owned_args.push("-p".to_string());
                owned_args.push(format!("{}:{}", port.host_port, port.container_port));
            }
        }

        // Volume mounts - handle SELinux labels for Podman
        for (host, container) in &config.volumes {
            owned_args.push("-v".to_string());
            if self.runtime_type.needs_selinux_labels() {
                owned_args.push(format!("{}:{}:Z", host, container));
            } else {
                owned_args.push(format!("{}:{}", host, container));
            }
        }

        // Environment variables
        for (key, value) in &config.env {
            owned_args.push("-e".to_string());
            owned_args.push(format!("{}={}", key, value));
        }

        // Working directory
        if let Some(ref workdir) = config.workdir {
            owned_args.push("-w".to_string());
            owned_args.push(workdir.clone());
        }

        // Image
        owned_args.push(config.image.clone());

        // Command
        for cmd in &config.command {
            owned_args.push(cmd.clone());
        }

        let output = Command::new(&self.binary)
            .args(&owned_args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|e| ContainerError::CommandFailed {
                command: "run".to_string(),
                message: e.to_string(),
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(ContainerError::CommandFailed {
                command: "run".to_string(),
                message: stderr.to_string(),
            });
        }

        // Return container ID (trimmed)
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    /// Stop a running container.
    pub async fn stop_container(
        &self,
        container_id: &str,
        timeout: Option<u32>,
    ) -> ContainerResult<()> {
        validate_container_id_or_name(container_id)?;

        let mut owned_args: Vec<String> = vec!["stop".to_string()];

        if let Some(t) = timeout {
            owned_args.push("-t".to_string());
            owned_args.push(t.to_string());
        }

        owned_args.push(container_id.to_string());

        let output = Command::new(&self.binary)
            .args(&owned_args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|e| ContainerError::CommandFailed {
                command: "stop".to_string(),
                message: e.to_string(),
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(ContainerError::CommandFailed {
                command: "stop".to_string(),
                message: stderr.to_string(),
            });
        }

        Ok(())
    }

    /// Start a stopped container.
    pub async fn start_container(&self, container_id: &str) -> ContainerResult<()> {
        validate_container_id_or_name(container_id)?;

        let output = Command::new(&self.binary)
            .args(["start", container_id])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|e| ContainerError::CommandFailed {
                command: "start".to_string(),
                message: e.to_string(),
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(ContainerError::CommandFailed {
                command: "start".to_string(),
                message: stderr.to_string(),
            });
        }

        Ok(())
    }

    /// Remove a container.
    pub async fn remove_container(&self, container_id: &str, force: bool) -> ContainerResult<()> {
        validate_container_id_or_name(container_id)?;

        let mut args = vec!["rm"];

        if force {
            args.push("-f");
        }

        args.push(container_id);

        let output = Command::new(&self.binary)
            .args(&args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|e| ContainerError::CommandFailed {
                command: "rm".to_string(),
                message: e.to_string(),
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(ContainerError::CommandFailed {
                command: "rm".to_string(),
                message: stderr.to_string(),
            });
        }

        Ok(())
    }

    /// List containers.
    #[allow(dead_code)]
    pub async fn list_containers(&self, all: bool) -> ContainerResult<Vec<Container>> {
        let mut args = vec!["ps", "--format", "json"];

        if all {
            args.push("-a");
        }

        let output = Command::new(&self.binary)
            .args(&args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|e| ContainerError::CommandFailed {
                command: "ps".to_string(),
                message: e.to_string(),
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(ContainerError::CommandFailed {
                command: "ps".to_string(),
                message: stderr.to_string(),
            });
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        if stdout.trim().is_empty() {
            return Ok(vec![]);
        }

        let containers: Vec<Container> =
            serde_json::from_str(&stdout).map_err(|e| ContainerError::ParseError(e.to_string()))?;

        Ok(containers)
    }

    /// Get container by ID or name.
    #[allow(dead_code)]
    pub async fn get_container(&self, id_or_name: &str) -> ContainerResult<Option<Container>> {
        validate_container_id_or_name(id_or_name)?;

        let output = Command::new(&self.binary)
            .args(["inspect", "--format", "json", id_or_name])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|e| ContainerError::CommandFailed {
                command: "inspect".to_string(),
                message: e.to_string(),
            })?;

        if !output.status.success() {
            // Container not found is not an error, just return None
            return Ok(None);
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let containers: Vec<Container> =
            serde_json::from_str(&stdout).map_err(|e| ContainerError::ParseError(e.to_string()))?;

        Ok(containers.into_iter().next())
    }

    /// Get the container state status string (e.g. "running", "exited") via `inspect`.
    ///
    /// Returns `Ok(None)` when the container does not exist.
    pub async fn container_state_status(
        &self,
        id_or_name: &str,
    ) -> ContainerResult<Option<String>> {
        validate_container_id_or_name(id_or_name)?;

        let output = Command::new(&self.binary)
            .args(["inspect", "--format", "{{.State.Status}}", id_or_name])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|e| ContainerError::CommandFailed {
                command: "inspect".to_string(),
                message: e.to_string(),
            })?;

        if !output.status.success() {
            // Container not found is not an error; callers treat it as missing.
            return Ok(None);
        }

        let status = String::from_utf8_lossy(&output.stdout)
            .trim()
            .trim_matches('"')
            .to_string();
        if status.is_empty() {
            return Ok(None);
        }

        Ok(Some(status))
    }

    /// Get container logs.
    #[allow(dead_code)]
    pub async fn get_logs(&self, container_id: &str, tail: Option<u32>) -> ContainerResult<String> {
        validate_container_id_or_name(container_id)?;

        let mut owned_args: Vec<String> = vec!["logs".to_string()];

        if let Some(n) = tail {
            owned_args.push("--tail".to_string());
            owned_args.push(n.to_string());
        }

        owned_args.push(container_id.to_string());

        let output = Command::new(&self.binary)
            .args(&owned_args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|e| ContainerError::CommandFailed {
                command: "logs".to_string(),
                message: e.to_string(),
            })?;

        // Logs command outputs to stderr for container stderr
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        Ok(format!("{}{}", stdout, stderr))
    }

    /// Get container stats (single snapshot).
    #[allow(dead_code)]
    pub async fn get_stats(&self, container_id: &str) -> ContainerResult<ContainerStats> {
        validate_container_id_or_name(container_id)?;

        let output = Command::new(&self.binary)
            .args(["stats", "--no-stream", "--format", "json", container_id])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|e| ContainerError::CommandFailed {
                command: "stats".to_string(),
                message: e.to_string(),
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(ContainerError::CommandFailed {
                command: "stats".to_string(),
                message: stderr.to_string(),
            });
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stats: Vec<ContainerStats> =
            serde_json::from_str(&stdout).map_err(|e| ContainerError::ParseError(e.to_string()))?;

        stats
            .into_iter()
            .next()
            .ok_or_else(|| ContainerError::ContainerNotFound(container_id.to_string()))
    }

    /// Check if an image exists locally.
    /// Check if an image exists locally.
    /// Uses `docker image inspect` (works for both Docker and Podman) instead of
    /// `podman image exists` which is Podman-specific.
    pub async fn image_exists(&self, image: &str) -> ContainerResult<bool> {
        validate_image_name(image)?;

        let output = Command::new(&self.binary)
            .args(["image", "inspect", image])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .output()
            .await
            .map_err(|e| ContainerError::CommandFailed {
                command: "image inspect".to_string(),
                message: e.to_string(),
            })?;

        Ok(output.status.success())
    }

    /// Pull an image.
    #[allow(dead_code)]
    pub async fn pull_image(&self, image: &str) -> ContainerResult<()> {
        validate_image_name(image)?;

        let output = Command::new(&self.binary)
            .args(["pull", image])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|e| ContainerError::CommandFailed {
                command: "pull".to_string(),
                message: e.to_string(),
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(ContainerError::CommandFailed {
                command: "pull".to_string(),
                message: stderr.to_string(),
            });
        }

        Ok(())
    }

    /// Get the digest (sha256) for a local image.
    ///
    /// Returns `Ok(None)` if the image does not exist locally.
    pub async fn get_image_digest(&self, image: &str) -> ContainerResult<Option<String>> {
        validate_image_name(image)?;

        // Use inspect to get the image digest
        // Format: {{.Digest}} returns the digest or empty string
        let output = Command::new(&self.binary)
            .args(["image", "inspect", "--format", "{{.Digest}}", image])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|e| ContainerError::CommandFailed {
                command: "image inspect".to_string(),
                message: e.to_string(),
            })?;

        if !output.status.success() {
            // Image not found is not an error, just return None
            return Ok(None);
        }

        let digest = String::from_utf8_lossy(&output.stdout).trim().to_string();

        // Empty string or "<none>" means no digest (local build without push)
        if digest.is_empty() || digest == "<none>" {
            // Fall back to getting the image ID as a pseudo-digest for local images
            return self.get_image_id(image).await;
        }

        Ok(Some(digest))
    }

    /// Get the image ID (sha256 hash) for a local image.
    ///
    /// This is useful for locally built images that don't have a registry digest.
    /// Returns `Ok(None)` if the image does not exist locally.
    pub async fn get_image_id(&self, image: &str) -> ContainerResult<Option<String>> {
        validate_image_name(image)?;

        let output = Command::new(&self.binary)
            .args(["image", "inspect", "--format", "{{.Id}}", image])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|e| ContainerError::CommandFailed {
                command: "image inspect".to_string(),
                message: e.to_string(),
            })?;

        if !output.status.success() {
            return Ok(None);
        }

        let id = String::from_utf8_lossy(&output.stdout).trim().to_string();

        if id.is_empty() {
            return Ok(None);
        }

        Ok(Some(id))
    }

    /// Execute a command in a container (detached, fire-and-forget).
    ///
    /// This runs `docker exec -d` to execute the command in the background.
    /// Useful for starting long-running processes like opencode serve.
    pub async fn exec_detached(&self, container_id: &str, command: &[&str]) -> ContainerResult<()> {
        validate_container_id_or_name(container_id)?;

        let mut args = vec!["exec", "-d", container_id];
        args.extend(command);

        let output = Command::new(&self.binary)
            .args(&args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|e| ContainerError::CommandFailed {
                command: "exec".to_string(),
                message: e.to_string(),
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(ContainerError::CommandFailed {
                command: "exec".to_string(),
                message: stderr.to_string(),
            });
        }

        Ok(())
    }

    /// Execute a command in a container and return the output.
    ///
    /// This runs `docker exec` and waits for the command to complete,
    /// returning stdout as a string.
    pub async fn exec_output(
        &self,
        container_id: &str,
        command: &[&str],
    ) -> ContainerResult<String> {
        validate_container_id_or_name(container_id)?;

        let mut args = vec!["exec", container_id];
        args.extend(command);

        let output = Command::new(&self.binary)
            .args(&args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|e| ContainerError::CommandFailed {
                command: "exec".to_string(),
                message: e.to_string(),
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(ContainerError::CommandFailed {
                command: "exec".to_string(),
                message: stderr.to_string(),
            });
        }

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_container_runtime_health_check() {
        let runtime = ContainerRuntime::new();
        // This test will only pass if docker or podman is installed
        if let Ok(version) = runtime.health_check().await {
            assert!(!version.is_empty());
        }
    }

    #[test]
    fn test_runtime_type_selinux() {
        assert!(!RuntimeType::Docker.needs_selinux_labels());
        assert!(RuntimeType::Podman.needs_selinux_labels());
    }
}
