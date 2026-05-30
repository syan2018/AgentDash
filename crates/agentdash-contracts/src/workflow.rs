use serde::{Deserialize, Serialize};
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
    LifecycleExecutionEventKind, LifecycleNodeType, LifecycleRunStatus,
    OutputPortDefinition, StandaloneFulfillment, ToolCapabilityDirective, ToolCapabilityPath,
    TransitionCondition, ValidationIssue, ValidationSeverity, WorkflowBindingKind,
    WorkflowContextBinding, WorkflowContract, WorkflowDefinitionSource, WorkflowHookRuleSpec,
    WorkflowHookTrigger, WorkflowInjectionSpec, WorkflowSessionTerminalState,
};

// ─── Run-oriented DTOs ──────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub struct LifecycleRunLinkDto {
    pub id: String,
    pub run_id: String,
    pub subject_kind: String,
    pub subject_id: String,
    pub role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub metadata: Option<serde_json::Value>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub struct StoryRunOverviewDto {
    pub id: String,
    pub lifecycle_id: String,
    pub status: LifecycleRunStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub session_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub last_activity_at: String,
    pub links: Vec<LifecycleRunLinkDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub struct StoryRunsResponse {
    pub story_id: String,
    pub runs: Vec<StoryRunOverviewDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub struct RunLinksResponse {
    pub run_id: String,
    pub links: Vec<LifecycleRunLinkDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub struct AttachRunLinkRequest {
    pub subject_kind: String,
    pub subject_id: String,
    pub role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub metadata: Option<serde_json::Value>,
}
