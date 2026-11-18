//! dingz ADR-0001 protocol v1. Unknown operations and invalid envelopes fail closed.
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const VERSION: u32 = 1;
pub const MAX_FRAME: usize = 65536;
pub const MAX_ENVELOPE: usize = 32768;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Done,
    Blocked,
    Delegation,
    Board,
    Wiki,
    Routine,
    #[default]
    Info,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Link {
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Envelope {
    pub title: String,
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    pub kind: Kind,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub links: Vec<Link>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub funnel: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub labels: Vec<String>,
}

pub fn text(value: &str, max: usize, required: bool, multiline: bool) -> Result<()> {
    ensure!(!required || !value.trim().is_empty(), "empty required text");
    ensure!(
        value.chars().count() <= max,
        "text exceeds {max} characters"
    );
    ensure!(
        !value
            .chars()
            .any(|c| c.is_control() && !(multiline && matches!(c, '\n' | '\r' | '\t'))),
        "control character in text"
    );
    Ok(())
}

pub fn id(value: &str) -> Result<()> {
    ensure!(
        !value.is_empty() && value.len() <= 128,
        "id must have 1..128 characters"
    );
    ensure!(
        value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"._:-".contains(&c)),
        "invalid id"
    );
    Ok(())
}

impl Envelope {
    pub fn validate(&self) -> Result<()> {
        text(&self.title, 200, true, false).context("title")?;
        text(&self.body, 4000, false, true).context("body")?;
        ensure!(self.links.len() <= 8, "too many links");
        for link in &self.links {
            text(&link.url, 2048, true, false)?;
            let (scheme, rest) = link.url.split_once(':').context("link needs URI scheme")?;
            let mut chars = scheme.chars();
            ensure!(
                chars.next().is_some_and(|c| c.is_ascii_alphabetic())
                    && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '.' | '-'))
                    && !rest.is_empty()
                    && !rest.chars().any(char::is_whitespace),
                "invalid URI"
            );
            if let Some(label) = &link.label {
                text(label, 120, true, false)?;
            }
        }
        for value in [&self.task, &self.funnel, &self.project]
            .into_iter()
            .flatten()
        {
            text(value, 128, true, false)?;
        }
        ensure!(self.labels.len() <= 16, "too many labels");
        let mut routes = 0;
        for (index, label) in self.labels.iter().enumerate() {
            text(label, 64, true, false)?;
            let mut chars = label.bytes();
            ensure!(
                chars
                    .next()
                    .is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
                    && chars.all(|c| c.is_ascii_lowercase()
                        || c.is_ascii_digit()
                        || b":._-".contains(&c)),
                "invalid label"
            );
            ensure!(!self.labels[..index].contains(label), "duplicate label");
            if let Some(route) = label.strip_prefix("route:") {
                ensure!(
                    ["local", "private", "cloud"].contains(&route),
                    "unknown route"
                );
                routes += 1;
            }
        }
        ensure!(routes <= 1, "multiple route labels");
        ensure!(
            serde_json::to_vec(self)?.len() <= MAX_ENVELOPE,
            "envelope exceeds byte limit"
        );
        Ok(())
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Filter {
    #[serde(default)]
    pub kinds: Vec<Kind>,
    #[serde(default)]
    pub agents: Vec<String>,
    #[serde(default)]
    pub projects: Vec<String>,
    #[serde(default)]
    pub funnels: Vec<String>,
    #[serde(default)]
    pub labels: Vec<String>,
}
impl Filter {
    pub fn validate(&self) -> Result<()> {
        ensure!(self.kinds.len() <= 32, "too many kinds");
        for list in [&self.agents, &self.projects, &self.funnels, &self.labels] {
            ensure!(list.len() <= 32, "too many filter entries");
            for value in list {
                text(value, 128, true, false)?;
            }
        }
        Ok(())
    }
    pub fn matches(&self, post: &StoredPost) -> bool {
        let e = &post.envelope;
        (self.kinds.is_empty() || self.kinds.contains(&e.kind))
            && (self.agents.is_empty() || self.agents.contains(&post.stamped.agent))
            && (self.projects.is_empty()
                || e.project.as_ref().is_some_and(|p| {
                    self.projects.iter().any(|prefix| {
                        p.strip_prefix(prefix)
                            .is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
                    })
                }))
            && (self.funnels.is_empty()
                || e.funnel.as_ref().is_some_and(|f| self.funnels.contains(f)))
            && (self.labels.is_empty() || e.labels.iter().any(|l| self.labels.contains(l)))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Mode {
    FollowUp,
    Steer,
    NewTask,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Reply {
    pub post_id: String,
    pub mode: Mode,
    pub text: String,
}
impl Reply {
    pub fn validate(&self) -> Result<()> {
        id(&self.post_id)?;
        text(&self.text, 4000, true, true)
    }
}

#[derive(Debug, Deserialize)]
#[serde(tag = "op", rename_all = "lowercase")]
pub enum Operation {
    Post {
        post: Envelope,
    },
    Watch {
        #[serde(default)]
        filter: Filter,
        #[serde(default)]
        since: Option<String>,
    },
    Reply {
        reply: Reply,
    },
}
#[derive(Debug, Deserialize)]
pub struct Request {
    pub v: u32,
    pub request_id: String,
    #[serde(flatten)]
    pub operation: Operation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Route {
    Local,
    Private,
    Cloud,
}
impl Route {
    pub fn label(self) -> &'static str {
        match self {
            Self::Local => "route:local",
            Self::Private => "route:private",
            Self::Cloud => "route:cloud",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Stamp {
    pub agent: String,
    pub workspace: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task: Option<String>,
    pub route: Route,
}
impl Stamp {
    pub fn validate(&self) -> Result<()> {
        for value in [&self.agent, &self.workspace] {
            text(value, 128, true, false)?;
        }
        for value in [&self.session, &self.task].into_iter().flatten() {
            id(value)?;
        }
        if let Some(session) = &self.session {
            ensure!(
                !session.starts_with("pending-") && !session.starts_with("tmp:"),
                "temporary Session identity cannot be registered"
            );
        }
        Ok(())
    }
    /// Keep the stricter caller route. A route label may only narrow.
    pub fn label(&self, post: &mut Envelope) {
        let requested = post
            .labels
            .iter()
            .find(|l| l.starts_with("route:"))
            .cloned();
        let strict = match (self.route, requested.as_deref()) {
            (Route::Local, _) | (_, Some("route:local")) => Route::Local,
            (Route::Private, _) | (_, Some("route:private")) => Route::Private,
            _ => Route::Cloud,
        };
        post.labels.retain(|l| !l.starts_with("route:"));
        post.labels.push(strict.label().to_owned());
        post.task.clone_from(&self.task);
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredPost {
    pub id: String,
    pub stored_at: String,
    pub stamped: Stamp,
    pub envelope: Envelope,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WireError {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub supported_versions: Option<Vec<u32>>,
}
impl WireError {
    pub fn new(code: &str, message: &str) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            supported_versions: None,
        }
    }
}
impl std::fmt::Display for WireError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}
impl std::error::Error for WireError {}

pub fn success(request_id: &str, result: Value) -> Value {
    serde_json::json!({"v":VERSION,"request_id":request_id,"ok":true,"result":result})
}
pub fn failure(request_id: Option<&str>, error: &WireError) -> Value {
    serde_json::json!({"v":VERSION,"request_id":request_id,"ok":false,"error":error})
}
