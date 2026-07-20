use std::sync::Arc;

use agentdash_agent_runtime_host::SharedCompleteAgentLiveCatalog;
use agentdash_agent_service_api::{
    AgentBindingGeneration, AgentServiceInstanceId, CompleteAgentService,
};
use agentdash_application_agentrun::agent_run::{
    AgentRunCompleteAgentResolverPort, AgentRunProductCommandFacade,
    AgentRunProductProjectionGateway, AgentRunProductProjectionQueryPort,
};
use async_trait::async_trait;
use sqlx::PgPool;

use crate::{
    PostgresAgentRunProductRuntimeBindingRepository, PostgresAgentRunTerminalProjectionStore,
    PostgresWorkspaceModulePresentationStore,
};

/// Product shell plus direct concrete-Agent presentation/command composition.
pub struct AgentRunProductProjectionComposition {
    pub gateway: Arc<dyn AgentRunProductProjectionQueryPort>,
    pub commands: Arc<AgentRunProductCommandFacade>,
    pub runtime_bindings: Arc<PostgresAgentRunProductRuntimeBindingRepository>,
    pub workspace_presentations: Arc<PostgresWorkspaceModulePresentationStore>,
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
        service_instance_id: &AgentServiceInstanceId,
    ) -> Result<Arc<dyn CompleteAgentService>, String> {
        self.catalog
            .current(service_instance_id)
            .await
            .map(|selection| selection.service())
            .ok_or_else(|| {
                format!("Complete Agent {service_instance_id} is unavailable in the current Host")
            })
    }

    async fn binding_generation(
        &self,
        binding: &agentdash_application_agentrun::agent_run::AgentRunProductRuntimeBinding,
    ) -> Result<AgentBindingGeneration, String> {
        self.provisioner
            .ensure_product_binding_route(binding)
            .await
            .map_err(|error| error.to_string())
    }
}

impl AgentRunProductProjectionComposition {
    pub fn build(
        pool: PgPool,
        live_agents: SharedCompleteAgentLiveCatalog,
        provisioner: Arc<crate::CompleteAgentProductRuntimeProvisioner>,
        runtime_bindings: Arc<PostgresAgentRunProductRuntimeBindingRepository>,
        workspace_presentations: Arc<PostgresWorkspaceModulePresentationStore>,
    ) -> Result<Self, String> {
        let terminals = Arc::new(PostgresAgentRunTerminalProjectionStore::new(pool));
        let agents = Arc::new(LiveCompleteAgentResolver {
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
                agents,
                workspace_presentations.clone(),
                workspace_presentations.clone(),
                terminals.clone(),
            ));
        Ok(Self {
            gateway,
            commands,
            runtime_bindings,
            workspace_presentations,
            terminals,
        })
    }
}
