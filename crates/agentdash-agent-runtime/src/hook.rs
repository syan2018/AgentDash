use std::collections::BTreeSet;

pub use agentdash_agent_runtime_contract::HookRunDecision as HookGateDecision;
pub use agentdash_agent_runtime_contract::{
    BoundRuntimeHookEntry, BoundRuntimeHookPlan, HookExecutionSite, RuntimeHookPlanBinding,
};
use agentdash_agent_runtime_contract::{
    HookAction, HookDefinitionId, HookEffectId, HookFailurePolicy, HookPlanDigest,
    HookPlanRevision, HookPoint, HookRunId, HookRunTerminal, RuntimeInteractionId, RuntimeItemId,
    RuntimeOperationId, RuntimeThreadId, RuntimeTurnId, SemanticStrength,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HookCorrelation {
    pub operation_id: Option<RuntimeOperationId>,
    pub turn_id: Option<RuntimeTurnId>,
    pub item_id: Option<RuntimeItemId>,
    pub interaction_id: Option<RuntimeInteractionId>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RuntimeHookInvocation {
    pub hook_run_id: HookRunId,
    pub thread_id: RuntimeThreadId,
    pub definition_id: HookDefinitionId,
    pub point: HookPoint,
    pub correlation: HookCorrelation,
    pub input: serde_json::Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HookRunStatus {
    Accepted,
    Running,
    Completed,
    Blocked,
    Failed,
    Stopped,
    Cancelled,
}

impl HookRunStatus {
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Completed | Self::Blocked | Self::Failed | Self::Stopped | Self::Cancelled
        )
    }

    pub fn terminal(self) -> Option<HookRunTerminal> {
        match self {
            Self::Completed => Some(HookRunTerminal::Completed),
            Self::Blocked => Some(HookRunTerminal::Blocked),
            Self::Failed => Some(HookRunTerminal::Failed),
            Self::Stopped => Some(HookRunTerminal::Stopped),
            Self::Cancelled => Some(HookRunTerminal::Cancelled),
            Self::Accepted | Self::Running => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HookRun {
    pub hook_run_id: HookRunId,
    pub thread_id: RuntimeThreadId,
    pub definition_id: HookDefinitionId,
    pub point: HookPoint,
    pub plan_revision: HookPlanRevision,
    pub plan_digest: HookPlanDigest,
    pub actions: BTreeSet<HookAction>,
    pub delivered_strength: SemanticStrength,
    pub failure_policy: HookFailurePolicy,
    pub site: HookExecutionSite,
    pub correlation: HookCorrelation,
    pub input: serde_json::Value,
    pub status: HookRunStatus,
    pub decision: Option<HookGateDecision>,
    pub terminal_message: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HookCompletion {
    pub status: HookRunStatus,
    pub decision: HookGateDecision,
    pub message: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HookEffectDescriptor {
    pub effect_type: String,
    pub schema_version: u32,
    pub target_authority: String,
    pub retry_limit: u32,
    pub payload_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HookEffect {
    pub effect_id: HookEffectId,
    pub hook_run_id: HookRunId,
    pub thread_id: RuntimeThreadId,
    pub idempotency_key: String,
    pub descriptor: HookEffectDescriptor,
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum HookAdmission {
    SilentObserver,
    Durable(Box<HookRun>),
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum HookRuntimeError {
    #[error("hook definition {definition_id} is not bound at point {point:?}")]
    DefinitionNotBound {
        definition_id: HookDefinitionId,
        point: HookPoint,
    },
    #[error("hook run is already terminal")]
    AlreadyTerminal,
    #[error("hook run transition from {from:?} to {to:?} is invalid")]
    InvalidTransition {
        from: HookRunStatus,
        to: HookRunStatus,
    },
    #[error("hook effect coordinates do not match its hook run")]
    EffectCoordinates,
    #[error("hook effect descriptor or idempotency key is invalid")]
    InvalidEffectDescriptor,
    #[error("hook completion decision is incompatible with its actions or failure policy")]
    CompletionPolicy,
    #[error("hook correlation does not belong to the invocation thread")]
    InvalidCorrelation,
    #[error("bound hook plan contains duplicate definition and point coordinates")]
    InvalidPlan,
    #[error("retry durable effect policy requires an emit-effect action")]
    RetryPolicyWithoutEffect,
}

#[derive(Debug, Error)]
pub enum HookOrchestrationError {
    #[error(transparent)]
    Domain(#[from] HookRuntimeError),
    #[error("hook runtime store failed: {0}")]
    Store(#[from] crate::RuntimeStoreError),
    #[error("hook journal transition failed: {0}")]
    Transition(#[from] crate::TransitionError),
    #[error("runtime thread was not found")]
    ThreadNotFound,
    #[error("hook run was not found")]
    RunNotFound,
    #[error("hook run id was reused with different immutable coordinates")]
    RunConflict,
}

fn admit_hook(
    plan: &BoundRuntimeHookPlan,
    invocation: RuntimeHookInvocation,
) -> Result<HookAdmission, HookRuntimeError> {
    let entry = plan
        .entries
        .iter()
        .find(|entry| {
            entry.definition_id == invocation.definition_id && entry.point == invocation.point
        })
        .ok_or_else(|| HookRuntimeError::DefinitionNotBound {
            definition_id: invocation.definition_id.clone(),
            point: invocation.point,
        })?;
    let silent = entry
        .actions
        .iter()
        .all(|action| *action == HookAction::Observe)
        && entry.failure_policy == HookFailurePolicy::ObserveOnly;
    if silent {
        return Ok(HookAdmission::SilentObserver);
    }
    Ok(HookAdmission::Durable(Box::new(HookRun {
        hook_run_id: invocation.hook_run_id,
        thread_id: invocation.thread_id,
        definition_id: invocation.definition_id,
        point: invocation.point,
        plan_revision: plan.revision,
        plan_digest: plan.digest.clone(),
        actions: entry.actions.clone(),
        delivered_strength: entry.delivered_strength,
        failure_policy: entry.failure_policy,
        site: entry.site,
        correlation: invocation.correlation,
        input: invocation.input,
        status: HookRunStatus::Accepted,
        decision: None,
        terminal_message: None,
    })))
}

impl HookRun {
    fn same_identity_as(&self, other: &Self) -> bool {
        self.hook_run_id == other.hook_run_id
            && self.thread_id == other.thread_id
            && self.definition_id == other.definition_id
            && self.point == other.point
            && self.plan_revision == other.plan_revision
            && self.plan_digest == other.plan_digest
            && self.actions == other.actions
            && self.delivered_strength == other.delivered_strength
            && self.failure_policy == other.failure_policy
            && self.site == other.site
            && self.correlation == other.correlation
            && self.input == other.input
    }

    pub fn start(&mut self) -> Result<(), HookRuntimeError> {
        self.transition(HookRunStatus::Running, None)
    }

    pub fn complete(&mut self, completion: HookCompletion) -> Result<(), HookRuntimeError> {
        if !completion.status.is_terminal() {
            return Err(HookRuntimeError::InvalidTransition {
                from: self.status,
                to: completion.status,
            });
        }
        let decision_allowed = match completion.status {
            HookRunStatus::Blocked => {
                completion.decision == HookGateDecision::Block
                    && self.actions.contains(&HookAction::Block)
            }
            HookRunStatus::Failed => {
                self.failure_policy != HookFailurePolicy::FailClosed
                    && completion.decision == HookGateDecision::Continue
                    && completion
                        .message
                        .as_deref()
                        .is_some_and(|message| !message.trim().is_empty())
            }
            HookRunStatus::Stopped => completion.decision == HookGateDecision::Stop,
            HookRunStatus::Completed => completion.decision == HookGateDecision::Continue,
            HookRunStatus::Cancelled => completion.decision == HookGateDecision::Continue,
            HookRunStatus::Accepted | HookRunStatus::Running => false,
        };
        if !decision_allowed {
            return Err(HookRuntimeError::CompletionPolicy);
        }
        self.decision = Some(completion.decision);
        self.transition(completion.status, completion.message)
    }

    pub fn recover_after_interruption(&mut self) -> Result<(), HookRuntimeError> {
        if self.status == HookRunStatus::Accepted {
            self.start()
        } else if self.status == HookRunStatus::Running {
            Ok(())
        } else {
            Err(HookRuntimeError::AlreadyTerminal)
        }
    }

    pub fn validate_effect(&self, effect: &HookEffect) -> Result<(), HookRuntimeError> {
        if self.hook_run_id != effect.hook_run_id || self.thread_id != effect.thread_id {
            return Err(HookRuntimeError::EffectCoordinates);
        }
        if !self.actions.contains(&HookAction::EmitEffect)
            || effect.idempotency_key.trim().is_empty()
            || effect.descriptor.effect_type.trim().is_empty()
            || effect.descriptor.schema_version == 0
            || effect.descriptor.target_authority.trim().is_empty()
            || effect.descriptor.payload_digest != hook_effect_payload_digest(&effect.payload)
        {
            return Err(HookRuntimeError::InvalidEffectDescriptor);
        }
        Ok(())
    }

    fn transition(
        &mut self,
        next: HookRunStatus,
        message: Option<String>,
    ) -> Result<(), HookRuntimeError> {
        if self.status.is_terminal() {
            return Err(HookRuntimeError::AlreadyTerminal);
        }
        let valid = matches!(
            (self.status, next),
            (HookRunStatus::Accepted, HookRunStatus::Running)
                | (HookRunStatus::Running, HookRunStatus::Completed)
                | (HookRunStatus::Running, HookRunStatus::Blocked)
                | (HookRunStatus::Running, HookRunStatus::Failed)
                | (HookRunStatus::Running, HookRunStatus::Stopped)
                | (HookRunStatus::Running, HookRunStatus::Cancelled)
        );
        if !valid {
            return Err(HookRuntimeError::InvalidTransition {
                from: self.status,
                to: next,
            });
        }
        self.status = next;
        self.terminal_message = message;
        Ok(())
    }
}

pub fn hook_effect_payload_digest(payload: &serde_json::Value) -> String {
    fn canonicalize(value: &serde_json::Value) -> serde_json::Value {
        match value {
            serde_json::Value::Array(values) => {
                serde_json::Value::Array(values.iter().map(canonicalize).collect())
            }
            serde_json::Value::Object(values) => {
                let mut keys = values.keys().collect::<Vec<_>>();
                keys.sort_unstable();
                let mut canonical = serde_json::Map::new();
                for key in keys {
                    canonical.insert(key.clone(), canonicalize(&values[key]));
                }
                serde_json::Value::Object(canonical)
            }
            scalar => scalar.clone(),
        }
    }

    let bytes = serde_json::to_vec(&canonicalize(payload))
        .expect("serde_json::Value always serializes to canonical JSON");
    format!("sha256:{:x}", Sha256::digest(bytes))
}

impl<S> crate::ManagedAgentRuntime<S>
where
    S: crate::RuntimeRepository + crate::RuntimeUnitOfWork,
{
    pub async fn bind_hook_plan(
        &self,
        binding: RuntimeHookPlanBinding,
    ) -> Result<RuntimeHookPlanBinding, HookOrchestrationError> {
        validate_bound_hook_plan(&binding.plan)?;
        let mut thread = self
            .store()
            .load_thread(&binding.thread_id)
            .await?
            .ok_or(HookOrchestrationError::ThreadNotFound)?;
        if let Some(existing) = self.store().load_hook_plan(&binding.thread_id).await? {
            if existing == binding {
                return Ok(existing);
            }
            let expected = existing.plan.revision.0.saturating_add(1);
            if binding.plan.revision.0 != expected {
                return Err(HookOrchestrationError::RunConflict);
            }
        } else if binding.plan.revision.0 != 1 {
            return Err(HookOrchestrationError::RunConflict);
        }
        let expected = thread.revision;
        let events = thread.append_events([
            agentdash_agent_runtime_contract::RuntimeEvent::HookPlanBound {
                plan_revision: binding.plan.revision,
                plan_digest: binding.plan.digest.clone(),
            },
        ])?;
        self.store()
            .commit(crate::RuntimeCommit {
                expected_projection_revision: Some(expected),
                projection: thread,
                operation: None,
                operation_terminals: Vec::new(),
                events,
                outbox: Vec::new(),
                context_activation_outbox: Vec::new(),
                context_preparation_work_items: Vec::new(),
                context_checkpoints: Vec::new(),
                context_candidates: Vec::new(),
                context_activations: Vec::new(),
                context_head: None,
                hook_plan_binding: Some(binding.clone()),
                hook_runs: Vec::new(),
                hook_effects: Vec::new(),
                quarantine: Vec::new(),
            })
            .await?;
        Ok(binding)
    }

    pub async fn accept_hook(
        &self,
        invocation: RuntimeHookInvocation,
    ) -> Result<HookAdmission, HookOrchestrationError> {
        let durable_plan = self
            .store()
            .load_hook_plan(&invocation.thread_id)
            .await?
            .ok_or(HookOrchestrationError::RunConflict)?;
        let admission = admit_hook(&durable_plan.plan, invocation)?;
        let HookAdmission::Durable(run) = admission else {
            return Ok(HookAdmission::SilentObserver);
        };
        let mut thread = self
            .store()
            .load_thread(&run.thread_id)
            .await?
            .ok_or(HookOrchestrationError::ThreadNotFound)?;
        if let Some(operation_id) = &run.correlation.operation_id {
            let operation = self.store().find_operation(operation_id).await?;
            if operation.is_none_or(|operation| operation.thread_id != run.thread_id) {
                return Err(HookRuntimeError::InvalidCorrelation.into());
            }
        }
        let turn_matches = run
            .correlation
            .turn_id
            .as_ref()
            .is_none_or(|turn_id| thread.turns.contains_key(turn_id));
        let item_matches = run.correlation.item_id.as_ref().is_none_or(|item_id| {
            run.correlation.turn_id.as_ref().is_some_and(|turn_id| {
                thread
                    .items
                    .get(item_id)
                    .is_some_and(|item| item.turn_id == *turn_id)
            })
        });
        let interaction_matches =
            run.correlation
                .interaction_id
                .as_ref()
                .is_none_or(|interaction_id| {
                    run.correlation.turn_id.as_ref().is_some_and(|turn_id| {
                        thread
                            .interactions
                            .get(interaction_id)
                            .is_some_and(|interaction| interaction.turn_id == *turn_id)
                    })
                });
        if !turn_matches || !item_matches || !interaction_matches {
            return Err(HookRuntimeError::InvalidCorrelation.into());
        }
        if let Some(existing) = self.store().load_hook_run(&run.hook_run_id).await? {
            if existing.same_identity_as(&run) {
                return Ok(HookAdmission::Durable(Box::new(existing)));
            }
            return Err(HookOrchestrationError::RunConflict);
        }
        let expected = thread.revision;
        let events = thread.append_events([
            agentdash_agent_runtime_contract::RuntimeEvent::HookRunAccepted {
                hook_run_id: run.hook_run_id.clone(),
                definition_id: run.definition_id.clone(),
                point: run.point,
                plan_revision: run.plan_revision,
                plan_digest: run.plan_digest.clone(),
                operation_id: run.correlation.operation_id.clone(),
                turn_id: run.correlation.turn_id.clone(),
                item_id: run.correlation.item_id.clone(),
                interaction_id: run.correlation.interaction_id.clone(),
            },
        ])?;
        let result = self
            .store()
            .commit(crate::RuntimeCommit {
                expected_projection_revision: Some(expected),
                projection: thread,
                operation: None,
                operation_terminals: Vec::new(),
                events,
                outbox: Vec::new(),
                context_activation_outbox: Vec::new(),
                context_preparation_work_items: Vec::new(),
                context_checkpoints: Vec::new(),
                context_candidates: Vec::new(),
                context_activations: Vec::new(),
                context_head: None,
                hook_plan_binding: None,
                hook_runs: vec![(*run).clone()],
                hook_effects: Vec::new(),
                quarantine: Vec::new(),
            })
            .await;
        if let Err(error) = result {
            if let Some(existing) = self.store().load_hook_run(&run.hook_run_id).await?
                && existing.same_identity_as(&run)
            {
                return Ok(HookAdmission::Durable(Box::new(existing)));
            }
            return Err(error.into());
        }
        Ok(HookAdmission::Durable(run))
    }

    pub async fn request_hook_interaction(
        &self,
        hook_run_id: &HookRunId,
        interaction_id: RuntimeInteractionId,
        prompt: String,
    ) -> Result<(), HookOrchestrationError> {
        let run = self
            .store()
            .load_hook_run(hook_run_id)
            .await?
            .ok_or(HookOrchestrationError::RunNotFound)?;
        let mut thread = self
            .store()
            .load_thread(&run.thread_id)
            .await?
            .ok_or(HookOrchestrationError::ThreadNotFound)?;
        if thread.interactions.contains_key(&interaction_id) {
            return Ok(());
        }
        let turn_id = run
            .correlation
            .turn_id
            .clone()
            .ok_or(HookRuntimeError::InvalidCorrelation)?;
        let expected = thread.revision;
        let events = thread.append_events([
            agentdash_agent_runtime_contract::RuntimeEvent::InteractionRequested {
                turn_id,
                item_id: run.correlation.item_id.clone(),
                interaction_id,
                interaction_kind:
                    agentdash_agent_runtime_contract::RuntimeInteractionKind::PermissionApproval,
                prompt,
            },
        ])?;
        self.store()
            .commit(crate::RuntimeCommit {
                expected_projection_revision: Some(expected),
                projection: thread,
                operation: None,
                operation_terminals: Vec::new(),
                events,
                outbox: Vec::new(),
                context_activation_outbox: Vec::new(),
                context_preparation_work_items: Vec::new(),
                context_checkpoints: Vec::new(),
                context_candidates: Vec::new(),
                context_activations: Vec::new(),
                context_head: None,
                hook_plan_binding: None,
                hook_runs: Vec::new(),
                hook_effects: Vec::new(),
                quarantine: Vec::new(),
            })
            .await?;
        Ok(())
    }

    pub async fn start_hook(
        &self,
        hook_run_id: &HookRunId,
    ) -> Result<HookRun, HookOrchestrationError> {
        let mut run = self
            .store()
            .load_hook_run(hook_run_id)
            .await?
            .ok_or(HookOrchestrationError::RunNotFound)?;
        if run.status == HookRunStatus::Running {
            return Ok(run);
        }
        if run.status.is_terminal() {
            return Err(HookRuntimeError::AlreadyTerminal.into());
        }
        let mut thread = self
            .store()
            .load_thread(&run.thread_id)
            .await?
            .ok_or(HookOrchestrationError::ThreadNotFound)?;
        let expected = thread.revision;
        run.start()?;
        let events = thread.append_events([
            agentdash_agent_runtime_contract::RuntimeEvent::HookRunStarted {
                hook_run_id: run.hook_run_id.clone(),
            },
        ])?;
        let result = self
            .store()
            .commit(crate::RuntimeCommit {
                expected_projection_revision: Some(expected),
                projection: thread,
                operation: None,
                operation_terminals: Vec::new(),
                events,
                outbox: Vec::new(),
                context_activation_outbox: Vec::new(),
                context_preparation_work_items: Vec::new(),
                context_checkpoints: Vec::new(),
                context_candidates: Vec::new(),
                context_activations: Vec::new(),
                context_head: None,
                hook_plan_binding: None,
                hook_runs: vec![run.clone()],
                hook_effects: Vec::new(),
                quarantine: Vec::new(),
            })
            .await;
        if let Err(error) = result {
            if let Some(existing) = self.store().load_hook_run(hook_run_id).await?
                && existing.same_identity_as(&run)
                && existing.status == HookRunStatus::Running
            {
                return Ok(existing);
            }
            return Err(error.into());
        }
        Ok(run)
    }

    pub async fn complete_hook(
        &self,
        hook_run_id: &HookRunId,
        completion: HookCompletion,
        effects: Vec<HookEffect>,
    ) -> Result<HookRun, HookOrchestrationError> {
        let mut run = self
            .store()
            .load_hook_run(hook_run_id)
            .await?
            .ok_or(HookOrchestrationError::RunNotFound)?;
        if run.status.is_terminal() {
            let same_terminal = run.status == completion.status
                && run.decision == Some(completion.decision)
                && run.terminal_message == completion.message;
            let mut durable_effects = self.store().hook_effects(hook_run_id).await?;
            let mut requested_effects = effects;
            durable_effects.sort_by(|left, right| left.effect_id.cmp(&right.effect_id));
            requested_effects.sort_by(|left, right| left.effect_id.cmp(&right.effect_id));
            if same_terminal && durable_effects == requested_effects {
                return Ok(run);
            }
            return Err(HookOrchestrationError::RunConflict);
        }
        for effect in &effects {
            run.validate_effect(effect)?;
        }
        let mut effect_ids = BTreeSet::new();
        let mut idempotency_keys = BTreeSet::new();
        if effects.iter().any(|effect| {
            !effect_ids.insert(effect.effect_id.clone())
                || !idempotency_keys.insert(effect.idempotency_key.clone())
        }) {
            return Err(HookRuntimeError::InvalidEffectDescriptor.into());
        }
        if run.failure_policy == HookFailurePolicy::RetryDurableEffect
            && completion.status == HookRunStatus::Completed
            && effects.is_empty()
        {
            return Err(HookRuntimeError::CompletionPolicy.into());
        }
        let mut thread = self
            .store()
            .load_thread(&run.thread_id)
            .await?
            .ok_or(HookOrchestrationError::ThreadNotFound)?;
        let expected = thread.revision;
        let requested_completion = completion.clone();
        run.complete(completion)?;
        let terminal = run.status.terminal().expect("completion is terminal");
        let mut effect_ids = effects
            .iter()
            .map(|effect| effect.effect_id.clone())
            .collect::<Vec<_>>();
        effect_ids.sort();
        let events = thread.append_events([
            agentdash_agent_runtime_contract::RuntimeEvent::HookRunTerminal {
                hook_run_id: run.hook_run_id.clone(),
                terminal,
                decision: run.decision.expect("terminal hook run has a decision"),
                message: run.terminal_message.clone(),
                effect_ids,
            },
        ])?;
        let requested_effects = effects.clone();
        let result = self
            .store()
            .commit(crate::RuntimeCommit {
                expected_projection_revision: Some(expected),
                projection: thread,
                operation: None,
                operation_terminals: Vec::new(),
                events,
                outbox: Vec::new(),
                context_activation_outbox: Vec::new(),
                context_preparation_work_items: Vec::new(),
                context_checkpoints: Vec::new(),
                context_candidates: Vec::new(),
                context_activations: Vec::new(),
                context_head: None,
                hook_plan_binding: None,
                hook_runs: vec![run.clone()],
                hook_effects: effects,
                quarantine: Vec::new(),
            })
            .await;
        if let Err(error) = result {
            if let Some(existing) = self.store().load_hook_run(hook_run_id).await?
                && existing.status == requested_completion.status
                && existing.decision == Some(requested_completion.decision)
                && existing.terminal_message == requested_completion.message
            {
                let mut durable_effects = self.store().hook_effects(hook_run_id).await?;
                let mut requested_effects = requested_effects;
                durable_effects.sort_by(|left, right| left.effect_id.cmp(&right.effect_id));
                requested_effects.sort_by(|left, right| left.effect_id.cmp(&right.effect_id));
                if durable_effects == requested_effects {
                    return Ok(existing);
                }
            }
            return Err(error.into());
        }
        Ok(run)
    }

    pub async fn recoverable_hook_runs(&self) -> Result<Vec<HookRun>, HookOrchestrationError> {
        Ok(self.store().recoverable_hook_runs().await?)
    }
}

pub(crate) fn validate_bound_hook_plan(
    plan: &BoundRuntimeHookPlan,
) -> Result<(), HookRuntimeError> {
    let mut coordinates = BTreeSet::new();
    if plan.entries.iter().any(|entry| {
        entry.actions.is_empty() || !coordinates.insert((entry.definition_id.clone(), entry.point))
    }) {
        return Err(HookRuntimeError::InvalidPlan);
    }
    if plan.entries.iter().any(|entry| {
        entry.failure_policy == HookFailurePolicy::RetryDurableEffect
            && !entry.actions.contains(&HookAction::EmitEffect)
    }) {
        return Err(HookRuntimeError::RetryPolicyWithoutEffect);
    }
    Ok(())
}

pub fn failure_completion(policy: HookFailurePolicy, message: String) -> HookCompletion {
    match policy {
        HookFailurePolicy::FailClosed => HookCompletion {
            status: HookRunStatus::Blocked,
            decision: HookGateDecision::Block,
            message: Some(message),
        },
        HookFailurePolicy::FailOpenWithDiagnostic | HookFailurePolicy::ObserveOnly => {
            HookCompletion {
                status: HookRunStatus::Failed,
                decision: HookGateDecision::Continue,
                message: Some(message),
            }
        }
        HookFailurePolicy::RetryDurableEffect => HookCompletion {
            status: HookRunStatus::Failed,
            decision: HookGateDecision::Continue,
            message: Some(message),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id<T: std::str::FromStr>(value: &str) -> T
    where
        T::Err: std::fmt::Debug,
    {
        value.parse().expect("valid id")
    }

    fn invocation() -> RuntimeHookInvocation {
        RuntimeHookInvocation {
            hook_run_id: id("hook-run-1"),
            thread_id: id("thread-1"),
            definition_id: id("hook-1"),
            point: HookPoint::BeforeTurn,
            correlation: HookCorrelation {
                operation_id: None,
                turn_id: None,
                item_id: None,
                interaction_id: None,
            },
            input: serde_json::json!({"prompt": "hi"}),
        }
    }

    fn plan(actions: BTreeSet<HookAction>, policy: HookFailurePolicy) -> BoundRuntimeHookPlan {
        BoundRuntimeHookPlan {
            revision: HookPlanRevision(3),
            digest: id("plan-3"),
            entries: vec![BoundRuntimeHookEntry {
                definition_id: id("hook-1"),
                point: HookPoint::BeforeTurn,
                actions,
                delivered_strength: SemanticStrength::ExactDurableBoundary,
                failure_policy: policy,
                required: true,
                site: HookExecutionSite::ManagedRuntime,
            }],
        }
    }

    #[test]
    fn silent_observer_does_not_create_a_durable_run() {
        let admitted = admit_hook(
            &plan([HookAction::Observe].into(), HookFailurePolicy::ObserveOnly),
            invocation(),
        )
        .expect("admit observer");
        assert!(matches!(admitted, HookAdmission::SilentObserver));
    }

    #[test]
    fn actionful_run_has_exact_terminal_and_failure_policy() {
        let HookAdmission::Durable(mut run) = admit_hook(
            &plan([HookAction::Block].into(), HookFailurePolicy::FailClosed),
            invocation(),
        )
        .expect("admit run") else {
            panic!("actionful hook must be durable")
        };
        run.start().expect("start");
        let completion = failure_completion(run.failure_policy, "denied".to_string());
        assert_eq!(completion.decision, HookGateDecision::Block);
        run.complete(completion).expect("terminal");
        assert_eq!(run.status, HookRunStatus::Blocked);
        assert!(matches!(
            run.start(),
            Err(HookRuntimeError::AlreadyTerminal)
        ));
    }

    #[test]
    fn failure_policy_validates_the_terminal_status_and_decision_together() {
        let HookAdmission::Durable(mut fail_closed) = admit_hook(
            &plan([HookAction::Block].into(), HookFailurePolicy::FailClosed),
            invocation(),
        )
        .expect("admit fail-closed hook") else {
            panic!("fail-closed hook is durable")
        };
        fail_closed.start().expect("start fail-closed hook");
        assert!(matches!(
            fail_closed.complete(HookCompletion {
                status: HookRunStatus::Failed,
                decision: HookGateDecision::Block,
                message: Some("bridge unavailable".to_string()),
            }),
            Err(HookRuntimeError::CompletionPolicy)
        ));

        let HookAdmission::Durable(mut fail_open) = admit_hook(
            &plan(
                [HookAction::AddContext].into(),
                HookFailurePolicy::FailOpenWithDiagnostic,
            ),
            invocation(),
        )
        .expect("admit fail-open hook") else {
            panic!("fail-open hook is durable")
        };
        fail_open.start().expect("start fail-open hook");
        fail_open
            .complete(failure_completion(
                HookFailurePolicy::FailOpenWithDiagnostic,
                "context provider unavailable".to_string(),
            ))
            .expect("fail-open converges to failed and continue");
        assert_eq!(fail_open.status, HookRunStatus::Failed);
        assert_eq!(fail_open.decision, Some(HookGateDecision::Continue));
    }

    #[test]
    fn effect_digest_is_derived_from_canonical_payload() {
        let left = serde_json::json!({"b": [2, {"z": true, "a": null}], "a": 1});
        let right = serde_json::json!({"a": 1, "b": [2, {"a": null, "z": true}]});
        assert_eq!(
            hook_effect_payload_digest(&left),
            hook_effect_payload_digest(&right)
        );
    }

    #[test]
    fn non_runtime_execution_sites_still_produce_canonical_hook_runs() {
        let mut callback_plan = plan(
            [HookAction::RewriteInput].into(),
            HookFailurePolicy::FailClosed,
        );
        callback_plan.entries[0].site = HookExecutionSite::DriverNative;
        let HookAdmission::Durable(run) = admit_hook(&callback_plan, invocation())
            .expect("driver-native route is admitted by the runtime journal")
        else {
            panic!("actionful driver hook is durable")
        };
        assert_eq!(run.site, HookExecutionSite::DriverNative);
        assert_eq!(run.status, HookRunStatus::Accepted);
    }
}
