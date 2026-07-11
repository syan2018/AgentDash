use std::{collections::BTreeMap, sync::Arc};

use agentdash_integration_api::{
    AgentRuntimeDriverContribution, AgentRuntimeDriverFactory, AgentServiceDefinition,
    AgentServiceDefinitionId,
};
use jsonschema::validator_for;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum DefinitionRegistryError {
    #[error("Agent service definition is duplicated: {definition_id}")]
    DuplicateDefinition {
        definition_id: AgentServiceDefinitionId,
    },
    #[error("Agent service factory key does not match definition {definition_id}")]
    FactoryKeyMismatch {
        definition_id: AgentServiceDefinitionId,
    },
    #[error("Agent service schema digest does not match definition {definition_id}")]
    SchemaDigestMismatch {
        definition_id: AgentServiceDefinitionId,
    },
    #[error("Agent service definition {definition_id} has no supported protocol revision")]
    MissingProtocolRevision {
        definition_id: AgentServiceDefinitionId,
    },
    #[error("Agent service definition {definition_id} is invalid: {reason}")]
    InvalidDefinition {
        definition_id: AgentServiceDefinitionId,
        reason: String,
    },
    #[error("Agent service definition was not registered: {definition_id}")]
    UnknownDefinition {
        definition_id: AgentServiceDefinitionId,
    },
}

#[derive(Clone)]
struct RegisteredDefinition {
    definition: AgentServiceDefinition,
    factory: Arc<dyn AgentRuntimeDriverFactory>,
}

#[derive(Clone, Default)]
pub struct AgentServiceDefinitionRegistry {
    definitions: BTreeMap<AgentServiceDefinitionId, RegisteredDefinition>,
}

impl AgentServiceDefinitionRegistry {
    pub fn collect(
        contributions: impl IntoIterator<Item = AgentRuntimeDriverContribution>,
    ) -> Result<Self, DefinitionRegistryError> {
        let mut registry = Self::default();
        for contribution in contributions {
            registry.register(contribution)?;
        }
        Ok(registry)
    }

    fn register(
        &mut self,
        contribution: AgentRuntimeDriverContribution,
    ) -> Result<(), DefinitionRegistryError> {
        let definition_id = contribution.definition.provenance.definition_id.clone();
        if self.definitions.contains_key(&definition_id) {
            return Err(DefinitionRegistryError::DuplicateDefinition { definition_id });
        }
        if contribution.factory.factory_key() != &contribution.definition.factory_key {
            return Err(DefinitionRegistryError::FactoryKeyMismatch { definition_id });
        }
        if contribution
            .definition
            .supported_protocol_revisions
            .is_empty()
        {
            return Err(DefinitionRegistryError::MissingProtocolRevision { definition_id });
        }
        if contribution
            .definition
            .supported_protocol_revisions
            .contains(&0)
            || contribution
                .definition
                .provenance
                .publisher_integration
                .trim()
                .is_empty()
            || contribution
                .definition
                .provenance
                .service_version
                .trim()
                .is_empty()
        {
            return Err(DefinitionRegistryError::InvalidDefinition {
                definition_id,
                reason: "publisher, service version, and positive protocol revisions are required"
                    .to_string(),
            });
        }
        validator_for(&contribution.definition.config_schema).map_err(|error| {
            DefinitionRegistryError::InvalidDefinition {
                definition_id: definition_id.clone(),
                reason: format!("config schema is invalid: {error}"),
            }
        })?;
        let mut slots = std::collections::BTreeSet::new();
        for slot in &contribution.definition.credential_slots {
            if slot.purpose.trim().is_empty() || !slots.insert(slot.slot.clone()) {
                return Err(DefinitionRegistryError::InvalidDefinition {
                    definition_id,
                    reason: "credential slots require unique identities and non-empty purposes"
                        .to_string(),
                });
            }
        }
        let actual_schema_digest = schema_digest(&contribution.definition.config_schema);
        if actual_schema_digest != contribution.definition.config_schema_digest.as_str() {
            return Err(DefinitionRegistryError::SchemaDigestMismatch { definition_id });
        }
        self.definitions.insert(
            definition_id,
            RegisteredDefinition {
                definition: contribution.definition,
                factory: contribution.factory,
            },
        );
        Ok(())
    }

    pub fn definitions(&self) -> Vec<AgentServiceDefinition> {
        self.definitions
            .values()
            .map(|registered| registered.definition.clone())
            .collect()
    }

    pub fn definition(
        &self,
        id: &AgentServiceDefinitionId,
    ) -> Result<AgentServiceDefinition, DefinitionRegistryError> {
        self.definitions
            .get(id)
            .map(|registered| registered.definition.clone())
            .ok_or_else(|| DefinitionRegistryError::UnknownDefinition {
                definition_id: id.clone(),
            })
    }

    pub fn factory(
        &self,
        id: &AgentServiceDefinitionId,
    ) -> Result<Arc<dyn AgentRuntimeDriverFactory>, DefinitionRegistryError> {
        self.definitions
            .get(id)
            .map(|registered| registered.factory.clone())
            .ok_or_else(|| DefinitionRegistryError::UnknownDefinition {
                definition_id: id.clone(),
            })
    }
}

pub fn schema_digest(value: &serde_json::Value) -> String {
    agentdash_integration_api::agent_service_schema_digest(value)
}

pub fn canonical_json(value: &serde_json::Value) -> Vec<u8> {
    fn canonicalize(value: &serde_json::Value) -> serde_json::Value {
        match value {
            serde_json::Value::Object(object) => {
                let mut entries = object.iter().collect::<Vec<_>>();
                entries.sort_by(|left, right| left.0.cmp(right.0));
                let mut canonical = serde_json::Map::new();
                for (key, value) in entries {
                    canonical.insert(key.clone(), canonicalize(value));
                }
                serde_json::Value::Object(canonical)
            }
            serde_json::Value::Array(items) => {
                serde_json::Value::Array(items.iter().map(canonicalize).collect())
            }
            other => other.clone(),
        }
    }

    serde_json::to_vec(&canonicalize(value)).unwrap_or_default()
}
