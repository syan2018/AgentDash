use async_trait::async_trait;
use serde_json::Value;
use uuid::Uuid;

use agentdash_agent_runtime_contract::RuntimeThreadId;
use agentdash_platform_spi::{AgentConfig, HookResolution};

use super::CompanionDispatchTargetPlan;
use crate::agent_run::PreparedAgentRunProductInputDelivery;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompanionContinuationRuntimeProtocol {
    FullFork,
    FreshCreate,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CompanionContinuationInputSource {
    pub namespace: String,
    pub kind: String,
    pub source_ref: Option<String>,
    pub correlation_ref: Option<String>,
    pub actor: String,
    pub route: Option<String>,
    pub display_label_key: String,
    pub metadata: Option<Value>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CompanionContinuationRequest {
    pub request_id: Uuid,
    pub dispatch_id: String,
    pub runtime_protocol: CompanionContinuationRuntimeProtocol,
    pub runtime_protocol_request_id: Uuid,
    pub project_id: Uuid,
    pub parent_run_id: Uuid,
    pub parent_agent_id: Uuid,
    pub parent_frame_id: Uuid,
    pub child_run_id: Uuid,
    pub child_agent_id: Uuid,
    pub child_frame_id: Option<Uuid>,
    pub child_runtime_thread_id: RuntimeThreadId,
    pub selected_project_agent_id: Uuid,
    pub selected_agent_key: String,
    pub companion_executor_config: AgentConfig,
    pub parent_runtime_thread_id: String,
    pub parent_turn_id: String,
    pub protocol_plan: CompanionDispatchTargetPlan,
    pub companion_label: String,
    pub slice_mode: String,
    pub adoption_mode: String,
    pub wait: bool,
    pub task_id: Option<Uuid>,
    pub first_input_text: String,
    pub first_input_source: CompanionContinuationInputSource,
    pub after_dispatch_hook_effect: CompanionContinuationEffectIdentity,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompanionContinuationPhase {
    Requested,
    RuntimeReady,
    FirstInputPrepared,
    FirstInputConverged,
    GateConverged,
    ChannelConverged,
    TaskConverged,
    AfterDispatchHookConverged,
    Succeeded,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompanionContinuationEffectIdentity {
    pub effect_id: Uuid,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompanionRuntimeReadyEvidence {
    pub child_run_id: Uuid,
    pub child_agent_id: Uuid,
    pub child_frame_id: Uuid,
    pub child_runtime_thread_id: RuntimeThreadId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompanionFirstInputEvidence {
    pub input_handoff_id: Uuid,
    pub runtime_operation_id: Option<String>,
    pub submitted_by_runtime_protocol: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CompanionPreparedFirstInputEvidence {
    ProductDelivery {
        envelope: PreparedAgentRunProductInputDelivery,
    },
    FreshRuntimeProtocol,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompanionGateEvidence {
    pub gate_id: Option<Uuid>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompanionChannelEvidence {
    pub channel_id: Uuid,
    pub delivery_id: Uuid,
    pub input_handoff_id: Uuid,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompanionTaskEvidence {
    pub task_id: Option<Uuid>,
    pub assigned_agent_id: Option<Uuid>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CompanionAfterDispatchHookEvidence {
    pub effect: CompanionContinuationEffectIdentity,
    pub parent_frame_id: Uuid,
    pub child_frame_id: Uuid,
    pub child_runtime_thread_id: RuntimeThreadId,
    pub resolution: HookResolution,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct CompanionContinuationEvidence {
    pub runtime: Option<CompanionRuntimeReadyEvidence>,
    pub prepared_first_input: Option<CompanionPreparedFirstInputEvidence>,
    pub first_input: Option<CompanionFirstInputEvidence>,
    pub gate: Option<CompanionGateEvidence>,
    pub channel: Option<CompanionChannelEvidence>,
    pub task: Option<CompanionTaskEvidence>,
    pub after_dispatch_hook: Option<CompanionAfterDispatchHookEvidence>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CompanionContinuation {
    request: CompanionContinuationRequest,
    phase: CompanionContinuationPhase,
    evidence: CompanionContinuationEvidence,
}

impl CompanionContinuation {
    pub fn requested(request: CompanionContinuationRequest) -> Result<Self, String> {
        if request.dispatch_id.trim().is_empty()
            || request.first_input_text.trim().is_empty()
            || request.first_input_source.namespace.trim().is_empty()
            || request.first_input_source.kind.trim().is_empty()
            || request.first_input_source.actor.trim().is_empty()
            || request.parent_turn_id.trim().is_empty()
            || request.slice_mode.trim().is_empty()
        {
            return Err("Companion continuation request is incomplete".to_owned());
        }
        if request.after_dispatch_hook_effect
            != companion_after_dispatch_hook_effect_identity(request.request_id)
        {
            return Err("Companion continuation After hook identity drifted".to_owned());
        }
        let protocol_matches_plan = matches!(
            (
                &request.runtime_protocol,
                &request.protocol_plan.preparation
            ),
            (
                CompanionContinuationRuntimeProtocol::FullFork,
                super::CompanionRuntimePreparation::ForkParentHistory { .. }
            ) | (
                CompanionContinuationRuntimeProtocol::FreshCreate,
                super::CompanionRuntimePreparation::FreshCreate { .. }
            )
        );
        if !protocol_matches_plan
            || request.protocol_plan.first_submit_input.text != request.first_input_text
        {
            return Err("Companion continuation protocol plan is inconsistent".to_owned());
        }
        Ok(Self {
            request,
            phase: CompanionContinuationPhase::Requested,
            evidence: CompanionContinuationEvidence::default(),
        })
    }

    pub fn request(&self) -> &CompanionContinuationRequest {
        &self.request
    }

    pub fn phase(&self) -> CompanionContinuationPhase {
        self.phase
    }

    pub fn evidence(&self) -> &CompanionContinuationEvidence {
        &self.evidence
    }

    fn effect_identity(
        &self,
        phase: CompanionContinuationPhase,
    ) -> CompanionContinuationEffectIdentity {
        let phase = phase_slug(phase);
        CompanionContinuationEffectIdentity {
            effect_id: stable_uuid(self.request.request_id, phase),
            idempotency_key: format!("companion-continuation:{}:{phase}", self.request.request_id),
        }
    }

    fn record_runtime(&mut self, evidence: CompanionRuntimeReadyEvidence) -> Result<(), String> {
        if self.phase != CompanionContinuationPhase::Requested
            || evidence.child_run_id != self.request.child_run_id
            || evidence.child_agent_id != self.request.child_agent_id
            || self
                .request
                .child_frame_id
                .is_some_and(|frame_id| evidence.child_frame_id != frame_id)
            || evidence.child_runtime_thread_id != self.request.child_runtime_thread_id
        {
            return Err("Companion Runtime-ready evidence does not match the request".to_owned());
        }
        self.evidence.runtime = Some(evidence);
        self.phase = CompanionContinuationPhase::RuntimeReady;
        Ok(())
    }

    fn record_prepared_first_input(
        &mut self,
        evidence: CompanionPreparedFirstInputEvidence,
    ) -> Result<(), String> {
        if self.phase != CompanionContinuationPhase::RuntimeReady {
            return Err("Companion first input preparation is out of order".to_owned());
        }
        self.evidence.prepared_first_input = Some(evidence);
        self.phase = CompanionContinuationPhase::FirstInputPrepared;
        Ok(())
    }

    fn record_first_input(&mut self, evidence: CompanionFirstInputEvidence) -> Result<(), String> {
        if self.phase != CompanionContinuationPhase::FirstInputPrepared {
            return Err("Companion first input is out of order".to_owned());
        }
        self.evidence.first_input = Some(evidence);
        self.phase = CompanionContinuationPhase::FirstInputConverged;
        Ok(())
    }

    fn record_gate(&mut self, evidence: CompanionGateEvidence) -> Result<(), String> {
        if self.phase != CompanionContinuationPhase::FirstInputConverged
            || self.request.wait != evidence.gate_id.is_some()
        {
            return Err("Companion gate evidence does not match the request".to_owned());
        }
        self.evidence.gate = Some(evidence);
        self.phase = CompanionContinuationPhase::GateConverged;
        Ok(())
    }

    fn record_channel(&mut self, evidence: CompanionChannelEvidence) -> Result<(), String> {
        let input_handoff_id = self
            .evidence
            .first_input
            .as_ref()
            .map(|evidence| evidence.input_handoff_id);
        if self.phase != CompanionContinuationPhase::GateConverged
            || input_handoff_id != Some(evidence.input_handoff_id)
        {
            return Err("Companion channel evidence does not match the input".to_owned());
        }
        self.evidence.channel = Some(evidence);
        self.phase = CompanionContinuationPhase::ChannelConverged;
        Ok(())
    }

    fn record_task(&mut self, evidence: CompanionTaskEvidence) -> Result<(), String> {
        if self.phase != CompanionContinuationPhase::ChannelConverged
            || evidence.task_id != self.request.task_id
            || evidence.assigned_agent_id
                != self.request.task_id.map(|_| self.request.child_agent_id)
        {
            return Err("Companion task evidence does not match the request".to_owned());
        }
        self.evidence.task = Some(evidence);
        self.phase = CompanionContinuationPhase::TaskConverged;
        Ok(())
    }

    fn record_after_dispatch_hook(
        &mut self,
        evidence: CompanionAfterDispatchHookEvidence,
    ) -> Result<(), String> {
        let runtime = self
            .evidence
            .runtime
            .as_ref()
            .ok_or_else(|| "Companion Runtime-ready evidence is missing".to_owned())?;
        if self.phase != CompanionContinuationPhase::TaskConverged
            || evidence.effect != self.request.after_dispatch_hook_effect
            || evidence.parent_frame_id != self.request.parent_frame_id
            || evidence.child_frame_id != runtime.child_frame_id
            || evidence.child_runtime_thread_id != runtime.child_runtime_thread_id
        {
            return Err("Companion After hook evidence does not match the request".to_owned());
        }
        self.evidence.after_dispatch_hook = Some(evidence);
        self.phase = CompanionContinuationPhase::AfterDispatchHookConverged;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompanionRuntimeReadiness {
    Pending,
    Ready(CompanionRuntimeReadyEvidence),
    Failed(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompanionEffectProgress<T> {
    Pending,
    Applied(T),
}

#[async_trait]
pub trait CompanionContinuationEffectPort: Send + Sync {
    async fn converge_runtime(
        &self,
        continuation: &CompanionContinuation,
    ) -> Result<CompanionRuntimeReadiness, String>;
    async fn prepare_first_input(
        &self,
        continuation: &CompanionContinuation,
        identity: &CompanionContinuationEffectIdentity,
    ) -> Result<CompanionEffectProgress<CompanionPreparedFirstInputEvidence>, String>;
    async fn converge_first_input(
        &self,
        continuation: &CompanionContinuation,
        identity: &CompanionContinuationEffectIdentity,
    ) -> Result<CompanionEffectProgress<CompanionFirstInputEvidence>, String>;
    async fn converge_gate(
        &self,
        continuation: &CompanionContinuation,
        identity: &CompanionContinuationEffectIdentity,
    ) -> Result<CompanionGateEvidence, String>;
    async fn converge_channel(
        &self,
        continuation: &CompanionContinuation,
        identity: &CompanionContinuationEffectIdentity,
    ) -> Result<CompanionChannelEvidence, String>;
    async fn converge_task(
        &self,
        continuation: &CompanionContinuation,
        identity: &CompanionContinuationEffectIdentity,
    ) -> Result<CompanionTaskEvidence, String>;
    async fn converge_after_dispatch_hook(
        &self,
        continuation: &CompanionContinuation,
        identity: &CompanionContinuationEffectIdentity,
    ) -> Result<CompanionEffectProgress<CompanionAfterDispatchHookEvidence>, String>;
}

/// Runs the complete Product orchestration inside the current tool invocation.
///
/// Every downstream effect uses a stable identity and remains recoverable from its actual owner.
/// `Pending` stops the current handoff; no Product queue or background promise is created.
pub async fn run_companion_continuation(
    request: CompanionContinuationRequest,
    effects: &dyn CompanionContinuationEffectPort,
) -> Result<CompanionContinuation, String> {
    let mut continuation = CompanionContinuation::requested(request)?;
    for _ in 0..8 {
        let progressed = match continuation.phase {
            CompanionContinuationPhase::Requested => {
                match effects.converge_runtime(&continuation).await? {
                    CompanionRuntimeReadiness::Pending => false,
                    CompanionRuntimeReadiness::Ready(evidence) => {
                        continuation.record_runtime(evidence)?;
                        true
                    }
                    CompanionRuntimeReadiness::Failed(reason) => return Err(reason),
                }
            }
            CompanionContinuationPhase::RuntimeReady => {
                let identity =
                    continuation.effect_identity(CompanionContinuationPhase::FirstInputPrepared);
                match effects
                    .prepare_first_input(&continuation, &identity)
                    .await?
                {
                    CompanionEffectProgress::Pending => false,
                    CompanionEffectProgress::Applied(evidence) => {
                        continuation.record_prepared_first_input(evidence)?;
                        true
                    }
                }
            }
            CompanionContinuationPhase::FirstInputPrepared => {
                let identity =
                    continuation.effect_identity(CompanionContinuationPhase::FirstInputConverged);
                match effects
                    .converge_first_input(&continuation, &identity)
                    .await?
                {
                    CompanionEffectProgress::Pending => false,
                    CompanionEffectProgress::Applied(evidence) => {
                        continuation.record_first_input(evidence)?;
                        true
                    }
                }
            }
            CompanionContinuationPhase::FirstInputConverged => {
                let identity =
                    continuation.effect_identity(CompanionContinuationPhase::GateConverged);
                let evidence = effects.converge_gate(&continuation, &identity).await?;
                continuation.record_gate(evidence)?;
                true
            }
            CompanionContinuationPhase::GateConverged => {
                let identity =
                    continuation.effect_identity(CompanionContinuationPhase::ChannelConverged);
                let evidence = effects.converge_channel(&continuation, &identity).await?;
                continuation.record_channel(evidence)?;
                true
            }
            CompanionContinuationPhase::ChannelConverged => {
                let identity =
                    continuation.effect_identity(CompanionContinuationPhase::TaskConverged);
                let evidence = effects.converge_task(&continuation, &identity).await?;
                continuation.record_task(evidence)?;
                true
            }
            CompanionContinuationPhase::TaskConverged => {
                let identity = continuation.request.after_dispatch_hook_effect.clone();
                match effects
                    .converge_after_dispatch_hook(&continuation, &identity)
                    .await?
                {
                    CompanionEffectProgress::Pending => false,
                    CompanionEffectProgress::Applied(evidence) => {
                        continuation.record_after_dispatch_hook(evidence)?;
                        true
                    }
                }
            }
            CompanionContinuationPhase::AfterDispatchHookConverged => {
                continuation.phase = CompanionContinuationPhase::Succeeded;
                true
            }
            CompanionContinuationPhase::Succeeded => return Ok(continuation),
        };
        if !progressed {
            return Ok(continuation);
        }
    }
    Ok(continuation)
}

pub fn companion_after_dispatch_hook_effect_identity(
    request_id: Uuid,
) -> CompanionContinuationEffectIdentity {
    let phase = phase_slug(CompanionContinuationPhase::AfterDispatchHookConverged);
    CompanionContinuationEffectIdentity {
        effect_id: stable_uuid(request_id, phase),
        idempotency_key: format!("companion-continuation:{request_id}:{phase}"),
    }
}

fn stable_uuid(request_id: Uuid, phase: &str) -> Uuid {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(
        format!("agentdash.companion-continuation/v1:{request_id}:{phase}").as_bytes(),
    );
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    bytes[6] = (bytes[6] & 0x0f) | 0x50;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    Uuid::from_bytes(bytes)
}

fn phase_slug(phase: CompanionContinuationPhase) -> &'static str {
    match phase {
        CompanionContinuationPhase::Requested => "requested",
        CompanionContinuationPhase::RuntimeReady => "runtime-ready",
        CompanionContinuationPhase::FirstInputPrepared => "first-input-prepared",
        CompanionContinuationPhase::FirstInputConverged => "first-input-converged",
        CompanionContinuationPhase::GateConverged => "gate-converged",
        CompanionContinuationPhase::ChannelConverged => "channel-converged",
        CompanionContinuationPhase::TaskConverged => "task-converged",
        CompanionContinuationPhase::AfterDispatchHookConverged => "after-dispatch-hook-converged",
        CompanionContinuationPhase::Succeeded => "succeeded",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn after_dispatch_hook_identity_is_stable_and_phase_scoped() {
        let request_id = Uuid::new_v4();
        let first = companion_after_dispatch_hook_effect_identity(request_id);
        let second = companion_after_dispatch_hook_effect_identity(request_id);

        assert_eq!(first, second);
        assert!(
            first
                .idempotency_key
                .contains("after-dispatch-hook-converged")
        );
        assert_ne!(
            first.effect_id,
            stable_uuid(
                request_id,
                phase_slug(CompanionContinuationPhase::TaskConverged)
            )
        );
    }
}
