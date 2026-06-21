use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

use crate::agent_run_mailbox::{MailboxMessageView, MailboxStateView};
use crate::shared_library::InstalledAssetSourceDto;
use crate::vfs::ResolvedVfsSurface;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DefinitionSource {
    BuiltinSeed,
    UserAuthored,
    Cloned,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ValidationSeverity {
    Error,
    Warning,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowTargetKind {
    Project,
    Story,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityScopeDto {
    Project,
    Story,
    Task,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ToolClusterDto {
    Read,
    Write,
    Execute,
    Workflow,
    Collaboration,
    Task,
    WorkspaceModule,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PlatformMcpScopeDto {
    Relay,
    Story,
    Workflow,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ToolSourceDto {
    Platform { cluster: ToolClusterDto },
    PlatformMcp { scope: PlatformMcpScopeDto },
    Mcp { server_name: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
pub struct ToolDescriptorDto {
    pub name: String,
    pub display_name: String,
    pub description: String,
    pub source: ToolSourceDto,
    pub capability_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
pub struct CapabilityCatalogEntryDto {
    pub key: String,
    pub label: String,
    pub description: String,
    pub allowed_scopes: Vec<CapabilityScopeDto>,
    pub auto_granted: bool,
    pub agent_can_grant: bool,
    pub workflow_can_grant: bool,
    pub tools: Vec<ToolDescriptorDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
pub struct CapabilityCatalogResponse {
    pub capabilities: Vec<CapabilityCatalogEntryDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
pub struct ValidationIssue {
    pub code: String,
    pub message: String,
    pub field_path: String,
    pub severity: ValidationSeverity,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
pub struct WorkflowContextBinding {
    pub locator: String,
    pub reason: String,
    #[serde(default = "bool_true")]
    pub required: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub title: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq, Default)]
pub struct WorkflowInjectionSpec {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub guidance: Option<String>,
    #[serde(default)]
    pub context_bindings: Vec<WorkflowContextBinding>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowHookTrigger {
    UserPromptSubmit,
    BeforeTool,
    AfterTool,
    AfterTurn,
    BeforeStop,
    SessionTerminal,
    BeforeSubagentDispatch,
    AfterSubagentDispatch,
    CompanionResult,
    BeforeCompact,
    AfterCompact,
    BeforeProviderRequest,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
pub struct WorkflowHookRuleSpec {
    pub key: String,
    pub trigger: WorkflowHookTrigger,
    #[serde(default)]
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub preset: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub params: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub script: Option<String>,
    #[serde(default = "bool_true")]
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq, Default)]
pub struct CapabilityConfig {
    #[serde(default)]
    pub tool_directives: Vec<ToolCapabilityDirective>,
    #[serde(default)]
    #[ts(type = "Array<unknown>")]
    pub mount_directives: Vec<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[serde(transparent)]
#[ts(type = "string")]
pub struct ToolCapabilityPath(pub String);

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ToolCapabilityDirective {
    Add(ToolCapabilityPath),
    Remove(ToolCapabilityPath),
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StandaloneFulfillment {
    #[default]
    Required,
    Optional {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        default_value: Option<String>,
    },
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GateStrategy {
    #[default]
    Existence,
    Schema,
    LlmJudge,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ContextStrategy {
    #[default]
    Full,
    Summary,
    MetadataOnly,
    Custom,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
pub struct OutputPortDefinition {
    pub key: String,
    pub description: String,
    #[serde(default)]
    pub gate_strategy: GateStrategy,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub gate_params: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
pub struct InputPortDefinition {
    pub key: String,
    pub description: String,
    #[serde(default)]
    pub context_strategy: ContextStrategy,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub context_template: Option<String>,
    #[serde(default)]
    pub standalone_fulfillment: StandaloneFulfillment,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq, Default)]
pub struct AgentProcedureContract {
    #[serde(default)]
    pub injection: WorkflowInjectionSpec,
    #[serde(default)]
    pub hook_rules: Vec<WorkflowHookRuleSpec>,
    #[serde(default)]
    pub capability_config: CapabilityConfig,
    #[serde(default)]
    pub output_ports: Vec<OutputPortDefinition>,
    #[serde(default)]
    pub input_ports: Vec<InputPortDefinition>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub struct AgentProcedureResponse {
    pub id: String,
    pub project_id: String,
    pub key: String,
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub target_kinds: Vec<WorkflowTargetKind>,
    pub source: DefinitionSource,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub installed_source: Option<InstalledAssetSourceDto>,
    pub version: i32,
    pub contract: AgentProcedureContract,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub struct WorkflowGraphResponse {
    pub id: String,
    pub project_id: String,
    pub key: String,
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub target_kinds: Vec<WorkflowTargetKind>,
    pub source: DefinitionSource,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub installed_source: Option<InstalledAssetSourceDto>,
    pub version: i32,
    pub entry_activity_key: String,
    pub activities: Vec<ActivityDefinition>,
    #[serde(default)]
    pub transitions: Vec<ActivityTransition>,
    pub created_at: String,
    pub updated_at: String,
}

impl CapabilityConfig {
    pub fn is_empty(&self) -> bool {
        self.tool_directives.is_empty() && self.mount_directives.is_empty()
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowSessionTerminalState {
    Completed,
    Failed,
    Interrupted,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq, Default)]
pub struct EffectiveSessionContract {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub lifecycle_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub active_activity_key: Option<String>,
    #[serde(default)]
    pub injection: WorkflowInjectionSpec,
    #[serde(default)]
    pub hook_rules: Vec<WorkflowHookRuleSpec>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
pub struct ActivityDefinition {
    pub key: String,
    #[serde(default)]
    pub description: String,
    pub executor: ActivityExecutorSpec,
    #[serde(default)]
    pub input_ports: Vec<InputPortDefinition>,
    #[serde(default)]
    pub output_ports: Vec<OutputPortDefinition>,
    #[serde(default)]
    pub completion_policy: ActivityCompletionPolicy,
    #[serde(default)]
    pub iteration_policy: ActivityIterationPolicy,
    #[serde(default)]
    pub join_policy: ActivityJoinPolicy,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ActivityExecutorSpec {
    Agent(AgentActivityExecutorSpec),
    Function(FunctionActivityExecutorSpec),
    Human(HumanActivityExecutorSpec),
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
pub struct AgentActivityExecutorSpec {
    pub procedure_key: String,
    pub agent_reuse_policy: AgentReusePolicy,
    pub runtime_session_policy: RuntimeSessionPolicy,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentReusePolicy {
    #[default]
    CreateActivityAgent,
    ContinueCurrentAgent,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeSessionPolicy {
    #[default]
    CreateNew,
    DeliverToCurrentTrace,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum FunctionActivityExecutorSpec {
    ApiRequest(ApiRequestExecutorSpec),
    BashExec(BashExecExecutorSpec),
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
pub struct ApiRequestExecutorSpec {
    pub method: String,
    pub url_template: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub body_template: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
pub struct BashExecExecutorSpec {
    pub command: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub working_directory: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum HumanActivityExecutorSpec {
    Approval(HumanApprovalExecutorSpec),
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
pub struct HumanApprovalExecutorSpec {
    pub form_schema_key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub title: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ActivityCompletionPolicy {
    OutputPorts {
        required_ports: Vec<String>,
    },
    #[default]
    ExecutorTerminal,
    HumanDecision {
        decision_port: String,
    },
    HookGate {
        hook_key: String,
    },
    OpenEnded,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, PartialEq, Eq)]
pub struct ActivityIterationPolicy {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub max_attempts: Option<u32>,
    #[serde(default)]
    pub artifact_alias: ArtifactAliasPolicy,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactAliasPolicy {
    #[default]
    Latest,
    PerAttempt,
    LatestAndHistory,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ActivityJoinPolicy {
    #[default]
    All,
    Any,
    First,
    NOfM {
        n: u32,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
pub struct ActivityTransition {
    pub from: String,
    pub to: String,
    #[serde(default = "default_activity_transition_kind")]
    pub kind: ActivityTransitionKind,
    #[serde(default)]
    pub condition: TransitionCondition,
    #[serde(default)]
    pub artifact_bindings: Vec<ArtifactBinding>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub max_traversals: Option<u32>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ActivityTransitionKind {
    Flow,
    Artifact,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TransitionCondition {
    #[default]
    Always,
    ArtifactFieldEquals {
        activity: String,
        port: String,
        path: String,
        value: Value,
    },
    HumanDecisionEquals {
        activity: String,
        decision_port: String,
        value: String,
    },
    AgentSignalEquals {
        activity: String,
        signal_key: String,
        value: Value,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
pub struct ArtifactBinding {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub from_activity: Option<String>,
    pub from_port: String,
    pub to_port: String,
    #[serde(default)]
    pub alias: ArtifactAliasPolicy,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
pub struct SubmitOrchestrationHumanDecisionRequest {
    pub orchestration_id: String,
    pub node_path: String,
    #[serde(default = "default_attempt")]
    pub attempt: u32,
    pub decision: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub resolved_by: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct SubmitOrchestrationHumanDecisionResponse {
    pub run: LifecycleRunView,
    pub gate_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub struct ContinueLifecycleRunResponse {
    pub run: LifecycleRunView,
    pub drain_result: OrchestrationExecutorDrainResultDto,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub struct OrchestrationExecutorDrainResultDto {
    #[serde(default)]
    pub launched_agent_nodes: Vec<LaunchedAgentNodeDto>,
    #[serde(default)]
    pub opened_human_gates: Vec<OpenedHumanGateDto>,
    #[serde(default)]
    pub completed_effect_nodes: Vec<String>,
    #[serde(default)]
    pub failed_nodes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct LaunchedAgentNodeDto {
    pub run_id: String,
    pub orchestration_id: String,
    pub node_path: String,
    pub attempt: u32,
    pub runtime_session_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct OpenedHumanGateDto {
    pub run_id: String,
    pub orchestration_id: String,
    pub node_path: String,
    pub attempt: u32,
    pub gate_id: String,
}

fn default_attempt() -> u32 {
    1
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ExecutorRunRef {
    RuntimeSession { session_id: String },
    FunctionRun { run_id: String },
    HumanDecision { decision_id: String },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LifecycleRunStatus {
    Draft,
    Ready,
    Running,
    Blocked,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LifecycleExecutionEventKind {
    ActivityActivated,
    ActivityCompleted,
    ConstraintBlocked,
    CompletionEvaluated,
    ArtifactAppended,
    ContextInjected,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
pub struct LifecycleExecutionEntry {
    pub timestamp: DateTime<Utc>,
    pub activity_key: String,
    pub event_kind: LifecycleExecutionEventKind,
    pub summary: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub detail: Option<Value>,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LifecycleNodeType {
    #[default]
    AgentNode,
    PhaseNode,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct DeleteWorkflowGraphResponse {
    pub deleted: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct DeleteAgentProcedureResponse {
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
#[serde(rename_all = "snake_case")]
pub struct PreflightWorkflowScriptRequest {
    pub project_id: String,
    pub source_text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "JsonValue")]
    pub args: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "JsonValue")]
    pub ctx: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub runtime_session_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub struct WorkflowScriptPreflightDiagnosticDto {
    pub code: String,
    pub severity: ValidationSeverity,
    pub message: String,
    pub source_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub struct WorkflowScriptPlanPreviewNodeDto {
    pub node_id: String,
    pub node_path: String,
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub label: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub struct WorkflowScriptPlanPreviewDto {
    pub plan_digest: String,
    pub node_count: usize,
    pub entry_node_ids: Vec<String>,
    pub nodes: Vec<WorkflowScriptPlanPreviewNodeDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub struct WorkflowScriptApiEndpointDto {
    pub method: String,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub struct WorkflowScriptBashCommandDto {
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub working_directory: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub struct WorkflowScriptHumanGateCapabilityDto {
    pub name: String,
    pub form_schema: String,
    pub decision_port: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub struct WorkflowScriptCapabilitySummaryDto {
    #[serde(default)]
    pub agent_procedure_keys: Vec<String>,
    #[serde(default)]
    pub function_api_endpoints: Vec<WorkflowScriptApiEndpointDto>,
    #[serde(default)]
    pub local_effect_capabilities: Vec<String>,
    #[serde(default)]
    pub bash_commands: Vec<WorkflowScriptBashCommandDto>,
    #[serde(default)]
    pub human_gates: Vec<WorkflowScriptHumanGateCapabilityDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub struct PreflightWorkflowScriptResponse {
    pub valid: bool,
    pub source_digest: String,
    #[ts(type = "JsonValue")]
    pub source_ref: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "JsonValue")]
    pub raw_builder_document: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "JsonValue")]
    pub plan_snapshot: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub plan_preview: Option<WorkflowScriptPlanPreviewDto>,
    pub capability_summary: WorkflowScriptCapabilitySummaryDto,
    pub diagnostics: Vec<WorkflowScriptPreflightDiagnosticDto>,
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

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub struct SubjectRefDto {
    pub kind: String,
    pub id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub struct LifecycleRunRefDto {
    pub run_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub struct AgentRunRefDto {
    pub run_id: String,
    pub agent_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub struct AgentFrameRefDto {
    pub agent_id: String,
    pub frame_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub revision: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub struct RuntimeSessionRefDto {
    pub runtime_session_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub struct SessionShellDto {
    pub id: String,
    pub title: String,
    pub title_source: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub last_event_seq: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub last_turn_id: Option<String>,
    pub last_delivery_status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub struct RuntimeSessionExecutionAnchorDto {
    pub runtime_session_id: String,
    pub run_id: String,
    pub agent_id: String,
    pub launch_frame_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub orchestration_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub node_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub node_attempt: Option<u32>,
    pub created_by_kind: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub struct RuntimeSessionTraceMeta {
    pub runtime_session_ref: RuntimeSessionRefDto,
    pub last_event_seq: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub executor_session_id: Option<String>,
    pub trace_title: String,
    pub trace_title_source: String,
    pub delivery_status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub last_turn_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub terminal_summary: Option<String>,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub struct AgentRunWorkspaceShell {
    pub display_title: String,
    pub title_source: String,
    pub workspace_status: String,
    pub delivery_status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub last_turn_id: Option<String>,
    pub last_activity_at: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentRunWorkspaceControlPlaneStatus {
    Ready,
    Running,
    Cancelling,
    Terminal,
    FrameMissing,
    DeliveryMissing,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub struct AgentRunWorkspaceControlPlaneView {
    pub status: AgentRunWorkspaceControlPlaneStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ConversationExecutionStatus {
    Draft,
    ModelRequired,
    Ready,
    StartingClaimed,
    RunningActive,
    Cancelling,
    Terminal,
    FrameMissing,
    DeliveryMissing,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ConversationModelConfigStatus {
    Resolved,
    ModelRequired,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ConversationModelConfigSource {
    ProjectAgentPreset,
    FrameExecutionProfile,
    UserOverride,
    ExecutorDiscoveryDefault,
    Unspecified,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct ConversationEffectiveExecutorConfigView {
    pub executor: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub provider_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub model_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub agent_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub thinking_level: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub permission_policy: Option<String>,
    pub source: ConversationModelConfigSource,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct ConversationModelConfigView {
    pub status: ConversationModelConfigStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub effective_executor_config: Option<ConversationEffectiveExecutorConfigView>,
    #[serde(default)]
    pub missing_fields: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub message: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ConversationCommandKind {
    StartDraft,
    SubmitMessage,
    PromoteMailboxMessage,
    DeleteMailboxMessage,
    ResumeMailbox,
    Cancel,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ConversationCommandPlacement {
    ComposerPrimary,
    ComposerSecondary,
    MailboxRow,
    MailboxBanner,
    Header,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct ConversationCommandStaleGuardView {
    pub snapshot_id: String,
    pub run_id: String,
    pub agent_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub frame_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub runtime_session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub active_turn_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct AgentRunCommandPreconditionView {
    pub command_id: String,
    pub command_kind: ConversationCommandKind,
    pub stale_guard: ConversationCommandStaleGuardView,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct ConversationCommandView {
    pub kind: ConversationCommandKind,
    pub command_id: String,
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub unavailable_reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub disabled_code: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub shortcut: Option<String>,
    pub requires_input: bool,
    pub executor_config_policy: String,
    #[serde(default)]
    pub placement: Vec<ConversationCommandPlacement>,
    pub stale_guard: ConversationCommandStaleGuardView,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct ConversationKeyboardMapView {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub enter: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub ctrl_enter: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct ConversationCommandSetView {
    #[serde(default)]
    pub commands: Vec<ConversationCommandView>,
    pub keyboard: ConversationKeyboardMapView,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub struct ConversationExecutionView {
    pub status: ConversationExecutionStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub runtime_session_ref: Option<RuntimeSessionRefDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub active_turn_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub struct ConversationMailboxSnapshotView {
    pub visible_message_count: usize,
    pub paused: bool,
    pub user_attention: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub resume_command: Option<ConversationCommandView>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub state: Option<MailboxStateView>,
    #[serde(default)]
    pub messages: Vec<MailboxMessageView>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct ConversationDiagnosticView {
    pub code: String,
    pub severity: ValidationSeverity,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "JsonValue")]
    pub detail: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub struct AgentConversationIdentity {
    pub run_ref: LifecycleRunRefDto,
    pub agent_ref: AgentRunRefDto,
    pub project_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub struct AgentConversationLifecycleContext {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub frame_ref: Option<AgentFrameRefDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub delivery_runtime_ref: Option<RuntimeSessionRefDto>,
    #[serde(default)]
    pub subject_associations: Vec<LifecycleSubjectAssociationDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub struct AgentConversationSnapshot {
    pub snapshot_id: String,
    pub identity: AgentConversationIdentity,
    pub lifecycle_context: AgentConversationLifecycleContext,
    pub execution: ConversationExecutionView,
    pub model_config: ConversationModelConfigView,
    pub commands: ConversationCommandSetView,
    pub mailbox: ConversationMailboxSnapshotView,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub resource_surface: Option<ResolvedVfsSurface>,
    #[serde(default)]
    pub diagnostics: Vec<ConversationDiagnosticView>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub struct AgentRunWorkspaceView {
    pub run_ref: LifecycleRunRefDto,
    pub agent_ref: AgentRunRefDto,
    pub project_id: String,
    pub shell: AgentRunWorkspaceShell,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub delivery_runtime_ref: Option<RuntimeSessionRefDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub delivery_trace_meta: Option<RuntimeSessionTraceMeta>,
    pub control_plane: AgentRunWorkspaceControlPlaneView,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub agent: Option<AgentRunView>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub frame_runtime: Option<AgentFrameRuntimeView>,
    #[serde(default)]
    pub subject_associations: Vec<LifecycleSubjectAssociationDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub resource_surface: Option<ResolvedVfsSurface>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub conversation: Option<AgentConversationSnapshot>,
    /// lineage 父节点：本 Run 若为 subagent 则指向其父，供"隶属于"跳转。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub parent: Option<AgentRunLineageRef>,
    /// 本 Run 直接派发的 subagent（一跳子节点），供右侧展开/下钻。
    #[serde(default)]
    pub children: Vec<AgentRunLineageRef>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub struct LifecycleSubjectAssociationDto {
    pub id: String,
    pub anchor_run_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub anchor_agent_id: Option<String>,
    pub subject_ref: SubjectRefDto,
    pub role: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub metadata: Option<Value>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub struct RuntimeNodeView {
    pub node_id: String,
    pub node_path: String,
    pub kind: String,
    pub status: String,
    pub attempt: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub executor_run_ref: Option<ExecutorRunRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub started_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub completed_at: Option<String>,
    #[serde(default)]
    pub children: Vec<RuntimeNodeView>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub struct ActiveRuntimeNodeRefDto {
    pub run_id: String,
    pub orchestration_id: String,
    pub node_path: String,
    pub attempt: u32,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub struct OrchestrationInstanceView {
    pub orchestration_id: String,
    pub role: String,
    pub status: String,
    pub plan_digest: String,
    pub source_ref: Value,
    #[serde(default)]
    pub ready_node_ids: Vec<String>,
    #[serde(default)]
    pub nodes: Vec<RuntimeNodeView>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub struct AgentRunView {
    pub agent_ref: AgentRunRefDto,
    pub project_id: String,
    /// Agent 创建/启动来源（标准化枚举 slug，取代原 `agent_kind`）。
    pub source: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub project_agent_id: Option<String>,
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub current_frame_id: Option<String>,
    /// 投递用的 runtime session（由 execution anchor 提供）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub delivery_runtime_ref: Option<RuntimeSessionRefDto>,
    /// agent 最新 execution status（如 running / completed / idle）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub last_delivery_status: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LifecycleRunTopology {
    Plain,
    WorkflowGraph,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub struct LifecycleRunView {
    pub run_ref: LifecycleRunRefDto,
    pub project_id: String,
    pub topology: LifecycleRunTopology,
    pub status: LifecycleRunStatus,
    #[serde(default)]
    pub orchestrations: Vec<OrchestrationInstanceView>,
    #[serde(default)]
    pub active_runtime_node_refs: Vec<ActiveRuntimeNodeRefDto>,
    #[serde(default)]
    pub agents: Vec<AgentRunView>,
    #[serde(default)]
    pub subject_associations: Vec<LifecycleSubjectAssociationDto>,
    #[serde(default)]
    pub runtime_trace_refs: Vec<RuntimeSessionRefDto>,
    #[serde(default)]
    pub execution_log: Vec<LifecycleExecutionEntry>,
    pub created_at: String,
    pub updated_at: String,
    pub last_activity_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub struct AgentFrameRuntimeView {
    pub frame_ref: AgentFrameRefDto,
    #[serde(default)]
    pub capability_surface: Value,
    #[serde(default)]
    pub context_slice: Value,
    #[serde(default)]
    pub vfs_surface: Value,
    #[serde(default)]
    pub mcp_surface: Value,
    #[serde(default)]
    pub runtime_session_refs: Vec<RuntimeSessionRefDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub execution_profile: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub effective_executor_config: Option<ConversationEffectiveExecutorConfigView>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub struct SubjectExecutionView {
    pub subject_ref: SubjectRefDto,
    #[serde(default)]
    pub associations: Vec<LifecycleSubjectAssociationDto>,
    #[serde(default)]
    pub runs: Vec<LifecycleRunView>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub current_agent: Option<AgentRunView>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub latest_runtime_node: Option<RuntimeNodeView>,
    #[serde(default)]
    pub artifacts: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub struct RuntimeSessionTraceView {
    pub runtime_session_ref: RuntimeSessionRefDto,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub frame_ref: Option<AgentFrameRefDto>,
    #[serde(default)]
    pub events: Vec<Value>,
    #[serde(default)]
    pub turns: Vec<Value>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SessionRuntimeControlPlaneStatus {
    UnboundTrace,
    AnchoredIdle,
    AnchoredRunning,
    AnchoredCancelling,
    Terminal,
    FrameMissing,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub struct SessionRuntimeControlPlaneView {
    pub status: SessionRuntimeControlPlaneStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub struct SessionRuntimeControlView {
    pub runtime_session_ref: RuntimeSessionRefDto,
    pub session_meta: SessionShellDto,
    pub control_plane: SessionRuntimeControlPlaneView,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub anchor: Option<RuntimeSessionExecutionAnchorDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub run: Option<LifecycleRunView>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub agent: Option<AgentRunView>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub frame_runtime: Option<AgentFrameRuntimeView>,
    #[serde(default)]
    pub subject_associations: Vec<LifecycleSubjectAssociationDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub struct AgentRunCommandOnlyRequest {
    pub command: AgentRunCommandPreconditionView,
    pub client_command_id: String,
}

/// AgentRun lineage 控制树上的一跳引用（父或子）。
///
/// 用于右侧会话栏展示从属关系与跳转。`relation_kind` 来自 `AgentLineage`。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub struct AgentRunLineageRef {
    pub run_id: String,
    pub agent_id: String,
    /// Agent 创建/启动来源（标准化枚举 slug，取代原 `agent_kind`）。
    pub source: String,
    pub relation_kind: String,
    pub display_title: String,
    /// 该节点子树（传递闭包）下的 subagent 总数；前端据此决定是否显示展开箭头。
    #[serde(default)]
    pub subagent_count: u32,
}

/// AgentRun 列表内联的直接子 Agent 节点（一跳），携带真实 shell 状态，免前端懒加载。
///
/// 与 run 级 `AgentRunWorkspaceListEntry` 区分：子节点不持有 run_status / subject 等 run 级字段，
/// 仅承载渲染一行子 Agent 所需信息 + 自身子树规模（供「N sub」深层提示）。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub struct AgentRunListChild {
    pub run_ref: LifecycleRunRefDto,
    pub agent_ref: AgentRunRefDto,
    /// 面向用户的身份标识：绑定 Project Agent 的显示名（preset.display_name || name）。
    /// 未绑定 project agent（动态 companion 等）时为 None。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub project_agent_label: Option<String>,
    /// Agent 创建/启动来源（标准化枚举 slug）。
    #[serde(default)]
    pub source: String,
    /// 含 display_title / delivery_status / last_activity_at 等执行态。
    pub shell: AgentRunWorkspaceShell,
    /// 该子自身子树（传递闭包）下的 subagent 总数；前端据此决定是否显示展开开关。
    #[serde(default)]
    pub subagent_count: u32,
    /// 递归内联的下一层直接子 Agent，支持列表内任意深度展开（深度上限兜底）。
    #[serde(default)]
    pub children: Vec<AgentRunListChild>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub delivery_runtime_ref: Option<RuntimeSessionRefDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub struct AgentRunWorkspaceListEntry {
    pub run_ref: LifecycleRunRefDto,
    pub agent_ref: AgentRunRefDto,
    pub project_id: String,
    pub shell: AgentRunWorkspaceShell,
    pub run_status: LifecycleRunStatus,
    /// 面向用户的身份标识：绑定 Project Agent 的显示名（preset.display_name || name）。
    /// 未绑定 project agent 时为 None。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub project_agent_label: Option<String>,
    /// Agent 创建/启动来源（标准化枚举 slug），供列表行展示来源标签。
    #[serde(default)]
    pub source: String,
    /// 该主 Run 子树（传递闭包）下的 subagent 总数，0 表示无子。
    #[serde(default)]
    pub subagent_count: u32,
    /// 该主 Run 的直接子 Agent（一跳），已内联 shell 状态，前端免懒加载。
    #[serde(default)]
    pub children: Vec<AgentRunListChild>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub delivery_runtime_ref: Option<RuntimeSessionRefDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub delivery_trace_meta: Option<RuntimeSessionTraceMeta>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub frame_ref: Option<AgentFrameRefDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub subject_ref: Option<SubjectRefDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub subject_label: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub struct AgentRunWorkspaceListView {
    pub project_id: String,
    pub agent_runs: Vec<AgentRunWorkspaceListEntry>,
    /// 下一页游标（keyset，不透明）；None 表示已到尾页。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub struct ProjectActiveAgentsView {
    pub project_id: String,
    pub runs: Vec<LifecycleRunView>,
    pub agents: Vec<AgentRunView>,
}

fn bool_true() -> bool {
    true
}

fn default_activity_transition_kind() -> ActivityTransitionKind {
    ActivityTransitionKind::Flow
}
