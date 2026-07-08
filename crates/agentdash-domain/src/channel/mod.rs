use std::collections::BTreeSet;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::DomainError;
use crate::agent_run_mailbox::MailboxSourceIdentity;

pub const CHANNEL_REGISTRY_SCHEMA_VERSION: u32 = 1;

pub type ChannelId = Uuid;
pub type ChannelBindingId = Uuid;
pub type ChannelMessageId = Uuid;
pub type ChannelDeliveryId = Uuid;
pub type ChannelReplyAddressId = Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChannelRegistryDocument {
    #[serde(default = "default_channel_registry_schema_version")]
    pub schema_version: u32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub channels: Vec<ChannelRecord>,
}

impl Default for ChannelRegistryDocument {
    fn default() -> Self {
        Self {
            schema_version: CHANNEL_REGISTRY_SCHEMA_VERSION,
            channels: Vec::new(),
        }
    }
}

impl ChannelRegistryDocument {
    pub fn apply(&mut self, mutation: ChannelRegistryMutation) -> Result<(), DomainError> {
        match mutation {
            ChannelRegistryMutation::UpsertChannel(record) => {
                record.validate()?;
                match self
                    .channels
                    .iter_mut()
                    .find(|existing| existing.channel.id == record.channel.id)
                {
                    Some(existing) => *existing = record,
                    None => self.channels.push(record),
                }
            }
            ChannelRegistryMutation::CloseChannel { channel_id, reason } => {
                let record = self.channel_mut(channel_id)?;
                record.channel.status = ChannelStatus::Closed;
                record.channel.closed_at = Some(Utc::now());
                record.channel.close_reason = reason;
                record.channel.updated_at = Utc::now();
            }
            ChannelRegistryMutation::UpdateChannelPolicy { channel_id, policy } => {
                policy.validate()?;
                let record = self.channel_mut(channel_id)?;
                record.channel.policy = policy;
                record.channel.updated_at = Utc::now();
            }
            ChannelRegistryMutation::AddParticipant {
                channel_id,
                participant,
            } => {
                participant.validate()?;
                let record = self.channel_mut(channel_id)?;
                match record
                    .participants
                    .iter_mut()
                    .find(|existing| existing.participant_ref == participant.participant_ref)
                {
                    Some(existing) => *existing = participant,
                    None => record.participants.push(participant),
                }
                record.channel.updated_at = Utc::now();
            }
            ChannelRegistryMutation::RemoveParticipant {
                channel_id,
                participant_ref,
            } => {
                let record = self.channel_mut(channel_id)?;
                let before = record.participants.len();
                record
                    .participants
                    .retain(|participant| participant.participant_ref != participant_ref);
                if before == record.participants.len() {
                    return Err(not_found(
                        "channel_participant",
                        participant_ref.stable_key(),
                    ));
                }
                record.channel.updated_at = Utc::now();
            }
            ChannelRegistryMutation::UpdateParticipantPolicy {
                channel_id,
                participant_ref,
                operations,
                ingress_policy,
                egress_policy,
            } => {
                if operations.is_empty() {
                    return Err(invalid_config(
                        "channel participant operations must not be empty",
                    ));
                }
                let record = self.channel_mut(channel_id)?;
                let participant = record
                    .participants
                    .iter_mut()
                    .find(|participant| participant.participant_ref == participant_ref)
                    .ok_or_else(|| {
                        not_found("channel_participant", participant_ref.stable_key())
                    })?;
                participant.operations = operations;
                participant.ingress_policy = ingress_policy;
                participant.egress_policy = egress_policy;
                record.channel.updated_at = Utc::now();
            }
            ChannelRegistryMutation::UpsertBinding {
                channel_id,
                binding,
            } => {
                binding.validate()?;
                let record = self.channel_mut(channel_id)?;
                match record
                    .bindings
                    .iter_mut()
                    .find(|existing| existing.binding_id == binding.binding_id)
                {
                    Some(existing) => *existing = binding,
                    None => record.bindings.push(binding),
                }
                record.channel.updated_at = Utc::now();
            }
            ChannelRegistryMutation::RemoveBinding {
                channel_id,
                binding_ref,
            } => {
                let record = self.channel_mut(channel_id)?;
                let before = record.bindings.len();
                record
                    .bindings
                    .retain(|binding| binding.binding_id != binding_ref);
                if before == record.bindings.len() {
                    return Err(not_found("channel_binding", binding_ref.to_string()));
                }
                record.channel.updated_at = Utc::now();
            }
            ChannelRegistryMutation::RecordDeliveryState { channel_id, state } => {
                let record = self.channel_mut(channel_id)?;
                match record
                    .delivery_state
                    .iter_mut()
                    .find(|existing| existing.delivery_id == state.delivery_id)
                {
                    Some(existing) => *existing = state,
                    None => record.delivery_state.push(state),
                }
                let max_items = record.channel.policy.delivery.max_delivery_state_items;
                prune_delivery_states(&mut record.delivery_state, None, max_items);
                record.channel.updated_at = Utc::now();
            }
            ChannelRegistryMutation::PruneDeliveryState {
                channel_id,
                before,
                max_items,
            } => {
                let record = self.channel_mut(channel_id)?;
                prune_delivery_states(&mut record.delivery_state, Some(before), max_items);
                record.channel.updated_at = Utc::now();
            }
        }

        self.validate()
    }

    pub fn channel(&self, channel_id: ChannelId) -> Result<&ChannelRecord, DomainError> {
        self.channels
            .iter()
            .find(|record| record.channel.id == channel_id)
            .ok_or_else(|| not_found("channel", channel_id.to_string()))
    }

    pub fn validate(&self) -> Result<(), DomainError> {
        if self.schema_version != CHANNEL_REGISTRY_SCHEMA_VERSION {
            return Err(invalid_config(format!(
                "channel registry schema_version {} is unsupported",
                self.schema_version
            )));
        }
        let mut channel_ids = BTreeSet::new();
        for record in &self.channels {
            if !channel_ids.insert(record.channel.id) {
                return Err(conflict(
                    "channel_registry",
                    "channel_id",
                    format!("duplicate channel id {}", record.channel.id),
                ));
            }
            record.validate()?;
        }
        Ok(())
    }

    fn channel_mut(&mut self, channel_id: ChannelId) -> Result<&mut ChannelRecord, DomainError> {
        self.channels
            .iter_mut()
            .find(|record| record.channel.id == channel_id)
            .ok_or_else(|| not_found("channel", channel_id.to_string()))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ChannelRegistryMutation {
    UpsertChannel(ChannelRecord),
    CloseChannel {
        channel_id: ChannelId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
    },
    UpdateChannelPolicy {
        channel_id: ChannelId,
        policy: ChannelPolicy,
    },
    AddParticipant {
        channel_id: ChannelId,
        participant: ChannelParticipant,
    },
    RemoveParticipant {
        channel_id: ChannelId,
        participant_ref: ChannelParticipantRef,
    },
    UpdateParticipantPolicy {
        channel_id: ChannelId,
        participant_ref: ChannelParticipantRef,
        operations: BTreeSet<ChannelOperation>,
        ingress_policy: ChannelIngressPolicy,
        egress_policy: ChannelEgressPolicy,
    },
    UpsertBinding {
        channel_id: ChannelId,
        binding: ChannelBinding,
    },
    RemoveBinding {
        channel_id: ChannelId,
        binding_ref: ChannelBindingId,
    },
    RecordDeliveryState {
        channel_id: ChannelId,
        state: ChannelDeliveryState,
    },
    PruneDeliveryState {
        channel_id: ChannelId,
        before: DateTime<Utc>,
        max_items: usize,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChannelRecord {
    pub channel: Channel,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub participants: Vec<ChannelParticipant>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bindings: Vec<ChannelBinding>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reply_addresses: Vec<ChannelReplyAddress>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub delivery_state: Vec<ChannelDeliveryState>,
}

impl ChannelRecord {
    pub fn new(channel: Channel) -> Self {
        Self {
            channel,
            participants: Vec::new(),
            bindings: Vec::new(),
            reply_addresses: Vec::new(),
            delivery_state: Vec::new(),
        }
    }

    pub fn validate(&self) -> Result<(), DomainError> {
        self.channel.validate()?;
        let mut participant_refs = BTreeSet::new();
        for participant in &self.participants {
            participant.validate()?;
            if !participant_refs.insert(participant.participant_ref.clone()) {
                return Err(conflict(
                    "channel_record",
                    "participant_ref",
                    format!(
                        "duplicate participant {}",
                        participant.participant_ref.stable_key()
                    ),
                ));
            }
        }

        let mut binding_ids = BTreeSet::new();
        for binding in &self.bindings {
            binding.validate()?;
            if !binding_ids.insert(binding.binding_id) {
                return Err(conflict(
                    "channel_record",
                    "binding_id",
                    format!("duplicate binding {}", binding.binding_id),
                ));
            }
        }

        let mut reply_address_ids = BTreeSet::new();
        for address in &self.reply_addresses {
            address.validate()?;
            if !reply_address_ids.insert(address.address_id) {
                return Err(conflict(
                    "channel_record",
                    "reply_address_id",
                    format!("duplicate reply address {}", address.address_id),
                ));
            }
        }

        let mut delivery_ids = BTreeSet::new();
        for state in &self.delivery_state {
            if !delivery_ids.insert(state.delivery_id) {
                return Err(conflict(
                    "channel_record",
                    "delivery_id",
                    format!("duplicate delivery state {}", state.delivery_id),
                ));
            }
        }

        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Channel {
    pub id: ChannelId,
    pub owner: ChannelOwner,
    pub medium: ChannelMedium,
    pub topology: ChannelTopology,
    pub lifecycle: ChannelLifecycle,
    pub status: ChannelStatus,
    pub policy: ChannelPolicy,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub aliases: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub closed_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub close_reason: Option<String>,
}

impl Channel {
    pub fn new(owner: ChannelOwner, medium: ChannelMedium, topology: ChannelTopology) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            owner,
            medium,
            topology,
            lifecycle: ChannelLifecycle::Runtime,
            status: ChannelStatus::Open,
            policy: ChannelPolicy::default(),
            aliases: Vec::new(),
            created_at: now,
            updated_at: now,
            closed_at: None,
            close_reason: None,
        }
    }

    pub fn validate(&self) -> Result<(), DomainError> {
        self.owner.validate()?;
        self.policy.validate()?;
        if self.status == ChannelStatus::Open && self.closed_at.is_some() {
            return Err(invalid_config("open channel must not have closed_at"));
        }
        for alias in &self.aliases {
            validate_non_empty("channel.alias", alias)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ChannelOwner {
    Project { project_id: Uuid },
    Story { story_id: Uuid },
    LifecycleRun { run_id: Uuid },
    System,
}

impl ChannelOwner {
    pub fn validate(&self) -> Result<(), DomainError> {
        Ok(())
    }

    pub fn stable_key(&self) -> String {
        match self {
            Self::Project { project_id } => format!("project:{project_id}"),
            Self::Story { story_id } => format!("story:{story_id}"),
            Self::LifecycleRun { run_id } => format!("lifecycle_run:{run_id}"),
            Self::System => "system".to_string(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChannelMedium {
    Runtime,
    Project,
    Im,
    Human,
    Terminal,
    System,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChannelTopology {
    Direct,
    Group,
    Broadcast,
    Thread,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChannelLifecycle {
    Runtime,
    Persistent,
    Ephemeral,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChannelStatus {
    Open,
    Closed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChannelPolicy {
    pub broadcast: ChannelBroadcastPolicy,
    pub delivery: ChannelDeliveryPolicy,
    #[serde(default)]
    pub default_operations: BTreeSet<ChannelOperation>,
}

impl Default for ChannelPolicy {
    fn default() -> Self {
        Self {
            broadcast: ChannelBroadcastPolicy::default(),
            delivery: ChannelDeliveryPolicy::default(),
            default_operations: default_participant_operations(),
        }
    }
}

impl ChannelPolicy {
    pub fn validate(&self) -> Result<(), DomainError> {
        if self.default_operations.is_empty() {
            return Err(invalid_config(
                "channel policy default_operations must not be empty",
            ));
        }
        self.delivery.validate()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChannelBroadcastPolicy {
    pub audience: ChannelAudience,
    pub include_sender: bool,
}

impl Default for ChannelBroadcastPolicy {
    fn default() -> Self {
        Self {
            audience: ChannelAudience::AllParticipants,
            include_sender: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChannelDeliveryPolicy {
    #[serde(default)]
    pub require_ack: bool,
    #[serde(default = "default_delivery_state_max_items")]
    pub max_delivery_state_items: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dedup_key: Option<String>,
}

impl Default for ChannelDeliveryPolicy {
    fn default() -> Self {
        Self {
            require_ack: false,
            max_delivery_state_items: default_delivery_state_max_items(),
            dedup_key: None,
        }
    }
}

impl ChannelDeliveryPolicy {
    pub fn validate(&self) -> Result<(), DomainError> {
        if self.max_delivery_state_items == 0 {
            return Err(invalid_config(
                "channel delivery max_delivery_state_items must be greater than zero",
            ));
        }
        if let Some(dedup_key) = &self.dedup_key {
            validate_non_empty("channel.delivery.dedup_key", dedup_key)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ChannelParticipantRef {
    AgentRun {
        run_id: Uuid,
        agent_id: Uuid,
    },
    LifecycleAgent {
        run_id: Uuid,
        agent_id: Uuid,
    },
    User {
        user_id: String,
    },
    Human {
        user_id: String,
    },
    External {
        provider: String,
        external_user_ref: String,
    },
    System {
        key: String,
    },
    Platform {
        key: String,
    },
}

impl ChannelParticipantRef {
    pub fn validate(&self) -> Result<(), DomainError> {
        match self {
            Self::AgentRun { .. } | Self::LifecycleAgent { .. } => Ok(()),
            Self::User { user_id } | Self::Human { user_id } => {
                validate_non_empty("channel.participant.user_id", user_id)
            }
            Self::External {
                provider,
                external_user_ref,
            } => {
                validate_non_empty("channel.participant.provider", provider)?;
                validate_non_empty("channel.participant.external_user_ref", external_user_ref)
            }
            Self::System { key } | Self::Platform { key } => {
                validate_non_empty("channel.participant.key", key)
            }
        }
    }

    pub fn stable_key(&self) -> String {
        match self {
            Self::AgentRun { run_id, agent_id } => {
                format!("agent_run:{run_id}:{agent_id}")
            }
            Self::LifecycleAgent { run_id, agent_id } => {
                format!("lifecycle_agent:{run_id}:{agent_id}")
            }
            Self::User { user_id } => format!("user:{user_id}"),
            Self::Human { user_id } => format!("human:{user_id}"),
            Self::External {
                provider,
                external_user_ref,
            } => format!("external:{provider}:{external_user_ref}"),
            Self::System { key } => format!("system:{key}"),
            Self::Platform { key } => format!("platform:{key}"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChannelParticipant {
    pub participant_ref: ChannelParticipantRef,
    pub role: ChannelRole,
    pub operations: BTreeSet<ChannelOperation>,
    pub ingress_policy: ChannelIngressPolicy,
    pub egress_policy: ChannelEgressPolicy,
    pub joined_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub left_at: Option<DateTime<Utc>>,
}

impl ChannelParticipant {
    pub fn new(participant_ref: ChannelParticipantRef, role: ChannelRole) -> Self {
        Self {
            participant_ref,
            role,
            operations: default_participant_operations(),
            ingress_policy: ChannelIngressPolicy::ParticipantsOnly,
            egress_policy: ChannelEgressPolicy::ParticipantsOnly,
            joined_at: Utc::now(),
            left_at: None,
        }
    }

    pub fn validate(&self) -> Result<(), DomainError> {
        self.participant_ref.validate()?;
        if self.operations.is_empty() {
            return Err(invalid_config(
                "channel participant operations must not be empty",
            ));
        }
        if let Some(left_at) = self.left_at {
            if left_at < self.joined_at {
                return Err(invalid_config(
                    "channel participant left_at must not be before joined_at",
                ));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChannelRole {
    Owner,
    Member,
    Observer,
    External,
    System,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChannelOperation {
    Read,
    Receive,
    Reply,
    Publish,
    Broadcast,
    ManageParticipants,
    ManageBindings,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChannelIngressPolicy {
    Open,
    ParticipantsOnly,
    Disabled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChannelEgressPolicy {
    Open,
    ParticipantsOnly,
    Disabled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChannelBinding {
    pub binding_id: ChannelBindingId,
    pub provider: String,
    pub external_workspace_ref: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub external_room_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub external_thread_ref: Option<String>,
    pub identity_mapping_policy: ChannelIdentityMappingPolicy,
    pub status: ChannelBindingStatus,
}

impl ChannelBinding {
    pub fn new(provider: impl Into<String>, external_workspace_ref: impl Into<String>) -> Self {
        Self {
            binding_id: Uuid::new_v4(),
            provider: provider.into(),
            external_workspace_ref: external_workspace_ref.into(),
            external_room_ref: None,
            external_thread_ref: None,
            identity_mapping_policy: ChannelIdentityMappingPolicy::ProviderUserRef,
            status: ChannelBindingStatus::Active,
        }
    }

    pub fn validate(&self) -> Result<(), DomainError> {
        validate_non_empty("channel.binding.provider", &self.provider)?;
        validate_non_empty(
            "channel.binding.external_workspace_ref",
            &self.external_workspace_ref,
        )?;
        if let Some(room_ref) = &self.external_room_ref {
            validate_non_empty("channel.binding.external_room_ref", room_ref)?;
        }
        if let Some(thread_ref) = &self.external_thread_ref {
            validate_non_empty("channel.binding.external_thread_ref", thread_ref)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChannelIdentityMappingPolicy {
    ProviderUserRef,
    AgentDashUser,
    ExternalOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChannelBindingStatus {
    Active,
    Disabled,
    Unresolved,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChannelReplyAddress {
    pub address_id: ChannelReplyAddressId,
    pub participant_ref: ChannelParticipantRef,
    pub address: ChannelAddress,
    pub status: ChannelReplyAddressStatus,
    pub created_at: DateTime<Utc>,
}

impl ChannelReplyAddress {
    pub fn validate(&self) -> Result<(), DomainError> {
        self.participant_ref.validate()?;
        self.address.validate()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChannelReplyAddressStatus {
    Active,
    Revoked,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChannelAddress {
    pub namespace: String,
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub correlation_ref: Option<String>,
    pub actor: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub route: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_label_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Value>,
}

impl ChannelAddress {
    pub fn new(
        namespace: impl Into<String>,
        kind: impl Into<String>,
        actor: impl Into<String>,
    ) -> Self {
        Self {
            namespace: namespace.into(),
            kind: kind.into(),
            source_ref: None,
            correlation_ref: None,
            actor: actor.into(),
            route: None,
            display_label_key: None,
            metadata: None,
        }
    }

    pub fn with_source_ref(mut self, source_ref: impl Into<String>) -> Self {
        self.source_ref = Some(source_ref.into());
        self
    }

    pub fn with_correlation_ref(mut self, correlation_ref: impl Into<String>) -> Self {
        self.correlation_ref = Some(correlation_ref.into());
        self
    }

    pub fn with_route(mut self, route: impl Into<String>) -> Self {
        self.route = Some(route.into());
        self
    }

    pub fn with_display_label_key(mut self, display_label_key: impl Into<String>) -> Self {
        self.display_label_key = Some(display_label_key.into());
        self
    }

    pub fn with_metadata(mut self, metadata: Value) -> Self {
        self.metadata = Some(metadata);
        self
    }

    pub fn validate(&self) -> Result<(), DomainError> {
        validate_non_empty("channel.address.namespace", &self.namespace)?;
        validate_non_empty("channel.address.kind", &self.kind)?;
        validate_non_empty("channel.address.actor", &self.actor)?;
        if let Some(source_ref) = &self.source_ref {
            validate_non_empty("channel.address.source_ref", source_ref)?;
        }
        if let Some(correlation_ref) = &self.correlation_ref {
            validate_non_empty("channel.address.correlation_ref", correlation_ref)?;
        }
        if let Some(route) = &self.route {
            validate_non_empty("channel.address.route", route)?;
        }
        if let Some(display_label_key) = &self.display_label_key {
            validate_non_empty("channel.address.display_label_key", display_label_key)?;
        }
        Ok(())
    }
}

pub fn channel_address_to_mailbox_source_identity(
    address: &ChannelAddress,
) -> MailboxSourceIdentity {
    let mut source = MailboxSourceIdentity::new(
        address.namespace.clone(),
        address.kind.clone(),
        address.actor.clone(),
    );
    if let Some(source_ref) = &address.source_ref {
        source = source.with_source_ref(source_ref.clone());
    }
    if let Some(correlation_ref) = &address.correlation_ref {
        source = source.with_correlation_ref(correlation_ref.clone());
    }
    if let Some(route) = &address.route {
        source = source.with_route(route.clone());
    }
    if let Some(metadata) = &address.metadata {
        source = source.with_metadata(metadata.clone());
    }
    source.with_display_label_key(format!(
        "mailbox.source.{}.{}",
        address.namespace, address.kind
    ))
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChannelMessage {
    pub id: ChannelMessageId,
    pub channel_id: ChannelId,
    pub sender: ChannelParticipantRef,
    pub audience: ChannelAudience,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thread_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub correlation_ref: Option<String>,
    pub address: ChannelAddress,
    pub payload: ChannelPayload,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub content_refs: Vec<ChannelContentRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_event_ref: Option<String>,
    pub created_at: DateTime<Utc>,
}

impl ChannelMessage {
    pub fn new(
        channel_id: ChannelId,
        sender: ChannelParticipantRef,
        payload: ChannelPayload,
        address: ChannelAddress,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            channel_id,
            sender,
            audience: ChannelAudience::AllParticipants,
            thread_ref: None,
            correlation_ref: None,
            address,
            payload,
            content_refs: Vec::new(),
            provider_event_ref: None,
            created_at: Utc::now(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ChannelAudience {
    AllParticipants,
    Participants {
        participant_refs: Vec<ChannelParticipantRef>,
    },
    Role {
        role: ChannelRole,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChannelPayload {
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

impl ChannelPayload {
    pub fn text(kind: impl Into<String>, text: impl Into<String>) -> Self {
        Self {
            kind: kind.into(),
            text: Some(text.into()),
            data: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChannelContentRef {
    pub kind: String,
    pub content_ref: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChannelDeliveryIntent {
    pub id: ChannelDeliveryId,
    pub message: ChannelMessage,
    pub target: ChannelDeliveryTarget,
    pub policy: ChannelDeliveryPolicy,
    pub created_at: DateTime<Utc>,
}

impl ChannelDeliveryIntent {
    pub fn new(message: ChannelMessage, target: ChannelDeliveryTarget) -> Self {
        Self {
            id: Uuid::new_v4(),
            message,
            target,
            policy: ChannelDeliveryPolicy::default(),
            created_at: Utc::now(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ChannelDeliveryTarget {
    Mailbox { run_id: Uuid, agent_id: Uuid },
    LifecycleGate { gate_id: Uuid },
    ExternalBinding { binding_id: ChannelBindingId },
    Notification { user_id: String },
    Platform { broker_key: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChannelDeliveryState {
    pub delivery_id: ChannelDeliveryId,
    pub message_id: ChannelMessageId,
    pub target: ChannelDeliveryTarget,
    pub status: ChannelDeliveryStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub materialized_ref: Option<MaterializedDeliveryRef>,
    pub updated_at: DateTime<Utc>,
}

impl ChannelDeliveryState {
    pub fn new(
        delivery_id: ChannelDeliveryId,
        message_id: ChannelMessageId,
        target: ChannelDeliveryTarget,
        status: ChannelDeliveryStatus,
    ) -> Self {
        Self {
            delivery_id,
            message_id,
            target,
            status,
            materialized_ref: None,
            updated_at: Utc::now(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChannelDeliveryStatus {
    Planned,
    Materialized,
    Delivered,
    Failed,
    Skipped,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MaterializedDeliveryRef {
    MailboxMessage { message_id: Uuid },
    LifecycleGate { gate_id: Uuid },
    PublishOutbox { outbox_id: Uuid },
    ProviderEvent { provider: String, event_ref: String },
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ChannelRef {
    pub owner: ChannelOwner,
    pub channel_id: ChannelId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChannelCapabilityRef {
    pub channel_ref: ChannelRef,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub aliases: Vec<String>,
    pub operations: BTreeSet<ChannelOperation>,
    pub ingress_policy: ChannelIngressPolicy,
    pub egress_policy: ChannelEgressPolicy,
    pub readiness: ChannelReadiness,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChannelReadiness {
    Ready,
    UnresolvedBinding,
    UnsupportedProvider,
    Disabled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ChannelDirective {
    Expose {
        channel_ref: ChannelRef,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        aliases: Vec<String>,
        operations: BTreeSet<ChannelOperation>,
    },
    Revoke {
        channel_ref: ChannelRef,
    },
}

fn prune_delivery_states(
    delivery_state: &mut Vec<ChannelDeliveryState>,
    before: Option<DateTime<Utc>>,
    max_items: usize,
) {
    if let Some(before) = before {
        delivery_state.retain(|state| state.updated_at >= before);
    }
    delivery_state.sort_by_key(|state| std::cmp::Reverse(state.updated_at));
    if delivery_state.len() > max_items {
        delivery_state.truncate(max_items);
    }
}

fn default_channel_registry_schema_version() -> u32 {
    CHANNEL_REGISTRY_SCHEMA_VERSION
}

fn default_delivery_state_max_items() -> usize {
    128
}

fn default_participant_operations() -> BTreeSet<ChannelOperation> {
    [
        ChannelOperation::Read,
        ChannelOperation::Receive,
        ChannelOperation::Reply,
        ChannelOperation::Publish,
    ]
    .into_iter()
    .collect()
}

fn validate_non_empty(field: &'static str, value: &str) -> Result<(), DomainError> {
    if value.trim().is_empty() {
        return Err(invalid_config(format!("{field} must not be empty")));
    }
    Ok(())
}

fn invalid_config(message: impl Into<String>) -> DomainError {
    DomainError::InvalidConfig(message.into())
}

fn not_found(entity: &'static str, id: String) -> DomainError {
    DomainError::NotFound { entity, id }
}

fn conflict(entity: &'static str, constraint: &'static str, message: String) -> DomainError {
    DomainError::Conflict {
        entity,
        constraint,
        message,
    }
}

#[cfg(test)]
mod tests {
    use chrono::Duration;
    use serde_json::json;

    use super::*;

    #[test]
    fn default_registry_roundtrips_from_empty_json() {
        let registry: ChannelRegistryDocument = serde_json::from_value(json!({})).unwrap();

        assert_eq!(registry.schema_version, CHANNEL_REGISTRY_SCHEMA_VERSION);
        assert!(registry.channels.is_empty());

        let restored: ChannelRegistryDocument =
            serde_json::from_value(serde_json::to_value(&registry).unwrap()).unwrap();
        assert_eq!(restored, registry);
    }

    #[test]
    fn mutation_upserts_and_closes_channel() {
        let mut registry = ChannelRegistryDocument::default();
        let channel = Channel::new(
            ChannelOwner::LifecycleRun {
                run_id: Uuid::new_v4(),
            },
            ChannelMedium::Runtime,
            ChannelTopology::Direct,
        );
        let channel_id = channel.id;

        registry
            .apply(ChannelRegistryMutation::UpsertChannel(ChannelRecord::new(
                channel,
            )))
            .unwrap();
        assert_eq!(registry.channels.len(), 1);

        registry
            .apply(ChannelRegistryMutation::CloseChannel {
                channel_id,
                reason: Some("complete".to_string()),
            })
            .unwrap();

        let record = registry.channel(channel_id).unwrap();
        assert_eq!(record.channel.status, ChannelStatus::Closed);
        assert_eq!(record.channel.close_reason.as_deref(), Some("complete"));
        assert!(record.channel.closed_at.is_some());
    }

    #[test]
    fn mutation_manages_participant_policy() {
        let mut registry = registry_with_channel();
        let channel_id = registry.channels[0].channel.id;
        let participant_ref = ChannelParticipantRef::AgentRun {
            run_id: Uuid::new_v4(),
            agent_id: Uuid::new_v4(),
        };
        let participant = ChannelParticipant::new(participant_ref.clone(), ChannelRole::Member);

        registry
            .apply(ChannelRegistryMutation::AddParticipant {
                channel_id,
                participant,
            })
            .unwrap();
        assert_eq!(registry.channel(channel_id).unwrap().participants.len(), 1);

        let operations = [ChannelOperation::Read, ChannelOperation::Broadcast]
            .into_iter()
            .collect();
        registry
            .apply(ChannelRegistryMutation::UpdateParticipantPolicy {
                channel_id,
                participant_ref: participant_ref.clone(),
                operations,
                ingress_policy: ChannelIngressPolicy::Disabled,
                egress_policy: ChannelEgressPolicy::Open,
            })
            .unwrap();
        let participant = &registry.channel(channel_id).unwrap().participants[0];
        assert!(
            participant
                .operations
                .contains(&ChannelOperation::Broadcast)
        );
        assert_eq!(participant.ingress_policy, ChannelIngressPolicy::Disabled);

        registry
            .apply(ChannelRegistryMutation::RemoveParticipant {
                channel_id,
                participant_ref,
            })
            .unwrap();
        assert!(
            registry
                .channel(channel_id)
                .unwrap()
                .participants
                .is_empty()
        );
    }

    #[test]
    fn mutation_updates_channel_policy() {
        let mut registry = registry_with_channel();
        let channel_id = registry.channels[0].channel.id;
        let mut policy = ChannelPolicy::default();
        policy.broadcast.include_sender = true;
        policy.delivery.max_delivery_state_items = 8;

        registry
            .apply(ChannelRegistryMutation::UpdateChannelPolicy { channel_id, policy })
            .unwrap();

        let record = registry.channel(channel_id).unwrap();
        assert!(record.channel.policy.broadcast.include_sender);
        assert_eq!(record.channel.policy.delivery.max_delivery_state_items, 8);
    }

    #[test]
    fn mutation_upserts_and_removes_binding() {
        let mut registry = registry_with_channel();
        let channel_id = registry.channels[0].channel.id;
        let mut binding = ChannelBinding::new("slack", "workspace-1");
        binding.external_room_ref = Some("room-1".to_string());
        let binding_id = binding.binding_id;

        registry
            .apply(ChannelRegistryMutation::UpsertBinding {
                channel_id,
                binding,
            })
            .unwrap();
        assert_eq!(registry.channel(channel_id).unwrap().bindings.len(), 1);

        registry
            .apply(ChannelRegistryMutation::RemoveBinding {
                channel_id,
                binding_ref: binding_id,
            })
            .unwrap();
        assert!(registry.channel(channel_id).unwrap().bindings.is_empty());
    }

    #[test]
    fn delivery_state_is_deduped_and_pruned() {
        let mut registry = registry_with_channel();
        let channel_id = registry.channels[0].channel.id;
        registry.channels[0]
            .channel
            .policy
            .delivery
            .max_delivery_state_items = 2;

        let message_id = Uuid::new_v4();
        let delivery_id = Uuid::new_v4();
        let old = delivery_state(delivery_id, message_id, Utc::now() - Duration::minutes(10));
        let updated = delivery_state(delivery_id, message_id, Utc::now());
        let other = delivery_state(
            Uuid::new_v4(),
            message_id,
            Utc::now() - Duration::minutes(1),
        );
        let newest = delivery_state(
            Uuid::new_v4(),
            message_id,
            Utc::now() + Duration::minutes(1),
        );

        registry
            .apply(ChannelRegistryMutation::RecordDeliveryState {
                channel_id,
                state: old,
            })
            .unwrap();
        registry
            .apply(ChannelRegistryMutation::RecordDeliveryState {
                channel_id,
                state: updated,
            })
            .unwrap();
        registry
            .apply(ChannelRegistryMutation::RecordDeliveryState {
                channel_id,
                state: other,
            })
            .unwrap();
        registry
            .apply(ChannelRegistryMutation::RecordDeliveryState {
                channel_id,
                state: newest,
            })
            .unwrap();

        let states = &registry.channel(channel_id).unwrap().delivery_state;
        assert_eq!(states.len(), 2);
        assert!(states.iter().any(|state| state.delivery_id == delivery_id));

        registry
            .apply(ChannelRegistryMutation::PruneDeliveryState {
                channel_id,
                before: Utc::now() + Duration::seconds(30),
                max_items: 1,
            })
            .unwrap();
        let states = &registry.channel(channel_id).unwrap().delivery_state;
        assert_eq!(states.len(), 1);
        assert_eq!(states[0].status, ChannelDeliveryStatus::Planned);
    }

    #[test]
    fn address_does_not_default_to_mailbox_display_key() {
        let address = ChannelAddress::new("companion", "dispatch", "agent")
            .with_source_ref("dispatch-1")
            .with_correlation_ref("corr-1")
            .with_route("child");

        address.validate().unwrap();
        assert_eq!(address.display_label_key, None);
    }

    #[test]
    fn address_mapper_preserves_mailbox_display_key_semantics() {
        let address = ChannelAddress::new("terminal", "hook_auto_resume", "system")
            .with_source_ref("effect-1")
            .with_correlation_ref("runtime:turn:1")
            .with_display_label_key("channel.source.terminal.hook_auto_resume");

        let source = channel_address_to_mailbox_source_identity(&address);

        assert_eq!(source.namespace, "terminal");
        assert_eq!(source.kind, "hook_auto_resume");
        assert_eq!(source.source_ref.as_deref(), Some("effect-1"));
        assert_eq!(
            source.display_label_key,
            "mailbox.source.terminal.hook_auto_resume"
        );
    }

    #[test]
    fn validation_rejects_duplicate_participants() {
        let mut registry = registry_with_channel();
        let participant_ref = ChannelParticipantRef::System {
            key: "companion".to_string(),
        };
        let participant = ChannelParticipant::new(participant_ref, ChannelRole::System);
        registry.channels[0].participants = vec![participant.clone(), participant];

        let error = registry.validate().expect_err("duplicate participant");
        assert!(matches!(error, DomainError::Conflict { .. }));
    }

    fn registry_with_channel() -> ChannelRegistryDocument {
        let channel = Channel::new(
            ChannelOwner::LifecycleRun {
                run_id: Uuid::new_v4(),
            },
            ChannelMedium::Runtime,
            ChannelTopology::Direct,
        );
        let mut registry = ChannelRegistryDocument::default();
        registry
            .apply(ChannelRegistryMutation::UpsertChannel(ChannelRecord::new(
                channel,
            )))
            .unwrap();
        registry
    }

    fn delivery_state(
        delivery_id: Uuid,
        message_id: Uuid,
        updated_at: DateTime<Utc>,
    ) -> ChannelDeliveryState {
        ChannelDeliveryState {
            delivery_id,
            message_id,
            target: ChannelDeliveryTarget::Mailbox {
                run_id: Uuid::new_v4(),
                agent_id: Uuid::new_v4(),
            },
            status: ChannelDeliveryStatus::Planned,
            materialized_ref: None,
            updated_at,
        }
    }
}
