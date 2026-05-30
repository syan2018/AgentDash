use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkflowContextBinding {
    pub locator: String,
    pub reason: String,
    #[serde(default = "bool_true")]
    pub required: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct WorkflowInjectionSpec {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub guidance: Option<String>,
    #[serde(default)]
    pub context_bindings: Vec<WorkflowContextBinding>,
}

fn bool_true() -> bool {
    true
}
