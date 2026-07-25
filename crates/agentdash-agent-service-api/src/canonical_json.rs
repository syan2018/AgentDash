use serde::Serialize;
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

/// Serializes a contract value with every JSON object key ordered lexicographically.
///
/// Arrays retain their semantic order. The result is stable across `serde_json` map backends and
/// JSONB round-trips, so persisted digest evidence can be revalidated after restart.
pub fn canonical_json_bytes<T: Serialize>(value: &T) -> Result<Vec<u8>, serde_json::Error> {
    let mut value = serde_json::to_value(value)?;
    sort_object_keys(&mut value);
    serde_json::to_vec(&value)
}

pub fn canonical_json_sha256<T: Serialize>(value: &T) -> Result<String, serde_json::Error> {
    canonical_json_bytes(value).map(|bytes| format!("sha256:{:x}", Sha256::digest(bytes)))
}

fn sort_object_keys(value: &mut Value) {
    match value {
        Value::Array(values) => values.iter_mut().for_each(sort_object_keys),
        Value::Object(object) => {
            let mut entries = std::mem::take(object).into_iter().collect::<Vec<_>>();
            entries.sort_unstable_by(|(left, _), (right, _)| left.cmp(right));
            let mut sorted = Map::new();
            for (key, mut value) in entries {
                sort_object_keys(&mut value);
                sorted.insert(key, value);
            }
            *object = sorted;
        }
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recursive_object_order_is_not_digest_significant() {
        let left = serde_json::json!({
            "z": [{"b": 2, "a": 1}],
            "a": {"d": 4, "c": 3}
        });
        let right: Value = serde_json::from_str(r#"{"a":{"c":3,"d":4},"z":[{"a":1,"b":2}]}"#)
            .expect("equivalent JSON");

        assert_eq!(
            canonical_json_bytes(&left).expect("left bytes"),
            canonical_json_bytes(&right).expect("right bytes")
        );
        assert_eq!(
            canonical_json_sha256(&left).expect("left digest"),
            canonical_json_sha256(&right).expect("right digest")
        );
    }
}
