use std::sync::Arc;

use agentdash_application_workflow::gate::{
    GateDeliveryIntent, GateMailboxWakeIntent, GateProducerTerminalConvergenceResult,
    GateProducerTerminalConvergenceService, GateProducerTerminalEvent,
};
use agentdash_diagnostics::{Subsystem, diag};
use agentdash_domain::workflow::{AgentRunDeliveryBindingRepository, LifecycleGateRepository};
use async_trait::async_trait;

use crate::ApplicationError;
use crate::companion::gate_control::{
    CompanionParentMailboxDelivery, CompanionParentMailboxDeliveryCommand,
    build_parent_result_mailbox_input_text,
};

#[async_trait]
pub trait GateProducerTerminalConvergencePort: Send + Sync {
    async fn observe_gate_producer_terminal(
        &self,
        event: GateProducerTerminalEvent,
    ) -> Result<GateProducerTerminalConvergenceResult, ApplicationError>;
}

#[derive(Clone)]
pub struct GateProducerTerminalConvergenceDeps {
    pub gate_repo: Arc<dyn LifecycleGateRepository>,
    pub delivery_binding_repo: Arc<dyn AgentRunDeliveryBindingRepository>,
    pub mailbox_wake_delivery: Arc<dyn GateMailboxWakeDelivery>,
}

#[derive(Clone)]
pub struct GateProducerTerminalConvergenceServiceAdapter {
    deps: GateProducerTerminalConvergenceDeps,
}

impl GateProducerTerminalConvergenceServiceAdapter {
    pub fn new(deps: GateProducerTerminalConvergenceDeps) -> Self {
        Self { deps }
    }

    pub fn with_mailbox_wake_delivery(
        gate_repo: Arc<dyn LifecycleGateRepository>,
        delivery_binding_repo: Arc<dyn AgentRunDeliveryBindingRepository>,
        mailbox_wake_delivery: Arc<dyn GateMailboxWakeDelivery>,
    ) -> Self {
        Self::new(GateProducerTerminalConvergenceDeps {
            gate_repo,
            delivery_binding_repo,
            mailbox_wake_delivery,
        })
    }

    #[cfg(test)]
    pub fn noop(
        gate_repo: Arc<dyn LifecycleGateRepository>,
        delivery_binding_repo: Arc<dyn AgentRunDeliveryBindingRepository>,
    ) -> Self {
        Self::with_mailbox_wake_delivery(
            gate_repo,
            delivery_binding_repo,
            Arc::new(NoopGateMailboxWakeDelivery),
        )
    }

    async fn observe(
        &self,
        event: GateProducerTerminalEvent,
    ) -> Result<GateProducerTerminalConvergenceResult, ApplicationError> {
        let result = GateProducerTerminalConvergenceService::new(
            self.deps.gate_repo.clone(),
            self.deps.delivery_binding_repo.clone(),
        )
        .observe_producer_terminal(event.clone())
        .await
        .map_err(application_error_from_workflow_gate_error)?;

        if result.no_matching_gate_wait_policy() {
            diag!(
                Debug,
                Subsystem::AgentRun,
                producer = ?event.producer,
                terminal_state = %event.terminal_state,
                delivery_trace_ref = ?event.trace_ref,
                "gate producer terminal fallback found no matching gate wait policy"
            );
        }

        for outcome in &result.outcomes {
            for intent in &outcome.delivery_intents {
                if let GateDeliveryIntent::MailboxWake(intent) = intent {
                    self.deps
                        .mailbox_wake_delivery
                        .deliver_mailbox_wake(intent)
                        .await?;
                }
            }
            diag!(
                Debug,
                Subsystem::AgentRun,
                gate_id = %outcome.gate_id,
                outcome_kind = ?outcome.kind,
                result_status = ?outcome.result_status,
                delivery_intent_count = outcome.delivery_intents.len(),
                "gate producer terminal fallback outcome delivered"
            );
        }
        Ok(result)
    }
}

#[async_trait]
impl GateProducerTerminalConvergencePort for GateProducerTerminalConvergenceServiceAdapter {
    async fn observe_gate_producer_terminal(
        &self,
        event: GateProducerTerminalEvent,
    ) -> Result<GateProducerTerminalConvergenceResult, ApplicationError> {
        self.observe(event).await
    }
}

fn companion_label_from_wake(intent: &GateMailboxWakeIntent) -> String {
    intent
        .payload
        .get("display")
        .and_then(serde_json::Value::as_object)
        .and_then(|display| display.get("companion_label"))
        .and_then(serde_json::Value::as_str)
        .or_else(|| {
            intent
                .payload
                .get("companion_label")
                .and_then(serde_json::Value::as_str)
        })
        .unwrap_or("companion")
        .to_string()
}

#[async_trait]
pub trait GateMailboxWakeDelivery: Send + Sync {
    async fn deliver_mailbox_wake(
        &self,
        intent: &GateMailboxWakeIntent,
    ) -> Result<(), ApplicationError>;
}

#[derive(Clone)]
pub struct CompanionGateMailboxWakeDelivery {
    companion_parent_mailbox_delivery: Arc<dyn CompanionParentMailboxDelivery>,
}

impl CompanionGateMailboxWakeDelivery {
    pub fn new(companion_parent_mailbox_delivery: Arc<dyn CompanionParentMailboxDelivery>) -> Self {
        Self {
            companion_parent_mailbox_delivery,
        }
    }
}

#[async_trait]
impl GateMailboxWakeDelivery for CompanionGateMailboxWakeDelivery {
    async fn deliver_mailbox_wake(
        &self,
        intent: &GateMailboxWakeIntent,
    ) -> Result<(), ApplicationError> {
        if intent.namespace != "companion" {
            return Err(ApplicationError::Conflict(format!(
                "unsupported gate mailbox wake namespace `{}` for gate {}",
                intent.namespace, intent.gate_id
            )));
        }
        deliver_companion_child_result_to_parent(
            self.companion_parent_mailbox_delivery.as_ref(),
            intent,
        )
        .await
    }
}

#[cfg(test)]
#[derive(Clone, Default)]
struct NoopGateMailboxWakeDelivery;

#[cfg(test)]
#[async_trait]
impl GateMailboxWakeDelivery for NoopGateMailboxWakeDelivery {
    async fn deliver_mailbox_wake(
        &self,
        _intent: &GateMailboxWakeIntent,
    ) -> Result<(), ApplicationError> {
        Ok(())
    }
}

async fn deliver_companion_child_result_to_parent(
    delivery: &dyn CompanionParentMailboxDelivery,
    intent: &GateMailboxWakeIntent,
) -> Result<(), ApplicationError> {
    let companion_label = companion_label_from_wake(intent);
    let summary = intent
        .payload
        .get("summary")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("")
        .trim();
    let status = intent
        .payload
        .get("status")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("completed");
    let input_text = build_parent_result_mailbox_input_text(
        intent.gate_id,
        &intent.request_id,
        &companion_label,
        status,
        summary,
        &intent.payload,
    );
    delivery
        .deliver_child_result_to_parent(CompanionParentMailboxDeliveryCommand {
            gate_id: intent.gate_id,
            request_id: intent.request_id.clone(),
            run_id: intent.target_run_id,
            parent_agent_id: intent.target_agent_id,
            parent_delivery_runtime_session_id: intent.target_delivery_runtime_session_id.clone(),
            child_agent_id: intent.producer_agent_id,
            child_delivery_runtime_session_id: intent.producer_delivery_runtime_session_id.clone(),
            resolved_turn_id: intent.resolved_turn_id.clone(),
            payload: intent.payload.clone(),
            input_text,
        })
        .await?;
    Ok(())
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
