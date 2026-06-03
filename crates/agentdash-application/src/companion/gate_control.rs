use std::sync::Arc;

use agentdash_domain::workflow::{
    AgentFrameRepository, AgentLineageRepository, LifecycleAgentRepository, LifecycleGate,
    LifecycleGateRepository, RuntimeDeliverySelectionPolicy,
    RuntimeSessionExecutionAnchorRepository,
};
use async_trait::async_trait;
use uuid::Uuid;

use super::{
    PayloadTypeRegistry, build_companion_event_notification,
    build_companion_human_response_notification, payload_types,
};
use crate::workflow::resolve_current_frame_for_runtime_session;
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
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone)]
pub struct CompanionChildResultCompleteResult {
    pub gate_id: Uuid,
    pub parent_agent_id: Uuid,
    pub parent_delivery_runtime_session_id: Option<String>,
    pub child_delivery_runtime_session_id: Option<String>,
    pub payload: serde_json::Value,
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

#[derive(Clone)]
pub struct SessionEventingCompanionGateDelivery {
    eventing: SessionEventingService,
}

#[derive(Clone, Default)]
pub struct NoopCompanionGateDelivery;

impl SessionEventingCompanionGateDelivery {
    pub fn new(eventing: SessionEventingService) -> Self {
        Self { eventing }
    }
}

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
    frame_repo: Arc<dyn AgentFrameRepository>,
    agent_repo: Arc<dyn LifecycleAgentRepository>,
    anchor_repo: Arc<dyn RuntimeSessionExecutionAnchorRepository>,
    lineage_repo: Arc<dyn AgentLineageRepository>,
    delivery: Arc<dyn CompanionGateNotificationDelivery>,
}

impl CompanionGateControlService {
    pub fn new(
        gate_repo: Arc<dyn LifecycleGateRepository>,
        frame_repo: Arc<dyn AgentFrameRepository>,
        agent_repo: Arc<dyn LifecycleAgentRepository>,
        anchor_repo: Arc<dyn RuntimeSessionExecutionAnchorRepository>,
        lineage_repo: Arc<dyn AgentLineageRepository>,
        delivery: Arc<dyn CompanionGateNotificationDelivery>,
    ) -> Self {
        Self {
            gate_repo,
            frame_repo,
            agent_repo,
            anchor_repo,
            lineage_repo,
            delivery,
        }
    }

    pub fn with_session_eventing(
        gate_repo: Arc<dyn LifecycleGateRepository>,
        frame_repo: Arc<dyn AgentFrameRepository>,
        agent_repo: Arc<dyn LifecycleAgentRepository>,
        anchor_repo: Arc<dyn RuntimeSessionExecutionAnchorRepository>,
        lineage_repo: Arc<dyn AgentLineageRepository>,
        eventing: SessionEventingService,
    ) -> Self {
        Self::new(
            gate_repo,
            frame_repo,
            agent_repo,
            anchor_repo,
            lineage_repo,
            Arc::new(SessionEventingCompanionGateDelivery::new(eventing)),
        )
    }

    pub async fn respond(
        &self,
        command: RespondCompanionGateCommand,
    ) -> Result<CompanionGateRespondResult, ApplicationError> {
        if let Some(error) = payload_types::payload_object_error(&command.payload) {
            return Err(ApplicationError::BadRequest(error));
        }

        let mut gate = self.gate_repo.get(command.gate_id).await?.ok_or_else(|| {
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
        let turn_id = gate_meta
            .as_ref()
            .and_then(|metadata| metadata.get("turn_id"))
            .and_then(serde_json::Value::as_str)
            .map(str::to_string);

        let registry = PayloadTypeRegistry::with_builtins();
        if let Some(error) = registry.validate_response(&command.payload, request_type.as_deref()) {
            return Err(ApplicationError::BadRequest(error));
        }

        let delivery_runtime_session_id = self.resolve_delivery_runtime_session_id(&gate).await?;
        let request_id = gate.id.to_string();

        gate.payload_json = Some(command.payload.clone());
        gate.resolve("companion_respond");
        self.gate_repo.update(&gate).await?;

        if let Some(delivery_runtime_session_id) = delivery_runtime_session_id.clone() {
            let notification = CompanionGateResponseNotification {
                delivery_runtime_session_id,
                turn_id,
                request_id: request_id.clone(),
                payload: command.payload,
                request_type,
                gate_resolved: true,
            };
            if let Err(error) = self.delivery.deliver_human_response(notification).await {
                tracing::warn!(error = %error, gate_id = %gate.id, "companion gate resolved but runtime notification delivery failed");
            }
        }

        Ok(CompanionGateRespondResult {
            gate_id: gate.id,
            request_id,
            delivery_runtime_session_id,
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

        let child_frame = match resolve_current_frame_for_runtime_session(
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

        let mut gate = match self
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
        let summary = command
            .payload
            .get("summary")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("")
            .trim();
        let status = normalize_companion_result_status(
            command
                .payload
                .get("status")
                .and_then(serde_json::Value::as_str),
        )?;
        let resolution_payload = serde_json::json!({
            "status": status,
            "summary": summary,
            "findings": command.payload.get("findings"),
            "follow_ups": command.payload.get("follow_ups"),
            "artifact_refs": command.payload.get("artifact_refs"),
            "child_agent_id": child_frame.agent_id.to_string(),
            "resolved_turn_id": resolved_turn_id,
        });

        gate.payload_json = Some(resolution_payload.clone());
        gate.resolve(format!("child_agent:{}", child_frame.agent_id));
        self.gate_repo.update(&gate).await?;

        let parent_delivery_runtime_session_id = self
            .select_delivery_runtime_session_id(
                parent_agent_id,
                RuntimeDeliverySelectionPolicy::LatestAttached,
            )
            .await?;
        let child_delivery_runtime_session_id = self
            .select_delivery_runtime_session_id(
                child_frame.agent_id,
                RuntimeDeliverySelectionPolicy::Specific {
                    runtime_session_id: child_runtime_session_id,
                },
            )
            .await?;

        if let Some(session_id) = parent_delivery_runtime_session_id.clone() {
            let notification = CompanionGateEventNotification {
                delivery_runtime_session_id: session_id,
                turn_id: resolved_turn_id.clone(),
                event_type: "companion_result_available".to_string(),
                message: "Companion child agent 已回传结果 (gate resolved)".to_string(),
                payload: resolution_payload.clone(),
            };
            if let Err(error) = self.delivery.deliver_companion_event(notification).await {
                tracing::warn!(error = %error, gate_id = %gate.id, parent_agent_id = %parent_agent_id, "companion gate resolved but parent result notification delivery failed");
            }
        }

        if let Some(session_id) = child_delivery_runtime_session_id.clone() {
            let notification = CompanionGateEventNotification {
                delivery_runtime_session_id: session_id,
                turn_id: resolved_turn_id.clone(),
                event_type: "companion_result_returned".to_string(),
                message: "已通过 LifecycleGate 回传结果到 parent agent".to_string(),
                payload: resolution_payload.clone(),
            };
            if let Err(error) = self.delivery.deliver_companion_event(notification).await {
                tracing::warn!(error = %error, gate_id = %gate.id, child_agent_id = %child_frame.agent_id, "companion gate resolved but child result notification delivery failed");
            }
        }

        Ok(Some(CompanionChildResultCompleteResult {
            gate_id: gate.id,
            parent_agent_id,
            parent_delivery_runtime_session_id,
            child_delivery_runtime_session_id,
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
        let prompt = command
            .payload
            .get("prompt")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| ApplicationError::BadRequest("payload.prompt 不能为空".to_string()))?;

        let (_anchor, _agent, child_frame) = resolve_current_frame_for_runtime_session(
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
        let child_delivery_runtime_session_id = self
            .select_delivery_runtime_session_id(
                child_frame.agent_id,
                RuntimeDeliverySelectionPolicy::Specific {
                    runtime_session_id: command.child_runtime_session_id.clone(),
                },
            )
            .await?
            .ok_or_else(|| {
                ApplicationError::Conflict(format!(
                    "child agent {} 没有关联 runtime session {} 的 anchor",
                    child_frame.agent_id, command.child_runtime_session_id
                ))
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
        let parent_frame = self
            .frame_repo
            .get_current(parent_agent_id)
            .await?
            .ok_or_else(|| {
                ApplicationError::Conflict("parent agent 没有活跃的 frame".to_string())
            })?;
        let parent_delivery_runtime_session_id = self
            .select_delivery_runtime_session_id(
                parent_agent_id,
                RuntimeDeliverySelectionPolicy::LatestAttached,
            )
            .await?
            .ok_or_else(|| {
                ApplicationError::Conflict(
                    "parent agent 没有关联的 runtime session anchor".to_string(),
                )
            })?;

        let companion_label = format!("child:{}", child_frame.agent_id);
        let mut gate = LifecycleGate::open(
            lineage.run_id,
            Some(parent_agent_id),
            Some(parent_frame.id),
            COMPANION_PARENT_REQUEST_GATE_KIND,
            "pending-parent-request",
            None,
        );
        gate.correlation_id = gate.id.to_string();
        let request_id = gate.id.to_string();
        let review_payload = serde_json::json!({
            "gate_id": request_id,
            "request_id": request_id,
            "run_id": lineage.run_id.to_string(),
            "child_agent_id": child_frame.agent_id.to_string(),
            "child_frame_id": child_frame.id.to_string(),
            "parent_agent_id": parent_agent_id.to_string(),
            "parent_frame_id": parent_frame.id.to_string(),
            "companion_label": companion_label,
            "companion_session_id": child_delivery_runtime_session_id,
            "parent_session_id": parent_delivery_runtime_session_id,
            "request_type": "review",
            "adoption_mode": agentdash_spi::action_type::FOLLOW_UP_REQUIRED,
            "status": "pending",
            "summary": prompt,
            "turn_id": command.turn_id,
            "wait": command.wait,
            "payload": command.payload,
        });
        gate.payload_json = Some(review_payload.clone());
        self.gate_repo.create(&gate).await?;

        let notification = CompanionGateEventNotification {
            delivery_runtime_session_id: parent_delivery_runtime_session_id.clone(),
            turn_id: command.turn_id,
            event_type: "companion_review_request".to_string(),
            message: format!("Companion `{companion_label}` 请求审阅: {prompt}"),
            payload: review_payload.clone(),
        };
        if let Err(error) = self.delivery.deliver_companion_event(notification).await {
            tracing::warn!(error = %error, gate_id = %gate.id, parent_agent_id = %parent_agent_id, "parent companion request gate opened but runtime notification delivery failed");
        }

        Ok(CompanionParentRequestOpenResult {
            gate_id: gate.id,
            request_id,
            run_id: gate.run_id,
            parent_agent_id,
            parent_frame_id: parent_frame.id,
            parent_delivery_runtime_session_id,
            child_agent_id: child_frame.agent_id,
            child_frame_id: child_frame.id,
            child_delivery_runtime_session_id,
            companion_label,
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
        let Some(mut gate) = self.gate_repo.get(gate_id).await? else {
            return Ok(None);
        };
        if gate.gate_kind != COMPANION_PARENT_REQUEST_GATE_KIND {
            return Ok(None);
        }

        let (_anchor, _agent, parent_frame) = resolve_current_frame_for_runtime_session(
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
            .select_delivery_runtime_session_id(
                parent_frame.agent_id,
                RuntimeDeliverySelectionPolicy::Specific {
                    runtime_session_id: command.parent_runtime_session_id.clone(),
                },
            )
            .await?
            .ok_or_else(|| {
                ApplicationError::Conflict(format!(
                    "parent agent {} 没有关联 runtime session {} 的 anchor",
                    parent_frame.agent_id, command.parent_runtime_session_id
                ))
            })?;

        let mut resolution_payload = command.payload.clone();
        if let Some(object) = resolution_payload.as_object_mut() {
            object.insert(
                "gate_id".to_string(),
                serde_json::Value::String(gate.id.to_string()),
            );
            object.insert(
                "request_id".to_string(),
                serde_json::Value::String(gate.id.to_string()),
            );
            object.insert(
                "resolved_turn_id".to_string(),
                serde_json::Value::String(command.resolved_turn_id.clone()),
            );
        }

        gate.payload_json = Some(resolution_payload.clone());
        gate.resolve(format!("parent_agent:{}", parent_frame.agent_id));
        self.gate_repo.update(&gate).await?;

        let notification = CompanionGateEventNotification {
            delivery_runtime_session_id: parent_delivery_runtime_session_id.clone(),
            turn_id: command.resolved_turn_id,
            event_type: "companion_parent_request_resolved".to_string(),
            message: "Parent companion request 已通过 LifecycleGate resolve".to_string(),
            payload: resolution_payload.clone(),
        };
        if let Err(error) = self.delivery.deliver_companion_event(notification).await {
            tracing::warn!(error = %error, gate_id = %gate.id, parent_agent_id = %parent_frame.agent_id, "parent companion request gate resolved but runtime notification delivery failed");
        }

        Ok(Some(CompanionParentRequestResolveResult {
            gate_id: gate.id,
            parent_agent_id: parent_frame.agent_id,
            parent_frame_id: parent_frame.id,
            parent_delivery_runtime_session_id,
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

        self.select_delivery_runtime_session_id(
            frame.agent_id,
            RuntimeDeliverySelectionPolicy::LatestAttached,
        )
        .await
    }

    async fn select_delivery_runtime_session_id(
        &self,
        agent_id: Uuid,
        policy: RuntimeDeliverySelectionPolicy,
    ) -> Result<Option<String>, ApplicationError> {
        let runtime_session_id = match policy {
            RuntimeDeliverySelectionPolicy::Specific { runtime_session_id } => self
                .anchor_repo
                .find_by_session(&runtime_session_id)
                .await?
                .filter(|anchor| anchor.agent_id == agent_id)
                .map(|anchor| anchor.runtime_session_id),
            RuntimeDeliverySelectionPolicy::LaunchPrimary => self
                .anchor_repo
                .list_by_agent(agent_id)
                .await?
                .into_iter()
                .min_by_key(|anchor| anchor.created_at)
                .map(|anchor| anchor.runtime_session_id),
            RuntimeDeliverySelectionPolicy::LatestAttached => self
                .anchor_repo
                .latest_for_agent(agent_id)
                .await?
                .map(|anchor| anchor.runtime_session_id),
        };
        Ok(runtime_session_id)
    }
}

fn normalize_companion_result_status(
    status: Option<&str>,
) -> Result<&'static str, ApplicationError> {
    match status.unwrap_or("completed").trim() {
        "" => Ok("completed"),
        "completed" => Ok("completed"),
        "blocked" => Ok("blocked"),
        "needs_follow_up" => Ok("needs_follow_up"),
        other => Err(ApplicationError::BadRequest(format!(
            "payload.status 不支持 `{other}`，应为 completed/blocked/needs_follow_up"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::HashMap,
        sync::{Arc, Mutex},
    };

    use agentdash_domain::{
        DomainError,
        workflow::{AgentFrame, AgentLineage, LifecycleAgent, RuntimeSessionExecutionAnchor},
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
                .find(|frame| frame.agent_id == agent_id)
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

        async fn append_visible_canvas_mount(
            &self,
            frame_id: Uuid,
            mount_id: &str,
        ) -> Result<(), DomainError> {
            if let Some(frame) = self.frames.lock().unwrap().get_mut(&frame_id) {
                frame.append_visible_canvas_mount(mount_id);
            }
            Ok(())
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
    }

    #[derive(Default)]
    struct MemoryAgentRepo {
        agents: Mutex<HashMap<Uuid, LifecycleAgent>>,
    }

    impl MemoryAgentRepo {
        fn from_frame_repo(frame_repo: &MemoryFrameRepo, run_id: Uuid, project_id: Uuid) -> Self {
            let mut agents = HashMap::new();
            for frame in frame_repo.frames.lock().unwrap().values() {
                let mut agent = LifecycleAgent::new_root(run_id, project_id, "test");
                agent.id = frame.agent_id;
                agent.status = "running".to_string();
                agent.set_current_frame(frame.id);
                agents.insert(agent.id, agent);
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
                            frame.graph_instance_id,
                            frame.activity_key.clone(),
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
        async fn upsert(&self, anchor: &RuntimeSessionExecutionAnchor) -> Result<(), DomainError> {
            self.anchors
                .lock()
                .unwrap()
                .insert(anchor.runtime_session_id.clone(), anchor.clone());
            Ok(())
        }

        async fn update_assignment(
            &self,
            runtime_session_id: &str,
            assignment_id: Uuid,
            attempt: i32,
        ) -> Result<(), DomainError> {
            if let Some(anchor) = self.anchors.lock().unwrap().get_mut(runtime_session_id) {
                anchor.fill_assignment(assignment_id, attempt);
            }
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

        async fn latest_for_agent(
            &self,
            agent_id: Uuid,
        ) -> Result<Option<RuntimeSessionExecutionAnchor>, DomainError> {
            Ok(self
                .anchors
                .lock()
                .unwrap()
                .values()
                .filter(|anchor| anchor.agent_id == agent_id)
                .max_by_key(|anchor| anchor.updated_at)
                .cloned())
        }
    }

    fn service_for_test(
        gate_repo: Arc<MemoryGateRepo>,
        frame_repo: Arc<MemoryFrameRepo>,
        lineage_repo: Arc<MemoryLineageRepo>,
        delivery: Arc<CapturingDelivery>,
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
        CompanionGateControlService::new(
            gate_repo,
            frame_repo,
            agent_repo,
            anchor_repo,
            lineage_repo,
            delivery,
        )
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
        let service = service_for_test(
            gate_repo.clone(),
            frame_repo,
            lineage_repo,
            delivery.clone(),
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

        let notifications = delivery.response_notifications.lock().unwrap();
        assert_eq!(notifications.len(), 1);
        assert_eq!(
            notifications[0].delivery_runtime_session_id,
            "session-latest"
        );
        assert_eq!(notifications[0].turn_id.as_deref(), Some("turn-1"));
        assert_eq!(notifications[0].request_id, gate_id.to_string());
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
        let service = service_for_test(
            gate_repo.clone(),
            frame_repo,
            lineage_repo,
            delivery.clone(),
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
                wait: true,
                payload: serde_json::json!({ "prompt": "please review" }),
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
    async fn resolve_parent_request_resolves_only_parent_owned_gate() {
        let run_id = Uuid::new_v4();
        let parent_agent_id = Uuid::new_v4();
        let parent_frame = AgentFrame::new_revision(parent_agent_id, 1, "parent");
        let parent_frame_id = parent_frame.id;
        let mut gate = LifecycleGate::open(
            run_id,
            Some(parent_agent_id),
            Some(parent_frame_id),
            COMPANION_PARENT_REQUEST_GATE_KIND,
            "pending-parent-request",
            Some(serde_json::json!({
                "request_type": "review",
                "summary": "please review"
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
        let lineage_repo = Arc::new(MemoryLineageRepo::default());
        let delivery = Arc::new(CapturingDelivery::default());
        let service = service_for_test(
            gate_repo.clone(),
            frame_repo,
            lineage_repo,
            delivery.clone(),
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
