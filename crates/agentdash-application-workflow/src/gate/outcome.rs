use agentdash_domain::workflow::LifecycleGate;
use serde_json::Value;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GateTransitionKind {
    Opened,
    Resolved,
}

#[derive(Debug, Clone)]
pub struct GateTransitionOutcome {
    pub gate: LifecycleGate,
    pub transition: GateTransitionKind,
    pub delivery_intents: Vec<GateDeliveryIntent>,
}

#[derive(Debug, Clone)]
pub enum GateDeliveryIntent {
    MailboxWake(GateMailboxWakeIntent),
    CompanionHumanResponse(CompanionHumanResponseDeliveryIntent),
    CompanionParentRequest(CompanionParentRequestDeliveryIntent),
    CompanionParentResponseToChild(CompanionParentResponseDeliveryIntent),
    CompanionChildResultToParent(CompanionChildResultDeliveryIntent),
}

#[derive(Debug, Clone)]
pub struct GateMailboxWakeIntent {
    pub gate_id: Uuid,
    pub namespace: String,
    pub request_id: String,
    pub target_run_id: Uuid,
    pub target_agent_id: Uuid,
    pub target_runtime_thread_id: String,
    pub producer_agent_id: Uuid,
    pub producer_runtime_thread_id: Option<String>,
    pub resolved_turn_id: String,
    pub client_command_id: String,
    pub payload: Value,
}

#[derive(Debug, Clone)]
pub struct CompanionHumanResponseDeliveryIntent {
    pub gate_id: Uuid,
    pub request_id: String,
    pub run_id: Uuid,
    pub agent_id: Uuid,
    pub turn_id: Option<String>,
    pub request_type: Option<String>,
    pub payload: Value,
}

#[derive(Debug, Clone)]
pub struct CompanionParentRequestDeliveryIntent {
    pub gate_id: Uuid,
    pub request_id: String,
    pub run_id: Uuid,
    pub parent_agent_id: Uuid,
    pub parent_runtime_thread_id: String,
    pub child_agent_id: Uuid,
    pub child_runtime_thread_id: String,
    pub turn_id: String,
    pub wait: bool,
    pub payload: Value,
}

#[derive(Debug, Clone)]
pub struct CompanionParentResponseDeliveryIntent {
    pub gate_id: Uuid,
    pub request_id: String,
    pub run_id: Uuid,
    pub parent_agent_id: Uuid,
    pub parent_runtime_thread_id: String,
    pub child_agent_id: Uuid,
    pub child_runtime_thread_id: String,
    pub resolved_turn_id: String,
    pub payload: Value,
}

#[derive(Debug, Clone)]
pub struct CompanionChildResultDeliveryIntent {
    pub gate_id: Uuid,
    pub request_id: String,
    pub run_id: Uuid,
    pub parent_agent_id: Uuid,
    pub parent_runtime_thread_id: String,
    pub child_agent_id: Uuid,
    pub child_runtime_thread_id: Option<String>,
    pub resolved_turn_id: String,
    pub payload: Value,
}
