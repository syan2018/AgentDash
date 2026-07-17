use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{
    ContextCapability, ContextFidelity, LifecycleCapability, RuntimeCommandKind, RuntimeProfile,
    RuntimeThreadStatus,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AvailabilityPredicate {
    Lifecycle {
        capability: LifecycleCapability,
    },
    ActiveTurn,
    NoActiveTurn,
    PendingInteraction,
    Context {
        capability: ContextCapability,
        minimum_fidelity: ContextFidelity,
    },
    ToolHotReplace,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum CommandAvailability {
    Available,
    Unavailable {
        unmet: Vec<AvailabilityPredicate>,
        reason: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct AvailabilityState {
    pub thread_status: RuntimeThreadStatus,
    pub has_active_turn: bool,
    pub has_pending_interaction: bool,
}

pub fn command_availability(
    command: RuntimeCommandKind,
    profile: &RuntimeProfile,
    state: &AvailabilityState,
) -> CommandAvailability {
    let mut unmet = Vec::new();
    let required_lifecycle = match command {
        RuntimeCommandKind::ThreadStart => Some(LifecycleCapability::ThreadStart),
        RuntimeCommandKind::ThreadResume => Some(LifecycleCapability::ThreadResume),
        // ThreadRebind validates ThreadResume against the proposed profile. The currently bound
        // profile belongs to the lost binding and is not its admission authority.
        RuntimeCommandKind::ThreadRebind => None,
        RuntimeCommandKind::ThreadFork => Some(LifecycleCapability::ThreadFork),
        RuntimeCommandKind::TurnStart => Some(LifecycleCapability::TurnStart),
        RuntimeCommandKind::TurnSteer => Some(LifecycleCapability::TurnSteer),
        RuntimeCommandKind::TurnInterrupt => Some(LifecycleCapability::TurnInterrupt),
        RuntimeCommandKind::ToolSetReplace => Some(LifecycleCapability::ToolSetReplace),
        // Surface adoption is a canonical platform transition. A connector can either apply the
        // complete surface natively or accept the tool-set hot-replace projection while Runtime
        // remains the owner of the AgentFrame/context presentation facts.
        RuntimeCommandKind::SurfaceAdopt => None,
        RuntimeCommandKind::ThreadSettingsUpdate
        | RuntimeCommandKind::InteractionRespond
        | RuntimeCommandKind::ContextCompact => None,
    };

    if let Some(capability) = required_lifecycle
        && !profile.lifecycle.contains(&capability)
    {
        unmet.push(AvailabilityPredicate::Lifecycle { capability });
    }

    match command {
        RuntimeCommandKind::TurnStart if state.has_active_turn => {
            unmet.push(AvailabilityPredicate::NoActiveTurn);
        }
        RuntimeCommandKind::SurfaceAdopt => {
            if !profile
                .lifecycle
                .contains(&LifecycleCapability::SurfaceAdopt)
            {
                if !profile
                    .lifecycle
                    .contains(&LifecycleCapability::ToolSetReplace)
                {
                    unmet.push(AvailabilityPredicate::Lifecycle {
                        capability: LifecycleCapability::SurfaceAdopt,
                    });
                }
                if profile.tools.configuration_boundary < crate::ConfigurationBoundary::HotReplace {
                    unmet.push(AvailabilityPredicate::ToolHotReplace);
                }
            }
        }
        RuntimeCommandKind::TurnSteer | RuntimeCommandKind::TurnInterrupt
            if !state.has_active_turn =>
        {
            unmet.push(AvailabilityPredicate::ActiveTurn);
        }
        RuntimeCommandKind::InteractionRespond if !state.has_pending_interaction => {
            unmet.push(AvailabilityPredicate::PendingInteraction);
        }
        RuntimeCommandKind::ContextCompact => {
            for capability in [
                ContextCapability::PrepareCompaction,
                ContextCapability::ActivateCheckpoint,
            ] {
                if !profile.context.capabilities.contains(&capability) {
                    unmet.push(AvailabilityPredicate::Context {
                        capability,
                        minimum_fidelity: ContextFidelity::DriverExact,
                    });
                }
            }
            if profile.context.fidelity < ContextFidelity::DriverExact {
                unmet.push(AvailabilityPredicate::Context {
                    capability: ContextCapability::Read,
                    minimum_fidelity: ContextFidelity::DriverExact,
                });
            }
        }
        RuntimeCommandKind::ToolSetReplace
            if profile.tools.configuration_boundary < crate::ConfigurationBoundary::HotReplace =>
        {
            unmet.push(AvailabilityPredicate::ToolHotReplace);
        }
        _ => {}
    }

    if state.thread_status != RuntimeThreadStatus::Active
        && !matches!(
            command,
            RuntimeCommandKind::ThreadStart
                | RuntimeCommandKind::ThreadResume
                | RuntimeCommandKind::ThreadRebind
                | RuntimeCommandKind::ThreadFork
        )
    {
        return CommandAvailability::Unavailable {
            unmet,
            reason: "thread is not active".to_string(),
        };
    }

    if unmet.is_empty() {
        CommandAvailability::Available
    } else {
        CommandAvailability::Unavailable {
            unmet,
            reason: "bound runtime profile does not satisfy command predicates".to_string(),
        }
    }
}
