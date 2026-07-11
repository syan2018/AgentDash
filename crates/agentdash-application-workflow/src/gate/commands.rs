use agentdash_domain::workflow::{ExecutorSpec, GateWaitPolicyTemplate};
use serde_json::Value;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub enum LifecycleGateCommand {
    OpenCompanionGate(OpenCompanionGateCommand),
    OpenWorkflowHumanGate(OpenWorkflowHumanGateCommand),
    ResolveWorkflowHumanGate(ResolveWorkflowHumanGateCommand),
    RespondHuman(RespondHumanGateCommand),
    OpenParentRequest(OpenParentRequestGateCommand),
    ResolveParentRequest(ResolveParentRequestGateCommand),
    CompleteChildResult(CompleteChildResultGateCommand),
    ResolveGatePayload(ResolveGatePayloadCommand),
}

#[derive(Debug, Clone)]
pub struct OpenCompanionGateCommand {
    pub run_id: Uuid,
    pub agent_id: Uuid,
    pub frame_id: Option<Uuid>,
    pub gate_kind: String,
    pub correlation_id: String,
    pub payload: Option<Value>,
    pub wait_policy: Option<GateWaitPolicyTemplate>,
}

#[derive(Debug, Clone)]
pub struct OpenWorkflowHumanGateCommand {
    pub run_id: Uuid,
    pub orchestration_id: Uuid,
    pub node_path: String,
    pub attempt: u32,
    pub plan_node_id: String,
    pub label: Option<String>,
    pub executor: Option<ExecutorSpec>,
}

#[derive(Debug, Clone)]
pub struct ResolveWorkflowHumanGateCommand {
    pub gate_id: Uuid,
    pub decision: Value,
    pub resolved_by: String,
}

#[derive(Debug, Clone)]
pub struct RespondHumanGateCommand {
    pub gate_id: Uuid,
    pub payload: Value,
    pub resolved_by: String,
}

#[derive(Debug, Clone)]
pub struct OpenParentRequestGateCommand {
    pub run_id: Uuid,
    pub parent_agent_id: Uuid,
    pub parent_frame_id: Uuid,
    pub parent_runtime_thread_id: String,
    pub child_agent_id: Uuid,
    pub child_frame_id: Uuid,
    pub child_runtime_thread_id: String,
    pub turn_id: String,
    pub wait: bool,
    pub companion_label: String,
    pub message: String,
    pub payload: Value,
}

#[derive(Debug, Clone)]
pub struct ResolveParentRequestGateCommand {
    pub gate_id: Uuid,
    pub run_id: Uuid,
    pub parent_agent_id: Uuid,
    pub parent_frame_id: Uuid,
    pub parent_runtime_thread_id: String,
    pub child_agent_id: Uuid,
    pub child_frame_id: Uuid,
    pub child_runtime_thread_id: String,
    pub resolved_turn_id: String,
    pub payload: Value,
    pub resolved_by: String,
}

#[derive(Debug, Clone)]
pub struct CompleteChildResultGateCommand {
    pub gate_id: Uuid,
    pub request_id: String,
    pub run_id: Uuid,
    pub parent_agent_id: Uuid,
    pub parent_runtime_thread_id: String,
    pub child_agent_id: Uuid,
    pub child_runtime_thread_id: Option<String>,
    pub resolved_turn_id: String,
    pub companion_label: String,
    pub payload: Value,
    pub resolved_by: String,
}

#[derive(Debug, Clone)]
pub struct ResolveGatePayloadCommand {
    pub gate_id: Uuid,
    pub payload: Value,
    pub resolved_by: String,
}
