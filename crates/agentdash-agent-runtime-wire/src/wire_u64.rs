use std::borrow::Cow;

use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::{Deserialize, Deserializer, Serialize, Serializer, de};
use ts_rs::TS;

pub const CANONICAL_U64_PATTERN: &str = concat!(
    "^(0|[1-9][0-9]{0,18}|1[0-7][0-9]{18}|18[0-3][0-9]{17}|",
    "184[0-3][0-9]{16}|1844[0-5][0-9]{15}|18446[0-6][0-9]{14}|",
    "184467[0-3][0-9]{13}|1844674[0-3][0-9]{12}|184467440[0-6][0-9]{10}|",
    "1844674407[0-2][0-9]{9}|18446744073[0-6][0-9]{8}|",
    "1844674407370[0-8][0-9]{6}|18446744073709[0-4][0-9]{5}|",
    "184467440737095[0-4][0-9]{4}|18446744073709550[0-9]{3}|",
    "18446744073709551[0-5][0-9]{2}|1844674407370955160[0-9]{1}|",
    "1844674407370955161[0-4]|18446744073709551615)$"
);

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

    #[test]
    fn schema_pattern_has_an_explicit_u64_upper_bound() {
        assert!(CANONICAL_U64_PATTERN.contains("18446744073709551615"));
        assert!(!CANONICAL_U64_PATTERN.contains("[0-9]{0,19}"));
    }
}
