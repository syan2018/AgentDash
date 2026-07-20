use std::sync::Arc;

use agentdash_agent_runtime_contract::ManagedAgentRuntimeGateway;
use agentdash_application_agentrun::agent_run::{
    AgentRunProductCommandFacade, AgentRunProductRuntimeBindingRepository, ProductMailboxFacade,
};
use sqlx::PgPool;

use crate::{PostgresProductMailboxRepository, PostgresProductRuntimeCommandClaimRepository};

/// Construction-only Product persistence bindings for the production composition root.
pub struct AgentRunProductPersistenceComposition {
    pub runtime_command_claims: Arc<PostgresProductRuntimeCommandClaimRepository>,
    pub mailbox: Arc<PostgresProductMailboxRepository>,
}

impl AgentRunProductPersistenceComposition {
    pub fn build(pool: PgPool) -> Self {
        Self {
            runtime_command_claims: Arc::new(PostgresProductRuntimeCommandClaimRepository::new(
                pool.clone(),
            )),
            mailbox: Arc::new(PostgresProductMailboxRepository::new(pool)),
        }
    }

    pub fn product_command_facade(
        &self,
        bindings: Arc<dyn AgentRunProductRuntimeBindingRepository>,
        runtime: Arc<dyn ManagedAgentRuntimeGateway>,
    ) -> AgentRunProductCommandFacade {
        AgentRunProductCommandFacade::new(bindings, runtime, self.runtime_command_claims.clone())
    }

    pub fn product_mailbox_facade(
        &self,
        bindings: Arc<dyn AgentRunProductRuntimeBindingRepository>,
    ) -> ProductMailboxFacade {
        ProductMailboxFacade::new(bindings, self.mailbox.clone(), self.mailbox.clone())
    }
}
