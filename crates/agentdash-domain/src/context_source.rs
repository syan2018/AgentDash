use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ContextSourceKind {
    ManualText,
    File,
    ProjectSnapshot,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ContextSlot {
    Requirements,
    Constraints,
    Codebase,
    #[default]
    References,
    InstructionAppend,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ContextDelivery {
    Inline,
    #[default]
    Resource,
    Lazy,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ContextSourceRef {
    pub kind: ContextSourceKind,
    pub locator: String,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub slot: ContextSlot,
    #[serde(default)]
    pub priority: i32,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub max_chars: Option<usize>,
    #[serde(default)]
    pub delivery: ContextDelivery,
}
