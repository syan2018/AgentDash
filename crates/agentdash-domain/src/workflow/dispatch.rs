use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::lifecycle_subject_association::SubjectRef;

// ─── Policy Enums ────────────────────────────────────────────────────────────

/// 触发 dispatch 的来源类型。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionSource {
    User,
    Routine,
    ParentAgent,
    ProjectAgent,
    Api,
    Migration,
}

/// 决定 LifecycleRun 的复用策略。
///
/// - `ReuseExisting`: 复用 `parent_run_id` 指向的 run，不创建新 graph instance。
/// - `AppendGraph`: 复用 `parent_run_id` 指向的 run 并追加一个 WorkflowGraphInstance。
/// - `CreateLinkedRun`: 创建独立 LifecycleRun（新生命周期/上下文/控制边界）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RunPolicy {
    ReuseExisting,
    AppendGraph,
    CreateLinkedRun,
}

/// 决定 LifecycleAgent 的创建策略。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentPolicy {
    Create,
    Reuse,
    Resume,
    SpawnChild,
}

/// 上下文继承策略。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ContextPolicy {
    Inherit,
    Slice,
    Isolated,
}

/// Capability 授予策略。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityPolicy {
    Baseline,
    InheritedSlice,
    GrantConstrained,
}

/// RuntimeSession 创建/附加策略。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RuntimePolicy {
    CreateRuntimeSession,
    AttachExisting(Uuid),
    ContinueCurrent(Uuid),
}

/// Gate 创建策略与参数。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatePolicy {
    pub gate_kind: String,
    pub correlation_id: Option<String>,
    pub payload: Option<serde_json::Value>,
}

// ─── Ref Types ───────────────────────────────────────────────────────────────

/// 目标可执行图的引用——可按 ID 或 key 查找。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowGraphRef {
    ById(Uuid),
    ByKey { project_id: Uuid, key: String },
}

/// 单个 Agent Activity 的 procedure override 引用。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentProcedureRef {
    ById(Uuid),
    ByKey { project_id: Uuid, key: String },
}

/// Subject/agent/run 视图入口引用。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubjectExecutionRef {
    pub subject_ref: SubjectRef,
    pub association_id: Uuid,
}

// ─── Intent ──────────────────────────────────────────────────────────────────

/// 创建 / 复用 agent runtime surface。
///
/// `subject_ref` 只表达可选的 project/run control association；需要强制 subject
/// execution 语义时使用 `SubjectExecutionIntent`。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentLaunchIntent {
    pub project_id: Uuid,
    pub source: ExecutionSource,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subject_ref: Option<SubjectRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_run_id: Option<Uuid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_agent_id: Option<Uuid>,
    pub workflow_graph_ref: WorkflowGraphRef,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_procedure_ref: Option<AgentProcedureRef>,
    pub run_policy: RunPolicy,
    pub agent_policy: AgentPolicy,
    pub context_policy: ContextPolicy,
    pub capability_policy: CapabilityPolicy,
    pub runtime_policy: RuntimePolicy,
}

/// 以业务 SubjectRef 进入执行控制面。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubjectExecutionIntent {
    pub project_id: Uuid,
    pub source: ExecutionSource,
    pub subject_ref: SubjectRef,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_run_id: Option<Uuid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_agent_id: Option<Uuid>,
    pub workflow_graph_ref: WorkflowGraphRef,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_procedure_ref: Option<AgentProcedureRef>,
    pub run_policy: RunPolicy,
    pub agent_policy: AgentPolicy,
    pub context_policy: ContextPolicy,
    pub capability_policy: CapabilityPolicy,
    pub runtime_policy: RuntimePolicy,
}

/// 只启动 tracked lifecycle process + root graph instance。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LifecycleRunStartIntent {
    pub project_id: Uuid,
    pub source: ExecutionSource,
    pub workflow_graph_ref: WorkflowGraphRef,
}

/// 创建交互 gate，并可选创建 child agent surface。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InteractionDispatchIntent {
    pub project_id: Uuid,
    pub source: ExecutionSource,
    pub parent_run_id: Uuid,
    pub parent_agent_id: Uuid,
    pub workflow_graph_ref: WorkflowGraphRef,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_procedure_ref: Option<AgentProcedureRef>,
    pub context_policy: ContextPolicy,
    pub capability_policy: CapabilityPolicy,
    pub runtime_policy: RuntimePolicy,
    pub gate_policy: GatePolicy,
}

/// 业务执行进入控制面的分类入口。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", content = "intent", rename_all = "snake_case")]
pub enum ExecutionIntent {
    AgentLaunch(AgentLaunchIntent),
    SubjectExecution(SubjectExecutionIntent),
    LifecycleRunStart(LifecycleRunStartIntent),
    InteractionDispatch(InteractionDispatchIntent),
}

// ─── Result ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentLaunchDispatchResult {
    pub run_ref: Uuid,
    pub graph_instance_ref: Uuid,
    pub agent_ref: Uuid,
    pub frame_ref: Uuid,
    pub assignment_ref: Uuid,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime_session_ref: Option<Uuid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trace_ref: Option<Uuid>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubjectExecutionDispatchResult {
    pub run_ref: Uuid,
    pub graph_instance_ref: Uuid,
    pub agent_ref: Uuid,
    pub frame_ref: Uuid,
    pub assignment_ref: Uuid,
    pub subject_execution_ref: SubjectExecutionRef,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime_session_ref: Option<Uuid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trace_ref: Option<Uuid>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LifecycleRunStartDispatchResult {
    pub run_ref: Uuid,
    pub graph_instance_ref: Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InteractionGateOpenedDispatchResult {
    pub run_ref: Uuid,
    pub graph_instance_ref: Uuid,
    pub agent_ref: Uuid,
    pub frame_ref: Uuid,
    pub assignment_ref: Uuid,
    pub gate_ref: Uuid,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime_session_ref: Option<Uuid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trace_ref: Option<Uuid>,
}

/// Dispatch 调度结果按 intent family 分类，避免全 optional DTO 掩盖必需锚点。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", content = "result", rename_all = "snake_case")]
pub enum ExecutionDispatchResult {
    AgentLaunch(AgentLaunchDispatchResult),
    SubjectExecution(SubjectExecutionDispatchResult),
    LifecycleRunStart(LifecycleRunStartDispatchResult),
    InteractionGateOpened(InteractionGateOpenedDispatchResult),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn execution_intent_serializes_as_discriminated_taxonomy() {
        let intent = ExecutionIntent::SubjectExecution(SubjectExecutionIntent {
            project_id: Uuid::new_v4(),
            source: ExecutionSource::ProjectAgent,
            subject_ref: SubjectRef::new("project", Uuid::new_v4()),
            parent_run_id: None,
            parent_agent_id: None,
            workflow_graph_ref: WorkflowGraphRef::ByKey {
                project_id: Uuid::new_v4(),
                key: "builtin.freeform_session".to_string(),
            },
            agent_procedure_ref: None,
            run_policy: RunPolicy::CreateLinkedRun,
            agent_policy: AgentPolicy::Create,
            context_policy: ContextPolicy::Isolated,
            capability_policy: CapabilityPolicy::Baseline,
            runtime_policy: RuntimePolicy::CreateRuntimeSession,
        });
        let json = serde_json::to_string(&intent).expect("serialize");
        let deserialized: ExecutionIntent = serde_json::from_str(&json).expect("deserialize");
        assert!(json.contains("subject_execution"));
        assert!(matches!(
            deserialized,
            ExecutionIntent::SubjectExecution(SubjectExecutionIntent {
                source: ExecutionSource::ProjectAgent,
                run_policy: RunPolicy::CreateLinkedRun,
                ..
            })
        ));
    }

    #[test]
    fn subject_execution_result_serializes_required_assignment_ref() {
        let assignment_ref = Uuid::new_v4();
        let result = ExecutionDispatchResult::SubjectExecution(SubjectExecutionDispatchResult {
            run_ref: Uuid::new_v4(),
            graph_instance_ref: Uuid::new_v4(),
            agent_ref: Uuid::new_v4(),
            frame_ref: Uuid::new_v4(),
            assignment_ref,
            subject_execution_ref: SubjectExecutionRef {
                subject_ref: SubjectRef::new("task", Uuid::new_v4()),
                association_id: Uuid::new_v4(),
            },
            runtime_session_ref: None,
            trace_ref: None,
        });
        let json = serde_json::to_string(&result).expect("serialize");
        assert!(!json.contains("runtime_session_ref"));
        assert!(json.contains(&assignment_ref.to_string()));
        assert!(json.contains("assignment_ref"));
        assert!(json.contains("subject_execution"));
    }
}
