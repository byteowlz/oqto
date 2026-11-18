//! Canonical JSON as the contract defines it: object keys sorted, no whitespace, UTF-8.
//!
//! Used for view hashes (`value_sha256`) and for request content equality. Written out by
//! hand so the result never depends on serde_json's map feature flags.

use serde_json::Value;
use sha2::{Digest, Sha256};

/// Canonical JSON text of `value`.
pub fn to_string(value: &Value) -> String {
    let mut out = String::new();
    write(value, &mut out);
    out
}

/// Lowercase hex SHA-256 of the canonical JSON of `value`.
pub fn sha256(value: &Value) -> String {
    hex::encode(Sha256::digest(to_string(value).as_bytes()))
}

fn write(value: &Value, out: &mut String) {
    match value {
        Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            out.push('{');
            for (index, key) in keys.into_iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                write_str(key, out);
                out.push(':');
                if let Some(item) = map.get(key) {
                    write(item, out);
                }
            }
            out.push('}');
        }
        Value::Array(items) => {
            out.push('[');
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                write(item, out);
            }
            out.push(']');
        }
        Value::String(text) => write_str(text, out),
        other => out.push_str(&other.to_string()),
    }
}

fn write_str(text: &str, out: &mut String) {
    // serde_json escapes exactly like Python's json.dumps(ensure_ascii=False): quote,
    // backslash, \b \f \n \r \t, and other control characters as lowercase \u00XX.
    out.push_str(&Value::String(text.to_owned()).to_string());
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn sorts_keys_and_drops_whitespace() {
        let value = json!({"b": [1, {"d": 1, "c": "x\u{1b}\n"}], "a": null});
        assert_eq!(
            to_string(&value),
            r#"{"a":null,"b":[1,{"c":"x\u001b\n","d":1}]}"#
        );
    }

    #[test]
    fn keeps_non_ascii_as_utf8() {
        assert_eq!(to_string(&json!("grüße")), "\"grüße\"");
    }
}
