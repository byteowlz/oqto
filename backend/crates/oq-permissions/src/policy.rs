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
        Ok(())
    }

    pub fn mode(&self, target: &Target) -> RuleMode {
        self.rules
            .iter()
            .find(|rule| rule.target == *target)
            .map_or(RuleMode::Deny, |rule| rule.mode)
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
        bounded_id(&self.target.resource_id)?;
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
