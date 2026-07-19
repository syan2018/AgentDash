use std::sync::Arc;

use agentdash_agent_runtime_contract::ManagedAgentRuntimeGateway;
use agentdash_application_agentrun::agent_run::{
    AgentRunAppliedResourceSurfaceCompilerPort, AgentRunAppliedResourceSurfaceMaterializer,
    AgentRunProductCommandFacade, AgentRunProductRuntimeBindingRepository, ProductMailboxFacade,
};
use sqlx::PgPool;

use crate::{
    PostgresAgentRunAppliedResourceSurfaceRepository, PostgresProductMailboxRepository,
    PostgresProductRuntimeCommandClaimRepository,
};

/// Construction-only Product persistence bindings for the S5 production composition root.
///
/// Callers must complete `applied_resource_surface_materializer(...).materialize(...)` before
/// activating the corresponding Managed Runtime target. This helper deliberately exposes no
/// Runtime activation operation, API route, or alternate Product read model.
pub struct AgentRunProductPersistenceComposition {
    pub applied_resource_surfaces: Arc<PostgresAgentRunAppliedResourceSurfaceRepository>,
    pub runtime_command_claims: Arc<PostgresProductRuntimeCommandClaimRepository>,
    pub mailbox: Arc<PostgresProductMailboxRepository>,
}

impl AgentRunProductPersistenceComposition {
    pub fn build(pool: PgPool) -> Self {
        Self {
            applied_resource_surfaces: Arc::new(
                PostgresAgentRunAppliedResourceSurfaceRepository::new(pool.clone()),
            ),
            runtime_command_claims: Arc::new(PostgresProductRuntimeCommandClaimRepository::new(
                pool.clone(),
            )),
            mailbox: Arc::new(PostgresProductMailboxRepository::new(pool)),
        }
    }

    pub fn applied_resource_surface_materializer(
        &self,
        compiler: Arc<dyn AgentRunAppliedResourceSurfaceCompilerPort>,
    ) -> AgentRunAppliedResourceSurfaceMaterializer {
        AgentRunAppliedResourceSurfaceMaterializer::new(
            compiler,
            self.applied_resource_surfaces.clone(),
        )
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
