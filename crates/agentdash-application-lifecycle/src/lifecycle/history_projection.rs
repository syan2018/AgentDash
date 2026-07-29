use std::sync::{Arc, OnceLock};

use agentdash_agent_protocol::{CanonicalConversationView, CompletedConversationItem};
use agentdash_agent_runtime_contract::{
    AgentContextAuthority, AgentContextFidelity, AgentInteractionSnapshot, AgentLifecycleStatus,
    AgentObservation, AgentRuntimeView, AgentSnapshotAuthority, AgentSnapshotRevision,
    RuntimeThreadId, SemanticFidelity,
};
use agentdash_application_agentrun::agent_run::AgentRunProductProjectionQueryPort;
use agentdash_domain::agent_run_target::AgentRunTarget;
use async_trait::async_trait;
use serde::Serialize;
use thiserror::Error;

/// Lifecycle-owned, rebuildable view of one Complete Agent conversation.
///
/// This is deliberately a read model. Its only input is the canonical Agent Runtime
/// projection, so deleting this value never loses Agent or Product authority.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LifecycleHistoryProjection {
    pub target: AgentRunTarget,
    pub runtime_thread_id: RuntimeThreadId,
    pub observation: AgentObservation,
}

impl LifecycleHistoryProjection {
    pub fn from_runtime(target: AgentRunTarget, snapshot: AgentRuntimeView) -> Self {
        Self {
            target,
            runtime_thread_id: snapshot.thread_id,
            observation: snapshot.observation,
        }
    }

    pub const fn projection_revision(&self) -> AgentSnapshotRevision {
        self.observation.revision
    }

    pub const fn captured_at_ms(&self) -> u64 {
        self.observation.source_info.observed_at_ms
    }

    pub const fn lifecycle(&self) -> AgentLifecycleStatus {
        self.observation.lifecycle
    }

    pub fn thread_name(&self) -> Option<&str> {
        self.observation
            .thread_name
            .as_ref()
            .and_then(|name| name.thread_name.as_deref())
    }

    pub const fn authority(&self) -> AgentSnapshotAuthority {
        self.observation.source_info.authority
    }

    pub const fn fidelity(&self) -> SemanticFidelity {
        self.observation.source_info.fidelity
    }

    pub const fn context_authority(&self) -> AgentContextAuthority {
        self.observation.context.authority
    }

    pub const fn context_fidelity(&self) -> AgentContextFidelity {
        self.observation.context.fidelity
    }

    pub fn interactions(&self) -> &[AgentInteractionSnapshot] {
        &self.observation.interactions
    }

    pub fn conversation(&self) -> CanonicalConversationView<'_> {
        self.observation.conversation()
    }

    pub fn active_turn_id(&self) -> Option<&str> {
        self.conversation()
            .active_turn()
            .map(|turn| turn.id.as_str())
    }

    pub fn items(&self) -> impl Iterator<Item = CompletedConversationItem<'_>> {
        self.conversation().completed_items()
    }

    pub fn message_items(&self) -> impl Iterator<Item = CompletedConversationItem<'_>> {
        self.items().filter(|completed| completed.item.is_message())
    }

    pub fn tool_items(&self) -> impl Iterator<Item = CompletedConversationItem<'_>> {
        self.items()
            .filter(|completed| completed.item.is_tool_activity())
    }

    pub fn write_items(&self) -> impl Iterator<Item = CompletedConversationItem<'_>> {
        self.items()
            .filter(|completed| completed.item.is_file_change())
    }

    pub fn compaction_items(&self) -> impl Iterator<Item = CompletedConversationItem<'_>> {
        self.items()
            .filter(|completed| completed.item.is_context_compaction())
    }

    pub fn terminal_control_items(&self) -> impl Iterator<Item = CompletedConversationItem<'_>> {
        self.items()
            .filter(|completed| completed.item.is_terminal_control())
    }

    pub fn items_for_turn(
        &self,
        turn_id: &str,
    ) -> impl Iterator<Item = CompletedConversationItem<'_>> {
        let turn_id = turn_id.to_owned();
        self.items()
            .filter(move |completed| completed.turn_id == turn_id)
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
            .runtime_view(target)
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
