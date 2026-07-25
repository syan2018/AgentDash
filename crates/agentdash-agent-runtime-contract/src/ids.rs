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
runtime_id!(RuntimeTurnId);
runtime_id!(RuntimeItemId);
runtime_id!(RuntimeInteractionId);
runtime_id!(RuntimeOperationId);
runtime_id!(RuntimePayloadDigest);
runtime_id!(RuntimeIdempotencyKey);
runtime_id!(RuntimeSourceRef);
runtime_id!(RuntimeContextPackageId);
runtime_id!(RuntimeContextContributionId);
runtime_id!(RuntimeContextSourceRef);
runtime_id!(RuntimeContextSourceRevision);

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
        #[ts(type = "RuntimeU64")]
        pub struct $name(
            #[serde(with = "crate::wire_u64")]
            #[schemars(with = "crate::wire_u64::RuntimeU64")]
            pub u64,
        );
    };
}

revision!(SurfaceRevision);
revision!(RuntimeProjectionRevision);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_reject_empty_values() {
        assert!(RuntimeThreadId::new("  ").is_err());
    }

    #[test]
    fn runtime_identity_families_remain_distinct() {
        fn thread(_: RuntimeThreadId) {}
        fn operation(_: RuntimeOperationId) {}

        thread(RuntimeThreadId::new("thread-1").expect("valid id"));
        operation(RuntimeOperationId::new("operation-1").expect("valid id"));
    }
}
