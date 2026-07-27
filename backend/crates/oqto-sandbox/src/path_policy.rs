//! Backend-neutral filesystem access policy (ADR-0028).
//!
//! This module decides access only. Mount/materialisation adapters resolve
//! symbolic resources to sandbox-visible absolute paths before constructing a
//! [`ResolvedPolicy`]. Backend adapters compile that resolved policy; they must
//! not add their own precedence rules.

use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fmt,
    path::{Component, Path, PathBuf},
};

/// Filesystem access ordered from least to most permissive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Access {
    None,
    Read,
    Write,
}

/// Stable provenance for an access decision.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuleOrigin {
    pub layer: PolicyLayer,
    pub source: String,
}

/// Policy layers ordered by authority, not by permissiveness.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PolicyLayer {
    System,
    Admin,
    Workspace,
    Session,
}

/// One rule after symbolic roots have been resolved.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedRule {
    path: PathBuf,
    pub access: Access,
    pub origin: RuleOrigin,
}

impl ResolvedRule {
    pub fn new(
        path: impl Into<PathBuf>,
        access: Access,
        origin: RuleOrigin,
    ) -> Result<Self, PolicyError> {
        Ok(Self {
            path: validated_absolute(path.into())?,
            access,
            origin,
        })
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }
}

/// A backend-independent policy whose paths are sandbox-visible and absolute.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedPolicy {
    pub default: Access,
    pub default_origin: RuleOrigin,
    rules: Vec<ResolvedRule>,
}

impl ResolvedPolicy {
    #[must_use]
    pub fn new(default: Access, default_origin: RuleOrigin) -> Self {
        Self {
            default,
            default_origin,
            rules: Vec::new(),
        }
    }

    pub fn add_rule(&mut self, rule: ResolvedRule) {
        self.rules.push(rule);
    }

    #[must_use]
    pub fn rules(&self) -> &[ResolvedRule] {
        &self.rules
    }

    /// Resolve one absolute sandbox-visible path.
    ///
    /// The longest component-prefix wins. Equal-specificity rules resolve to
    /// the lower access, independent of insertion order.
    pub fn resolve(&self, path: &Path) -> Result<Resolution, PolicyError> {
        let path = validated_absolute(path.to_path_buf())?;
        let mut winner: Option<&ResolvedRule> = None;

        for rule in &self.rules {
            if !path.starts_with(&rule.path) {
                continue;
            }
            winner = match winner {
                None => Some(rule),
                Some(current) => {
                    let rule_depth = component_depth(&rule.path);
                    let current_depth = component_depth(&current.path);
                    if rule_depth > current_depth
                        || (rule_depth == current_depth && rule.access < current.access)
                    {
                        Some(rule)
                    } else {
                        Some(current)
                    }
                }
            };
        }

        Ok(match winner {
            Some(rule) => Resolution {
                access: rule.access,
                origin: rule.origin.clone(),
                matched_path: Some(rule.path.clone()),
            },
            None => Resolution {
                access: self.default,
                origin: self.default_origin.clone(),
                matched_path: None,
            },
        })
    }
}

/// Explainable result of resolving one policy layer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Resolution {
    pub access: Access,
    pub origin: RuleOrigin,
    pub matched_path: Option<PathBuf>,
}

/// Resolve all authority layers by taking the least permissive result.
///
/// Path specificity is local to a layer. It can never let a lower-authority
/// layer override a restriction from a higher-authority layer.
pub fn resolve_layers(
    policies: &[ResolvedPolicy],
    path: &Path,
) -> Result<LayeredResolution, PolicyError> {
    let decisions = policies
        .iter()
        .map(|policy| policy.resolve(path))
        .collect::<Result<Vec<_>, _>>()?;
    let access = decisions
        .iter()
        .map(|decision| decision.access)
        .min()
        .unwrap_or(Access::None);
    let decisive = decisions
        .iter()
        .filter(|decision| decision.access == access)
        .cloned()
        .collect();
    Ok(LayeredResolution {
        access,
        decisive,
        decisions,
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LayeredResolution {
    pub access: Access,
    /// All layer decisions tied for the least permissive result.
    pub decisive: Vec<Resolution>,
    /// Every layer decision, retained for UI explanation and diagnostics.
    pub decisions: Vec<Resolution>,
}

/// Stable, dotted identifier for a runtime-discovered resource.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ResourceId(String);

impl ResourceId {
    pub fn parse(value: impl Into<String>) -> Result<Self, PolicyError> {
        let value = value.into();
        let valid = value.split('.').count() >= 2
            && value.split('.').all(|segment| {
                !segment.is_empty()
                    && segment
                        .chars()
                        .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_')
                    && segment
                        .chars()
                        .next()
                        .is_some_and(|ch| ch.is_ascii_lowercase())
            });
        if !valid {
            return Err(PolicyError::InvalidResourceId(value));
        }
        Ok(Self(value))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ResourceId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourceKind {
    Filesystem,
}

/// One trusted provider's declaration of a sandbox-visible resource.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceDeclaration {
    pub id: ResourceId,
    pub kind: ResourceKind,
    pub owner: String,
    pub display_name: String,
    sandbox_path: PathBuf,
    pub capabilities: Vec<Access>,
    pub required: bool,
    pub schema_version: u32,
}

impl ResourceDeclaration {
    pub fn filesystem(
        id: ResourceId,
        owner: impl Into<String>,
        display_name: impl Into<String>,
        sandbox_path: impl Into<PathBuf>,
        capabilities: Vec<Access>,
        required: bool,
        schema_version: u32,
    ) -> Result<Self, PolicyError> {
        if schema_version == 0 {
            return Err(PolicyError::InvalidSchemaVersion);
        }
        let sandbox_path = validated_absolute(sandbox_path.into())?;
        let capabilities = normalized_capabilities(capabilities);
        Ok(Self {
            id,
            kind: ResourceKind::Filesystem,
            owner: owner.into(),
            display_name: display_name.into(),
            sandbox_path,
            capabilities,
            required,
            schema_version,
        })
    }

    #[must_use]
    pub fn sandbox_path(&self) -> &Path {
        &self.sandbox_path
    }

    #[must_use]
    pub fn supports(&self, access: Access) -> bool {
        access == Access::None || self.capabilities.contains(&access)
    }
}

/// Runtime-discovered resources for one execution target.
#[derive(Debug, Default, Clone)]
pub struct ResourceRegistry {
    resources: BTreeMap<ResourceId, ResourceDeclaration>,
}

impl ResourceRegistry {
    pub fn register(&mut self, resource: ResourceDeclaration) -> Result<(), PolicyError> {
        if self.resources.contains_key(&resource.id) {
            return Err(PolicyError::DuplicateResource(resource.id));
        }
        self.resources.insert(resource.id.clone(), resource);
        Ok(())
    }

    #[must_use]
    pub fn get(&self, id: &ResourceId) -> Option<&ResourceDeclaration> {
        self.resources.get(id)
    }

    pub fn iter(&self) -> impl Iterator<Item = &ResourceDeclaration> {
        self.resources.values()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PolicyError {
    PathMustBeAbsolute(PathBuf),
    ParentTraversal(PathBuf),
    InvalidResourceId(String),
    DuplicateResource(ResourceId),
    InvalidSchemaVersion,
}

impl fmt::Display for PolicyError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PathMustBeAbsolute(path) => {
                write!(
                    formatter,
                    "policy path must be absolute: {}",
                    path.display()
                )
            }
            Self::ParentTraversal(path) => {
                write!(
                    formatter,
                    "policy path contains parent traversal: {}",
                    path.display()
                )
            }
            Self::InvalidResourceId(id) => write!(
                formatter,
                "resource id must contain at least two lowercase dotted segments: {id}"
            ),
            Self::DuplicateResource(id) => write!(formatter, "resource already registered: {id}"),
            Self::InvalidSchemaVersion => {
                write!(formatter, "resource schema version must be non-zero")
            }
        }
    }
}

impl std::error::Error for PolicyError {}

fn validated_absolute(path: PathBuf) -> Result<PathBuf, PolicyError> {
    if !path.is_absolute() {
        return Err(PolicyError::PathMustBeAbsolute(path));
    }
    if path
        .components()
        .any(|component| component == Component::ParentDir)
    {
        return Err(PolicyError::ParentTraversal(path));
    }
    Ok(path)
}

fn component_depth(path: &Path) -> usize {
    path.components().count()
}

fn normalized_capabilities(capabilities: Vec<Access>) -> Vec<Access> {
    let maximum = capabilities.into_iter().max().unwrap_or(Access::None);
    match maximum {
        Access::None => Vec::new(),
        Access::Read => vec![Access::Read],
        Access::Write => vec![Access::Read, Access::Write],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn origin(layer: PolicyLayer, source: &str) -> RuleOrigin {
        RuleOrigin {
            layer,
            source: source.to_string(),
        }
    }

    fn rule(path: &str, access: Access, layer: PolicyLayer) -> ResolvedRule {
        ResolvedRule::new(path, access, origin(layer, "test")).expect("valid test rule")
    }

    #[test]
    fn most_specific_component_prefix_wins() {
        let mut policy = ResolvedPolicy::new(Access::None, origin(PolicyLayer::Admin, "default"));
        policy.add_rule(rule("/work", Access::Read, PolicyLayer::Admin));
        policy.add_rule(rule("/work/src", Access::Write, PolicyLayer::Admin));

        assert_eq!(
            policy
                .resolve(Path::new("/work/src/lib.rs"))
                .unwrap()
                .access,
            Access::Write
        );
        assert_eq!(
            policy.resolve(Path::new("/work/README.md")).unwrap().access,
            Access::Read
        );
        assert_eq!(
            policy.resolve(Path::new("/workspace")).unwrap().access,
            Access::None
        );
    }

    #[test]
    fn equal_specificity_chooses_lower_access_regardless_of_order() {
        for rules in [[Access::Write, Access::None], [Access::None, Access::Write]] {
            let mut policy =
                ResolvedPolicy::new(Access::Write, origin(PolicyLayer::Admin, "default"));
            for access in rules {
                policy.add_rule(rule("/work/secret", access, PolicyLayer::Admin));
            }
            assert_eq!(
                policy
                    .resolve(Path::new("/work/secret/key"))
                    .unwrap()
                    .access,
                Access::None
            );
        }
    }

    #[test]
    fn layer_specificity_never_overrides_a_more_authoritative_restriction() {
        let mut admin = ResolvedPolicy::new(Access::Write, origin(PolicyLayer::Admin, "default"));
        admin.add_rule(rule("/work", Access::Read, PolicyLayer::Admin));
        let mut workspace =
            ResolvedPolicy::new(Access::Write, origin(PolicyLayer::Workspace, "default"));
        workspace.add_rule(rule(
            "/work/specific",
            Access::Write,
            PolicyLayer::Workspace,
        ));

        let result = resolve_layers(&[admin, workspace], Path::new("/work/specific/file")).unwrap();
        assert_eq!(result.access, Access::Read);
        assert_eq!(result.decisive[0].origin.layer, PolicyLayer::Admin);
    }

    #[test]
    fn empty_layer_set_fails_closed() {
        let result = resolve_layers(&[], Path::new("/work/file")).unwrap();
        assert_eq!(result.access, Access::None);
    }

    #[test]
    fn relative_and_parent_traversal_paths_are_rejected() {
        assert!(matches!(
            ResolvedRule::new("relative", Access::Read, origin(PolicyLayer::Admin, "test")),
            Err(PolicyError::PathMustBeAbsolute(_))
        ));
        assert!(matches!(
            ResolvedRule::new(
                "/work/../secret",
                Access::Read,
                origin(PolicyLayer::Admin, "test")
            ),
            Err(PolicyError::ParentTraversal(_))
        ));
    }

    #[test]
    fn resource_ids_are_namespaced_and_stable() {
        assert_eq!(
            ResourceId::parse("agent.sessions").unwrap().as_str(),
            "agent.sessions"
        );
        for invalid in [
            "sessions",
            "Agent.sessions",
            "agent..sessions",
            "agent.-sessions",
        ] {
            assert!(ResourceId::parse(invalid).is_err(), "accepted {invalid}");
        }
    }

    #[test]
    fn registry_rejects_duplicate_resource_identity() {
        let id = ResourceId::parse("agent.sessions").unwrap();
        let declaration = ResourceDeclaration::filesystem(
            id,
            "pi-adapter",
            "Agent sessions",
            "/agent-state/sessions",
            vec![Access::Write],
            true,
            1,
        )
        .unwrap();
        let mut registry = ResourceRegistry::default();
        registry.register(declaration.clone()).unwrap();
        assert!(matches!(
            registry.register(declaration),
            Err(PolicyError::DuplicateResource(_))
        ));
    }

    #[test]
    fn write_capability_implies_read() {
        let declaration = ResourceDeclaration::filesystem(
            ResourceId::parse("agent.sessions").unwrap(),
            "pi-adapter",
            "Agent sessions",
            "/agent-state/sessions",
            vec![Access::Write],
            true,
            1,
        )
        .unwrap();
        assert!(declaration.supports(Access::Read));
        assert!(declaration.supports(Access::Write));
    }
}
