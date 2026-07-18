use std::sync::Arc;

use agentdash_agent_runtime_contract::ManagedAgentRuntimeGateway;
use agentdash_application_agentrun::agent_run::{
    AgentRunProductProjectionGateway, AgentRunProductProjectionQueryPort,
    ProductAgentRunRuntimeProjectionAdapter,
};
use sqlx::PgPool;

use crate::{
    PostgresAgentRunProductRuntimeBindingRepository, PostgresAgentRunTerminalProjectionStore,
    PostgresWorkspaceModulePresentationStore,
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
}

impl AgentRunProductProjectionComposition {
    pub fn build(pool: PgPool, runtime: Arc<dyn ManagedAgentRuntimeGateway>) -> Self {
        let runtime_bindings = Arc::new(PostgresAgentRunProductRuntimeBindingRepository::new(
            pool.clone(),
        ));
        let workspace_presentations =
            Arc::new(PostgresWorkspaceModulePresentationStore::new(pool.clone()));
        let terminals = Arc::new(PostgresAgentRunTerminalProjectionStore::new(pool));
        let runtime_projection = Arc::new(ProductAgentRunRuntimeProjectionAdapter::new(runtime));
        let gateway: Arc<dyn AgentRunProductProjectionQueryPort> =
            Arc::new(AgentRunProductProjectionGateway::new(
                runtime_bindings.clone(),
                runtime_projection,
                workspace_presentations.clone(),
                workspace_presentations.clone(),
                terminals.clone(),
            ));
        Self {
            gateway,
            runtime_bindings,
            workspace_presentations,
            terminals,
        }
    }
}
