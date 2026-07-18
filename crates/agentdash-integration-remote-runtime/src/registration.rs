use std::sync::Arc;

use agentdash_agent_runtime_wire::RuntimeWireAgentBindingTarget;
use agentdash_agent_service_api::{
    AgentBindingGeneration, AgentHostCallbacks, AgentServiceDescriptor, AgentServiceInstanceId,
    CompleteAgentService,
};
use agentdash_integration_api::{
    AgentDashIntegration, CompleteAgentContributionError, CompleteAgentOfferProvenance,
    CompleteAgentPlacementRequirement, CompleteAgentRegistrationContribution,
    CompleteAgentServiceFactory, CompleteAgentServiceFactoryError,
};

use crate::{RemoteCompleteAgentService, RuntimeWirePlacement};

struct RemoteCompleteAgentServiceFactory {
    instance_id: AgentServiceInstanceId,
    local_binding_generation: AgentBindingGeneration,
    target: RuntimeWireAgentBindingTarget,
    placement: Arc<dyn RuntimeWirePlacement>,
    callbacks: Arc<dyn AgentHostCallbacks>,
}

#[async_trait::async_trait]
impl CompleteAgentServiceFactory for RemoteCompleteAgentServiceFactory {
    async fn materialize(
        &self,
    ) -> Result<Arc<dyn CompleteAgentService>, CompleteAgentServiceFactoryError> {
        Ok(RemoteCompleteAgentRegistration::new(
            self.instance_id.clone(),
            self.local_binding_generation,
            self.target.clone(),
            self.placement.clone(),
            self.callbacks.clone(),
        )
        .service())
    }
}

pub struct RemoteCompleteAgentIntegration {
    registration: CompleteAgentRegistrationContribution,
}

impl RemoteCompleteAgentIntegration {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        expected_descriptor: AgentServiceDescriptor,
        instance_id: AgentServiceInstanceId,
        host_id: impl Into<String>,
        transport_id: impl Into<String>,
        offer_provenance: CompleteAgentOfferProvenance,
        local_binding_generation: AgentBindingGeneration,
        target: RuntimeWireAgentBindingTarget,
        placement: Arc<dyn RuntimeWirePlacement>,
        callbacks: Arc<dyn AgentHostCallbacks>,
    ) -> Result<Self, CompleteAgentContributionError> {
        Ok(Self {
            registration: remote_complete_agent_contribution(
                expected_descriptor,
                instance_id,
                host_id,
                transport_id,
                offer_provenance,
                local_binding_generation,
                target,
                placement,
                callbacks,
            )?,
        })
    }
}

impl AgentDashIntegration for RemoteCompleteAgentIntegration {
    fn name(&self) -> &str {
        &self.registration.offer_provenance.publisher_integration
    }

    fn complete_agent_registrations(&self) -> Vec<CompleteAgentRegistrationContribution> {
        vec![self.registration.clone()]
    }
}

#[allow(clippy::too_many_arguments)]
pub fn remote_complete_agent_contribution(
    expected_descriptor: AgentServiceDescriptor,
    instance_id: AgentServiceInstanceId,
    host_id: impl Into<String>,
    transport_id: impl Into<String>,
    offer_provenance: CompleteAgentOfferProvenance,
    local_binding_generation: AgentBindingGeneration,
    target: RuntimeWireAgentBindingTarget,
    placement: Arc<dyn RuntimeWirePlacement>,
    callbacks: Arc<dyn AgentHostCallbacks>,
) -> Result<CompleteAgentRegistrationContribution, CompleteAgentContributionError> {
    let placement_requirement = CompleteAgentPlacementRequirement::Remote {
        host_id: host_id.into(),
        transport_id: transport_id.into(),
    };
    CompleteAgentRegistrationContribution::new(
        expected_descriptor,
        instance_id.clone(),
        placement_requirement,
        offer_provenance,
        Arc::new(RemoteCompleteAgentServiceFactory {
            instance_id,
            local_binding_generation,
            target,
            placement,
            callbacks,
        }),
    )
}

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
