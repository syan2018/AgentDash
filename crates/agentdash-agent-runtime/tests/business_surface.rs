use std::{collections::BTreeSet, str::FromStr};

use agentdash_agent_protocol::{
    BackboneEvent, ItemCompletedNotification, ItemStartedNotification, ItemUpdatedNotification,
};
use agentdash_agent_runtime::*;
use agentdash_agent_runtime_contract::*;
use agentdash_agent_runtime_test_support::session_parity::{
    NormalizedPresentationEvent, PresentationDurability as StrictDurability,
    compare_ordered_presentation_events,
};
use serde::Deserialize;

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

#[test]
fn tool_projection_matrix_uses_declared_typed_families() {
    let cases = [
        (ToolProtocolProjection::Command, "shellExec"),
        (ToolProtocolProjection::FileChange, "fileChange"),
        (ToolProtocolProjection::FsRead, "fsRead"),
        (ToolProtocolProjection::FsGrep, "fsGrep"),
        (ToolProtocolProjection::FsGlob, "fsGlob"),
        (
            ToolProtocolProjection::Mcp {
                server_key: "server".into(),
            },
            "mcpToolCall",
        ),
        (
            ToolProtocolProjection::Dynamic {
                namespace: Some("declared".into()),
            },
            "dynamicToolCall",
        ),
    ];
    for (projection, expected_type) in cases {
        let tool = ToolContribution {
            meta: meta("tool:matrix", ContributionRequirement::Required),
            runtime_name: "matrix_tool".into(),
            description: "matrix".into(),
            parameters_schema: serde_json::json!({"type":"object"}),
            capability_key: "matrix".into(),
            tool_path: "matrix::tool".into(),
            allowed_channels: [ToolChannel::DirectCallback].into(),
            configuration_boundary: ConfigurationBoundary::Binding,
            protocol_projection: projection,
            presentation_emitter: ToolPresentationEmitter::ToolBroker,
            parity_fixture_id: "main_tool_matrix_lifecycle".into(),
        };
        let args = serde_json::json!({"command":"pwd","path":".","pattern":"x","changes":[],"duration_ms":10,"node_id":"node"});
        let started = tool
            .project_started("item-matrix", args.clone())
            .expect("started projection");
        let output = if expected_type == "mcpToolCall" {
            serde_json::json!({"content":[]})
        } else {
            serde_json::json!({})
        };
        let completed = tool
            .project_completed("item-matrix", args, &output, false)
            .unwrap_or_else(|error| panic!("{expected_type} completed projection: {error}"));
        assert_eq!(
            serde_json::to_value(started).unwrap()["type"],
            expected_type
        );
        assert_eq!(
            serde_json::to_value(completed).unwrap()["type"],
            expected_type
        );
        let update = tool.project_update(Vec::new());
        assert!(
            matches!(update, RuntimeConversationDelta::ToolProgress { content_items } if content_items.is_empty())
        );
        let failed_output = if expected_type == "mcpToolCall" {
            serde_json::json!({"message":"failed"})
        } else {
            serde_json::json!({})
        };
        let failed = tool
            .project_completed(
                "item-matrix-failed",
                serde_json::json!({"changes":[]}),
                &failed_output,
                true,
            )
            .unwrap_or_else(|error| panic!("{expected_type} failed projection: {error}"));
        let failed_json = serde_json::to_value(failed).unwrap();
        assert_eq!(failed_json["type"], expected_type);
        assert!(failed_json.get("success").is_some() || failed_json.get("status").is_some());
    }
}

#[derive(Debug, Deserialize)]
struct McpParityFixture {
    oracle_commit: String,
    scenarios: Vec<McpParityScenario>,
}

#[derive(Debug, Deserialize)]
struct McpParityScenario {
    id: String,
    runtime_name: String,
    server_key: String,
    fixture_id: String,
    arguments: serde_json::Value,
    progress_message: String,
    completed_output: serde_json::Value,
    failed_output: serde_json::Value,
    protected_events: Vec<serde_json::Value>,
}

#[test]
fn direct_and_relay_mcp_lifecycles_match_main_protected_bodies_strictly() {
    let fixture: McpParityFixture =
        serde_json::from_str(include_str!("../fixtures/main-mcp-tool-lifecycle.json"))
            .expect("valid MCP Main fixture");
    assert_eq!(
        fixture.oracle_commit,
        "957fa9d60ea3d67efa1bb278fe5b376cf0c34598"
    );

    for scenario in fixture.scenarios {
        let tool = ToolContribution {
            meta: meta(
                &format!("tool:mcp:{}", scenario.id),
                ContributionRequirement::Required,
            ),
            runtime_name: scenario.runtime_name,
            description: format!("{} MCP fixture", scenario.id),
            parameters_schema: serde_json::json!({"type":"object"}),
            capability_key: format!("mcp:{}", scenario.server_key),
            tool_path: format!("mcp::{}", scenario.id),
            allowed_channels: [ToolChannel::DirectCallback].into(),
            configuration_boundary: ConfigurationBoundary::Binding,
            protocol_projection: ToolProtocolProjection::Dynamic { namespace: None },
            presentation_emitter: ToolPresentationEmitter::ToolBroker,
            parity_fixture_id: scenario.fixture_id,
        };
        let item_id = scenario.protected_events[0]["payload"]["item"]["id"]
            .as_str()
            .expect("pinned Main started item id");
        let failed_item_id = scenario.protected_events[3]["payload"]["item"]["id"]
            .as_str()
            .expect("pinned Main failed item id");
        let started = tool
            .project_started(&item_id, scenario.arguments.clone())
            .expect("MCP started");
        let completed = tool
            .project_completed(
                &item_id,
                scenario.arguments.clone(),
                &scenario.completed_output,
                false,
            )
            .expect("MCP completed");
        let failed = tool
            .project_completed(
                failed_item_id,
                scenario.arguments.clone(),
                &scenario.failed_output,
                true,
            )
            .expect("MCP failed");
        let updated = tool
            .project_updated(
                &item_id,
                scenario.arguments,
                vec![
                    agentdash_agent_protocol::DynamicToolCallOutputContentItem::InputText {
                        text: scenario.progress_message,
                    },
                ],
            )
            .expect("Native MCP tool update uses Main DynamicToolCall projection");

        let current = vec![
            NormalizedPresentationEvent {
                durability: StrictDurability::Durable,
                event: serde_json::to_value(BackboneEvent::ItemStarted(ItemStartedNotification {
                    item: started.item().clone(),
                    thread_id: "session-fixture".into(),
                    turn_id: "turn-fixture".into(),
                    started_at_ms: 1_720_000_000_000,
                }))
                .unwrap(),
            },
            NormalizedPresentationEvent {
                durability: StrictDurability::Ephemeral,
                event: serde_json::to_value(BackboneEvent::ItemUpdated(ItemUpdatedNotification {
                    item: updated.item().clone(),
                    thread_id: "session-fixture".into(),
                    turn_id: "turn-fixture".into(),
                    updated_at_ms: 1_720_000_000_001,
                }))
                .unwrap(),
            },
            NormalizedPresentationEvent {
                durability: StrictDurability::Durable,
                event: serde_json::to_value(BackboneEvent::ItemCompleted(
                    ItemCompletedNotification {
                        item: completed.item().clone(),
                        thread_id: "session-fixture".into(),
                        turn_id: "turn-fixture".into(),
                        completed_at_ms: 1_720_000_000_002,
                    },
                ))
                .unwrap(),
            },
            NormalizedPresentationEvent {
                durability: StrictDurability::Durable,
                event: serde_json::to_value(BackboneEvent::ItemCompleted(
                    ItemCompletedNotification {
                        item: failed.item().clone(),
                        thread_id: "session-fixture".into(),
                        turn_id: "turn-fixture".into(),
                        completed_at_ms: 1_720_000_000_003,
                    },
                ))
                .unwrap(),
            },
        ];
        let main = scenario
            .protected_events
            .into_iter()
            .enumerate()
            .map(|(index, event)| NormalizedPresentationEvent {
                durability: if index == 1 {
                    StrictDurability::Ephemeral
                } else {
                    StrictDurability::Durable
                },
                event,
            })
            .collect::<Vec<_>>();
        compare_ordered_presentation_events(&main, &current)
            .unwrap_or_else(|error| panic!("{} MCP Main mismatch: {error:?}", scenario.id));
    }
}

#[test]
fn file_change_projection_preserves_owner_patch_and_terminal_changes() {
    let tool = ToolContribution {
        meta: meta("tool:apply-patch", ContributionRequirement::Required),
        runtime_name: "fs_apply_patch".into(),
        description: "apply patch".into(),
        parameters_schema: serde_json::json!({"type":"object"}),
        capability_key: "fs.write".into(),
        tool_path: "vfs::apply_patch".into(),
        allowed_channels: [ToolChannel::DirectCallback].into(),
        configuration_boundary: ConfigurationBoundary::Binding,
        protocol_projection: ToolProtocolProjection::FileChange,
        presentation_emitter: ToolPresentationEmitter::ToolBroker,
        parity_fixture_id: "main_tool_apply_patch_lifecycle".into(),
    };
    let patch =
        "*** Begin Patch\n*** Update File: main://src/lib.rs\n@@\n-old\n+new\n*** End Patch";
    let arguments = serde_json::json!({"patch":patch});
    let started =
        serde_json::to_value(tool.project_started("patch-1", arguments.clone()).unwrap()).unwrap();
    assert_eq!(started["changes"][0]["path"], "main://src/lib.rs");
    assert_eq!(started["changes"][0]["kind"]["type"], "update");
    assert!(
        started["changes"][0]["diff"]
            .as_str()
            .unwrap()
            .contains("-old\n+new")
    );
    let completed = serde_json::to_value(tool.project_completed(
        "patch-1",
        arguments,
        &serde_json::json!({"changes":[{"path":"main://src/lib.rs","kind":{"type":"update","move_path":null},"diff":patch}]}),
        false,
    ).unwrap()).unwrap();
    assert_eq!(completed["status"], "completed");
    assert_eq!(completed["changes"][0]["diff"], patch);
}

#[test]
fn shell_projection_uses_arguments_for_execution_mode_and_terminal_control_identity() {
    let tool = ToolContribution {
        meta: meta("tool:shell-split", ContributionRequirement::Required),
        runtime_name: "shell_exec".into(),
        description: "shell".into(),
        parameters_schema: serde_json::json!({"type":"object"}),
        capability_key: "shell".into(),
        tool_path: "vfs::shell".into(),
        allowed_channels: [ToolChannel::DirectCallback].into(),
        configuration_boundary: ConfigurationBoundary::Binding,
        protocol_projection: ToolProtocolProjection::Command,
        presentation_emitter: ToolPresentationEmitter::ToolBroker,
        parity_fixture_id: "main_tool_shell_split_lifecycle".into(),
    };
    let platform = serde_json::to_value(
        tool.project_started("platform", serde_json::json!({"command":"pwd"}))
            .unwrap(),
    )
    .unwrap();
    let mount = serde_json::to_value(
        tool.project_started(
            "mount",
            serde_json::json!({"command":"pwd","cwd":"main://src"}),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(platform["type"], "shellExec");
    assert_eq!(platform["executionMode"], "platform");
    assert_eq!(mount["executionMode"], "mountExec");
    for operation in ["read", "write", "status", "resize", "terminate"] {
        let arguments = serde_json::json!({
            "operation": operation, "terminal_id":"term-42", "data":"input", "cols":120, "rows":40
        });
        let started =
            serde_json::to_value(tool.project_started(operation, arguments.clone()).unwrap())
                .unwrap();
        assert_eq!(started["type"], "terminalControl");
        assert_eq!(started["operation"], operation);
        assert_eq!(started["terminalId"], "term-42");
        assert_eq!(started["input"], "input");
        let terminal = serde_json::to_value(tool.project_completed(
            operation,
            arguments,
            &serde_json::json!({"terminal_id":"term-42","state":"completed","aggregated_output":"chunk","exit_code":0}),
            false,
        ).unwrap()).unwrap();
        assert_eq!(terminal["type"], "terminalControl");
        assert_eq!(terminal["terminalId"], "term-42");
        assert_eq!(terminal["aggregatedOutput"], "chunk");
        assert_eq!(terminal["success"], true);
    }
}

#[test]
fn command_projection_reads_owner_terminal_contract() {
    let tool = ToolContribution {
        meta: meta("tool:shell", ContributionRequirement::Required),
        runtime_name: "shell_exec".into(),
        description: "shell".into(),
        parameters_schema: serde_json::json!({"type":"object"}),
        capability_key: "shell".into(),
        tool_path: "vfs::shell".into(),
        allowed_channels: [ToolChannel::DirectCallback].into(),
        configuration_boundary: ConfigurationBoundary::Binding,
        protocol_projection: ToolProtocolProjection::Command,
        presentation_emitter: ToolPresentationEmitter::ToolBroker,
        parity_fixture_id: "main_tool_shell_lifecycle".into(),
    };
    let completed = serde_json::to_value(tool.project_completed(
        "shell-1",
        serde_json::json!({"command":"printf ok","cwd":"main://"}),
        &serde_json::json!({"original_command":"printf ok","cwd":"main://","aggregated_output":"ok","exit_code":0,"state":"completed"}),
        false,
    ).unwrap()).unwrap();
    assert_eq!(completed["command"], "printf ok");
    assert_eq!(completed["cwd"], "main://");
    assert_eq!(completed["aggregatedOutput"], "ok");
    assert_eq!(completed["exitCode"], 0);
    assert_eq!(completed["status"], "completed");
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
        protocol_projection: ToolProtocolProjection::Dynamic {
            namespace: Some("test".to_string()),
        },
        presentation_emitter: ToolPresentationEmitter::ToolBroker,
        parity_fixture_id: "main_tool_workspace_read_lifecycle".into(),
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
            protocol_projection: ToolProtocolProjection::Dynamic {
                namespace: Some("test".to_string()),
            },
            presentation_emitter: ToolPresentationEmitter::ToolBroker,
            parity_fixture_id: format!("main_tool_{key}_lifecycle"),
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
fn compiler_rejects_missing_or_shared_main_parity_fixture() {
    let tool = |key: &str, runtime_name: &str, fixture_id: &str| {
        CapabilityContribution::Tool(ToolContribution {
            meta: meta(key, ContributionRequirement::Required),
            runtime_name: runtime_name.to_string(),
            description: "fixture admission".to_string(),
            parameters_schema: serde_json::json!({"type":"object"}),
            capability_key: runtime_name.to_string(),
            tool_path: format!("fixture::{runtime_name}"),
            allowed_channels: [ToolChannel::DirectCallback].into(),
            configuration_boundary: ConfigurationBoundary::Binding,
            protocol_projection: ToolProtocolProjection::Dynamic { namespace: None },
            presentation_emitter: ToolPresentationEmitter::ToolBroker,
            parity_fixture_id: fixture_id.to_string(),
        })
    };

    let missing = AgentSurfaceCompiler
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
            contributions: vec![tool("tool:missing", "missing", "")],
            capability_packs: Vec::new(),
        })
        .expect_err("blank fixture must fail admission");
    assert!(matches!(
        missing,
        SurfaceCompileError::InvalidToolProjector { .. }
    ));

    let shared = AgentSurfaceCompiler
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
                tool("tool:first", "first", "main_tool_shared_lifecycle"),
                tool("tool:second", "second", "main_tool_shared_lifecycle"),
            ],
            capability_packs: Vec::new(),
        })
        .expect_err("one fixture cannot self-prove multiple contributions");
    assert!(matches!(
        shared,
        SurfaceCompileError::ConflictingToolParityFixture { .. }
    ));
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
        protocol_projection: ToolProtocolProjection::Dynamic {
            namespace: Some("test".to_string()),
        },
        presentation_emitter: ToolPresentationEmitter::ToolBroker,
        parity_fixture_id: "main_tool_optional_lifecycle".into(),
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
        protocol_projection: ToolProtocolProjection::Dynamic {
            namespace: Some("test".to_string()),
        },
        presentation_emitter: ToolPresentationEmitter::ToolBroker,
        parity_fixture_id: "main_tool_required_lifecycle".into(),
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
