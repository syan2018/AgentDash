use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowDefinitionSource {
    BuiltinSeed,
    UserAuthored,
    Cloned,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ValidationSeverity {
    Error,
    Warning,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ValidationIssue {
    pub code: String,
    pub message: String,
    pub field_path: String,
    pub severity: ValidationSeverity,
}

impl ValidationIssue {
    pub fn error(
        code: impl Into<String>,
        message: impl Into<String>,
        field_path: impl Into<String>,
    ) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            field_path: field_path.into(),
            severity: ValidationSeverity::Error,
        }
    }

    pub fn warning(
        code: impl Into<String>,
        message: impl Into<String>,
        field_path: impl Into<String>,
    ) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            field_path: field_path.into(),
            severity: ValidationSeverity::Warning,
        }
    }
}
