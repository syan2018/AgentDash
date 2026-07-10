use std::{collections::BTreeMap, fmt, sync::Arc};

use agentdash_agent_runtime_contract::{
    AgentRuntimeDriver, RuntimeDriverGeneration, RuntimeProfile, RuntimeServiceInstanceId,
};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

macro_rules! integration_id {
    ($name:ident) => {
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            pub fn new(value: impl Into<String>) -> Result<Self, InvalidAgentServiceId> {
                let value = value.into();
                if value.trim().is_empty() {
                    return Err(InvalidAgentServiceId {
                        type_name: stringify!($name),
                    });
                }
                Ok(Self(value))
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(&self.0)
            }
        }
    };
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("{type_name} must not be empty")]
pub struct InvalidAgentServiceId {
    type_name: &'static str,
}

integration_id!(AgentServiceDefinitionId);
integration_id!(AgentServiceOfferId);
integration_id!(AgentServiceBuildDigest);
integration_id!(AgentServiceSchemaDigest);
integration_id!(AgentRuntimeFactoryKey);
integration_id!(AgentRuntimeCredentialSlot);
integration_id!(AgentRuntimeCredentialRef);
integration_id!(AgentRuntimePlacementId);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct AgentServiceProvenance {
    pub definition_id: AgentServiceDefinitionId,
    pub publisher_integration: String,
    pub service_version: String,
    pub build_digest: AgentServiceBuildDigest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CredentialSlotDefinition {
    pub slot: AgentRuntimeCredentialSlot,
    pub purpose: String,
    pub required: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct AgentServiceDefinition {
    pub provenance: AgentServiceProvenance,
    pub factory_key: AgentRuntimeFactoryKey,
    pub supported_protocol_revisions: Vec<u32>,
    pub config_schema: Value,
    pub config_schema_digest: AgentServiceSchemaDigest,
    pub credential_slots: Vec<CredentialSlotDefinition>,
    pub service_profile_upper_bound: RuntimeProfile,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AgentRuntimePlacement {
    InProcess,
    LocalProcess {
        host_id: String,
    },
    Remote {
        host_id: String,
        transport_id: AgentRuntimePlacementId,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ActivatedAgentServiceInstance {
    pub instance_id: RuntimeServiceInstanceId,
    pub instance_revision: u64,
    pub generation: RuntimeDriverGeneration,
    pub definition: AgentServiceDefinition,
    pub config: Value,
    pub credentials: BTreeMap<AgentRuntimeCredentialSlot, AgentRuntimeCredentialRef>,
    pub placement: AgentRuntimePlacement,
}

#[derive(Clone, PartialEq, Eq)]
pub struct CredentialLease {
    pub slot: AgentRuntimeCredentialSlot,
    pub purpose: String,
    pub secret: String,
}

impl fmt::Debug for CredentialLease {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CredentialLease")
            .field("slot", &self.slot)
            .field("purpose", &self.purpose)
            .field("secret", &"[REDACTED]")
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CredentialResolveError {
    #[error("credential reference is unavailable for slot {slot}: {reason}")]
    Unavailable {
        slot: AgentRuntimeCredentialSlot,
        reason: String,
    },
    #[error("credential purpose is not allowed for slot {slot}")]
    PurposeDenied { slot: AgentRuntimeCredentialSlot },
}

#[async_trait]
pub trait AgentRuntimeCredentialBroker: Send + Sync {
    async fn resolve(
        &self,
        slot: &AgentRuntimeCredentialSlot,
        reference: &AgentRuntimeCredentialRef,
        purpose: &str,
    ) -> Result<CredentialLease, CredentialResolveError>;
}

#[derive(Clone)]
pub struct RuntimeDriverHostPorts {
    pub credentials: Arc<dyn AgentRuntimeCredentialBroker>,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum DriverFactoryError {
    #[error("driver configuration is invalid: {reason}")]
    InvalidConfiguration { reason: String },
    #[error("driver credential is unavailable for slot {slot}: {reason}")]
    CredentialUnavailable {
        slot: AgentRuntimeCredentialSlot,
        reason: String,
    },
    #[error("driver could not be created: {reason}")]
    Unavailable { reason: String, retryable: bool },
}

#[async_trait]
pub trait AgentRuntimeDriverFactory: Send + Sync {
    fn factory_key(&self) -> &AgentRuntimeFactoryKey;

    async fn create(
        &self,
        instance: ActivatedAgentServiceInstance,
        host: RuntimeDriverHostPorts,
    ) -> Result<Arc<dyn AgentRuntimeDriver>, DriverFactoryError>;
}

#[derive(Clone)]
pub struct AgentRuntimeDriverContribution {
    pub definition: AgentServiceDefinition,
    pub factory: Arc<dyn AgentRuntimeDriverFactory>,
}
