use agentdash_agent_runtime_contract::{
    AvailabilityState, ConfigurationBoundary, ContextCapability, ContextFidelity, ContextProfile,
    DeliveryMechanism, DriverCommandEnvelope, HookAction, HookFailurePolicy, HookPoint,
    HookPointCapability, HookProfile, HookRequirement, InputModality, InputProfile,
    InstructionProfile, InteractionProfile, LifecycleCapability, ProfileProvenance,
    ReferenceRuntimeClass, RuntimeCommand, RuntimeCommandKind, RuntimeDescriptor,
    RuntimeDriverGeneration, RuntimeEvent, RuntimeEventEnvelope, RuntimeInput, RuntimeItemContent,
    RuntimeOperationId, RuntimeOperationTerminal, RuntimeProfile, RuntimeRevision,
    RuntimeThreadStatus, RuntimeTurnId, SemanticStrength, TelemetryCapability, ToolChannel,
    ToolProfile, WorkspaceCapability, WorkspaceProfile, command_availability,
    intersect_profile_layers,
};

use super::*;

fn id<T: std::str::FromStr>(value: &str) -> T
where
    T::Err: std::fmt::Debug,
{
    value.parse().expect("valid id")
}

fn envelope(event: RuntimeEvent) -> RuntimeEventEnvelope {
    envelope_for_thread("thread-1", event)
}

fn envelope_for_thread(thread_id: &str, event: RuntimeEvent) -> RuntimeEventEnvelope {
    RuntimeEventEnvelope {
        thread_id: id(thread_id),
        sequence: None,
        revision: RuntimeRevision(1),
        event,
    }
}

fn full_profile() -> RuntimeProfile {
    RuntimeProfile {
        reference_class: ReferenceRuntimeClass::ManagedThread,
        input: InputProfile {
            modalities: set([
                InputModality::Text,
                InputModality::Image,
                InputModality::Structured,
            ]),
        },
        instruction: InstructionProfile {
            channels: set([]),
            configuration_boundary: ConfigurationBoundary::HotReplace,
        },
        tools: ToolProfile {
            channels: set([ToolChannel::DirectCallback, ToolChannel::McpFacade]),
            configuration_boundary: ConfigurationBoundary::HotReplace,
            cancellation: true,
        },
        workspace: WorkspaceProfile {
            capabilities: set([WorkspaceCapability::Read, WorkspaceCapability::Write]),
            mechanism: DeliveryMechanism::Native,
        },
        interactions: InteractionProfile {
            kinds: set([]),
            durable_correlation: true,
        },
        lifecycle: set([
            LifecycleCapability::ThreadStart,
            LifecycleCapability::TurnStart,
            LifecycleCapability::TurnSteer,
            LifecycleCapability::TurnInterrupt,
            LifecycleCapability::ToolSetReplace,
        ]),
        hooks: HookProfile {
            points: Vec::new(),
            configuration_boundary: ConfigurationBoundary::HotReplace,
        },
        context: ContextProfile {
            capabilities: set([
                ContextCapability::Read,
                ContextCapability::PrepareCompaction,
                ContextCapability::ActivateCheckpoint,
            ]),
            fidelity: ContextFidelity::PlatformExact,
            activation_idempotent: true,
        },
        telemetry_config: set([TelemetryCapability::Deltas]),
    }
}

#[test]
fn exactly_one_terminal_trace_is_accepted() {
    let operation_id: RuntimeOperationId = id("operation-1");
    let turn_id: RuntimeTurnId = id("turn-1");
    let mut validator = RuntimeTraceValidator::default();
    validator
        .observe(&envelope(RuntimeEvent::OperationAccepted {
            operation_id: operation_id.clone(),
        }))
        .expect("accept operation");
    validator
        .observe(&envelope(RuntimeEvent::TurnStarted {
            turn_id: turn_id.clone(),
        }))
        .expect("start turn");
    validator
        .observe(&envelope(RuntimeEvent::TurnTerminal {
            turn_id,
            terminal: agentdash_agent_runtime_contract::RuntimeTurnTerminal::Lost,
            message: Some("driver result became unknowable".to_string()),
        }))
        .expect("terminal turn");
    validator
        .observe(&envelope(RuntimeEvent::OperationTerminal {
            operation_id,
            terminal: RuntimeOperationTerminal::Lost {
                retryable: false,
                message: Some("lost".to_string()),
            },
        }))
        .expect("terminal operation");
    validator.finish().expect("complete trace");
}

#[test]
fn retryable_failure_and_lost_are_typed_terminal_outcomes() {
    let retryable_operation: RuntimeOperationId = id("operation-retryable");
    let lost_operation: RuntimeOperationId = id("operation-lost");
    let mut validator = RuntimeTraceValidator::default();
    for operation_id in [&retryable_operation, &lost_operation] {
        validator
            .observe(&envelope(RuntimeEvent::OperationAccepted {
                operation_id: operation_id.clone(),
            }))
            .expect("accept operation");
    }
    let retryable_terminal = RuntimeEvent::OperationTerminal {
        operation_id: retryable_operation,
        terminal: RuntimeOperationTerminal::Failed {
            retryable: true,
            message: Some("service temporarily unavailable".to_string()),
        },
    };
    let lost_terminal = RuntimeEvent::OperationTerminal {
        operation_id: lost_operation,
        terminal: RuntimeOperationTerminal::Lost {
            retryable: false,
            message: Some("driver result is unknowable".to_string()),
        },
    };
    let retryable_json = serde_json::to_value(&retryable_terminal).expect("serialize failure");
    let lost_json = serde_json::to_value(&lost_terminal).expect("serialize lost");
    assert_eq!(
        retryable_json
            .pointer("/terminal/kind")
            .and_then(serde_json::Value::as_str),
        Some("failed")
    );
    assert_eq!(
        retryable_json
            .pointer("/terminal/retryable")
            .and_then(serde_json::Value::as_bool),
        Some(true)
    );
    assert_eq!(
        lost_json
            .pointer("/terminal/kind")
            .and_then(serde_json::Value::as_str),
        Some("lost")
    );
    validator
        .observe(&envelope(retryable_terminal))
        .expect("terminal retryable operation");
    validator
        .observe(&envelope(lost_terminal))
        .expect("terminal lost operation");
    validator.finish().expect("both operations are terminal");
}

#[test]
fn duplicate_operation_acceptance_has_a_distinct_violation() {
    let operation_id: RuntimeOperationId = id("operation-1");
    let mut validator = RuntimeTraceValidator::default();
    validator
        .observe(&envelope(RuntimeEvent::OperationAccepted {
            operation_id: operation_id.clone(),
        }))
        .expect("first acceptance");
    let error = validator
        .observe(&envelope(RuntimeEvent::OperationAccepted {
            operation_id: operation_id.clone(),
        }))
        .expect_err("duplicate acceptance must fail");
    assert_eq!(
        error,
        ConformanceViolation::DuplicateOperationAcceptance(operation_id)
    );
}

#[test]
fn final_item_is_authoritative_and_delta_after_terminal_is_invalid() {
    let turn_id: RuntimeTurnId = id("turn-1");
    let item_id: agentdash_agent_runtime_contract::RuntimeItemId = id("item-1");
    let mut validator = RuntimeTraceValidator::default();
    validator
        .observe(&envelope(RuntimeEvent::ItemStarted {
            turn_id: turn_id.clone(),
            item_id: item_id.clone(),
            initial_content: RuntimeItemContent::AgentMessage {
                text: String::new(),
            },
        }))
        .expect("start item");
    validator
        .observe(&envelope(RuntimeEvent::ItemTerminal {
            turn_id: turn_id.clone(),
            item_id: item_id.clone(),
            terminal: agentdash_agent_runtime_contract::RuntimeItemTerminal::Completed {
                final_content: agentdash_agent_runtime_contract::RuntimeItemContent::AgentMessage {
                    text: "authoritative final".to_string(),
                },
            },
        }))
        .expect("terminal item");
    let error = validator
        .observe(&envelope(RuntimeEvent::ItemDelta {
            turn_id,
            item_id: item_id.clone(),
            delta: "late".to_string(),
        }))
        .expect_err("late delta must fail");
    assert_eq!(error, ConformanceViolation::DeltaAfterItemTerminal(item_id));
}

#[test]
fn item_terminal_cannot_change_thread_or_turn_parent() {
    let item_id: agentdash_agent_runtime_contract::RuntimeItemId = id("item-1");
    let mut validator = RuntimeTraceValidator::default();
    validator
        .observe(&envelope(RuntimeEvent::ItemStarted {
            turn_id: id("turn-1"),
            item_id: item_id.clone(),
            initial_content: RuntimeItemContent::AgentMessage {
                text: String::new(),
            },
        }))
        .expect("start item");

    let error = validator
        .observe(&envelope_for_thread(
            "thread-2",
            RuntimeEvent::ItemTerminal {
                turn_id: id("turn-2"),
                item_id: item_id.clone(),
                terminal: agentdash_agent_runtime_contract::RuntimeItemTerminal::Lost {
                    message: Some("lost".to_string()),
                },
            },
        ))
        .expect_err("cross-parent terminal must fail");
    assert_eq!(error, ConformanceViolation::ItemParentMismatch(item_id));
}

#[test]
fn interaction_terminal_cannot_change_thread_or_turn_parent() {
    let interaction_id: agentdash_agent_runtime_contract::RuntimeInteractionId =
        id("interaction-1");
    let mut validator = RuntimeTraceValidator::default();
    validator
        .observe(&envelope(RuntimeEvent::InteractionRequested {
            turn_id: id("turn-1"),
            item_id: None,
            interaction_id: interaction_id.clone(),
            interaction_kind:
                agentdash_agent_runtime_contract::RuntimeInteractionKind::UserInputRequest,
            prompt: "need input".to_string(),
        }))
        .expect("request interaction");

    let error = validator
        .observe(&envelope_for_thread(
            "thread-2",
            RuntimeEvent::InteractionTerminal {
                turn_id: id("turn-2"),
                interaction_id: interaction_id.clone(),
                terminal: agentdash_agent_runtime_contract::RuntimeInteractionTerminal::Cancelled,
            },
        ))
        .expect_err("cross-parent interaction terminal must fail");
    assert_eq!(
        error,
        ConformanceViolation::InteractionParentMismatch(interaction_id)
    );
}

#[test]
fn profile_intersection_cannot_increase_service_guarantees() {
    let service = full_profile();
    let mut transport = full_profile();
    transport.input.modalities.remove(&InputModality::Image);
    transport.context.fidelity = ContextFidelity::EventProjected;
    transport.tools.configuration_boundary = ConfigurationBoundary::ThreadStart;

    let effective = service.intersect(&transport);
    assert!(!effective.input.modalities.contains(&InputModality::Image));
    assert_eq!(effective.context.fidelity, ContextFidelity::EventProjected);
    assert_eq!(
        effective.tools.configuration_boundary,
        ConfigurationBoundary::ThreadStart
    );
}

#[test]
fn service_transport_and_host_policy_all_constrain_effective_profile() {
    let service = full_profile();
    let mut transport = full_profile();
    transport.input.modalities.remove(&InputModality::Image);
    let mut host = full_profile();
    host.lifecycle.remove(&LifecycleCapability::TurnInterrupt);
    let effective = intersect_profile_layers(
        &service,
        &transport,
        &host,
        ProfileProvenance {
            service_digest: id("service-profile"),
            transport_digest: id("transport-profile"),
            host_policy_digest: id("host-profile"),
        },
    );
    assert!(
        !effective
            .profile
            .input
            .modalities
            .contains(&InputModality::Image)
    );
    assert!(
        !effective
            .profile
            .lifecycle
            .contains(&LifecycleCapability::TurnInterrupt)
    );
}

#[test]
fn availability_uses_typed_profile_predicates() {
    let mut profile = full_profile();
    profile.lifecycle.remove(&LifecycleCapability::TurnSteer);
    let availability = command_availability(
        RuntimeCommandKind::TurnSteer,
        &profile,
        &AvailabilityState {
            thread_status: RuntimeThreadStatus::Active,
            has_active_turn: true,
            has_pending_interaction: false,
        },
    );
    assert!(matches!(
        availability,
        agentdash_agent_runtime_contract::CommandAvailability::Unavailable { .. }
    ));
}

#[test]
fn required_fail_closed_hook_rejects_route_without_fail_closed_guarantee() {
    let profile = HookProfile {
        points: vec![HookPointCapability {
            point: HookPoint::BeforeTool,
            actions: set([HookAction::Block]),
            strength: SemanticStrength::ExactSynchronous,
            mechanism: DeliveryMechanism::Native,
            failure_policies: set([HookFailurePolicy::FailOpenWithDiagnostic]),
            acknowledged: true,
        }],
        configuration_boundary: ConfigurationBoundary::Binding,
    };
    let requirement = HookRequirement {
        point: HookPoint::BeforeTool,
        actions: set([HookAction::Block]),
        minimum_strength: SemanticStrength::ExactSynchronous,
        failure_policy: HookFailurePolicy::FailClosed,
        required: true,
    };

    assert!(!profile.satisfies(&requirement));
}

#[test]
fn retry_durable_effect_must_survive_every_profile_layer() {
    let capability = HookPointCapability {
        point: HookPoint::AfterTurn,
        actions: set([HookAction::EmitEffect]),
        strength: SemanticStrength::ExactDurableBoundary,
        mechanism: DeliveryMechanism::HostAdaptedExact,
        failure_policies: set([HookFailurePolicy::RetryDurableEffect]),
        acknowledged: true,
    };
    let mut service = full_profile();
    service.hooks = HookProfile {
        points: vec![capability.clone()],
        configuration_boundary: ConfigurationBoundary::Binding,
    };
    let mut transport = full_profile();
    transport.hooks = HookProfile {
        points: vec![HookPointCapability {
            failure_policies: set([HookFailurePolicy::FailOpenWithDiagnostic]),
            ..capability
        }],
        configuration_boundary: ConfigurationBoundary::Binding,
    };
    let requirement = HookRequirement {
        point: HookPoint::AfterTurn,
        actions: set([HookAction::EmitEffect]),
        minimum_strength: SemanticStrength::ExactDurableBoundary,
        failure_policy: HookFailurePolicy::RetryDurableEffect,
        required: true,
    };

    let effective = service.intersect(&transport);
    assert!(!effective.hooks.satisfies(&requirement));
}

#[tokio::test]
async fn unsupported_command_has_no_side_effect() {
    let profile = full_profile();
    let descriptor = RuntimeDescriptor {
        protocol_revision: 1,
        service_instance_id: id("service-1"),
        profile,
        profile_digest: id("profile-1"),
    };
    let driver = UnsupportedRecordingDriver::new(descriptor);
    let command = DriverCommandEnvelope {
        request_id: id("request-1"),
        binding_id: id("binding-1"),
        generation: RuntimeDriverGeneration(1),
        source_thread_id: id("source-thread-1"),
        command: RuntimeCommand::TurnStart {
            thread_id: id("thread-1"),
            input: vec![RuntimeInput::Text {
                text: "hello".to_string(),
            }],
        },
    };
    assert_unsupported_before_side_effect(&driver, command)
        .await
        .expect("unsupported must be side-effect free");
}
