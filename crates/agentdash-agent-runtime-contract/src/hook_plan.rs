use std::collections::BTreeSet;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{
    HookAction, HookDefinitionId, HookFailurePolicy, HookPlanDigest, HookPlanRevision, HookPoint,
    RuntimeThreadId, SemanticStrength,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum HookExecutionSite {
    ManagedRuntime,
    ToolBroker,
    AgentCoreCallback,
    DriverNative,
    ObservedEventReaction,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
pub struct BoundRuntimeHookEntry {
    pub definition_id: HookDefinitionId,
    pub point: HookPoint,
    pub actions: BTreeSet<HookAction>,
    pub delivered_strength: SemanticStrength,
    pub failure_policy: HookFailurePolicy,
    pub required: bool,
    pub site: HookExecutionSite,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
pub struct BoundRuntimeHookPlan {
    pub revision: HookPlanRevision,
    pub digest: HookPlanDigest,
    pub entries: Vec<BoundRuntimeHookEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
pub struct RuntimeHookPlanBinding {
    pub thread_id: RuntimeThreadId,
    pub plan: BoundRuntimeHookPlan,
}
