use std::sync::Arc;

use agentdash_application_runtime_gateway::runtime_gateway::validate_json_schema_subset;
use agentdash_domain::interaction::{
    InteractionActor, InteractionCommandCommit, InteractionCommandDefinition,
    InteractionCommandRequest, InteractionCommandTransaction, InteractionCommandTransactionPort,
    InteractionDefinitionRepository, InteractionError, InteractionEvent,
    InteractionEventRepository, InteractionInstance, InteractionInstanceRepository,
    InteractionInstanceStatus, OperationEffectAdmissionAudit, OperationEffectIntent,
    OperationEffectPrincipalRef, OperationEffectSafety, OperationEffectStatus,
    PlatformCommandHandler, ResolvedInteractionCommand, StatePatchOperation,
};
use agentdash_domain::operation::OperationRef;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde_json::Value;
use sha2::{Digest, Sha256};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum InteractionApplicationError {
    #[error(transparent)]
    Domain(#[from] InteractionError),
    #[error("Interaction command 输入无效: {field}: {reason}")]
    InvalidCommand { field: &'static str, reason: String },
    #[error("Interaction contract 不可用: {reason}")]
    ContractUnavailable { reason: String },
}

pub type InteractionApplicationResult<T> = Result<T, InteractionApplicationError>;

#[derive(Debug, Clone, PartialEq)]
pub struct InteractionCommandInput {
    pub instance_id: Uuid,
    pub command_id: Uuid,
    pub command_key: String,
    pub payload: Value,
    pub expected_state_revision: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InteractionCloseInput {
    pub instance_id: Uuid,
    pub expected_state_revision: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InteractionCommandAdmission {
    pub actor: InteractionActor,
    pub origin: agentdash_domain::interaction::InteractionCommandOrigin,
    pub attachment_id: Option<Uuid>,
    pub capability_revision_ref: String,
}

/// 由 authenticated host adapter 构造，不提供 serde wire contract。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InteractionCommandCallerContext {
    AuthenticatedUser { user_id: String },
    ResolvedAgentRun { run_id: Uuid, agent_id: Uuid },
}

impl InteractionCommandCallerContext {
    fn matches_actor(&self, actor: &InteractionActor) -> bool {
        match (self, actor) {
            (
                Self::AuthenticatedUser { user_id: expected },
                InteractionActor::Human { user_id },
            ) => expected == user_id,
            (
                Self::ResolvedAgentRun {
                    run_id: expected_run,
                    agent_id: expected_agent,
                },
                InteractionActor::Agent {
                    run_id: Some(run_id),
                    agent_id,
                },
            ) => expected_run == run_id && expected_agent == agent_id,
            _ => false,
        }
    }
}

#[async_trait]
pub trait InteractionCommandAdmissionPort: Send + Sync {
    async fn admit(
        &self,
        instance: &InteractionInstance,
        input: &InteractionCommandInput,
        caller: &InteractionCommandCallerContext,
    ) -> InteractionApplicationResult<InteractionCommandAdmission>;
    async fn admit_close(
        &self,
        instance: &InteractionInstance,
        input: &InteractionCloseInput,
        caller: &InteractionCommandCallerContext,
    ) -> InteractionApplicationResult<()>;
}

#[async_trait]
pub trait InteractionEffectDescriptorAdmissionPort: Send + Sync {
    async fn admit_replay_safe(
        &self,
        operation_ref: &OperationRef,
    ) -> InteractionApplicationResult<OperationEffectSafety>;
}

#[derive(Clone)]
pub struct InteractionCommandService {
    definitions: Arc<dyn InteractionDefinitionRepository>,
    instances: Arc<dyn InteractionInstanceRepository>,
    transactions: Arc<dyn InteractionCommandTransactionPort>,
    events: Arc<dyn InteractionEventRepository>,
    admission: Arc<dyn InteractionCommandAdmissionPort>,
    effect_admission: Arc<dyn InteractionEffectDescriptorAdmissionPort>,
}

impl InteractionCommandService {
    pub fn new(
        definitions: Arc<dyn InteractionDefinitionRepository>,
        instances: Arc<dyn InteractionInstanceRepository>,
        transactions: Arc<dyn InteractionCommandTransactionPort>,
        events: Arc<dyn InteractionEventRepository>,
        admission: Arc<dyn InteractionCommandAdmissionPort>,
        effect_admission: Arc<dyn InteractionEffectDescriptorAdmissionPort>,
    ) -> Self {
        Self {
            definitions,
            instances,
            transactions,
            events,
            admission,
            effect_admission,
        }
    }

    pub async fn execute(
        &self,
        input: InteractionCommandInput,
        caller: InteractionCommandCallerContext,
        now: DateTime<Utc>,
    ) -> InteractionApplicationResult<InteractionCommandCommit> {
        let instance = self
            .instances
            .get(input.instance_id)
            .await?
            .ok_or_else(|| InteractionError::NotFound {
                entity: "interaction_instance",
                id: input.instance_id.to_string(),
            })?;
        if instance.status != InteractionInstanceStatus::Open {
            return Err(InteractionError::InvalidStatusTransition {
                from: instance.status.as_str(),
                to: "command",
            }
            .into());
        }
        let revision = self
            .definitions
            .get_revision(instance.definition_revision_id)
            .await?
            .ok_or_else(|| InteractionError::NotFound {
                entity: "interaction_definition_revision",
                id: instance.definition_revision_id.to_string(),
            })?;
        if revision.definition_id != instance.definition_id
            || revision.owner != instance.owner
            || revision.interaction_contract_version != instance.interaction_contract_version
        {
            return Err(InteractionApplicationError::ContractUnavailable {
                reason: "instance 与 pinned definition revision identity 不一致".to_string(),
            });
        }
        let admission = self.admission.admit(&instance, &input, &caller).await?;
        if !caller.matches_actor(&admission.actor) {
            return Err(InteractionApplicationError::ContractUnavailable {
                reason: "admission actor 与 authenticated caller context 不一致".to_string(),
            });
        }
        let request = InteractionCommandRequest {
            instance_id: input.instance_id,
            command_id: input.command_id,
            command_key: input.command_key,
            payload: input.payload,
            expected_state_revision: input.expected_state_revision,
            actor: admission.actor,
            origin: admission.origin,
            attachment_id: admission.attachment_id,
        };
        let definition = revision
            .command_definitions
            .iter()
            .find(|definition| definition.command_key == request.command_key)
            .cloned()
            .ok_or_else(|| InteractionError::NotFound {
                entity: "interaction_command_definition",
                id: request.command_key.clone(),
            })?;
        validate_json_schema_subset(&definition.payload_schema, &request.payload).map_err(
            |reason| InteractionApplicationError::InvalidCommand {
                field: "payload",
                reason,
            },
        )?;
        let resolved = revision.resolve_command(request)?;
        let next_state = apply_resolved_command(&instance.state, &resolved, &definition)?;
        validate_json_schema_subset(&revision.state_schema, &next_state).map_err(|reason| {
            InteractionApplicationError::InvalidCommand {
                field: "state",
                reason,
            }
        })?;
        if let Some(contract) = &definition.state_patch_v1 {
            contract.validate_state_size(&next_state)?;
        }
        let next_revision = instance.state_revision.checked_add(1).ok_or_else(|| {
            InteractionApplicationError::ContractUnavailable {
                reason: "state revision overflow".to_string(),
            }
        })?;
        let event = InteractionEvent {
            id: Uuid::new_v4(),
            instance_id: instance.id,
            sequence: next_revision,
            command_id: resolved.request.command_id,
            command_key: resolved.request.command_key.clone(),
            handler: resolved.handler.clone(),
            actor: resolved.request.actor.clone(),
            payload: resolved.request.payload.clone(),
            resulting_state_revision: next_revision,
            created_at: now,
        };
        let admitted_safety = if let Some(effect) = &definition.operation_effect {
            Some(
                self.effect_admission
                    .admit_replay_safe(&effect.operation_ref)
                    .await?,
            )
        } else {
            None
        };
        let effect_intent = build_effect_intent(
            &instance,
            &resolved,
            &definition,
            &event,
            admission.capability_revision_ref,
            admitted_safety,
            now,
        )?;
        let request_digest = command_digest(&resolved)?;
        self.transactions
            .commit(InteractionCommandTransaction {
                command: resolved,
                request_digest,
                previous_state_revision: instance.state_revision,
                next_state,
                next_state_revision: next_revision,
                event,
                effect_intent,
            })
            .await
            .map_err(Into::into)
    }

    pub async fn close(
        &self,
        input: InteractionCloseInput,
        caller: InteractionCommandCallerContext,
    ) -> InteractionApplicationResult<InteractionInstance> {
        let instance = self
            .instances
            .get(input.instance_id)
            .await?
            .ok_or_else(|| InteractionError::NotFound {
                entity: "interaction_instance",
                id: input.instance_id.to_string(),
            })?;
        self.admission
            .admit_close(&instance, &input, &caller)
            .await?;
        self.instances
            .close(input.instance_id, input.expected_state_revision)
            .await
            .map_err(Into::into)
    }

    pub async fn rebuild_state(&self, instance_id: Uuid) -> InteractionApplicationResult<Value> {
        let instance =
            self.instances
                .get(instance_id)
                .await?
                .ok_or_else(|| InteractionError::NotFound {
                    entity: "interaction_instance",
                    id: instance_id.to_string(),
                })?;
        let revision = self
            .definitions
            .get_revision(instance.definition_revision_id)
            .await?
            .ok_or_else(|| InteractionError::NotFound {
                entity: "interaction_definition_revision",
                id: instance.definition_revision_id.to_string(),
            })?;
        let events = self.events.list_events(instance_id, 0).await?;
        let mut state = revision.initial_state.clone();
        let mut expected_sequence = 1_u64;
        for event in events {
            if event.sequence != expected_sequence {
                return Err(InteractionApplicationError::ContractUnavailable {
                    reason: format!(
                        "event sequence 非连续: expected={expected_sequence}, actual={}",
                        event.sequence
                    ),
                });
            }
            let definition = revision
                .command_definitions
                .iter()
                .find(|definition| definition.command_key == event.command_key)
                .ok_or_else(|| InteractionApplicationError::ContractUnavailable {
                    reason: format!("event command definition 缺失: {}", event.command_key),
                })?;
            state = apply_handler(&state, &event.handler, &event.payload, definition)?;
            expected_sequence += 1;
        }
        let rebuilt_revision = expected_sequence.saturating_sub(1);
        if rebuilt_revision != instance.state_revision || state != instance.state {
            return Err(InteractionApplicationError::ContractUnavailable {
                reason: format!(
                    "rebuilt state 与 canonical instance 不一致: rebuilt_revision={rebuilt_revision}, instance_revision={}",
                    instance.state_revision
                ),
            });
        }
        validate_json_schema_subset(&revision.state_schema, &state).map_err(|reason| {
            InteractionApplicationError::InvalidCommand {
                field: "rebuilt_state",
                reason,
            }
        })?;
        Ok(state)
    }
}

fn apply_resolved_command(
    state: &Value,
    resolved: &ResolvedInteractionCommand,
    definition: &InteractionCommandDefinition,
) -> InteractionApplicationResult<Value> {
    apply_handler(
        state,
        &resolved.handler,
        &resolved.request.payload,
        definition,
    )
}

fn apply_handler(
    state: &Value,
    handler: &PlatformCommandHandler,
    payload: &Value,
    definition: &InteractionCommandDefinition,
) -> InteractionApplicationResult<Value> {
    match handler {
        PlatformCommandHandler::StatePatchV1 => {
            let operations: Vec<StatePatchOperation> = serde_json::from_value(payload.clone())
                .map_err(|error| InteractionApplicationError::InvalidCommand {
                    field: "state_patch_v1",
                    reason: error.to_string(),
                })?;
            let contract = definition.state_patch_v1.as_ref().ok_or_else(|| {
                InteractionApplicationError::ContractUnavailable {
                    reason: "state_patch_v1 contract 缺失".to_string(),
                }
            })?;
            contract.validate(&operations)?;
            let patch: json_patch::Patch =
                serde_json::from_value(payload.clone()).map_err(|error| {
                    InteractionApplicationError::InvalidCommand {
                        field: "state_patch_v1",
                        reason: error.to_string(),
                    }
                })?;
            let mut next = state.clone();
            json_patch::patch(&mut next, &patch).map_err(|error| {
                InteractionApplicationError::InvalidCommand {
                    field: "state_patch_v1",
                    reason: error.to_string(),
                }
            })?;
            Ok(next)
        }
    }
}

fn build_effect_intent(
    instance: &InteractionInstance,
    resolved: &ResolvedInteractionCommand,
    definition: &InteractionCommandDefinition,
    event: &InteractionEvent,
    capability_revision_ref: String,
    admitted_safety: Option<OperationEffectSafety>,
    now: DateTime<Utc>,
) -> InteractionApplicationResult<Option<OperationEffectIntent>> {
    let Some(effect) = &definition.operation_effect else {
        return Ok(None);
    };
    let safety =
        admitted_safety.ok_or_else(|| InteractionApplicationError::ContractUnavailable {
            reason: "effect descriptor admission 缺失".to_string(),
        })?;
    let principal = match &resolved.request.actor {
        InteractionActor::Human { user_id } => OperationEffectPrincipalRef::Human {
            user_id: user_id.clone(),
        },
        InteractionActor::Agent { agent_id, run_id } => OperationEffectPrincipalRef::Agent {
            agent_id: *agent_id,
            run_id: *run_id,
        },
    };
    let intent = OperationEffectIntent {
        effect_id: resolved.request.command_id,
        instance_id: instance.id,
        source_event_id: event.id,
        operation_ref: effect.operation_ref.clone(),
        validated_input: resolved.request.payload.clone(),
        admission_audit: OperationEffectAdmissionAudit {
            principal,
            scope: instance.owner.clone(),
            capability_revision_ref,
            admitted_at: now,
        },
        idempotency_key: format!(
            "interaction:{}:command:{}:effect",
            instance.id, resolved.request.command_id
        ),
        safety,
        status: OperationEffectStatus::Pending,
        attempt: 0,
        next_attempt_at: now,
        claim_token: None,
        claimed_at: None,
        claim_expires_at: None,
        completed_at: None,
        last_failure_code: None,
    };
    intent.validate()?;
    Ok(Some(intent))
}

fn command_digest(command: &ResolvedInteractionCommand) -> InteractionApplicationResult<String> {
    let bytes = serde_json::to_vec(command).map_err(|error| {
        InteractionApplicationError::InvalidCommand {
            field: "command",
            reason: error.to_string(),
        }
    })?;
    Ok(format!("sha256:{:x}", Sha256::digest(bytes)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentdash_domain::interaction::*;
    use async_trait::async_trait;
    use std::sync::Mutex;

    struct FixtureStore {
        revision: InteractionDefinitionRevision,
        instance: InteractionInstance,
        committed: Mutex<Option<InteractionCommandTransaction>>,
    }
    #[async_trait]
    impl InteractionDefinitionRepository for FixtureStore {
        async fn create(
            &self,
            _: &InteractionDefinition,
            _: &InteractionDefinitionRevision,
        ) -> Result<(), InteractionError> {
            unsupported()
        }
        async fn get(&self, _: Uuid) -> Result<Option<InteractionDefinition>, InteractionError> {
            Ok(None)
        }
        async fn get_revision(
            &self,
            id: Uuid,
        ) -> Result<Option<InteractionDefinitionRevision>, InteractionError> {
            Ok((id == self.revision.revision_id).then(|| self.revision.clone()))
        }
        async fn list_by_owner(
            &self,
            _: &InteractionOwner,
        ) -> Result<Vec<InteractionDefinition>, InteractionError> {
            Ok(vec![])
        }
        async fn commit_revision(
            &self,
            _: Uuid,
            _: DefinitionRevisionCommit,
        ) -> Result<InteractionDefinition, InteractionError> {
            unsupported()
        }
        async fn archive(&self, _: Uuid) -> Result<InteractionDefinition, InteractionError> {
            unsupported()
        }
    }
    #[async_trait]
    impl InteractionInstanceRepository for FixtureStore {
        async fn create(&self, _: &InteractionInstance) -> Result<(), InteractionError> {
            unsupported()
        }
        async fn get(&self, id: Uuid) -> Result<Option<InteractionInstance>, InteractionError> {
            Ok((id == self.instance.id).then(|| self.instance.clone()))
        }
        async fn list_by_owner(
            &self,
            _: &InteractionOwner,
        ) -> Result<Vec<InteractionInstance>, InteractionError> {
            Ok(vec![])
        }
        async fn close(
            &self,
            id: Uuid,
            expected: u64,
        ) -> Result<InteractionInstance, InteractionError> {
            if id != self.instance.id || expected != self.instance.state_revision {
                return Err(InteractionError::StateRevisionConflict {
                    instance_id: id,
                    expected,
                    actual: self.instance.state_revision,
                });
            }
            let mut instance = self.instance.clone();
            instance.close(Utc::now())?;
            Ok(instance)
        }
        async fn attach(&self, _: &InteractionAttachment) -> Result<(), InteractionError> {
            unsupported()
        }
        async fn detach(&self, _: Uuid) -> Result<(), InteractionError> {
            unsupported()
        }
        async fn upsert_runtime_binding(
            &self,
            _: &InteractionRuntimeBinding,
        ) -> Result<(), InteractionError> {
            unsupported()
        }
        async fn list_runtime_bindings(
            &self,
            _: Uuid,
            _: Option<Uuid>,
        ) -> Result<Vec<InteractionRuntimeBinding>, InteractionError> {
            Ok(vec![])
        }
    }
    #[async_trait]
    impl InteractionCommandTransactionPort for FixtureStore {
        async fn commit(
            &self,
            transaction: InteractionCommandTransaction,
        ) -> Result<InteractionCommandCommit, InteractionError> {
            *self
                .committed
                .lock()
                .map_err(|_| InteractionError::Persistence {
                    operation: "fixture_lock",
                    message: "poisoned".into(),
                })? = Some(transaction.clone());
            let mut instance = self.instance.clone();
            instance.state = transaction.next_state;
            instance.state_revision = transaction.next_state_revision;
            Ok(InteractionCommandCommit::Committed {
                instance,
                event: transaction.event,
                effect_intent: transaction.effect_intent,
            })
        }
    }
    #[async_trait]
    impl InteractionEventRepository for FixtureStore {
        async fn list_events(
            &self,
            _: Uuid,
            _: u64,
        ) -> Result<Vec<InteractionEvent>, InteractionError> {
            Ok(vec![])
        }
    }
    #[async_trait]
    impl InteractionCommandAdmissionPort for FixtureStore {
        async fn admit(
            &self,
            instance: &InteractionInstance,
            _: &InteractionCommandInput,
            caller: &InteractionCommandCallerContext,
        ) -> InteractionApplicationResult<InteractionCommandAdmission> {
            match caller {
                InteractionCommandCallerContext::AuthenticatedUser { user_id }
                    if instance.owner == InteractionOwner::User(user_id.clone()) =>
                {
                    Ok(InteractionCommandAdmission {
                        actor: InteractionActor::Human {
                            user_id: user_id.clone(),
                        },
                        origin: InteractionCommandOrigin::UserWorkshop,
                        attachment_id: None,
                        capability_revision_ref: "capability:1".into(),
                    })
                }
                InteractionCommandCallerContext::ResolvedAgentRun { run_id, agent_id } => {
                    Ok(InteractionCommandAdmission {
                        actor: InteractionActor::Agent {
                            agent_id: *agent_id,
                            run_id: Some(*run_id),
                        },
                        origin: InteractionCommandOrigin::AgentFrame,
                        attachment_id: None,
                        capability_revision_ref: "capability:1".into(),
                    })
                }
                _ => Err(InteractionApplicationError::InvalidCommand {
                    field: "caller",
                    reason: "caller 无 instance access".into(),
                }),
            }
        }
        async fn admit_close(
            &self,
            instance: &InteractionInstance,
            _: &InteractionCloseInput,
            caller: &InteractionCommandCallerContext,
        ) -> InteractionApplicationResult<()> {
            match caller {
                InteractionCommandCallerContext::AuthenticatedUser { user_id }
                    if instance.owner == InteractionOwner::User(user_id.clone()) =>
                {
                    Ok(())
                }
                _ => Err(InteractionApplicationError::InvalidCommand {
                    field: "caller",
                    reason: "只有 owner human 可关闭 instance".into(),
                }),
            }
        }
    }
    #[async_trait]
    impl InteractionEffectDescriptorAdmissionPort for FixtureStore {
        async fn admit_replay_safe(
            &self,
            _: &OperationRef,
        ) -> InteractionApplicationResult<OperationEffectSafety> {
            Ok(OperationEffectSafety::Idempotent)
        }
    }
    fn unsupported<T>() -> Result<T, InteractionError> {
        Err(InteractionError::Persistence {
            operation: "fixture_unsupported",
            message: "unsupported".into(),
        })
    }

    fn fixture() -> Arc<FixtureStore> {
        let owner = InteractionOwner::User("u".into());
        let definition_id = Uuid::new_v4();
        let source = SourceBundle::new(
            "main.tsx",
            vec![SourceFile::new("main.tsx", "", None).expect("file")],
            SourceSandboxConfig::default(),
        )
        .expect("source");
        let mut revision=InteractionDefinitionRevision::new_canvas_v1(definition_id,1,owner.clone(),"test","",source,serde_json::json!({"count":0}),serde_json::json!({"type":"object","properties":{"count":{"type":"integer"}},"additionalProperties":false}),"u").expect("revision");
        revision
            .command_definitions
            .push(InteractionCommandDefinition {
                command_key: "set_count".into(),
                handler: PlatformCommandHandler::StatePatchV1,
                actor_policy: CommandActorPolicy::Direct,
                payload_schema: serde_json::json!({"type":"array","items":{"type":"object"}}),
                state_patch_v1: Some(
                    StatePatchV1Contract::new(vec!["/count".into()], 1, 1024).expect("contract"),
                ),
                operation_effect: None,
            });
        let instance = InteractionInstance::new_v1(
            owner,
            definition_id,
            revision.revision_id,
            revision.initial_state.clone(),
            InteractionRetention { retain_until: None },
        )
        .expect("instance");
        Arc::new(FixtureStore {
            revision,
            instance,
            committed: Mutex::new(None),
        })
    }

    #[tokio::test]
    async fn human_and_agent_share_typed_command_path() {
        let store = fixture();
        let service = InteractionCommandService::new(
            store.clone(),
            store.clone(),
            store.clone(),
            store.clone(),
            store.clone(),
            store.clone(),
        );
        let outcome = service
            .execute(
                InteractionCommandInput {
                    instance_id: store.instance.id,
                    command_id: Uuid::new_v4(),
                    command_key: "set_count".into(),
                    payload: serde_json::json!([{"op":"replace","path":"/count","value":2}]),
                    expected_state_revision: 0,
                },
                InteractionCommandCallerContext::ResolvedAgentRun {
                    run_id: Uuid::new_v4(),
                    agent_id: Uuid::new_v4(),
                },
                Utc::now(),
            )
            .await
            .expect("command");
        match outcome {
            InteractionCommandCommit::Committed { instance, .. } => {
                assert_eq!(instance.state, serde_json::json!({"count":2}))
            }
            InteractionCommandCommit::Duplicate { .. } => assert!(false, "fixture always commits"),
        }
    }

    #[tokio::test]
    async fn human_only_is_rejected_before_transaction() {
        let store = fixture();
        let mut revision = store.revision.clone();
        revision.command_definitions[0].actor_policy = CommandActorPolicy::HumanOnly;
        let store = Arc::new(FixtureStore {
            revision,
            instance: store.instance.clone(),
            committed: Mutex::new(None),
        });
        let service = InteractionCommandService::new(
            store.clone(),
            store.clone(),
            store.clone(),
            store.clone(),
            store.clone(),
            store.clone(),
        );
        let result = service
            .execute(
                InteractionCommandInput {
                    instance_id: store.instance.id,
                    command_id: Uuid::new_v4(),
                    command_key: "set_count".into(),
                    payload: serde_json::json!([{"op":"replace","path":"/count","value":2}]),
                    expected_state_revision: 0,
                },
                InteractionCommandCallerContext::ResolvedAgentRun {
                    run_id: Uuid::new_v4(),
                    agent_id: Uuid::new_v4(),
                },
                Utc::now(),
            )
            .await;
        assert!(matches!(
            result,
            Err(InteractionApplicationError::Domain(
                InteractionError::HumanOnlyCommand { .. }
            ))
        ));
        assert!(store.committed.lock().expect("lock").is_none());
    }

    #[tokio::test]
    async fn explicit_close_requires_admitted_owner_human() {
        let store = fixture();
        let service = InteractionCommandService::new(
            store.clone(),
            store.clone(),
            store.clone(),
            store.clone(),
            store.clone(),
            store.clone(),
        );
        let denied = service
            .close(
                InteractionCloseInput {
                    instance_id: store.instance.id,
                    expected_state_revision: 0,
                },
                InteractionCommandCallerContext::ResolvedAgentRun {
                    run_id: Uuid::new_v4(),
                    agent_id: Uuid::new_v4(),
                },
            )
            .await;
        assert!(matches!(
            denied,
            Err(InteractionApplicationError::InvalidCommand {
                field: "caller",
                ..
            })
        ));
        let closed = service
            .close(
                InteractionCloseInput {
                    instance_id: store.instance.id,
                    expected_state_revision: 0,
                },
                InteractionCommandCallerContext::AuthenticatedUser {
                    user_id: "u".into(),
                },
            )
            .await
            .expect("owner closes");
        assert_eq!(closed.status, InteractionInstanceStatus::Closed);
    }

    #[tokio::test]
    async fn effect_identity_is_stable_from_command_and_safety_is_admitted() {
        let base = fixture();
        let mut revision = base.revision.clone();
        revision.command_definitions[0].operation_effect =
            Some(InteractionOperationEffectDefinition {
                operation_ref: OperationRef::new("host", "core", "notify", 1).expect("operation"),
            });
        let store = Arc::new(FixtureStore {
            revision,
            instance: base.instance.clone(),
            committed: Mutex::new(None),
        });
        let service = InteractionCommandService::new(
            store.clone(),
            store.clone(),
            store.clone(),
            store.clone(),
            store.clone(),
            store.clone(),
        );
        let command_id = Uuid::new_v4();
        let outcome = service
            .execute(
                InteractionCommandInput {
                    instance_id: store.instance.id,
                    command_id,
                    command_key: "set_count".into(),
                    payload: serde_json::json!([{"op":"replace","path":"/count","value":2}]),
                    expected_state_revision: 0,
                },
                InteractionCommandCallerContext::AuthenticatedUser {
                    user_id: "u".into(),
                },
                Utc::now(),
            )
            .await
            .expect("command");
        let effect = match outcome {
            InteractionCommandCommit::Committed {
                effect_intent: Some(effect),
                ..
            } => effect,
            _ => return assert!(false, "effect must exist"),
        };
        assert_eq!(effect.effect_id, command_id);
        assert_eq!(effect.safety, OperationEffectSafety::Idempotent);
    }
}
