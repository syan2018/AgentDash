use std::sync::Arc;

use agentdash_agent_runtime_wire::RuntimeWireAgentBindingTarget;
use agentdash_agent_service_api::{
    AgentHostCallbacks, AgentServiceDescriptor, AgentServiceInstanceId, CompleteAgentService,
};
use agentdash_integration_api::{
    AgentDashIntegration, CompleteAgentContributionError, CompleteAgentPlacementRequirement,
    CompleteAgentRegistrationClaim, CompleteAgentRegistrationContribution,
    CompleteAgentRemoteBindingMapping, CompleteAgentServiceFactory,
    CompleteAgentServiceFactoryError,
};

use crate::{RemoteCompleteAgentService, RuntimeWirePlacement};

struct RemoteCompleteAgentServiceFactory {
    binding: CompleteAgentRemoteBindingMapping,
    placement: Arc<dyn RuntimeWirePlacement>,
    callbacks: Arc<dyn AgentHostCallbacks>,
}

#[async_trait::async_trait]
impl CompleteAgentServiceFactory for RemoteCompleteAgentServiceFactory {
    async fn materialize(
        &self,
    ) -> Result<Arc<dyn CompleteAgentService>, CompleteAgentServiceFactoryError> {
        Ok(RemoteCompleteAgentRegistration::new(
            self.binding.local_service_instance_id.clone(),
            RuntimeWireAgentBindingTarget {
                service_instance_id: self.binding.remote_service_instance_id.clone(),
                binding_generation: self.binding.remote_binding_generation,
            },
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
        declared_descriptor: AgentServiceDescriptor,
        instance_id: AgentServiceInstanceId,
        host_id: impl Into<String>,
        transport_id: impl Into<String>,
        registration_claim: CompleteAgentRegistrationClaim,
        remote_binding: CompleteAgentRemoteBindingMapping,
        target: RuntimeWireAgentBindingTarget,
        placement: Arc<dyn RuntimeWirePlacement>,
        callbacks: Arc<dyn AgentHostCallbacks>,
    ) -> Result<Self, CompleteAgentContributionError> {
        Ok(Self {
            registration: remote_complete_agent_contribution(
                declared_descriptor,
                instance_id,
                host_id,
                transport_id,
                registration_claim,
                remote_binding,
                target,
                placement,
                callbacks,
            )?,
        })
    }
}

impl AgentDashIntegration for RemoteCompleteAgentIntegration {
    fn name(&self) -> &str {
        &self
            .registration
            .facts()
            .registration_claim()
            .publisher_integration
    }

    fn complete_agent_registrations(&self) -> Vec<CompleteAgentRegistrationContribution> {
        vec![self.registration.clone()]
    }
}

#[allow(clippy::too_many_arguments)]
pub fn remote_complete_agent_contribution(
    declared_descriptor: AgentServiceDescriptor,
    instance_id: AgentServiceInstanceId,
    host_id: impl Into<String>,
    transport_id: impl Into<String>,
    registration_claim: CompleteAgentRegistrationClaim,
    remote_binding: CompleteAgentRemoteBindingMapping,
    target: RuntimeWireAgentBindingTarget,
    placement: Arc<dyn RuntimeWirePlacement>,
    callbacks: Arc<dyn AgentHostCallbacks>,
) -> Result<CompleteAgentRegistrationContribution, CompleteAgentContributionError> {
    validate_wire_target(&remote_binding, &target)?;
    let placement_requirement = CompleteAgentPlacementRequirement::Remote {
        host_id: host_id.into(),
        transport_id: transport_id.into(),
    };
    CompleteAgentRegistrationContribution::new(
        declared_descriptor,
        instance_id,
        placement_requirement,
        Some(remote_binding.clone()),
        registration_claim,
        Arc::new(RemoteCompleteAgentServiceFactory {
            binding: remote_binding,
            placement,
            callbacks,
        }),
    )
}

fn validate_wire_target(
    binding: &CompleteAgentRemoteBindingMapping,
    target: &RuntimeWireAgentBindingTarget,
) -> Result<(), CompleteAgentContributionError> {
    if target.service_instance_id != binding.remote_service_instance_id {
        return Err(CompleteAgentContributionError::RemoteBindingMismatch {
            coordinate: "remote_service_instance_id".to_owned(),
            expected: binding.remote_service_instance_id.to_string(),
            actual: target.service_instance_id.to_string(),
        });
    }
    if target.binding_generation != binding.remote_binding_generation {
        return Err(CompleteAgentContributionError::RemoteBindingMismatch {
            coordinate: "remote_binding_generation".to_owned(),
            expected: binding.remote_binding_generation.0.to_string(),
            actual: target.binding_generation.0.to_string(),
        });
    }
    Ok(())
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
        target: RuntimeWireAgentBindingTarget,
        placement: Arc<dyn RuntimeWirePlacement>,
        callbacks: Arc<dyn AgentHostCallbacks>,
    ) -> Self {
        Self {
            instance_id,
            service: RemoteCompleteAgentService::new(target, placement, callbacks),
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

#[cfg(test)]
mod tests {
    use agentdash_agent_service_api::AgentBindingGeneration;

    use super::*;

    fn instance(value: &str) -> AgentServiceInstanceId {
        AgentServiceInstanceId::new(value).expect("instance")
    }

    fn mapping() -> CompleteAgentRemoteBindingMapping {
        CompleteAgentRemoteBindingMapping {
            local_service_instance_id: instance("local-agent"),
            remote_service_instance_id: instance("remote-agent"),
            remote_binding_generation: AgentBindingGeneration(9),
        }
    }

    fn target(service_instance_id: &str, generation: u64) -> RuntimeWireAgentBindingTarget {
        RuntimeWireAgentBindingTarget {
            service_instance_id: instance(service_instance_id),
            binding_generation: AgentBindingGeneration(generation),
        }
    }

    #[test]
    fn explicit_remote_generation_mapping_pins_the_wire_target() {
        let mapping = mapping();

        validate_wire_target(&mapping, &target("remote-agent", 9))
            .expect("target must match declared remote side");
        assert_eq!(mapping.local_service_instance_id, instance("local-agent"));
        assert_eq!(mapping.remote_service_instance_id, instance("remote-agent"));
        assert_eq!(mapping.remote_binding_generation, AgentBindingGeneration(9));
    }

    #[test]
    fn remote_service_identity_mismatch_is_rejected_before_factory_side_effects() {
        let error = validate_wire_target(&mapping(), &target("another-agent", 9))
            .expect_err("foreign target identity");

        assert!(matches!(
            error,
            CompleteAgentContributionError::RemoteBindingMismatch {
                coordinate,
                ..
            } if coordinate == "remote_service_instance_id"
        ));
    }

    #[test]
    fn old_remote_generation_is_fenced_before_factory_side_effects() {
        let error = validate_wire_target(&mapping(), &target("remote-agent", 8))
            .expect_err("stale target generation");

        assert!(matches!(
            error,
            CompleteAgentContributionError::RemoteBindingMismatch {
                coordinate,
                expected,
                actual,
            } if coordinate == "remote_binding_generation"
                && expected == "9"
                && actual == "8"
        ));
    }
}
