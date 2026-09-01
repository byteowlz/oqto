//! Immutable App-defined Agent Context catalog (ADR-0044).

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::capability::{CONTEXT_DIR, PACKAGE_PATH_LIMITS, is_valid_operation_id};
use crate::path::{bounded_relative_path, is_beneath};
use crate::{AppPackageError, AppPackageErrorCode};

pub const AGENT_CONTEXT_SCHEMA_V0: &str = "oqto-app-context/v0";
const MAX_TOPICS: usize = 64;
const MAX_ACTIONS: usize = 64;
const MAX_TEXT_BYTES: usize = 512;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContextLifetime {
    Ephemeral,
    SessionLocal,
    DurableReference,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContextDisclosure {
    Ambient,
    ExplicitIntent,
    Sensitive,
    HighVolume,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextTopicDefinition {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub description: Option<String>,
    pub schema_file: PathBuf,
    pub schema_version: String,
    pub lifetime: ContextLifetime,
    pub disclosure: ContextDisclosure,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextActionDefinition {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub required_topics: Vec<String>,
    #[serde(default)]
    pub requires_user_activation: bool,
    pub input_schema_file: PathBuf,
    pub operation: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentContextCatalog {
    pub schema: String,
    pub topics: Vec<ContextTopicDefinition>,
    pub actions: Vec<ContextActionDefinition>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawCatalog {
    schema: String,
    #[serde(default, rename = "topic")]
    topics: Vec<RawTopic>,
    #[serde(default, rename = "action")]
    actions: Vec<RawAction>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawTopic {
    id: String,
    title: String,
    #[serde(default)]
    description: Option<String>,
    schema_file: String,
    schema_version: String,
    lifetime: ContextLifetime,
    disclosure: ContextDisclosure,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawAction {
    id: String,
    title: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    required_topics: Vec<String>,
    #[serde(default)]
    requires_user_activation: bool,
    input_schema_file: String,
    operation: String,
}

pub fn parse_agent_context_catalog(
    bytes: &[u8],
    catalog_path: &Path,
) -> Result<AgentContextCatalog, AppPackageError> {
    let text =
        std::str::from_utf8(bytes).map_err(|_| invalid(catalog_path, "catalog must be UTF-8"))?;
    let raw: RawCatalog = toml::from_str(text).map_err(|error| {
        invalid(
            catalog_path,
            format!("invalid context catalog TOML: {error}"),
        )
    })?;
    if raw.schema != AGENT_CONTEXT_SCHEMA_V0 {
        return Err(invalid(
            catalog_path,
            "unsupported Agent Context catalog schema",
        ));
    }
    if raw.topics.is_empty() || raw.topics.len() > MAX_TOPICS || raw.actions.len() > MAX_ACTIONS {
        return Err(invalid(
            catalog_path,
            "context catalog topic/action count is outside limits",
        ));
    }

    let mut topic_ids = BTreeSet::new();
    let mut topics = Vec::with_capacity(raw.topics.len());
    for topic in raw.topics {
        validate_id(&topic.id, catalog_path)?;
        validate_text(&topic.title, catalog_path, "topic title")?;
        if !topic_ids.insert(topic.id.clone()) {
            return Err(invalid(
                catalog_path,
                format!("duplicate topic id {:?}", topic.id),
            ));
        }
        topics.push(ContextTopicDefinition {
            id: topic.id,
            title: topic.title,
            description: validate_optional_text(topic.description, catalog_path)?,
            schema_file: validate_schema_path(&topic.schema_file, catalog_path)?,
            schema_version: topic.schema_version,
            lifetime: topic.lifetime,
            disclosure: topic.disclosure,
        });
    }

    let mut action_ids = BTreeSet::new();
    let mut actions = Vec::with_capacity(raw.actions.len());
    for action in raw.actions {
        validate_id(&action.id, catalog_path)?;
        validate_text(&action.title, catalog_path, "action title")?;
        if !action_ids.insert(action.id.clone()) {
            return Err(invalid(
                catalog_path,
                format!("duplicate action id {:?}", action.id),
            ));
        }
        let mut required = BTreeSet::new();
        for topic in &action.required_topics {
            if !topic_ids.contains(topic) || !required.insert(topic.clone()) {
                return Err(invalid(
                    catalog_path,
                    format!("action references unknown or duplicate topic {topic:?}"),
                ));
            }
        }
        if !is_valid_operation_id(&action.operation) {
            return Err(invalid(
                catalog_path,
                "context action operation id is invalid",
            ));
        }
        actions.push(ContextActionDefinition {
            id: action.id,
            title: action.title,
            description: validate_optional_text(action.description, catalog_path)?,
            required_topics: action.required_topics,
            requires_user_activation: action.requires_user_activation,
            input_schema_file: validate_schema_path(&action.input_schema_file, catalog_path)?,
            operation: action.operation,
        });
    }

    Ok(AgentContextCatalog {
        schema: raw.schema,
        topics,
        actions,
    })
}

fn validate_schema_path(value: &str, catalog: &Path) -> Result<PathBuf, AppPackageError> {
    let path = bounded_relative_path(value, PACKAGE_PATH_LIMITS)
        .map_err(|error| error.with_code(AppPackageErrorCode::InvalidCapabilityRequest))?;
    if !is_beneath(&path, CONTEXT_DIR)
        || path.extension().and_then(|value| value.to_str()) != Some("json")
    {
        return Err(invalid(
            catalog,
            "context schema must be a JSON file under context/",
        ));
    }
    Ok(path)
}

fn validate_id(value: &str, path: &Path) -> Result<(), AppPackageError> {
    if !is_valid_operation_id(value) || value.starts_with("oqto.") || value.starts_with("desktop.")
    {
        return Err(invalid(
            path,
            format!("invalid or reserved context id {value:?}"),
        ));
    }
    Ok(())
}

fn validate_text(value: &str, path: &Path, field: &str) -> Result<(), AppPackageError> {
    if value.trim().is_empty() || value.len() > MAX_TEXT_BYTES {
        return Err(invalid(
            path,
            format!("{field} must contain 1-{MAX_TEXT_BYTES} bytes"),
        ));
    }
    Ok(())
}

fn validate_optional_text(
    value: Option<String>,
    path: &Path,
) -> Result<Option<String>, AppPackageError> {
    if let Some(text) = value.as_deref() {
        validate_text(text, path, "description")?;
    }
    Ok(value)
}

fn invalid(path: &Path, message: impl Into<String>) -> AppPackageError {
    AppPackageError::new(AppPackageErrorCode::InvalidCapabilityRequest, message).at(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    const CATALOG: &str = r#"
schema = "oqto-app-context/v0"

[[topic]]
id = "gallery.selection"
title = "Selected images"
schema_file = "context/gallery-selection.schema.json"
schema_version = "gallery.selection/v1"
lifetime = "session_local"
disclosure = "explicit_intent"

[[action]]
id = "gallery.selection.clear"
title = "Clear selection"
required_topics = ["gallery.selection"]
input_schema_file = "context/clear.schema.json"
operation = "gallery.selection.clear"
"#;

    #[test]
    fn parses_unfamiliar_domain_vocabulary() -> Result<(), AppPackageError> {
        let catalog =
            parse_agent_context_catalog(CATALOG.as_bytes(), Path::new("context/catalog.toml"))?;
        assert_eq!(catalog.topics[0].id, "gallery.selection");
        assert_eq!(catalog.actions[0].required_topics, ["gallery.selection"]);
        Ok(())
    }

    #[test]
    fn rejects_platform_namespace_and_unknown_topic() {
        let reserved = CATALOG.replace("gallery.selection", "oqto.session");
        assert!(
            parse_agent_context_catalog(reserved.as_bytes(), Path::new("context/catalog.toml"))
                .is_err()
        );
        let unknown = CATALOG.replace(
            "required_topics = [\"gallery.selection\"]",
            "required_topics = [\"gallery.missing\"]",
        );
        assert!(
            parse_agent_context_catalog(unknown.as_bytes(), Path::new("context/catalog.toml"))
                .is_err()
        );
    }

    #[test]
    fn rejects_schema_escape() {
        let escaping = CATALOG.replace(
            "context/gallery-selection.schema.json",
            "../gallery-selection.schema.json",
        );
        assert!(
            parse_agent_context_catalog(escaping.as_bytes(), Path::new("context/catalog.toml"))
                .is_err()
        );
    }
}
