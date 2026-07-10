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
    channel_message_origin_to_mailbox_source_identity,
};
use agentdash_domain::workflow::LifecycleRunRepository;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::ApplicationError;

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
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProviderNeutralPublishIntent {
    pub binding_id: ChannelBindingId,
    pub provider: String,
    pub external_workspace_ref: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub external_room_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub external_thread_ref: Option<String>,
    pub message: ChannelMessage,
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
    Unsupported {
        provider: String,
    },
}

#[async_trait]
pub trait ChannelBindingResolver: Send + Sync {
    async fn resolve_binding(
        &self,
        key: &ProviderEventKey,
    ) -> Result<ChannelBindingResolution, ApplicationError>;
}

pub struct UnsupportedChannelBindingResolver;

#[async_trait]
impl ChannelBindingResolver for UnsupportedChannelBindingResolver {
    async fn resolve_binding(
        &self,
        key: &ProviderEventKey,
    ) -> Result<ChannelBindingResolution, ApplicationError> {
        key.validate()?;
        Ok(ChannelBindingResolution::Unsupported {
            provider: key.provider.clone(),
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ChannelIngressOutcome {
    Resolved {
        owner: ChannelOwner,
        message: ChannelMessage,
    },
    Unresolved,
    Unsupported {
        provider: String,
    },
}

pub struct ChannelService {
    owner_store: Arc<dyn ChannelOwnerStore>,
    binding_resolver: Arc<dyn ChannelBindingResolver>,
}

impl ChannelService {
    pub fn new(
        owner_store: Arc<dyn ChannelOwnerStore>,
        binding_resolver: Arc<dyn ChannelBindingResolver>,
    ) -> Self {
        Self {
            owner_store,
            binding_resolver,
        }
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
        self.owner_store
            .mutate_registry(&owner, ChannelRegistryMutation::UpsertChannel(record))
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
            .owner_store
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
        self.owner_store
            .mutate_registry(
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
        self.owner_store
            .mutate_registry(
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
        self.owner_store
            .mutate_registry(
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
        self.owner_store
            .mutate_registry(
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
        self.owner_store
            .mutate_registry(
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
        self.owner_store
            .mutate_registry(
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
        self.owner_store
            .mutate_registry(
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
        match self.binding_resolver.resolve_binding(&envelope.key).await? {
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
            ChannelBindingResolution::Unsupported { provider } => {
                Ok(ChannelIngressOutcome::Unsupported { provider })
            }
        }
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
        self.owner_store
            .mutate_registry(
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

    pub fn publish_outbox_intent(
        &self,
        binding: &ChannelBinding,
        message: ChannelMessage,
    ) -> Result<ProviderNeutralPublishIntent, ApplicationError> {
        binding.validate().map_err(ApplicationError::from)?;
        if binding.status != ChannelBindingStatus::Active {
            return Err(ApplicationError::Conflict(format!(
                "channel binding {} is not active",
                binding.binding_id
            )));
        }
        Ok(ProviderNeutralPublishIntent {
            binding_id: binding.binding_id,
            provider: binding.provider.clone(),
            external_workspace_ref: binding.external_workspace_ref.clone(),
            external_room_ref: binding.external_room_ref.clone(),
            external_thread_ref: binding.external_thread_ref.clone(),
            message,
        })
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
    use agentdash_domain::channel::{
        ChannelEgressPolicy, ChannelIngressPolicy, ChannelKey, ChannelOperation, ChannelPayload,
        ChannelPolicy, ChannelRole,
    };
    use agentdash_domain::workflow::{LifecycleRun, LifecycleRunRepository};
    use agentdash_test_support::workflow::MemoryLifecycleRunRepository;

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
        let service = ChannelService::new(
            Arc::new(LifecycleRunChannelOwnerStore::new(repo)),
            Arc::new(StaticBindingResolver {
                resolution: ChannelBindingResolution::Unresolved,
            }),
        );

        let outcome = service
            .ingest_external_event(provider_envelope("slack"))
            .await
            .expect("ingest");
        assert_eq!(outcome, ChannelIngressOutcome::Unresolved);
    }

    #[tokio::test]
    async fn unsupported_binding_is_explicit() {
        let repo = Arc::new(MemoryLifecycleRunRepository::default());
        let service = ChannelService::new(
            Arc::new(LifecycleRunChannelOwnerStore::new(repo)),
            Arc::new(UnsupportedChannelBindingResolver),
        );

        let outcome = service
            .ingest_external_event(provider_envelope("feishu"))
            .await
            .expect("ingest");
        assert_eq!(
            outcome,
            ChannelIngressOutcome::Unsupported {
                provider: "feishu".to_string()
            }
        );
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

    #[test]
    fn publish_outbox_intent_is_provider_neutral() {
        let service = ChannelService::new(
            Arc::new(UnsupportedOwnerStore),
            Arc::new(UnsupportedChannelBindingResolver),
        );
        let mut binding = ChannelBinding::new("slack", "workspace-1");
        binding.external_room_ref = Some("room-1".to_string());
        let channel = Channel::new(
            ChannelOwner::Project {
                project_id: Uuid::new_v4(),
            },
            ChannelKey::parse("im:slack:thread").unwrap(),
        );
        let message = ChannelMessage::new(
            channel.id,
            ChannelParticipantRef::Service {
                key: "system".to_string(),
            },
            ChannelPayload::text("response", "ok"),
            ChannelMessageOrigin::new("im.slack", "thread_reply", "agent"),
        );

        let intent = service
            .publish_outbox_intent(&binding, message)
            .expect("publish intent");

        assert_eq!(intent.provider, "slack");
        assert_eq!(intent.external_room_ref.as_deref(), Some("room-1"));
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
        let service = ChannelService::new(
            Arc::new(UnsupportedOwnerStore),
            Arc::new(UnsupportedChannelBindingResolver),
        );
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
        let service = ChannelService::new(
            Arc::new(UnsupportedOwnerStore),
            Arc::new(UnsupportedChannelBindingResolver),
        );
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
        ChannelService::new(
            Arc::new(LifecycleRunChannelOwnerStore::new(repo)),
            Arc::new(UnsupportedChannelBindingResolver),
        )
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
    }
}
