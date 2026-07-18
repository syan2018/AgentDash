use std::{collections::BTreeMap, sync::Arc};

use agentdash_agent_runtime_contract::{ManagedAgentRuntimeGateway, ManagedRuntimeGatewayError};
use agentdash_agent_runtime_host::{
    CompleteAgentCallbackBroker, CompleteAgentHookHandler, CompleteAgentHost,
    CompleteAgentHostError, CompleteAgentPlacement, CompleteAgentServiceRegistry,
    CompleteAgentToolHandler, complete_agent_managed_runtime_gateway,
};
use agentdash_agent_service_api::{
    AgentHostCallbacks, AgentServiceDescriptor, AgentServiceInstanceId, CompleteAgentService,
};
use async_trait::async_trait;
use sqlx::PgPool;
use thiserror::Error;
use tokio::sync::RwLock;

use crate::{
    PostgresCompleteAgentCallbackRepository, PostgresCompleteAgentHostRepository,
    PostgresManagedRuntimeStateRepository,
};

#[derive(Default)]
struct ProcessCompleteAgentServiceRegistry {
    services: RwLock<BTreeMap<AgentServiceInstanceId, Arc<dyn CompleteAgentService>>>,
}

#[async_trait]
impl CompleteAgentServiceRegistry for ProcessCompleteAgentServiceRegistry {
    async fn attach(
        &self,
        instance_id: AgentServiceInstanceId,
        service: Arc<dyn CompleteAgentService>,
    ) {
        self.services.write().await.insert(instance_id, service);
    }

    async fn resolve(
        &self,
        instance_id: &AgentServiceInstanceId,
    ) -> Option<Arc<dyn CompleteAgentService>> {
        self.services.read().await.get(instance_id).cloned()
    }
}

#[derive(Debug, Error)]
pub enum CompleteAgentCompositionError {
    #[error(transparent)]
    Runtime(#[from] ManagedRuntimeGatewayError),
    #[error(transparent)]
    Host(#[from] CompleteAgentHostError),
}

/// Final production kernel for Managed Runtime and Complete Agent coordination.
///
/// The process registry contains live handles only. Every service descriptor, placement,
/// binding, generation, effect, callback and normalized Runtime fact is recovered from the three
/// PostgreSQL repositories.
pub struct CompleteAgentComposition {
    pub runtime_repository: Arc<PostgresManagedRuntimeStateRepository>,
    pub host_repository: Arc<PostgresCompleteAgentHostRepository>,
    pub callback_repository: Arc<PostgresCompleteAgentCallbackRepository>,
    pub host: Arc<CompleteAgentHost>,
    pub callbacks: Arc<CompleteAgentCallbackBroker>,
    pub runtime: Arc<dyn ManagedAgentRuntimeGateway>,
}

impl CompleteAgentComposition {
    pub fn build(
        pool: PgPool,
        tool_handler: Arc<dyn CompleteAgentToolHandler>,
        hook_handler: Arc<dyn CompleteAgentHookHandler>,
        dispatch_owner: impl Into<String>,
        lease_duration_ms: u64,
    ) -> Result<Self, CompleteAgentCompositionError> {
        let runtime_repository = Arc::new(PostgresManagedRuntimeStateRepository::new(pool.clone()));
        let host_repository = Arc::new(PostgresCompleteAgentHostRepository::new(pool.clone()));
        let callback_repository = Arc::new(PostgresCompleteAgentCallbackRepository::new(pool));
        let registry = Arc::new(ProcessCompleteAgentServiceRegistry::default());
        let host = Arc::new(CompleteAgentHost::new(host_repository.clone(), registry));
        let callbacks = Arc::new(CompleteAgentCallbackBroker::new(
            tool_handler,
            hook_handler,
            host_repository.clone(),
            callback_repository.clone(),
        ));
        let runtime = complete_agent_managed_runtime_gateway(
            runtime_repository.clone(),
            host.clone(),
            dispatch_owner,
            lease_duration_ms,
        )?;
        Ok(Self {
            runtime_repository,
            host_repository,
            callback_repository,
            host,
            callbacks,
            runtime,
        })
    }

    pub fn host_callbacks(&self) -> Arc<dyn AgentHostCallbacks> {
        self.callbacks.clone()
    }

    pub async fn register_service(
        &self,
        instance_id: AgentServiceInstanceId,
        placement: CompleteAgentPlacement,
        service: Arc<dyn CompleteAgentService>,
    ) -> Result<AgentServiceDescriptor, CompleteAgentCompositionError> {
        Ok(self
            .host
            .register_service(instance_id, placement, service)
            .await?)
    }
}
