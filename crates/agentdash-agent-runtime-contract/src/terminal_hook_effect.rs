use std::collections::BTreeSet;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{
    RuntimeHookEffectKind, RuntimeTerminalHookEffectHandlerId, RuntimeTerminalHookEffectHandlerType,
};

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema, TS,
)]
#[serde(transparent)]
pub struct RuntimeTerminalHookEffectHandlerRevision(pub u64);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct RuntimeTerminalHookEffectHandlerRef {
    pub handler_type: RuntimeTerminalHookEffectHandlerType,
    pub handler_id: RuntimeTerminalHookEffectHandlerId,
    pub revision: RuntimeTerminalHookEffectHandlerRevision,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct RuntimeTerminalHookEffectBinding {
    pub handler: RuntimeTerminalHookEffectHandlerRef,
    pub supported_effect_kinds: BTreeSet<RuntimeHookEffectKind>,
}
