//! Settings service - config file management with hot-reload.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::{RwLock, watch};

use super::schema::{SettingsScope, filter_schema_by_scope};

/// A settings value with metadata about its source.
#[derive(Debug, Clone, Serialize)]
pub struct SettingsValue {
    /// The current value
    pub value: Value,
    /// Whether this value is explicitly set in config (vs default)
    pub is_configured: bool,
    /// The default value from schema (if any)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<Value>,
}

/// Request to update configuration values.
#[derive(Debug, Clone, Deserialize)]
pub struct ConfigUpdate {
    /// Map of dotted paths to new values (e.g., "voice.default_voice": "af_bella")
    pub values: HashMap<String, Value>,
}

/// Settings file format.
#[derive(Debug, Clone, Copy)]
enum SettingsFormat {
    Toml,
    Json,
}

/// Settings service for managing configuration.
pub struct SettingsService {
    /// Embedded schema for the app
    schema: Value,
    /// Base config directory (e.g., ~/.config/oqto)
    config_dir: PathBuf,
    /// Config filename
    config_filename: String,
    /// Settings file format
    format: SettingsFormat,
    /// Current config values (cached)
    values: Arc<RwLock<Value>>,
    /// Reload notification channel
    reload_tx: watch::Sender<()>,
}

impl SettingsService {
    /// Create a new settings service for TOML configuration files.
    pub fn new(schema: Value, config_dir: PathBuf, config_filename: &str) -> Result<Self> {
        Self::new_with_format(schema, config_dir, config_filename, SettingsFormat::Toml)
    }

    /// Create a new settings service for JSON configuration files.
    pub fn new_json(schema: Value, config_dir: PathBuf, config_filename: &str) -> Result<Self> {
        Self::new_with_format(schema, config_dir, config_filename, SettingsFormat::Json)
    }

    fn new_with_format(
        schema: Value,
        config_dir: PathBuf,
        config_filename: &str,
        format: SettingsFormat,
    ) -> Result<Self> {
        let (reload_tx, _reload_rx) = watch::channel(());

        let config_path = config_dir.join(config_filename);
        tracing::info!("Loading settings from: {:?}", config_path);

        let values = if config_path.exists() {
            let v = match format {
                SettingsFormat::Toml => load_toml_as_json(&config_path)?,
                SettingsFormat::Json => load_json_as_json(&config_path)?,
            };
            // Debug: log sessions config at load time
            if let Some(sessions) = v.get("sessions") {
                tracing::info!("Loaded sessions config: {:?}", sessions);
            }
            v
        } else {
            tracing::warn!("Config file not found: {:?}", config_path);
            Value::Object(serde_json::Map::new())
        };

        Ok(Self {
            schema,
            config_dir,
            config_filename: config_filename.to_string(),
            format,
            values: Arc::new(RwLock::new(values)),
            reload_tx,
        })
    }

    /// Get the config file path.
    pub fn config_path(&self) -> PathBuf {
        self.config_dir.join(&self.config_filename)
    }

    /// Get the schema, filtered by user scope.
    pub fn get_schema(&self, scope: SettingsScope) -> Value {
        filter_schema_by_scope(&self.schema, scope)
    }

    /// Create a new settings service scoped to a different config directory.
    pub fn with_config_dir(&self, config_dir: PathBuf) -> Result<Self> {
        Self::new_with_format(
            self.schema.clone(),
            config_dir,
            &self.config_filename,
            self.format,
        )
    }

    /// Get current values with metadata about configured vs default.
    pub async fn get_values(&self, scope: SettingsScope) -> HashMap<String, SettingsValue> {
        let values = self.values.read().await;
        let filtered_schema = self.get_schema(scope);

        // Debug: log what we're working with
        if let Some(sessions) = values.get("sessions") {
            tracing::debug!("Sessions config: {:?}", sessions);
        } else {
            tracing::debug!("No sessions config found in values");
        }

        let result = extract_values_with_metadata(&values, &filtered_schema, "");

        // Debug: log the sessions.max_concurrent_sessions value
        if let Some(max_sessions) = result.get("sessions.max_concurrent_sessions") {
            tracing::info!("sessions.max_concurrent_sessions = {:?}", max_sessions);
        }

        result
    }

    /// Update configuration values.
    ///
    /// Only allows updating values the user has permission to change.
    pub async fn update_values(&self, updates: ConfigUpdate, scope: SettingsScope) -> Result<()> {
        // Validate that user can update these paths
        let filtered_schema = self.get_schema(scope);

        for path in updates.values.keys() {
            if !path_exists_in_schema(&filtered_schema, path) {
                anyhow::bail!(
                    "Cannot update '{}': permission denied or invalid path",
                    path
                );
            }
        }

        let config_path = self.config_path();

        match self.format {
            SettingsFormat::Toml => {
                // Read current TOML, apply updates, write back
                let mut toml_value = if config_path.exists() {
                    let content = std::fs::read_to_string(&config_path)
                        .context("Failed to read config file")?;
                    content
                        .parse::<toml::Value>()
                        .context("Failed to parse config file")?
                } else {
                    toml::Value::Table(toml::map::Map::new())
                };

                for (path, value) in updates.values {
                    set_toml_value(&mut toml_value, &path, json_to_toml(&value)?)?;
                }

                let content =
                    toml::to_string_pretty(&toml_value).context("Failed to serialize config")?;
                if let Some(parent) = config_path.parent() {
                    std::fs::create_dir_all(parent).context("Failed to create config directory")?;
                }
                std::fs::write(&config_path, content).context("Failed to write config file")?;
            }
            SettingsFormat::Json => {
                let mut json_value = if config_path.exists() {
                    let content = std::fs::read_to_string(&config_path)
                        .context("Failed to read config file")?;
                    serde_json::from_str::<Value>(&content)
                        .context("Failed to parse config file")?
                } else {
                    Value::Object(serde_json::Map::new())
                };

                for (path, value) in updates.values {
                    set_json_value(&mut json_value, &path, value)?;
                }

                if let Some(parent) = config_path.parent() {
                    std::fs::create_dir_all(parent).context("Failed to create config directory")?;
                }
                let content = serde_json::to_string_pretty(&json_value)
                    .context("Failed to serialize config")?;
                std::fs::write(&config_path, content).context("Failed to write config file")?;
            }
        }

        // Reload cached values
        self.reload().await?;

        Ok(())
    }

    /// Reload configuration from disk.
    pub async fn reload(&self) -> Result<()> {
        let config_path = self.config_path();
        let new_values = if config_path.exists() {
            match self.format {
                SettingsFormat::Toml => load_toml_as_json(&config_path)?,
                SettingsFormat::Json => load_json_as_json(&config_path)?,
            }
        } else {
            Value::Object(serde_json::Map::new())
        };

        {
            let mut values = self.values.write().await;
            *values = new_values;
        }

        // Notify subscribers
        let _ = self.reload_tx.send(());

        tracing::info!("Settings reloaded from {:?}", config_path);
        Ok(())
    }
}

/// Load a TOML file as JSON Value.
fn load_toml_as_json(path: &Path) -> Result<Value> {
    let content =
        std::fs::read_to_string(path).with_context(|| format!("Failed to read {:?}", path))?;

    let toml_value: toml::Value = content
        .parse()
        .with_context(|| format!("Failed to parse {:?}", path))?;

    toml_to_json(&toml_value)
}

/// Load a JSON file as JSON Value.
fn load_json_as_json(path: &Path) -> Result<Value> {
    let content =
        std::fs::read_to_string(path).with_context(|| format!("Failed to read {:?}", path))?;
    serde_json::from_str(&content).context("Failed to parse config file")
}

/// Convert TOML Value to JSON Value.
fn toml_to_json(toml: &toml::Value) -> Result<Value> {
    match toml {
        toml::Value::String(s) => Ok(Value::String(s.clone())),
        toml::Value::Integer(i) => Ok(Value::Number((*i).into())),
        toml::Value::Float(f) => serde_json::Number::from_f64(*f)
            .map(Value::Number)
            .ok_or_else(|| anyhow::anyhow!("Invalid float value")),
        toml::Value::Boolean(b) => Ok(Value::Bool(*b)),
        toml::Value::Datetime(dt) => Ok(Value::String(dt.to_string())),
        toml::Value::Array(arr) => {
            let json_arr: Result<Vec<Value>> = arr.iter().map(toml_to_json).collect();
            Ok(Value::Array(json_arr?))
        }
        toml::Value::Table(table) => {
            let mut map = serde_json::Map::new();
            for (k, v) in table {
                map.insert(k.clone(), toml_to_json(v)?);
            }
            Ok(Value::Object(map))
        }
    }
}

/// Convert JSON Value to TOML Value.
fn json_to_toml(json: &Value) -> Result<toml::Value> {
    match json {
        Value::Null => Ok(toml::Value::String("".to_string())), // TOML doesn't have null
        Value::Bool(b) => Ok(toml::Value::Boolean(*b)),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Ok(toml::Value::Integer(i))
            } else if let Some(f) = n.as_f64() {
                Ok(toml::Value::Float(f))
            } else {
                anyhow::bail!("Invalid number")
            }
        }
        Value::String(s) => Ok(toml::Value::String(s.clone())),
        Value::Array(arr) => {
            let toml_arr: Result<Vec<toml::Value>> = arr.iter().map(json_to_toml).collect();
            Ok(toml::Value::Array(toml_arr?))
        }
        Value::Object(map) => {
            let mut table = toml::map::Map::new();
            for (k, v) in map {
                table.insert(k.clone(), json_to_toml(v)?);
            }
            Ok(toml::Value::Table(table))
        }
    }
}

/// Set a value in a TOML structure using a dotted path.
fn set_toml_value(root: &mut toml::Value, path: &str, value: toml::Value) -> Result<()> {
    let parts: Vec<&str> = path.split('.').collect();

    let mut current = root;
    for (i, part) in parts.iter().enumerate() {
        if i == parts.len() - 1 {
            // Last part - set the value
            if let toml::Value::Table(table) = current {
                table.insert(part.to_string(), value);
                return Ok(());
            } else {
                anyhow::bail!("Cannot set value at '{}': parent is not a table", path);
            }
        } else {
            // Navigate/create intermediate tables
            if let toml::Value::Table(table) = current {
                current = table
                    .entry(part.to_string())
                    .or_insert_with(|| toml::Value::Table(toml::map::Map::new()));
            } else {
                anyhow::bail!(
                    "Cannot navigate path '{}': intermediate is not a table",
                    path
                );
            }
        }
    }

    Ok(())
}

fn set_json_value(root: &mut Value, path: &str, value: Value) -> Result<()> {
    let parts: Vec<&str> = path.split('.').collect();
    let mut current = root;

    for (i, part) in parts.iter().enumerate() {
        if i == parts.len() - 1 {
            match current {
                Value::Object(map) => {
                    map.insert(part.to_string(), value.clone());
                    return Ok(());
                }
                _ => {
                    *current = Value::Object(serde_json::Map::new());
                    if let Value::Object(map) = current {
                        map.insert(part.to_string(), value.clone());
                        return Ok(());
                    }
                    anyhow::bail!("Failed to set JSON value at '{}'", path);
                }
            }
        } else {
            current = match current {
                Value::Object(map) => map
                    .entry(part.to_string())
                    .or_insert_with(|| Value::Object(serde_json::Map::new())),
                _ => {
                    *current = Value::Object(serde_json::Map::new());
                    if let Value::Object(map) = current {
                        map.entry(part.to_string())
                            .or_insert_with(|| Value::Object(serde_json::Map::new()))
                    } else {
                        anyhow::bail!("Failed to create JSON object at '{}'", path);
                    }
                }
            };
        }
    }

    Ok(())
}

/// Check if a dotted path exists in the schema.
fn path_exists_in_schema(schema: &Value, path: &str) -> bool {
    let parts: Vec<&str> = path.split('.').collect();
    let mut current = schema;

    for part in parts {
        // Look for the property in "properties"
        if let Some(props) = current.get("properties")
            && let Some(prop) = props.get(part)
        {
            current = prop;
            continue;
        }
        // Also check direct object properties
        if let Some(prop) = current.get(part) {
            current = prop;
            continue;
        }
        return false;
    }

    true
}

/// Extract values with metadata about configured vs default.
fn extract_values_with_metadata(
    values: &Value,
    schema: &Value,
    prefix: &str,
) -> HashMap<String, SettingsValue> {
    let mut result = HashMap::new();

    if let Some(props) = schema.get("properties")
        && let Value::Object(props_map) = props
    {
        for (key, prop_schema) in props_map {
            let path = if prefix.is_empty() {
                key.clone()
            } else {
                format!("{}.{}", prefix, key)
            };

            // Check if this is a leaf value or a nested object
            let prop_type = prop_schema.get("type").and_then(|t| t.as_str());

            if prop_type == Some("object") && prop_schema.get("properties").is_some() {
                // Nested object - recurse
                let nested_values = values.get(key).unwrap_or(&Value::Null);
                let nested = extract_values_with_metadata(nested_values, prop_schema, &path);
                result.extend(nested);
            } else {
                // Leaf value
                let current_value = get_nested_value(values, key);
                let default_value = prop_schema.get("default").cloned();
                let is_configured = current_value.is_some();

                let value = current_value
                    .or_else(|| default_value.clone())
                    .unwrap_or(Value::Null);

                result.insert(
                    path,
                    SettingsValue {
                        value,
                        is_configured,
                        default: default_value,
                    },
                );
            }
        }
    }

    result
}

/// Get a nested value from a JSON object.
fn get_nested_value(value: &Value, key: &str) -> Option<Value> {
    value.get(key).cloned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    #[allow(clippy::approx_constant)]
    fn test_toml_json_roundtrip() {
        let json = json!({
            "string": "hello",
            "number": 42,
            "float": 3.14,
            "bool": true,
            "nested": {
                "value": "inner"
            }
        });

        let toml = json_to_toml(&json).unwrap();
        let back = toml_to_json(&toml).unwrap();

        assert_eq!(json, back);
    }

    #[test]
    fn test_set_toml_value() {
        let mut root = toml::Value::Table(toml::map::Map::new());

        set_toml_value(
            &mut root,
            "voice.default_voice",
            toml::Value::String("af_bella".to_string()),
        )
        .unwrap();

        let voice = root.get("voice").unwrap().as_table().unwrap();
        let default_voice = voice.get("default_voice").unwrap().as_str().unwrap();

        assert_eq!(default_voice, "af_bella");
    }

    #[test]
    fn test_path_exists_in_schema() {
        let schema = json!({
            "properties": {
                "voice": {
                    "type": "object",
                    "properties": {
                        "default_voice": {
                            "type": "string"
                        }
                    }
                }
            }
        });

        assert!(path_exists_in_schema(&schema, "voice"));
        assert!(path_exists_in_schema(&schema, "voice.default_voice"));
        assert!(!path_exists_in_schema(&schema, "voice.nonexistent"));
        assert!(!path_exists_in_schema(&schema, "nonexistent"));
    }
}
