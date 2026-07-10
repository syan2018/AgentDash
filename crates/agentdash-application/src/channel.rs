use std::collections::BTreeSet;
use std::sync::Arc;

use agentdash_domain::agent_run_mailbox::{
    ConsumptionBarrier, MailboxDelivery, MailboxDrainMode, MailboxMessageOrigin,
    NewAgentRunMailboxMessage,
};
use agentdash_domain::channel::{
    Channel, ChannelBinding, ChannelBindingId, ChannelBindingStatus, ChannelCapabilityRef,
    ChannelDeliveryIntent, ChannelDeliveryState, ChannelDeliveryTarget, ChannelEgressPolicy,
    ChannelIngressPolicy, ChannelLocator, ChannelMessage, ChannelMessageOrigin, ChannelOperation,
    ChannelOwner, ChannelParticipant, ChannelParticipantRef, ChannelPolicy, ChannelReadiness,
    ChannelRecord, ChannelRef, ChannelRegistryDocument, ChannelRegistryMutation, ChannelStatus,
    MaterializedDeliveryRef, channel_message_origin_to_mailbox_source_identity,
};
use agentdash_domain::workflow::LifecycleRunRepository;
use agentdash_spi::channel_binding::{
    ChannelOutboundRequest, ChannelProviderInboundEvent, ChannelProviderReceipt,
};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::ApplicationError;
use provider::{ChannelBindingIndex, ChannelBindingProviderRegistry, map_provider_error};

pub mod provider;

#[async_trait]
pub trait ChannelOwnerStore: Send + Sync {
    async fn load_registry(
        &self,
        owner: &ChannelOwner,
    ) -> Result<ChannelRegistryDocument, ApplicationError>;

    async fn mutate_registry(
        &self,
        owner: &ChannelOwner,
        mutation: ChannelRegistryMutation,
    ) -> Result<ChannelRegistryDocument, ApplicationError>;

    async fn list_binding_registries(
        &self,
    ) -> Result<Vec<(ChannelOwner, ChannelRegistryDocument)>, ApplicationError>;
}

pub struct LifecycleRunChannelOwnerStore {
    lifecycle_run_repo: Arc<dyn LifecycleRunRepository>,
}

impl LifecycleRunChannelOwnerStore {
    pub fn new(lifecycle_run_repo: Arc<dyn LifecycleRunRepository>) -> Self {
        Self { lifecycle_run_repo }
    }
}

#[async_trait]
impl ChannelOwnerStore for LifecycleRunChannelOwnerStore {
    async fn load_registry(
        &self,
        owner: &ChannelOwner,
    ) -> Result<ChannelRegistryDocument, ApplicationError> {
        let ChannelOwner::LifecycleRun { run_id } = owner else {
            return Err(ApplicationError::InvalidConfig(format!(
                "channel owner `{}` is not backed by LifecycleRunChannelOwnerStore",
                owner.stable_key()
            )));
        };
        self.lifecycle_run_repo
            .load_channel_registry(*run_id)
            .await
            .map_err(ApplicationError::from)
    }

    async fn mutate_registry(
        &self,
        owner: &ChannelOwner,
        mutation: ChannelRegistryMutation,
    ) -> Result<ChannelRegistryDocument, ApplicationError> {
        let ChannelOwner::LifecycleRun { run_id } = owner else {
            return Err(ApplicationError::InvalidConfig(format!(
                "channel owner `{}` is not backed by LifecycleRunChannelOwnerStore",
                owner.stable_key()
            )));
        };
        if let ChannelRegistryMutation::UpsertChannel(record)
        | ChannelRegistryMutation::CreateChannelIfAbsent(record) = &mutation
            && &record.channel.owner != owner
        {
            return Err(ApplicationError::Conflict(format!(
                "channel record owner {} does not match store owner {}",
                record.channel.owner.stable_key(),
                owner.stable_key()
            )));
        }
        self.lifecycle_run_repo
            .mutate_channel_registry(*run_id, mutation)
            .await
            .map_err(ApplicationError::from)
    }

    async fn list_binding_registries(
        &self,
    ) -> Result<Vec<(ChannelOwner, ChannelRegistryDocument)>, ApplicationError> {
        Ok(self
            .lifecycle_run_repo
            .list_channel_registries_with_bindings()
            .await
            .map_err(ApplicationError::from)?
            .into_iter()
            .map(|(run_id, registry)| (ChannelOwner::LifecycleRun { run_id }, registry))
            .collect())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderEventKey {
    pub provider: String,
    pub external_workspace_ref: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub external_room_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub external_thread_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_event_ref: Option<String>,
}

impl ProviderEventKey {
    pub fn validate(&self) -> Result<(), ApplicationError> {
        validate_non_empty("provider", &self.provider)?;
        validate_non_empty("external_workspace_ref", &self.external_workspace_ref)?;
        if let Some(room_ref) = &self.external_room_ref {
            validate_non_empty("external_room_ref", room_ref)?;
        }
        if let Some(thread_ref) = &self.external_thread_ref {
            validate_non_empty("external_thread_ref", thread_ref)?;
        }
        if let Some(event_ref) = &self.provider_event_ref {
            validate_non_empty("provider_event_ref", event_ref)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProviderNeutralInboundEnvelope {
    pub key: ProviderEventKey,
    pub sender: ChannelParticipantRef,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payload: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub correlation_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reply_target: Option<agentdash_domain::channel::ChannelReplyTarget>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ChannelMailboxMaterializationCommand {
    pub delivery_id: Uuid,
    pub message: NewAgentRunMailboxMessage,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ChannelGateMaterializationCommand {
    pub delivery_id: Uuid,
    pub message_id: Uuid,
    pub gate_id: Uuid,
    pub channel_id: Uuid,
    pub correlation_ref: Option<String>,
    pub origin: ChannelMessageOrigin,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChannelBindingResolution {
    Resolved {
        owner: ChannelOwner,
        channel_id: Uuid,
        binding: ChannelBinding,
    },
    Unresolved,
}

#[async_trait]
pub trait ChannelBindingResolver: Send + Sync {
    async fn resolve_binding(
        &self,
        key: &ProviderEventKey,
    ) -> Result<ChannelBindingResolution, ApplicationError>;
}

#[derive(Debug, Clone, PartialEq)]
pub enum ChannelIngressOutcome {
    Resolved {
        owner: ChannelOwner,
        message: ChannelMessage,
    },
    Unresolved,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ChannelExternalDeliveryPlan {
    pub intent: ChannelDeliveryIntent,
    pub operation: ChannelOperation,
    pub binding: ChannelBinding,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ChannelExternalDeliveryResult {
    pub receipt: ChannelProviderReceipt,
    pub state: ChannelDeliveryState,
}

pub struct ChannelService {
    owner_store: Arc<dyn ChannelOwnerStore>,
    binding_resolver: Option<Arc<dyn ChannelBindingResolver>>,
    provider_registry: Option<Arc<ChannelBindingProviderRegistry>>,
    binding_index: Option<Arc<dyn ChannelBindingIndex>>,
    binding_mutation_lock: tokio::sync::Mutex<()>,
}

impl ChannelService {
    pub fn new(owner_store: Arc<dyn ChannelOwnerStore>) -> Self {
        Self {
            owner_store,
            binding_resolver: None,
            provider_registry: None,
            binding_index: None,
            binding_mutation_lock: tokio::sync::Mutex::new(()),
        }
    }

    pub fn with_binding_resolver(
        mut self,
        binding_resolver: Arc<dyn ChannelBindingResolver>,
    ) -> Self {
        self.binding_resolver = Some(binding_resolver);
        self
    }

    pub fn with_provider_registry(
        mut self,
        provider_registry: Arc<ChannelBindingProviderRegistry>,
    ) -> Self {
        self.provider_registry = Some(provider_registry);
        self
    }

    pub fn with_binding_index(mut self, binding_index: Arc<dyn ChannelBindingIndex>) -> Self {
        self.binding_index = Some(binding_index);
        self
    }

    pub async fn rebuild_binding_index(&self) -> Result<(), ApplicationError> {
        let binding_index = self.binding_index.as_ref().ok_or_else(|| {
            ApplicationError::InvalidConfig(
                "channel binding index is not configured for rebuild".to_string(),
            )
        })?;
        binding_index.clear().await?;
        for (owner, registry) in self.owner_store.list_binding_registries().await? {
            if let Err(error) = binding_index.replace_owner(&owner, &registry).await {
                binding_index.clear().await?;
                return Err(error);
            }
        }
        Ok(())
    }

    async fn mutate_registry(
        &self,
        owner: &ChannelOwner,
        mutation: ChannelRegistryMutation,
    ) -> Result<ChannelRegistryDocument, ApplicationError> {
        let Some(binding_index) = &self.binding_index else {
            return self.owner_store.mutate_registry(owner, mutation).await;
        };
        let _guard = self.binding_mutation_lock.lock().await;
        let mut candidate = self.owner_store.load_registry(owner).await?;
        candidate
            .apply(mutation.clone())
            .map_err(ApplicationError::from)?;
        binding_index
            .validate_owner_replacement(owner, &candidate)
            .await?;

        let registry = self.owner_store.mutate_registry(owner, mutation).await?;
        if let Err(error) = binding_index.replace_owner(owner, &registry).await {
            let _ = binding_index.remove_owner(owner).await;
            return Err(error);
        }
        Ok(registry)
    }

    pub async fn load_registry(
        &self,
        owner: &ChannelOwner,
    ) -> Result<ChannelRegistryDocument, ApplicationError> {
        self.owner_store.load_registry(owner).await
    }

    pub async fn upsert_channel(
        &self,
        record: ChannelRecord,
    ) -> Result<ChannelRegistryDocument, ApplicationError> {
        let owner = record.channel.owner.clone();
        self.mutate_registry(&owner, ChannelRegistryMutation::UpsertChannel(record))
            .await
    }

    pub async fn create_if_absent(
        &self,
        locator: ChannelLocator,
        aliases: Vec<String>,
    ) -> Result<ChannelRecord, ApplicationError> {
        let mut channel = Channel::new(locator.owner.clone(), locator.channel_key.clone());
        channel.aliases = aliases;
        self.create_record_if_absent(ChannelRecord::new(channel))
            .await
    }

    pub async fn create_record_if_absent(
        &self,
        record: ChannelRecord,
    ) -> Result<ChannelRecord, ApplicationError> {
        let owner = record.channel.owner.clone();
        let channel_key = record.channel.key.clone();
        let registry = self
            .mutate_registry(
                &owner,
                ChannelRegistryMutation::CreateChannelIfAbsent(record),
            )
            .await?;
        registry
            .channels
            .into_iter()
            .find(|existing| existing.channel.owner == owner && existing.channel.key == channel_key)
            .ok_or_else(|| {
                ApplicationError::Conflict(format!(
                    "channel create_if_absent did not materialize locator {}:{}",
                    owner.stable_key(),
                    channel_key
                ))
            })
    }

    pub async fn close_channel(
        &self,
        owner: &ChannelOwner,
        channel_id: Uuid,
        reason: Option<String>,
    ) -> Result<ChannelRegistryDocument, ApplicationError> {
        self.mutate_registry(
            owner,
            ChannelRegistryMutation::CloseChannel { channel_id, reason },
        )
        .await
    }

    pub async fn update_policy(
        &self,
        owner: &ChannelOwner,
        channel_id: Uuid,
        policy: ChannelPolicy,
    ) -> Result<ChannelRegistryDocument, ApplicationError> {
        self.mutate_registry(
            owner,
            ChannelRegistryMutation::UpdateChannelPolicy { channel_id, policy },
        )
        .await
    }

    pub async fn add_participant(
        &self,
        owner: &ChannelOwner,
        channel_id: Uuid,
        participant: ChannelParticipant,
    ) -> Result<ChannelRegistryDocument, ApplicationError> {
        self.mutate_registry(
            owner,
            ChannelRegistryMutation::AddParticipant {
                channel_id,
                participant,
            },
        )
        .await
    }

    pub async fn remove_participant(
        &self,
        owner: &ChannelOwner,
        channel_id: Uuid,
        participant_ref: ChannelParticipantRef,
    ) -> Result<ChannelRegistryDocument, ApplicationError> {
        self.mutate_registry(
            owner,
            ChannelRegistryMutation::RemoveParticipant {
                channel_id,
                participant_ref,
            },
        )
        .await
    }

    pub async fn update_participant_policy(
        &self,
        owner: &ChannelOwner,
        channel_id: Uuid,
        participant_ref: ChannelParticipantRef,
        operations: BTreeSet<ChannelOperation>,
        ingress_policy: ChannelIngressPolicy,
        egress_policy: ChannelEgressPolicy,
    ) -> Result<ChannelRegistryDocument, ApplicationError> {
        self.mutate_registry(
            owner,
            ChannelRegistryMutation::UpdateParticipantPolicy {
                channel_id,
                participant_ref,
                operations,
                ingress_policy,
                egress_policy,
            },
        )
        .await
    }

    pub async fn bind_external_room(
        &self,
        owner: &ChannelOwner,
        channel_id: Uuid,
        binding: ChannelBinding,
    ) -> Result<ChannelRegistryDocument, ApplicationError> {
        self.mutate_registry(
            owner,
            ChannelRegistryMutation::UpsertBinding {
                channel_id,
                binding,
            },
        )
        .await
    }

    pub async fn unbind_external_room(
        &self,
        owner: &ChannelOwner,
        channel_id: Uuid,
        binding_ref: ChannelBindingId,
    ) -> Result<ChannelRegistryDocument, ApplicationError> {
        self.mutate_registry(
            owner,
            ChannelRegistryMutation::RemoveBinding {
                channel_id,
                binding_ref,
            },
        )
        .await
    }

    pub async fn ingest_external_event(
        &self,
        envelope: ProviderNeutralInboundEnvelope,
    ) -> Result<ChannelIngressOutcome, ApplicationError> {
        envelope.key.validate()?;
        envelope.sender.validate().map_err(ApplicationError::from)?;
        let binding_resolver = self.binding_resolver.as_ref().ok_or_else(|| {
            ApplicationError::Unavailable("channel binding resolver is not configured".to_string())
        })?;
        match binding_resolver.resolve_binding(&envelope.key).await? {
            ChannelBindingResolution::Resolved {
                owner,
                channel_id,
                binding,
            } => {
                let kind = if envelope.key.external_thread_ref.is_some() {
                    "thread_reply"
                } else {
                    "room_message"
                };
                let mut origin = ChannelMessageOrigin::new(
                    format!("im.{}", envelope.key.provider),
                    kind,
                    "external",
                );
                if let Some(event_ref) = &envelope.key.provider_event_ref {
                    origin = origin.with_source_ref(event_ref);
                }
                let mut message = ChannelMessage::new(
                    channel_id,
                    envelope.sender,
                    agentdash_domain::channel::ChannelPayload {
                        kind: "provider_event".to_string(),
                        text: envelope.text,
                        data: envelope.payload,
                    },
                    origin,
                );
                message.provider_event_ref = envelope.key.provider_event_ref;
                message.correlation_ref = envelope
                    .correlation_ref
                    .or_else(|| message.provider_event_ref.clone())
                    .or_else(|| Some(binding.binding_id.to_string()));
                message.reply_target = envelope.reply_target;
                let registry = self.owner_store.load_registry(&owner).await?;
                let record = registry
                    .channel(channel_id)
                    .map_err(ApplicationError::from)?;
                if !record.bindings.iter().any(|candidate| {
                    candidate.binding_id == binding.binding_id
                        && candidate.status == ChannelBindingStatus::Active
                }) {
                    return Err(ApplicationError::Conflict(format!(
                        "resolved channel binding {} is not active in channel {}",
                        binding.binding_id, channel_id
                    )));
                }
                validate_message_admission(record, &message, ChannelOperation::Publish)?;
                Ok(ChannelIngressOutcome::Resolved { owner, message })
            }
            ChannelBindingResolution::Unresolved => Ok(ChannelIngressOutcome::Unresolved),
        }
    }

    pub async fn ingest_provider_event(
        &self,
        event: ChannelProviderInboundEvent,
    ) -> Result<ChannelIngressOutcome, ApplicationError> {
        validate_non_empty("provider", &event.provider)?;
        let provider = self.provider(&event.provider)?;
        let provider_key = event.provider.clone();
        let normalized = provider
            .normalize_inbound(event)
            .await
            .map_err(map_provider_error)?;
        self.ingest_external_event(ProviderNeutralInboundEnvelope {
            key: ProviderEventKey {
                provider: provider_key,
                external_workspace_ref: normalized.external_workspace_ref,
                external_room_ref: normalized.external_room_ref,
                external_thread_ref: normalized.external_thread_ref,
                provider_event_ref: Some(normalized.provider_event_ref),
            },
            sender: normalized.sender,
            text: normalized.text,
            payload: normalized.payload,
            correlation_ref: normalized.correlation_ref,
            reply_target: normalized.reply_target,
        })
        .await
    }

    pub async fn plan_broadcast_deliveries(
        &self,
        owner: &ChannelOwner,
        message: ChannelMessage,
    ) -> Result<Vec<ChannelDeliveryIntent>, ApplicationError> {
        let registry = self.owner_store.load_registry(owner).await?;
        let record = registry
            .channel(message.channel_id)
            .map_err(ApplicationError::from)?;
        if &record.channel.owner != owner {
            return Err(ApplicationError::Conflict(format!(
                "channel {} does not belong to owner {}",
                message.channel_id,
                owner.stable_key()
            )));
        }

        validate_message_admission(record, &message, ChannelOperation::Broadcast)?;
        let active_participants = participants_for_message(record, &message)?;
        Ok(active_participants
            .into_iter()
            .filter_map(participant_to_delivery_target)
            .map(|target| ChannelDeliveryIntent::new(message.clone(), target))
            .collect())
    }

    pub async fn record_delivery_state(
        &self,
        owner: &ChannelOwner,
        channel_id: Uuid,
        state: ChannelDeliveryState,
    ) -> Result<ChannelRegistryDocument, ApplicationError> {
        self.mutate_registry(
            owner,
            ChannelRegistryMutation::RecordDeliveryState { channel_id, state },
        )
        .await
    }

    pub async fn project_participant_capability(
        &self,
        owner: &ChannelOwner,
        participant_ref: &ChannelParticipantRef,
    ) -> Result<Vec<ChannelCapabilityRef>, ApplicationError> {
        let registry = self.owner_store.load_registry(owner).await?;
        let mut refs = Vec::new();
        for record in registry.channels {
            let Some(participant) = record.participants.into_iter().find(|participant| {
                participant.left_at.is_none() && &participant.participant_ref == participant_ref
            }) else {
                continue;
            };
            refs.push(ChannelCapabilityRef {
                channel_ref: ChannelRef {
                    owner: record.channel.owner.clone(),
                    channel_id: record.channel.id,
                },
                aliases: record.channel.aliases.clone(),
                operations: participant.operations,
                ingress_policy: participant.ingress_policy,
                egress_policy: participant.egress_policy,
                readiness: if record.channel.status
                    == agentdash_domain::channel::ChannelStatus::Open
                {
                    ChannelReadiness::Ready
                } else {
                    ChannelReadiness::Disabled
                },
            });
        }
        Ok(refs)
    }

    pub async fn plan_external_delivery(
        &self,
        owner: &ChannelOwner,
        binding_id: ChannelBindingId,
        operation: ChannelOperation,
        message: ChannelMessage,
    ) -> Result<ChannelExternalDeliveryPlan, ApplicationError> {
        if !matches!(
            operation,
            ChannelOperation::Publish | ChannelOperation::Reply
        ) {
            return Err(ApplicationError::BadRequest(format!(
                "external channel delivery does not support {operation:?}"
            )));
        }
        if operation == ChannelOperation::Reply && message.reply_target.is_none() {
            return Err(ApplicationError::BadRequest(
                "channel reply requires a reply target".to_string(),
            ));
        }
        let registry = self.owner_store.load_registry(owner).await?;
        let record = registry
            .channel(message.channel_id)
            .map_err(ApplicationError::from)?;
        if &record.channel.owner != owner {
            return Err(ApplicationError::Conflict(format!(
                "channel {} does not belong to owner {}",
                message.channel_id,
                owner.stable_key()
            )));
        }
        validate_message_admission(record, &message, operation)?;
        let binding = record
            .bindings
            .iter()
            .find(|binding| binding.binding_id == binding_id)
            .cloned()
            .ok_or_else(|| {
                ApplicationError::NotFound(format!("channel binding {binding_id} was not found"))
            })?;
        binding.validate().map_err(ApplicationError::from)?;
        if binding.status != ChannelBindingStatus::Active {
            return Err(ApplicationError::Conflict(format!(
                "channel binding {} is not active",
                binding.binding_id
            )));
        }
        self.provider(&binding.provider)?;
        Ok(ChannelExternalDeliveryPlan {
            intent: ChannelDeliveryIntent::new(
                message,
                ChannelDeliveryTarget::ExternalBinding { binding_id },
            ),
            operation,
            binding,
        })
    }

    pub async fn dispatch_external_delivery(
        &self,
        owner: &ChannelOwner,
        plan: ChannelExternalDeliveryPlan,
    ) -> Result<ChannelExternalDeliveryResult, ApplicationError> {
        let binding_id = match &plan.intent.target {
            ChannelDeliveryTarget::ExternalBinding { binding_id } => *binding_id,
            _ => {
                return Err(ApplicationError::BadRequest(format!(
                    "channel delivery {} target is not an external binding",
                    plan.intent.id
                )));
            }
        };
        if binding_id != plan.binding.binding_id {
            return Err(ApplicationError::Conflict(
                "external delivery binding does not match its materialization plan".to_string(),
            ));
        }
        let registry = self.owner_store.load_registry(owner).await?;
        let record = registry
            .channel(plan.intent.message.channel_id)
            .map_err(ApplicationError::from)?;
        if &record.channel.owner != owner {
            return Err(ApplicationError::Conflict(format!(
                "channel {} does not belong to owner {}",
                plan.intent.message.channel_id,
                owner.stable_key()
            )));
        }
        validate_message_admission(record, &plan.intent.message, plan.operation)?;
        let current_binding = record
            .bindings
            .iter()
            .find(|binding| binding.binding_id == binding_id)
            .cloned()
            .ok_or_else(|| {
                ApplicationError::NotFound(format!("channel binding {binding_id} was not found"))
            })?;
        if current_binding.status != ChannelBindingStatus::Active {
            return Err(ApplicationError::Conflict(format!(
                "channel binding {binding_id} is not active"
            )));
        }
        let provider = self.provider(&current_binding.provider)?;
        let receipt = provider
            .publish(ChannelOutboundRequest {
                binding: current_binding,
                operation: plan.operation,
                message: plan.intent.message.clone(),
            })
            .await
            .map_err(map_provider_error)?;
        validate_non_empty("provider_receipt.provider", &receipt.provider)?;
        validate_non_empty(
            "provider_receipt.provider_event_ref",
            &receipt.provider_event_ref,
        )?;
        if receipt.provider != provider.provider_key() {
            return Err(ApplicationError::Conflict(format!(
                "provider receipt `{}` does not match selected provider `{}`",
                receipt.provider,
                provider.provider_key()
            )));
        }
        let mut state = ChannelDeliveryState::new(
            plan.intent.id,
            plan.intent.message.id,
            ChannelDeliveryTarget::ExternalBinding { binding_id },
            agentdash_domain::channel::ChannelDeliveryStatus::Delivered,
        );
        state.materialized_ref = Some(MaterializedDeliveryRef::ProviderEvent {
            provider: receipt.provider.clone(),
            event_ref: receipt.provider_event_ref.clone(),
        });
        self.record_delivery_state(owner, plan.intent.message.channel_id, state.clone())
            .await?;
        Ok(ChannelExternalDeliveryResult { receipt, state })
    }

    fn provider(
        &self,
        provider_key: &str,
    ) -> Result<Arc<dyn agentdash_spi::channel_binding::ChannelBindingProvider>, ApplicationError>
    {
        self.provider_registry
            .as_ref()
            .ok_or_else(|| {
                ApplicationError::Unavailable(
                    "channel binding provider registry is not configured".to_string(),
                )
            })?
            .require(provider_key)
    }

    pub fn materialize_delivery_to_mailbox(
        &self,
        intent: &ChannelDeliveryIntent,
    ) -> Result<ChannelMailboxMaterializationCommand, ApplicationError> {
        let (run_id, agent_id) = match &intent.target {
            ChannelDeliveryTarget::Mailbox { run_id, agent_id } => (*run_id, *agent_id),
            _ => {
                return Err(ApplicationError::BadRequest(format!(
                    "channel delivery {} target is not mailbox",
                    intent.id
                )));
            }
        };
        let source = channel_message_origin_to_mailbox_source_identity(&intent.message.origin);
        let correlation_ref = channel_message_correlation_ref(&intent.message);
        let payload_json = serde_json::json!({
            "channel": {
                "channel_id": intent.message.channel_id,
                "message_id": intent.message.id,
                "delivery_id": intent.id,
                "correlation_ref": correlation_ref,
                "thread_ref": intent.message.thread_ref,
                "provider_event_ref": intent.message.provider_event_ref,
            },
            "payload": intent.message.payload.clone(),
            "content_refs": intent.message.content_refs.clone(),
        });
        Ok(ChannelMailboxMaterializationCommand {
            delivery_id: intent.id,
            message: NewAgentRunMailboxMessage {
                run_id,
                agent_id,
                delivery_runtime_session_id: None,
                origin: mailbox_origin_from_channel_message_origin(&intent.message.origin),
                source,
                delivery: MailboxDelivery::LaunchOrContinueTurn,
                barrier: ConsumptionBarrier::ImmediateIfIdle,
                drain_mode: MailboxDrainMode::One,
                priority: 0,
                source_dedup_key: Some(format!("channel_delivery:{}", intent.id)),
                queued_agent_run_turn_id: None,
                expected_active_agent_run_turn_id: None,
                command_receipt_id: None,
                payload_json: Some(payload_json),
                executor_config_json: None,
                launch_planning_input: None,
                preview: channel_message_preview(&intent.message),
                has_images: false,
                retain_payload: true,
            },
        })
    }

    pub fn materialize_delivery_to_gate(
        &self,
        intent: &ChannelDeliveryIntent,
    ) -> Result<ChannelGateMaterializationCommand, ApplicationError> {
        let gate_id = match &intent.target {
            ChannelDeliveryTarget::LifecycleGate { gate_id } => *gate_id,
            _ => {
                return Err(ApplicationError::BadRequest(format!(
                    "channel delivery {} target is not lifecycle_gate",
                    intent.id
                )));
            }
        };
        Ok(ChannelGateMaterializationCommand {
            delivery_id: intent.id,
            message_id: intent.message.id,
            gate_id,
            channel_id: intent.message.channel_id,
            correlation_ref: channel_message_correlation_ref(&intent.message),
            origin: intent.message.origin.clone(),
        })
    }
}

fn mailbox_origin_from_channel_message_origin(
    address: &ChannelMessageOrigin,
) -> MailboxMessageOrigin {
    match address.namespace.as_str() {
        "companion" => MailboxMessageOrigin::Companion,
        "workflow" => MailboxMessageOrigin::Workflow,
        "core" => MailboxMessageOrigin::User,
        namespace if namespace.starts_with("im.") => MailboxMessageOrigin::System,
        _ => MailboxMessageOrigin::System,
    }
}

fn channel_message_preview(message: &ChannelMessage) -> String {
    let preview = message
        .payload
        .text
        .as_deref()
        .filter(|text| !text.trim().is_empty())
        .unwrap_or(message.payload.kind.as_str());
    preview.chars().take(280).collect()
}

fn channel_message_correlation_ref(message: &ChannelMessage) -> Option<String> {
    message.correlation_ref.clone()
}

fn participants_for_message(
    record: &ChannelRecord,
    message: &ChannelMessage,
) -> Result<Vec<ChannelParticipant>, ApplicationError> {
    if let agentdash_domain::channel::ChannelAudience::Participants { participant_refs } =
        &message.audience
    {
        for participant_ref in participant_refs {
            if !record.participants.iter().any(|participant| {
                participant.left_at.is_none() && participant.participant_ref == *participant_ref
            }) {
                return Err(ApplicationError::Conflict(format!(
                    "channel audience participant {} is not active",
                    participant_ref.stable_key()
                )));
            }
        }
    }
    let participants = match &message.audience {
        agentdash_domain::channel::ChannelAudience::AllParticipants => record
            .participants
            .iter()
            .filter(|participant| participant.left_at.is_none())
            .cloned()
            .collect(),
        agentdash_domain::channel::ChannelAudience::Participants { participant_refs } => record
            .participants
            .iter()
            .filter(|participant| {
                participant.left_at.is_none()
                    && participant_refs
                        .iter()
                        .any(|expected| expected == &participant.participant_ref)
            })
            .cloned()
            .collect(),
        agentdash_domain::channel::ChannelAudience::Role { role } => record
            .participants
            .iter()
            .filter(|participant| participant.left_at.is_none() && participant.role == *role)
            .cloned()
            .collect(),
    };

    let mut participants: Vec<ChannelParticipant> = participants;
    if !record.channel.policy.broadcast.include_sender {
        participants.retain(|participant| participant.participant_ref != message.sender);
    }
    Ok(participants)
}

fn validate_message_admission(
    record: &ChannelRecord,
    message: &ChannelMessage,
    operation: ChannelOperation,
) -> Result<(), ApplicationError> {
    if record.channel.status != ChannelStatus::Open {
        return Err(ApplicationError::Conflict(format!(
            "channel {} is closed",
            record.channel.id
        )));
    }
    let sender = record
        .participants
        .iter()
        .find(|participant| {
            participant.left_at.is_none() && participant.participant_ref == message.sender
        })
        .ok_or_else(|| {
            ApplicationError::Conflict(format!(
                "channel sender {} is not an active participant",
                message.sender.stable_key()
            ))
        })?;
    if !sender.operations.contains(&operation) {
        return Err(ApplicationError::Conflict(format!(
            "channel sender {} is not allowed to {operation:?}",
            message.sender.stable_key()
        )));
    }
    if sender.egress_policy == ChannelEgressPolicy::Disabled {
        return Err(ApplicationError::Conflict(format!(
            "channel sender {} egress is disabled",
            message.sender.stable_key()
        )));
    }
    let recipients = participants_for_message(record, message)?;
    if recipients.is_empty() {
        return Err(ApplicationError::Conflict(
            "channel message has no admitted recipients".to_string(),
        ));
    }
    for recipient in recipients {
        if recipient.ingress_policy == ChannelIngressPolicy::Disabled
            || !recipient.operations.contains(&ChannelOperation::Receive)
        {
            return Err(ApplicationError::Conflict(format!(
                "channel recipient {} cannot receive messages",
                recipient.participant_ref.stable_key()
            )));
        }
    }
    Ok(())
}

fn participant_to_delivery_target(
    participant: ChannelParticipant,
) -> Option<ChannelDeliveryTarget> {
    match participant.participant_ref {
        ChannelParticipantRef::Agent { run_id, agent_id } => {
            Some(ChannelDeliveryTarget::Mailbox { run_id, agent_id })
        }
        ChannelParticipantRef::User { user_id } => {
            Some(ChannelDeliveryTarget::Notification { user_id })
        }
        ChannelParticipantRef::Service { key } => {
            Some(ChannelDeliveryTarget::Platform { broker_key: key })
        }
        ChannelParticipantRef::External { .. } => None,
    }
}

fn validate_non_empty(field: &'static str, value: &str) -> Result<(), ApplicationError> {
    if value.trim().is_empty() {
        return Err(ApplicationError::BadRequest(format!("{field} 不能为空")));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, Ordering};

    use agentdash_domain::channel::{
        ChannelAudience, ChannelEgressPolicy, ChannelIngressPolicy, ChannelOperation,
        ChannelPayload, ChannelPolicy, ChannelReplyTarget, ChannelRole,
    };
    use agentdash_domain::workflow::{LifecycleRun, LifecycleRunRepository};
    use agentdash_spi::channel_binding::{
        ChannelBindingError, ChannelBindingProvider, ChannelOutboundRequest,
        ChannelProviderInboundEvent, ChannelProviderReceipt, NormalizedChannelIngress,
    };
    use agentdash_test_support::workflow::MemoryLifecycleRunRepository;
    use tokio::sync::Mutex;

    use super::provider::{
        ChannelBindingProviderRegistry, InMemoryChannelBindingIndex, IndexedChannelBindingResolver,
    };
    use super::*;

    #[tokio::test]
    async fn lifecycle_owner_store_mutates_registry_without_global_scan() {
        let repo = Arc::new(MemoryLifecycleRunRepository::default());
        let run = LifecycleRun::new_plain(Uuid::new_v4());
        LifecycleRunRepository::create(repo.as_ref(), &run)
            .await
            .expect("create run");
        let service = test_service(repo);
        let owner = ChannelOwner::LifecycleRun { run_id: run.id };

        let record = service
            .create_if_absent(
                locator(owner.clone(), "runtime:companion"),
                vec!["companion".to_string()],
            )
            .await
            .expect("create channel");

        let registry = service.load_registry(&owner).await.expect("load registry");
        assert_eq!(registry.channels.len(), 1);
        assert_eq!(registry.channels[0].channel.id, record.channel.id);
    }

    #[tokio::test]
    async fn unresolved_binding_does_not_require_owner_scan() {
        let repo = Arc::new(MemoryLifecycleRunRepository::default());
        let service = ChannelService::new(Arc::new(LifecycleRunChannelOwnerStore::new(repo)))
            .with_binding_resolver(Arc::new(StaticBindingResolver {
                resolution: ChannelBindingResolution::Unresolved,
            }));

        let outcome = service
            .ingest_external_event(provider_envelope("slack"))
            .await
            .expect("ingest");
        assert_eq!(outcome, ChannelIngressOutcome::Unresolved);
    }

    #[tokio::test]
    async fn missing_binding_runtime_is_explicitly_unavailable() {
        let repo = Arc::new(MemoryLifecycleRunRepository::default());
        let service = ChannelService::new(Arc::new(LifecycleRunChannelOwnerStore::new(repo)));

        let error = service
            .ingest_external_event(provider_envelope("feishu"))
            .await
            .expect_err("binding runtime must be configured");
        assert!(matches!(error, ApplicationError::Unavailable(_)));
    }

    #[tokio::test]
    async fn delivery_planning_does_not_mutate_delivery_state() {
        let repo = Arc::new(MemoryLifecycleRunRepository::default());
        let run = LifecycleRun::new_plain(Uuid::new_v4());
        LifecycleRunRepository::create(repo.as_ref(), &run)
            .await
            .expect("create run");
        let service = test_service(repo);
        let owner = ChannelOwner::LifecycleRun { run_id: run.id };
        let record = service
            .create_if_absent(locator(owner.clone(), "runtime:delivery"), vec![])
            .await
            .expect("create channel");
        let sender = ChannelParticipantRef::Agent {
            run_id: run.id,
            agent_id: Uuid::new_v4(),
        };
        let receiver = ChannelParticipantRef::Agent {
            run_id: run.id,
            agent_id: Uuid::new_v4(),
        };
        service
            .add_participant(
                &owner,
                record.channel.id,
                ChannelParticipant::new(sender.clone(), ChannelRole::Member),
            )
            .await
            .expect("add sender");
        service
            .add_participant(
                &owner,
                record.channel.id,
                ChannelParticipant::new(receiver.clone(), ChannelRole::Member),
            )
            .await
            .expect("add receiver");

        let message = ChannelMessage::new(
            record.channel.id,
            sender,
            ChannelPayload::text("request", "hello"),
            ChannelMessageOrigin::new("companion", "dispatch", "agent"),
        );
        let intents = service
            .plan_broadcast_deliveries(&owner, message)
            .await
            .expect("plan deliveries");

        assert_eq!(intents.len(), 1);
        assert!(matches!(
            intents[0].target,
            ChannelDeliveryTarget::Mailbox { .. }
        ));
        let registry = service.load_registry(&owner).await.expect("load registry");
        assert!(registry.channels[0].delivery_state.is_empty());
    }

    #[tokio::test]
    async fn broadcast_admission_rechecks_sender_status_and_recipient_policy() {
        let repo = Arc::new(MemoryLifecycleRunRepository::default());
        let run = LifecycleRun::new_plain(Uuid::new_v4());
        LifecycleRunRepository::create(repo.as_ref(), &run)
            .await
            .expect("create run");
        let service = test_service(repo);
        let owner = ChannelOwner::LifecycleRun { run_id: run.id };
        let record = service
            .create_if_absent(locator(owner.clone(), "runtime:admission"), vec![])
            .await
            .expect("create channel");
        let sender = ChannelParticipantRef::Agent {
            run_id: run.id,
            agent_id: Uuid::new_v4(),
        };
        let receiver = ChannelParticipantRef::Agent {
            run_id: run.id,
            agent_id: Uuid::new_v4(),
        };
        for participant_ref in [sender.clone(), receiver.clone()] {
            service
                .add_participant(
                    &owner,
                    record.channel.id,
                    ChannelParticipant::new(participant_ref, ChannelRole::Member),
                )
                .await
                .expect("add participant");
        }
        let message = || {
            ChannelMessage::new(
                record.channel.id,
                sender.clone(),
                ChannelPayload::text("request", "hello"),
                ChannelMessageOrigin::new("companion", "dispatch", "agent"),
            )
        };

        let operations = [ChannelOperation::Read, ChannelOperation::Receive]
            .into_iter()
            .collect();
        service
            .update_participant_policy(
                &owner,
                record.channel.id,
                receiver,
                operations,
                ChannelIngressPolicy::Disabled,
                ChannelEgressPolicy::ParticipantsOnly,
            )
            .await
            .expect("disable recipient ingress");
        assert!(
            service
                .plan_broadcast_deliveries(&owner, message())
                .await
                .is_err()
        );

        service
            .close_channel(&owner, record.channel.id, Some("complete".to_string()))
            .await
            .expect("close channel");
        assert!(
            service
                .plan_broadcast_deliveries(&owner, message())
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn participant_projection_returns_visible_channel_refs() {
        let repo = Arc::new(MemoryLifecycleRunRepository::default());
        let run = LifecycleRun::new_plain(Uuid::new_v4());
        LifecycleRunRepository::create(repo.as_ref(), &run)
            .await
            .expect("create run");
        let service = test_service(repo);
        let owner = ChannelOwner::LifecycleRun { run_id: run.id };
        let record = service
            .create_if_absent(
                locator(owner.clone(), "runtime:review"),
                vec!["review".to_string()],
            )
            .await
            .expect("create channel");
        let participant_ref = ChannelParticipantRef::Agent {
            run_id: run.id,
            agent_id: Uuid::new_v4(),
        };
        service
            .add_participant(
                &owner,
                record.channel.id,
                ChannelParticipant::new(participant_ref.clone(), ChannelRole::Member),
            )
            .await
            .expect("add participant");

        let refs = service
            .project_participant_capability(&owner, &participant_ref)
            .await
            .expect("project capability");

        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].aliases, vec!["review"]);
        assert!(refs[0].operations.contains(&ChannelOperation::Reply));
        assert_eq!(refs[0].readiness, ChannelReadiness::Ready);
    }

    #[tokio::test]
    async fn service_exposes_semantic_update_and_remove_mutations() {
        let repo = Arc::new(MemoryLifecycleRunRepository::default());
        let run = LifecycleRun::new_plain(Uuid::new_v4());
        LifecycleRunRepository::create(repo.as_ref(), &run)
            .await
            .expect("create run");
        let service = test_service(repo);
        let owner = ChannelOwner::LifecycleRun { run_id: run.id };
        let record = service
            .create_if_absent(locator(owner.clone(), "runtime:mutations"), vec![])
            .await
            .expect("create channel");
        let participant_ref = ChannelParticipantRef::Agent {
            run_id: run.id,
            agent_id: Uuid::new_v4(),
        };
        service
            .add_participant(
                &owner,
                record.channel.id,
                ChannelParticipant::new(participant_ref.clone(), ChannelRole::Member),
            )
            .await
            .expect("add participant");
        let mut binding = ChannelBinding::new("slack", "workspace-1");
        binding.external_room_ref = Some("room-1".to_string());
        let binding_id = binding.binding_id;
        service
            .bind_external_room(&owner, record.channel.id, binding)
            .await
            .expect("bind room");

        let mut policy = ChannelPolicy::default();
        policy.broadcast.include_sender = true;
        let operations = [ChannelOperation::Read, ChannelOperation::Receive]
            .into_iter()
            .collect();
        let registry = service
            .update_policy(&owner, record.channel.id, policy)
            .await
            .expect("update policy");
        assert!(registry.channels[0].channel.policy.broadcast.include_sender);
        let registry = service
            .update_participant_policy(
                &owner,
                record.channel.id,
                participant_ref.clone(),
                operations,
                ChannelIngressPolicy::Disabled,
                ChannelEgressPolicy::Open,
            )
            .await
            .expect("update participant policy");
        assert_eq!(
            registry.channels[0].participants[0].ingress_policy,
            ChannelIngressPolicy::Disabled
        );

        let registry = service
            .unbind_external_room(&owner, record.channel.id, binding_id)
            .await
            .expect("unbind room");
        assert!(registry.channels[0].bindings.is_empty());
        let registry = service
            .remove_participant(&owner, record.channel.id, participant_ref)
            .await
            .expect("remove participant");
        assert!(registry.channels[0].participants.is_empty());
    }

    #[tokio::test]
    async fn indexed_provider_ingress_and_reply_share_service_admission() {
        let repo = Arc::new(MemoryLifecycleRunRepository::default());
        let run = LifecycleRun::new_plain(Uuid::new_v4());
        LifecycleRunRepository::create(repo.as_ref(), &run)
            .await
            .expect("create run");
        let owner = ChannelOwner::LifecycleRun { run_id: run.id };
        let index = Arc::new(InMemoryChannelBindingIndex::default());
        let provider = Arc::new(TestChannelBindingProvider::default());
        let provider_registry = Arc::new(
            ChannelBindingProviderRegistry::new([
                provider.clone() as Arc<dyn ChannelBindingProvider>
            ])
            .expect("provider registry"),
        );
        let service = ChannelService::new(Arc::new(LifecycleRunChannelOwnerStore::new(repo)))
            .with_binding_resolver(Arc::new(IndexedChannelBindingResolver::new(index.clone())))
            .with_provider_registry(provider_registry)
            .with_binding_index(index.clone());
        let record = service
            .create_if_absent(locator(owner.clone(), "im:test:room-1"), vec![])
            .await
            .expect("create channel");
        let external = ChannelParticipantRef::External {
            provider: "test".to_string(),
            external_user_ref: "external-user-1".to_string(),
        };
        let agent = ChannelParticipantRef::Agent {
            run_id: run.id,
            agent_id: Uuid::new_v4(),
        };
        for participant_ref in [external.clone(), agent.clone()] {
            service
                .add_participant(
                    &owner,
                    record.channel.id,
                    ChannelParticipant::new(participant_ref, ChannelRole::Member),
                )
                .await
                .expect("add participant");
        }
        let mut binding = ChannelBinding::new("test", "workspace-1");
        binding.external_room_ref = Some("room-1".to_string());
        service
            .bind_external_room(&owner, record.channel.id, binding.clone())
            .await
            .expect("bind room");
        let outcome = service
            .ingest_provider_event(ChannelProviderInboundEvent {
                provider: "test".to_string(),
                payload: serde_json::json!({
                    "workspace": "workspace-1",
                    "room": "room-1",
                    "event": "event-in-1",
                    "user": "external-user-1",
                    "text": "hello"
                }),
            })
            .await
            .expect("ingest provider event");
        let ChannelIngressOutcome::Resolved { message, .. } = outcome else {
            panic!("provider event must resolve");
        };
        assert_eq!(message.channel_id, record.channel.id);
        assert_eq!(message.provider_event_ref.as_deref(), Some("event-in-1"));

        let mut reply = ChannelMessage::new(
            record.channel.id,
            agent,
            ChannelPayload::text("reply", "ack"),
            ChannelMessageOrigin::new("agent", "reply", "agent"),
        );
        reply.audience = ChannelAudience::Participants {
            participant_refs: vec![external],
        };
        reply.reply_target = Some(ChannelReplyTarget {
            namespace: "test".to_string(),
            route: "room-1".to_string(),
            target_ref: Some("event-in-1".to_string()),
            metadata: None,
        });
        let plan = service
            .plan_external_delivery(&owner, binding.binding_id, ChannelOperation::Reply, reply)
            .await
            .expect("plan reply");
        let mut disabled_binding = binding.clone();
        disabled_binding.status = ChannelBindingStatus::Disabled;
        service
            .bind_external_room(&owner, record.channel.id, disabled_binding)
            .await
            .expect("disable binding");
        assert!(matches!(
            service
                .dispatch_external_delivery(&owner, plan.clone())
                .await,
            Err(ApplicationError::Conflict(message)) if message.contains("not active")
        ));
        assert!(provider.published.lock().await.is_empty());
        service
            .bind_external_room(&owner, record.channel.id, binding.clone())
            .await
            .expect("restore binding");
        let result = service
            .dispatch_external_delivery(&owner, plan)
            .await
            .expect("dispatch reply");

        assert_eq!(result.receipt.provider_event_ref, "event-out-1");
        let registry = service.load_registry(&owner).await.expect("load registry");
        assert_eq!(registry.channels[0].delivery_state, vec![result.state]);
        assert_eq!(provider.published.lock().await.len(), 1);
    }

    #[tokio::test]
    async fn provider_replay_and_unavailable_binding_are_explicit() {
        let provider = TestChannelBindingProvider::default();
        let event = ChannelProviderInboundEvent {
            provider: "test".to_string(),
            payload: serde_json::json!({
                "workspace": "workspace-1",
                "room": "room-1",
                "event": "duplicate-event",
                "user": "external-user-1"
            }),
        };
        provider
            .normalize_inbound(event.clone())
            .await
            .expect("first event");
        assert!(matches!(
            provider.normalize_inbound(event).await,
            Err(ChannelBindingError::Rejected(message)) if message.contains("duplicate")
        ));

        let index = InMemoryChannelBindingIndex::default();
        let owner = ChannelOwner::Project {
            project_id: Uuid::new_v4(),
        };
        let mut binding = ChannelBinding::new("test", "workspace-1");
        binding.status = ChannelBindingStatus::Disabled;
        let mut record = ChannelRecord::new(Channel::new(
            owner.clone(),
            agentdash_domain::channel::ChannelKey::parse("im:test:disabled").unwrap(),
        ));
        record.bindings.push(binding);
        let registry = ChannelRegistryDocument {
            channels: vec![record],
            ..ChannelRegistryDocument::default()
        };
        index
            .replace_owner(&owner, &registry)
            .await
            .expect("project disabled binding");
        assert!(
            index
                .resolve(&ProviderEventKey {
                    provider: "test".to_string(),
                    external_workspace_ref: "workspace-1".to_string(),
                    external_room_ref: None,
                    external_thread_ref: None,
                    provider_event_ref: None,
                })
                .await
                .expect("resolve disabled binding")
                .is_none()
        );
    }

    #[tokio::test]
    async fn binding_index_rebuilds_after_restart_and_tracks_remove_or_disable() {
        let repo = Arc::new(MemoryLifecycleRunRepository::default());
        let run = LifecycleRun::new_plain(Uuid::new_v4());
        LifecycleRunRepository::create(repo.as_ref(), &run)
            .await
            .expect("create run");
        let owner = ChannelOwner::LifecycleRun { run_id: run.id };
        let first_index = Arc::new(InMemoryChannelBindingIndex::default());
        let first_service =
            ChannelService::new(Arc::new(LifecycleRunChannelOwnerStore::new(repo.clone())))
                .with_binding_resolver(Arc::new(IndexedChannelBindingResolver::new(
                    first_index.clone(),
                )))
                .with_binding_index(first_index);
        let record = first_service
            .create_if_absent(locator(owner.clone(), "im:test:restart"), vec![])
            .await
            .expect("create channel");
        let mut binding = ChannelBinding::new("test", "workspace-1");
        binding.external_room_ref = Some("room-restart".to_string());
        first_service
            .bind_external_room(&owner, record.channel.id, binding.clone())
            .await
            .expect("bind room");

        let lookup = ProviderEventKey {
            provider: "test".to_string(),
            external_workspace_ref: "workspace-1".to_string(),
            external_room_ref: Some("room-restart".to_string()),
            external_thread_ref: None,
            provider_event_ref: None,
        };
        let restarted_index = Arc::new(InMemoryChannelBindingIndex::default());
        let restarted_resolver =
            Arc::new(IndexedChannelBindingResolver::new(restarted_index.clone()));
        let restarted_service =
            ChannelService::new(Arc::new(LifecycleRunChannelOwnerStore::new(repo)))
                .with_binding_resolver(restarted_resolver.clone())
                .with_binding_index(restarted_index);
        restarted_service
            .rebuild_binding_index()
            .await
            .expect("rebuild binding index");
        assert!(matches!(
            restarted_resolver.resolve_binding(&lookup).await.unwrap(),
            ChannelBindingResolution::Resolved { channel_id, .. }
                if channel_id == record.channel.id
        ));

        restarted_service
            .unbind_external_room(&owner, record.channel.id, binding.binding_id)
            .await
            .expect("remove binding");
        assert_eq!(
            restarted_resolver.resolve_binding(&lookup).await.unwrap(),
            ChannelBindingResolution::Unresolved
        );

        binding.status = ChannelBindingStatus::Disabled;
        restarted_service
            .bind_external_room(&owner, record.channel.id, binding)
            .await
            .expect("store disabled binding");
        assert_eq!(
            restarted_resolver.resolve_binding(&lookup).await.unwrap(),
            ChannelBindingResolution::Unresolved
        );
    }

    #[tokio::test]
    async fn concurrent_cross_owner_binding_keeps_one_canonical_owner() {
        let repo = Arc::new(MemoryLifecycleRunRepository::default());
        let first_run = LifecycleRun::new_plain(Uuid::new_v4());
        let second_run = LifecycleRun::new_plain(Uuid::new_v4());
        for run in [&first_run, &second_run] {
            LifecycleRunRepository::create(repo.as_ref(), run)
                .await
                .expect("create run");
        }
        let index = Arc::new(InMemoryChannelBindingIndex::default());
        let service = ChannelService::new(Arc::new(LifecycleRunChannelOwnerStore::new(repo)))
            .with_binding_resolver(Arc::new(IndexedChannelBindingResolver::new(index.clone())))
            .with_binding_index(index);
        let first_owner = ChannelOwner::LifecycleRun {
            run_id: first_run.id,
        };
        let second_owner = ChannelOwner::LifecycleRun {
            run_id: second_run.id,
        };
        let first_channel = service
            .create_if_absent(locator(first_owner.clone(), "im:test:first"), vec![])
            .await
            .expect("create first channel");
        let second_channel = service
            .create_if_absent(locator(second_owner.clone(), "im:test:second"), vec![])
            .await
            .expect("create second channel");
        let mut first_binding = ChannelBinding::new("test", "workspace-shared");
        first_binding.external_room_ref = Some("room-shared".to_string());
        let mut second_binding = ChannelBinding::new("test", "workspace-shared");
        second_binding.external_room_ref = Some("room-shared".to_string());

        let (first_result, second_result) = tokio::join!(
            service.bind_external_room(&first_owner, first_channel.channel.id, first_binding,),
            service.bind_external_room(&second_owner, second_channel.channel.id, second_binding,),
        );
        assert!(first_result.is_ok() ^ second_result.is_ok());
        assert!(matches!(
            first_result.as_ref().err().or(second_result.as_ref().err()),
            Some(ApplicationError::Conflict(message)) if message.contains("already owned")
        ));

        let first_registry = service.load_registry(&first_owner).await.unwrap();
        let second_registry = service.load_registry(&second_owner).await.unwrap();
        let binding_count =
            first_registry.channels[0].bindings.len() + second_registry.channels[0].bindings.len();
        assert_eq!(binding_count, 1);
    }

    #[tokio::test]
    async fn failed_registry_mutation_preserves_existing_binding_projection() {
        let repo = Arc::new(MemoryLifecycleRunRepository::default());
        let run = LifecycleRun::new_plain(Uuid::new_v4());
        LifecycleRunRepository::create(repo.as_ref(), &run)
            .await
            .expect("create run");
        let owner = ChannelOwner::LifecycleRun { run_id: run.id };
        let owner_store = Arc::new(FailingChannelOwnerStore::new(repo));
        let index = Arc::new(InMemoryChannelBindingIndex::default());
        let resolver = Arc::new(IndexedChannelBindingResolver::new(index.clone()));
        let service = ChannelService::new(owner_store.clone())
            .with_binding_resolver(resolver.clone())
            .with_binding_index(index);
        let record = service
            .create_if_absent(locator(owner.clone(), "im:test:persist-failure"), vec![])
            .await
            .expect("create channel");
        let mut binding = ChannelBinding::new("test", "workspace-failure");
        binding.external_room_ref = Some("room-failure".to_string());
        service
            .bind_external_room(&owner, record.channel.id, binding.clone())
            .await
            .expect("bind room");
        let lookup = ProviderEventKey {
            provider: "test".to_string(),
            external_workspace_ref: "workspace-failure".to_string(),
            external_room_ref: Some("room-failure".to_string()),
            external_thread_ref: None,
            provider_event_ref: None,
        };

        owner_store.fail_next();
        assert!(matches!(
            service
                .unbind_external_room(&owner, record.channel.id, binding.binding_id)
                .await,
            Err(ApplicationError::Internal(message)) if message.contains("injected")
        ));
        assert!(matches!(
            resolver.resolve_binding(&lookup).await.unwrap(),
            ChannelBindingResolution::Resolved { .. }
        ));
        assert_eq!(
            service.load_registry(&owner).await.unwrap().channels[0]
                .bindings
                .len(),
            1
        );
    }

    #[test]
    fn address_mapper_uses_mailbox_display_key_semantics() {
        let address = ChannelMessageOrigin::new("companion", "dispatch", "agent")
            .with_source_ref("dispatch-1")
            .with_display_label_key("channel.source.companion.dispatch");

        let source = channel_message_origin_to_mailbox_source_identity(&address);

        assert_eq!(source.namespace, "companion");
        assert_eq!(source.kind, "dispatch");
        assert_eq!(source.source_ref.as_deref(), Some("dispatch-1"));
        assert_eq!(
            source.display_label_key,
            "mailbox.source.companion.dispatch"
        );
    }

    #[test]
    fn mailbox_materializer_outputs_message_command_without_queue_state() {
        let service = ChannelService::new(Arc::new(UnsupportedOwnerStore));
        let channel_id = Uuid::new_v4();
        let run_id = Uuid::new_v4();
        let agent_id = Uuid::new_v4();
        let message = ChannelMessage::new(
            channel_id,
            ChannelParticipantRef::Service {
                key: "system".to_string(),
            },
            ChannelPayload::text("dispatch", "review this"),
            ChannelMessageOrigin::new("companion", "dispatch", "agent")
                .with_source_ref("dispatch-1"),
        );
        let intent = ChannelDeliveryIntent::new(
            message,
            ChannelDeliveryTarget::Mailbox { run_id, agent_id },
        );

        let command = service
            .materialize_delivery_to_mailbox(&intent)
            .expect("mailbox materialization");

        assert_eq!(command.message.run_id, run_id);
        assert_eq!(command.message.agent_id, agent_id);
        assert_eq!(command.message.origin, MailboxMessageOrigin::Companion);
        assert_eq!(
            command.message.source_dedup_key,
            Some(format!("channel_delivery:{}", intent.id))
        );
        let payload = command.message.payload_json.expect("payload refs");
        assert_eq!(
            payload["channel"]["channel_id"],
            serde_json::json!(channel_id)
        );
        assert!(payload.get("mailbox_queue_state").is_none());
        assert!(payload.get("gate_payload").is_none());
    }

    #[test]
    fn gate_materializer_returns_refs_without_gate_payload() {
        let service = ChannelService::new(Arc::new(UnsupportedOwnerStore));
        let gate_id = Uuid::new_v4();
        let mut message = ChannelMessage::new(
            Uuid::new_v4(),
            ChannelParticipantRef::Service {
                key: "system".to_string(),
            },
            ChannelPayload::text("response", "done"),
            ChannelMessageOrigin::new("companion", "result", "agent"),
        );
        message.correlation_ref = Some("gate-correlation".to_string());
        let intent = ChannelDeliveryIntent::new(
            message.clone(),
            ChannelDeliveryTarget::LifecycleGate { gate_id },
        );

        let command = service
            .materialize_delivery_to_gate(&intent)
            .expect("gate materialization");

        assert_eq!(command.gate_id, gate_id);
        assert_eq!(command.message_id, message.id);
        assert_eq!(command.correlation_ref.as_deref(), Some("gate-correlation"));
    }

    fn test_service(repo: Arc<MemoryLifecycleRunRepository>) -> ChannelService {
        ChannelService::new(Arc::new(LifecycleRunChannelOwnerStore::new(repo)))
    }

    fn locator(owner: ChannelOwner, key: &str) -> ChannelLocator {
        ChannelLocator::new(
            owner,
            agentdash_domain::channel::ChannelKey::parse(key).unwrap(),
        )
    }

    fn provider_envelope(provider: &str) -> ProviderNeutralInboundEnvelope {
        ProviderNeutralInboundEnvelope {
            key: ProviderEventKey {
                provider: provider.to_string(),
                external_workspace_ref: "workspace-1".to_string(),
                external_room_ref: Some("room-1".to_string()),
                external_thread_ref: None,
                provider_event_ref: Some("event-1".to_string()),
            },
            sender: ChannelParticipantRef::External {
                provider: provider.to_string(),
                external_user_ref: "user-1".to_string(),
            },
            text: Some("hello".to_string()),
            payload: None,
            correlation_ref: None,
            reply_target: None,
        }
    }

    #[derive(Default)]
    struct TestChannelBindingProvider {
        seen_events: Mutex<BTreeSet<String>>,
        published: Mutex<Vec<ChannelOutboundRequest>>,
    }

    #[async_trait]
    impl ChannelBindingProvider for TestChannelBindingProvider {
        fn provider_key(&self) -> &str {
            "test"
        }

        async fn normalize_inbound(
            &self,
            event: ChannelProviderInboundEvent,
        ) -> Result<NormalizedChannelIngress, ChannelBindingError> {
            if event.provider != self.provider_key() {
                return Err(ChannelBindingError::Rejected(format!(
                    "provider {} does not match {}",
                    event.provider,
                    self.provider_key()
                )));
            }
            let required = |key: &str| {
                event
                    .payload
                    .get(key)
                    .and_then(Value::as_str)
                    .map(str::to_string)
                    .ok_or_else(|| ChannelBindingError::Rejected(format!("missing {key}")))
            };
            let provider_event_ref = required("event")?;
            if !self
                .seen_events
                .lock()
                .await
                .insert(provider_event_ref.clone())
            {
                return Err(ChannelBindingError::Rejected(format!(
                    "duplicate provider event {provider_event_ref}"
                )));
            }
            Ok(NormalizedChannelIngress {
                external_workspace_ref: required("workspace")?,
                external_room_ref: Some(required("room")?),
                external_thread_ref: None,
                provider_event_ref,
                sender: ChannelParticipantRef::External {
                    provider: self.provider_key().to_string(),
                    external_user_ref: required("user")?,
                },
                text: event
                    .payload
                    .get("text")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                payload: Some(event.payload),
                correlation_ref: None,
                reply_target: None,
            })
        }

        async fn publish(
            &self,
            request: ChannelOutboundRequest,
        ) -> Result<ChannelProviderReceipt, ChannelBindingError> {
            if request.binding.status != ChannelBindingStatus::Active {
                return Err(ChannelBindingError::Unavailable {
                    provider: self.provider_key().to_string(),
                });
            }
            self.published.lock().await.push(request);
            Ok(ChannelProviderReceipt {
                provider: self.provider_key().to_string(),
                provider_event_ref: "event-out-1".to_string(),
                metadata: None,
            })
        }
    }

    struct StaticBindingResolver {
        resolution: ChannelBindingResolution,
    }

    #[async_trait]
    impl ChannelBindingResolver for StaticBindingResolver {
        async fn resolve_binding(
            &self,
            key: &ProviderEventKey,
        ) -> Result<ChannelBindingResolution, ApplicationError> {
            key.validate()?;
            Ok(self.resolution.clone())
        }
    }

    struct UnsupportedOwnerStore;

    struct FailingChannelOwnerStore {
        inner: LifecycleRunChannelOwnerStore,
        fail_next_mutation: AtomicBool,
    }

    impl FailingChannelOwnerStore {
        fn new(repo: Arc<MemoryLifecycleRunRepository>) -> Self {
            Self {
                inner: LifecycleRunChannelOwnerStore::new(repo),
                fail_next_mutation: AtomicBool::new(false),
            }
        }

        fn fail_next(&self) {
            self.fail_next_mutation.store(true, Ordering::SeqCst);
        }
    }

    #[async_trait]
    impl ChannelOwnerStore for FailingChannelOwnerStore {
        async fn load_registry(
            &self,
            owner: &ChannelOwner,
        ) -> Result<ChannelRegistryDocument, ApplicationError> {
            self.inner.load_registry(owner).await
        }

        async fn mutate_registry(
            &self,
            owner: &ChannelOwner,
            mutation: ChannelRegistryMutation,
        ) -> Result<ChannelRegistryDocument, ApplicationError> {
            if self.fail_next_mutation.swap(false, Ordering::SeqCst) {
                return Err(ApplicationError::Internal(
                    "injected channel registry persistence failure".to_string(),
                ));
            }
            self.inner.mutate_registry(owner, mutation).await
        }

        async fn list_binding_registries(
            &self,
        ) -> Result<Vec<(ChannelOwner, ChannelRegistryDocument)>, ApplicationError> {
            self.inner.list_binding_registries().await
        }
    }

    #[async_trait]
    impl ChannelOwnerStore for UnsupportedOwnerStore {
        async fn load_registry(
            &self,
            owner: &ChannelOwner,
        ) -> Result<ChannelRegistryDocument, ApplicationError> {
            Err(ApplicationError::InvalidConfig(owner.stable_key()))
        }

        async fn mutate_registry(
            &self,
            owner: &ChannelOwner,
            _mutation: ChannelRegistryMutation,
        ) -> Result<ChannelRegistryDocument, ApplicationError> {
            Err(ApplicationError::InvalidConfig(owner.stable_key()))
        }

        async fn list_binding_registries(
            &self,
        ) -> Result<Vec<(ChannelOwner, ChannelRegistryDocument)>, ApplicationError> {
            Err(ApplicationError::InvalidConfig(
                "unsupported owner store".to_string(),
            ))
        }
    }
}
