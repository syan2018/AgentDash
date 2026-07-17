//! AgentDash-owned canonical vocabulary shared by the managed runtime and drivers.
//!
//! This crate deliberately has no application, repository, transport, or vendor dependencies.

pub mod availability;
pub mod command;
pub mod context;
pub mod driver;
pub mod error;
pub mod event;
pub mod gateway;
pub mod hook_plan;
pub mod ids;
pub mod profile;
pub mod snapshot;
pub mod terminal_hook_effect;

pub use availability::*;
pub use command::*;
pub use context::*;
pub use driver::*;
pub use error::*;
pub use event::*;
pub use gateway::*;
pub use hook_plan::*;
pub use ids::*;
pub use profile::*;
pub use snapshot::*;
pub use terminal_hook_effect::*;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Root used by the owned TypeScript and JSON Schema generator.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct RuntimeContractSchema {
    pub command: RuntimeCommandEnvelope,
    pub operation_receipt: OperationReceipt,
    pub execute_error: RuntimeExecuteError,
    pub snapshot_query: RuntimeSnapshotQuery,
    pub event: RuntimeEventEnvelope,
    pub immutable_presentation_event: ImmutablePresentationEvent,
    pub journal_record: RuntimeJournalRecord,
    pub event_subscription: RuntimeEventSubscription,
    pub snapshot: RuntimeSnapshot,
    pub snapshot_result: RuntimeSnapshotResult,
    pub snapshot_error: RuntimeSnapshotError,
    pub subscribe_error: RuntimeSubscribeError,
    pub availability_state: AvailabilityState,
    pub command_availability: CommandAvailability,
    pub effective_profile: EffectiveRuntimeProfile,
    pub hook_requirement: HookRequirement,
    pub driver_describe_request: DriverDescribeRequest,
    pub descriptor: RuntimeDescriptor,
    pub driver_bind_request: DriverBindRequest,
    pub driver_binding: DriverBinding,
    pub driver_command: DriverCommandEnvelope,
    pub driver_dispatch_receipt: DriverDispatchReceipt,
    pub driver_event: DriverEventEnvelope,
    pub driver_inspection_query: DriverInspectionQuery,
    pub driver_inspection: DriverInspection,
    pub driver_error: DriverError,
}

#[cfg(test)]
mod schema_tests {
    use super::*;

    #[test]
    fn json_schema_covers_every_public_contract_family() {
        let schema = schemars::schema_for!(RuntimeContractSchema);
        let schema = serde_json::to_value(schema).expect("serialize runtime contract schema");
        let properties = schema
            .get("properties")
            .and_then(serde_json::Value::as_object)
            .expect("schema root properties");
        for family in [
            "command",
            "operation_receipt",
            "execute_error",
            "snapshot_query",
            "event",
            "immutable_presentation_event",
            "journal_record",
            "event_subscription",
            "snapshot",
            "snapshot_error",
            "subscribe_error",
            "availability_state",
            "command_availability",
            "effective_profile",
            "hook_requirement",
            "driver_describe_request",
            "descriptor",
            "driver_bind_request",
            "driver_binding",
            "driver_command",
            "driver_dispatch_receipt",
            "driver_event",
            "driver_inspection_query",
            "driver_inspection",
            "driver_error",
        ] {
            assert!(properties.contains_key(family), "missing {family} schema");
        }
    }

    #[test]
    fn presentation_event_schema_is_the_exhaustive_backbone_union() {
        let schema = schemars::schema_for!(RuntimeContractSchema);
        let schema = serde_json::to_value(schema).expect("serialize runtime contract schema");
        let presentation = schema
            .pointer("/properties/immutable_presentation_event")
            .expect("immutable presentation schema root");
        assert!(
            presentation
                .to_string()
                .contains("ImmutablePresentationEvent"),
            "presentation root must reference its typed definition"
        );
        let backbone = schema
            .pointer("/$defs/BackboneEvent")
            .expect("BackboneEvent must be present in typed schema definitions");
        let variants = backbone
            .get("oneOf")
            .and_then(serde_json::Value::as_array)
            .expect("BackboneEvent must be an exhaustive union");
        assert!(
            variants.len() >= 16,
            "BackboneEvent variants were collapsed"
        );
        let backbone_json = backbone.to_string();
        for discriminant in [
            "agent_message_delta",
            "item_completed",
            "user_input_submitted",
            "approval_request",
            "platform",
        ] {
            assert!(
                backbone_json.contains(discriminant),
                "BackboneEvent schema is missing {discriminant}"
            );
        }
    }
}
