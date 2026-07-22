use std::sync::Arc;

use agentdash_agent_runtime_host::SharedCompleteAgentLiveCatalog;
use agentdash_application_agentrun::agent_run::{
    AgentRunCompleteAgentResolverPort, AgentRunProductCommandFacade,
    AgentRunProductProjectionGateway, AgentRunProductProjectionQueryPort,
    AgentRunResolvedCompleteAgent,
};
use async_trait::async_trait;
use sqlx::PgPool;

use crate::{
    PostgresAgentRunProductRuntimeBindingRepository, PostgresAgentRunTerminalProjectionStore,
};

/// Product shell plus direct concrete-Agent presentation/command composition.
pub struct AgentRunProductProjectionComposition {
    pub gateway: Arc<dyn AgentRunProductProjectionQueryPort>,
    pub commands: Arc<AgentRunProductCommandFacade>,
    pub agents: Arc<dyn AgentRunCompleteAgentResolverPort>,
    pub runtime_bindings: Arc<PostgresAgentRunProductRuntimeBindingRepository>,
    pub terminals: Arc<PostgresAgentRunTerminalProjectionStore>,
}

struct LiveCompleteAgentResolver {
    catalog: SharedCompleteAgentLiveCatalog,
    provisioner: Arc<crate::CompleteAgentProductRuntimeProvisioner>,
}

#[async_trait]
impl AgentRunCompleteAgentResolverPort for LiveCompleteAgentResolver {
    async fn resolve(
        &self,
        binding: &agentdash_application_agentrun::agent_run::AgentRunProductRuntimeBinding,
    ) -> Result<AgentRunResolvedCompleteAgent, String> {
        let binding_generation = self
            .provisioner
            .ensure_product_binding_route(binding)
            .await
            .map_err(|error| error.to_string())?;
        let service = self
            .catalog
            .current(&binding.agent.service_instance_id)
            .await
            .map(|selection| selection.service())
            .ok_or_else(|| {
                format!(
                    "Complete Agent {} is unavailable after restoring its Product binding route",
                    binding.agent.service_instance_id
                )
            })?;
        Ok(AgentRunResolvedCompleteAgent {
            service,
            binding_generation,
        })
    }
}

impl AgentRunProductProjectionComposition {
    pub fn build(
        pool: PgPool,
        live_agents: SharedCompleteAgentLiveCatalog,
        provisioner: Arc<crate::CompleteAgentProductRuntimeProvisioner>,
        runtime_bindings: Arc<PostgresAgentRunProductRuntimeBindingRepository>,
    ) -> Result<Self, String> {
        let terminals = Arc::new(PostgresAgentRunTerminalProjectionStore::new(pool));
        let agents: Arc<dyn AgentRunCompleteAgentResolverPort> =
            Arc::new(LiveCompleteAgentResolver {
                catalog: live_agents,
                provisioner,
            });
        let commands = Arc::new(AgentRunProductCommandFacade::new(
            runtime_bindings.clone(),
            agents.clone(),
        ));
        let gateway: Arc<dyn AgentRunProductProjectionQueryPort> =
            Arc::new(AgentRunProductProjectionGateway::new(
                runtime_bindings.clone(),
                agents.clone(),
                terminals.clone(),
            ));
        Ok(Self {
            gateway,
            commands,
            agents,
            runtime_bindings,
            terminals,
        })
    }
}
