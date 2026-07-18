use std::{fmt, str::FromStr};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("{type_name} must not be empty")]
pub struct InvalidAgentServiceId {
    type_name: &'static str,
}

macro_rules! service_id {
    ($name:ident) => {
        #[derive(
            Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
        )]
        #[serde(transparent)]
        #[schemars(transparent)]
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

            pub fn into_inner(self) -> String {
                self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(&self.0)
            }
        }

        impl FromStr for $name {
            type Err = InvalidAgentServiceId;

            fn from_str(value: &str) -> Result<Self, Self::Err> {
                Self::new(value)
            }
        }
    };
}

service_id!(AgentServiceDefinitionId);
service_id!(AgentServiceInstanceId);
service_id!(AgentSourceCoordinate);
service_id!(AgentSourceRevision);
service_id!(AgentSourceCursor);
service_id!(AgentCommandId);
service_id!(AgentEffectIdentity);
service_id!(AgentIdempotencyKey);
service_id!(AgentTurnId);
service_id!(AgentItemId);
service_id!(AgentInteractionId);
service_id!(AgentContextPackageId);
service_id!(AgentContextSourceCoordinate);
service_id!(AgentContextSourceRevision);
service_id!(AgentSurfaceDigest);
service_id!(AgentProfileDigest);
service_id!(AgentPayloadDigest);
service_id!(AgentCallbackRouteId);
service_id!(AgentToolName);
service_id!(AgentHookDefinitionId);

macro_rules! service_revision {
    ($name:ident) => {
        #[derive(
            Debug,
            Clone,
            Copy,
            Default,
            PartialEq,
            Eq,
            PartialOrd,
            Ord,
            Hash,
            Serialize,
            Deserialize,
            JsonSchema,
        )]
        #[serde(transparent)]
        #[schemars(transparent)]
        pub struct $name(pub u64);
    };
}

service_revision!(AgentBindingGeneration);
service_revision!(AgentSnapshotRevision);
service_revision!(AgentSurfaceRevision);
service_revision!(AgentContextSchemaVersion);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_reject_blank_values() {
        assert!(AgentSourceCoordinate::new("  ").is_err());
        assert!(AgentEffectIdentity::new("").is_err());
    }

    #[test]
    fn source_and_effect_identities_are_distinct_types() {
        fn source(_: AgentSourceCoordinate) {}
        fn effect(_: AgentEffectIdentity) {}

        source(AgentSourceCoordinate::new("source-1").expect("source"));
        effect(AgentEffectIdentity::new("effect-1").expect("effect"));
    }
}
