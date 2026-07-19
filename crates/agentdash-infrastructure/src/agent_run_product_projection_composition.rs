use std::sync::Arc;

use agentdash_agent_runtime_contract::ManagedAgentRuntimeGateway;
use agentdash_application_agentrun::agent_run::{
    AgentRunProductProjectionGateway, AgentRunProductProjectionQueryPort,
    AgentRunThreadNameProjectionObserver, ProductAgentRunRuntimeProjectionAdapter,
};
use agentdash_application_ports::project_projection_notification::ProjectProjectionNotificationPort;
use agentdash_domain::workflow::LifecycleRunRepository;
use sqlx::PgPool;

use crate::{
    PostgresAgentRunProductRuntimeBindingRepository, PostgresAgentRunTerminalProjectionStore,
    PostgresWorkspaceModulePresentationStore,
    managed_runtime_product_change_delivery::{
        ManagedRuntimeProductChangeConsumer, PostgresManagedRuntimeProductChangeDelivery,
    },
};

/// Final Product projection composition over Managed Runtime and the two Product-owned sagas.
///
/// Managed Runtime remains the only Runtime fact authority. The Product gateway can only read
/// Runtime snapshot/change through the canonical adapter and can only mutate workspace/terminal
/// projection state through their dedicated PostgreSQL units of work.
pub struct AgentRunProductProjectionComposition {
    pub gateway: Arc<dyn AgentRunProductProjectionQueryPort>,
    pub runtime_bindings: Arc<PostgresAgentRunProductRuntimeBindingRepository>,
    pub workspace_presentations: Arc<PostgresWorkspaceModulePresentationStore>,
    pub terminals: Arc<PostgresAgentRunTerminalProjectionStore>,
    runtime_change_consumer: Arc<ManagedRuntimeProductChangeConsumer>,
}

impl AgentRunProductProjectionComposition {
    pub fn build(
        pool: PgPool,
        runtime: Arc<dyn ManagedAgentRuntimeGateway>,
        runs: Arc<dyn LifecycleRunRepository>,
        notifications: Arc<dyn ProjectProjectionNotificationPort>,
        change_claim_owner: impl Into<String>,
        change_claim_lease_duration_ms: u64,
    ) -> Result<Self, String> {
        let runtime_bindings = Arc::new(PostgresAgentRunProductRuntimeBindingRepository::new(
            pool.clone(),
        ));
        let workspace_presentations =
            Arc::new(PostgresWorkspaceModulePresentationStore::new(pool.clone()));
        let terminals = Arc::new(PostgresAgentRunTerminalProjectionStore::new(pool.clone()));
        let runtime_projection = Arc::new(ProductAgentRunRuntimeProjectionAdapter::new(runtime));
        let thread_name_observer = Arc::new(AgentRunThreadNameProjectionObserver::new(
            runtime_bindings.clone(),
            runtime_projection.clone(),
            runs,
            notifications,
        ));
        let runtime_change_consumer = Arc::new(ManagedRuntimeProductChangeConsumer::new(
            PostgresManagedRuntimeProductChangeDelivery::new(
                pool,
                change_claim_owner,
                change_claim_lease_duration_ms,
            )?,
            runtime_bindings.clone(),
            thread_name_observer,
        ));
        let gateway: Arc<dyn AgentRunProductProjectionQueryPort> =
            Arc::new(AgentRunProductProjectionGateway::new(
                runtime_bindings.clone(),
                runtime_projection,
                workspace_presentations.clone(),
                workspace_presentations.clone(),
                terminals.clone(),
            ));
        Ok(Self {
            gateway,
            runtime_bindings,
            workspace_presentations,
            terminals,
            runtime_change_consumer,
        })
    }

    pub async fn drain_runtime_change_outbox(&self, limit: usize) -> Result<usize, String> {
        self.runtime_change_consumer.drain(limit).await
    }
}
