use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{
    ContextCheckpointId, ContextDigest, ContextFidelity, ContextRecipeRevision, ContextRevision,
    RuntimeInput, RuntimeItemContent, RuntimeItemId, RuntimeThreadId, ThreadSettingsRevision,
    ToolSetRevision,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum ContextCompactionTrigger {
    Manual,
    Automatic,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ContextBlock {
    Instruction { text: String },
    Input { input: Vec<RuntimeInput> },
    RuntimeItem { content: RuntimeItemContent },
    CompactionSummary { summary: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct ContextProvenance {
    pub settings_revision: ThreadSettingsRevision,
    pub tool_set_revision: ToolSetRevision,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct ContextRecipe {
    pub revision: ContextRecipeRevision,
    pub provenance: ContextProvenance,
    pub source_item_ids: Vec<RuntimeItemId>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct MaterializedContext {
    pub recipe: ContextRecipe,
    pub blocks: Vec<ContextBlock>,
    pub digest: ContextDigest,
    pub fidelity: ContextFidelity,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct ContextCheckpointView {
    pub checkpoint_id: ContextCheckpointId,
    pub thread_id: RuntimeThreadId,
    pub revision: ContextRevision,
    pub materialized: MaterializedContext,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct ActiveContextHeadView {
    pub checkpoint_id: ContextCheckpointId,
    pub revision: ContextRevision,
    pub digest: ContextDigest,
    pub provenance: ContextProvenance,
    pub fidelity: ContextFidelity,
}
