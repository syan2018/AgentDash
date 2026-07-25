use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};

pub fn canonical_request_digest<T: Serialize>(value: &T) -> Result<String, serde_json::Error> {
    let value = serde_json::to_value(value)?;
    let bytes = serde_json::to_vec(&canonicalize_json(&value))?;
    Ok(format!("sha256:{:x}", Sha256::digest(bytes)))
}

fn canonicalize_json(value: &Value) -> Value {
    match value {
        Value::Object(object) => {
            let mut entries = object.iter().collect::<Vec<_>>();
            entries.sort_by(|left, right| left.0.cmp(right.0));
            let mut canonical = serde_json::Map::new();
            for (key, value) in entries {
                canonical.insert(key.clone(), canonicalize_json(value));
            }
            Value::Object(canonical)
        }
        Value::Array(items) => Value::Array(items.iter().map(canonicalize_json).collect()),
        other => other.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn digest_is_stable_across_nested_object_key_order() {
        let left = serde_json::json!({
            "metadata": { "z": 1, "a": { "right": true, "left": false } },
            "input": ["hello"]
        });
        let right = serde_json::json!({
            "input": ["hello"],
            "metadata": { "a": { "left": false, "right": true }, "z": 1 }
        });

        assert_eq!(
            canonical_request_digest(&left).unwrap(),
            canonical_request_digest(&right).unwrap()
        );
    }

    #[test]
    fn digest_changes_when_semantic_field_changes() {
        assert_ne!(
            canonical_request_digest(&serde_json::json!({ "input": "one" })).unwrap(),
            canonical_request_digest(&serde_json::json!({ "input": "two" })).unwrap()
        );
    }
}
