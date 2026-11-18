//! Optional, source-implicit catalog facts. No IO, runtime, stores, authorization,
//! source discovery or UI grouping. The caller supplies ONE admitted source set.
//! Only delegation may drive subagent collapse; fork is independent provenance.
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;
use std::collections::HashMap;

pub const MAX_CATALOG_ROWS: usize = 5000;
pub const MAX_RELATIONS_PER_ROW: usize = 2;
/// Bounded tolerance for malformed peer arrays; normalized output is always <=2.
pub const MAX_OPTIONAL_INPUT_RECORDS: usize = 200;
pub const MAX_PUBLIC_ID_BYTES: usize = 256;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RelationKind {
    Delegation,
    Fork,
}
impl RelationKind {
    fn index(self) -> usize {
        match self {
            Self::Delegation => 0,
            Self::Fork => 1,
        }
    }
    fn parse(value: &Value) -> Option<Self> {
        match value.as_str()? {
            "delegation" => Some(Self::Delegation),
            "fork" => Some(Self::Fork),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Relation {
    pub kind: RelationKind,
    pub parent_session_id: String,
}

/// Adapter-owned row identity, already bound to one authenticated catalog source.
/// Normalization changes only optional facts, never the IDs or row membership.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CatalogRow {
    pub id: String,
    pub relations: Vec<Relation>,
}

/// Missing, null, unknown or malformed optional metadata is never a row error.
/// Duplicate/conflicting known kinds are discarded independently; a malformed
/// delegation cannot erase a valid independent fork in the same bounded array.
pub fn deserialize_optional<'de, D>(deserializer: D) -> Result<Vec<Relation>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Value::deserialize(deserializer)?;
    Ok(parse_optional(&value))
}

pub fn parse_optional(value: &Value) -> Vec<Relation> {
    let Some(values) = value
        .as_array()
        .filter(|rows| rows.len() <= MAX_OPTIONAL_INPUT_RECORDS)
    else {
        return Vec::new();
    };
    let mut parents: [Option<String>; 2] = [None, None];
    let mut invalid = [false; 2];
    for value in values {
        let Some(kind) = value.get("kind").and_then(RelationKind::parse) else {
            continue;
        };
        let slot = kind.index();
        let parent = value
            .get("parent_session_id")
            .and_then(Value::as_str)
            .filter(|id| valid_id(id));
        if parents[slot].is_some() || parent.is_none() {
            invalid[slot] = true;
        } else {
            parents[slot] = parent.map(str::to_owned);
        }
    }
    [RelationKind::Delegation, RelationKind::Fork]
        .into_iter()
        .filter_map(|kind| {
            let slot = kind.index();
            (!invalid[slot])
                .then(|| parents[slot].take())
                .flatten()
                .map(|parent_session_id| Relation {
                    kind,
                    parent_session_id,
                })
        })
        .collect()
}

pub fn valid_id(value: &str) -> bool {
    !value.is_empty() && value.len() <= MAX_PUBLIC_ID_BYTES && !value.chars().any(char::is_control)
}

/// Retain only distinct admitted endpoints and at most one parent per kind;
/// remove cycle-member edges independently for each kind. All base rows stay.
/// No global registry, cross-source join, ancestor discovery or title inference.
pub fn normalize(rows: &mut [CatalogRow]) {
    if rows.len() > MAX_CATALOG_ROWS {
        for row in rows {
            row.relations.clear();
        }
        return;
    }
    let mut index: HashMap<String, Option<usize>> = HashMap::new();
    for (position, row) in rows.iter().enumerate() {
        if valid_id(&row.id) {
            index
                .entry(row.id.clone())
                .and_modify(|found| *found = None)
                .or_insert(Some(position));
        }
    }
    for (position, row) in rows.iter_mut().enumerate() {
        let unique = index.get(&row.id) == Some(&Some(position));
        let mut retained: [Option<Relation>; 2] = [None, None];
        let mut invalid = [false; 2];
        for relation in std::mem::take(&mut row.relations) {
            let slot = relation.kind.index();
            if !unique
                || relation.parent_session_id == row.id
                || !index
                    .get(&relation.parent_session_id)
                    .is_some_and(Option::is_some)
                || retained[slot].is_some()
            {
                invalid[slot] = true;
            } else {
                retained[slot] = Some(relation);
            }
        }
        row.relations = retained
            .into_iter()
            .enumerate()
            .filter_map(
                |(slot, relation)| {
                    if invalid[slot] { None } else { relation }
                },
            )
            .collect();
    }
    for kind in [RelationKind::Delegation, RelationKind::Fork] {
        remove_cycles(rows, &index, kind);
    }
}

fn remove_cycles(
    rows: &mut [CatalogRow],
    index: &HashMap<String, Option<usize>>,
    kind: RelationKind,
) {
    let parents: Vec<Option<usize>> = rows
        .iter()
        .map(|row| {
            row.relations
                .iter()
                .find(|relation| relation.kind == kind)
                .and_then(|relation| index.get(&relation.parent_session_id).copied().flatten())
        })
        .collect();
    let mut colors = vec![0u8; rows.len()];
    for start in 0..rows.len() {
        let mut cursor = Some(start);
        while let Some(position) = cursor.filter(|position| colors[*position] == 0) {
            colors[position] = 1;
            cursor = parents[position];
        }
        if let Some(cycle) = cursor.filter(|position| colors[*position] == 1) {
            let mut position = cycle;
            loop {
                rows[position]
                    .relations
                    .retain(|relation| relation.kind != kind);
                match parents[position] {
                    Some(next) if next != cycle => position = next,
                    _ => break,
                }
            }
        }
        cursor = Some(start);
        while let Some(position) = cursor.filter(|position| colors[*position] == 1) {
            colors[position] = 2;
            cursor = parents[position];
        }
    }
}

#[cfg(test)]
mod tests;
