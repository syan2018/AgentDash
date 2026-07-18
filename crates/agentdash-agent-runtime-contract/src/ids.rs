use std::{fmt, str::FromStr};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use ts_rs::TS;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("{type_name} must not be empty")]
pub struct InvalidRuntimeId {
    type_name: &'static str,
}

macro_rules! runtime_id {
    ($name:ident) => {
        #[derive(
            Debug,
            Clone,
            PartialEq,
            Eq,
            PartialOrd,
            Ord,
            Hash,
            Serialize,
            Deserialize,
            JsonSchema,
            TS,
        )]
        #[serde(transparent)]
        #[schemars(transparent)]
        pub struct $name(String);

        impl $name {
            pub fn new(value: impl Into<String>) -> Result<Self, InvalidRuntimeId> {
                let value = value.into();
                if value.trim().is_empty() {
                    return Err(InvalidRuntimeId {
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
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl FromStr for $name {
            type Err = InvalidRuntimeId;

            fn from_str(value: &str) -> Result<Self, Self::Err> {
                Self::new(value)
            }
        }
    };
}

runtime_id!(RuntimeThreadId);
runtime_id!(PresentationThreadId);
runtime_id!(PresentationTurnId);
runtime_id!(PresentationItemId);
runtime_id!(RuntimeTurnId);
runtime_id!(RuntimeItemId);
runtime_id!(RuntimeInteractionId);
runtime_id!(RuntimeOperationId);
runtime_id!(RuntimeBindingId);
runtime_id!(RuntimeRecoveryIntentId);
runtime_id!(RuntimeServiceInstanceId);
runtime_id!(HostIncarnationId);
runtime_id!(ContextCheckpointId);
runtime_id!(ContextCandidateId);
runtime_id!(ContextCompactionId);
runtime_id!(ContextActivationId);
runtime_id!(ContextDigest);
runtime_id!(DriverContextRevision);
runtime_id!(DriverThreadId);
runtime_id!(DriverTurnId);
runtime_id!(DriverItemId);
runtime_id!(DriverRequestId);
runtime_id!(DriverBindingId);
runtime_id!(IdempotencyKey);
runtime_id!(ProfileDigest);
runtime_id!(SurfaceDigest);
runtime_id!(HookDefinitionId);
runtime_id!(HookRunId);
runtime_id!(HookEffectId);
runtime_id!(HookPlanDigest);
runtime_id!(RuntimeTerminalHookEffectHandlerType);
runtime_id!(RuntimeTerminalHookEffectHandlerId);
runtime_id!(RuntimeHookEffectKind);
runtime_id!(RuntimeTransientEventId);
runtime_id!(RuntimePayloadDigest);

macro_rules! revision {
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
            TS,
        )]
        #[serde(transparent)]
        #[schemars(transparent)]
        pub struct $name(pub u64);
    };
}

revision!(RuntimeRevision);
revision!(RuntimeDriverGeneration);
revision!(BindingEpoch);
revision!(ContextRevision);
revision!(ContextRecipeRevision);
revision!(ThreadSettingsRevision);
revision!(ToolSetRevision);
revision!(SurfaceRevision);
revision!(HookPlanRevision);
revision!(EventSequence);
revision!(RuntimeTransientSequence);
revision!(OperationSequence);
revision!(RuntimeProjectionRevision);
revision!(RuntimeChangeSequence);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_reject_empty_values() {
        assert!(RuntimeThreadId::new("  ").is_err());
    }

    #[test]
    fn canonical_and_driver_ids_have_distinct_types() {
        fn canonical(_: RuntimeThreadId) {}
        fn source(_: DriverThreadId) {}

        canonical(RuntimeThreadId::new("thread-1").expect("valid id"));
        source(DriverThreadId::new("source-thread-1").expect("valid id"));
    }
}
