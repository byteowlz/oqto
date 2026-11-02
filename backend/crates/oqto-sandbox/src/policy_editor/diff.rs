//! Semantic containment diff at all compiler rule boundaries.
//! Approval/network semantics are not represented; never treat this as a full
//! authorization diff or proof that publication is safe.
use super::{EffectiveNode, effective_node};
use crate::path_policy::ResolvedPolicy;
use anyhow::Result;
use std::{collections::BTreeSet, path::Path};

#[derive(Debug, Clone)]
pub struct ContainmentChange {
    pub current: EffectiveNode,
    pub proposed: EffectiveNode,
}

/// Both owners must come from the canonical parser/compiler. Workspace and
/// protected layers remain fixed ceilings while comparing an owner proposal.
pub fn containment_diff(
    current: &ResolvedPolicy,
    proposed: &ResolvedPolicy,
    workspace: &ResolvedPolicy,
    protected: &ResolvedPolicy,
    root: &Path,
) -> Result<Vec<ContainmentChange>> {
    let mut paths = BTreeSet::from([root.to_path_buf()]);
    for layer in [current, proposed, workspace, protected] {
        paths.extend(
            layer
                .rules()
                .iter()
                .filter(|rule| rule.path().starts_with(root))
                .map(|rule| rule.path().to_path_buf()),
        );
    }
    let mut changes = Vec::new();
    for path in paths {
        let before = effective_node(current, workspace, protected, &path)?;
        let after = effective_node(proposed, workspace, protected, &path)?;
        if before.effective != after.effective
            || before.mixed != after.mixed
            || before.inherited != after.inherited
        {
            changes.push(ContainmentChange {
                current: before,
                proposed: after,
            });
        }
    }
    Ok(changes)
}
