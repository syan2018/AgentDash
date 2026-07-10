use std::{collections::BTreeSet, str::FromStr};

use agentdash_agent_runtime::*;
use agentdash_agent_runtime_contract::*;

fn id<T: FromStr>(value: &str) -> T
where
    T::Err: std::fmt::Debug,
{
    value.parse().expect("valid id")
}

fn meta(key: &str, requirement: ContributionRequirement) -> ContributionMeta {
    ContributionMeta {
        key: key.to_string(),
        source: SurfaceSourceRef {
            layer: "workflow".to_string(),
            key: "workflow:test".to_string(),
        },
        priority: 100,
        requirement,
    }
}

fn context_recipe() -> ContextRecipe {
    ContextRecipe {
        revision: ContextRecipeRevision(3),
        provenance: ContextProvenance {
            settings_revision: ThreadSettingsRevision(4),
            tool_set_revision: ToolSetRevision(5),
        },
        source_item_ids: Vec::new(),
    }
}

fn compile(contributions: Vec<CapabilityContribution>) -> AgentSurfaceSnapshot {
    AgentSurfaceCompiler
        .compile(AgentSurfaceCompileInput {
            revision: SurfaceRevision(7),
            context_recipe: context_recipe(),
            tool_set_revision: ToolSetRevision(5),
            hook_plan_revision: HookPlanRevision(2),
            workspace: WorkspaceRequirement {
                capabilities: [WorkspaceCapability::Read, WorkspaceCapability::Write].into(),
                minimum_mechanism: DeliveryMechanism::HostAdaptedExact,
                requirement: ContributionRequirement::Required,
            },
            contributions,
            capability_packs: Vec::new(),
        })
        .expect("compile surface")
}

fn runtime_profile(tool_channels: BTreeSet<ToolChannel>) -> RuntimeProfile {
    RuntimeProfile {
        reference_class: ReferenceRuntimeClass::ManagedThread,
        input: InputProfile {
            modalities: [InputModality::Text].into(),
        },
        instruction: InstructionProfile {
            channels: [InstructionChannel::Developer].into(),
            configuration_boundary: ConfigurationBoundary::Binding,
        },
        tools: ToolProfile {
            channels: tool_channels,
            configuration_boundary: ConfigurationBoundary::Binding,
            cancellation: true,
        },
        workspace: WorkspaceProfile {
            capabilities: [WorkspaceCapability::Read, WorkspaceCapability::Write].into(),
            mechanism: DeliveryMechanism::HostAdaptedExact,
        },
        interactions: InteractionProfile {
            kinds: BTreeSet::new(),
            durable_correlation: true,
        },
        lifecycle: BTreeSet::new(),
        hooks: HookProfile {
            points: Vec::new(),
            configuration_boundary: ConfigurationBoundary::Binding,
        },
        context: ContextProfile {
            capabilities: BTreeSet::new(),
            fidelity: ContextFidelity::PlatformExact,
            activation_idempotent: true,
        },
        telemetry_config: BTreeSet::new(),
    }
}

#[test]
fn compiler_expands_pack_and_preserves_tool_provenance() {
    let tool = ToolContribution {
        meta: meta("tool:workspace.read", ContributionRequirement::Required),
        runtime_name: "workspace_read".to_string(),
        description: "Read a workspace file".to_string(),
        parameters_schema: serde_json::json!({
            "type": "object",
            "properties": { "path": { "type": "string" } },
            "required": ["path"]
        }),
        capability_key: "file_read".to_string(),
        tool_path: "file_read::workspace_read".to_string(),
        allowed_channels: [ToolChannel::DirectCallback, ToolChannel::McpFacade].into(),
        configuration_boundary: ConfigurationBoundary::Binding,
    };
    let input = AgentSurfaceCompileInput {
        revision: SurfaceRevision(7),
        context_recipe: context_recipe(),
        tool_set_revision: ToolSetRevision(5),
        hook_plan_revision: HookPlanRevision(2),
        workspace: WorkspaceRequirement {
            capabilities: [WorkspaceCapability::Read].into(),
            minimum_mechanism: DeliveryMechanism::HostAdaptedExact,
            requirement: ContributionRequirement::Required,
        },
        contributions: Vec::new(),
        capability_packs: vec![CapabilityPack {
            key: "pack:workspace".to_string(),
            contributions: vec![CapabilityContribution::Tool(tool.clone())],
        }],
    };

    let surface = AgentSurfaceCompiler.compile(input).expect("compile pack");

    assert_eq!(surface.tools.tools, vec![tool]);
    assert!(surface.tools.digest.starts_with("sha256:"));
    assert!(surface.digest.as_str().starts_with("sha256:"));
}

#[test]
fn compiler_is_deterministic_across_input_order() {
    let instruction = CapabilityContribution::Instruction(InstructionContribution {
        meta: meta("instruction:developer", ContributionRequirement::Required),
        channel: InstructionChannel::Developer,
        content: "Follow the active workflow".to_string(),
    });
    let context = CapabilityContribution::Context(ContextContribution {
        meta: meta("context:task", ContributionRequirement::Optional),
        blocks: vec![ContextBlock::Instruction {
            text: "Task facts".to_string(),
        }],
        minimum_strength: SemanticStrength::ExactDurableBoundary,
    });

    let left = compile(vec![instruction.clone(), context.clone()]);
    let right = compile(vec![context, instruction]);

    assert_eq!(left.digest, right.digest);
    assert_eq!(left.context.digest, right.context.digest);
}

#[test]
fn conflicting_contribution_key_is_rejected() {
    let first = CapabilityContribution::Instruction(InstructionContribution {
        meta: meta("instruction:developer", ContributionRequirement::Required),
        channel: InstructionChannel::Developer,
        content: "first".to_string(),
    });
    let second = CapabilityContribution::Instruction(InstructionContribution {
        meta: meta("instruction:developer", ContributionRequirement::Required),
        channel: InstructionChannel::Developer,
        content: "second".to_string(),
    });

    let error = AgentSurfaceCompiler
        .compile(AgentSurfaceCompileInput {
            revision: SurfaceRevision(1),
            context_recipe: context_recipe(),
            tool_set_revision: ToolSetRevision(1),
            hook_plan_revision: HookPlanRevision(1),
            workspace: WorkspaceRequirement {
                capabilities: BTreeSet::new(),
                minimum_mechanism: DeliveryMechanism::Native,
                requirement: ContributionRequirement::Optional,
            },
            contributions: vec![first, second],
            capability_packs: Vec::new(),
        })
        .expect_err("conflict");

    assert!(matches!(
        error,
        SurfaceCompileError::ConflictingContribution { .. }
    ));
}

#[test]
fn distinct_contribution_keys_cannot_alias_one_tool_runtime_identity() {
    let tool = |key: &str, path: &str| {
        CapabilityContribution::Tool(ToolContribution {
            meta: meta(key, ContributionRequirement::Required),
            runtime_name: "workspace_read".to_string(),
            description: "Read".to_string(),
            parameters_schema: serde_json::json!({"type":"object"}),
            capability_key: "file_read".to_string(),
            tool_path: path.to_string(),
            allowed_channels: [ToolChannel::DirectCallback].into(),
            configuration_boundary: ConfigurationBoundary::Binding,
        })
    };

    let error = AgentSurfaceCompiler
        .compile(AgentSurfaceCompileInput {
            revision: SurfaceRevision(1),
            context_recipe: context_recipe(),
            tool_set_revision: ToolSetRevision(1),
            hook_plan_revision: HookPlanRevision(1),
            workspace: WorkspaceRequirement {
                capabilities: BTreeSet::new(),
                minimum_mechanism: DeliveryMechanism::Native,
                requirement: ContributionRequirement::Optional,
            },
            contributions: vec![
                tool("tool:a", "file_read::a"),
                tool("tool:b", "file_read::b"),
            ],
            capability_packs: Vec::new(),
        })
        .expect_err("runtime tool name must be unambiguous");

    assert_eq!(
        error,
        SurfaceCompileError::ConflictingToolRuntimeName {
            runtime_name: "workspace_read".to_string()
        }
    );
}

#[test]
fn hook_plan_binds_one_compatible_runtime_route() {
    let definition_id: HookDefinitionId = id("hook-before-tool");
    let surface = compile(vec![CapabilityContribution::Hook(HookDefinition {
        meta: meta("hook:before-tool", ContributionRequirement::Required),
        definition_id: definition_id.clone(),
        point: HookPoint::BeforeTool,
        actions: [HookAction::Block, HookAction::RequestApproval].into(),
        minimum_strength: SemanticStrength::ExactSynchronous,
        failure_policy: HookFailurePolicy::FailClosed,
        matcher: HookMatcher::ToolNames {
            names: ["shell_exec".to_string()].into(),
        },
        handler: HookHandler::Builtin {
            key: "supervised_tool_gate".to_string(),
        },
    })]);

    let binding = surface
        .hook_plan
        .bind_runtime_plan(
            id("thread-surface"),
            [HookRouteSelection {
                definition_id,
                site: HookExecutionSite::ToolBroker,
                delivered_strength: SemanticStrength::ExactSynchronous,
                actions: [HookAction::Block, HookAction::RequestApproval].into(),
                failure_policies: [HookFailurePolicy::FailClosed].into(),
            }],
        )
        .expect("bind route");

    assert_eq!(binding.plan.revision, HookPlanRevision(2));
    assert_eq!(binding.plan.digest, surface.hook_plan.digest);
    assert_eq!(binding.plan.entries.len(), 1);
    assert_eq!(binding.plan.entries[0].site, HookExecutionSite::ToolBroker);
}

#[test]
fn required_exact_hook_rejects_observed_route() {
    let definition_id: HookDefinitionId = id("hook-before-tool");
    let surface = compile(vec![CapabilityContribution::Hook(HookDefinition {
        meta: meta("hook:before-tool", ContributionRequirement::Required),
        definition_id: definition_id.clone(),
        point: HookPoint::BeforeTool,
        actions: [HookAction::Block].into(),
        minimum_strength: SemanticStrength::ExactSynchronous,
        failure_policy: HookFailurePolicy::FailClosed,
        matcher: HookMatcher::Any,
        handler: HookHandler::Builtin {
            key: "deny".to_string(),
        },
    })]);

    let error = surface
        .hook_plan
        .bind_runtime_plan(
            id("thread-surface"),
            [HookRouteSelection {
                definition_id,
                site: HookExecutionSite::ObservedEventReaction,
                delivered_strength: SemanticStrength::ObservedOnly,
                actions: [HookAction::Observe].into(),
                failure_policies: [HookFailurePolicy::ObserveOnly].into(),
            }],
        )
        .expect_err("observed cannot satisfy exact block");

    assert!(matches!(
        error,
        SurfaceCompileError::IncompatibleHookRoute { .. }
    ));
}

#[test]
fn hook_definition_cannot_be_bound_to_two_execution_sites() {
    let definition_id: HookDefinitionId = id("hook-before-tool");
    let surface = compile(vec![CapabilityContribution::Hook(HookDefinition {
        meta: meta("hook:before-tool", ContributionRequirement::Required),
        definition_id: definition_id.clone(),
        point: HookPoint::BeforeTool,
        actions: [HookAction::Block].into(),
        minimum_strength: SemanticStrength::ExactSynchronous,
        failure_policy: HookFailurePolicy::FailClosed,
        matcher: HookMatcher::Any,
        handler: HookHandler::Builtin {
            key: "deny".to_string(),
        },
    })]);
    let route = HookRouteSelection {
        definition_id,
        site: HookExecutionSite::ToolBroker,
        delivered_strength: SemanticStrength::ExactSynchronous,
        actions: [HookAction::Block].into(),
        failure_policies: [HookFailurePolicy::FailClosed].into(),
    };

    let error = surface
        .hook_plan
        .bind_runtime_plan(id("thread-surface"), [route.clone(), route])
        .expect_err("duplicate execution route");
    assert!(matches!(
        error,
        SurfaceCompileError::ConflictingHookRoute { .. }
    ));
}

#[test]
fn hook_plan_rejects_a_route_for_an_unknown_definition() {
    let surface = compile(Vec::new());
    let unknown: HookDefinitionId = id("hook-not-in-plan");
    let error = surface
        .hook_plan
        .bind_runtime_plan(
            id("thread-surface"),
            [HookRouteSelection {
                definition_id: unknown.clone(),
                site: HookExecutionSite::ToolBroker,
                delivered_strength: SemanticStrength::ExactSynchronous,
                actions: [HookAction::Block].into(),
                failure_policies: [HookFailurePolicy::FailClosed].into(),
            }],
        )
        .expect_err("route selection must not introduce a second hook fact");

    assert_eq!(
        error,
        SurfaceCompileError::UnknownHookRoute {
            definition_id: unknown
        }
    );
}

#[test]
fn required_tool_needs_a_callable_bound_channel() {
    let surface = compile(vec![CapabilityContribution::Tool(ToolContribution {
        meta: meta("tool:read", ContributionRequirement::Required),
        runtime_name: "read".to_string(),
        description: "Read".to_string(),
        parameters_schema: serde_json::json!({"type":"object"}),
        capability_key: "file_read".to_string(),
        tool_path: "file_read::read".to_string(),
        allowed_channels: [ToolChannel::DirectCallback].into(),
        configuration_boundary: ConfigurationBoundary::Binding,
    })]);

    let error = surface
        .bind_profile(
            id("thread-surface"),
            &runtime_profile([ToolChannel::DriverNative].into()),
            [],
        )
        .expect_err("driver-native cannot satisfy a direct callback requirement");

    assert!(matches!(
        error,
        SurfaceCompileError::IncompatibleContribution { key, .. } if key == "tool:read"
    ));
}

#[test]
fn required_hot_replace_tool_rejects_a_binding_only_runtime() {
    let surface = compile(vec![CapabilityContribution::Tool(ToolContribution {
        meta: meta("tool:dynamic", ContributionRequirement::Required),
        runtime_name: "dynamic".to_string(),
        description: "Dynamic tool".to_string(),
        parameters_schema: serde_json::json!({"type":"object"}),
        capability_key: "dynamic".to_string(),
        tool_path: "dynamic::call".to_string(),
        allowed_channels: [ToolChannel::DirectCallback].into(),
        configuration_boundary: ConfigurationBoundary::HotReplace,
    })]);

    let error = surface
        .bind_profile(
            id("thread-surface"),
            &runtime_profile([ToolChannel::DirectCallback].into()),
            [],
        )
        .expect_err("binding-only runtime cannot acknowledge a hot tool revision");
    assert!(matches!(
        error,
        SurfaceCompileError::IncompatibleContribution { key, .. } if key == "tool:dynamic"
    ));
}

#[test]
fn prompt_only_workspace_cannot_satisfy_exact_requirement() {
    let surface = compile(Vec::new());
    let mut profile = runtime_profile(BTreeSet::new());
    profile.workspace.mechanism = DeliveryMechanism::PromptOnly;

    let error = surface
        .bind_profile(id("thread-surface"), &profile, [])
        .expect_err("prompt-only workspace is not an executable capability");

    assert!(matches!(
        error,
        SurfaceCompileError::IncompatibleContribution { key, .. } if key == "workspace"
    ));
}

#[test]
fn required_skill_is_not_admitted_without_native_skill_ingress() {
    let surface = compile(vec![CapabilityContribution::Skill(SkillContribution {
        meta: meta("skill:review", ContributionRequirement::Required),
        resource_ref: "skill://review/SKILL.md".to_string(),
        description: "Review code".to_string(),
    })]);

    let error = surface
        .bind_profile(id("thread-surface"), &runtime_profile(BTreeSet::new()), [])
        .expect_err("prompt text cannot impersonate native Skill ingress");
    assert!(matches!(
        error,
        SurfaceCompileError::IncompatibleContribution { key, .. } if key == "skill:review"
    ));
}
