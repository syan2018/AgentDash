use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct OperationProviderRef {
    pub namespace: String,
    pub provider_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct OperationRef {
    pub provider: OperationProviderRef,
    pub operation_key: String,
    pub contract_version: u16,
}

impl OperationRef {
    pub fn new(
        namespace: impl Into<String>,
        provider_key: impl Into<String>,
        operation_key: impl Into<String>,
        contract_version: u16,
    ) -> Result<Self, OperationRefError> {
        let operation_ref = Self {
            provider: OperationProviderRef {
                namespace: namespace.into(),
                provider_key: provider_key.into(),
            },
            operation_key: operation_key.into(),
            contract_version,
        };
        operation_ref.validate()?;
        Ok(operation_ref)
    }

    pub fn validate(&self) -> Result<(), OperationRefError> {
        validate_segment("provider.namespace", &self.provider.namespace)?;
        validate_segment("provider.provider_key", &self.provider.provider_key)?;
        validate_segment("operation_key", &self.operation_key)?;
        if self.contract_version == 0 {
            return Err(OperationRefError::InvalidVersion);
        }
        Ok(())
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum OperationRefError {
    #[error("OperationRef 字段无效: {field}")]
    InvalidSegment { field: &'static str },
    #[error("OperationRef contract_version 必须大于 0")]
    InvalidVersion,
}

fn validate_segment(field: &'static str, value: &str) -> Result<(), OperationRefError> {
    let valid = !value.is_empty()
        && value.trim() == value
        && value.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '_' | '-' | '.')
        });
    if valid {
        Ok(())
    } else {
        Err(OperationRefError::InvalidSegment { field })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn operation_ref_is_provider_qualified_and_versioned() {
        let operation_ref = OperationRef::new("extension", "acme.weather", "forecast", 1)
            .expect("valid operation ref");
        assert_eq!(operation_ref.provider.namespace, "extension");
        assert_eq!(operation_ref.contract_version, 1);
    }

    #[test]
    fn operation_ref_rejects_unstable_segments() {
        assert!(matches!(
            OperationRef::new("extension", "bad provider", "forecast", 1),
            Err(OperationRefError::InvalidSegment { .. })
        ));
    }
}
