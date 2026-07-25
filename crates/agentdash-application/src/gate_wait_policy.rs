use std::sync::Arc;

use agentdash_application_agentrun::agent_run::AgentRunProductRuntimeBindingRepository;
use agentdash_application_workflow::gate::{
    GateDeliveryIntent, GateInputHandoffWakeIntent, GateProducerTerminalConvergenceResult,
    GateProducerTerminalConvergenceService, GateProducerTerminalEvent,
    GateWakeTargetRuntimeThreadQuery,
};
use agentdash_diagnostics::{Subsystem, diag};
use agentdash_domain::workflow::LifecycleGateRepository;
use async_trait::async_trait;

use crate::ApplicationError;
use crate::companion::gate_control::{
    CompanionParentInputHandoffDelivery, CompanionParentInputHandoffDeliveryCommand,
    build_parent_result_delivery_projection_text,
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
    pub runtime_binding_repo: Arc<dyn AgentRunProductRuntimeBindingRepository>,
    pub input_handoff_wake_delivery: Arc<dyn GateInputHandoffWakeDelivery>,
}

#[derive(Clone)]
pub struct GateProducerTerminalConvergenceServiceAdapter {
    deps: GateProducerTerminalConvergenceDeps,
}

impl GateProducerTerminalConvergenceServiceAdapter {
    pub fn new(deps: GateProducerTerminalConvergenceDeps) -> Self {
        Self { deps }
    }

    pub fn with_input_handoff_wake_delivery(
        gate_repo: Arc<dyn LifecycleGateRepository>,
        runtime_binding_repo: Arc<dyn AgentRunProductRuntimeBindingRepository>,
        input_handoff_wake_delivery: Arc<dyn GateInputHandoffWakeDelivery>,
    ) -> Self {
        Self::new(GateProducerTerminalConvergenceDeps {
            gate_repo,
            runtime_binding_repo,
            input_handoff_wake_delivery,
        })
    }

    #[cfg(test)]
    pub fn noop(
        gate_repo: Arc<dyn LifecycleGateRepository>,
        runtime_binding_repo: Arc<dyn AgentRunProductRuntimeBindingRepository>,
    ) -> Self {
        Self::with_input_handoff_wake_delivery(
            gate_repo,
            runtime_binding_repo,
            Arc::new(NoopGateInputHandoffWakeDelivery),
        )
    }

    async fn observe(
        &self,
        event: GateProducerTerminalEvent,
    ) -> Result<GateProducerTerminalConvergenceResult, ApplicationError> {
        let result = GateProducerTerminalConvergenceService::new(
            self.deps.gate_repo.clone(),
            Arc::new(ProductGateWakeTargetRuntimeThreadQuery {
                bindings: self.deps.runtime_binding_repo.clone(),
            }),
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
                if let GateDeliveryIntent::InputHandoffWake(intent) = intent {
                    self.deps
                        .input_handoff_wake_delivery
                        .deliver_input_handoff_wake(intent)
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

struct ProductGateWakeTargetRuntimeThreadQuery {
    bindings: Arc<dyn AgentRunProductRuntimeBindingRepository>,
}

#[async_trait]
impl GateWakeTargetRuntimeThreadQuery for ProductGateWakeTargetRuntimeThreadQuery {
    async fn resolve_runtime_thread(
        &self,
        run_id: uuid::Uuid,
        agent_id: uuid::Uuid,
    ) -> Result<Option<String>, String> {
        self.bindings
            .load_product_binding(&agentdash_domain::agent_run_target::AgentRunTarget {
                run_id,
                agent_id,
            })
            .await
            .map(|binding| binding.map(|binding| binding.runtime_thread_id.to_string()))
    }
}

fn companion_label_from_wake(intent: &GateInputHandoffWakeIntent) -> String {
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
pub trait GateInputHandoffWakeDelivery: Send + Sync {
    async fn deliver_input_handoff_wake(
        &self,
        intent: &GateInputHandoffWakeIntent,
    ) -> Result<(), ApplicationError>;
}

#[derive(Clone)]
pub struct CompanionGateInputHandoffWakeDelivery {
    companion_parent_input_handoff_delivery: Arc<dyn CompanionParentInputHandoffDelivery>,
}

impl CompanionGateInputHandoffWakeDelivery {
    pub fn new(
        companion_parent_input_handoff_delivery: Arc<dyn CompanionParentInputHandoffDelivery>,
    ) -> Self {
        Self {
            companion_parent_input_handoff_delivery,
        }
    }
}

#[async_trait]
impl GateInputHandoffWakeDelivery for CompanionGateInputHandoffWakeDelivery {
    async fn deliver_input_handoff_wake(
        &self,
        intent: &GateInputHandoffWakeIntent,
    ) -> Result<(), ApplicationError> {
        if intent.namespace != "companion" {
            return Err(ApplicationError::Conflict(format!(
                "unsupported gate input_handoff wake namespace `{}` for gate {}",
                intent.namespace, intent.gate_id
            )));
        }
        deliver_companion_child_result_to_parent(
            self.companion_parent_input_handoff_delivery.as_ref(),
            intent,
        )
        .await
    }
}

#[cfg(test)]
#[derive(Clone, Default)]
struct NoopGateInputHandoffWakeDelivery;

#[cfg(test)]
#[async_trait]
impl GateInputHandoffWakeDelivery for NoopGateInputHandoffWakeDelivery {
    async fn deliver_input_handoff_wake(
        &self,
        _intent: &GateInputHandoffWakeIntent,
    ) -> Result<(), ApplicationError> {
        Ok(())
    }
}

async fn deliver_companion_child_result_to_parent(
    delivery: &dyn CompanionParentInputHandoffDelivery,
    intent: &GateInputHandoffWakeIntent,
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
    let input_text = build_parent_result_delivery_projection_text(
        intent.gate_id,
        &intent.request_id,
        &companion_label,
        status,
        summary,
        &intent.payload,
    );
    delivery
        .deliver_child_result_to_parent(CompanionParentInputHandoffDeliveryCommand {
            gate_id: intent.gate_id,
            request_id: intent.request_id.clone(),
            run_id: intent.target_run_id,
            parent_agent_id: intent.target_agent_id,
            parent_runtime_thread_id: intent.target_runtime_thread_id.clone(),
            child_agent_id: intent.producer_agent_id,
            child_runtime_thread_id: intent.producer_runtime_thread_id.clone(),
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
