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

/// 业务执行进入控制面的统一入口。
///
/// 所有业务路径（ProjectAgent open、Task execution、Companion dispatch、Routine fire）
/// 都应构造 `ExecutionIntent` 并提交给 `LifecycleDispatchService`，
/// 而非自行组装 SessionBinding / owner DTO / SessionConstructionPlan。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionIntent {
    pub project_id: Uuid,
    pub source: ExecutionSource,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subject_ref: Option<SubjectRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_run_id: Option<Uuid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_agent_id: Option<Uuid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workflow_graph_ref: Option<WorkflowGraphRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_procedure_ref: Option<AgentProcedureRef>,
    pub run_policy: RunPolicy,
    pub agent_policy: AgentPolicy,
    pub context_policy: ContextPolicy,
    pub capability_policy: CapabilityPolicy,
    pub runtime_policy: RuntimePolicy,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gate_policy: Option<GatePolicy>,
}

// ─── Result ──────────────────────────────────────────────────────────────────

/// Dispatch 调度结果——包含所有目标锚点的稳定引用。
///
/// 前端可凭此进入 subject view、agent view 或 runtime trace view。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionDispatchResult {
    pub run_ref: Uuid,
    pub graph_instance_ref: Uuid,
    pub agent_ref: Uuid,
    pub frame_ref: Uuid,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime_session_ref: Option<Uuid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assignment_ref: Option<Uuid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gate_ref: Option<Uuid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subject_execution_ref: Option<SubjectExecutionRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trace_ref: Option<Uuid>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn execution_intent_roundtrip_serde() {
        let intent = ExecutionIntent {
            project_id: Uuid::new_v4(),
            source: ExecutionSource::ProjectAgent,
            subject_ref: Some(SubjectRef::new("project", Uuid::new_v4())),
            parent_run_id: None,
            parent_agent_id: None,
            workflow_graph_ref: Some(WorkflowGraphRef::ByKey {
                project_id: Uuid::new_v4(),
                key: "builtin.freeform_session".to_string(),
            }),
            agent_procedure_ref: None,
            run_policy: RunPolicy::CreateLinkedRun,
            agent_policy: AgentPolicy::Create,
            context_policy: ContextPolicy::Isolated,
            capability_policy: CapabilityPolicy::Baseline,
            runtime_policy: RuntimePolicy::CreateRuntimeSession,
            gate_policy: None,
        };
        let json = serde_json::to_string(&intent).expect("serialize");
        let deserialized: ExecutionIntent = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(deserialized.source, intent.source);
        assert_eq!(deserialized.run_policy, intent.run_policy);
    }

    #[test]
    fn dispatch_result_optional_fields_are_none() {
        let result = ExecutionDispatchResult {
            run_ref: Uuid::new_v4(),
            graph_instance_ref: Uuid::new_v4(),
            agent_ref: Uuid::new_v4(),
            frame_ref: Uuid::new_v4(),
            runtime_session_ref: None,
            assignment_ref: None,
            gate_ref: None,
            subject_execution_ref: None,
            trace_ref: None,
        };
        let json = serde_json::to_string(&result).expect("serialize");
        assert!(!json.contains("runtime_session_ref"));
        assert!(!json.contains("gate_ref"));
    }
}
