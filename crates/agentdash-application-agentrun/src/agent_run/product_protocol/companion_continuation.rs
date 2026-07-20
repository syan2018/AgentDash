use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;
use uuid::Uuid;

use agentdash_agent_runtime_contract::RuntimeThreadId;
use agentdash_platform_spi::{AgentConfig, HookResolution};

use super::CompanionDispatchTargetPlan;
use crate::agent_run::PreparedAgentRunProductInputDelivery;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompanionContinuationRuntimeProtocol {
    FullFork,
    FreshCreate,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompanionContinuationEffectIdentity {
    pub effect_id: Uuid,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompanionRuntimeReadyEvidence {
    pub child_run_id: Uuid,
    pub child_agent_id: Uuid,
    pub child_frame_id: Uuid,
    pub child_runtime_thread_id: RuntimeThreadId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompanionFirstInputEvidence {
    pub mailbox_message_id: Uuid,
    pub runtime_operation_id: Option<String>,
    pub submitted_by_runtime_protocol: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CompanionPreparedFirstInputEvidence {
    ProductDelivery {
        envelope: PreparedAgentRunProductInputDelivery,
    },
    FreshRuntimeProtocol,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompanionGateEvidence {
    pub gate_id: Option<Uuid>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompanionChannelEvidence {
    pub channel_id: Uuid,
    pub delivery_id: Uuid,
    pub mailbox_message_id: Uuid,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompanionTaskEvidence {
    pub task_id: Option<Uuid>,
    pub assigned_agent_id: Option<Uuid>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompanionAfterDispatchHookEvidence {
    pub effect: CompanionContinuationEffectIdentity,
    pub parent_frame_id: Uuid,
    pub child_frame_id: Uuid,
    pub child_runtime_thread_id: RuntimeThreadId,
    pub resolution: HookResolution,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct CompanionContinuationEvidence {
    pub runtime: Option<CompanionRuntimeReadyEvidence>,
    pub prepared_first_input: Option<CompanionPreparedFirstInputEvidence>,
    pub first_input: Option<CompanionFirstInputEvidence>,
    pub gate: Option<CompanionGateEvidence>,
    pub channel: Option<CompanionChannelEvidence>,
    pub task: Option<CompanionTaskEvidence>,
    pub after_dispatch_hook: Option<CompanionAfterDispatchHookEvidence>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompanionContinuationFailure {
    pub phase: CompanionContinuationPhase,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompanionContinuationSaga {
    request: CompanionContinuationRequest,
    phase: CompanionContinuationPhase,
    version: u64,
    evidence: CompanionContinuationEvidence,
    failure: Option<CompanionContinuationFailure>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompanionContinuationStep {
    InspectRuntime,
    PrepareFirstInput,
    ConvergeFirstInput,
    ConvergeGate,
    ConvergeChannel,
    ConvergeTask,
    ConvergeAfterDispatchHook,
    MarkSucceeded,
    Terminal,
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

impl CompanionContinuationSaga {
    pub fn requested(request: CompanionContinuationRequest) -> Result<Self, String> {
        if request.dispatch_id.trim().is_empty() {
            return Err("Companion continuation dispatch identity is empty".to_owned());
        }
        if request.first_input_text.trim().is_empty()
            || request.first_input_source.namespace.trim().is_empty()
            || request.first_input_source.kind.trim().is_empty()
            || request.first_input_source.actor.trim().is_empty()
            || request.parent_turn_id.trim().is_empty()
            || request.slice_mode.trim().is_empty()
        {
            return Err("Companion continuation first input evidence is incomplete".to_owned());
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
        if !protocol_matches_plan {
            return Err(
                "Companion continuation Runtime protocol and preparation plan drifted".to_owned(),
            );
        }
        if request.protocol_plan.first_submit_input.text != request.first_input_text {
            return Err(
                "Companion continuation first input and durable protocol plan drifted".to_owned(),
            );
        }
        Ok(Self {
            request,
            phase: CompanionContinuationPhase::Requested,
            version: 0,
            evidence: CompanionContinuationEvidence::default(),
            failure: None,
        })
    }

    pub fn request(&self) -> &CompanionContinuationRequest {
        &self.request
    }

    pub fn phase(&self) -> CompanionContinuationPhase {
        self.phase
    }

    pub fn version(&self) -> u64 {
        self.version
    }

    pub fn evidence(&self) -> &CompanionContinuationEvidence {
        &self.evidence
    }

    pub fn failure(&self) -> Option<&CompanionContinuationFailure> {
        self.failure.as_ref()
    }

    pub fn advance_persisted_version(mut self, expected_version: u64) -> Result<Self, String> {
        if self.version != expected_version {
            return Err("Companion continuation persisted version drifted".to_owned());
        }
        self.version = self
            .version
            .checked_add(1)
            .ok_or_else(|| "Companion continuation version overflow".to_owned())?;
        Ok(self)
    }

    pub fn next_step(&self) -> CompanionContinuationStep {
        if self.failure.is_some() || self.phase == CompanionContinuationPhase::Succeeded {
            return CompanionContinuationStep::Terminal;
        }
        match self.phase {
            CompanionContinuationPhase::Requested => CompanionContinuationStep::InspectRuntime,
            CompanionContinuationPhase::RuntimeReady => {
                CompanionContinuationStep::PrepareFirstInput
            }
            CompanionContinuationPhase::FirstInputPrepared => {
                CompanionContinuationStep::ConvergeFirstInput
            }
            CompanionContinuationPhase::FirstInputConverged => {
                CompanionContinuationStep::ConvergeGate
            }
            CompanionContinuationPhase::GateConverged => CompanionContinuationStep::ConvergeChannel,
            CompanionContinuationPhase::ChannelConverged => CompanionContinuationStep::ConvergeTask,
            CompanionContinuationPhase::TaskConverged => {
                CompanionContinuationStep::ConvergeAfterDispatchHook
            }
            CompanionContinuationPhase::AfterDispatchHookConverged => {
                CompanionContinuationStep::MarkSucceeded
            }
            CompanionContinuationPhase::Succeeded => CompanionContinuationStep::Terminal,
        }
    }

    pub fn effect_identity(
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
            return Err("Companion Runtime-ready evidence drifted".to_owned());
        }
        self.evidence.runtime = Some(evidence);
        self.phase = CompanionContinuationPhase::RuntimeReady;
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

    fn record_gate(&mut self, evidence: CompanionGateEvidence) -> Result<(), String> {
        if self.phase != CompanionContinuationPhase::FirstInputConverged
            || self.request.wait != evidence.gate_id.is_some()
        {
            return Err("Companion gate evidence drifted".to_owned());
        }
        self.evidence.gate = Some(evidence);
        self.phase = CompanionContinuationPhase::GateConverged;
        Ok(())
    }

    fn record_channel(&mut self, evidence: CompanionChannelEvidence) -> Result<(), String> {
        if self.phase != CompanionContinuationPhase::GateConverged
            || evidence.mailbox_message_id
                != self
                    .evidence
                    .first_input
                    .as_ref()
                    .ok_or_else(|| "Companion first input evidence is missing".to_owned())?
                    .mailbox_message_id
        {
            return Err("Companion channel evidence drifted".to_owned());
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
            return Err("Companion Task evidence drifted".to_owned());
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
            return Err("Companion After hook evidence drifted".to_owned());
        }
        self.evidence.after_dispatch_hook = Some(evidence);
        self.phase = CompanionContinuationPhase::AfterDispatchHookConverged;
        Ok(())
    }

    fn mark_succeeded(&mut self) -> Result<(), String> {
        if self.phase != CompanionContinuationPhase::AfterDispatchHookConverged {
            return Err("Companion continuation success is out of order".to_owned());
        }
        self.phase = CompanionContinuationPhase::Succeeded;
        Ok(())
    }

    fn mark_failed(&mut self, reason: String) {
        self.failure = Some(CompanionContinuationFailure {
            phase: self.phase,
            reason,
        });
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CompanionContinuationRepositoryError {
    #[error("Companion continuation saga already exists")]
    AlreadyExists,
    #[error("Companion continuation saga was not found")]
    NotFound,
    #[error("Companion continuation saga CAS conflict")]
    Conflict,
    #[error("Companion continuation persistence is unavailable: {0}")]
    Unavailable(String),
}

#[async_trait]
pub trait CompanionContinuationSagaRepository: Send + Sync {
    async fn create(
        &self,
        saga: CompanionContinuationSaga,
    ) -> Result<CompanionContinuationSaga, CompanionContinuationRepositoryError>;
    async fn load(
        &self,
        request_id: Uuid,
    ) -> Result<Option<CompanionContinuationSaga>, CompanionContinuationRepositoryError>;
    async fn list_recoverable(
        &self,
        limit: usize,
    ) -> Result<Vec<Uuid>, CompanionContinuationRepositoryError>;
    async fn save(
        &self,
        expected_version: u64,
        saga: CompanionContinuationSaga,
    ) -> Result<CompanionContinuationSaga, CompanionContinuationRepositoryError>;
}

#[async_trait]
pub trait CompanionContinuationEffectPort: Send + Sync {
    async fn converge_runtime(
        &self,
        saga: &CompanionContinuationSaga,
    ) -> Result<CompanionRuntimeReadiness, String>;
    async fn converge_first_input(
        &self,
        saga: &CompanionContinuationSaga,
        identity: &CompanionContinuationEffectIdentity,
    ) -> Result<CompanionEffectProgress<CompanionFirstInputEvidence>, String>;
    async fn prepare_first_input(
        &self,
        saga: &CompanionContinuationSaga,
        identity: &CompanionContinuationEffectIdentity,
    ) -> Result<CompanionEffectProgress<CompanionPreparedFirstInputEvidence>, String>;
    async fn converge_gate(
        &self,
        saga: &CompanionContinuationSaga,
        identity: &CompanionContinuationEffectIdentity,
    ) -> Result<CompanionGateEvidence, String>;
    async fn converge_channel(
        &self,
        saga: &CompanionContinuationSaga,
        identity: &CompanionContinuationEffectIdentity,
    ) -> Result<CompanionChannelEvidence, String>;
    async fn converge_task(
        &self,
        saga: &CompanionContinuationSaga,
        identity: &CompanionContinuationEffectIdentity,
    ) -> Result<CompanionTaskEvidence, String>;
    async fn converge_after_dispatch_hook(
        &self,
        saga: &CompanionContinuationSaga,
        identity: &CompanionContinuationEffectIdentity,
    ) -> Result<CompanionEffectProgress<CompanionAfterDispatchHookEvidence>, String>;
}

pub struct CompanionContinuationWorker<'a> {
    repository: &'a dyn CompanionContinuationSagaRepository,
    effects: &'a dyn CompanionContinuationEffectPort,
}

impl<'a> CompanionContinuationWorker<'a> {
    pub fn new(
        repository: &'a dyn CompanionContinuationSagaRepository,
        effects: &'a dyn CompanionContinuationEffectPort,
    ) -> Self {
        Self {
            repository,
            effects,
        }
    }

    pub async fn advance(
        &self,
        request_id: Uuid,
    ) -> Result<CompanionContinuationSaga, CompanionContinuationRepositoryError> {
        let mut saga = self
            .repository
            .load(request_id)
            .await?
            .ok_or(CompanionContinuationRepositoryError::NotFound)?;
        let expected_version = saga.version;
        let effect_result = match saga.next_step() {
            CompanionContinuationStep::InspectRuntime => {
                match self.effects.converge_runtime(&saga).await {
                    Ok(CompanionRuntimeReadiness::Pending) => return Ok(saga),
                    Ok(CompanionRuntimeReadiness::Ready(evidence)) => saga.record_runtime(evidence),
                    Ok(CompanionRuntimeReadiness::Failed(reason)) => {
                        saga.mark_failed(reason);
                        Ok(())
                    }
                    Err(reason) => Err(reason),
                }
            }
            CompanionContinuationStep::ConvergeFirstInput => {
                let identity =
                    saga.effect_identity(CompanionContinuationPhase::FirstInputConverged);
                match self.effects.converge_first_input(&saga, &identity).await {
                    Ok(CompanionEffectProgress::Pending) => return Ok(saga),
                    Ok(CompanionEffectProgress::Applied(evidence)) => {
                        saga.record_first_input(evidence)
                    }
                    Err(reason) => Err(reason),
                }
            }
            CompanionContinuationStep::PrepareFirstInput => {
                let identity = saga.effect_identity(CompanionContinuationPhase::FirstInputPrepared);
                match self.effects.prepare_first_input(&saga, &identity).await {
                    Ok(CompanionEffectProgress::Pending) => return Ok(saga),
                    Ok(CompanionEffectProgress::Applied(evidence)) => {
                        saga.record_prepared_first_input(evidence)
                    }
                    Err(reason) => Err(reason),
                }
            }
            CompanionContinuationStep::ConvergeGate => {
                let identity = saga.effect_identity(CompanionContinuationPhase::GateConverged);
                self.effects
                    .converge_gate(&saga, &identity)
                    .await
                    .and_then(|evidence| saga.record_gate(evidence))
            }
            CompanionContinuationStep::ConvergeChannel => {
                let identity = saga.effect_identity(CompanionContinuationPhase::ChannelConverged);
                self.effects
                    .converge_channel(&saga, &identity)
                    .await
                    .and_then(|evidence| saga.record_channel(evidence))
            }
            CompanionContinuationStep::ConvergeTask => {
                let identity = saga.effect_identity(CompanionContinuationPhase::TaskConverged);
                self.effects
                    .converge_task(&saga, &identity)
                    .await
                    .and_then(|evidence| saga.record_task(evidence))
            }
            CompanionContinuationStep::ConvergeAfterDispatchHook => {
                let identity = saga.request().after_dispatch_hook_effect.clone();
                match self
                    .effects
                    .converge_after_dispatch_hook(&saga, &identity)
                    .await
                {
                    Ok(CompanionEffectProgress::Pending) => return Ok(saga),
                    Ok(CompanionEffectProgress::Applied(evidence)) => {
                        saga.record_after_dispatch_hook(evidence)
                    }
                    Err(reason) => Err(reason),
                }
            }
            CompanionContinuationStep::MarkSucceeded => saga.mark_succeeded(),
            CompanionContinuationStep::Terminal => return Ok(saga),
        };
        effect_result.map_err(CompanionContinuationRepositoryError::Unavailable)?;
        self.repository.save(expected_version, saga).await
    }
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
    use std::collections::{BTreeMap, BTreeSet};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};

    use super::*;

    #[derive(Default)]
    struct RecordingCompanionContinuationRepository {
        saga: Mutex<Option<CompanionContinuationSaga>>,
        fail_next_save: AtomicBool,
    }

    impl RecordingCompanionContinuationRepository {
        fn fail_next_save(&self) {
            self.fail_next_save.store(true, Ordering::SeqCst);
        }
    }

    #[async_trait]
    impl CompanionContinuationSagaRepository for RecordingCompanionContinuationRepository {
        async fn create(
            &self,
            saga: CompanionContinuationSaga,
        ) -> Result<CompanionContinuationSaga, CompanionContinuationRepositoryError> {
            let mut stored = self.saga.lock().expect("repository lock");
            if stored.is_some() {
                return Err(CompanionContinuationRepositoryError::AlreadyExists);
            }
            *stored = Some(saga.clone());
            Ok(saga)
        }

        async fn load(
            &self,
            request_id: Uuid,
        ) -> Result<Option<CompanionContinuationSaga>, CompanionContinuationRepositoryError>
        {
            Ok(self
                .saga
                .lock()
                .expect("repository lock")
                .clone()
                .filter(|saga| saga.request().request_id == request_id))
        }

        async fn list_recoverable(
            &self,
            limit: usize,
        ) -> Result<Vec<Uuid>, CompanionContinuationRepositoryError> {
            Ok(self
                .saga
                .lock()
                .expect("repository lock")
                .as_ref()
                .filter(|saga| {
                    saga.phase() != CompanionContinuationPhase::Succeeded
                        && saga.failure().is_none()
                })
                .map(|saga| saga.request().request_id)
                .into_iter()
                .take(limit)
                .collect())
        }

        async fn save(
            &self,
            expected_version: u64,
            saga: CompanionContinuationSaga,
        ) -> Result<CompanionContinuationSaga, CompanionContinuationRepositoryError> {
            if self.fail_next_save.swap(false, Ordering::SeqCst) {
                return Err(CompanionContinuationRepositoryError::Unavailable(
                    "injected post-effect crash".to_owned(),
                ));
            }
            let mut stored = self.saga.lock().expect("repository lock");
            let current = stored
                .as_ref()
                .ok_or(CompanionContinuationRepositoryError::NotFound)?;
            if current.version() != expected_version {
                return Err(CompanionContinuationRepositoryError::Conflict);
            }
            let persisted = saga
                .advance_persisted_version(expected_version)
                .map_err(CompanionContinuationRepositoryError::Unavailable)?;
            *stored = Some(persisted.clone());
            Ok(persisted)
        }
    }

    #[derive(Default)]
    struct RecordingEffects {
        invocations: Mutex<BTreeMap<&'static str, Vec<Uuid>>>,
        logical_effects: Mutex<BTreeSet<Uuid>>,
        after_hook_pending_once: AtomicBool,
    }

    impl RecordingEffects {
        fn pend_after_hook_once(&self) {
            self.after_hook_pending_once.store(true, Ordering::SeqCst);
        }

        fn record(&self, phase: &'static str, identity: &CompanionContinuationEffectIdentity) {
            self.invocations
                .lock()
                .expect("invocation lock")
                .entry(phase)
                .or_default()
                .push(identity.effect_id);
            self.logical_effects
                .lock()
                .expect("logical effect lock")
                .insert(identity.effect_id);
        }

        fn phase_invocations(&self, phase: &'static str) -> Vec<Uuid> {
            self.invocations
                .lock()
                .expect("invocation lock")
                .get(phase)
                .cloned()
                .unwrap_or_default()
        }
    }

    #[async_trait]
    impl CompanionContinuationEffectPort for RecordingEffects {
        async fn converge_runtime(
            &self,
            saga: &CompanionContinuationSaga,
        ) -> Result<CompanionRuntimeReadiness, String> {
            let request = saga.request();
            Ok(CompanionRuntimeReadiness::Ready(
                CompanionRuntimeReadyEvidence {
                    child_run_id: request.child_run_id,
                    child_agent_id: request.child_agent_id,
                    child_frame_id: request.child_frame_id.unwrap_or_else(Uuid::new_v4),
                    child_runtime_thread_id: request.child_runtime_thread_id.clone(),
                },
            ))
        }

        async fn converge_first_input(
            &self,
            saga: &CompanionContinuationSaga,
            identity: &CompanionContinuationEffectIdentity,
        ) -> Result<CompanionEffectProgress<CompanionFirstInputEvidence>, String> {
            self.record("first_input", identity);
            Ok(CompanionEffectProgress::Applied(
                CompanionFirstInputEvidence {
                    mailbox_message_id: stable_uuid(saga.request().request_id, "mailbox"),
                    runtime_operation_id: Some("runtime-operation".to_owned()),
                    submitted_by_runtime_protocol: matches!(
                        saga.request().runtime_protocol,
                        CompanionContinuationRuntimeProtocol::FreshCreate
                    ),
                },
            ))
        }

        async fn prepare_first_input(
            &self,
            saga: &CompanionContinuationSaga,
            identity: &CompanionContinuationEffectIdentity,
        ) -> Result<CompanionEffectProgress<CompanionPreparedFirstInputEvidence>, String> {
            self.record("prepare_first_input", identity);
            Ok(CompanionEffectProgress::Applied(
                match saga.request().runtime_protocol {
                    CompanionContinuationRuntimeProtocol::FullFork => {
                        CompanionPreparedFirstInputEvidence::ProductDelivery {
                            envelope: PreparedAgentRunProductInputDelivery {
                                mailbox_message_id: stable_uuid(
                                    saga.request().request_id,
                                    "mailbox",
                                ),
                                command_request:
                                    crate::agent_run::AgentRunProductCommandRequest {
                                        target: agentdash_domain::agent_run_target::AgentRunTarget {
                                            run_id: saga.request().child_run_id,
                                            agent_id: saga.request().child_agent_id,
                                        },
                                        client_command_id: identity.idempotency_key.clone(),
                                        command:
                                            crate::agent_run::AgentRunProductCommand::SubmitInput {
                                                content: vec![
                                                    agentdash_agent_runtime_contract::ManagedRuntimeContentBlock::Text {
                                                        text: saga
                                                            .request()
                                                            .first_input_text
                                                            .clone(),
                                                    },
                                                ],
                                            },
                                    },
                                steered: false,
                            },
                        }
                    }
                    CompanionContinuationRuntimeProtocol::FreshCreate => {
                        CompanionPreparedFirstInputEvidence::FreshRuntimeProtocol
                    }
                },
            ))
        }

        async fn converge_gate(
            &self,
            saga: &CompanionContinuationSaga,
            identity: &CompanionContinuationEffectIdentity,
        ) -> Result<CompanionGateEvidence, String> {
            self.record("gate", identity);
            Ok(CompanionGateEvidence {
                gate_id: saga
                    .request()
                    .wait
                    .then(|| stable_uuid(saga.request().request_id, "gate")),
            })
        }

        async fn converge_channel(
            &self,
            saga: &CompanionContinuationSaga,
            identity: &CompanionContinuationEffectIdentity,
        ) -> Result<CompanionChannelEvidence, String> {
            self.record("channel", identity);
            Ok(CompanionChannelEvidence {
                channel_id: stable_uuid(saga.request().request_id, "channel"),
                delivery_id: identity.effect_id,
                mailbox_message_id: saga
                    .evidence()
                    .first_input
                    .as_ref()
                    .expect("first input evidence")
                    .mailbox_message_id,
            })
        }

        async fn converge_task(
            &self,
            saga: &CompanionContinuationSaga,
            identity: &CompanionContinuationEffectIdentity,
        ) -> Result<CompanionTaskEvidence, String> {
            self.record("task", identity);
            Ok(CompanionTaskEvidence {
                task_id: saga.request().task_id,
                assigned_agent_id: saga
                    .request()
                    .task_id
                    .map(|_| saga.request().child_agent_id),
            })
        }

        async fn converge_after_dispatch_hook(
            &self,
            saga: &CompanionContinuationSaga,
            identity: &CompanionContinuationEffectIdentity,
        ) -> Result<CompanionEffectProgress<CompanionAfterDispatchHookEvidence>, String> {
            self.record("after_dispatch_hook", identity);
            if self.after_hook_pending_once.swap(false, Ordering::SeqCst) {
                return Ok(CompanionEffectProgress::Pending);
            }
            let runtime = saga.evidence().runtime.as_ref().expect("runtime evidence");
            Ok(CompanionEffectProgress::Applied(
                CompanionAfterDispatchHookEvidence {
                    effect: identity.clone(),
                    parent_frame_id: saga.request().parent_frame_id,
                    child_frame_id: runtime.child_frame_id,
                    child_runtime_thread_id: runtime.child_runtime_thread_id.clone(),
                    resolution: HookResolution::default(),
                },
            ))
        }
    }

    fn request(protocol: CompanionContinuationRuntimeProtocol) -> CompanionContinuationRequest {
        let request_id = Uuid::new_v4();
        let parent_runtime_thread_id =
            RuntimeThreadId::new(format!("parent-thread-{request_id}")).expect("parent thread");
        let protocol_plan = match protocol {
            CompanionContinuationRuntimeProtocol::FullFork => CompanionDispatchTargetPlan {
                preparation: crate::agent_run::CompanionRuntimePreparation::ForkParentHistory {
                    parent_runtime_thread_id: parent_runtime_thread_id.clone(),
                    through_turn_id: agentdash_agent_runtime_contract::RuntimeTurnId::new("turn-1")
                        .expect("turn"),
                },
                context_application_requirement: None,
                adoption_mode: crate::agent_run::CompanionAdoptionMode::Suggestion,
                first_submit_input: crate::agent_run::SubmitInput {
                    text: "review this".to_owned(),
                },
                surface_facts: serde_json::json!({}),
            },
            CompanionContinuationRuntimeProtocol::FreshCreate => {
                crate::agent_run::compile_companion_dispatch_target(
                    crate::agent_run::CompanionContextMode::Compact,
                    crate::agent_run::CompanionAdoptionMode::Suggestion,
                    crate::agent_run::SubmitInput {
                        text: "review this".to_owned(),
                    },
                    crate::agent_run::CompanionContextSources {
                        parent_runtime_thread_id: parent_runtime_thread_id.clone(),
                        through_turn_id: None,
                        package_id: Uuid::new_v4(),
                        compact_summary: Some((
                            "summary".to_owned(),
                            crate::agent_run::CompanionContextSourceDraft {
                                authority: crate::agent_run::CompiledContextAuthority::AgentHistory,
                                source_coordinate: "parent".to_owned(),
                                source_revision: "rev-1".to_owned(),
                                source_digest: "sha256:source".to_owned(),
                            },
                        )),
                        workflow: None,
                        constraints: None,
                        surface_facts: serde_json::json!({}),
                    },
                )
                .expect("fresh plan")
            }
        };
        CompanionContinuationRequest {
            request_id,
            dispatch_id: format!("dispatch-{request_id}"),
            runtime_protocol: protocol,
            runtime_protocol_request_id: Uuid::new_v4(),
            project_id: Uuid::new_v4(),
            parent_run_id: Uuid::new_v4(),
            parent_agent_id: Uuid::new_v4(),
            parent_frame_id: Uuid::new_v4(),
            child_run_id: Uuid::new_v4(),
            child_agent_id: Uuid::new_v4(),
            child_frame_id: matches!(protocol, CompanionContinuationRuntimeProtocol::FreshCreate)
                .then(Uuid::new_v4),
            child_runtime_thread_id: RuntimeThreadId::new(format!("thread-{request_id}"))
                .expect("runtime thread"),
            selected_project_agent_id: Uuid::new_v4(),
            selected_agent_key: "reviewer".to_owned(),
            companion_executor_config: AgentConfig::default(),
            parent_runtime_thread_id: parent_runtime_thread_id.to_string(),
            parent_turn_id: "parent-turn-1".to_owned(),
            protocol_plan,
            companion_label: "reviewer".to_owned(),
            slice_mode: match protocol {
                CompanionContinuationRuntimeProtocol::FullFork => "full",
                CompanionContinuationRuntimeProtocol::FreshCreate => "compact",
            }
            .to_owned(),
            adoption_mode: "suggestion".to_owned(),
            wait: true,
            task_id: Some(Uuid::new_v4()),
            first_input_text: "review this".to_owned(),
            first_input_source: CompanionContinuationInputSource {
                namespace: "companion".to_owned(),
                kind: "dispatch".to_owned(),
                source_ref: Some(request_id.to_string()),
                correlation_ref: Some(request_id.to_string()),
                actor: "agent".to_owned(),
                route: Some("sub".to_owned()),
                display_label_key: "mailbox.source.companion.dispatch".to_owned(),
                metadata: None,
            },
            after_dispatch_hook_effect: companion_after_dispatch_hook_effect_identity(request_id),
        }
    }

    async fn drive_to_terminal(
        repository: &RecordingCompanionContinuationRepository,
        effects: &RecordingEffects,
        request_id: Uuid,
    ) -> CompanionContinuationSaga {
        let worker = CompanionContinuationWorker::new(repository, effects);
        for _ in 0..12 {
            let saga = worker.advance(request_id).await.expect("advance");
            if saga.phase() == CompanionContinuationPhase::Succeeded {
                return saga;
            }
        }
        panic!("continuation did not converge");
    }

    #[tokio::test]
    async fn full_first_input_replay_after_save_crash_has_one_logical_effect() {
        let repository = Arc::new(RecordingCompanionContinuationRepository::default());
        let effects = Arc::new(RecordingEffects::default());
        let request = request(CompanionContinuationRuntimeProtocol::FullFork);
        repository
            .create(CompanionContinuationSaga::requested(request.clone()).expect("request"))
            .await
            .expect("create");
        CompanionContinuationWorker::new(repository.as_ref(), effects.as_ref())
            .advance(request.request_id)
            .await
            .expect("runtime ready");
        CompanionContinuationWorker::new(repository.as_ref(), effects.as_ref())
            .advance(request.request_id)
            .await
            .expect("first input prepared");

        repository.fail_next_save();
        let failure = CompanionContinuationWorker::new(repository.as_ref(), effects.as_ref())
            .advance(request.request_id)
            .await
            .expect_err("save crash");
        assert!(matches!(
            failure,
            CompanionContinuationRepositoryError::Unavailable(_)
        ));

        let terminal =
            drive_to_terminal(repository.as_ref(), effects.as_ref(), request.request_id).await;
        let invocations = effects.phase_invocations("first_input");
        assert_eq!(invocations.len(), 2);
        assert_eq!(invocations[0], invocations[1]);
        assert_eq!(
            effects.logical_effects.lock().expect("logical lock").len(),
            6
        );
        assert!(
            !terminal
                .evidence()
                .first_input
                .as_ref()
                .expect("first input")
                .submitted_by_runtime_protocol
        );
        let CompanionPreparedFirstInputEvidence::ProductDelivery { envelope } = terminal
            .evidence()
            .prepared_first_input
            .as_ref()
            .expect("prepared first input")
        else {
            panic!("Full Companion must persist a Product delivery envelope");
        };
        assert!(matches!(
            envelope.command_request.command,
            crate::agent_run::AgentRunProductCommand::SubmitInput { .. }
        ));
    }

    #[tokio::test]
    async fn fresh_claims_the_inner_protocol_first_input_evidence() {
        let repository = RecordingCompanionContinuationRepository::default();
        let effects = RecordingEffects::default();
        let request = request(CompanionContinuationRuntimeProtocol::FreshCreate);
        repository
            .create(CompanionContinuationSaga::requested(request.clone()).expect("request"))
            .await
            .expect("create");

        let terminal = drive_to_terminal(&repository, &effects, request.request_id).await;
        let first_input = terminal
            .evidence()
            .first_input
            .as_ref()
            .expect("first input");
        assert!(first_input.submitted_by_runtime_protocol);
        assert_eq!(effects.phase_invocations("first_input").len(), 1);
    }

    #[tokio::test]
    async fn gate_channel_and_task_effect_identities_are_stable_and_distinct() {
        let request = request(CompanionContinuationRuntimeProtocol::FullFork);
        let saga = CompanionContinuationSaga::requested(request).expect("request");
        let phases = [
            CompanionContinuationPhase::GateConverged,
            CompanionContinuationPhase::ChannelConverged,
            CompanionContinuationPhase::TaskConverged,
        ];
        let first = phases.map(|phase| saga.effect_identity(phase));
        let second = phases.map(|phase| saga.effect_identity(phase));
        assert_eq!(first, second);
        assert_eq!(
            first
                .iter()
                .map(|identity| identity.effect_id)
                .collect::<BTreeSet<_>>()
                .len(),
            phases.len()
        );
    }

    #[tokio::test]
    async fn every_persisted_phase_is_recoverable_by_a_new_worker() {
        let repository = RecordingCompanionContinuationRepository::default();
        let effects = RecordingEffects::default();
        let request = request(CompanionContinuationRuntimeProtocol::FullFork);
        repository
            .create(CompanionContinuationSaga::requested(request.clone()).expect("request"))
            .await
            .expect("create");

        for expected in [
            CompanionContinuationPhase::RuntimeReady,
            CompanionContinuationPhase::FirstInputPrepared,
            CompanionContinuationPhase::FirstInputConverged,
            CompanionContinuationPhase::GateConverged,
            CompanionContinuationPhase::ChannelConverged,
            CompanionContinuationPhase::TaskConverged,
            CompanionContinuationPhase::AfterDispatchHookConverged,
            CompanionContinuationPhase::Succeeded,
        ] {
            let recovered = CompanionContinuationWorker::new(&repository, &effects)
                .advance(request.request_id)
                .await
                .expect("recovered advance");
            assert_eq!(recovered.phase(), expected);
        }
        assert!(
            repository
                .list_recoverable(1)
                .await
                .expect("list")
                .is_empty()
        );
    }

    #[test]
    fn runtime_protocol_must_match_the_durable_preparation_plan() {
        let mut request = request(CompanionContinuationRuntimeProtocol::FreshCreate);
        request.runtime_protocol = CompanionContinuationRuntimeProtocol::FullFork;
        assert!(
            CompanionContinuationSaga::requested(request)
                .expect_err("protocol drift must be rejected")
                .contains("preparation plan drifted")
        );
    }

    #[tokio::test]
    async fn after_dispatch_hook_pending_blocks_success_until_restart_recovers_it() {
        let repository = RecordingCompanionContinuationRepository::default();
        let effects = RecordingEffects::default();
        effects.pend_after_hook_once();
        let request = request(CompanionContinuationRuntimeProtocol::FullFork);
        repository
            .create(CompanionContinuationSaga::requested(request.clone()).expect("request"))
            .await
            .expect("create");

        for _ in 0..6 {
            CompanionContinuationWorker::new(&repository, &effects)
                .advance(request.request_id)
                .await
                .expect("advance to task");
        }
        let pending = CompanionContinuationWorker::new(&repository, &effects)
            .advance(request.request_id)
            .await
            .expect("pending after hook");
        assert_eq!(pending.phase(), CompanionContinuationPhase::TaskConverged);
        assert!(pending.evidence().after_dispatch_hook.is_none());

        let recovered = CompanionContinuationWorker::new(&repository, &effects)
            .advance(request.request_id)
            .await
            .expect("restart recovers after hook");
        assert_eq!(
            recovered.phase(),
            CompanionContinuationPhase::AfterDispatchHookConverged
        );
        let succeeded = CompanionContinuationWorker::new(&repository, &effects)
            .advance(request.request_id)
            .await
            .expect("mark succeeded");
        assert_eq!(succeeded.phase(), CompanionContinuationPhase::Succeeded);
    }

    #[tokio::test]
    async fn after_dispatch_hook_lost_save_response_reuses_one_effect_identity_and_real_child() {
        let repository = RecordingCompanionContinuationRepository::default();
        let effects = RecordingEffects::default();
        let request = request(CompanionContinuationRuntimeProtocol::FreshCreate);
        repository
            .create(CompanionContinuationSaga::requested(request.clone()).expect("request"))
            .await
            .expect("create");

        for _ in 0..6 {
            CompanionContinuationWorker::new(&repository, &effects)
                .advance(request.request_id)
                .await
                .expect("advance to task");
        }
        repository.fail_next_save();
        CompanionContinuationWorker::new(&repository, &effects)
            .advance(request.request_id)
            .await
            .expect_err("post-hook save response lost");

        let recovered = CompanionContinuationWorker::new(&repository, &effects)
            .advance(request.request_id)
            .await
            .expect("replay hook");
        let invocations = effects.phase_invocations("after_dispatch_hook");
        assert_eq!(invocations.len(), 2);
        assert_eq!(invocations[0], invocations[1]);
        assert_eq!(invocations[0], request.after_dispatch_hook_effect.effect_id);
        let runtime = recovered.evidence().runtime.as_ref().expect("runtime");
        let hook = recovered
            .evidence()
            .after_dispatch_hook
            .as_ref()
            .expect("hook evidence");
        assert_eq!(hook.child_frame_id, runtime.child_frame_id);
        assert_eq!(
            hook.child_runtime_thread_id,
            runtime.child_runtime_thread_id
        );
        assert_ne!(hook.child_frame_id, request.parent_frame_id);
        assert_eq!(
            effects.logical_effects.lock().expect("logical lock").len(),
            6
        );
    }
}
