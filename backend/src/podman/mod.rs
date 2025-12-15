//! Podman container management module.
//!
//! Provides an async interface to manage containers via the podman CLI.

mod container;
mod error;

pub use container::{Container, ContainerConfig, ContainerStats};
#[allow(unused_imports)]
pub use container::PortMapping;
pub use error::{PodmanError, PodmanResult};

use std::process::Stdio;
use tokio::process::Command;

/// Podman client for managing containers.
#[derive(Debug, Clone)]
pub struct Podman {
    /// Path to the podman binary
    binary: String,
}

impl Default for Podman {
    fn default() -> Self {
        Self::new()
    }
}

impl Podman {
    /// Create a new Podman client.
    pub fn new() -> Self {
        Self {
            binary: "podman".to_string(),
        }
    }

    /// Create a new Podman client with a custom binary path.
    #[allow(dead_code)]
    pub fn with_binary(binary: impl Into<String>) -> Self {
        Self {
            binary: binary.into(),
        }
    }

    /// Check if podman is available and working.
    pub async fn health_check(&self) -> PodmanResult<String> {
        let output = Command::new(&self.binary)
            .args(["version", "--format", "json"])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|e| PodmanError::CommandFailed {
                command: "version".to_string(),
                message: e.to_string(),
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(PodmanError::CommandFailed {
                command: "version".to_string(),
                message: stderr.to_string(),
            });
        }

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    /// Create and start a new container.
    pub async fn create_container(&self, config: &ContainerConfig) -> PodmanResult<String> {
        let mut args = vec!["run", "-d"];

        // Container name
        if let Some(ref name) = config.name {
            args.push("--name");
            args.push(name);
        }

        // Hostname
        if let Some(ref hostname) = config.hostname {
            args.push("--hostname");
            args.push(hostname);
        }

        // Port mappings
        for port in &config.ports {
            args.push("-p");
            let port_str = format!("{}:{}", port.host_port, port.container_port);
            args.push(Box::leak(port_str.into_boxed_str()));
        }

        // Volume mounts
        for (host, container) in &config.volumes {
            args.push("-v");
            let vol_str = format!("{}:{}:Z", host, container);
            args.push(Box::leak(vol_str.into_boxed_str()));
        }

        // Environment variables
        for (key, value) in &config.env {
            args.push("-e");
            let env_str = format!("{}={}", key, value);
            args.push(Box::leak(env_str.into_boxed_str()));
        }

        // Working directory
        if let Some(ref workdir) = config.workdir {
            args.push("-w");
            args.push(workdir);
        }

        // Image
        args.push(&config.image);

        // Command
        for cmd in &config.command {
            args.push(cmd);
        }

        let output = Command::new(&self.binary)
            .args(&args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|e| PodmanError::CommandFailed {
                command: "run".to_string(),
                message: e.to_string(),
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(PodmanError::CommandFailed {
                command: "run".to_string(),
                message: stderr.to_string(),
            });
        }

        // Return container ID (trimmed)
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    /// Stop a running container.
    pub async fn stop_container(&self, container_id: &str, timeout: Option<u32>) -> PodmanResult<()> {
        let mut args = vec!["stop"];
        
        if let Some(t) = timeout {
            args.push("-t");
            let timeout_str = t.to_string();
            args.push(Box::leak(timeout_str.into_boxed_str()));
        }
        
        args.push(container_id);

        let output = Command::new(&self.binary)
            .args(&args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|e| PodmanError::CommandFailed {
                command: "stop".to_string(),
                message: e.to_string(),
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(PodmanError::CommandFailed {
                command: "stop".to_string(),
                message: stderr.to_string(),
            });
        }

        Ok(())
    }

    /// Remove a container.
    pub async fn remove_container(&self, container_id: &str, force: bool) -> PodmanResult<()> {
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
            .map_err(|e| PodmanError::CommandFailed {
                command: "rm".to_string(),
                message: e.to_string(),
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(PodmanError::CommandFailed {
                command: "rm".to_string(),
                message: stderr.to_string(),
            });
        }

        Ok(())
    }

    /// List containers.
    #[allow(dead_code)]
    pub async fn list_containers(&self, all: bool) -> PodmanResult<Vec<Container>> {
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
            .map_err(|e| PodmanError::CommandFailed {
                command: "ps".to_string(),
                message: e.to_string(),
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(PodmanError::CommandFailed {
                command: "ps".to_string(),
                message: stderr.to_string(),
            });
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        if stdout.trim().is_empty() {
            return Ok(vec![]);
        }

        let containers: Vec<Container> = serde_json::from_str(&stdout)
            .map_err(|e| PodmanError::ParseError(e.to_string()))?;

        Ok(containers)
    }

    /// Get container by ID or name.
    #[allow(dead_code)]
    pub async fn get_container(&self, id_or_name: &str) -> PodmanResult<Option<Container>> {
        let output = Command::new(&self.binary)
            .args(["inspect", "--format", "json", id_or_name])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|e| PodmanError::CommandFailed {
                command: "inspect".to_string(),
                message: e.to_string(),
            })?;

        if !output.status.success() {
            // Container not found is not an error, just return None
            return Ok(None);
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let containers: Vec<Container> = serde_json::from_str(&stdout)
            .map_err(|e| PodmanError::ParseError(e.to_string()))?;

        Ok(containers.into_iter().next())
    }

    /// Get container logs.
    #[allow(dead_code)]
    pub async fn get_logs(&self, container_id: &str, tail: Option<u32>) -> PodmanResult<String> {
        let mut args = vec!["logs"];
        
        if let Some(n) = tail {
            args.push("--tail");
            let tail_str = n.to_string();
            args.push(Box::leak(tail_str.into_boxed_str()));
        }
        
        args.push(container_id);

        let output = Command::new(&self.binary)
            .args(&args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|e| PodmanError::CommandFailed {
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
    pub async fn get_stats(&self, container_id: &str) -> PodmanResult<ContainerStats> {
        let output = Command::new(&self.binary)
            .args(["stats", "--no-stream", "--format", "json", container_id])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|e| PodmanError::CommandFailed {
                command: "stats".to_string(),
                message: e.to_string(),
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(PodmanError::CommandFailed {
                command: "stats".to_string(),
                message: stderr.to_string(),
            });
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stats: Vec<ContainerStats> = serde_json::from_str(&stdout)
            .map_err(|e| PodmanError::ParseError(e.to_string()))?;

        stats.into_iter().next().ok_or_else(|| {
            PodmanError::ContainerNotFound(container_id.to_string())
        })
    }

    /// Check if an image exists locally.
    #[allow(dead_code)]
    pub async fn image_exists(&self, image: &str) -> PodmanResult<bool> {
        let output = Command::new(&self.binary)
            .args(["image", "exists", image])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|e| PodmanError::CommandFailed {
                command: "image exists".to_string(),
                message: e.to_string(),
            })?;

        Ok(output.status.success())
    }

    /// Pull an image.
    #[allow(dead_code)]
    pub async fn pull_image(&self, image: &str) -> PodmanResult<()> {
        let output = Command::new(&self.binary)
            .args(["pull", image])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|e| PodmanError::CommandFailed {
                command: "pull".to_string(),
                message: e.to_string(),
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(PodmanError::CommandFailed {
                command: "pull".to_string(),
                message: stderr.to_string(),
            });
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_podman_health_check() {
        let podman = Podman::new();
        // This test will only pass if podman is installed
        if let Ok(version) = podman.health_check().await {
            assert!(!version.is_empty());
        }
    }
}
