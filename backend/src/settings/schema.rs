//! JSON Schema handling with x-scope filtering.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::Path;

/// Scope for settings visibility.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SettingsScope {
    Admin,
    User,
}

impl SettingsScope {
    /// Check if this scope can view the given scope.
    pub fn can_view(&self, other: SettingsScope) -> bool {
        match self {
            SettingsScope::Admin => true, // Admin can see everything
            SettingsScope::User => other == SettingsScope::User,
        }
    }
}

/// Load a JSON schema from a file.
pub fn load_schema(path: &Path) -> Result<Value> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read schema from {:?}", path))?;

    serde_json::from_str(&content)
        .with_context(|| format!("Failed to parse schema from {:?}", path))
}

/// Load a JSON schema from embedded bytes.
pub fn load_schema_from_bytes(bytes: &[u8]) -> Result<Value> {
    serde_json::from_slice(bytes).context("Failed to parse embedded schema")
}

/// Filter a JSON schema based on the user's scope.
///
/// Removes properties where x-scope doesn't match the viewer's permissions.
/// Admin sees everything, users only see x-scope: "user" properties.
pub fn filter_schema_by_scope(schema: &Value, viewer_scope: SettingsScope) -> Value {
    filter_object(schema, viewer_scope)
}

fn filter_object(value: &Value, viewer_scope: SettingsScope) -> Value {
    match value {
        Value::Object(map) => {
            let mut result = serde_json::Map::new();

            for (key, val) in map {
                // Check if this property has an x-scope
                if let Some(scope_val) = val.get("x-scope") {
                    if let Some(scope_str) = scope_val.as_str() {
                        let prop_scope = match scope_str {
                            "admin" => SettingsScope::Admin,
                            "user" => SettingsScope::User,
                            _ => SettingsScope::Admin, // Default to admin for unknown
                        };

                        if !viewer_scope.can_view(prop_scope) {
                            continue; // Skip this property
                        }
                    }
                }

                // Recursively filter nested objects
                let filtered = filter_object(val, viewer_scope);
                result.insert(key.clone(), filtered);
            }

            // Handle "properties" specially - filter them too
            if let Some(props) = result.get("properties").cloned() {
                if let Value::Object(props_map) = props {
                    let mut filtered_props = serde_json::Map::new();

                    for (key, val) in props_map {
                        // Check x-scope on each property
                        if let Some(scope_val) = val.get("x-scope") {
                            if let Some(scope_str) = scope_val.as_str() {
                                let prop_scope = match scope_str {
                                    "admin" => SettingsScope::Admin,
                                    "user" => SettingsScope::User,
                                    _ => SettingsScope::Admin,
                                };

                                if !viewer_scope.can_view(prop_scope) {
                                    continue;
                                }
                            }
                        }

                        filtered_props.insert(key, filter_object(&val, viewer_scope));
                    }

                    result.insert("properties".to_string(), Value::Object(filtered_props));
                }
            }

            Value::Object(result)
        }
        Value::Array(arr) => {
            Value::Array(arr.iter().map(|v| filter_object(v, viewer_scope)).collect())
        }
        _ => value.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_filter_admin_sees_all() {
        let schema = json!({
            "properties": {
                "admin_setting": {
                    "type": "string",
                    "x-scope": "admin"
                },
                "user_setting": {
                    "type": "string",
                    "x-scope": "user"
                }
            }
        });

        let filtered = filter_schema_by_scope(&schema, SettingsScope::Admin);
        let props = filtered.get("properties").unwrap();

        assert!(props.get("admin_setting").is_some());
        assert!(props.get("user_setting").is_some());
    }

    #[test]
    fn test_filter_user_sees_only_user() {
        let schema = json!({
            "properties": {
                "admin_setting": {
                    "type": "string",
                    "x-scope": "admin"
                },
                "user_setting": {
                    "type": "string",
                    "x-scope": "user"
                }
            }
        });

        let filtered = filter_schema_by_scope(&schema, SettingsScope::User);
        let props = filtered.get("properties").unwrap();

        assert!(props.get("admin_setting").is_none());
        assert!(props.get("user_setting").is_some());
    }

    #[test]
    fn test_no_scope_defaults_to_visible() {
        let schema = json!({
            "properties": {
                "no_scope_setting": {
                    "type": "string"
                }
            }
        });

        let filtered = filter_schema_by_scope(&schema, SettingsScope::User);
        let props = filtered.get("properties").unwrap();

        // Properties without x-scope should be visible to all
        assert!(props.get("no_scope_setting").is_some());
    }
}
