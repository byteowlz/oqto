//! Global/workspace ceiling compiler. The workspace file lives in an
//! agent-writable directory, so it may only ever narrow: every path request is
//! evaluated against each layer independently and the most restrictive answer
//! wins. Built-in hard denies (credentials, system roots, protected runtime
//! roots and anything containing them) sit above every configuration.
use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Result, bail, ensure};
use serde::Deserialize;

use crate::{Capability, PathLayer, Policy, Rule, RuleMode, Target, policy::digest};

/// Reads of workspace files (and granted resources) through the read tool.
pub const WORKSPACE_READ: &str = "workspace-read";
/// Writes and edits of workspace files.
pub const WORKSPACE_WRITE: &str = "workspace-write";
/// Shell commands that could not be classified to a simple prefix.
pub const WORKSPACE_SHELL: &str = "workspace-shell";
/// Simple shell commands are `shell:<prefix>` (e.g. `shell:npm test`); without
/// an explicit rule they inherit the generic shell mode.
pub const SHELL_PREFIX: &str = "shell:";
pub const NETWORK_EGRESS: &str = "egress";
pub const PATH_PREFIX: &str = "path:";

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct CeilingFile {
    read: Option<RuleMode>,
    edit: Option<RuleMode>,
    shell: Option<RuleMode>,
    network: Option<RuleMode>,
    /// Owner pre-approved simple shell commands (global file only).
    #[serde(default)]
    allow_shell: Vec<String>,
    outside_paths: Option<RuleMode>,
    #[serde(default)]
    deny_paths: Vec<String>,
    #[serde(default)]
    ask_paths: Vec<String>,
    /// Owner pre-approval, honored only in the trusted global file.
    #[serde(default)]
    allow_paths: Vec<String>,
}

/// Trusted inputs. `home`/`workspace` come from the guard, never from a file.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CeilingInput {
    /// Raw TOML text of the owner's global ceiling, if any.
    pub global: Option<String>,
    /// Raw TOML text of the workspace's ceiling, if any.
    pub workspace_file: Option<String>,
    pub home: PathBuf,
    pub workspace: PathBuf,
    /// Protected runtime roots (owner profile, authority, sources, guard).
    pub hard_deny: Vec<PathBuf>,
    /// Operator "always in this workspace" decisions, stored by the guard
    /// outside the agent's reach. They lift `ask` to `allow`, never `deny`.
    #[serde(default)]
    pub remembered: Remembered,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct Remembered {
    #[serde(default)]
    pub read: bool,
    #[serde(default)]
    pub edit: bool,
    #[serde(default)]
    pub shell: bool,
    #[serde(default)]
    pub shell_prefixes: Vec<String>,
}

/// A remembered or configured shell prefix: a few plain words, no shell syntax.
pub fn valid_shell_prefix(prefix: &str) -> bool {
    !prefix.is_empty()
        && prefix.len() <= 128
        && prefix.split(' ').count() <= 3
        && prefix.split(' ').all(|word| {
            !word.is_empty()
                && word.chars().all(|c| {
                    c.is_ascii_alphanumeric()
                        || matches!(c, '-' | '_' | '.' | ':' | '/' | '@' | '+')
                })
        })
}

/// Paths no grant may touch, relative to home where applicable.
const HOME_DENIES: &[&str] = &[
    ".ssh",
    ".gnupg",
    ".aws",
    ".azure",
    ".config",
    ".pi",
    ".local/share/keyrings",
    ".local/share/oqto",
    ".password-store",
    ".kube",
    ".docker",
    ".netrc",
    ".git-credentials",
    ".npmrc",
    ".pypirc",
];
const SYSTEM_DENIES: &[&str] = &[
    "/etc/shadow",
    "/etc/gshadow",
    "/etc/sudoers",
    "/etc/sudoers.d",
    "/etc/oqto",
    "/root",
    "/proc",
    "/sys",
    "/dev",
    "/boot",
    "/run",
    "/var/run",
];

fn rank(mode: RuleMode) -> u8 {
    match mode {
        RuleMode::Deny => 0,
        RuleMode::Ask => 1,
        RuleMode::Allow => 2,
    }
}

pub(crate) fn narrower(a: RuleMode, b: RuleMode) -> RuleMode {
    if rank(a) <= rank(b) { a } else { b }
}

/// Lexically normalized absolute path; `..` and relative paths are refused.
pub fn normalize(path: &Path) -> Result<PathBuf> {
    ensure!(path.is_absolute(), "ceiling paths must be absolute");
    let mut out = PathBuf::from("/");
    for component in path.components() {
        match component {
            Component::RootDir | Component::CurDir => {}
            Component::Normal(part) => out.push(part),
            Component::ParentDir => bail!("ceiling paths must not contain '..'"),
            Component::Prefix(_) => bail!("unsupported path prefix"),
        }
    }
    Ok(out)
}

/// Lexical normalization for paths that may not exist yet.
pub fn normalize_lexical(path: &Path) -> Result<PathBuf> {
    normalize(path)
}

fn expand(raw: &str, home: &Path, base: &Path) -> Result<PathBuf> {
    ensure!(!raw.is_empty() && raw.len() <= 4096, "invalid ceiling path");
    let path = if raw == "~" {
        home.to_path_buf()
    } else if let Some(rest) = raw.strip_prefix("~/") {
        home.join(rest)
    } else if Path::new(raw).is_absolute() {
        PathBuf::from(raw)
    } else {
        base.join(raw)
    };
    normalize(&path)
}

fn parse(text: Option<&str>, name: &str) -> Result<CeilingFile> {
    match text {
        None => Ok(CeilingFile::default()),
        Some(text) => {
            ensure!(text.len() <= 64 * 1024, "{name} ceiling too large");
            toml::from_str(text).with_context(|| format!("invalid {name} ceiling"))
        }
    }
}

fn layer(file: &CeilingFile, home: &Path, base: &Path, name: &str) -> Result<PathLayer> {
    let default = file.outside_paths.unwrap_or(RuleMode::Ask);
    ensure!(
        default != RuleMode::Allow,
        "{name} ceiling: outside_paths may be \"ask\" or \"deny\"; unapproved access is never allowed"
    );
    ensure!(
        file.deny_paths.len() <= 256 && file.ask_paths.len() <= 256,
        "{name} ceiling: too many paths"
    );
    let render = |paths: &[String]| -> Result<Vec<String>> {
        paths
            .iter()
            .map(|raw| Ok(expand(raw, home, base)?.display().to_string()))
            .collect()
    };
    Ok(PathLayer {
        builtin: false,
        default,
        deny: render(&file.deny_paths)?,
        ask: render(&file.ask_paths)?,
        allow: render(&file.allow_paths)?,
    })
}

/// Compile the effective coding policy. Unsupported values fail closed.
pub fn compile_ceiling(input: &CeilingInput) -> Result<Policy> {
    let home = normalize(&input.home)?;
    let workspace = normalize(&input.workspace)?;
    let global = parse(input.global.as_deref(), "global")?;
    let local = parse(input.workspace_file.as_deref(), "workspace")?;
    // The workspace layer can only narrow: an unset workspace key is `allow`
    // (no further restriction) and the narrower answer wins.
    let mode = |global: Option<RuleMode>, default: RuleMode, local: Option<RuleMode>| {
        narrower(global.unwrap_or(default), local.unwrap_or(RuleMode::Allow))
    };
    let lift = |mode: RuleMode, remembered: bool| {
        if remembered && mode == RuleMode::Ask {
            RuleMode::Allow
        } else {
            mode
        }
    };
    let remembered = &input.remembered;
    let read = lift(
        mode(global.read, RuleMode::Allow, local.read),
        remembered.read,
    );
    let edit = lift(
        mode(global.edit, RuleMode::Ask, local.edit),
        remembered.edit,
    );
    let shell_base = mode(global.shell, RuleMode::Ask, local.shell);
    let shell = lift(shell_base, remembered.shell);
    ensure!(
        local.allow_shell.is_empty(),
        "workspace ceiling: allow_shell is owner-only; the workspace may deny or ask, never allow"
    );
    let network = narrower(
        global.network.unwrap_or(RuleMode::Ask),
        local.network.unwrap_or(RuleMode::Allow),
    );
    let mut hard = Vec::new();
    for relative in HOME_DENIES {
        hard.push(home.join(relative).display().to_string());
    }
    for path in SYSTEM_DENIES {
        hard.push((*path).to_string());
    }
    for path in &input.hard_deny {
        hard.push(normalize(path)?.display().to_string());
    }
    // The workspace is already granted to the hands; anything containing it
    // (home, /) is far broader than any request should get.
    hard.push(workspace.display().to_string());
    let mut layers = vec![
        PathLayer {
            builtin: true,
            default: RuleMode::Allow,
            deny: hard,
            ask: vec![],
            allow: vec![],
        },
        layer(&global, &home, &home, "global")?,
    ];
    if input.workspace_file.is_some() {
        ensure!(
            local.allow_paths.is_empty(),
            "workspace ceiling: allow_paths is owner-only; the workspace may deny or ask, never allow"
        );
        layers.push(layer(&local, &home, &workspace, "workspace")?);
    }
    let mut policy = Policy {
        revision: String::new(),
        rules: vec![
            Rule {
                target: Target {
                    capability: Capability::Execute,
                    resource_id: WORKSPACE_READ.into(),
                },
                mode: read,
            },
            Rule {
                target: Target {
                    capability: Capability::Execute,
                    resource_id: WORKSPACE_WRITE.into(),
                },
                mode: edit,
            },
            Rule {
                target: Target {
                    capability: Capability::Execute,
                    resource_id: WORKSPACE_SHELL.into(),
                },
                mode: shell,
            },
            Rule {
                target: Target {
                    capability: Capability::Network,
                    resource_id: NETWORK_EGRESS.into(),
                },
                mode: network,
            },
        ],
        paths: layers,
    };
    // Pre-approved simple commands lift `ask` (never `deny`) for their prefix.
    if shell_base == RuleMode::Ask {
        let mut prefixes: Vec<&String> = global
            .allow_shell
            .iter()
            .chain(&remembered.shell_prefixes)
            .collect();
        prefixes.sort();
        prefixes.dedup();
        ensure!(prefixes.len() <= 256, "too many allowed shell prefixes");
        for prefix in prefixes {
            ensure!(
                valid_shell_prefix(prefix),
                "invalid allowed shell prefix: {prefix:?}"
            );
            policy.rules.push(Rule {
                target: Target {
                    capability: Capability::Execute,
                    resource_id: format!("{SHELL_PREFIX}{prefix}"),
                },
                mode: RuleMode::Allow,
            });
        }
    }
    policy.revision = format!("ceiling-{}", &digest(&policy)?[..16]);
    policy.validate()?;
    Ok(policy)
}
