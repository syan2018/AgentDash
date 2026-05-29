use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

pub use agentdash_domain::workflow::{
    ActivityAttemptState, ActivityAttemptStatus, ActivityCompletionPolicy, ActivityDefinition,
    ActivityExecutorSpec, ActivityInputArtifact, ActivityIterationPolicy, ActivityJoinPolicy,
    ActivityLifecycleRunState, ActivityOutputArtifact, ActivityPortValue, ActivityRunStatus,
    ActivityTransition, ActivityTransitionKind, AgentActivityExecutorSpec, AgentSessionPolicy,
    ApiRequestExecutorSpec, ArtifactAliasPolicy, ArtifactBinding, BashExecExecutorSpec,
    CapabilityConfig, ContextStrategy, EffectiveSessionContract, ExecutorRunRef,
    FunctionActivityExecutorSpec, GateStrategy, HumanActivityExecutorSpec,
    HumanApprovalExecutorSpec, InputPortDefinition, LifecycleExecutionEntry,
    LifecycleExecutionEventKind, LifecycleNodeType, LifecycleRunStatus, OutputPortDefinition,
    StandaloneFulfillment, ToolCapabilityDirective, ToolCapabilityPath, TransitionCondition,
    ValidationIssue, ValidationSeverity, WorkflowBindingKind, WorkflowContextBinding,
    WorkflowContract, WorkflowDefinitionSource, WorkflowHookRuleSpec, WorkflowHookTrigger,
    WorkflowInjectionSpec, WorkflowSessionTerminalState,
};

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct DeleteActivityLifecycleDefinitionResponse {
    pub deleted: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct DeleteWorkflowDefinitionResponse {
    pub deleted: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct HookPresetResponse {
    pub key: String,
    pub trigger: Value,
    pub label: String,
    pub description: String,
    pub param_schema: Value,
    pub script: String,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct HookPresetsResponse {
    pub presets: BTreeMap<String, Vec<HookPresetResponse>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct ValidateHookScriptResponse {
    pub valid: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub errors: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct RegisterHookPresetResponse {
    pub registered: bool,
    pub key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct DeleteHookPresetResponse {
    pub removed: bool,
    pub key: String,
}
