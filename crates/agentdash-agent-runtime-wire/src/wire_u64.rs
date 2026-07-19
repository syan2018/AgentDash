use std::borrow::Cow;

use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::{Deserialize, Deserializer, Serialize, Serializer, de};
use ts_rs::TS;

pub const CANONICAL_U64_PATTERN: &str = "^(0|[1-9][0-9]{0,19})$";

/// Raw Runtime Wire framing coordinate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, TS)]
#[ts(type = "string & { readonly __runtime_wire_u64: \"canonical_unsigned_decimal\" }")]
pub struct RuntimeWireU64(pub u64);

impl Serialize for RuntimeWireU64 {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serialize(&self.0, serializer)
    }
}

impl<'de> Deserialize<'de> for RuntimeWireU64 {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserialize(deserializer).map(Self)
    }
}

impl JsonSchema for RuntimeWireU64 {
    fn schema_name() -> Cow<'static, str> {
        "RuntimeWireU64".into()
    }

    fn json_schema(_generator: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "type": "string",
            "description": "canonical unsigned-decimal Runtime Wire coordinate",
            "pattern": CANONICAL_U64_PATTERN,
            "maxLength": 20
        })
    }
}

pub fn serialize<S>(value: &u64, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serializer.serialize_str(&value.to_string())
}

pub fn deserialize<'de, D>(deserializer: D) -> Result<u64, D::Error>
where
    D: Deserializer<'de>,
{
    let value = String::deserialize(deserializer)?;
    parse(&value).map_err(de::Error::custom)
}

pub fn parse(value: &str) -> Result<u64, &'static str> {
    if value == "0" {
        return Ok(0);
    }
    if value.is_empty()
        || value.len() > 20
        || value.starts_with('0')
        || !value.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err("expected canonical unsigned-decimal u64 string");
    }
    value
        .parse()
        .map_err(|_| "unsigned-decimal value exceeds u64")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_frame_coordinate_domain_is_lossless() {
        let encoded = serde_json::to_string(&RuntimeWireU64(u64::MAX)).expect("encode");
        assert_eq!(encoded, "\"18446744073709551615\"");
        assert_eq!(
            serde_json::from_str::<RuntimeWireU64>(&encoded).expect("decode"),
            RuntimeWireU64(u64::MAX)
        );
        for invalid in [
            "\"00\"",
            "\"01\"",
            "\"-1\"",
            "1",
            "\"18446744073709551616\"",
        ] {
            assert!(
                serde_json::from_str::<RuntimeWireU64>(invalid).is_err(),
                "{invalid}"
            );
        }
    }
}
