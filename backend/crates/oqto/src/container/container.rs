//! Container types and configuration.

use serde::{Deserialize, Deserializer, Serialize};
use std::collections::HashMap;

use super::error::{ContainerError, ContainerResult};

/// Deserialize a field that can be either a string or an integer (Unix timestamp).
/// Converts integers to string representation.
fn deserialize_string_or_int<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    use serde::de::{self, Visitor};
    use std::fmt;

    struct StringOrInt;

    impl<'de> Visitor<'de> for StringOrInt {
        type Value = String;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("a string or an integer")
        }

        fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(value.to_string())
        }

        fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(value)
        }

        fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(value.to_string())
        }

        fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(value.to_string())
        }
    }

    deserializer.deserialize_any(StringOrInt)
}

/// Default empty string for optional fields.
fn default_empty_string() -> String {
    String::new()
}

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
    /// Port mappings (ignored when network_mode is "host").
    pub ports: Vec<PortMapping>,
    /// Volume mounts (host_path -> container_path).
    pub volumes: Vec<(String, String)>,
    /// Working directory inside the container.
    pub workdir: Option<String>,
    /// Labels for the container.
    #[allow(dead_code)]
    pub labels: HashMap<String, String>,
    /// Network mode (e.g., "host", "bridge", "none").
    pub network_mode: Option<String>,
}

impl ContainerConfig {
    /// Create a new container config with the given image.
    pub fn new(image: impl Into<String>) -> Self {
        Self {
            image: image.into(),
            ..Default::default()
        }
    }

    /// Validate all container configuration fields.
    ///
    /// This should be called before creating a container to ensure all inputs
    /// are safe and well-formed.
    pub fn validate(&self) -> ContainerResult<()> {
        // Validate image name
        validate_image_name(&self.image)?;

        // Validate container name if provided
        if let Some(ref name) = self.name {
            validate_container_name(name)?;
        }

        // Validate hostname if provided
        if let Some(ref hostname) = self.hostname {
            validate_hostname(hostname)?;
        }

        // Validate environment variable keys
        for key in self.env.keys() {
            validate_env_var_key(key)?;
        }

        // Validate volume paths
        for (host_path, container_path) in &self.volumes {
            validate_volume_path(host_path, "host")?;
            validate_volume_path(container_path, "container")?;
        }

        // Validate working directory if provided
        if let Some(ref workdir) = self.workdir {
            validate_container_path(workdir)?;
        }

        Ok(())
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
    pub fn volume(
        mut self,
        host_path: impl Into<String>,
        container_path: impl Into<String>,
    ) -> Self {
        self.volumes.push((host_path.into(), container_path.into()));
        self
    }

    /// Set the working directory.
    #[allow(dead_code)]
    pub fn workdir(mut self, workdir: impl Into<String>) -> Self {
        self.workdir = Some(workdir.into());
        self
    }

    /// Set the network mode (e.g., "host", "bridge", "none").
    #[allow(dead_code)]
    pub fn network_mode(mut self, mode: impl Into<String>) -> Self {
        self.network_mode = Some(mode.into());
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

    /// Creation timestamp (can be string or Unix timestamp integer from podman).
    #[serde(
        default = "default_empty_string",
        deserialize_with = "deserialize_string_or_int"
    )]
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

// ============================================================================
// Input Validation Functions
// ============================================================================

/// Validate a Docker/OCI image name.
///
/// Image names follow the pattern: `[registry/][namespace/]name[:tag][@digest]`
/// Valid characters: alphanumeric, `.`, `-`, `_`, `/`, `:`, `@`
///
/// Examples:
/// - `ubuntu:latest`
/// - `myregistry.io/myimage:v1.0`
/// - `library/nginx`
pub fn validate_image_name(image: &str) -> ContainerResult<()> {
    if image.is_empty() {
        return Err(ContainerError::InvalidInput(
            "image name cannot be empty".to_string(),
        ));
    }

    if image.len() > 256 {
        return Err(ContainerError::InvalidInput(
            "image name exceeds maximum length of 256 characters".to_string(),
        ));
    }

    // Check for valid characters
    let valid_chars = |c: char| {
        c.is_ascii_alphanumeric()
            || c == '.'
            || c == '-'
            || c == '_'
            || c == '/'
            || c == ':'
            || c == '@'
    };

    if !image.chars().all(valid_chars) {
        return Err(ContainerError::InvalidInput(format!(
            "image name '{}' contains invalid characters; only alphanumeric, '.', '-', '_', '/', ':', '@' are allowed",
            image
        )));
    }

    // Check for dangerous patterns
    if image.contains("..") {
        return Err(ContainerError::InvalidInput(
            "image name cannot contain '..'".to_string(),
        ));
    }

    Ok(())
}

/// Validate a container name.
///
/// Container names must be alphanumeric with hyphens and underscores.
/// They must start with a letter or underscore.
fn validate_container_name(name: &str) -> ContainerResult<()> {
    if name.is_empty() {
        return Err(ContainerError::InvalidInput(
            "container name cannot be empty".to_string(),
        ));
    }

    if name.len() > 128 {
        return Err(ContainerError::InvalidInput(
            "container name exceeds maximum length of 128 characters".to_string(),
        ));
    }

    // Must start with alphanumeric or underscore
    let first_char = name.chars().next().unwrap();
    if !first_char.is_ascii_alphanumeric() && first_char != '_' {
        return Err(ContainerError::InvalidInput(
            "container name must start with an alphanumeric character or underscore".to_string(),
        ));
    }

    // Only alphanumeric, hyphens, underscores
    let valid_chars = |c: char| c.is_ascii_alphanumeric() || c == '-' || c == '_';
    if !name.chars().all(valid_chars) {
        return Err(ContainerError::InvalidInput(format!(
            "container name '{}' contains invalid characters; only alphanumeric, '-', '_' are allowed",
            name
        )));
    }

    Ok(())
}

/// Validate a hostname.
///
/// Hostnames follow RFC 1123: alphanumeric with hyphens, max 63 chars per label.
fn validate_hostname(hostname: &str) -> ContainerResult<()> {
    if hostname.is_empty() {
        return Err(ContainerError::InvalidInput(
            "hostname cannot be empty".to_string(),
        ));
    }

    if hostname.len() > 253 {
        return Err(ContainerError::InvalidInput(
            "hostname exceeds maximum length of 253 characters".to_string(),
        ));
    }

    // Check each label
    for label in hostname.split('.') {
        if label.is_empty() {
            return Err(ContainerError::InvalidInput(
                "hostname cannot have empty labels".to_string(),
            ));
        }

        if label.len() > 63 {
            return Err(ContainerError::InvalidInput(
                "hostname label exceeds maximum length of 63 characters".to_string(),
            ));
        }

        // Must start and end with alphanumeric
        let first = label.chars().next().unwrap();
        let last = label.chars().last().unwrap();
        if !first.is_ascii_alphanumeric() || !last.is_ascii_alphanumeric() {
            return Err(ContainerError::InvalidInput(
                "hostname labels must start and end with alphanumeric characters".to_string(),
            ));
        }

        // Only alphanumeric and hyphens
        let valid_chars = |c: char| c.is_ascii_alphanumeric() || c == '-';
        if !label.chars().all(valid_chars) {
            return Err(ContainerError::InvalidInput(format!(
                "hostname '{}' contains invalid characters",
                hostname
            )));
        }
    }

    Ok(())
}

/// Validate an environment variable key.
///
/// Environment variable names should follow POSIX conventions:
/// alphanumeric and underscores, starting with a letter or underscore.
fn validate_env_var_key(key: &str) -> ContainerResult<()> {
    if key.is_empty() {
        return Err(ContainerError::InvalidInput(
            "environment variable key cannot be empty".to_string(),
        ));
    }

    if key.len() > 256 {
        return Err(ContainerError::InvalidInput(
            "environment variable key exceeds maximum length of 256 characters".to_string(),
        ));
    }

    // Must start with letter or underscore
    let first_char = key.chars().next().unwrap();
    if !first_char.is_ascii_alphabetic() && first_char != '_' {
        return Err(ContainerError::InvalidInput(format!(
            "environment variable key '{}' must start with a letter or underscore",
            key
        )));
    }

    // Only alphanumeric and underscores
    let valid_chars = |c: char| c.is_ascii_alphanumeric() || c == '_';
    if !key.chars().all(valid_chars) {
        return Err(ContainerError::InvalidInput(format!(
            "environment variable key '{}' contains invalid characters; only alphanumeric and '_' are allowed",
            key
        )));
    }

    Ok(())
}

/// Validate a volume path (host or container side).
fn validate_volume_path(path: &str, side: &str) -> ContainerResult<()> {
    if path.is_empty() {
        return Err(ContainerError::InvalidInput(format!(
            "{} volume path cannot be empty",
            side
        )));
    }

    if path.len() > 4096 {
        return Err(ContainerError::InvalidInput(format!(
            "{} volume path exceeds maximum length of 4096 characters",
            side
        )));
    }

    // Check for null bytes
    if path.contains('\0') {
        return Err(ContainerError::InvalidInput(format!(
            "{} volume path cannot contain null bytes",
            side
        )));
    }

    // Check for dangerous shell metacharacters
    let dangerous_chars = [
        '$', '`', '!', '&', '|', ';', '<', '>', '(', ')', '{', '}', '[', ']', '*', '?', '\\', '"',
        '\'', '\n', '\r',
    ];
    for c in dangerous_chars.iter() {
        if path.contains(*c) {
            return Err(ContainerError::InvalidInput(format!(
                "{} volume path contains dangerous character '{}'",
                side, c
            )));
        }
    }

    Ok(())
}

/// Validate a container-internal path.
fn validate_container_path(path: &str) -> ContainerResult<()> {
    if path.is_empty() {
        return Err(ContainerError::InvalidInput(
            "container path cannot be empty".to_string(),
        ));
    }

    // Must be absolute
    if !path.starts_with('/') {
        return Err(ContainerError::InvalidInput(
            "container path must be absolute (start with '/')".to_string(),
        ));
    }

    // Check for null bytes
    if path.contains('\0') {
        return Err(ContainerError::InvalidInput(
            "container path cannot contain null bytes".to_string(),
        ));
    }

    Ok(())
}

#[cfg(test)]
mod validation_tests {
    use super::*;

    #[test]
    fn test_validate_image_name_valid() {
        assert!(validate_image_name("ubuntu").is_ok());
        assert!(validate_image_name("ubuntu:latest").is_ok());
        assert!(validate_image_name("ubuntu:20.04").is_ok());
        assert!(validate_image_name("library/nginx").is_ok());
        assert!(validate_image_name("myregistry.io/myimage:v1.0").is_ok());
        assert!(validate_image_name("gcr.io/project/image@sha256:abc123").is_ok());
        assert!(validate_image_name("my-image_v1").is_ok());
    }

    #[test]
    fn test_validate_image_name_invalid() {
        assert!(validate_image_name("").is_err());
        assert!(validate_image_name("image with spaces").is_err());
        assert!(validate_image_name("image;rm -rf /").is_err());
        assert!(validate_image_name("image$(whoami)").is_err());
        assert!(validate_image_name("image`id`").is_err());
        assert!(validate_image_name("../../../etc/passwd").is_err());
    }

    #[test]
    fn test_validate_container_name_valid() {
        assert!(validate_container_name("mycontainer").is_ok());
        assert!(validate_container_name("my-container").is_ok());
        assert!(validate_container_name("my_container").is_ok());
        assert!(validate_container_name("container123").is_ok());
        assert!(validate_container_name("_private").is_ok());
    }

    #[test]
    fn test_validate_container_name_invalid() {
        assert!(validate_container_name("").is_err());
        assert!(validate_container_name("-starts-with-dash").is_err());
        assert!(validate_container_name("contains spaces").is_err());
        assert!(validate_container_name("has;semicolon").is_err());
        assert!(validate_container_name("$(whoami)").is_err());
    }

    #[test]
    fn test_validate_hostname_valid() {
        assert!(validate_hostname("localhost").is_ok());
        assert!(validate_hostname("my-host").is_ok());
        assert!(validate_hostname("host.example.com").is_ok());
        assert!(validate_hostname("sub1.sub2.example.com").is_ok());
    }

    #[test]
    fn test_validate_hostname_invalid() {
        assert!(validate_hostname("").is_err());
        assert!(validate_hostname("-invalid").is_err());
        assert!(validate_hostname("invalid-").is_err());
        assert!(validate_hostname("has spaces").is_err());
        assert!(validate_hostname("..").is_err());
    }

    #[test]
    fn test_validate_env_var_key_valid() {
        assert!(validate_env_var_key("PATH").is_ok());
        assert!(validate_env_var_key("MY_VAR").is_ok());
        assert!(validate_env_var_key("_PRIVATE").is_ok());
        assert!(validate_env_var_key("VAR123").is_ok());
    }

    #[test]
    fn test_validate_env_var_key_invalid() {
        assert!(validate_env_var_key("").is_err());
        assert!(validate_env_var_key("123VAR").is_err());
        assert!(validate_env_var_key("MY-VAR").is_err());
        assert!(validate_env_var_key("MY VAR").is_err());
        assert!(validate_env_var_key("$(whoami)").is_err());
    }

    #[test]
    fn test_validate_volume_path_valid() {
        assert!(validate_volume_path("/home/user/data", "host").is_ok());
        assert!(validate_volume_path("./relative/path", "host").is_ok());
        assert!(validate_volume_path("/var/lib/data", "container").is_ok());
    }

    #[test]
    fn test_validate_volume_path_invalid() {
        assert!(validate_volume_path("", "host").is_err());
        assert!(validate_volume_path("/path;rm -rf /", "host").is_err());
        assert!(validate_volume_path("/path$(whoami)", "host").is_err());
        assert!(validate_volume_path("/path`id`", "host").is_err());
        assert!(validate_volume_path("/path\0null", "host").is_err());
    }

    #[test]
    fn test_validate_container_path_valid() {
        assert!(validate_container_path("/home/user").is_ok());
        assert!(validate_container_path("/var/lib/data").is_ok());
        assert!(validate_container_path("/").is_ok());
    }

    #[test]
    fn test_validate_container_path_invalid() {
        assert!(validate_container_path("").is_err());
        assert!(validate_container_path("relative/path").is_err());
        assert!(validate_container_path("/path\0null").is_err());
    }

    #[test]
    fn test_container_config_validate() {
        let config = ContainerConfig::new("ubuntu:latest")
            .name("my-container")
            .hostname("myhost")
            .env("MY_VAR", "value")
            .volume("/host/path", "/container/path");

        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_container_config_validate_invalid_image() {
        let config = ContainerConfig::new("invalid$(whoami)");
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_container_config_validate_invalid_name() {
        let config = ContainerConfig::new("ubuntu").name("invalid;name");
        assert!(config.validate().is_err());
    }
}
