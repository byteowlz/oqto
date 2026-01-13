//! JSON Schema handling with x-scope filtering and $ref resolution.

use serde::{Deserialize, Serialize};
use serde_json::Value;

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

/// Filter a JSON schema based on the user's scope.
///
/// Resolves $ref references and removes properties where x-scope doesn't match
/// the viewer's permissions. Admin sees everything, users only see x-scope: "user" properties.
pub fn filter_schema_by_scope(schema: &Value, viewer_scope: SettingsScope) -> Value {
    // First resolve all $ref references
    let resolved = resolve_refs(schema, schema);
    // Then filter by scope
    filter_object(&resolved, viewer_scope)
}

/// Resolve all $ref references in a JSON schema.
///
/// Handles `allOf` with `$ref` by inlining the referenced definition.
fn resolve_refs(value: &Value, root: &Value) -> Value {
    match value {
        Value::Object(map) => {
            // Check for allOf with $ref - this is the pattern used in the mmry schema
            if let Some(all_of) = map.get("allOf") {
                if let Some(arr) = all_of.as_array() {
                    // Merge allOf items and the current object (excluding allOf)
                    let mut merged = serde_json::Map::new();

                    // First, copy all non-allOf properties from current object
                    for (key, val) in map {
                        if key != "allOf" {
                            merged.insert(key.clone(), resolve_refs(val, root));
                        }
                    }

                    // Then merge in properties from allOf references
                    for item in arr {
                        if let Some(ref_path) = item.get("$ref").and_then(|v| v.as_str()) {
                            if let Some(resolved) = resolve_ref_path(ref_path, root) {
                                let resolved = resolve_refs(&resolved, root);
                                if let Some(obj) = resolved.as_object() {
                                    for (key, val) in obj {
                                        // Don't overwrite existing keys (like default)
                                        if !merged.contains_key(key) {
                                            merged.insert(key.clone(), val.clone());
                                        }
                                    }
                                }
                            }
                        } else {
                            // Non-ref item in allOf, merge it
                            let resolved_item = resolve_refs(item, root);
                            if let Some(obj) = resolved_item.as_object() {
                                for (key, val) in obj {
                                    if !merged.contains_key(key) {
                                        merged.insert(key.clone(), val.clone());
                                    }
                                }
                            }
                        }
                    }

                    return Value::Object(merged);
                }
            }

            // Check for direct $ref
            if let Some(ref_path) = map.get("$ref").and_then(|v| v.as_str()) {
                if let Some(resolved) = resolve_ref_path(ref_path, root) {
                    return resolve_refs(&resolved, root);
                }
            }

            // Regular object - recursively resolve
            let mut result = serde_json::Map::new();
            for (key, val) in map {
                result.insert(key.clone(), resolve_refs(val, root));
            }
            Value::Object(result)
        }
        Value::Array(arr) => Value::Array(arr.iter().map(|v| resolve_refs(v, root)).collect()),
        _ => value.clone(),
    }
}

/// Resolve a $ref path like "#/definitions/AnalyzerConfig" to its value.
fn resolve_ref_path(ref_path: &str, root: &Value) -> Option<Value> {
    // Only handle local refs starting with #/
    if !ref_path.starts_with("#/") {
        return None;
    }

    let path = &ref_path[2..]; // Skip "#/"
    let parts: Vec<&str> = path.split('/').collect();

    let mut current = root;
    for part in parts {
        current = current.get(part)?;
    }

    Some(current.clone())
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

    #[test]
    fn test_resolve_refs_with_allof() {
        let schema = json!({
            "properties": {
                "analyzer": {
                    "default": {
                        "enabled": false,
                        "endpoint": null
                    },
                    "allOf": [
                        { "$ref": "#/definitions/AnalyzerConfig" }
                    ]
                }
            },
            "definitions": {
                "AnalyzerConfig": {
                    "type": "object",
                    "properties": {
                        "enabled": {
                            "description": "Enable analyzer",
                            "default": false,
                            "type": "boolean"
                        },
                        "endpoint": {
                            "description": "HTTP endpoint",
                            "default": null,
                            "type": ["string", "null"]
                        }
                    }
                }
            }
        });

        let resolved = filter_schema_by_scope(&schema, SettingsScope::Admin);
        let props = resolved.get("properties").unwrap();
        let analyzer = props.get("analyzer").unwrap();

        // Should have resolved the $ref and have properties
        assert!(analyzer.get("properties").is_some());
        let analyzer_props = analyzer.get("properties").unwrap();
        assert!(analyzer_props.get("enabled").is_some());
        assert!(analyzer_props.get("endpoint").is_some());

        // Should preserve the default from the parent
        assert!(analyzer.get("default").is_some());

        // Should have type from the referenced definition
        assert_eq!(analyzer.get("type").unwrap(), "object");
    }

    #[test]
    fn test_resolve_nested_refs() {
        let schema = json!({
            "properties": {
                "integrations": {
                    "allOf": [{ "$ref": "#/definitions/IntegrationsConfig" }]
                }
            },
            "definitions": {
                "IntegrationsConfig": {
                    "type": "object",
                    "properties": {
                        "lst": {
                            "allOf": [{ "$ref": "#/definitions/LstConfig" }]
                        }
                    }
                },
                "LstConfig": {
                    "type": "object",
                    "properties": {
                        "enabled": { "type": "boolean", "default": true }
                    }
                }
            }
        });

        let resolved = filter_schema_by_scope(&schema, SettingsScope::Admin);
        let integrations = resolved
            .get("properties")
            .unwrap()
            .get("integrations")
            .unwrap();
        let lst = integrations.get("properties").unwrap().get("lst").unwrap();
        let enabled = lst.get("properties").unwrap().get("enabled").unwrap();

        assert_eq!(enabled.get("type").unwrap(), "boolean");
        assert_eq!(enabled.get("default").unwrap(), true);
    }
}
