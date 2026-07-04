use agentdash_diagnostics::{DiagnosticErrorContext, Subsystem, diag_error};
use std::sync::Arc;

use agentdash_application_workflow::gate::{
    CompleteChildResultGateCommand, GateDeliveryIntent, GateNotificationIntent,
    LifecycleGateResolver, OpenParentRequestGateCommand, ResolveParentRequestGateCommand,
    RespondHumanGateCommand,
};
use agentdash_domain::workflow::{
    AgentFrameRepository, AgentLineageRepository, AgentRunDeliveryBindingRepository,
    LifecycleAgentRepository, LifecycleGateRepository, LifecycleRunRepository,
    RuntimeSessionExecutionAnchorRepository,
};
use async_trait::async_trait;
use uuid::Uuid;

use super::{
    PayloadTypeRegistry, build_companion_event_notification,
    build_companion_human_response_notification, payload_types,
};
use crate::agent_run::{
    DeliveryRuntimeSelection, DeliveryRuntimeSelectionError, DeliveryRuntimeSelectionRepositories,
    DeliveryRuntimeSelectionService,
};
use crate::lifecycle::resolve_current_frame_from_delivery_trace_ref;
use crate::{ApplicationError, session::SessionEventingService};

const COMPANION_PARENT_REQUEST_GATE_KIND: &str = "companion_parent_request";

#[derive(Debug, Clone)]
pub struct RespondCompanionGateCommand {
    pub gate_id: Uuid,
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone)]
pub struct CompanionGateRespondResult {
    pub gate_id: Uuid,
    pub request_id: String,
    pub delivery_runtime_session_id: Option<String>,
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
    pub parent_delivery_runtime_session_id: String,
    pub child_agent_id: Uuid,
    pub child_frame_id: Uuid,
    pub child_delivery_runtime_session_id: String,
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
    pub parent_delivery_runtime_session_id: String,
    pub child_agent_id: Uuid,
    pub child_frame_id: Uuid,
    pub child_delivery_runtime_session_id: String,
    pub child_mailbox_delivery: CompanionParentMailboxDeliveryResult,
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone)]
pub struct CompanionChildResultCompleteResult {
    pub gate_id: Uuid,
    pub parent_agent_id: Uuid,
    pub parent_delivery_runtime_session_id: Option<String>,
    pub child_delivery_runtime_session_id: Option<String>,
    pub parent_mailbox_delivery: CompanionParentMailboxDeliveryResult,
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone)]
pub struct CompanionParentMailboxDeliveryCommand {
    pub gate_id: Uuid,
    pub request_id: String,
    pub run_id: Uuid,
    pub parent_agent_id: Uuid,
    pub parent_delivery_runtime_session_id: String,
    pub child_agent_id: Uuid,
    pub child_delivery_runtime_session_id: Option<String>,
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
    pub parent_delivery_runtime_session_id: String,
    pub child_agent_id: Uuid,
    pub child_delivery_runtime_session_id: String,
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
    pub parent_delivery_runtime_session_id: String,
    pub child_agent_id: Uuid,
    pub child_delivery_runtime_session_id: String,
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
    pub delivery_runtime_session_id: String,
    pub turn_id: Option<String>,
    pub request_type: Option<String>,
    pub payload: serde_json::Value,
    pub input_text: String,
}

#[derive(Debug, Clone)]
pub struct CompanionParentMailboxDeliveryResult {
    pub mailbox_message_id: Option<Uuid>,
    pub command_receipt_id: Option<Uuid>,
    pub command_receipt_client_command_id: String,
    pub command_receipt_status: String,
    pub command_receipt_duplicate: bool,
    pub outcome: String,
    pub accepted_agent_run_turn_id: Option<String>,
    pub accepted_protocol_turn_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct CompanionGateResponseNotification {
    pub delivery_runtime_session_id: String,
    pub turn_id: Option<String>,
    pub request_id: String,
    pub payload: serde_json::Value,
    pub request_type: Option<String>,
    pub gate_resolved: bool,
}

#[derive(Debug, Clone)]
pub struct CompanionGateEventNotification {
    pub delivery_runtime_session_id: String,
    pub turn_id: String,
    pub event_type: String,
    pub message: String,
    pub payload: serde_json::Value,
}

#[async_trait]
pub trait CompanionGateNotificationDelivery: Send + Sync {
    async fn deliver_human_response(
        &self,
        notification: CompanionGateResponseNotification,
    ) -> Result<(), ApplicationError>;

    async fn deliver_companion_event(
        &self,
        notification: CompanionGateEventNotification,
    ) -> Result<(), ApplicationError>;
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

#[derive(Clone)]
pub struct SessionEventingCompanionGateDelivery {
    eventing: SessionEventingService,
}

#[cfg(test)]
#[derive(Clone, Default)]
pub struct NoopCompanionGateDelivery;

#[derive(Clone, Default)]
pub struct NoopCompanionParentMailboxDelivery;

#[derive(Clone, Default)]
pub struct NoopCompanionHumanResponseMailboxDelivery;

impl SessionEventingCompanionGateDelivery {
    pub fn new(eventing: SessionEventingService) -> Self {
        Self { eventing }
    }
}

#[cfg(test)]
#[async_trait]
impl CompanionGateNotificationDelivery for NoopCompanionGateDelivery {
    async fn deliver_human_response(
        &self,
        _notification: CompanionGateResponseNotification,
    ) -> Result<(), ApplicationError> {
        Ok(())
    }

    async fn deliver_companion_event(
        &self,
        _notification: CompanionGateEventNotification,
    ) -> Result<(), ApplicationError> {
        Ok(())
    }
}

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

#[async_trait]
impl CompanionGateNotificationDelivery for SessionEventingCompanionGateDelivery {
    async fn deliver_human_response(
        &self,
        notification: CompanionGateResponseNotification,
    ) -> Result<(), ApplicationError> {
        let envelope = build_companion_human_response_notification(
            &notification.delivery_runtime_session_id,
            notification.turn_id.as_deref(),
            &notification.request_id,
            &notification.payload,
            notification.request_type.as_deref(),
            notification.gate_resolved,
        );
        self.eventing
            .inject_notification(&notification.delivery_runtime_session_id, envelope)
            .await
            .map_err(ApplicationError::from)
    }

    async fn deliver_companion_event(
        &self,
        notification: CompanionGateEventNotification,
    ) -> Result<(), ApplicationError> {
        let envelope = build_companion_event_notification(
            &notification.delivery_runtime_session_id,
            &notification.turn_id,
            &notification.event_type,
            notification.message,
            notification.payload,
        );
        self.eventing
            .inject_notification(&notification.delivery_runtime_session_id, envelope)
            .await
            .map_err(ApplicationError::from)
    }
}

pub struct CompanionGateControlService {
    gate_repo: Arc<dyn LifecycleGateRepository>,
    gate_resolver: LifecycleGateResolver,
    run_repo: Arc<dyn LifecycleRunRepository>,
    frame_repo: Arc<dyn AgentFrameRepository>,
    agent_repo: Arc<dyn LifecycleAgentRepository>,
    anchor_repo: Arc<dyn RuntimeSessionExecutionAnchorRepository>,
    delivery_binding_repo: Arc<dyn AgentRunDeliveryBindingRepository>,
    lineage_repo: Arc<dyn AgentLineageRepository>,
    delivery: Arc<dyn CompanionGateNotificationDelivery>,
    parent_mailbox_delivery: Arc<dyn CompanionParentMailboxDelivery>,
    human_response_mailbox_delivery: Arc<dyn CompanionHumanResponseMailboxDelivery>,
}

#[derive(Clone)]
pub struct CompanionGateControlRepos {
    pub gate_repo: Arc<dyn LifecycleGateRepository>,
    pub run_repo: Arc<dyn LifecycleRunRepository>,
    pub frame_repo: Arc<dyn AgentFrameRepository>,
    pub agent_repo: Arc<dyn LifecycleAgentRepository>,
    pub anchor_repo: Arc<dyn RuntimeSessionExecutionAnchorRepository>,
    pub delivery_binding_repo: Arc<dyn AgentRunDeliveryBindingRepository>,
    pub lineage_repo: Arc<dyn AgentLineageRepository>,
}

pub struct CompanionGateControlDeps {
    pub repos: CompanionGateControlRepos,
    pub delivery: Arc<dyn CompanionGateNotificationDelivery>,
}

impl CompanionGateControlService {
    pub fn new(deps: CompanionGateControlDeps) -> Self {
        let CompanionGateControlDeps { repos, delivery } = deps;
        Self {
            gate_resolver: LifecycleGateResolver::new(repos.gate_repo.clone()),
            gate_repo: repos.gate_repo,
            run_repo: repos.run_repo,
            frame_repo: repos.frame_repo,
            agent_repo: repos.agent_repo,
            anchor_repo: repos.anchor_repo,
            delivery_binding_repo: repos.delivery_binding_repo,
            lineage_repo: repos.lineage_repo,
            delivery,
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

    pub fn with_session_eventing(
        repos: CompanionGateControlRepos,
        eventing: SessionEventingService,
    ) -> Self {
        Self::new(CompanionGateControlDeps {
            repos,
            delivery: Arc::new(SessionEventingCompanionGateDelivery::new(eventing)),
        })
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

        let delivery_runtime_session_id = self.resolve_delivery_runtime_session_id(&gate).await?;
        let request_id = gate.id.to_string();

        let Some(delivery_runtime_session_id) = delivery_runtime_session_id.clone() else {
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
                    delivery_runtime_session_id: delivery_runtime_session_id.clone(),
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
            delivery_runtime_session_id: Some(delivery_runtime_session_id),
            gate_resolved: true,
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
            self.anchor_repo.as_ref(),
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
            .list_open_for_agent(child_frame.agent_id)
            .await?
            .into_iter()
            .find(|gate| gate.correlation_id == command.request_id)
        {
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
        let parent_delivery_runtime_session_id = self
            .select_bound_delivery_runtime_session_id(lineage.run_id, parent_agent_id)
            .await?;
        let child_delivery_runtime_session_id = self
            .validate_bound_delivery_runtime_session_id(
                lineage.run_id,
                child_frame.agent_id,
                &child_runtime_session_id,
            )
            .await?;

        let Some(parent_delivery_runtime_session_id) = parent_delivery_runtime_session_id.clone()
        else {
            let error =
                "parent agent 缺少 current delivery runtime session，无法投递 companion result"
                    .to_string();
            return Err(ApplicationError::Conflict(error));
        };

        let outcome = self
            .gate_resolver
            .complete_child_result(CompleteChildResultGateCommand {
                gate_id: gate.id,
                request_id: command.request_id.clone(),
                run_id: lineage.run_id,
                parent_agent_id,
                parent_delivery_runtime_session_id: parent_delivery_runtime_session_id.clone(),
                child_agent_id: child_frame.agent_id,
                child_delivery_runtime_session_id: child_delivery_runtime_session_id.clone(),
                resolved_turn_id: resolved_turn_id.clone(),
                companion_label: companion_label.to_string(),
                payload: command.payload.clone(),
                resolved_by: format!("child_agent:{}", child_frame.agent_id),
            })
            .await?;
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
        let resolution_payload = result_intent.payload.clone();
        let summary = resolution_payload
            .get("summary")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("")
            .trim();
        let status = resolution_payload
            .get("status")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("completed");
        let input_text = build_parent_result_mailbox_input_text(
            result_intent.gate_id,
            &result_intent.request_id,
            companion_label,
            status,
            summary,
            &result_intent.payload,
        );
        let mailbox_result = self
            .parent_mailbox_delivery
            .deliver_child_result_to_parent(CompanionParentMailboxDeliveryCommand {
                gate_id: result_intent.gate_id,
                request_id: result_intent.request_id.clone(),
                run_id: result_intent.run_id,
                parent_agent_id: result_intent.parent_agent_id,
                parent_delivery_runtime_session_id: result_intent
                    .parent_delivery_runtime_session_id
                    .clone(),
                child_agent_id: result_intent.child_agent_id,
                child_delivery_runtime_session_id: result_intent
                    .child_delivery_runtime_session_id
                    .clone(),
                resolved_turn_id: result_intent.resolved_turn_id.clone(),
                payload: result_intent.payload.clone(),
                input_text,
            })
            .await?;

        self.deliver_notification_intents(&outcome.notification_intents, gate.id, None)
            .await;

        Ok(Some(CompanionChildResultCompleteResult {
            gate_id: gate.id,
            parent_agent_id,
            parent_delivery_runtime_session_id: Some(parent_delivery_runtime_session_id),
            child_delivery_runtime_session_id,
            parent_mailbox_delivery: mailbox_result,
            payload: resolution_payload,
        }))
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
            self.anchor_repo.as_ref(),
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
        let child_delivery_runtime_session_id = self
            .validate_bound_delivery_runtime_session_id(
                child_anchor.run_id,
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
        let parent_delivery_runtime_session_id = parent_selection.runtime_session_id;

        let companion_label = format!("child:{}", child_frame.agent_id);
        let outcome = self
            .gate_resolver
            .open_parent_request(OpenParentRequestGateCommand {
                run_id: lineage.run_id,
                parent_agent_id,
                parent_frame_id,
                parent_delivery_runtime_session_id: parent_delivery_runtime_session_id.clone(),
                child_agent_id: child_frame.agent_id,
                child_frame_id: child_frame.id,
                child_delivery_runtime_session_id: child_delivery_runtime_session_id.clone(),
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
                parent_delivery_runtime_session_id: parent_request_intent
                    .parent_delivery_runtime_session_id
                    .clone(),
                child_agent_id: parent_request_intent.child_agent_id,
                child_delivery_runtime_session_id: parent_request_intent
                    .child_delivery_runtime_session_id
                    .clone(),
                turn_id: parent_request_intent.turn_id.clone(),
                wait: parent_request_intent.wait,
                payload: parent_request_intent.payload.clone(),
                input_text,
            })
            .await?;

        self.deliver_notification_intents(&outcome.notification_intents, gate.id, None)
            .await;

        Ok(CompanionParentRequestOpenResult {
            gate_id: gate.id,
            request_id,
            run_id: gate.run_id,
            parent_agent_id,
            parent_frame_id,
            parent_delivery_runtime_session_id,
            child_agent_id: child_frame.agent_id,
            child_frame_id: child_frame.id,
            child_delivery_runtime_session_id,
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
            self.anchor_repo.as_ref(),
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
        let parent_delivery_runtime_session_id = self
            .validate_bound_delivery_runtime_session_id(
                parent_anchor.run_id,
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
        let child_delivery_runtime_session_id = request_payload
            .get("companion_session_id")
            .or_else(|| request_payload.get("child_delivery_runtime_session_id"))
            .and_then(serde_json::Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| {
                ApplicationError::Conflict(format!(
                    "parent request gate {} 缺少 child delivery runtime session",
                    gate.id
                ))
            })?;
        let child_delivery_runtime_session_id = self
            .validate_bound_delivery_runtime_session_id(
                parent_anchor.run_id,
                child_agent_id,
                &child_delivery_runtime_session_id,
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
                run_id: parent_anchor.run_id,
                parent_agent_id: parent_frame.agent_id,
                parent_frame_id: parent_frame.id,
                parent_delivery_runtime_session_id: parent_delivery_runtime_session_id.clone(),
                child_agent_id,
                child_frame_id,
                child_delivery_runtime_session_id: child_delivery_runtime_session_id.clone(),
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
                parent_delivery_runtime_session_id: parent_response_intent
                    .parent_delivery_runtime_session_id
                    .clone(),
                child_agent_id: parent_response_intent.child_agent_id,
                child_delivery_runtime_session_id: parent_response_intent
                    .child_delivery_runtime_session_id
                    .clone(),
                resolved_turn_id: parent_response_intent.resolved_turn_id.clone(),
                payload: parent_response_intent.payload.clone(),
                input_text,
            })
            .await?;

        self.deliver_notification_intents(&outcome.notification_intents, gate.id, None)
            .await;

        Ok(Some(CompanionParentRequestResolveResult {
            gate_id: gate.id,
            parent_agent_id: parent_frame.agent_id,
            parent_frame_id: parent_frame.id,
            parent_delivery_runtime_session_id,
            child_agent_id,
            child_frame_id,
            child_delivery_runtime_session_id,
            child_mailbox_delivery: mailbox_result,
            payload: resolution_payload,
        }))
    }

    async fn resolve_delivery_runtime_session_id(
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

        self.select_bound_delivery_runtime_session_id(gate.run_id, frame.agent_id)
            .await
    }

    async fn select_bound_delivery_runtime_session_id(
        &self,
        run_id: Uuid,
        agent_id: Uuid,
    ) -> Result<Option<String>, ApplicationError> {
        Ok(self
            .select_current_delivery(run_id, agent_id)
            .await?
            .map(|selection| selection.runtime_session_id))
    }

    async fn validate_bound_delivery_runtime_session_id(
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
    ) -> Result<Option<DeliveryRuntimeSelection>, ApplicationError> {
        match DeliveryRuntimeSelectionService::new(DeliveryRuntimeSelectionRepositories {
            lifecycle_runs: self.run_repo.as_ref(),
            lifecycle_agents: self.agent_repo.as_ref(),
            agent_frames: self.frame_repo.as_ref(),
            execution_anchors: self.anchor_repo.as_ref(),
            delivery_bindings: self.delivery_binding_repo.as_ref(),
        })
        .select_current_delivery(run_id, agent_id)
        .await
        {
            Ok(selection) => Ok(Some(selection)),
            Err(DeliveryRuntimeSelectionError::CurrentDeliveryMissing { .. }) => Ok(None),
            Err(error) => Err(application_error_from_selection_error(error)),
        }
    }

    async fn deliver_notification_intents(
        &self,
        intents: &[GateNotificationIntent],
        gate_id: Uuid,
        diagnostic_agent_id: Option<Uuid>,
    ) {
        for intent in intents {
            let event = match intent {
                GateNotificationIntent::CompanionReviewRequest(event)
                | GateNotificationIntent::CompanionParentRequestResolved(event)
                | GateNotificationIntent::CompanionResultAvailable(event)
                | GateNotificationIntent::CompanionResultReturned(event) => event,
            };
            let notification = CompanionGateEventNotification {
                delivery_runtime_session_id: event.delivery_runtime_session_id.clone(),
                turn_id: event.turn_id.clone(),
                event_type: event.event_type.clone(),
                message: event.message.clone(),
                payload: event.payload.clone(),
            };
            if let Err(error) = self.delivery.deliver_companion_event(notification).await {
                let mut context = DiagnosticErrorContext::new(
                    "companion.gate_notification",
                    "deliver_companion_event",
                )
                .with_field("gate_id", gate_id);
                if let Some(agent_id) = diagnostic_agent_id {
                    context = context.with_field("agent_id", agent_id);
                }
                diag_error!(
                    Warn,
                    Subsystem::AgentRun,
                    context = &context,
                    error = &error,
                    gate_id = %gate_id,
                    agent_id = ?diagnostic_agent_id,
                    "companion gate transition notification delivery failed"
                );
            }
        }
    }
}

fn application_error_from_selection_error(
    error: DeliveryRuntimeSelectionError,
) -> ApplicationError {
    match error {
        DeliveryRuntimeSelectionError::RunNotFound { .. }
        | DeliveryRuntimeSelectionError::AgentNotFound { .. }
        | DeliveryRuntimeSelectionError::CurrentFrameNotFound { .. }
        | DeliveryRuntimeSelectionError::LaunchFrameNotFound { .. }
        | DeliveryRuntimeSelectionError::SubjectNotFound { .. } => {
            ApplicationError::NotFound(error.to_string())
        }
        DeliveryRuntimeSelectionError::Repository(source) => ApplicationError::from(source),
        other => ApplicationError::Conflict(other.to_string()),
    }
}

fn build_parent_result_mailbox_input_text(
    gate_id: Uuid,
    request_id: &str,
    companion_label: &str,
    status: &str,
    summary: &str,
    payload: &serde_json::Value,
) -> String {
    let mut lines = vec![
        "Companion child result is available.".to_string(),
        format!("- request_id: {request_id}"),
        format!("- gate_id: {gate_id}"),
        format!("- companion_label: {companion_label}"),
        format!("- status: {status}"),
        format!("- summary: {summary}"),
    ];
    if let Some(findings) = payload
        .get("findings")
        .and_then(serde_json::Value::as_array)
    {
        let rendered = findings
            .iter()
            .filter_map(serde_json::Value::as_str)
            .map(|finding| format!("  - {finding}"))
            .collect::<Vec<_>>();
        if !rendered.is_empty() {
            lines.push("- findings:".to_string());
            lines.extend(rendered);
        }
    }
    if let Some(follow_ups) = payload
        .get("follow_ups")
        .and_then(serde_json::Value::as_array)
    {
        let rendered = follow_ups
            .iter()
            .filter_map(serde_json::Value::as_str)
            .map(|follow_up| format!("  - {follow_up}"))
            .collect::<Vec<_>>();
        if !rendered.is_empty() {
            lines.push("- follow_ups:".to_string());
            lines.extend(rendered);
        }
    }
    lines.join("\n")
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
        collections::HashMap,
        sync::{Arc, Mutex},
    };

    use agentdash_domain::{
        DomainError,
        workflow::{
            AgentFrame, AgentLineage, AgentRunDeliveryBinding, AgentRunDeliveryBindingRepository,
            AgentSource, DeliveryBindingStatus, LifecycleAgent, LifecycleGate, LifecycleRun,
            LifecycleRunRepository, RuntimeSessionExecutionAnchor,
        },
    };

    use super::*;

    #[derive(Default)]
    struct MemoryGateRepo {
        gates: Mutex<HashMap<Uuid, LifecycleGate>>,
    }

    #[async_trait]
    impl LifecycleGateRepository for MemoryGateRepo {
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

        async fn update(&self, gate: &LifecycleGate) -> Result<(), DomainError> {
            self.gates.lock().unwrap().insert(gate.id, gate.clone());
            Ok(())
        }
    }

    #[derive(Default)]
    struct MemoryFrameRepo {
        frames: Mutex<HashMap<Uuid, AgentFrame>>,
        runtime_sessions_by_frame: Mutex<HashMap<Uuid, Vec<String>>>,
    }

    impl MemoryFrameRepo {
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
    impl AgentFrameRepository for MemoryFrameRepo {
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
    struct MemoryLineageRepo {
        lineages: Mutex<Vec<AgentLineage>>,
    }

    #[async_trait]
    impl AgentLineageRepository for MemoryLineageRepo {
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
    struct MemoryAgentRepo {
        agents: Mutex<HashMap<Uuid, LifecycleAgent>>,
    }

    impl MemoryAgentRepo {
        fn from_frame_repo(frame_repo: &MemoryFrameRepo, run_id: Uuid, project_id: Uuid) -> Self {
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
    impl LifecycleAgentRepository for MemoryAgentRepo {
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
    struct MemoryAnchorRepo {
        anchors: Mutex<HashMap<String, RuntimeSessionExecutionAnchor>>,
    }

    impl MemoryAnchorRepo {
        fn from_frame_repo(frame_repo: &MemoryFrameRepo, run_id: Uuid) -> Self {
            let mut anchors = HashMap::new();
            let sessions_by_frame = frame_repo.runtime_sessions_by_frame.lock().unwrap();
            for frame in frame_repo.frames.lock().unwrap().values() {
                if let Some(session_ids) = sessions_by_frame.get(&frame.id) {
                    for runtime_session_id in session_ids {
                        let anchor = RuntimeSessionExecutionAnchor::new_dispatch(
                            runtime_session_id.clone(),
                            run_id,
                            frame.id,
                            frame.agent_id,
                        );
                        anchors.insert(runtime_session_id.clone(), anchor);
                    }
                }
            }
            Self {
                anchors: Mutex::new(anchors),
            }
        }
    }

    #[async_trait]
    impl RuntimeSessionExecutionAnchorRepository for MemoryAnchorRepo {
        async fn create_once(
            &self,
            anchor: &RuntimeSessionExecutionAnchor,
        ) -> Result<(), DomainError> {
            let mut anchors = self.anchors.lock().unwrap();
            if let Some(existing) = anchors.get(&anchor.runtime_session_id) {
                if existing.has_same_launch_coordinates_as(anchor) {
                    return Ok(());
                }
                return Err(existing.immutable_conflict(anchor));
            }
            anchors.insert(anchor.runtime_session_id.clone(), anchor.clone());
            Ok(())
        }

        async fn delete_by_session(&self, runtime_session_id: &str) -> Result<(), DomainError> {
            self.anchors.lock().unwrap().remove(runtime_session_id);
            Ok(())
        }

        async fn find_by_session(
            &self,
            runtime_session_id: &str,
        ) -> Result<Option<RuntimeSessionExecutionAnchor>, DomainError> {
            Ok(self
                .anchors
                .lock()
                .unwrap()
                .get(runtime_session_id)
                .cloned())
        }

        async fn list_by_run(
            &self,
            run_id: Uuid,
        ) -> Result<Vec<RuntimeSessionExecutionAnchor>, DomainError> {
            Ok(self
                .anchors
                .lock()
                .unwrap()
                .values()
                .filter(|anchor| anchor.run_id == run_id)
                .cloned()
                .collect())
        }

        async fn list_by_agent(
            &self,
            agent_id: Uuid,
        ) -> Result<Vec<RuntimeSessionExecutionAnchor>, DomainError> {
            Ok(self
                .anchors
                .lock()
                .unwrap()
                .values()
                .filter(|anchor| anchor.agent_id == agent_id)
                .cloned()
                .collect())
        }

        async fn list_by_project_session_ids(
            &self,
            runtime_session_ids: &[String],
        ) -> Result<Vec<RuntimeSessionExecutionAnchor>, DomainError> {
            let anchors = self.anchors.lock().unwrap();
            Ok(runtime_session_ids
                .iter()
                .filter_map(|id| anchors.get(id).cloned())
                .collect())
        }
    }

    #[derive(Default)]
    struct MemoryDeliveryBindingRepo {
        bindings: Mutex<HashMap<(Uuid, Uuid), AgentRunDeliveryBinding>>,
    }

    impl MemoryDeliveryBindingRepo {
        fn from_frame_repo(frame_repo: &MemoryFrameRepo, run_id: Uuid) -> Self {
            let frames: Vec<_> = frame_repo
                .frames
                .lock()
                .unwrap()
                .values()
                .cloned()
                .collect();
            let sessions_by_frame = frame_repo.runtime_sessions_by_frame.lock().unwrap();
            let mut latest_frames: HashMap<Uuid, AgentFrame> = HashMap::new();
            for frame in frames {
                let should_replace = latest_frames
                    .get(&frame.agent_id)
                    .is_none_or(|current| frame.revision > current.revision);
                if should_replace {
                    latest_frames.insert(frame.agent_id, frame);
                }
            }
            let mut bindings = HashMap::new();
            for frame in latest_frames.values() {
                let Some(runtime_session_id) = sessions_by_frame
                    .get(&frame.id)
                    .and_then(|session_ids| session_ids.last())
                else {
                    continue;
                };
                let anchor = RuntimeSessionExecutionAnchor::new_dispatch(
                    runtime_session_id.clone(),
                    run_id,
                    frame.id,
                    frame.agent_id,
                );
                let binding = AgentRunDeliveryBinding::from_anchor(
                    &anchor,
                    DeliveryBindingStatus::Running,
                    anchor.updated_at,
                );
                bindings.insert((binding.run_id, binding.agent_id), binding);
            }
            Self {
                bindings: Mutex::new(bindings),
            }
        }
    }

    #[async_trait]
    impl AgentRunDeliveryBindingRepository for MemoryDeliveryBindingRepo {
        async fn upsert(&self, binding: &AgentRunDeliveryBinding) -> Result<(), DomainError> {
            self.bindings
                .lock()
                .unwrap()
                .insert((binding.run_id, binding.agent_id), binding.clone());
            Ok(())
        }

        async fn get_current(
            &self,
            run_id: Uuid,
            agent_id: Uuid,
        ) -> Result<Option<AgentRunDeliveryBinding>, DomainError> {
            Ok(self
                .bindings
                .lock()
                .unwrap()
                .get(&(run_id, agent_id))
                .cloned())
        }

        async fn list_by_run(
            &self,
            run_id: Uuid,
        ) -> Result<Vec<AgentRunDeliveryBinding>, DomainError> {
            Ok(self
                .bindings
                .lock()
                .unwrap()
                .values()
                .filter(|binding| binding.run_id == run_id)
                .cloned()
                .collect())
        }

        async fn delete_by_session(&self, runtime_session_id: &str) -> Result<(), DomainError> {
            self.bindings
                .lock()
                .unwrap()
                .retain(|_, binding| binding.runtime_session_id != runtime_session_id);
            Ok(())
        }
    }

    #[derive(Default)]
    struct MemoryRunRepo {
        runs: Mutex<HashMap<Uuid, LifecycleRun>>,
    }

    impl MemoryRunRepo {
        fn with_run(run_id: Uuid, project_id: Uuid) -> Self {
            let mut run = LifecycleRun::new_plain(project_id);
            run.id = run_id;
            let mut runs = HashMap::new();
            runs.insert(run.id, run);
            Self {
                runs: Mutex::new(runs),
            }
        }
    }

    #[async_trait]
    impl LifecycleRunRepository for MemoryRunRepo {
        async fn create(&self, run: &LifecycleRun) -> Result<(), DomainError> {
            self.runs.lock().unwrap().insert(run.id, run.clone());
            Ok(())
        }

        async fn get_by_id(&self, id: Uuid) -> Result<Option<LifecycleRun>, DomainError> {
            Ok(self.runs.lock().unwrap().get(&id).cloned())
        }

        async fn list_by_ids(&self, ids: &[Uuid]) -> Result<Vec<LifecycleRun>, DomainError> {
            Ok(self
                .runs
                .lock()
                .unwrap()
                .values()
                .filter(|run| ids.contains(&run.id))
                .cloned()
                .collect())
        }

        async fn list_by_project(
            &self,
            project_id: Uuid,
        ) -> Result<Vec<LifecycleRun>, DomainError> {
            Ok(self
                .runs
                .lock()
                .unwrap()
                .values()
                .filter(|run| run.project_id == project_id)
                .cloned()
                .collect())
        }

        async fn update(&self, run: &LifecycleRun) -> Result<(), DomainError> {
            self.runs.lock().unwrap().insert(run.id, run.clone());
            Ok(())
        }

        async fn delete(&self, id: Uuid) -> Result<(), DomainError> {
            self.runs.lock().unwrap().remove(&id);
            Ok(())
        }
    }

    fn service_for_test(
        gate_repo: Arc<MemoryGateRepo>,
        frame_repo: Arc<MemoryFrameRepo>,
        lineage_repo: Arc<MemoryLineageRepo>,
        delivery: Arc<CapturingDelivery>,
        run_id: Uuid,
    ) -> CompanionGateControlService {
        service_for_test_with_parent_mailbox(
            gate_repo,
            frame_repo,
            lineage_repo,
            delivery,
            Arc::new(CapturingParentMailboxDelivery::default()),
            run_id,
        )
    }

    fn service_for_test_with_parent_mailbox(
        gate_repo: Arc<MemoryGateRepo>,
        frame_repo: Arc<MemoryFrameRepo>,
        lineage_repo: Arc<MemoryLineageRepo>,
        delivery: Arc<CapturingDelivery>,
        parent_mailbox_delivery: Arc<CapturingParentMailboxDelivery>,
        run_id: Uuid,
    ) -> CompanionGateControlService {
        service_for_test_with_mailboxes(
            gate_repo,
            frame_repo,
            lineage_repo,
            delivery,
            parent_mailbox_delivery,
            Arc::new(CapturingHumanMailboxDelivery::default()),
            run_id,
        )
    }

    fn service_for_test_with_mailboxes(
        gate_repo: Arc<MemoryGateRepo>,
        frame_repo: Arc<MemoryFrameRepo>,
        lineage_repo: Arc<MemoryLineageRepo>,
        delivery: Arc<CapturingDelivery>,
        parent_mailbox_delivery: Arc<CapturingParentMailboxDelivery>,
        human_mailbox_delivery: Arc<CapturingHumanMailboxDelivery>,
        run_id: Uuid,
    ) -> CompanionGateControlService {
        let project_id = Uuid::new_v4();
        let agent_repo = Arc::new(MemoryAgentRepo::from_frame_repo(
            frame_repo.as_ref(),
            run_id,
            project_id,
        ));
        let anchor_repo = Arc::new(MemoryAnchorRepo::from_frame_repo(
            frame_repo.as_ref(),
            run_id,
        ));
        let delivery_binding_repo = Arc::new(MemoryDeliveryBindingRepo::from_frame_repo(
            frame_repo.as_ref(),
            run_id,
        ));
        CompanionGateControlService::new(CompanionGateControlDeps {
            repos: CompanionGateControlRepos {
                gate_repo,
                run_repo: Arc::new(MemoryRunRepo::with_run(run_id, project_id)),
                frame_repo,
                agent_repo,
                anchor_repo,
                delivery_binding_repo,
                lineage_repo,
            },
            delivery,
        })
        .with_parent_mailbox_delivery(parent_mailbox_delivery)
        .with_human_response_mailbox_delivery(human_mailbox_delivery)
    }

    #[derive(Default)]
    struct CapturingDelivery {
        response_notifications: Mutex<Vec<CompanionGateResponseNotification>>,
        event_notifications: Mutex<Vec<CompanionGateEventNotification>>,
    }

    #[async_trait]
    impl CompanionGateNotificationDelivery for CapturingDelivery {
        async fn deliver_human_response(
            &self,
            notification: CompanionGateResponseNotification,
        ) -> Result<(), ApplicationError> {
            self.response_notifications
                .lock()
                .unwrap()
                .push(notification);
            Ok(())
        }

        async fn deliver_companion_event(
            &self,
            notification: CompanionGateEventNotification,
        ) -> Result<(), ApplicationError> {
            self.event_notifications.lock().unwrap().push(notification);
            Ok(())
        }
    }

    #[derive(Default)]
    struct CapturingParentMailboxDelivery {
        commands: Mutex<Vec<CompanionParentMailboxDeliveryCommand>>,
        parent_request_commands: Mutex<Vec<CompanionParentRequestMailboxDeliveryCommand>>,
        parent_response_commands: Mutex<Vec<CompanionParentResponseMailboxDeliveryCommand>>,
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
            self.commands.lock().unwrap().push(command);
            if let Some(message) = self.fail_with.lock().unwrap().take() {
                return Err(ApplicationError::Internal(message));
            }
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
        CompanionParentMailboxDeliveryResult {
            mailbox_message_id: Some(Uuid::new_v4()),
            command_receipt_id: Some(Uuid::new_v4()),
            command_receipt_client_command_id: client_command_id.into(),
            command_receipt_status: "accepted".to_string(),
            command_receipt_duplicate: false,
            outcome: "queued".to_string(),
            accepted_agent_run_turn_id: Some("parent-turn-1".to_string()),
            accepted_protocol_turn_id: Some("protocol-turn-1".to_string()),
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

        let gate_repo = Arc::new(MemoryGateRepo::default());
        gate_repo.create(&gate).await.expect("seed gate");
        let frame_repo = Arc::new(MemoryFrameRepo::default());
        frame_repo.create(&frame).await.expect("seed frame");
        frame_repo.seed_runtime_sessions(frame_id, ["session-old", "session-latest"]);
        let lineage_repo = Arc::new(MemoryLineageRepo::default());
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
        assert_eq!(
            result.delivery_runtime_session_id.as_deref(),
            Some("session-latest")
        );
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
        assert_eq!(commands[0].delivery_runtime_session_id, "session-latest");
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

        let gate_repo = Arc::new(MemoryGateRepo::default());
        gate_repo.create(&gate).await.expect("seed gate");
        let frame_repo = Arc::new(MemoryFrameRepo::default());
        frame_repo.create(&frame).await.expect("seed frame");
        frame_repo.seed_runtime_sessions(frame.id, ["session-1"]);
        let lineage_repo = Arc::new(MemoryLineageRepo::default());
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

        let gate_repo = Arc::new(MemoryGateRepo::default());
        gate_repo.create(&gate).await.expect("seed gate");
        let frame_repo = Arc::new(MemoryFrameRepo::default());
        frame_repo.create(&frame).await.expect("seed frame");
        frame_repo.seed_runtime_sessions(frame_id, ["session-latest"]);
        let lineage_repo = Arc::new(MemoryLineageRepo::default());
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
    async fn complete_child_result_resolves_child_owned_gate_and_delivers_events() {
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

        let gate_repo = Arc::new(MemoryGateRepo::default());
        gate_repo.create(&gate).await.expect("seed gate");
        let frame_repo = Arc::new(MemoryFrameRepo::default());
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
        let lineage_repo = Arc::new(MemoryLineageRepo::default());
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
            result.parent_delivery_runtime_session_id.as_deref(),
            Some("parent-session")
        );
        assert_eq!(
            result.child_delivery_runtime_session_id.as_deref(),
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
                mailbox_commands[0].parent_delivery_runtime_session_id,
                "parent-session"
            );
            assert!(
                mailbox_commands[0]
                    .input_text
                    .contains("Companion child result is available.")
            );
        }

        {
            let event_notifications = delivery.event_notifications.lock().unwrap();
            assert_eq!(event_notifications.len(), 2);
            assert_eq!(
                event_notifications[0].event_type,
                "companion_result_available"
            );
            assert_eq!(
                event_notifications[0].delivery_runtime_session_id,
                "parent-session"
            );
            assert_eq!(
                event_notifications[1].event_type,
                "companion_result_returned"
            );
            assert_eq!(
                event_notifications[1].delivery_runtime_session_id,
                "child-session"
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
            .expect("duplicate child result should be ignored");
        assert!(duplicate.is_none());
        assert_eq!(parent_mailbox_delivery.commands.lock().unwrap().len(), 1);
        assert_eq!(delivery.event_notifications.lock().unwrap().len(), 2);
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

        let gate_repo = Arc::new(MemoryGateRepo::default());
        let frame_repo = Arc::new(MemoryFrameRepo::default());
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
        let lineage_repo = Arc::new(MemoryLineageRepo::default());
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
        assert_eq!(result.parent_delivery_runtime_session_id, "parent-session");
        assert_eq!(result.child_delivery_runtime_session_id, "child-session");

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
            parent_request_commands[0].parent_delivery_runtime_session_id,
            "parent-session"
        );
        assert_eq!(
            parent_request_commands[0].child_delivery_runtime_session_id,
            "child-session"
        );

        let event_notifications = delivery.event_notifications.lock().unwrap();
        assert_eq!(event_notifications.len(), 1);
        assert_eq!(
            event_notifications[0].delivery_runtime_session_id,
            "parent-session"
        );
        assert_eq!(
            event_notifications[0].event_type,
            "companion_review_request"
        );
        assert_eq!(
            event_notifications[0]
                .payload
                .get("parent_frame_id")
                .and_then(serde_json::Value::as_str),
            Some(parent_frame_id.to_string().as_str())
        );
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

        let gate_repo = Arc::new(MemoryGateRepo::default());
        let frame_repo = Arc::new(MemoryFrameRepo::default());
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
        let lineage_repo = Arc::new(MemoryLineageRepo::default());
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

        let gate_repo = Arc::new(MemoryGateRepo::default());
        gate_repo.create(&gate).await.expect("seed gate");
        let frame_repo = Arc::new(MemoryFrameRepo::default());
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
        let lineage_repo = Arc::new(MemoryLineageRepo::default());
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

        let error = service
            .complete_child_result_to_parent(CompleteCompanionChildResultCommand {
                request_id: "dispatch-fail".to_string(),
                child_runtime_session_id: "child-session".to_string(),
                resolved_turn_id: "turn-child-1".to_string(),
                payload: serde_json::json!({
                    "status": "completed",
                    "summary": "review complete",
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
                .and_then(|payload| payload.get("parent_mailbox_delivery"))
                .and_then(|delivery| delivery.get("status"))
                .and_then(serde_json::Value::as_str),
            None
        );
        assert_eq!(parent_mailbox_delivery.commands.lock().unwrap().len(), 1);
        assert!(delivery.event_notifications.lock().unwrap().is_empty());
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

        let gate_repo = Arc::new(MemoryGateRepo::default());
        gate_repo.create(&gate).await.expect("seed gate");
        let frame_repo = Arc::new(MemoryFrameRepo::default());
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
        let lineage_repo = Arc::new(MemoryLineageRepo::default());
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
                .is_none()
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

        let gate_repo = Arc::new(MemoryGateRepo::default());
        let frame_repo = Arc::new(MemoryFrameRepo::default());
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
        let lineage_repo = Arc::new(MemoryLineageRepo::default());
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
        assert_eq!(
            result.parent_delivery_runtime_session_id,
            "parent-current-session"
        );

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

        let event_notifications = delivery.event_notifications.lock().unwrap();
        assert_eq!(event_notifications.len(), 1);
        assert_eq!(
            event_notifications[0].delivery_runtime_session_id,
            "parent-current-session"
        );
        assert_eq!(
            event_notifications[0]
                .payload
                .get("parent_frame_id")
                .and_then(serde_json::Value::as_str),
            Some(parent_current_frame.id.to_string().as_str())
        );
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

        let gate_repo = Arc::new(MemoryGateRepo::default());
        gate_repo.create(&gate).await.expect("seed gate");
        let frame_repo = Arc::new(MemoryFrameRepo::default());
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
        let lineage_repo = Arc::new(MemoryLineageRepo::default());
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
        assert_eq!(result.parent_delivery_runtime_session_id, "parent-session");
        assert_eq!(result.child_agent_id, child_agent_id);
        assert_eq!(result.child_frame_id, child_frame_id);
        assert_eq!(result.child_delivery_runtime_session_id, "child-session");
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
                .and_then(|payload| payload.get("child_delivery_runtime_session_id"))
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
                parent_response_commands[0].child_delivery_runtime_session_id,
                "child-session"
            );
        }

        {
            let event_notifications = delivery.event_notifications.lock().unwrap();
            assert_eq!(event_notifications.len(), 1);
            assert_eq!(
                event_notifications[0].event_type,
                "companion_parent_request_resolved"
            );
            assert_eq!(
                event_notifications[0].delivery_runtime_session_id,
                "parent-session"
            );
        }

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

        let gate_repo = Arc::new(MemoryGateRepo::default());
        gate_repo.create(&gate).await.expect("seed gate");
        let frame_repo = Arc::new(MemoryFrameRepo::default());
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
        let lineage_repo = Arc::new(MemoryLineageRepo::default());
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

        let gate_repo = Arc::new(MemoryGateRepo::default());
        gate_repo.create(&gate).await.expect("seed gate");
        let frame_repo = Arc::new(MemoryFrameRepo::default());
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
        let lineage_repo = Arc::new(MemoryLineageRepo::default());
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
