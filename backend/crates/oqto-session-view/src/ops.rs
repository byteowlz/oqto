//! The seven op verbs of upstream Chord, applied all or nothing.
//!
//! Rules from the contract (`spec.md` "Ops", `operations.json` `ops` and `op_refusals`),
//! tested against every vector in `ops-vectors.json`, which upstream's own `apply` produced.

use serde_json::{Map, Value};
use thiserror::Error;

/// Path segments a client must refuse.
pub const RESERVED_SEGMENTS: [&str; 3] = ["__proto__", "constructor", "prototype"];

/// Why an op was refused; the three classes of `operations.json` `op_refusals`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum OpRefusal {
    /// Not a tuple, unknown verb, wrong arity or payload, bad path shape, no bijection.
    #[error("invalid_op")]
    InvalidOp,
    /// Reserved segment, negative or non-integer index, string into an array, index past the end.
    #[error("unsafe_path")]
    UnsafePath,
    /// A parent is missing or not a container, or the target has the wrong type.
    #[error("unresolvable_path")]
    UnresolvablePath,
}

impl OpRefusal {
    /// The refusal class as named by the contract.
    pub const fn code(self) -> &'static str {
        match self {
            Self::InvalidOp => "invalid_op",
            Self::UnsafePath => "unsafe_path",
            Self::UnresolvablePath => "unresolvable_path",
        }
    }
}

/// Apply `ops` to `root` in order. On any refusal `root` is left exactly as it was.
pub fn apply(root: &mut Value, ops: &[Value]) -> Result<(), OpRefusal> {
    let mut next = root.clone();
    for op in ops {
        apply_one(&mut next, op)?;
    }
    *root = next;
    Ok(())
}

/// Structural check of one op as the frame schema sees it (verb, arity, path segments and
/// payload types). The applier checks more (a permutation must be a bijection).
pub fn schema_valid(op: &Value) -> bool {
    validate(op, false).is_ok()
}

fn as_index(value: &Value) -> Option<usize> {
    value.as_u64().and_then(|n| usize::try_from(n).ok())
}

fn check_path(path: &Value, non_empty: bool) -> Result<&[Value], OpRefusal> {
    let segments = path.as_array().ok_or(OpRefusal::InvalidOp)?;
    if non_empty && segments.is_empty() {
        return Err(OpRefusal::InvalidOp);
    }
    for segment in segments {
        match segment {
            Value::String(text) if RESERVED_SEGMENTS.contains(&text.as_str()) => {
                return Err(OpRefusal::UnsafePath);
            }
            Value::String(_) => {}
            other if as_index(other).is_some() => {}
            _ => return Err(OpRefusal::UnsafePath),
        }
    }
    Ok(segments)
}

fn validate(op: &Value, bijection: bool) -> Result<(), OpRefusal> {
    let tuple = op.as_array().ok_or(OpRefusal::InvalidOp)?;
    let verb = tuple
        .first()
        .and_then(Value::as_str)
        .ok_or(OpRefusal::InvalidOp)?;
    let arity = |n: usize| {
        if tuple.len() == n {
            Ok(())
        } else {
            Err(OpRefusal::InvalidOp)
        }
    };
    match verb {
        "r" => arity(2),
        "s" | "d" => {
            arity(if verb == "s" { 3 } else { 2 })?;
            check_path(&tuple[1], true).map(|_| ())
        }
        "a" => {
            arity(3)?;
            if !tuple[2].is_string() {
                return Err(OpRefusal::InvalidOp);
            }
            check_path(&tuple[1], true).map(|_| ())
        }
        "t" => {
            arity(3)?;
            if as_index(&tuple[2]).is_none() {
                return Err(OpRefusal::InvalidOp);
            }
            check_path(&tuple[1], true).map(|_| ())
        }
        "p" => {
            arity(5)?;
            check_path(&tuple[1], false)?;
            if as_index(&tuple[2]).is_none()
                || as_index(&tuple[3]).is_none()
                || !tuple[4].is_array()
            {
                return Err(OpRefusal::InvalidOp);
            }
            Ok(())
        }
        "m" => {
            arity(3)?;
            check_path(&tuple[1], false)?;
            let permutation = tuple[2].as_array().ok_or(OpRefusal::InvalidOp)?;
            let mut seen = vec![false; permutation.len()];
            for index in permutation {
                let index = as_index(index).ok_or(OpRefusal::InvalidOp)?;
                if !bijection {
                    continue;
                }
                match seen.get_mut(index) {
                    Some(slot) if !*slot => *slot = true,
                    _ => return Err(OpRefusal::InvalidOp),
                }
            }
            Ok(())
        }
        _ => Err(OpRefusal::InvalidOp),
    }
}

fn step<'a>(node: &'a mut Value, segment: &Value) -> Result<&'a mut Value, OpRefusal> {
    match node {
        Value::Array(items) => {
            let index = as_index(segment).ok_or(OpRefusal::UnsafePath)?;
            items.get_mut(index).ok_or(OpRefusal::UnresolvablePath)
        }
        Value::Object(map) => map
            .get_mut(&object_key(segment))
            .ok_or(OpRefusal::UnresolvablePath),
        _ => Err(OpRefusal::UnresolvablePath),
    }
}

fn object_key(segment: &Value) -> String {
    match segment {
        Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}

fn container<'a>(root: &'a mut Value, path: &[Value]) -> Result<&'a mut Value, OpRefusal> {
    let mut node = root;
    for segment in path {
        node = step(node, segment)?;
    }
    if node.is_array() || node.is_object() {
        Ok(node)
    } else {
        Err(OpRefusal::UnresolvablePath)
    }
}

fn apply_one(root: &mut Value, op: &Value) -> Result<(), OpRefusal> {
    validate(op, true)?;
    let tuple = op.as_array().ok_or(OpRefusal::InvalidOp)?;
    let verb = tuple[0].as_str().ok_or(OpRefusal::InvalidOp)?;
    if verb == "r" {
        *root = tuple[1].clone();
        return Ok(());
    }
    let path = tuple[1].as_array().ok_or(OpRefusal::InvalidOp)?;
    match verb {
        "p" => splice(root, path, tuple),
        "m" => permute(root, path, &tuple[2]),
        _ => {
            let (last, parent_path) = path.split_last().ok_or(OpRefusal::InvalidOp)?;
            let parent = container(root, parent_path)?;
            match parent {
                Value::Array(items) => at_index(items, last, verb, tuple),
                Value::Object(map) => at_key(map, &object_key(last), verb, tuple),
                _ => Err(OpRefusal::UnresolvablePath),
            }
        }
    }
}

fn target_array<'a>(root: &'a mut Value, path: &[Value]) -> Result<&'a mut Vec<Value>, OpRefusal> {
    let target = if path.is_empty() {
        root
    } else {
        container(root, path)?
    };
    target.as_array_mut().ok_or(OpRefusal::UnresolvablePath)
}

fn splice(root: &mut Value, path: &[Value], tuple: &[Value]) -> Result<(), OpRefusal> {
    let index = as_index(&tuple[2]).ok_or(OpRefusal::InvalidOp)?;
    let remove = as_index(&tuple[3]).ok_or(OpRefusal::InvalidOp)?;
    let items = tuple[4].as_array().ok_or(OpRefusal::InvalidOp)?.clone();
    let target = target_array(root, path)?;
    let start = index.min(target.len());
    let end = start.saturating_add(remove).min(target.len());
    target.splice(start..end, items);
    Ok(())
}

fn permute(root: &mut Value, path: &[Value], permutation: &Value) -> Result<(), OpRefusal> {
    let order: Vec<usize> = permutation
        .as_array()
        .ok_or(OpRefusal::InvalidOp)?
        .iter()
        .map(|index| as_index(index).ok_or(OpRefusal::InvalidOp))
        .collect::<Result<_, _>>()?;
    let target = target_array(root, path)?;
    if target.len() != order.len() {
        return Err(OpRefusal::UnresolvablePath);
    }
    let previous = target.clone();
    for (slot, source) in target.iter_mut().zip(order) {
        *slot = previous.get(source).cloned().ok_or(OpRefusal::InvalidOp)?;
    }
    Ok(())
}

fn at_index(
    items: &mut Vec<Value>,
    key: &Value,
    verb: &str,
    tuple: &[Value],
) -> Result<(), OpRefusal> {
    let index = as_index(key).ok_or(OpRefusal::UnsafePath)?;
    if index > items.len() {
        return Err(OpRefusal::UnsafePath);
    }
    let present = index < items.len();
    match verb {
        "s" if present => items[index] = tuple[2].clone(),
        "s" => items.push(tuple[2].clone()),
        "d" if present => {
            items.remove(index);
        }
        "d" => return Err(OpRefusal::UnresolvablePath),
        _ => {
            let current = items.get_mut(index).ok_or(OpRefusal::UnresolvablePath)?;
            edit_text(current, verb, &tuple[2])?;
        }
    }
    Ok(())
}

fn at_key(
    map: &mut Map<String, Value>,
    key: &str,
    verb: &str,
    tuple: &[Value],
) -> Result<(), OpRefusal> {
    match verb {
        "s" => {
            map.insert(key.to_owned(), tuple[2].clone());
        }
        "d" => {
            map.remove(key);
        }
        _ => {
            let current = map.get_mut(key).ok_or(OpRefusal::UnresolvablePath)?;
            edit_text(current, verb, &tuple[2])?;
        }
    }
    Ok(())
}

fn edit_text(current: &mut Value, verb: &str, argument: &Value) -> Result<(), OpRefusal> {
    let Value::String(text) = current else {
        return Err(OpRefusal::UnresolvablePath);
    };
    if verb == "a" {
        text.push_str(argument.as_str().ok_or(OpRefusal::InvalidOp)?);
    } else {
        let count = as_index(argument).ok_or(OpRefusal::InvalidOp)?;
        *text = drop_utf16(text, count);
    }
    Ok(())
}

/// Drop `count` leading UTF-16 code units. A cut inside a surrogate pair keeps the rest of
/// the text and replaces the orphaned half (JavaScript would keep a lone surrogate, which a
/// Rust string cannot hold).
fn drop_utf16(text: &str, count: usize) -> String {
    let units: Vec<u16> = text.encode_utf16().collect();
    let rest = units.get(count..).unwrap_or(&[]);
    String::from_utf16_lossy(rest)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn refusal_leaves_the_value_unchanged() {
        let mut value = json!({"xs": [1]});
        let ops = [json!(["s", ["xs", 0], 2]), json!(["s", ["xs", 5], 3])];
        assert_eq!(apply(&mut value, &ops), Err(OpRefusal::UnsafePath));
        assert_eq!(value, json!({"xs": [1]}));
    }

    #[test]
    fn schema_check_accepts_a_non_bijective_permutation() {
        assert!(schema_valid(&json!(["m", ["xs"], [0, 0]])));
        let mut value = json!({"xs": [1, 2]});
        assert_eq!(
            apply(&mut value, &[json!(["m", ["xs"], [0, 0]])]),
            Err(OpRefusal::InvalidOp)
        );
    }

    #[test]
    fn integer_segment_addresses_an_object_key() {
        let mut value = json!({"3": "a"});
        assert_eq!(apply(&mut value, &[json!(["a", [3], "b"])]), Ok(()));
        assert_eq!(value, json!({"3": "ab"}));
    }
}
