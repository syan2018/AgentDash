use serde_json::Value;
use uuid::Uuid;

use super::{
    InteractionAttachment, InteractionDefinition, InteractionDefinitionRevision, InteractionError,
    InteractionEvent, InteractionInstance, InteractionOwner, InteractionRuntimeBinding,
    OperationEffectIntent, ResolvedInteractionCommand,
};

#[derive(Debug, Clone, PartialEq)]
pub struct DefinitionRevisionCommit {
    pub expected_current_revision_id: Uuid,
    pub revision: InteractionDefinitionRevision,
}

#[async_trait::async_trait]
pub trait InteractionDefinitionRepository: Send + Sync {
    async fn create(
        &self,
        definition: &InteractionDefinition,
        initial_revision: &InteractionDefinitionRevision,
    ) -> Result<(), InteractionError>;
    async fn get(&self, id: Uuid) -> Result<Option<InteractionDefinition>, InteractionError>;
    async fn get_revision(
        &self,
        revision_id: Uuid,
    ) -> Result<Option<InteractionDefinitionRevision>, InteractionError>;
    async fn list_by_owner(
        &self,
        owner: &InteractionOwner,
    ) -> Result<Vec<InteractionDefinition>, InteractionError>;
    async fn commit_revision(
        &self,
        definition_id: Uuid,
        commit: DefinitionRevisionCommit,
    ) -> Result<InteractionDefinition, InteractionError>;
    async fn archive(&self, definition_id: Uuid)
    -> Result<InteractionDefinition, InteractionError>;
}

#[async_trait::async_trait]
pub trait InteractionInstanceRepository: Send + Sync {
    async fn create(&self, instance: &InteractionInstance) -> Result<(), InteractionError>;
    async fn get(&self, id: Uuid) -> Result<Option<InteractionInstance>, InteractionError>;
    async fn list_by_owner(
        &self,
        owner: &InteractionOwner,
    ) -> Result<Vec<InteractionInstance>, InteractionError>;
    async fn close(
        &self,
        instance_id: Uuid,
        expected_state_revision: u64,
    ) -> Result<InteractionInstance, InteractionError>;
    async fn attach(&self, attachment: &InteractionAttachment) -> Result<(), InteractionError>;
    async fn detach(&self, attachment_id: Uuid) -> Result<(), InteractionError>;
    async fn upsert_runtime_binding(
        &self,
        binding: &InteractionRuntimeBinding,
    ) -> Result<(), InteractionError>;
    async fn list_runtime_bindings(
        &self,
        instance_id: Uuid,
        attachment_id: Option<Uuid>,
    ) -> Result<Vec<InteractionRuntimeBinding>, InteractionError>;
}

#[derive(Debug, Clone, PartialEq)]
pub struct InteractionCommandTransaction {
    pub command: ResolvedInteractionCommand,
    pub request_digest: String,
    pub previous_state_revision: u64,
    pub next_state: Value,
    pub next_state_revision: u64,
    pub event: InteractionEvent,
    pub effect_intent: Option<OperationEffectIntent>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum InteractionCommandCommit {
    Committed {
        instance: InteractionInstance,
        event: InteractionEvent,
        effect_intent: Option<OperationEffectIntent>,
    },
    Duplicate {
        instance: InteractionInstance,
        event: InteractionEvent,
        effect_intent: Option<OperationEffectIntent>,
    },
}

/// 原子提交 command idempotency、event、state revision 与可选 effect intent。
#[async_trait::async_trait]
pub trait InteractionCommandTransactionPort: Send + Sync {
    async fn commit(
        &self,
        transaction: InteractionCommandTransaction,
    ) -> Result<InteractionCommandCommit, InteractionError>;
}

#[async_trait::async_trait]
pub trait OperationEffectIntentRepository: Send + Sync {
    async fn claim_due(
        &self,
        limit: usize,
        claimed_at: chrono::DateTime<chrono::Utc>,
    ) -> Result<Vec<OperationEffectIntent>, InteractionError>;
    async fn mark_succeeded(
        &self,
        effect_id: Uuid,
        claim_token: Uuid,
        completed_at: chrono::DateTime<chrono::Utc>,
    ) -> Result<(), InteractionError>;
    async fn mark_failed(
        &self,
        effect_id: Uuid,
        claim_token: Uuid,
        next_attempt_at: chrono::DateTime<chrono::Utc>,
        failure_code: &str,
        terminal: bool,
    ) -> Result<(), InteractionError>;
}
