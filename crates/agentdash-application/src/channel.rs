use std::collections::BTreeSet;
use std::sync::Arc;

use agentdash_domain::agent_run_mailbox::{
    ConsumptionBarrier, MailboxDelivery, MailboxDrainMode, MailboxMessageOrigin,
    NewAgentRunMailboxMessage,
};
use agentdash_domain::channel::{
    Channel, ChannelAddress, ChannelBinding, ChannelBindingId, ChannelBindingStatus,
    ChannelCapabilityRef, ChannelDeliveryIntent, ChannelDeliveryState, ChannelDeliveryTarget,
    ChannelEgressPolicy, ChannelIngressPolicy, ChannelMessage, ChannelOperation, ChannelOwner,
    ChannelParticipant, ChannelParticipantRef, ChannelPolicy, ChannelReadiness, ChannelRecord,
    ChannelRef, ChannelRegistryDocument, ChannelRegistryMutation, ChannelTopology,
    channel_address_to_mailbox_source_identity,
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
    pub address: ChannelAddress,
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
        message: Box<ChannelMessage>,
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

    pub async fn create_runtime_channel(
        &self,
        owner: ChannelOwner,
        topology: ChannelTopology,
        aliases: Vec<String>,
    ) -> Result<ChannelRecord, ApplicationError> {
        let mut channel = Channel::new(
            owner.clone(),
            agentdash_domain::channel::ChannelMedium::Runtime,
            topology,
        );
        channel.aliases = aliases;
        let record = ChannelRecord::new(channel);
        self.upsert_channel(record.clone()).await?;
        Ok(record)
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
                let mut address =
                    ChannelAddress::new(format!("im.{}", envelope.key.provider), kind, "external")
                        .with_correlation_ref(
                            envelope
                                .correlation_ref
                                .clone()
                                .or(envelope.key.provider_event_ref.clone())
                                .unwrap_or_else(|| binding.binding_id.to_string()),
                        );
                if let Some(event_ref) = &envelope.key.provider_event_ref {
                    address = address.with_source_ref(event_ref);
                }
                let mut message = ChannelMessage::new(
                    channel_id,
                    envelope.sender,
                    agentdash_domain::channel::ChannelPayload {
                        kind: "provider_event".to_string(),
                        text: envelope.text,
                        data: envelope.payload,
                    },
                    address,
                );
                message.provider_event_ref = envelope.key.provider_event_ref;
                message.correlation_ref = envelope.correlation_ref;
                Ok(ChannelIngressOutcome::Resolved {
                    owner,
                    message: Box::new(message),
                })
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
        let source = channel_address_to_mailbox_source_identity(&intent.message.address);
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
                id: None,
                run_id,
                agent_id,
                origin: mailbox_origin_from_channel_address(&intent.message.address),
                source,
                delivery: MailboxDelivery::LaunchOrContinueTurn,
                barrier: ConsumptionBarrier::ImmediateIfIdle,
                drain_mode: MailboxDrainMode::One,
                priority: 0,
                source_dedup_key: Some(format!("channel_delivery:{}", intent.id)),
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
            address: intent.message.address.clone(),
        })
    }
}

fn mailbox_origin_from_channel_address(address: &ChannelAddress) -> MailboxMessageOrigin {
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
    message
        .correlation_ref
        .clone()
        .or_else(|| message.address.correlation_ref.clone())
}

fn participants_for_message(
    record: &ChannelRecord,
    message: &ChannelMessage,
) -> Result<Vec<ChannelParticipant>, ApplicationError> {
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

fn participant_to_delivery_target(
    participant: ChannelParticipant,
) -> Option<ChannelDeliveryTarget> {
    match participant.participant_ref {
        ChannelParticipantRef::AgentRun { run_id, agent_id }
        | ChannelParticipantRef::LifecycleAgent { run_id, agent_id } => {
            Some(ChannelDeliveryTarget::Mailbox { run_id, agent_id })
        }
        ChannelParticipantRef::User { user_id } | ChannelParticipantRef::Human { user_id } => {
            Some(ChannelDeliveryTarget::Notification { user_id })
        }
        ChannelParticipantRef::Platform { key } => {
            Some(ChannelDeliveryTarget::Platform { broker_key: key })
        }
        ChannelParticipantRef::External { .. } | ChannelParticipantRef::System { .. } => None,
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
        ChannelEgressPolicy, ChannelIngressPolicy, ChannelMedium, ChannelOperation, ChannelPayload,
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
            .create_runtime_channel(
                owner.clone(),
                ChannelTopology::Direct,
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
            .create_runtime_channel(owner.clone(), ChannelTopology::Group, vec![])
            .await
            .expect("create channel");
        let sender = ChannelParticipantRef::AgentRun {
            run_id: run.id,
            agent_id: Uuid::new_v4(),
        };
        let receiver = ChannelParticipantRef::AgentRun {
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
            ChannelAddress::new("companion", "dispatch", "agent"),
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
    async fn participant_projection_returns_visible_channel_refs() {
        let repo = Arc::new(MemoryLifecycleRunRepository::default());
        let run = LifecycleRun::new_plain(Uuid::new_v4());
        LifecycleRunRepository::create(repo.as_ref(), &run)
            .await
            .expect("create run");
        let service = test_service(repo);
        let owner = ChannelOwner::LifecycleRun { run_id: run.id };
        let record = service
            .create_runtime_channel(
                owner.clone(),
                ChannelTopology::Direct,
                vec!["review".to_string()],
            )
            .await
            .expect("create channel");
        let participant_ref = ChannelParticipantRef::AgentRun {
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
            .create_runtime_channel(owner.clone(), ChannelTopology::Direct, vec![])
            .await
            .expect("create channel");
        let participant_ref = ChannelParticipantRef::AgentRun {
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
            ChannelOwner::System,
            ChannelMedium::Im,
            ChannelTopology::Thread,
        );
        let message = ChannelMessage::new(
            channel.id,
            ChannelParticipantRef::System {
                key: "system".to_string(),
            },
            ChannelPayload::text("response", "ok"),
            ChannelAddress::new("im.slack", "thread_reply", "agent"),
        );

        let intent = service
            .publish_outbox_intent(&binding, message)
            .expect("publish intent");

        assert_eq!(intent.provider, "slack");
        assert_eq!(intent.external_room_ref.as_deref(), Some("room-1"));
    }

    #[test]
    fn address_mapper_uses_mailbox_display_key_semantics() {
        let address = ChannelAddress::new("companion", "dispatch", "agent")
            .with_source_ref("dispatch-1")
            .with_correlation_ref("dispatch-1")
            .with_route("child")
            .with_display_label_key("channel.source.companion.dispatch");

        let source = channel_address_to_mailbox_source_identity(&address);

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
            ChannelParticipantRef::System {
                key: "system".to_string(),
            },
            ChannelPayload::text("dispatch", "review this"),
            ChannelAddress::new("companion", "dispatch", "agent")
                .with_source_ref("dispatch-1")
                .with_correlation_ref("dispatch-1"),
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
        let message = ChannelMessage::new(
            Uuid::new_v4(),
            ChannelParticipantRef::System {
                key: "system".to_string(),
            },
            ChannelPayload::text("response", "done"),
            ChannelAddress::new("companion", "result", "agent")
                .with_correlation_ref("gate-correlation"),
        );
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
