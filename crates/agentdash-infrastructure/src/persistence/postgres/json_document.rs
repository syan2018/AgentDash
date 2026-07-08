use serde::{Serialize, de::DeserializeOwned};
use serde_json::Value;

use agentdash_domain::common::error::DomainError;

pub(crate) fn to_jsonb<T: Serialize>(value: &T, field: &str) -> Result<Value, DomainError> {
    serde_json::to_value(value)
        .map_err(|error| DomainError::InvalidConfig(format!("{field}: {error}")))
}

pub(crate) fn to_optional_jsonb<T: Serialize>(
    value: Option<&T>,
    field: &str,
) -> Result<Option<Value>, DomainError> {
    value.map(|value| to_jsonb(value, field)).transpose()
}

pub(crate) fn from_jsonb<T: DeserializeOwned>(value: Value, field: &str) -> Result<T, DomainError> {
    serde_json::from_value(value)
        .map_err(|error| DomainError::InvalidConfig(format!("{field}: {error}")))
}

pub(crate) fn from_optional_jsonb<T: DeserializeOwned>(
    value: Option<Value>,
    field: &str,
) -> Result<Option<T>, DomainError> {
    value.map(|value| from_jsonb(value, field)).transpose()
}
