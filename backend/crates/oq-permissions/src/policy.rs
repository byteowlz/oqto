use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Already verified by the owning runner, never accepted from tool arguments.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Binding {
    pub account_id: String,
    pub principal_id: String,
    pub workspace_id: String,
    pub work_directory_id: String,
    pub session_id: String,
    /// Hash/identity of the explicitly authorized disclosure audience.
    pub audience: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    HistorySearch,
    MemoryRead,
    MemoryWrite,
    IssueRead,
    IssueCreate,
    IssueUpdate,
    IssueClose,
    FileRead,
    FileWrite,
    Network,
    Execute,
}

/// Semantic resource identity, not a model-chosen host path or database name.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Target {
    pub capability: Capability,
    pub resource_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Operation {
    /// Stable protected Session/store + native tool-task correlation, supplied
    /// by the runner. The same id with different arguments is an error.
    pub request_id: String,
    pub target: Target,
    pub arguments: serde_json::Value,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuleMode {
    Allow,
    Ask,
    Deny,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rule {
    pub target: Target,
    pub mode: RuleMode,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    pub revision: String,
    pub rules: Vec<Rule>,
    /// Ceiling layers for `path:` file resources. Each layer is evaluated
    /// independently; the most restrictive answer across layers wins.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub paths: Vec<PathLayer>,
}

/// One ceiling layer (built-in, global or workspace).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PathLayer {
    pub default: RuleMode,
    /// Denied: the path is inside one of these, or contains one of them.
    pub deny: Vec<String>,
    /// Requestable roots (inside); otherwise `default`.
    pub ask: Vec<String>,
}

impl PathLayer {
    fn mode(&self, path: &std::path::Path) -> RuleMode {
        let denied = self.deny.iter().any(|root| {
            let root = std::path::Path::new(root);
            path.starts_with(root) || root.starts_with(path)
        });
        if denied {
            RuleMode::Deny
        } else if self
            .ask
            .iter()
            .any(|root| path.starts_with(std::path::Path::new(root)))
        {
            RuleMode::Ask
        } else {
            self.default
        }
    }
}

impl Policy {
    pub fn validate(&self) -> Result<()> {
        bounded_id(&self.revision)?;
        ensure!(self.rules.len() <= 128, "too many policy rules");
        for (i, rule) in self.rules.iter().enumerate() {
            bounded_id(&rule.target.resource_id)?;
            ensure!(
                !self.rules[..i].iter().any(|r| r.target == rule.target),
                "duplicate/ambiguous policy rule"
            );
        }
        ensure!(self.paths.len() <= 8, "too many ceiling layers");
        for layer in &self.paths {
            ensure!(
                layer.deny.len() <= 512 && layer.ask.len() <= 512,
                "ceiling layer too large"
            );
            for path in layer.deny.iter().chain(&layer.ask) {
                ensure!(
                    crate::ceiling::normalize(std::path::Path::new(path))?.as_os_str()
                        == std::ffi::OsStr::new(path),
                    "ceiling paths must be absolute and normalized"
                );
            }
        }
        Ok(())
    }

    pub fn mode(&self, target: &Target) -> RuleMode {
        if let Some(rule) = self.rules.iter().find(|rule| rule.target == *target) {
            return rule.mode;
        }
        match (target.capability, target.resource_id.strip_prefix("path:")) {
            (Capability::FileRead | Capability::FileWrite, Some(path))
                if !self.paths.is_empty() =>
            {
                let path = std::path::Path::new(path);
                // Unnormalized or relative resources never match a ceiling.
                match crate::ceiling::normalize(path) {
                    Ok(normal) if normal.as_os_str() == path.as_os_str() => self
                        .paths
                        .iter()
                        .map(|layer| layer.mode(path))
                        .fold(RuleMode::Allow, crate::ceiling::narrower),
                    _ => RuleMode::Deny,
                }
            }
            _ => RuleMode::Deny,
        }
    }
}

impl Binding {
    pub(crate) fn validate(&self) -> Result<()> {
        for id in [
            &self.account_id,
            &self.principal_id,
            &self.workspace_id,
            &self.work_directory_id,
            &self.session_id,
            &self.audience,
        ] {
            bounded_id(id)?;
        }
        ensure!(
            !self.session_id.starts_with("pending-") && !self.session_id.starts_with("tmp:"),
            "temporary public Session identity is forbidden"
        );
        Ok(())
    }
}

impl Operation {
    pub(crate) fn validate(&self) -> Result<()> {
        bounded_id(&self.request_id)?;
        if let Some(path) = self.target.resource_id.strip_prefix("path:") {
            ensure!(
                !path.is_empty() && path.len() <= 4096 && !path.chars().any(char::is_control),
                "invalid path resource"
            );
        } else {
            bounded_id(&self.target.resource_id)?;
        }
        let encoded = serde_json::to_vec(&self.arguments)?;
        ensure!(
            encoded.len() <= 8192,
            "operation arguments exceed 8192 bytes"
        );
        Ok(())
    }
}

pub(crate) fn bounded_id(id: &str) -> Result<()> {
    ensure!(
        !id.is_empty() && id.len() <= 256 && !id.chars().any(char::is_control),
        "invalid or oversized identity"
    );
    Ok(())
}

pub(crate) fn digest<T: Serialize>(value: &T) -> Result<String> {
    // Explicitly order keys even if another workspace dependency enables
    // serde_json/preserve_order. This is our versioned encoding, not a claim
    // of RFC 8785 number canonicalization.
    let canonical = canonicalize(serde_json::to_value(value)?);
    Ok(format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&canonical)?)
    ))
}

fn canonicalize(value: serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Object(map) => {
            let sorted: std::collections::BTreeMap<_, _> = map.into_iter().collect();
            serde_json::Value::Object(
                sorted
                    .into_iter()
                    .map(|(key, value)| (key, canonicalize(value)))
                    .collect(),
            )
        }
        serde_json::Value::Array(array) => {
            serde_json::Value::Array(array.into_iter().map(canonicalize).collect())
        }
        other => other,
    }
}
