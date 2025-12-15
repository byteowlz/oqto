//! Container types and configuration.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Port mapping configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortMapping {
    /// Port on the host.
    pub host_port: u16,
    /// Port in the container.
    pub container_port: u16,
    /// Protocol (tcp or udp).
    #[serde(default = "default_protocol")]
    pub protocol: String,
}

fn default_protocol() -> String {
    "tcp".to_string()
}

impl PortMapping {
    /// Create a new port mapping.
    pub fn new(host_port: u16, container_port: u16) -> Self {
        Self {
            host_port,
            container_port,
            protocol: default_protocol(),
        }
    }

    /// Create a UDP port mapping.
    #[allow(dead_code)]
    pub fn udp(host_port: u16, container_port: u16) -> Self {
        Self {
            host_port,
            container_port,
            protocol: "udp".to_string(),
        }
    }
}

/// Configuration for creating a new container.
#[derive(Debug, Clone, Default)]
pub struct ContainerConfig {
    /// Container name (optional).
    pub name: Option<String>,
    /// Container hostname.
    pub hostname: Option<String>,
    /// Docker/OCI image to use.
    pub image: String,
    /// Command to run.
    pub command: Vec<String>,
    /// Environment variables.
    pub env: HashMap<String, String>,
    /// Port mappings.
    pub ports: Vec<PortMapping>,
    /// Volume mounts (host_path -> container_path).
    pub volumes: Vec<(String, String)>,
    /// Working directory inside the container.
    pub workdir: Option<String>,
    /// Labels for the container.
    #[allow(dead_code)]
    pub labels: HashMap<String, String>,
}

impl ContainerConfig {
    /// Create a new container config with the given image.
    pub fn new(image: impl Into<String>) -> Self {
        Self {
            image: image.into(),
            ..Default::default()
        }
    }

    /// Set the container name.
    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// Set the container hostname.
    pub fn hostname(mut self, hostname: impl Into<String>) -> Self {
        self.hostname = Some(hostname.into());
        self
    }

    /// Set the command to run.
    #[allow(dead_code)]
    pub fn command(mut self, cmd: Vec<String>) -> Self {
        self.command = cmd;
        self
    }

    /// Add an environment variable.
    pub fn env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.env.insert(key.into(), value.into());
        self
    }

    /// Add multiple environment variables.
    #[allow(dead_code)]
    pub fn envs(mut self, envs: HashMap<String, String>) -> Self {
        self.env.extend(envs);
        self
    }

    /// Add a port mapping.
    pub fn port(mut self, host_port: u16, container_port: u16) -> Self {
        self.ports.push(PortMapping::new(host_port, container_port));
        self
    }

    /// Add a volume mount.
    pub fn volume(mut self, host_path: impl Into<String>, container_path: impl Into<String>) -> Self {
        self.volumes.push((host_path.into(), container_path.into()));
        self
    }

    /// Set the working directory.
    #[allow(dead_code)]
    pub fn workdir(mut self, workdir: impl Into<String>) -> Self {
        self.workdir = Some(workdir.into());
        self
    }

    /// Add a label.
    #[allow(dead_code)]
    pub fn label(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.labels.insert(key.into(), value.into());
        self
    }
}

/// Container state.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[allow(dead_code)]
pub enum ContainerState {
    /// Container is being created.
    Created,
    /// Container is running.
    Running,
    /// Container is paused.
    Paused,
    /// Container is restarting.
    Restarting,
    /// Container is being removed.
    Removing,
    /// Container has exited.
    Exited,
    /// Container is dead.
    Dead,
    /// Unknown state.
    #[default]
    #[serde(other)]
    Unknown,
}

impl std::fmt::Display for ContainerState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ContainerState::Created => write!(f, "created"),
            ContainerState::Running => write!(f, "running"),
            ContainerState::Paused => write!(f, "paused"),
            ContainerState::Restarting => write!(f, "restarting"),
            ContainerState::Removing => write!(f, "removing"),
            ContainerState::Exited => write!(f, "exited"),
            ContainerState::Dead => write!(f, "dead"),
            ContainerState::Unknown => write!(f, "unknown"),
        }
    }
}

/// Container information from podman ps/inspect.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
#[allow(dead_code)]
pub struct Container {
    /// Container ID.
    #[serde(alias = "Id")]
    pub id: String,

    /// Container names.
    #[serde(default)]
    pub names: Vec<String>,

    /// Image used.
    #[serde(default)]
    pub image: String,

    /// Container state.
    #[serde(default)]
    pub state: ContainerState,

    /// Status string (e.g., "Up 5 minutes").
    #[serde(default)]
    pub status: String,

    /// Creation timestamp.
    #[serde(default)]
    pub created: String,

    /// Port bindings.
    #[serde(default)]
    pub ports: Vec<ContainerPort>,
}

/// Port binding information.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct ContainerPort {
    /// Host IP.
    #[serde(default, rename = "hostIP")]
    pub host_ip: String,
    /// Host port.
    #[serde(default, rename = "hostPort")]
    pub host_port: u16,
    /// Container port.
    #[serde(default, rename = "containerPort")]
    pub container_port: u16,
    /// Protocol.
    #[serde(default)]
    pub protocol: String,
}

/// Container resource statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
#[allow(dead_code)]
pub struct ContainerStats {
    /// Container ID.
    #[serde(alias = "ContainerID", alias = "Container")]
    pub container_id: String,

    /// Container name.
    #[serde(default, alias = "Name")]
    pub name: String,

    /// CPU percentage.
    #[serde(default, alias = "CPUPerc", alias = "CPU")]
    pub cpu_percent: String,

    /// Memory usage.
    #[serde(default, alias = "MemUsage", alias = "MemUsageBytes")]
    pub mem_usage: String,

    /// Memory percentage.
    #[serde(default, alias = "MemPerc", alias = "Mem")]
    pub mem_percent: String,

    /// Network I/O.
    #[serde(default, alias = "NetIO")]
    pub net_io: String,

    /// Block I/O.
    #[serde(default, alias = "BlockIO")]
    pub block_io: String,

    /// Number of PIDs.
    #[serde(default, alias = "PIDs")]
    pub pids: String,
}
