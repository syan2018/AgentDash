use std::sync::Arc;

use agentdash_agent_runtime_wire::RuntimeWireAgentBindingTarget;
use agentdash_agent_service_api::{
    AgentBindingGeneration, AgentHostCallbacks, AgentServiceInstanceId, CompleteAgentService,
};

use crate::{RemoteCompleteAgentService, RuntimeWirePlacement};

/// Host-ready registration for one remotely placed Complete Agent service instance.
///
/// Relay supplies the placement stream and never becomes the service identity.
pub struct RemoteCompleteAgentRegistration {
    instance_id: AgentServiceInstanceId,
    service: Arc<RemoteCompleteAgentService>,
}

impl RemoteCompleteAgentRegistration {
    pub fn new(
        instance_id: AgentServiceInstanceId,
        local_binding_generation: AgentBindingGeneration,
        target: RuntimeWireAgentBindingTarget,
        placement: Arc<dyn RuntimeWirePlacement>,
        callbacks: Arc<dyn AgentHostCallbacks>,
    ) -> Self {
        Self {
            instance_id,
            service: RemoteCompleteAgentService::new(
                local_binding_generation,
                target,
                placement,
                callbacks,
            ),
        }
    }

    pub fn instance_id(&self) -> &AgentServiceInstanceId {
        &self.instance_id
    }

    pub fn service(&self) -> Arc<dyn CompleteAgentService> {
        self.service.clone()
    }

    pub fn into_parts(self) -> (AgentServiceInstanceId, Arc<dyn CompleteAgentService>) {
        (self.instance_id, self.service)
    }
}
