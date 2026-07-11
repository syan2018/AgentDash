use std::sync::Arc;

use agentdash_application_ports::agent_run_runtime::{
    AgentRunRuntimeBindingRepository, AgentRunRuntimeTarget,
};
use agentdash_application_workflow::gate::{
    CompleteChildResultGateCommand, GateDeliveryIntent, LifecycleGateResolver,
    OpenParentRequestGateCommand, ResolveParentRequestGateCommand, RespondHumanGateCommand,
};
#[cfg(test)]
use agentdash_application_workflow::gate::{
    GateProducerTerminalConvergenceResult, GateProducerTerminalEvent,
};
use agentdash_domain::workflow::{
    AgentFrameRepository, AgentLineageRepository, LifecycleAgentRepository, LifecycleGate,
    LifecycleGateRepository,
};
use async_trait::async_trait;
use uuid::Uuid;

use super::{PayloadTypeRegistry, payload_types};
use crate::ApplicationError;
#[cfg(test)]
use crate::gate_wait_policy::GateProducerTerminalConvergencePort;
use crate::lifecycle::resolve_current_frame_from_delivery_trace_ref;

const COMPANION_PARENT_REQUEST_GATE_KIND: &str = "companion_parent_request";
const COMPANION_CHILD_WAIT_GATE_KIND: &str = "companion_wait";
const COMPANION_CHILD_BLOCKING_WAIT_GATE_KIND: &str = "companion_wait_blocking";
const COMPANION_CHILD_FOLLOW_UP_WAIT_GATE_KIND: &str = "companion_wait_follow_up";

fn is_companion_child_wait_gate(gate_kind: &str, payload: Option<&serde_json::Value>) -> bool {
    match gate_kind {
        COMPANION_CHILD_BLOCKING_WAIT_GATE_KIND | COMPANION_CHILD_FOLLOW_UP_WAIT_GATE_KIND => true,
        COMPANION_CHILD_WAIT_GATE_KIND => payload
            .and_then(|payload| payload.get("request_type"))
            .is_none(),
        _ => false,
    }
}

#[derive(Debug, Clone)]
pub struct RespondCompanionGateCommand {
    pub gate_id: Uuid,
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone)]
pub struct CompanionGateRespondResult {
    pub gate_id: Uuid,
    pub request_id: String,
    pub runtime_thread_id: Option<String>,
    pub gate_resolved: bool,
}

#[derive(Debug, Clone)]
pub struct CompleteCompanionChildResultCommand {
    pub request_id: String,
    pub child_runtime_session_id: String,
    pub resolved_turn_id: String,
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone)]
pub struct OpenCompanionParentRequestCommand {
    pub child_runtime_session_id: String,
    pub turn_id: String,
    pub wait: bool,
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone)]
pub struct CompanionParentRequestOpenResult {
    pub gate_id: Uuid,
    pub request_id: String,
    pub run_id: Uuid,
    pub parent_agent_id: Uuid,
    pub parent_frame_id: Uuid,
    pub parent_runtime_thread_id: String,
    pub child_agent_id: Uuid,
    pub child_frame_id: Uuid,
    pub child_runtime_thread_id: String,
    pub companion_label: String,
    pub parent_mailbox_delivery: CompanionParentMailboxDeliveryResult,
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone)]
pub struct ResolveCompanionParentRequestCommand {
    pub request_id: String,
    pub parent_runtime_session_id: String,
    pub resolved_turn_id: String,
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone)]
pub struct CompanionParentRequestResolveResult {
    pub gate_id: Uuid,
    pub parent_agent_id: Uuid,
    pub parent_frame_id: Uuid,
    pub parent_runtime_thread_id: String,
    pub child_agent_id: Uuid,
    pub child_frame_id: Uuid,
    pub child_runtime_thread_id: String,
    pub child_mailbox_delivery: CompanionParentMailboxDeliveryResult,
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone)]
pub struct CompanionChildResultCompleteResult {
    pub gate_id: Uuid,
    pub parent_agent_id: Uuid,
    pub parent_runtime_thread_id: Option<String>,
    pub child_runtime_thread_id: Option<String>,
    pub parent_mailbox_delivery: CompanionParentMailboxDeliveryResult,
    pub payload: serde_json::Value,
}

struct ChildResultPayloadDeliveryInput {
    gate_id: Uuid,
    request_id: String,
    run_id: Uuid,
    parent_agent_id: Uuid,
    parent_runtime_thread_id: String,
    child_agent_id: Uuid,
    child_runtime_thread_id: Option<String>,
    resolved_turn_id: String,
    companion_label: String,
    payload: serde_json::Value,
}

struct ResolvedChildResultDeliveryInput {
    gate: LifecycleGate,
    expected_request_id: Option<String>,
    parent_agent_id: Uuid,
    parent_runtime_thread_id: String,
    child_agent_id: Uuid,
    child_runtime_thread_id: Option<String>,
    fallback_resolved_turn_id: String,
    companion_label: String,
}

struct CurrentRuntimeBindingSelection {
    current_frame_id: Uuid,
    runtime_session_id: String,
}

#[derive(Debug, Clone)]
pub struct CompanionParentMailboxDeliveryCommand {
    pub gate_id: Uuid,
    pub request_id: String,
    pub run_id: Uuid,
    pub parent_agent_id: Uuid,
    pub parent_runtime_thread_id: String,
    pub child_agent_id: Uuid,
    pub child_runtime_thread_id: Option<String>,
    pub resolved_turn_id: String,
    pub payload: serde_json::Value,
    pub input_text: String,
}

#[derive(Debug, Clone)]
pub struct CompanionParentRequestMailboxDeliveryCommand {
    pub gate_id: Uuid,
    pub request_id: String,
    pub run_id: Uuid,
    pub parent_agent_id: Uuid,
    pub parent_runtime_thread_id: String,
    pub child_agent_id: Uuid,
    pub child_runtime_thread_id: String,
    pub turn_id: String,
    pub wait: bool,
    pub payload: serde_json::Value,
    pub input_text: String,
}

#[derive(Debug, Clone)]
pub struct CompanionParentResponseMailboxDeliveryCommand {
    pub gate_id: Uuid,
    pub request_id: String,
    pub run_id: Uuid,
    pub parent_agent_id: Uuid,
    pub parent_runtime_thread_id: String,
    pub child_agent_id: Uuid,
    pub child_runtime_thread_id: String,
    pub resolved_turn_id: String,
    pub payload: serde_json::Value,
    pub input_text: String,
}

#[derive(Debug, Clone)]
pub struct CompanionHumanResponseMailboxDeliveryCommand {
    pub gate_id: Uuid,
    pub request_id: String,
    pub run_id: Uuid,
    pub agent_id: Uuid,
    pub runtime_thread_id: String,
    pub turn_id: Option<String>,
    pub request_type: Option<String>,
    pub payload: serde_json::Value,
    pub input_text: String,
}

#[derive(Debug, Clone)]
pub struct CompanionParentMailboxDeliveryResult {
    pub mailbox_message_id: Option<Uuid>,
    pub accepted_runtime_operation_id: Option<String>,
    pub command_receipt_client_command_id: String,
    pub command_receipt_status: String,
    pub command_receipt_duplicate: bool,
    pub outcome: String,
    pub runtime_operation_id: Option<String>,
}

#[async_trait]
pub trait CompanionParentMailboxDelivery: Send + Sync {
    async fn deliver_child_result_to_parent(
        &self,
        command: CompanionParentMailboxDeliveryCommand,
    ) -> Result<CompanionParentMailboxDeliveryResult, ApplicationError>;

    async fn deliver_parent_request_to_parent(
        &self,
        command: CompanionParentRequestMailboxDeliveryCommand,
    ) -> Result<CompanionParentMailboxDeliveryResult, ApplicationError>;

    async fn deliver_parent_response_to_child(
        &self,
        command: CompanionParentResponseMailboxDeliveryCommand,
    ) -> Result<CompanionParentMailboxDeliveryResult, ApplicationError>;
}

#[async_trait]
pub trait CompanionHumanResponseMailboxDelivery: Send + Sync {
    async fn deliver_human_response_to_requesting_agent(
        &self,
        command: CompanionHumanResponseMailboxDeliveryCommand,
    ) -> Result<CompanionParentMailboxDeliveryResult, ApplicationError>;
}

#[derive(Clone, Default)]
pub struct NoopCompanionParentMailboxDelivery;

#[derive(Clone, Default)]
pub struct NoopCompanionHumanResponseMailboxDelivery;

#[async_trait]
impl CompanionParentMailboxDelivery for NoopCompanionParentMailboxDelivery {
    async fn deliver_child_result_to_parent(
        &self,
        _command: CompanionParentMailboxDeliveryCommand,
    ) -> Result<CompanionParentMailboxDeliveryResult, ApplicationError> {
        Err(ApplicationError::Internal(
            "companion parent mailbox delivery 未配置".to_string(),
        ))
    }

    async fn deliver_parent_request_to_parent(
        &self,
        _command: CompanionParentRequestMailboxDeliveryCommand,
    ) -> Result<CompanionParentMailboxDeliveryResult, ApplicationError> {
        Err(ApplicationError::Internal(
            "companion parent request mailbox delivery 未配置".to_string(),
        ))
    }

    async fn deliver_parent_response_to_child(
        &self,
        _command: CompanionParentResponseMailboxDeliveryCommand,
    ) -> Result<CompanionParentMailboxDeliveryResult, ApplicationError> {
        Err(ApplicationError::Internal(
            "companion parent response mailbox delivery 未配置".to_string(),
        ))
    }
}

#[async_trait]
impl CompanionHumanResponseMailboxDelivery for NoopCompanionHumanResponseMailboxDelivery {
    async fn deliver_human_response_to_requesting_agent(
        &self,
        _command: CompanionHumanResponseMailboxDeliveryCommand,
    ) -> Result<CompanionParentMailboxDeliveryResult, ApplicationError> {
        Err(ApplicationError::Internal(
            "companion human response mailbox delivery 未配置".to_string(),
        ))
    }
}

pub struct CompanionGateControlService {
    gate_repo: Arc<dyn LifecycleGateRepository>,
    gate_resolver: LifecycleGateResolver,
    frame_repo: Arc<dyn AgentFrameRepository>,
    agent_repo: Arc<dyn LifecycleAgentRepository>,
    runtime_binding_repo: Arc<dyn AgentRunRuntimeBindingRepository>,
    lineage_repo: Arc<dyn AgentLineageRepository>,
    parent_mailbox_delivery: Arc<dyn CompanionParentMailboxDelivery>,
    human_response_mailbox_delivery: Arc<dyn CompanionHumanResponseMailboxDelivery>,
}

#[derive(Clone)]
pub struct CompanionGateControlRepos {
    pub gate_repo: Arc<dyn LifecycleGateRepository>,
    pub frame_repo: Arc<dyn AgentFrameRepository>,
    pub agent_repo: Arc<dyn LifecycleAgentRepository>,
    pub runtime_binding_repo: Arc<dyn AgentRunRuntimeBindingRepository>,
    pub lineage_repo: Arc<dyn AgentLineageRepository>,
}

pub struct CompanionGateControlDeps {
    pub repos: CompanionGateControlRepos,
}

impl CompanionGateControlService {
    pub fn new(deps: CompanionGateControlDeps) -> Self {
        let CompanionGateControlDeps { repos } = deps;
        Self {
            gate_resolver: LifecycleGateResolver::new(repos.gate_repo.clone()),
            gate_repo: repos.gate_repo,
            frame_repo: repos.frame_repo,
            agent_repo: repos.agent_repo,
            runtime_binding_repo: repos.runtime_binding_repo,
            lineage_repo: repos.lineage_repo,
            parent_mailbox_delivery: Arc::new(NoopCompanionParentMailboxDelivery),
            human_response_mailbox_delivery: Arc::new(NoopCompanionHumanResponseMailboxDelivery),
        }
    }

    pub fn with_parent_mailbox_delivery(
        mut self,
        parent_mailbox_delivery: Arc<dyn CompanionParentMailboxDelivery>,
    ) -> Self {
        self.parent_mailbox_delivery = parent_mailbox_delivery;
        self
    }

    pub fn with_human_response_mailbox_delivery(
        mut self,
        human_response_mailbox_delivery: Arc<dyn CompanionHumanResponseMailboxDelivery>,
    ) -> Self {
        self.human_response_mailbox_delivery = human_response_mailbox_delivery;
        self
    }

    pub fn with_agent_run_projection(repos: CompanionGateControlRepos) -> Self {
        Self::new(CompanionGateControlDeps { repos })
    }

    pub async fn respond(
        &self,
        command: RespondCompanionGateCommand,
    ) -> Result<CompanionGateRespondResult, ApplicationError> {
        if let Some(error) = payload_types::payload_object_error(&command.payload) {
            return Err(ApplicationError::BadRequest(error));
        }

        let gate = self.gate_repo.get(command.gate_id).await?.ok_or_else(|| {
            ApplicationError::NotFound(format!("gate 不存在: {}", command.gate_id))
        })?;

        if !gate.is_open() {
            return Err(ApplicationError::Conflict(format!(
                "gate 已关闭: {}",
                command.gate_id
            )));
        }

        let gate_meta = gate.payload_json.clone();
        let request_type = gate_meta
            .as_ref()
            .and_then(|metadata| metadata.get("request_type"))
            .and_then(serde_json::Value::as_str)
            .map(str::to_string);

        let registry = PayloadTypeRegistry::with_builtins();
        if let Some(error) = registry.validate_response(&command.payload, request_type.as_deref()) {
            return Err(ApplicationError::BadRequest(error));
        }

        let runtime_thread_id = self.resolve_runtime_thread_id(&gate).await?;
        let request_id = gate.id.to_string();

        let Some(runtime_thread_id) = runtime_thread_id.clone() else {
            let error =
                "requesting agent 缺少 current delivery runtime session，无法投递 human response"
                    .to_string();
            return Err(ApplicationError::Conflict(error));
        };

        let outcome = self
            .gate_resolver
            .respond_human(RespondHumanGateCommand {
                gate_id: gate.id,
                payload: command.payload.clone(),
                resolved_by: "companion_respond".to_string(),
            })
            .await?;
        let human_response_intent = outcome
            .delivery_intents
            .into_iter()
            .find_map(|intent| match intent {
                GateDeliveryIntent::CompanionHumanResponse(intent) => Some(intent),
                _ => None,
            })
            .ok_or_else(|| {
                ApplicationError::Internal(format!(
                    "human response gate {} 缺少 delivery intent",
                    gate.id
                ))
            })?;
        let input_text = build_human_response_mailbox_input_text(
            human_response_intent.gate_id,
            &human_response_intent.request_id,
            human_response_intent.turn_id.as_deref(),
            &human_response_intent.payload,
        );
        self.human_response_mailbox_delivery
            .deliver_human_response_to_requesting_agent(
                CompanionHumanResponseMailboxDeliveryCommand {
                    gate_id: human_response_intent.gate_id,
                    request_id: human_response_intent.request_id.clone(),
                    run_id: human_response_intent.run_id,
                    agent_id: human_response_intent.agent_id,
                    runtime_thread_id: runtime_thread_id.clone(),
                    turn_id: human_response_intent.turn_id.clone(),
                    request_type: human_response_intent.request_type.clone(),
                    payload: human_response_intent.payload.clone(),
                    input_text,
                },
            )
            .await?;

        Ok(CompanionGateRespondResult {
            gate_id: gate.id,
            request_id,
            runtime_thread_id: Some(runtime_thread_id),
            gate_resolved: true,
        })
    }

    async fn ensure_resolved_child_result_delivery(
        &self,
        input: ResolvedChildResultDeliveryInput,
    ) -> Result<Option<CompanionChildResultCompleteResult>, ApplicationError> {
        let payload = input
            .gate
            .payload_json
            .clone()
            .unwrap_or_else(|| serde_json::json!({}));
        let request_id = payload
            .get("request_id")
            .and_then(serde_json::Value::as_str)
            .unwrap_or(input.gate.correlation_id.as_str())
            .to_string();
        if let Some(expected_request_id) = input.expected_request_id.as_deref()
            && request_id != expected_request_id
            && input.gate.correlation_id != expected_request_id
        {
            return Ok(None);
        }
        let resolved_turn_id = payload
            .get("resolved_turn_id")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string)
            .unwrap_or(input.fallback_resolved_turn_id);

        self.deliver_child_result_payload_to_parent(ChildResultPayloadDeliveryInput {
            gate_id: input.gate.id,
            request_id,
            run_id: input.gate.run_id,
            parent_agent_id: input.parent_agent_id,
            parent_runtime_thread_id: input.parent_runtime_thread_id,
            child_agent_id: input.child_agent_id,
            child_runtime_thread_id: input.child_runtime_thread_id,
            resolved_turn_id,
            companion_label: input.companion_label,
            payload,
        })
        .await
        .map(Some)
    }

    async fn deliver_child_result_payload_to_parent(
        &self,
        input: ChildResultPayloadDeliveryInput,
    ) -> Result<CompanionChildResultCompleteResult, ApplicationError> {
        let summary = input
            .payload
            .get("summary")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("")
            .trim();
        let status = input
            .payload
            .get("status")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("completed");
        let input_text = build_parent_result_delivery_projection_text(
            input.gate_id,
            &input.request_id,
            &input.companion_label,
            status,
            summary,
            &input.payload,
        );
        let mailbox_result = self
            .parent_mailbox_delivery
            .deliver_child_result_to_parent(CompanionParentMailboxDeliveryCommand {
                gate_id: input.gate_id,
                request_id: input.request_id,
                run_id: input.run_id,
                parent_agent_id: input.parent_agent_id,
                parent_runtime_thread_id: input.parent_runtime_thread_id.clone(),
                child_agent_id: input.child_agent_id,
                child_runtime_thread_id: input.child_runtime_thread_id.clone(),
                resolved_turn_id: input.resolved_turn_id,
                payload: input.payload.clone(),
                input_text,
            })
            .await?;

        Ok(CompanionChildResultCompleteResult {
            gate_id: input.gate_id,
            parent_agent_id: input.parent_agent_id,
            parent_runtime_thread_id: Some(input.parent_runtime_thread_id),
            child_runtime_thread_id: input.child_runtime_thread_id,
            parent_mailbox_delivery: mailbox_result,
            payload: input.payload,
        })
    }

    pub async fn complete_child_result_to_parent(
        &self,
        command: CompleteCompanionChildResultCommand,
    ) -> Result<Option<CompanionChildResultCompleteResult>, ApplicationError> {
        if let Some(error) = payload_types::payload_object_error(&command.payload) {
            return Err(ApplicationError::BadRequest(error));
        }
        let registry = PayloadTypeRegistry::with_builtins();
        if let Some(error) = registry.validate_response(&command.payload, None) {
            return Err(ApplicationError::BadRequest(error));
        }

        let child_frame = match resolve_current_frame_from_delivery_trace_ref(
            &command.child_runtime_session_id,
            self.runtime_binding_repo.as_ref(),
            self.agent_repo.as_ref(),
            self.frame_repo.as_ref(),
        )
        .await?
        {
            Some((_anchor, _agent, frame)) => frame,
            None => return Ok(None),
        };
        let lineage = match self.lineage_repo.find_parent(child_frame.agent_id).await? {
            Some(lineage) => lineage,
            None => return Ok(None),
        };
        let parent_agent_id = match lineage.parent_agent_id {
            Some(agent_id) => agent_id,
            None => return Ok(None),
        };

        let gate = match self
            .gate_repo
            .find_by_agent_and_correlation(child_frame.agent_id, &command.request_id)
            .await?
            .filter(|gate| {
                is_companion_child_wait_gate(&gate.gate_kind, gate.payload_json.as_ref())
            }) {
            Some(gate) => gate,
            None => return Ok(None),
        };

        let resolved_turn_id = command.resolved_turn_id.clone();
        let child_runtime_session_id = command.child_runtime_session_id.clone();
        let gate_meta = gate.payload_json.clone();
        let companion_label = gate_meta
            .as_ref()
            .and_then(|metadata| metadata.get("companion_label"))
            .and_then(serde_json::Value::as_str)
            .unwrap_or("companion");
        let parent_runtime_thread_id = self
            .select_bound_runtime_thread_id(lineage.run_id, parent_agent_id)
            .await?;
        let child_runtime_thread_id = self
            .validate_bound_runtime_thread_id(
                lineage.run_id,
                child_frame.agent_id,
                &child_runtime_session_id,
            )
            .await?;

        let Some(parent_runtime_thread_id) = parent_runtime_thread_id.clone() else {
            let error =
                "parent agent 缺少 current delivery runtime session，无法投递 companion result"
                    .to_string();
            return Err(ApplicationError::Conflict(error));
        };

        if !gate.is_open() {
            return self
                .ensure_resolved_child_result_delivery(ResolvedChildResultDeliveryInput {
                    gate,
                    expected_request_id: Some(command.request_id.clone()),
                    parent_agent_id,
                    parent_runtime_thread_id,
                    child_agent_id: child_frame.agent_id,
                    child_runtime_thread_id,
                    fallback_resolved_turn_id: resolved_turn_id,
                    companion_label: companion_label.to_string(),
                })
                .await;
        }

        let complete_command = CompleteChildResultGateCommand {
            gate_id: gate.id,
            request_id: command.request_id.clone(),
            run_id: lineage.run_id,
            parent_agent_id,
            parent_runtime_thread_id: parent_runtime_thread_id.clone(),
            child_agent_id: child_frame.agent_id,
            child_runtime_thread_id: child_runtime_thread_id.clone(),
            resolved_turn_id: resolved_turn_id.clone(),
            companion_label: companion_label.to_string(),
            payload: command.payload.clone(),
            resolved_by: format!("child_agent:{}", child_frame.agent_id),
        };
        let outcome = match self
            .gate_resolver
            .complete_child_result(complete_command)
            .await
        {
            Ok(outcome) => outcome,
            Err(agentdash_application_workflow::WorkflowApplicationError::Conflict(message)) => {
                let latest_gate = self.gate_repo.get(gate.id).await?.ok_or_else(|| {
                    ApplicationError::NotFound(format!(
                        "child result gate {} disappeared after resolve conflict",
                        gate.id
                    ))
                })?;
                if latest_gate.is_open() {
                    return Err(ApplicationError::Conflict(message));
                }
                return self
                    .ensure_resolved_child_result_delivery(ResolvedChildResultDeliveryInput {
                        gate: latest_gate,
                        expected_request_id: Some(command.request_id.clone()),
                        parent_agent_id,
                        parent_runtime_thread_id,
                        child_agent_id: child_frame.agent_id,
                        child_runtime_thread_id,
                        fallback_resolved_turn_id: resolved_turn_id,
                        companion_label: companion_label.to_string(),
                    })
                    .await;
            }
            Err(error) => return Err(application_error_from_workflow_gate_error(error)),
        };
        let result_intent = outcome
            .delivery_intents
            .iter()
            .find_map(|intent| match intent {
                GateDeliveryIntent::CompanionChildResultToParent(intent) => Some(intent),
                _ => None,
            })
            .ok_or_else(|| {
                ApplicationError::Internal(format!(
                    "child result gate {} 缺少 delivery intent",
                    gate.id
                ))
            })?;
        let result = self
            .deliver_child_result_payload_to_parent(ChildResultPayloadDeliveryInput {
                gate_id: result_intent.gate_id,
                request_id: result_intent.request_id.clone(),
                run_id: result_intent.run_id,
                parent_agent_id: result_intent.parent_agent_id,
                parent_runtime_thread_id: result_intent.parent_runtime_thread_id.clone(),
                child_agent_id: result_intent.child_agent_id,
                child_runtime_thread_id: result_intent.child_runtime_thread_id.clone(),
                resolved_turn_id: result_intent.resolved_turn_id.clone(),
                companion_label: companion_label.to_string(),
                payload: result_intent.payload.clone(),
            })
            .await?;

        Ok(Some(result))
    }

    #[cfg(test)]
    pub(crate) async fn observe_gate_producer_terminal(
        &self,
        event: GateProducerTerminalEvent,
    ) -> Result<GateProducerTerminalConvergenceResult, ApplicationError> {
        crate::gate_wait_policy::GateProducerTerminalConvergenceServiceAdapter::with_mailbox_wake_delivery(
            self.gate_repo.clone(),
            self.runtime_binding_repo.clone(),
            Arc::new(crate::gate_wait_policy::CompanionGateMailboxWakeDelivery::new(
                self.parent_mailbox_delivery.clone(),
            )),
        )
        .observe_gate_producer_terminal(event)
        .await
    }

    pub async fn open_parent_request(
        &self,
        command: OpenCompanionParentRequestCommand,
    ) -> Result<CompanionParentRequestOpenResult, ApplicationError> {
        if let Some(error) = payload_types::payload_object_error(&command.payload) {
            return Err(ApplicationError::BadRequest(error));
        }
        let message = command
            .payload
            .get("message")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| ApplicationError::BadRequest("payload.message 不能为空".to_string()))?;

        let (child_anchor, _agent, child_frame) = resolve_current_frame_from_delivery_trace_ref(
            &command.child_runtime_session_id,
            self.runtime_binding_repo.as_ref(),
            self.agent_repo.as_ref(),
            self.frame_repo.as_ref(),
        )
        .await?
        .ok_or_else(|| {
            ApplicationError::Conflict(
                "当前 runtime session 没有关联的 AgentFrame，无法向 parent 提审".to_string(),
            )
        })?;
        let lineage = self
            .lineage_repo
            .find_parent(child_frame.agent_id)
            .await?
            .ok_or_else(|| {
                ApplicationError::Conflict(
                    "当前 agent 没有 parent lineage，无法向 parent 提审".to_string(),
                )
            })?;
        let parent_agent_id = lineage.parent_agent_id.ok_or_else(|| {
            ApplicationError::Conflict("lineage 中 parent_agent_id 为空".to_string())
        })?;
        let child_runtime_thread_id = self
            .validate_bound_runtime_thread_id(
                child_anchor.target.run_id,
                child_frame.agent_id,
                &command.child_runtime_session_id,
            )
            .await?
            .ok_or_else(|| {
                ApplicationError::Conflict(format!(
                    "child agent {} 缺少 current delivery runtime session",
                    child_frame.agent_id
                ))
            })?;
        let parent_selection = self
            .select_current_delivery(lineage.run_id, parent_agent_id)
            .await?
            .ok_or_else(|| {
                ApplicationError::Conflict(
                    "parent agent 缺少 current delivery runtime session".to_string(),
                )
            })?;
        let parent_frame_id = parent_selection.current_frame_id;
        let parent_runtime_thread_id = parent_selection.runtime_session_id;

        let companion_label = format!("child:{}", child_frame.agent_id);
        let outcome = self
            .gate_resolver
            .open_parent_request(OpenParentRequestGateCommand {
                run_id: lineage.run_id,
                parent_agent_id,
                parent_frame_id,
                parent_runtime_thread_id: parent_runtime_thread_id.clone(),
                child_agent_id: child_frame.agent_id,
                child_frame_id: child_frame.id,
                child_runtime_thread_id: child_runtime_thread_id.clone(),
                turn_id: command.turn_id.clone(),
                wait: command.wait,
                companion_label: companion_label.clone(),
                message: message.to_string(),
                payload: command.payload.clone(),
            })
            .await?;
        let gate = outcome.gate;
        let request_id = gate.id.to_string();
        let review_payload = gate.payload_json.clone().ok_or_else(|| {
            ApplicationError::Internal(format!("parent request gate {} 缺少 payload", gate.id))
        })?;
        let parent_request_intent = outcome
            .delivery_intents
            .iter()
            .find_map(|intent| match intent {
                GateDeliveryIntent::CompanionParentRequest(intent) => Some(intent),
                _ => None,
            })
            .ok_or_else(|| {
                ApplicationError::Internal(format!(
                    "parent request gate {} 缺少 delivery intent",
                    gate.id
                ))
            })?;

        let input_text = build_parent_request_mailbox_input_text(
            parent_request_intent.gate_id,
            &parent_request_intent.request_id,
            &companion_label,
            message,
            parent_request_intent.wait,
            &parent_request_intent.payload,
        );
        let mailbox_result = self
            .parent_mailbox_delivery
            .deliver_parent_request_to_parent(CompanionParentRequestMailboxDeliveryCommand {
                gate_id: parent_request_intent.gate_id,
                request_id: parent_request_intent.request_id.clone(),
                run_id: parent_request_intent.run_id,
                parent_agent_id: parent_request_intent.parent_agent_id,
                parent_runtime_thread_id: parent_request_intent.parent_runtime_thread_id.clone(),
                child_agent_id: parent_request_intent.child_agent_id,
                child_runtime_thread_id: parent_request_intent.child_runtime_thread_id.clone(),
                turn_id: parent_request_intent.turn_id.clone(),
                wait: parent_request_intent.wait,
                payload: parent_request_intent.payload.clone(),
                input_text,
            })
            .await?;

        Ok(CompanionParentRequestOpenResult {
            gate_id: gate.id,
            request_id,
            run_id: gate.run_id,
            parent_agent_id,
            parent_frame_id,
            parent_runtime_thread_id,
            child_agent_id: child_frame.agent_id,
            child_frame_id: child_frame.id,
            child_runtime_thread_id,
            companion_label,
            parent_mailbox_delivery: mailbox_result,
            payload: review_payload,
        })
    }

    pub async fn resolve_parent_request(
        &self,
        command: ResolveCompanionParentRequestCommand,
    ) -> Result<Option<CompanionParentRequestResolveResult>, ApplicationError> {
        if let Some(error) = payload_types::payload_object_error(&command.payload) {
            return Err(ApplicationError::BadRequest(error));
        }
        let registry = PayloadTypeRegistry::with_builtins();
        if let Some(error) = registry.validate_response(&command.payload, None) {
            return Err(ApplicationError::BadRequest(error));
        }

        let request_id = command.request_id.trim();
        let Ok(gate_id) = Uuid::parse_str(request_id) else {
            return Ok(None);
        };
        let Some(gate) = self.gate_repo.get(gate_id).await? else {
            return Ok(None);
        };
        if gate.gate_kind != COMPANION_PARENT_REQUEST_GATE_KIND {
            return Ok(None);
        }

        let (parent_anchor, _agent, parent_frame) = resolve_current_frame_from_delivery_trace_ref(
            &command.parent_runtime_session_id,
            self.runtime_binding_repo.as_ref(),
            self.agent_repo.as_ref(),
            self.frame_repo.as_ref(),
        )
        .await?
        .ok_or_else(|| {
            ApplicationError::Conflict(format!(
                "runtime session {} 没有关联的 parent AgentFrame",
                command.parent_runtime_session_id
            ))
        })?;
        if gate.agent_id != Some(parent_frame.agent_id) || gate.frame_id != Some(parent_frame.id) {
            return Err(ApplicationError::Conflict(format!(
                "parent request gate {} 不属于当前 parent frame {}",
                gate.id, parent_frame.id
            )));
        }
        if !gate.is_open() {
            return Err(ApplicationError::Conflict(format!(
                "parent request gate 已关闭: {}",
                gate.id
            )));
        }
        let parent_runtime_thread_id = self
            .validate_bound_runtime_thread_id(
                parent_anchor.target.run_id,
                parent_frame.agent_id,
                &command.parent_runtime_session_id,
            )
            .await?
            .ok_or_else(|| {
                ApplicationError::Conflict(format!(
                    "parent agent {} 缺少 current delivery runtime session",
                    parent_frame.agent_id
                ))
            })?;
        let request_payload = gate.payload_json.clone().ok_or_else(|| {
            ApplicationError::Conflict(format!("parent request gate {} 缺少 payload", gate.id))
        })?;
        let child_agent_id = payload_uuid(&request_payload, "child_agent_id")?;
        let child_frame_id = payload_uuid(&request_payload, "child_frame_id")?;
        let child_runtime_thread_id = request_payload
            .get("companion_session_id")
            .or_else(|| request_payload.get("child_runtime_thread_id"))
            .and_then(serde_json::Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| {
                ApplicationError::Conflict(format!(
                    "parent request gate {} 缺少 child delivery runtime session",
                    gate.id
                ))
            })?;
        let child_runtime_thread_id = self
            .validate_bound_runtime_thread_id(
                parent_anchor.target.run_id,
                child_agent_id,
                &child_runtime_thread_id,
            )
            .await?
            .ok_or_else(|| {
                ApplicationError::Conflict(format!(
                    "child agent {} 缺少 current delivery runtime session",
                    child_agent_id
                ))
            })?;

        let outcome = self
            .gate_resolver
            .resolve_parent_request(ResolveParentRequestGateCommand {
                gate_id: gate.id,
                run_id: parent_anchor.target.run_id,
                parent_agent_id: parent_frame.agent_id,
                parent_frame_id: parent_frame.id,
                parent_runtime_thread_id: parent_runtime_thread_id.clone(),
                child_agent_id,
                child_frame_id,
                child_runtime_thread_id: child_runtime_thread_id.clone(),
                resolved_turn_id: command.resolved_turn_id.clone(),
                payload: command.payload.clone(),
                resolved_by: format!("parent_agent:{}", parent_frame.agent_id),
            })
            .await?;
        let parent_response_intent = outcome
            .delivery_intents
            .iter()
            .find_map(|intent| match intent {
                GateDeliveryIntent::CompanionParentResponseToChild(intent) => Some(intent),
                _ => None,
            })
            .ok_or_else(|| {
                ApplicationError::Internal(format!(
                    "parent response gate {} 缺少 delivery intent",
                    gate.id
                ))
            })?;
        let resolution_payload = parent_response_intent.payload.clone();

        let input_text = build_parent_response_mailbox_input_text(
            parent_response_intent.gate_id,
            &parent_response_intent.request_id,
            &parent_response_intent.resolved_turn_id,
            &parent_response_intent.payload,
        );
        let mailbox_result = self
            .parent_mailbox_delivery
            .deliver_parent_response_to_child(CompanionParentResponseMailboxDeliveryCommand {
                gate_id: parent_response_intent.gate_id,
                request_id: parent_response_intent.request_id.clone(),
                run_id: parent_response_intent.run_id,
                parent_agent_id: parent_response_intent.parent_agent_id,
                parent_runtime_thread_id: parent_response_intent.parent_runtime_thread_id.clone(),
                child_agent_id: parent_response_intent.child_agent_id,
                child_runtime_thread_id: parent_response_intent.child_runtime_thread_id.clone(),
                resolved_turn_id: parent_response_intent.resolved_turn_id.clone(),
                payload: parent_response_intent.payload.clone(),
                input_text,
            })
            .await?;

        Ok(Some(CompanionParentRequestResolveResult {
            gate_id: gate.id,
            parent_agent_id: parent_frame.agent_id,
            parent_frame_id: parent_frame.id,
            parent_runtime_thread_id,
            child_agent_id,
            child_frame_id,
            child_runtime_thread_id,
            child_mailbox_delivery: mailbox_result,
            payload: resolution_payload,
        }))
    }

    async fn resolve_runtime_thread_id(
        &self,
        gate: &agentdash_domain::workflow::LifecycleGate,
    ) -> Result<Option<String>, ApplicationError> {
        let frame = if let Some(frame_id) = gate.frame_id {
            self.frame_repo.get(frame_id).await?.ok_or_else(|| {
                ApplicationError::NotFound(format!("gate frame 不存在: {frame_id}"))
            })?
        } else if let Some(agent_id) = gate.agent_id {
            self.frame_repo
                .get_current(agent_id)
                .await?
                .ok_or_else(|| {
                    ApplicationError::NotFound(format!("gate agent 没有当前 frame: {agent_id}"))
                })?
        } else {
            return Err(ApplicationError::Conflict(format!(
                "gate 缺少 agent/frame owner: {}",
                gate.id
            )));
        };

        if let Some(agent_id) = gate.agent_id
            && frame.agent_id != agent_id
        {
            return Err(ApplicationError::Conflict(format!(
                "gate frame {} 不属于 gate agent {}",
                frame.id, agent_id
            )));
        }

        self.select_bound_runtime_thread_id(gate.run_id, frame.agent_id)
            .await
    }

    async fn select_bound_runtime_thread_id(
        &self,
        run_id: Uuid,
        agent_id: Uuid,
    ) -> Result<Option<String>, ApplicationError> {
        Ok(self
            .select_current_delivery(run_id, agent_id)
            .await?
            .map(|selection| selection.runtime_session_id))
    }

    async fn validate_bound_runtime_thread_id(
        &self,
        run_id: Uuid,
        agent_id: Uuid,
        runtime_session_id: &str,
    ) -> Result<Option<String>, ApplicationError> {
        let Some(selection) = self.select_current_delivery(run_id, agent_id).await? else {
            return Ok(None);
        };
        if selection.runtime_session_id == runtime_session_id {
            return Ok(Some(selection.runtime_session_id));
        }
        Err(ApplicationError::Conflict(format!(
            "agent {agent_id} current delivery runtime session {} 不匹配提交 runtime session {runtime_session_id}",
            selection.runtime_session_id
        )))
    }

    async fn select_current_delivery(
        &self,
        run_id: Uuid,
        agent_id: Uuid,
    ) -> Result<Option<CurrentRuntimeBindingSelection>, ApplicationError> {
        let target = AgentRunRuntimeTarget { run_id, agent_id };
        let Some(binding) = self
            .runtime_binding_repo
            .load(&target)
            .await
            .map_err(|error| ApplicationError::Internal(error.to_string()))?
        else {
            return Ok(None);
        };
        let Some(agent) = self.agent_repo.get(agent_id).await? else {
            return Err(ApplicationError::NotFound(format!(
                "LifecycleAgent 不存在: {agent_id}"
            )));
        };
        if agent.run_id != run_id {
            return Err(ApplicationError::Conflict(format!(
                "LifecycleAgent {agent_id} 不属于 LifecycleRun {run_id}"
            )));
        }
        let Some(frame) = self.frame_repo.get_current(agent_id).await? else {
            return Err(ApplicationError::NotFound(format!(
                "LifecycleAgent {agent_id} 没有 current AgentFrame"
            )));
        };
        Ok(Some(CurrentRuntimeBindingSelection {
            current_frame_id: frame.id,
            runtime_session_id: binding.thread_id.to_string(),
        }))
    }
}

fn application_error_from_workflow_gate_error(
    error: agentdash_application_workflow::WorkflowApplicationError,
) -> ApplicationError {
    match error {
        agentdash_application_workflow::WorkflowApplicationError::BadRequest(message)
        | agentdash_application_workflow::WorkflowApplicationError::ModelRequired(message) => {
            ApplicationError::BadRequest(message)
        }
        agentdash_application_workflow::WorkflowApplicationError::NotFound(message) => {
            ApplicationError::NotFound(message)
        }
        agentdash_application_workflow::WorkflowApplicationError::Conflict(message) => {
            ApplicationError::Conflict(message)
        }
        agentdash_application_workflow::WorkflowApplicationError::Internal(message) => {
            ApplicationError::Internal(message)
        }
    }
}

pub(crate) fn build_parent_result_delivery_projection_text(
    gate_id: Uuid,
    request_id: &str,
    companion_label: &str,
    status: &str,
    summary: &str,
    payload: &serde_json::Value,
) -> String {
    const SUMMARY_LIMIT: usize = 1_000;
    const ITEM_LIMIT: usize = 500;
    const MAX_ITEMS: usize = 12;

    let mut lines = vec![
        "Companion result delivery projection.".to_string(),
        format!("- request_id: {request_id}"),
        format!("- gate_id: {gate_id}"),
        format!("- companion_label: {companion_label}"),
        format!("- status: {status}"),
        format!(
            "- summary: {}",
            bounded_projection_text(summary, SUMMARY_LIMIT)
        ),
    ];
    if let Some(findings) = payload
        .get("findings")
        .and_then(serde_json::Value::as_array)
    {
        let rendered = findings
            .iter()
            .filter_map(serde_json::Value::as_str)
            .take(MAX_ITEMS)
            .map(|finding| format!("  - {}", bounded_projection_text(finding, ITEM_LIMIT)))
            .collect::<Vec<_>>();
        if !rendered.is_empty() {
            lines.push("- findings:".to_string());
            lines.extend(rendered);
            if findings.len() > MAX_ITEMS {
                lines.push(format!("  - ... {} more", findings.len() - MAX_ITEMS));
            }
        }
    }
    if let Some(follow_ups) = payload
        .get("follow_ups")
        .and_then(serde_json::Value::as_array)
    {
        let rendered = follow_ups
            .iter()
            .filter_map(serde_json::Value::as_str)
            .take(MAX_ITEMS)
            .map(|follow_up| format!("  - {}", bounded_projection_text(follow_up, ITEM_LIMIT)))
            .collect::<Vec<_>>();
        if !rendered.is_empty() {
            lines.push("- follow_ups:".to_string());
            lines.extend(rendered);
            if follow_ups.len() > MAX_ITEMS {
                lines.push(format!("  - ... {} more", follow_ups.len() - MAX_ITEMS));
            }
        }
    }
    lines.join("\n")
}

fn bounded_projection_text(text: &str, max_chars: usize) -> String {
    let trimmed = text.trim();
    if trimmed.chars().count() <= max_chars {
        return trimmed.to_string();
    }
    let mut bounded = trimmed.chars().take(max_chars).collect::<String>();
    bounded.push_str("...");
    bounded
}

fn build_parent_request_mailbox_input_text(
    gate_id: Uuid,
    request_id: &str,
    companion_label: &str,
    message: &str,
    wait: bool,
    payload: &serde_json::Value,
) -> String {
    let mut lines = vec![
        "Companion parent request is available.".to_string(),
        format!("- request_id: {request_id}"),
        format!("- gate_id: {gate_id}"),
        format!("- companion_label: {companion_label}"),
        format!("- wait: {wait}"),
        format!("- message: {message}"),
    ];
    if let Some(request_type) = payload
        .get("request_type")
        .and_then(serde_json::Value::as_str)
    {
        lines.push(format!("- request_type: {request_type}"));
    }
    lines.join("\n")
}

fn build_parent_response_mailbox_input_text(
    gate_id: Uuid,
    request_id: &str,
    resolved_turn_id: &str,
    payload: &serde_json::Value,
) -> String {
    let status = payload
        .get("status")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("resolved");
    let summary = payload
        .get("summary")
        .and_then(serde_json::Value::as_str)
        .or_else(|| payload.get("message").and_then(serde_json::Value::as_str))
        .unwrap_or("");
    let mut lines = vec![
        "Companion parent response is available.".to_string(),
        format!("- request_id: {request_id}"),
        format!("- gate_id: {gate_id}"),
        format!("- resolved_turn_id: {resolved_turn_id}"),
        format!("- status: {status}"),
    ];
    if !summary.trim().is_empty() {
        lines.push(format!("- summary: {}", summary.trim()));
    }
    lines.join("\n")
}

fn build_human_response_mailbox_input_text(
    gate_id: Uuid,
    request_id: &str,
    turn_id: Option<&str>,
    payload: &serde_json::Value,
) -> String {
    let status = payload
        .get("status")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("responded");
    let summary = payload
        .get("summary")
        .and_then(serde_json::Value::as_str)
        .or_else(|| payload.get("choice").and_then(serde_json::Value::as_str))
        .or_else(|| payload.get("message").and_then(serde_json::Value::as_str))
        .unwrap_or("");
    let mut lines = vec![
        "Companion human response is available.".to_string(),
        format!("- request_id: {request_id}"),
        format!("- gate_id: {gate_id}"),
        format!("- status: {status}"),
    ];
    if let Some(turn_id) = turn_id {
        lines.push(format!("- requesting_turn_id: {turn_id}"));
    }
    if !summary.trim().is_empty() {
        lines.push(format!("- summary: {}", summary.trim()));
    }
    lines.join("\n")
}

fn payload_uuid(payload: &serde_json::Value, key: &str) -> Result<Uuid, ApplicationError> {
    let value = payload
        .get(key)
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| ApplicationError::Conflict(format!("gate payload 缺少 {key}")))?;
    Uuid::parse_str(value)
        .map_err(|_| ApplicationError::Conflict(format!("gate payload {key} 不是有效 UUID")))
}

#[cfg(test)]
mod tests {
    use std::{
        collections::{BTreeSet, HashMap, HashSet},
        str::FromStr,
        sync::{Arc, Mutex},
    };

    use agentdash_agent_runtime_contract::*;
    use agentdash_application_ports::agent_run_runtime::{
        AgentRunRuntimeBinding, AgentRunRuntimeBindingError,
    };
    use agentdash_domain::{
        DomainError,
        workflow::{
            AgentFrame, AgentLineage, AgentSource, GateWaitPolicy, GateWaitPolicyEnvelope,
            LifecycleAgent, LifecycleGate, WaitExpectedResult, WaitProducerRef,
            WaitTerminalOutcome, WaitTerminalPolicy, WaitWakeTarget,
        },
    };

    use super::*;

    #[test]
    fn parent_result_delivery_projection_text_is_bounded() {
        let long_summary = "s".repeat(1_200);
        let long_finding = "f".repeat(700);
        let findings = (0..13)
            .map(|_| serde_json::Value::String(long_finding.clone()))
            .collect::<Vec<_>>();
        let payload = serde_json::json!({
            "findings": findings,
            "follow_ups": ["short follow-up"],
        });

        let text = build_parent_result_delivery_projection_text(
            Uuid::new_v4(),
            "request-1",
            "companion",
            "failed",
            &long_summary,
            &payload,
        );

        assert!(text.starts_with("Companion result delivery projection."));
        assert!(text.contains(&format!("{}...", "s".repeat(1_000))));
        assert!(!text.contains(&"s".repeat(1_100)));
        assert!(text.contains(&format!("  - {}...", "f".repeat(500))));
        assert!(!text.contains(&"f".repeat(650)));
        assert!(text.contains("  - ... 1 more"));
    }

    #[derive(Default)]
    struct FixtureGateRepo {
        gates: Mutex<HashMap<Uuid, LifecycleGate>>,
    }

    #[async_trait]
    impl LifecycleGateRepository for FixtureGateRepo {
        async fn create(&self, gate: &LifecycleGate) -> Result<(), DomainError> {
            self.gates.lock().unwrap().insert(gate.id, gate.clone());
            Ok(())
        }

        async fn get(&self, id: Uuid) -> Result<Option<LifecycleGate>, DomainError> {
            Ok(self.gates.lock().unwrap().get(&id).cloned())
        }

        async fn list_open_for_agent(
            &self,
            agent_id: Uuid,
        ) -> Result<Vec<LifecycleGate>, DomainError> {
            Ok(self
                .gates
                .lock()
                .unwrap()
                .values()
                .filter(|gate| gate.agent_id == Some(agent_id) && gate.is_open())
                .cloned()
                .collect())
        }

        async fn list_open_gate_wait_policies(
            &self,
            limit: usize,
        ) -> Result<Vec<LifecycleGate>, DomainError> {
            Ok(self
                .gates
                .lock()
                .unwrap()
                .values()
                .filter(|gate| {
                    gate.is_open()
                        && gate
                            .payload_json
                            .as_ref()
                            .and_then(GateWaitPolicyEnvelope::from_payload_opt)
                            .is_some()
                })
                .take(limit)
                .cloned()
                .collect())
        }

        async fn list_by_wait_producer(
            &self,
            producer: &WaitProducerRef,
        ) -> Result<Vec<LifecycleGate>, DomainError> {
            Ok(self
                .gates
                .lock()
                .unwrap()
                .values()
                .filter(|gate| {
                    gate.payload_json
                        .as_ref()
                        .and_then(GateWaitPolicyEnvelope::from_payload_opt)
                        .is_some_and(|declaration| declaration.wait_policy.source == *producer)
                })
                .cloned()
                .collect())
        }

        async fn find_by_agent_and_correlation(
            &self,
            agent_id: Uuid,
            correlation_id: &str,
        ) -> Result<Option<LifecycleGate>, DomainError> {
            Ok(self
                .gates
                .lock()
                .unwrap()
                .values()
                .find(|gate| {
                    gate.agent_id == Some(agent_id) && gate.correlation_id == correlation_id
                })
                .cloned())
        }

        async fn update(&self, gate: &LifecycleGate) -> Result<(), DomainError> {
            self.gates.lock().unwrap().insert(gate.id, gate.clone());
            Ok(())
        }
    }

    #[derive(Default)]
    struct FixtureFrameRepo {
        frames: Mutex<HashMap<Uuid, AgentFrame>>,
        runtime_sessions_by_frame: Mutex<HashMap<Uuid, Vec<String>>>,
    }

    impl FixtureFrameRepo {
        fn seed_runtime_sessions<I, S>(&self, frame_id: Uuid, session_ids: I)
        where
            I: IntoIterator<Item = S>,
            S: Into<String>,
        {
            self.runtime_sessions_by_frame
                .lock()
                .unwrap()
                .insert(frame_id, session_ids.into_iter().map(Into::into).collect());
        }
    }

    #[async_trait]
    impl AgentFrameRepository for FixtureFrameRepo {
        async fn create(&self, frame: &AgentFrame) -> Result<(), DomainError> {
            self.frames.lock().unwrap().insert(frame.id, frame.clone());
            Ok(())
        }

        async fn get(&self, frame_id: Uuid) -> Result<Option<AgentFrame>, DomainError> {
            Ok(self.frames.lock().unwrap().get(&frame_id).cloned())
        }

        async fn get_current(&self, agent_id: Uuid) -> Result<Option<AgentFrame>, DomainError> {
            Ok(self
                .frames
                .lock()
                .unwrap()
                .values()
                .filter(|frame| frame.agent_id == agent_id)
                .max_by_key(|frame| frame.revision)
                .cloned())
        }

        async fn list_by_agent(&self, agent_id: Uuid) -> Result<Vec<AgentFrame>, DomainError> {
            Ok(self
                .frames
                .lock()
                .unwrap()
                .values()
                .filter(|frame| frame.agent_id == agent_id)
                .cloned()
                .collect())
        }
    }

    #[derive(Default)]
    struct FixtureLineageRepo {
        lineages: Mutex<Vec<AgentLineage>>,
    }

    #[async_trait]
    impl AgentLineageRepository for FixtureLineageRepo {
        async fn create(&self, lineage: &AgentLineage) -> Result<(), DomainError> {
            self.lineages.lock().unwrap().push(lineage.clone());
            Ok(())
        }

        async fn list_children(&self, agent_id: Uuid) -> Result<Vec<AgentLineage>, DomainError> {
            Ok(self
                .lineages
                .lock()
                .unwrap()
                .iter()
                .filter(|lineage| lineage.parent_agent_id == Some(agent_id))
                .cloned()
                .collect())
        }

        async fn find_parent(
            &self,
            child_agent_id: Uuid,
        ) -> Result<Option<AgentLineage>, DomainError> {
            Ok(self
                .lineages
                .lock()
                .unwrap()
                .iter()
                .find(|lineage| lineage.child_agent_id == child_agent_id)
                .cloned())
        }

        async fn list_by_run(&self, run_id: Uuid) -> Result<Vec<AgentLineage>, DomainError> {
            Ok(self
                .lineages
                .lock()
                .unwrap()
                .iter()
                .filter(|lineage| lineage.run_id == run_id)
                .cloned()
                .collect())
        }
    }

    #[derive(Default)]
    struct FixtureAgentRepo {
        agents: Mutex<HashMap<Uuid, LifecycleAgent>>,
    }

    impl FixtureAgentRepo {
        fn from_frame_repo(frame_repo: &FixtureFrameRepo, run_id: Uuid, project_id: Uuid) -> Self {
            let mut agents = HashMap::new();
            let frames: Vec<_> = frame_repo
                .frames
                .lock()
                .unwrap()
                .values()
                .cloned()
                .collect();
            for frame in &frames {
                let mut agent = LifecycleAgent::new_root(run_id, project_id, AgentSource::Unknown);
                agent.id = frame.agent_id;
                agent.status = "running".to_string();
                agents.entry(agent.id).or_insert(agent);
            }
            Self {
                agents: Mutex::new(agents),
            }
        }
    }

    #[async_trait]
    impl LifecycleAgentRepository for FixtureAgentRepo {
        async fn create(&self, agent: &LifecycleAgent) -> Result<(), DomainError> {
            self.agents.lock().unwrap().insert(agent.id, agent.clone());
            Ok(())
        }

        async fn get(&self, id: Uuid) -> Result<Option<LifecycleAgent>, DomainError> {
            Ok(self.agents.lock().unwrap().get(&id).cloned())
        }

        async fn list_by_run(&self, run_id: Uuid) -> Result<Vec<LifecycleAgent>, DomainError> {
            Ok(self
                .agents
                .lock()
                .unwrap()
                .values()
                .filter(|agent| agent.run_id == run_id)
                .cloned()
                .collect())
        }

        async fn update(&self, agent: &LifecycleAgent) -> Result<(), DomainError> {
            self.agents.lock().unwrap().insert(agent.id, agent.clone());
            Ok(())
        }
    }

    #[derive(Default)]
    struct FixtureRuntimeBindingRepo {
        bindings: Mutex<Vec<AgentRunRuntimeBinding>>,
    }

    impl FixtureRuntimeBindingRepo {
        fn from_frame_repo(frame_repo: &FixtureFrameRepo, run_id: Uuid) -> Self {
            let mut bindings = Vec::new();
            let sessions_by_frame = frame_repo.runtime_sessions_by_frame.lock().unwrap();
            for frame in frame_repo.frames.lock().unwrap().values() {
                if let Some(session_ids) = sessions_by_frame.get(&frame.id)
                    && let Some(runtime_session_id) = session_ids.last()
                {
                    bindings.push(runtime_binding(run_id, frame.agent_id, runtime_session_id));
                }
            }
            Self {
                bindings: Mutex::new(bindings),
            }
        }
    }

    #[async_trait]
    impl AgentRunRuntimeBindingRepository for FixtureRuntimeBindingRepo {
        async fn load(
            &self,
            target: &AgentRunRuntimeTarget,
        ) -> Result<Option<AgentRunRuntimeBinding>, AgentRunRuntimeBindingError> {
            Ok(self
                .bindings
                .lock()
                .unwrap()
                .iter()
                .find(|binding| &binding.target == target)
                .cloned())
        }
        async fn load_by_thread_id(
            &self,
            thread_id: &RuntimeThreadId,
        ) -> Result<Option<AgentRunRuntimeBinding>, AgentRunRuntimeBindingError> {
            Ok(self
                .bindings
                .lock()
                .unwrap()
                .iter()
                .find(|binding| &binding.thread_id == thread_id)
                .cloned())
        }
        async fn list_by_run(
            &self,
            run_id: Uuid,
        ) -> Result<Vec<AgentRunRuntimeBinding>, AgentRunRuntimeBindingError> {
            Ok(self
                .bindings
                .lock()
                .unwrap()
                .iter()
                .filter(|binding| binding.target.run_id == run_id)
                .cloned()
                .collect())
        }
        async fn list_by_agent(
            &self,
            agent_id: Uuid,
        ) -> Result<Vec<AgentRunRuntimeBinding>, AgentRunRuntimeBindingError> {
            Ok(self
                .bindings
                .lock()
                .unwrap()
                .iter()
                .filter(|binding| binding.target.agent_id == agent_id)
                .cloned()
                .collect())
        }
        async fn insert(
            &self,
            binding: AgentRunRuntimeBinding,
        ) -> Result<AgentRunRuntimeBinding, AgentRunRuntimeBindingError> {
            self.bindings.lock().unwrap().push(binding.clone());
            Ok(binding)
        }
    }

    fn runtime_id<T: FromStr>(value: &str) -> T
    where
        T::Err: std::fmt::Debug,
    {
        value.parse().expect("valid runtime id")
    }

    fn runtime_binding(run_id: Uuid, agent_id: Uuid, thread_id: &str) -> AgentRunRuntimeBinding {
        AgentRunRuntimeBinding {
            target: AgentRunRuntimeTarget { run_id, agent_id },
            thread_id: runtime_id(thread_id),
            binding_id: runtime_id(&format!("binding-{thread_id}")),
            driver_generation: RuntimeDriverGeneration(1),
            source_thread_id: runtime_id(&format!("source-{thread_id}")),
            profile_digest: runtime_id("profile-gate-control"),
            profile_provenance: ProfileProvenance {
                service_digest: runtime_id("service-gate-control"),
                transport_digest: runtime_id("transport-gate-control"),
                host_policy_digest: runtime_id("policy-gate-control"),
            },
            bound_profile: RuntimeProfile {
                reference_class: ReferenceRuntimeClass::ManagedThread,
                input: InputProfile {
                    modalities: BTreeSet::new(),
                },
                instruction: InstructionProfile {
                    channels: BTreeSet::new(),
                    configuration_boundary: ConfigurationBoundary::Binding,
                },
                tools: ToolProfile {
                    channels: BTreeSet::new(),
                    configuration_boundary: ConfigurationBoundary::Binding,
                    cancellation: true,
                },
                workspace: WorkspaceProfile {
                    capabilities: BTreeSet::new(),
                    mechanism: DeliveryMechanism::Native,
                },
                interactions: InteractionProfile {
                    kinds: BTreeSet::new(),
                    durable_correlation: true,
                },
                lifecycle: BTreeSet::new(),
                hooks: HookProfile {
                    points: Vec::new(),
                    configuration_boundary: ConfigurationBoundary::Binding,
                },
                context: ContextProfile {
                    capabilities: BTreeSet::new(),
                    fidelity: ContextFidelity::Opaque,
                    activation_idempotent: false,
                },
                telemetry_config: BTreeSet::new(),
            },
            surface_digest: runtime_id("surface-gate-control"),
            settings_revision: ThreadSettingsRevision(0),
            tool_set_revision: ToolSetRevision(0),
            hook_plan: BoundRuntimeHookPlan {
                revision: HookPlanRevision(1),
                digest: runtime_id("hook-gate-control"),
                entries: Vec::new(),
            },
        }
    }

    fn service_for_test(
        gate_repo: Arc<FixtureGateRepo>,
        frame_repo: Arc<FixtureFrameRepo>,
        lineage_repo: Arc<FixtureLineageRepo>,
        _delivery: Arc<CapturingDelivery>,
        run_id: Uuid,
    ) -> CompanionGateControlService {
        service_for_test_with_parent_mailbox(
            gate_repo,
            frame_repo,
            lineage_repo,
            Arc::new(CapturingDelivery::default()),
            Arc::new(CapturingParentMailboxDelivery::default()),
            run_id,
        )
    }

    fn service_for_test_with_parent_mailbox(
        gate_repo: Arc<FixtureGateRepo>,
        frame_repo: Arc<FixtureFrameRepo>,
        lineage_repo: Arc<FixtureLineageRepo>,
        _delivery: Arc<CapturingDelivery>,
        parent_mailbox_delivery: Arc<CapturingParentMailboxDelivery>,
        run_id: Uuid,
    ) -> CompanionGateControlService {
        service_for_test_with_mailboxes(
            gate_repo,
            frame_repo,
            lineage_repo,
            Arc::new(CapturingDelivery::default()),
            parent_mailbox_delivery,
            Arc::new(CapturingHumanMailboxDelivery::default()),
            run_id,
        )
    }

    fn service_for_test_with_mailboxes(
        gate_repo: Arc<FixtureGateRepo>,
        frame_repo: Arc<FixtureFrameRepo>,
        lineage_repo: Arc<FixtureLineageRepo>,
        _delivery: Arc<CapturingDelivery>,
        parent_mailbox_delivery: Arc<CapturingParentMailboxDelivery>,
        human_mailbox_delivery: Arc<CapturingHumanMailboxDelivery>,
        run_id: Uuid,
    ) -> CompanionGateControlService {
        let project_id = Uuid::new_v4();
        let agent_repo = Arc::new(FixtureAgentRepo::from_frame_repo(
            frame_repo.as_ref(),
            run_id,
            project_id,
        ));
        let runtime_binding_repo = Arc::new(FixtureRuntimeBindingRepo::from_frame_repo(
            frame_repo.as_ref(),
            run_id,
        ));
        CompanionGateControlService::new(CompanionGateControlDeps {
            repos: CompanionGateControlRepos {
                gate_repo,
                frame_repo,
                agent_repo,
                runtime_binding_repo,
                lineage_repo,
            },
        })
        .with_parent_mailbox_delivery(parent_mailbox_delivery)
        .with_human_response_mailbox_delivery(human_mailbox_delivery)
    }

    #[derive(Default)]
    struct CapturingDelivery {
        response_notifications: Mutex<Vec<()>>,
        event_notifications: Mutex<Vec<()>>,
    }

    #[derive(Default)]
    struct CapturingParentMailboxDelivery {
        commands: Mutex<Vec<CompanionParentMailboxDeliveryCommand>>,
        parent_request_commands: Mutex<Vec<CompanionParentRequestMailboxDeliveryCommand>>,
        parent_response_commands: Mutex<Vec<CompanionParentResponseMailboxDeliveryCommand>>,
        delivered_child_result_gate_ids: Mutex<HashSet<Uuid>>,
        fail_with: Mutex<Option<String>>,
    }

    #[derive(Default)]
    struct CapturingHumanMailboxDelivery {
        commands: Mutex<Vec<CompanionHumanResponseMailboxDeliveryCommand>>,
        fail_with: Mutex<Option<String>>,
    }

    impl CapturingHumanMailboxDelivery {
        fn fail_next(&self, message: impl Into<String>) {
            *self.fail_with.lock().unwrap() = Some(message.into());
        }
    }

    #[async_trait]
    impl CompanionHumanResponseMailboxDelivery for CapturingHumanMailboxDelivery {
        async fn deliver_human_response_to_requesting_agent(
            &self,
            command: CompanionHumanResponseMailboxDeliveryCommand,
        ) -> Result<CompanionParentMailboxDeliveryResult, ApplicationError> {
            self.commands.lock().unwrap().push(command);
            if let Some(message) = self.fail_with.lock().unwrap().take() {
                return Err(ApplicationError::Internal(message));
            }
            Ok(captured_mailbox_result("companion-human-response:test"))
        }
    }

    impl CapturingParentMailboxDelivery {
        fn fail_next(&self, message: impl Into<String>) {
            *self.fail_with.lock().unwrap() = Some(message.into());
        }
    }

    #[async_trait]
    impl CompanionParentMailboxDelivery for CapturingParentMailboxDelivery {
        async fn deliver_child_result_to_parent(
            &self,
            command: CompanionParentMailboxDeliveryCommand,
        ) -> Result<CompanionParentMailboxDeliveryResult, ApplicationError> {
            if self
                .delivered_child_result_gate_ids
                .lock()
                .unwrap()
                .contains(&command.gate_id)
            {
                return Ok(captured_mailbox_result_with_duplicate(
                    "companion-result:test",
                    true,
                ));
            }
            self.commands.lock().unwrap().push(command.clone());
            if let Some(message) = self.fail_with.lock().unwrap().take() {
                return Err(ApplicationError::Internal(message));
            }
            self.delivered_child_result_gate_ids
                .lock()
                .unwrap()
                .insert(command.gate_id);
            Ok(captured_mailbox_result("companion-result:test"))
        }

        async fn deliver_parent_request_to_parent(
            &self,
            command: CompanionParentRequestMailboxDeliveryCommand,
        ) -> Result<CompanionParentMailboxDeliveryResult, ApplicationError> {
            self.parent_request_commands.lock().unwrap().push(command);
            if let Some(message) = self.fail_with.lock().unwrap().take() {
                return Err(ApplicationError::Internal(message));
            }
            Ok(captured_mailbox_result("companion-parent-request:test"))
        }

        async fn deliver_parent_response_to_child(
            &self,
            command: CompanionParentResponseMailboxDeliveryCommand,
        ) -> Result<CompanionParentMailboxDeliveryResult, ApplicationError> {
            self.parent_response_commands.lock().unwrap().push(command);
            if let Some(message) = self.fail_with.lock().unwrap().take() {
                return Err(ApplicationError::Internal(message));
            }
            Ok(captured_mailbox_result("companion-parent-response:test"))
        }
    }

    fn captured_mailbox_result(
        client_command_id: impl Into<String>,
    ) -> CompanionParentMailboxDeliveryResult {
        captured_mailbox_result_with_duplicate(client_command_id, false)
    }

    fn captured_mailbox_result_with_duplicate(
        client_command_id: impl Into<String>,
        duplicate: bool,
    ) -> CompanionParentMailboxDeliveryResult {
        CompanionParentMailboxDeliveryResult {
            mailbox_message_id: Some(Uuid::new_v4()),
            accepted_runtime_operation_id: Some("operation-test".to_string()),
            command_receipt_client_command_id: client_command_id.into(),
            command_receipt_status: "accepted".to_string(),
            command_receipt_duplicate: duplicate,
            outcome: "queued".to_string(),
            runtime_operation_id: Some("operation-parent-1".to_string()),
        }
    }

    #[tokio::test]
    async fn respond_resolves_gate_and_delivers_by_anchor_runtime_ref() {
        let run_id = Uuid::new_v4();
        let agent_id = Uuid::new_v4();
        let frame = AgentFrame::new_revision(agent_id, 1, "test");
        let frame_id = frame.id;
        let gate = LifecycleGate::open(
            run_id,
            Some(agent_id),
            Some(frame_id),
            "companion_wait",
            "human-request",
            Some(serde_json::json!({
                "turn_id": "turn-1",
                "request_type": "decision"
            })),
        );
        let gate_id = gate.id;

        let gate_repo = Arc::new(FixtureGateRepo::default());
        gate_repo.create(&gate).await.expect("seed gate");
        let frame_repo = Arc::new(FixtureFrameRepo::default());
        frame_repo.create(&frame).await.expect("seed frame");
        frame_repo.seed_runtime_sessions(frame_id, ["session-old", "session-latest"]);
        let lineage_repo = Arc::new(FixtureLineageRepo::default());
        let delivery = Arc::new(CapturingDelivery::default());
        let parent_mailbox_delivery = Arc::new(CapturingParentMailboxDelivery::default());
        let human_mailbox_delivery = Arc::new(CapturingHumanMailboxDelivery::default());
        let service = service_for_test_with_mailboxes(
            gate_repo.clone(),
            frame_repo,
            lineage_repo,
            delivery.clone(),
            parent_mailbox_delivery,
            human_mailbox_delivery.clone(),
            run_id,
        );

        let result = service
            .respond(RespondCompanionGateCommand {
                gate_id,
                payload: serde_json::json!({
                    "type": "decision",
                    "status": "approved",
                    "choice": "YES",
                    "summary": "YES"
                }),
            })
            .await
            .expect("respond");

        assert!(result.gate_resolved);
        assert_eq!(result.runtime_thread_id.as_deref(), Some("session-latest"));
        let stored = gate_repo
            .get(gate_id)
            .await
            .expect("load gate")
            .expect("gate exists");
        assert!(!stored.is_open());
        assert_eq!(stored.resolved_by.as_deref(), Some("companion_respond"));
        assert_eq!(
            stored
                .payload_json
                .as_ref()
                .and_then(|payload| payload.get("status"))
                .and_then(serde_json::Value::as_str),
            Some("approved")
        );
        assert!(
            stored
                .payload_json
                .as_ref()
                .and_then(|payload| payload.get("human_mailbox_delivery"))
                .is_none()
        );

        let notifications = delivery.response_notifications.lock().unwrap();
        assert!(notifications.is_empty());
        let commands = human_mailbox_delivery.commands.lock().unwrap();
        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].gate_id, gate_id);
        assert_eq!(commands[0].agent_id, agent_id);
        assert_eq!(commands[0].runtime_thread_id, "session-latest");
        assert_eq!(commands[0].turn_id.as_deref(), Some("turn-1"));
        assert_eq!(commands[0].request_id, gate_id.to_string());
    }

    #[tokio::test]
    async fn respond_rejects_already_closed_gate() {
        let run_id = Uuid::new_v4();
        let agent_id = Uuid::new_v4();
        let frame = AgentFrame::new_revision(agent_id, 1, "test");
        let mut gate = LifecycleGate::open(
            run_id,
            Some(agent_id),
            Some(frame.id),
            "companion_wait",
            "human-request",
            Some(serde_json::json!({ "request_type": "decision" })),
        );
        gate.resolve("previous");

        let gate_repo = Arc::new(FixtureGateRepo::default());
        gate_repo.create(&gate).await.expect("seed gate");
        let frame_repo = Arc::new(FixtureFrameRepo::default());
        frame_repo.create(&frame).await.expect("seed frame");
        frame_repo.seed_runtime_sessions(frame.id, ["session-1"]);
        let lineage_repo = Arc::new(FixtureLineageRepo::default());
        let delivery = Arc::new(CapturingDelivery::default());
        let service = service_for_test(
            gate_repo,
            frame_repo,
            lineage_repo,
            delivery.clone(),
            run_id,
        );

        let error = service
            .respond(RespondCompanionGateCommand {
                gate_id: gate.id,
                payload: serde_json::json!({ "type": "decision", "status": "approved" }),
            })
            .await
            .expect_err("closed gate should be rejected");

        assert!(matches!(error, ApplicationError::Conflict(_)));
        assert!(delivery.response_notifications.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn respond_keeps_delivery_failure_out_of_gate_payload() {
        let run_id = Uuid::new_v4();
        let agent_id = Uuid::new_v4();
        let frame = AgentFrame::new_revision(agent_id, 1, "test");
        let frame_id = frame.id;
        let gate = LifecycleGate::open(
            run_id,
            Some(agent_id),
            Some(frame_id),
            "companion_wait",
            "human-request",
            Some(serde_json::json!({
                "turn_id": "turn-1",
                "request_type": "decision"
            })),
        );
        let gate_id = gate.id;

        let gate_repo = Arc::new(FixtureGateRepo::default());
        gate_repo.create(&gate).await.expect("seed gate");
        let frame_repo = Arc::new(FixtureFrameRepo::default());
        frame_repo.create(&frame).await.expect("seed frame");
        frame_repo.seed_runtime_sessions(frame_id, ["session-latest"]);
        let lineage_repo = Arc::new(FixtureLineageRepo::default());
        let delivery = Arc::new(CapturingDelivery::default());
        let parent_mailbox_delivery = Arc::new(CapturingParentMailboxDelivery::default());
        let human_mailbox_delivery = Arc::new(CapturingHumanMailboxDelivery::default());
        human_mailbox_delivery.fail_next("mailbox unavailable");
        let service = service_for_test_with_mailboxes(
            gate_repo.clone(),
            frame_repo,
            lineage_repo,
            delivery,
            parent_mailbox_delivery,
            human_mailbox_delivery.clone(),
            run_id,
        );

        let error = service
            .respond(RespondCompanionGateCommand {
                gate_id,
                payload: serde_json::json!({
                    "type": "decision",
                    "status": "approved",
                    "choice": "YES",
                    "summary": "YES"
                }),
            })
            .await
            .expect_err("mailbox failure should be returned");

        assert!(matches!(error, ApplicationError::Internal(_)));
        let stored = gate_repo
            .get(gate_id)
            .await
            .expect("load gate")
            .expect("gate exists");
        assert!(!stored.is_open());
        assert_eq!(
            stored
                .payload_json
                .as_ref()
                .and_then(|payload| payload.get("status"))
                .and_then(serde_json::Value::as_str),
            Some("approved")
        );
        assert!(
            stored
                .payload_json
                .as_ref()
                .and_then(|payload| payload.get("human_mailbox_delivery"))
                .is_none()
        );
        assert_eq!(human_mailbox_delivery.commands.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn complete_child_result_resolves_child_owned_gate_and_delivers_parent_mailbox_wake() {
        let run_id = Uuid::new_v4();
        let parent_agent_id = Uuid::new_v4();
        let child_agent_id = Uuid::new_v4();

        let parent_frame = AgentFrame::new_revision(parent_agent_id, 1, "parent");
        let child_frame = AgentFrame::new_revision(child_agent_id, 1, "child");

        let gate = LifecycleGate::open(
            run_id,
            Some(child_agent_id),
            Some(child_frame.id),
            "companion_wait_follow_up",
            "dispatch-1",
            Some(serde_json::json!({
                "parent_agent_id": parent_agent_id,
                "dispatch_id": "dispatch-1",
            })),
        );
        let gate_id = gate.id;
        let lineage = AgentLineage::new(
            run_id,
            Some(parent_agent_id),
            child_agent_id,
            "companion",
            Some(child_frame.id),
            None,
        );

        let gate_repo = Arc::new(FixtureGateRepo::default());
        gate_repo.create(&gate).await.expect("seed gate");
        let frame_repo = Arc::new(FixtureFrameRepo::default());
        frame_repo
            .create(&parent_frame)
            .await
            .expect("seed parent frame");
        frame_repo.seed_runtime_sessions(parent_frame.id, ["parent-session"]);
        frame_repo
            .create(&child_frame)
            .await
            .expect("seed child frame");
        frame_repo.seed_runtime_sessions(child_frame.id, ["child-session"]);
        let lineage_repo = Arc::new(FixtureLineageRepo::default());
        lineage_repo.create(&lineage).await.expect("seed lineage");
        let delivery = Arc::new(CapturingDelivery::default());
        let parent_mailbox_delivery = Arc::new(CapturingParentMailboxDelivery::default());
        let service = service_for_test_with_parent_mailbox(
            gate_repo.clone(),
            frame_repo,
            lineage_repo,
            delivery.clone(),
            parent_mailbox_delivery.clone(),
            run_id,
        );

        let result = service
            .complete_child_result_to_parent(CompleteCompanionChildResultCommand {
                request_id: "dispatch-1".to_string(),
                child_runtime_session_id: "child-session".to_string(),
                resolved_turn_id: "turn-child-1".to_string(),
                payload: serde_json::json!({
                    "status": "completed",
                    "summary": "review complete",
                    "findings": ["looks good"],
                    "follow_ups": [],
                    "artifact_refs": [],
                }),
            })
            .await
            .expect("complete child result")
            .expect("matched gate");

        assert_eq!(result.gate_id, gate_id);
        assert_eq!(result.parent_agent_id, parent_agent_id);
        assert_eq!(
            result.parent_runtime_thread_id.as_deref(),
            Some("parent-session")
        );
        assert_eq!(
            result.child_runtime_thread_id.as_deref(),
            Some("child-session")
        );
        assert_eq!(result.parent_mailbox_delivery.outcome, "queued");

        let stored = gate_repo
            .get(gate_id)
            .await
            .expect("load gate")
            .expect("gate exists");
        assert!(!stored.is_open());
        let expected_resolved_by = format!("child_agent:{child_agent_id}");
        assert_eq!(
            stored.resolved_by.as_deref(),
            Some(expected_resolved_by.as_str())
        );
        assert_eq!(
            stored
                .payload_json
                .as_ref()
                .and_then(|payload| payload.get("status"))
                .and_then(serde_json::Value::as_str),
            Some("completed")
        );
        assert_eq!(
            stored
                .payload_json
                .as_ref()
                .and_then(|payload| payload.get("parent_mailbox_delivery"))
                .and_then(|delivery| delivery.get("status"))
                .and_then(serde_json::Value::as_str),
            None
        );

        {
            let mailbox_commands = parent_mailbox_delivery.commands.lock().unwrap();
            assert_eq!(mailbox_commands.len(), 1);
            assert_eq!(mailbox_commands[0].gate_id, gate_id);
            assert_eq!(mailbox_commands[0].request_id, "dispatch-1");
            assert_eq!(mailbox_commands[0].run_id, run_id);
            assert_eq!(mailbox_commands[0].parent_agent_id, parent_agent_id);
            assert_eq!(mailbox_commands[0].child_agent_id, child_agent_id);
            assert_eq!(
                mailbox_commands[0].parent_runtime_thread_id,
                "parent-session"
            );
            assert!(
                mailbox_commands[0]
                    .input_text
                    .contains("- status: completed")
            );
            assert!(
                mailbox_commands[0]
                    .input_text
                    .contains("- summary: review complete")
            );
            assert!(
                mailbox_commands[0]
                    .input_text
                    .starts_with("Companion result delivery projection.")
            );
        }

        let duplicate = service
            .complete_child_result_to_parent(CompleteCompanionChildResultCommand {
                request_id: "dispatch-1".to_string(),
                child_runtime_session_id: "child-session".to_string(),
                resolved_turn_id: "turn-child-2".to_string(),
                payload: serde_json::json!({
                    "status": "completed",
                    "summary": "duplicate",
                    "findings": [],
                    "follow_ups": [],
                    "artifact_refs": [],
                }),
            })
            .await
            .expect("duplicate child result should ensure delivery")
            .expect("resolved gate should return delivery result");
        assert!(duplicate.parent_mailbox_delivery.command_receipt_duplicate);
        assert_eq!(parent_mailbox_delivery.commands.lock().unwrap().len(), 1);
        assert!(delivery.event_notifications.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn observe_gate_producer_terminal_failed_resolves_gate_and_delivers_parent_wake() {
        let run_id = Uuid::new_v4();
        let parent_agent_id = Uuid::new_v4();
        let child_agent_id = Uuid::new_v4();

        let parent_frame = AgentFrame::new_revision(parent_agent_id, 1, "parent");
        let child_frame = AgentFrame::new_revision(child_agent_id, 1, "child");

        let mut gate = LifecycleGate::open(
            run_id,
            Some(child_agent_id),
            Some(child_frame.id),
            "companion_wait_follow_up",
            "dispatch-terminal",
            Some(serde_json::json!({
                "parent_agent_id": parent_agent_id,
                "dispatch_id": "dispatch-terminal",
                "companion_label": "project-survey",
            })),
        );
        let gate_id = gate.id;
        let declaration = GateWaitPolicyEnvelope::new(GateWaitPolicy {
            source: WaitProducerRef::AgentRunDelivery {
                run_id,
                agent_id: child_agent_id,
                frame_id: Some(child_frame.id),
            },
            expected_result: WaitExpectedResult {
                kind: "companion_result".to_string(),
                correlation_ref: Some("dispatch-terminal".to_string()),
            },
            terminal_policy: WaitTerminalPolicy {
                failed: WaitTerminalOutcome {
                    status: "failed".to_string(),
                    failure_kind: "runtime_terminal_failed".to_string(),
                },
                interrupted: WaitTerminalOutcome {
                    status: "cancelled".to_string(),
                    failure_kind: "runtime_terminal_cancelled".to_string(),
                },
                completed: WaitTerminalOutcome {
                    status: "failed".to_string(),
                    failure_kind: "missing_companion_respond".to_string(),
                },
            },
            wake_target: WaitWakeTarget {
                namespace: "companion".to_string(),
                target_run_id: run_id,
                target_agent_id: parent_agent_id,
                client_command_id: format!("companion-result:{gate_id}"),
            },
        })
        .with_display_value("companion_label", serde_json::json!("project-survey"));
        gate.payload_json = Some(
            declaration
                .write_into_payload(gate.payload_json.take())
                .expect("write gate wait policy"),
        );
        let lineage = AgentLineage::new(
            run_id,
            Some(parent_agent_id),
            child_agent_id,
            "companion",
            Some(child_frame.id),
            None,
        );

        let gate_repo = Arc::new(FixtureGateRepo::default());
        gate_repo.create(&gate).await.expect("seed gate");
        let frame_repo = Arc::new(FixtureFrameRepo::default());
        frame_repo
            .create(&parent_frame)
            .await
            .expect("seed parent frame");
        frame_repo.seed_runtime_sessions(parent_frame.id, ["parent-session"]);
        frame_repo
            .create(&child_frame)
            .await
            .expect("seed child frame");
        frame_repo.seed_runtime_sessions(child_frame.id, ["child-session"]);
        let lineage_repo = Arc::new(FixtureLineageRepo::default());
        lineage_repo.create(&lineage).await.expect("seed lineage");
        let delivery = Arc::new(CapturingDelivery::default());
        let parent_mailbox_delivery = Arc::new(CapturingParentMailboxDelivery::default());
        let service = service_for_test_with_parent_mailbox(
            gate_repo.clone(),
            frame_repo,
            lineage_repo,
            delivery.clone(),
            parent_mailbox_delivery.clone(),
            run_id,
        );

        let event = GateProducerTerminalEvent {
            producer: WaitProducerRef::AgentRunDelivery {
                run_id,
                agent_id: child_agent_id,
                frame_id: Some(child_frame.id),
            },
            terminal_state: "failed".to_string(),
            terminal_message: Some("provider model unsupported".to_string()),
            terminal_diagnostic: None,
            producer_last_message: None,
            source_turn_id: Some("turn-child-1".to_string()),
            trace_ref: Some("child-session".to_string()),
        };
        let result = service
            .observe_gate_producer_terminal(event.clone())
            .await
            .expect("terminal convergence");

        assert_eq!(result.outcomes.len(), 1);
        assert_eq!(result.outcomes[0].gate_id, gate_id);
        assert_eq!(
            result.outcomes[0].kind,
            agentdash_application_workflow::gate::GateProducerTerminalConvergenceOutcomeKind::Resolved
        );
        assert_eq!(result.outcomes[0].result_status.as_deref(), Some("failed"));

        let stored = gate_repo
            .get(gate_id)
            .await
            .expect("load gate")
            .expect("gate exists");
        assert!(!stored.is_open());
        let payload = stored.payload_json.as_ref().expect("resolved payload");
        assert_eq!(
            payload.get("status").and_then(serde_json::Value::as_str),
            Some("failed")
        );
        assert_eq!(
            payload
                .get("terminal_state")
                .and_then(serde_json::Value::as_str),
            Some("failed")
        );
        assert_eq!(
            payload
                .get("terminal_message")
                .and_then(serde_json::Value::as_str),
            Some("provider model unsupported")
        );
        assert_eq!(
            payload.get("source").and_then(serde_json::Value::as_str),
            Some("producer_terminal")
        );
        assert_eq!(
            payload
                .get("delivery_trace_ref")
                .and_then(serde_json::Value::as_str),
            Some("child-session")
        );
        assert_eq!(
            payload
                .get("failure_kind")
                .and_then(serde_json::Value::as_str),
            Some("runtime_terminal_failed")
        );
        assert_eq!(
            payload
                .get("wait_policy")
                .and_then(|value| value.get("source"))
                .and_then(|value| value.get("kind"))
                .and_then(serde_json::Value::as_str),
            Some("agent_run_delivery")
        );

        {
            let mailbox_commands = parent_mailbox_delivery.commands.lock().unwrap();
            assert_eq!(mailbox_commands.len(), 1);
            assert_eq!(mailbox_commands[0].gate_id, gate_id);
            assert_eq!(mailbox_commands[0].request_id, "dispatch-terminal");
            assert_eq!(mailbox_commands[0].parent_agent_id, parent_agent_id);
            assert_eq!(mailbox_commands[0].child_agent_id, child_agent_id);
            assert!(mailbox_commands[0].input_text.contains("- status: failed"));
        }

        let replay = service
            .observe_gate_producer_terminal(event)
            .await
            .expect("terminal convergence replay");
        assert_eq!(replay.outcomes.len(), 1);
        assert_eq!(
            replay.outcomes[0].kind,
            agentdash_application_workflow::gate::GateProducerTerminalConvergenceOutcomeKind::AlreadyResolvedEnsuredDelivery
        );
        assert_eq!(parent_mailbox_delivery.commands.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn open_parent_request_creates_parent_owned_gate_and_delivery_event() {
        let run_id = Uuid::new_v4();
        let parent_agent_id = Uuid::new_v4();
        let child_agent_id = Uuid::new_v4();

        let parent_frame = AgentFrame::new_revision(parent_agent_id, 1, "parent");
        let parent_frame_id = parent_frame.id;
        let child_frame = AgentFrame::new_revision(child_agent_id, 1, "child");
        let child_frame_id = child_frame.id;
        let lineage = AgentLineage::new(
            run_id,
            Some(parent_agent_id),
            child_agent_id,
            "companion",
            Some(child_frame_id),
            None,
        );

        let gate_repo = Arc::new(FixtureGateRepo::default());
        let frame_repo = Arc::new(FixtureFrameRepo::default());
        frame_repo
            .create(&parent_frame)
            .await
            .expect("seed parent frame");
        frame_repo.seed_runtime_sessions(parent_frame_id, ["parent-session"]);
        frame_repo
            .create(&child_frame)
            .await
            .expect("seed child frame");
        frame_repo.seed_runtime_sessions(child_frame_id, ["child-session"]);
        let lineage_repo = Arc::new(FixtureLineageRepo::default());
        lineage_repo.create(&lineage).await.expect("seed lineage");
        let delivery = Arc::new(CapturingDelivery::default());
        let parent_mailbox_delivery = Arc::new(CapturingParentMailboxDelivery::default());
        let service = service_for_test_with_parent_mailbox(
            gate_repo.clone(),
            frame_repo,
            lineage_repo,
            delivery.clone(),
            parent_mailbox_delivery.clone(),
            run_id,
        );

        let result = service
            .open_parent_request(OpenCompanionParentRequestCommand {
                child_runtime_session_id: "child-session".to_string(),
                turn_id: "turn-child-1".to_string(),
                wait: true,
                payload: serde_json::json!({ "message": "please review" }),
            })
            .await
            .expect("open parent request");

        assert_eq!(result.request_id, result.gate_id.to_string());
        assert_eq!(result.run_id, run_id);
        assert_eq!(result.parent_agent_id, parent_agent_id);
        assert_eq!(result.parent_frame_id, parent_frame_id);
        assert_eq!(result.child_agent_id, child_agent_id);
        assert_eq!(result.child_frame_id, child_frame_id);
        assert_eq!(result.parent_runtime_thread_id, "parent-session");
        assert_eq!(result.child_runtime_thread_id, "child-session");

        let stored = gate_repo
            .get(result.gate_id)
            .await
            .expect("load gate")
            .expect("gate exists");
        assert!(stored.is_open());
        assert_eq!(stored.agent_id, Some(parent_agent_id));
        assert_eq!(stored.frame_id, Some(parent_frame_id));
        assert_eq!(stored.gate_kind, COMPANION_PARENT_REQUEST_GATE_KIND);
        assert_eq!(stored.correlation_id, result.gate_id.to_string());
        assert_eq!(
            stored
                .payload_json
                .as_ref()
                .and_then(|payload| payload.get("request_id"))
                .and_then(serde_json::Value::as_str),
            Some(result.request_id.as_str())
        );
        assert!(
            stored
                .payload_json
                .as_ref()
                .and_then(|payload| payload.get("parent_mailbox_delivery"))
                .is_none()
        );
        assert_eq!(
            result
                .parent_mailbox_delivery
                .command_receipt_client_command_id,
            "companion-parent-request:test"
        );

        let parent_request_commands = parent_mailbox_delivery
            .parent_request_commands
            .lock()
            .unwrap();
        assert_eq!(parent_request_commands.len(), 1);
        assert_eq!(parent_request_commands[0].gate_id, result.gate_id);
        assert_eq!(parent_request_commands[0].run_id, run_id);
        assert_eq!(parent_request_commands[0].parent_agent_id, parent_agent_id);
        assert_eq!(parent_request_commands[0].child_agent_id, child_agent_id);
        assert_eq!(
            parent_request_commands[0].parent_runtime_thread_id,
            "parent-session"
        );
        assert_eq!(
            parent_request_commands[0].child_runtime_thread_id,
            "child-session"
        );

        assert!(delivery.event_notifications.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn open_parent_request_keeps_mailbox_failure_out_of_gate_payload() {
        let run_id = Uuid::new_v4();
        let parent_agent_id = Uuid::new_v4();
        let child_agent_id = Uuid::new_v4();
        let parent_frame = AgentFrame::new_revision(parent_agent_id, 1, "parent");
        let child_frame = AgentFrame::new_revision(child_agent_id, 1, "child");
        let lineage = AgentLineage::new(
            run_id,
            Some(parent_agent_id),
            child_agent_id,
            "companion",
            Some(child_frame.id),
            None,
        );

        let gate_repo = Arc::new(FixtureGateRepo::default());
        let frame_repo = Arc::new(FixtureFrameRepo::default());
        frame_repo
            .create(&parent_frame)
            .await
            .expect("seed parent frame");
        frame_repo.seed_runtime_sessions(parent_frame.id, ["parent-session"]);
        frame_repo
            .create(&child_frame)
            .await
            .expect("seed child frame");
        frame_repo.seed_runtime_sessions(child_frame.id, ["child-session"]);
        let lineage_repo = Arc::new(FixtureLineageRepo::default());
        lineage_repo.create(&lineage).await.expect("seed lineage");
        let delivery = Arc::new(CapturingDelivery::default());
        let parent_mailbox_delivery = Arc::new(CapturingParentMailboxDelivery::default());
        parent_mailbox_delivery.fail_next("parent mailbox unavailable");
        let service = service_for_test_with_parent_mailbox(
            gate_repo.clone(),
            frame_repo,
            lineage_repo,
            delivery.clone(),
            parent_mailbox_delivery.clone(),
            run_id,
        );

        let error = service
            .open_parent_request(OpenCompanionParentRequestCommand {
                child_runtime_session_id: "child-session".to_string(),
                turn_id: "turn-child-1".to_string(),
                wait: false,
                payload: serde_json::json!({ "message": "please review" }),
            })
            .await
            .expect_err("mailbox failure should fail command");

        assert!(matches!(error, ApplicationError::Internal(_)));
        assert!(delivery.event_notifications.lock().unwrap().is_empty());
        let gates = gate_repo.gates.lock().unwrap();
        let stored = gates.values().next().expect("gate persisted");
        assert!(stored.is_open());
        assert!(
            stored
                .payload_json
                .as_ref()
                .and_then(|payload| payload.get("parent_mailbox_delivery"))
                .is_none()
        );
        assert_eq!(
            parent_mailbox_delivery
                .parent_request_commands
                .lock()
                .unwrap()
                .len(),
            1
        );
    }

    #[tokio::test]
    async fn complete_child_result_keeps_mailbox_failure_out_of_gate_payload() {
        let run_id = Uuid::new_v4();
        let parent_agent_id = Uuid::new_v4();
        let child_agent_id = Uuid::new_v4();

        let parent_frame = AgentFrame::new_revision(parent_agent_id, 1, "parent");
        let child_frame = AgentFrame::new_revision(child_agent_id, 1, "child");
        let gate = LifecycleGate::open(
            run_id,
            Some(child_agent_id),
            Some(child_frame.id),
            "companion_wait_follow_up",
            "dispatch-fail",
            Some(serde_json::json!({
                "parent_agent_id": parent_agent_id,
                "dispatch_id": "dispatch-fail",
                "companion_label": "reviewer",
            })),
        );
        let gate_id = gate.id;
        let lineage = AgentLineage::new(
            run_id,
            Some(parent_agent_id),
            child_agent_id,
            "companion",
            Some(child_frame.id),
            None,
        );

        let gate_repo = Arc::new(FixtureGateRepo::default());
        gate_repo.create(&gate).await.expect("seed gate");
        let frame_repo = Arc::new(FixtureFrameRepo::default());
        frame_repo
            .create(&parent_frame)
            .await
            .expect("seed parent frame");
        frame_repo.seed_runtime_sessions(parent_frame.id, ["parent-session"]);
        frame_repo
            .create(&child_frame)
            .await
            .expect("seed child frame");
        frame_repo.seed_runtime_sessions(child_frame.id, ["child-session"]);
        let lineage_repo = Arc::new(FixtureLineageRepo::default());
        lineage_repo.create(&lineage).await.expect("seed lineage");
        let delivery = Arc::new(CapturingDelivery::default());
        let parent_mailbox_delivery = Arc::new(CapturingParentMailboxDelivery::default());
        parent_mailbox_delivery.fail_next("mailbox unavailable");
        let service = service_for_test_with_parent_mailbox(
            gate_repo.clone(),
            frame_repo,
            lineage_repo,
            delivery.clone(),
            parent_mailbox_delivery.clone(),
            run_id,
        );

        let command = CompleteCompanionChildResultCommand {
            request_id: "dispatch-fail".to_string(),
            child_runtime_session_id: "child-session".to_string(),
            resolved_turn_id: "turn-child-1".to_string(),
            payload: serde_json::json!({
                "status": "completed",
                "summary": "review complete",
            }),
        };
        let error = service
            .complete_child_result_to_parent(command.clone())
            .await
            .expect_err("mailbox failure should be returned");

        assert!(matches!(error, ApplicationError::Internal(_)));
        let stored = gate_repo
            .get(gate_id)
            .await
            .expect("load gate")
            .expect("gate exists");
        assert!(!stored.is_open());
        assert_eq!(
            stored
                .payload_json
                .as_ref()
                .and_then(|payload| payload.get("parent_mailbox_delivery"))
                .and_then(|delivery| delivery.get("status"))
                .and_then(serde_json::Value::as_str),
            None
        );
        assert_eq!(parent_mailbox_delivery.commands.lock().unwrap().len(), 1);
        assert!(delivery.event_notifications.lock().unwrap().is_empty());

        let retry = service
            .complete_child_result_to_parent(command)
            .await
            .expect("resolved gate delivery retry")
            .expect("resolved gate should retry delivery");
        assert!(!retry.parent_mailbox_delivery.command_receipt_duplicate);
        assert_eq!(parent_mailbox_delivery.commands.lock().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn duplicate_child_result_does_not_deliver_second_parent_mailbox_message() {
        let run_id = Uuid::new_v4();
        let parent_agent_id = Uuid::new_v4();
        let child_agent_id = Uuid::new_v4();

        let parent_frame = AgentFrame::new_revision(parent_agent_id, 1, "parent");
        let child_frame = AgentFrame::new_revision(child_agent_id, 1, "child");
        let gate = LifecycleGate::open(
            run_id,
            Some(child_agent_id),
            Some(child_frame.id),
            "companion_wait_follow_up",
            "dispatch-duplicate",
            Some(serde_json::json!({ "dispatch_id": "dispatch-duplicate" })),
        );
        let lineage = AgentLineage::new(
            run_id,
            Some(parent_agent_id),
            child_agent_id,
            "companion",
            Some(child_frame.id),
            None,
        );

        let gate_repo = Arc::new(FixtureGateRepo::default());
        gate_repo.create(&gate).await.expect("seed gate");
        let frame_repo = Arc::new(FixtureFrameRepo::default());
        frame_repo
            .create(&parent_frame)
            .await
            .expect("seed parent frame");
        frame_repo.seed_runtime_sessions(parent_frame.id, ["parent-session"]);
        frame_repo
            .create(&child_frame)
            .await
            .expect("seed child frame");
        frame_repo.seed_runtime_sessions(child_frame.id, ["child-session"]);
        let lineage_repo = Arc::new(FixtureLineageRepo::default());
        lineage_repo.create(&lineage).await.expect("seed lineage");
        let delivery = Arc::new(CapturingDelivery::default());
        let parent_mailbox_delivery = Arc::new(CapturingParentMailboxDelivery::default());
        let service = service_for_test_with_parent_mailbox(
            gate_repo,
            frame_repo,
            lineage_repo,
            delivery,
            parent_mailbox_delivery.clone(),
            run_id,
        );

        let command = CompleteCompanionChildResultCommand {
            request_id: "dispatch-duplicate".to_string(),
            child_runtime_session_id: "child-session".to_string(),
            resolved_turn_id: "turn-child-1".to_string(),
            payload: serde_json::json!({
                "status": "completed",
                "summary": "review complete",
            }),
        };
        assert!(
            service
                .complete_child_result_to_parent(command.clone())
                .await
                .expect("first completion")
                .is_some()
        );
        assert!(
            service
                .complete_child_result_to_parent(command)
                .await
                .expect("duplicate completion")
                .expect("resolved gate should ensure delivery")
                .parent_mailbox_delivery
                .command_receipt_duplicate
        );
        assert_eq!(parent_mailbox_delivery.commands.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn open_parent_request_uses_parent_current_frame_after_delivery_refresh() {
        let run_id = Uuid::new_v4();
        let parent_agent_id = Uuid::new_v4();
        let child_agent_id = Uuid::new_v4();

        let parent_launch_frame = AgentFrame::new_revision(parent_agent_id, 1, "parent-launch");
        let parent_current_frame = AgentFrame::new_revision(parent_agent_id, 2, "parent-current");
        let child_frame = AgentFrame::new_revision(child_agent_id, 1, "child");
        let lineage = AgentLineage::new(
            run_id,
            Some(parent_agent_id),
            child_agent_id,
            "companion",
            Some(child_frame.id),
            None,
        );

        let gate_repo = Arc::new(FixtureGateRepo::default());
        let frame_repo = Arc::new(FixtureFrameRepo::default());
        frame_repo
            .create(&parent_launch_frame)
            .await
            .expect("seed parent launch frame");
        frame_repo
            .create(&parent_current_frame)
            .await
            .expect("seed parent current frame");
        frame_repo.seed_runtime_sessions(parent_current_frame.id, ["parent-current-session"]);
        frame_repo
            .create(&child_frame)
            .await
            .expect("seed child frame");
        frame_repo.seed_runtime_sessions(child_frame.id, ["child-session"]);
        let lineage_repo = Arc::new(FixtureLineageRepo::default());
        lineage_repo.create(&lineage).await.expect("seed lineage");
        let delivery = Arc::new(CapturingDelivery::default());
        let service = service_for_test(
            gate_repo.clone(),
            frame_repo,
            lineage_repo,
            delivery.clone(),
            run_id,
        );

        let result = service
            .open_parent_request(OpenCompanionParentRequestCommand {
                child_runtime_session_id: "child-session".to_string(),
                turn_id: "turn-child-1".to_string(),
                wait: false,
                payload: serde_json::json!({ "message": "please review latest frame" }),
            })
            .await
            .expect("open parent request");

        assert_eq!(result.parent_frame_id, parent_current_frame.id);
        assert_ne!(
            result.parent_frame_id, parent_launch_frame.id,
            "parent request gate must bind to parent AgentRun current frame"
        );
        assert_eq!(result.parent_runtime_thread_id, "parent-current-session");

        let stored = gate_repo
            .get(result.gate_id)
            .await
            .expect("load gate")
            .expect("gate exists");
        assert_eq!(stored.agent_id, Some(parent_agent_id));
        assert_eq!(stored.frame_id, Some(parent_current_frame.id));
        assert_eq!(
            stored
                .payload_json
                .as_ref()
                .and_then(|payload| payload.get("parent_frame_id"))
                .and_then(serde_json::Value::as_str),
            Some(parent_current_frame.id.to_string().as_str())
        );

        assert!(delivery.event_notifications.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn resolve_parent_request_resolves_only_parent_owned_gate() {
        let run_id = Uuid::new_v4();
        let parent_agent_id = Uuid::new_v4();
        let child_agent_id = Uuid::new_v4();
        let parent_frame = AgentFrame::new_revision(parent_agent_id, 1, "parent");
        let parent_frame_id = parent_frame.id;
        let child_frame = AgentFrame::new_revision(child_agent_id, 1, "child");
        let child_frame_id = child_frame.id;
        let mut gate = LifecycleGate::open(
            run_id,
            Some(parent_agent_id),
            Some(parent_frame_id),
            COMPANION_PARENT_REQUEST_GATE_KIND,
            "pending-parent-request",
            Some(serde_json::json!({
                "request_type": "review",
                "summary": "please review",
                "child_agent_id": child_agent_id.to_string(),
                "child_frame_id": child_frame_id.to_string(),
                "companion_session_id": "child-session"
            })),
        );
        gate.correlation_id = gate.id.to_string();
        let gate_id = gate.id;

        let gate_repo = Arc::new(FixtureGateRepo::default());
        gate_repo.create(&gate).await.expect("seed gate");
        let frame_repo = Arc::new(FixtureFrameRepo::default());
        frame_repo
            .create(&parent_frame)
            .await
            .expect("seed parent frame");
        frame_repo.seed_runtime_sessions(parent_frame_id, ["parent-session"]);
        frame_repo
            .create(&child_frame)
            .await
            .expect("seed child frame");
        frame_repo.seed_runtime_sessions(child_frame_id, ["child-session"]);
        let lineage_repo = Arc::new(FixtureLineageRepo::default());
        let delivery = Arc::new(CapturingDelivery::default());
        let parent_mailbox_delivery = Arc::new(CapturingParentMailboxDelivery::default());
        let service = service_for_test_with_parent_mailbox(
            gate_repo.clone(),
            frame_repo,
            lineage_repo,
            delivery.clone(),
            parent_mailbox_delivery.clone(),
            run_id,
        );

        let result = service
            .resolve_parent_request(ResolveCompanionParentRequestCommand {
                request_id: gate_id.to_string(),
                parent_runtime_session_id: "parent-session".to_string(),
                resolved_turn_id: "turn-parent-1".to_string(),
                payload: serde_json::json!({
                    "status": "approved",
                    "summary": "looks good"
                }),
            })
            .await
            .expect("resolve parent request")
            .expect("matched parent request gate");

        assert_eq!(result.gate_id, gate_id);
        assert_eq!(result.parent_agent_id, parent_agent_id);
        assert_eq!(result.parent_frame_id, parent_frame_id);
        assert_eq!(result.parent_runtime_thread_id, "parent-session");
        assert_eq!(result.child_agent_id, child_agent_id);
        assert_eq!(result.child_frame_id, child_frame_id);
        assert_eq!(result.child_runtime_thread_id, "child-session");
        assert_eq!(
            result
                .child_mailbox_delivery
                .command_receipt_client_command_id,
            "companion-parent-response:test"
        );

        let stored = gate_repo
            .get(gate_id)
            .await
            .expect("load gate")
            .expect("gate exists");
        assert!(!stored.is_open());
        let expected_resolved_by = format!("parent_agent:{parent_agent_id}");
        assert_eq!(
            stored.resolved_by.as_deref(),
            Some(expected_resolved_by.as_str())
        );
        assert_eq!(
            stored
                .payload_json
                .as_ref()
                .and_then(|payload| payload.get("request_id"))
                .and_then(serde_json::Value::as_str),
            Some(gate_id.to_string().as_str())
        );
        assert!(
            stored
                .payload_json
                .as_ref()
                .and_then(|payload| payload.get("child_mailbox_delivery"))
                .is_none()
        );
        assert_eq!(
            stored
                .payload_json
                .as_ref()
                .and_then(|payload| payload.get("child_runtime_thread_id"))
                .and_then(serde_json::Value::as_str),
            Some("child-session")
        );

        {
            let parent_response_commands = parent_mailbox_delivery
                .parent_response_commands
                .lock()
                .unwrap();
            assert_eq!(parent_response_commands.len(), 1);
            assert_eq!(parent_response_commands[0].gate_id, gate_id);
            assert_eq!(parent_response_commands[0].run_id, run_id);
            assert_eq!(parent_response_commands[0].parent_agent_id, parent_agent_id);
            assert_eq!(parent_response_commands[0].child_agent_id, child_agent_id);
            assert_eq!(
                parent_response_commands[0].child_runtime_thread_id,
                "child-session"
            );
        }

        assert!(delivery.event_notifications.lock().unwrap().is_empty());

        let duplicate_error = service
            .resolve_parent_request(ResolveCompanionParentRequestCommand {
                request_id: gate_id.to_string(),
                parent_runtime_session_id: "parent-session".to_string(),
                resolved_turn_id: "turn-parent-2".to_string(),
                payload: serde_json::json!({
                    "status": "approved",
                    "summary": "duplicate"
                }),
            })
            .await
            .expect_err("closed gate should reject duplicate response");
        assert!(matches!(duplicate_error, ApplicationError::Conflict(_)));
        assert_eq!(
            parent_mailbox_delivery
                .parent_response_commands
                .lock()
                .unwrap()
                .len(),
            1
        );
    }

    #[tokio::test]
    async fn resolve_parent_request_keeps_child_mailbox_failure_out_of_gate_payload() {
        let run_id = Uuid::new_v4();
        let parent_agent_id = Uuid::new_v4();
        let child_agent_id = Uuid::new_v4();
        let parent_frame = AgentFrame::new_revision(parent_agent_id, 1, "parent");
        let child_frame = AgentFrame::new_revision(child_agent_id, 1, "child");
        let mut gate = LifecycleGate::open(
            run_id,
            Some(parent_agent_id),
            Some(parent_frame.id),
            COMPANION_PARENT_REQUEST_GATE_KIND,
            "pending-parent-request",
            Some(serde_json::json!({
                "request_type": "review",
                "summary": "please review",
                "child_agent_id": child_agent_id.to_string(),
                "child_frame_id": child_frame.id.to_string(),
                "companion_session_id": "child-session"
            })),
        );
        gate.correlation_id = gate.id.to_string();
        let gate_id = gate.id;

        let gate_repo = Arc::new(FixtureGateRepo::default());
        gate_repo.create(&gate).await.expect("seed gate");
        let frame_repo = Arc::new(FixtureFrameRepo::default());
        frame_repo
            .create(&parent_frame)
            .await
            .expect("seed parent frame");
        frame_repo.seed_runtime_sessions(parent_frame.id, ["parent-session"]);
        frame_repo
            .create(&child_frame)
            .await
            .expect("seed child frame");
        frame_repo.seed_runtime_sessions(child_frame.id, ["child-session"]);
        let lineage_repo = Arc::new(FixtureLineageRepo::default());
        let delivery = Arc::new(CapturingDelivery::default());
        let parent_mailbox_delivery = Arc::new(CapturingParentMailboxDelivery::default());
        parent_mailbox_delivery.fail_next("child mailbox unavailable");
        let service = service_for_test_with_parent_mailbox(
            gate_repo.clone(),
            frame_repo,
            lineage_repo,
            delivery.clone(),
            parent_mailbox_delivery.clone(),
            run_id,
        );

        let error = service
            .resolve_parent_request(ResolveCompanionParentRequestCommand {
                request_id: gate_id.to_string(),
                parent_runtime_session_id: "parent-session".to_string(),
                resolved_turn_id: "turn-parent-1".to_string(),
                payload: serde_json::json!({
                    "status": "approved",
                    "summary": "looks good"
                }),
            })
            .await
            .expect_err("mailbox failure should fail command");

        assert!(matches!(error, ApplicationError::Internal(_)));
        assert!(delivery.event_notifications.lock().unwrap().is_empty());
        let stored = gate_repo
            .get(gate_id)
            .await
            .expect("load gate")
            .expect("gate exists");
        assert!(!stored.is_open());
        assert!(
            stored
                .payload_json
                .as_ref()
                .and_then(|payload| payload.get("child_mailbox_delivery"))
                .is_none()
        );
        assert_eq!(
            parent_mailbox_delivery
                .parent_response_commands
                .lock()
                .unwrap()
                .len(),
            1
        );
    }

    #[tokio::test]
    async fn resolve_parent_request_rejects_delivery_session_for_another_frame() {
        let run_id = Uuid::new_v4();
        let parent_agent_id = Uuid::new_v4();
        let other_agent_id = Uuid::new_v4();
        let parent_frame = AgentFrame::new_revision(parent_agent_id, 1, "parent");
        let parent_frame_id = parent_frame.id;
        let other_frame = AgentFrame::new_revision(other_agent_id, 1, "other");
        let mut gate = LifecycleGate::open(
            run_id,
            Some(parent_agent_id),
            Some(parent_frame_id),
            COMPANION_PARENT_REQUEST_GATE_KIND,
            "pending-parent-request",
            None,
        );
        gate.correlation_id = gate.id.to_string();

        let gate_repo = Arc::new(FixtureGateRepo::default());
        gate_repo.create(&gate).await.expect("seed gate");
        let frame_repo = Arc::new(FixtureFrameRepo::default());
        frame_repo
            .create(&parent_frame)
            .await
            .expect("seed parent frame");
        frame_repo.seed_runtime_sessions(parent_frame_id, ["parent-session"]);
        frame_repo
            .create(&other_frame)
            .await
            .expect("seed other frame");
        frame_repo.seed_runtime_sessions(other_frame.id, ["other"]);
        let lineage_repo = Arc::new(FixtureLineageRepo::default());
        let delivery = Arc::new(CapturingDelivery::default());
        let service = service_for_test(
            gate_repo,
            frame_repo,
            lineage_repo,
            delivery.clone(),
            run_id,
        );

        let error = service
            .resolve_parent_request(ResolveCompanionParentRequestCommand {
                request_id: gate.id.to_string(),
                parent_runtime_session_id: "other".to_string(),
                resolved_turn_id: "turn-parent-1".to_string(),
                payload: serde_json::json!({ "status": "approved" }),
            })
            .await
            .expect_err("wrong frame should be rejected");

        assert!(matches!(error, ApplicationError::Conflict(_)));
        assert!(delivery.event_notifications.lock().unwrap().is_empty());
    }
}
