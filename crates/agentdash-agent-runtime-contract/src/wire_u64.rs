use std::borrow::Cow;

use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::{Deserialize, Deserializer, Serialize, Serializer, de};
use ts_rs::TS;

pub const CANONICAL_U64_PATTERN: &str = "^(0|[1-9][0-9]{0,19})$";

/// Raw Managed Runtime wire representation of a semantic Rust `u64`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, TS)]
#[ts(type = "string & { readonly __runtime_u64: \"canonical_unsigned_decimal\" }")]
pub struct RuntimeU64(pub u64);

impl Serialize for RuntimeU64 {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serialize(&self.0, serializer)
    }
}

impl<'de> Deserialize<'de> for RuntimeU64 {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserialize(deserializer).map(Self)
    }
}

impl JsonSchema for RuntimeU64 {
    fn schema_name() -> Cow<'static, str> {
        "RuntimeU64".into()
    }

    fn json_schema(_generator: &mut SchemaGenerator) -> Schema {
        canonical_u64_schema("canonical unsigned-decimal Runtime coordinate")
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

fn canonical_u64_schema(description: &str) -> Schema {
    json_schema!({
        "type": "string",
        "description": description,
        "pattern": CANONICAL_U64_PATTERN,
        "maxLength": 20
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_unsigned_decimal_covers_full_u64_domain() {
        for (literal, expected) in [
            ("0", Some(0)),
            ("18446744073709551615", Some(u64::MAX)),
            ("00", None),
            ("01", None),
            ("-1", None),
            ("18446744073709551616", None),
        ] {
            assert_eq!(parse(literal).ok(), expected, "{literal}");
        }
    }

    #[test]
    fn json_number_is_rejected_and_max_round_trips() {
        assert!(serde_json::from_str::<RuntimeU64>("1").is_err());
        let encoded = serde_json::to_string(&RuntimeU64(u64::MAX)).expect("serialize max");
        assert_eq!(encoded, "\"18446744073709551615\"");
        assert_eq!(
            serde_json::from_str::<RuntimeU64>(&encoded).expect("deserialize max"),
            RuntimeU64(u64::MAX)
        );
    }
}
