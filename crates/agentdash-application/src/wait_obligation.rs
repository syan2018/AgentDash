use std::sync::Arc;

use agentdash_application_workflow::gate::{
    GateDeliveryIntent, GateMailboxWakeIntent, GateNotificationIntent,
    GateProducerTerminalConvergenceResult, GateProducerTerminalConvergenceService,
    GateProducerTerminalEvent,
};
use agentdash_diagnostics::{DiagnosticErrorContext, Subsystem, diag, diag_error};
use agentdash_domain::workflow::{AgentRunDeliveryBindingRepository, LifecycleGateRepository};
use async_trait::async_trait;
use uuid::Uuid;

use crate::ApplicationError;
use crate::companion::gate_control::{
    CompanionGateEventNotification, CompanionGateNotificationDelivery,
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
    pub companion_event_delivery: Arc<dyn CompanionGateNotificationDelivery>,
    pub companion_parent_mailbox_delivery: Arc<dyn CompanionParentMailboxDelivery>,
}

#[derive(Clone)]
pub struct GateProducerTerminalConvergenceServiceAdapter {
    deps: GateProducerTerminalConvergenceDeps,
}

impl GateProducerTerminalConvergenceServiceAdapter {
    pub fn new(deps: GateProducerTerminalConvergenceDeps) -> Self {
        Self { deps }
    }

    pub fn with_companion_delivery(
        gate_repo: Arc<dyn LifecycleGateRepository>,
        delivery_binding_repo: Arc<dyn AgentRunDeliveryBindingRepository>,
        companion_event_delivery: Arc<dyn CompanionGateNotificationDelivery>,
        companion_parent_mailbox_delivery: Arc<dyn CompanionParentMailboxDelivery>,
    ) -> Self {
        Self::new(GateProducerTerminalConvergenceDeps {
            gate_repo,
            delivery_binding_repo,
            companion_event_delivery,
            companion_parent_mailbox_delivery,
        })
    }

    #[cfg(test)]
    pub fn noop(
        gate_repo: Arc<dyn LifecycleGateRepository>,
        delivery_binding_repo: Arc<dyn AgentRunDeliveryBindingRepository>,
    ) -> Self {
        Self::with_companion_delivery(
            gate_repo,
            delivery_binding_repo,
            Arc::new(crate::companion::gate_control::NoopCompanionGateDelivery),
            Arc::new(crate::companion::gate_control::NoopCompanionParentMailboxDelivery),
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

        if result.no_matching_obligation() {
            diag!(
                Debug,
                Subsystem::AgentRun,
                producer = ?event.producer,
                terminal_state = %event.terminal_state,
                delivery_trace_ref = ?event.trace_ref,
                "gate producer terminal convergence found no matching obligation"
            );
        }

        for outcome in &result.outcomes {
            for intent in &outcome.delivery_intents {
                if let GateDeliveryIntent::MailboxWake(intent) = intent {
                    if intent.namespace != "companion" {
                        return Err(ApplicationError::Conflict(format!(
                            "unsupported gate mailbox wake namespace `{}` for gate {}",
                            intent.namespace, intent.gate_id
                        )));
                    }
                    let companion_label = companion_label_from_wake(intent);
                    deliver_companion_child_result_to_parent(
                        self.deps.companion_parent_mailbox_delivery.as_ref(),
                        CompanionChildResultDeliveryInput {
                            gate_id: intent.gate_id,
                            request_id: intent.request_id.clone(),
                            run_id: intent.target_run_id,
                            parent_agent_id: intent.target_agent_id,
                            parent_delivery_runtime_session_id: intent
                                .target_delivery_runtime_session_id
                                .clone(),
                            child_agent_id: intent.producer_agent_id,
                            child_delivery_runtime_session_id: intent
                                .producer_delivery_runtime_session_id
                                .clone(),
                            resolved_turn_id: intent.resolved_turn_id.clone(),
                            companion_label,
                            payload: intent.payload.clone(),
                        },
                    )
                    .await?;
                }
            }
            deliver_notification_intents(
                self.deps.companion_event_delivery.as_ref(),
                &outcome.notification_intents,
                outcome.gate_id,
                None,
            )
            .await;
            diag!(
                Debug,
                Subsystem::AgentRun,
                gate_id = %outcome.gate_id,
                outcome_kind = ?outcome.kind,
                result_status = ?outcome.result_status,
                delivery_intent_count = outcome.delivery_intents.len(),
                notification_intent_count = outcome.notification_intents.len(),
                "gate producer terminal convergence outcome delivered"
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

struct CompanionChildResultDeliveryInput {
    gate_id: Uuid,
    request_id: String,
    run_id: Uuid,
    parent_agent_id: Uuid,
    parent_delivery_runtime_session_id: String,
    child_agent_id: Uuid,
    child_delivery_runtime_session_id: Option<String>,
    resolved_turn_id: String,
    companion_label: String,
    payload: serde_json::Value,
}

async fn deliver_companion_child_result_to_parent(
    delivery: &dyn CompanionParentMailboxDelivery,
    input: CompanionChildResultDeliveryInput,
) -> Result<(), ApplicationError> {
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
    let input_text = build_parent_result_mailbox_input_text(
        input.gate_id,
        &input.request_id,
        &input.companion_label,
        status,
        summary,
        &input.payload,
    );
    delivery
        .deliver_child_result_to_parent(CompanionParentMailboxDeliveryCommand {
            gate_id: input.gate_id,
            request_id: input.request_id,
            run_id: input.run_id,
            parent_agent_id: input.parent_agent_id,
            parent_delivery_runtime_session_id: input.parent_delivery_runtime_session_id,
            child_agent_id: input.child_agent_id,
            child_delivery_runtime_session_id: input.child_delivery_runtime_session_id,
            resolved_turn_id: input.resolved_turn_id,
            payload: input.payload,
            input_text,
        })
        .await?;
    Ok(())
}

async fn deliver_notification_intents(
    delivery: &dyn CompanionGateNotificationDelivery,
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
        if let Err(error) = delivery.deliver_companion_event(notification).await {
            let mut context = DiagnosticErrorContext::new(
                "gate_producer_terminal.gate_notification",
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
                "gate producer terminal notification delivery failed"
            );
        }
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
