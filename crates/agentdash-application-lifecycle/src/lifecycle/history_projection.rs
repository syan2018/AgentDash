use std::sync::{Arc, OnceLock};

use agentdash_agent_protocol::CanonicalConversationRecord;
use agentdash_agent_runtime_contract::{
    ManagedRuntimeInteraction, ManagedRuntimeItem, ManagedRuntimeItemBody,
    ManagedRuntimeLifecycleStatus, ManagedRuntimeProjectionAuthority,
    ManagedRuntimeProjectionFidelity, ManagedRuntimeSnapshot, ManagedRuntimeTurn,
    RuntimeProjectionRevision, RuntimeThreadId, RuntimeTurnId,
};
use agentdash_application_agentrun::agent_run::AgentRunProductProjectionQueryPort;
use agentdash_domain::agent_run_target::AgentRunTarget;
use async_trait::async_trait;
use serde::Serialize;
use thiserror::Error;

/// Lifecycle-owned, rebuildable view of one Complete Agent conversation.
///
/// This is deliberately a read model. Its only input is the canonical Managed Runtime
/// projection, so deleting this value never loses Agent or Product authority.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LifecycleHistoryProjection {
    pub target: AgentRunTarget,
    pub runtime_thread_id: RuntimeThreadId,
    pub projection_revision: RuntimeProjectionRevision,
    pub captured_at_ms: u64,
    pub lifecycle: ManagedRuntimeLifecycleStatus,
    pub active_turn_id: Option<RuntimeTurnId>,
    pub thread_name: Option<String>,
    pub authority: ManagedRuntimeProjectionAuthority,
    pub fidelity: ManagedRuntimeProjectionFidelity,
    pub turns: Vec<ManagedRuntimeTurn>,
    pub items: Vec<ManagedRuntimeItem>,
    pub interactions: Vec<ManagedRuntimeInteraction>,
    /// Exact source-ordered App Server-shaped history used by events.json and reconnect readers.
    pub conversation_history: Vec<CanonicalConversationRecord>,
}

impl LifecycleHistoryProjection {
    pub fn from_runtime(target: AgentRunTarget, snapshot: ManagedRuntimeSnapshot) -> Self {
        Self {
            target,
            runtime_thread_id: snapshot.thread_id,
            projection_revision: snapshot.revision,
            captured_at_ms: snapshot.captured_at_ms,
            lifecycle: snapshot.lifecycle,
            active_turn_id: snapshot.active_turn_id,
            thread_name: snapshot.thread_name,
            authority: snapshot.authority,
            fidelity: snapshot.fidelity,
            turns: snapshot.turns,
            items: snapshot.items,
            interactions: snapshot.interactions,
            conversation_history: snapshot.conversation_history,
        }
    }

    pub fn message_items(&self) -> impl Iterator<Item = &ManagedRuntimeItem> {
        self.items.iter().filter(|item| {
            matches!(
                item.presentation.body,
                ManagedRuntimeItemBody::UserMessage { .. }
                    | ManagedRuntimeItemBody::HookPrompt { .. }
                    | ManagedRuntimeItemBody::AgentMessage { .. }
            )
        })
    }

    pub fn tool_items(&self) -> impl Iterator<Item = &ManagedRuntimeItem> {
        self.items.iter().filter(|item| {
            matches!(
                item.presentation.body,
                ManagedRuntimeItemBody::CommandExecution { .. }
                    | ManagedRuntimeItemBody::FileChange { .. }
                    | ManagedRuntimeItemBody::FileRead { .. }
                    | ManagedRuntimeItemBody::FileSearch { .. }
                    | ManagedRuntimeItemBody::McpToolCall { .. }
                    | ManagedRuntimeItemBody::DynamicToolCall { .. }
                    | ManagedRuntimeItemBody::CollaborationToolCall { .. }
                    | ManagedRuntimeItemBody::SubagentActivity { .. }
                    | ManagedRuntimeItemBody::WebSearch { .. }
                    | ManagedRuntimeItemBody::ImageView { .. }
                    | ManagedRuntimeItemBody::ImageGeneration { .. }
                    | ManagedRuntimeItemBody::Sleep { .. }
                    | ManagedRuntimeItemBody::Review { .. }
                    | ManagedRuntimeItemBody::TerminalControl { .. }
                    | ManagedRuntimeItemBody::GenericToolActivity { .. }
            )
        })
    }

    pub fn write_items(&self) -> impl Iterator<Item = &ManagedRuntimeItem> {
        self.items.iter().filter(|item| {
            matches!(
                item.presentation.body,
                ManagedRuntimeItemBody::FileChange { .. }
            )
        })
    }

    pub fn compaction_items(&self) -> impl Iterator<Item = &ManagedRuntimeItem> {
        self.items.iter().filter(|item| {
            matches!(
                item.presentation.body,
                ManagedRuntimeItemBody::ContextCompaction { .. }
            )
        })
    }

    pub fn terminal_control_items(&self) -> impl Iterator<Item = &ManagedRuntimeItem> {
        self.items.iter().filter(|item| {
            matches!(
                item.presentation.body,
                ManagedRuntimeItemBody::TerminalControl { .. }
            )
        })
    }

    pub fn items_for_turn(
        &self,
        turn_id: &RuntimeTurnId,
    ) -> impl Iterator<Item = &ManagedRuntimeItem> {
        self.items
            .iter()
            .filter(move |item| &item.turn_id == turn_id)
    }
}

#[derive(Debug, Error)]
pub enum LifecycleHistoryQueryError {
    #[error("Lifecycle history projection is not bound to the Product Runtime query")]
    NotBound,
    #[error("Lifecycle history projection failed: {0}")]
    Projection(String),
}

#[async_trait]
pub trait LifecycleHistoryQueryPort: Send + Sync {
    async fn load(
        &self,
        target: &AgentRunTarget,
    ) -> Result<LifecycleHistoryProjection, LifecycleHistoryQueryError>;
}

pub struct ProductRuntimeLifecycleHistoryQuery {
    product_projection: Arc<dyn AgentRunProductProjectionQueryPort>,
}

impl ProductRuntimeLifecycleHistoryQuery {
    pub fn new(product_projection: Arc<dyn AgentRunProductProjectionQueryPort>) -> Self {
        Self { product_projection }
    }
}

#[async_trait]
impl LifecycleHistoryQueryPort for ProductRuntimeLifecycleHistoryQuery {
    async fn load(
        &self,
        target: &AgentRunTarget,
    ) -> Result<LifecycleHistoryProjection, LifecycleHistoryQueryError> {
        let snapshot = self
            .product_projection
            .runtime_snapshot(target)
            .await
            .map_err(|error| LifecycleHistoryQueryError::Projection(error.to_string()))?;
        Ok(LifecycleHistoryProjection::from_runtime(
            target.clone(),
            snapshot,
        ))
    }
}

/// Breaks the composition cycle between VFS-backed tools and the Runtime projection.
///
/// The provider is registered while the VFS kernel is built; the Product projection is bound
/// once the Complete Agent composition exists. Reads before binding fail explicitly.
#[derive(Clone, Default)]
pub struct DeferredLifecycleHistoryQuery {
    inner: Arc<OnceLock<Arc<dyn LifecycleHistoryQueryPort>>>,
}

impl DeferredLifecycleHistoryQuery {
    pub fn bind_product_projection(
        &self,
        product_projection: Arc<dyn AgentRunProductProjectionQueryPort>,
    ) -> Result<(), LifecycleHistoryQueryError> {
        self.inner
            .set(Arc::new(ProductRuntimeLifecycleHistoryQuery::new(
                product_projection,
            )))
            .map_err(|_| {
                LifecycleHistoryQueryError::Projection(
                    "Lifecycle history projection was already bound".to_string(),
                )
            })
    }
}

#[async_trait]
impl LifecycleHistoryQueryPort for DeferredLifecycleHistoryQuery {
    async fn load(
        &self,
        target: &AgentRunTarget,
    ) -> Result<LifecycleHistoryProjection, LifecycleHistoryQueryError> {
        let query = self
            .inner
            .get()
            .ok_or(LifecycleHistoryQueryError::NotBound)?;
        query.load(target).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[tokio::test]
    async fn deferred_query_fails_explicitly_before_composition_binding() {
        let query = DeferredLifecycleHistoryQuery::default();
        let error = query
            .load(&AgentRunTarget {
                run_id: Uuid::new_v4(),
                agent_id: Uuid::new_v4(),
            })
            .await
            .expect_err("unbound query must fail");
        assert!(matches!(error, LifecycleHistoryQueryError::NotBound));
    }
}
