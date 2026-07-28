//! Agent Skill catalog discovery and effective-skill resolution.
//!
//! This module is read-only. Callers supply placement-local roots; discovery
//! never expands `~` or assumes the backend can access a remote Workspace.

mod catalog;

pub use catalog::{
    CatalogRoots, DiscoveredSkill, EffectiveState, SkillCatalog, SkillDiagnostic, SkillLocation,
    SkillMutability, SkillPolicy, SkillScope,
};
