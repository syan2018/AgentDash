mod activity_def;
mod capability;
mod contract;
mod hook_rule;
mod injection;
mod lifecycle_def;
mod metadata;
mod mount_directive;
mod orchestration;
mod ports;
mod run_state;
mod script_asset;
mod task_plan;

pub use activity_def::{
    ActivityCompletionPolicy, ActivityDefinition, ActivityExecutorSpec, ActivityIterationPolicy,
    ActivityJoinPolicy, ActivityTransition, ActivityTransitionKind, AgentActivityExecutorSpec,
    AgentReusePolicy, ApiRequestExecutorSpec, ArtifactAliasPolicy, ArtifactBinding,
    BashExecExecutorSpec, FunctionActivityExecutorSpec, HumanActivityExecutorSpec,
    HumanApprovalExecutorSpec, RuntimeSessionPolicy, TransitionCondition,
};
pub use capability::{
    CapabilityConfig, ToolCapabilityDirective, ToolCapabilityPath, ToolCapabilityReduction,
    ToolCapabilitySlotState, mcp_capability_key, mcp_tool_capability_path,
    reduce_tool_capability_directives,
};
pub use contract::{
    AgentProcedureContract, EffectiveSessionContract, WorkflowSessionTerminalState,
};
pub use hook_rule::{WorkflowHookRuleSpec, WorkflowHookTrigger};
pub use injection::{WorkflowContextBinding, WorkflowInjectionSpec};
pub use lifecycle_def::LifecycleNodeType;
pub use metadata::{DefinitionSource, ValidationIssue, ValidationSeverity};
pub use mount_directive::MountDirective;
pub use orchestration::{
    ActivationRule, AgentFrameRef, AgentProcedureExecutionSpec, AgentRunRef, DispatchLeaseSnapshot,
    DispatchOutboxItem, DispatchState, ExecutorSpec, LifecycleContext, NodeCacheRef,
    NodeCacheState, NodePortValue, OrchestrationInstance, OrchestrationJournalFact,
    OrchestrationLimits, OrchestrationPlanSnapshot, OrchestrationSourceRef, OrchestrationStatus,
    PlanActivation, PlanNode, PlanNodeKind, RuntimeNodeError, RuntimeNodeState, RuntimeNodeStatus,
    RuntimeTraceRef, StateArtifactRef, StateExchangeRule, StateExchangeSnapshot,
};
pub use ports::{
    ContextStrategy, GateStrategy, InputPortDefinition, OutputPortDefinition, StandaloneFulfillment,
};
pub use run_state::{
    ExecutorRunRef, LifecycleExecutionEntry, LifecycleExecutionEventKind, LifecycleRunStatus,
};
pub use script_asset::{
    RunScriptArtifact, RunScriptArtifactStatus, WorkflowScriptApiEndpoint,
    WorkflowScriptBashCommand, WorkflowScriptCapabilitySummary, WorkflowScriptDefinition,
    WorkflowScriptDefinitionScope, WorkflowScriptDefinitionStatus,
    WorkflowScriptHumanGateCapability, WorkflowScriptProvenance, WorkflowScriptProvenanceSource,
    workflow_script_source_digest,
};
pub use task_plan::{
    LifecycleTaskPlanItem, LifecycleTaskPlanItemDraft, LifecycleTaskPlanItemPatch, TaskPlanStatus,
    TaskPriority,
};

#[cfg(test)]
use super::validation::{validate_agent_procedure, validate_workflow_graph};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::Mount;

    fn sample_contract() -> AgentProcedureContract {
        AgentProcedureContract {
            injection: WorkflowInjectionSpec {
                guidance: Some("read spec first".to_string()),
                context_bindings: vec![WorkflowContextBinding {
                    locator: ".trellis/workflow.md".to_string(),
                    reason: "workflow".to_string(),
                    required: true,
                    title: None,
                }],
            },
            ..AgentProcedureContract::default()
        }
    }

    #[test]
    fn validate_agent_procedure_rejects_duplicate_output_port_keys() {
        let mut contract = sample_contract();
        contract.output_ports = vec![
            OutputPortDefinition {
                key: "a".to_string(),
                description: "x".to_string(),
                gate_strategy: GateStrategy::Existence,
                gate_params: None,
            },
            OutputPortDefinition {
                key: "a".to_string(),
                description: "y".to_string(),
                gate_strategy: GateStrategy::Existence,
                gate_params: None,
            },
        ];

        let error = validate_agent_procedure("wf", "Workflow", &contract).expect_err("fail");
        assert!(error.contains("重复"));
    }

    fn activity_agent(
        key: &str,
        input_ports: Vec<InputPortDefinition>,
        output_ports: Vec<OutputPortDefinition>,
    ) -> ActivityDefinition {
        ActivityDefinition {
            key: key.to_string(),
            description: String::new(),
            executor: ActivityExecutorSpec::Agent(
                AgentActivityExecutorSpec::create_activity_agent(format!("workflow.{key}")),
            ),
            input_ports,
            output_ports,
            completion_policy: ActivityCompletionPolicy::ExecutorTerminal,
            iteration_policy: ActivityIterationPolicy {
                max_attempts: Some(3),
                artifact_alias: ArtifactAliasPolicy::LatestAndHistory,
            },
            join_policy: ActivityJoinPolicy::All,
        }
    }

    fn activity_human_approval(
        key: &str,
        input_ports: Vec<InputPortDefinition>,
        output_ports: Vec<OutputPortDefinition>,
    ) -> ActivityDefinition {
        ActivityDefinition {
            key: key.to_string(),
            description: String::new(),
            executor: ActivityExecutorSpec::Human(HumanActivityExecutorSpec::Approval(
                HumanApprovalExecutorSpec {
                    form_schema_key: "approval.plan_review".to_string(),
                    title: None,
                },
            )),
            input_ports,
            output_ports,
            completion_policy: ActivityCompletionPolicy::HumanDecision {
                decision_port: "decision".to_string(),
            },
            iteration_policy: ActivityIterationPolicy {
                max_attempts: Some(3),
                artifact_alias: ArtifactAliasPolicy::LatestAndHistory,
            },
            join_policy: ActivityJoinPolicy::All,
        }
    }

    fn input_port(key: &str) -> InputPortDefinition {
        InputPortDefinition {
            key: key.to_string(),
            description: format!("{key} input"),
            context_strategy: ContextStrategy::Full,
            context_template: None,
            standalone_fulfillment: StandaloneFulfillment::Required,
        }
    }

    fn output_port(key: &str) -> OutputPortDefinition {
        OutputPortDefinition {
            key: key.to_string(),
            description: format!("{key} output"),
            gate_strategy: GateStrategy::Existence,
            gate_params: None,
        }
    }

    #[test]
    fn validate_workflow_graph_accepts_human_approval_loop() {
        let activities = vec![
            activity_agent(
                "plan",
                vec![input_port("feedback")],
                vec![output_port("proposal")],
            ),
            activity_human_approval(
                "approval",
                vec![input_port("proposal")],
                vec![output_port("decision")],
            ),
            activity_agent(
                "implement",
                vec![input_port("approved_plan")],
                vec![output_port("summary")],
            ),
        ];
        let transitions = vec![
            ActivityTransition {
                from: "plan".to_string(),
                to: "approval".to_string(),
                kind: ActivityTransitionKind::Flow,
                condition: TransitionCondition::Always,
                artifact_bindings: vec![ArtifactBinding {
                    from_activity: None,
                    from_port: "proposal".to_string(),
                    to_port: "proposal".to_string(),
                    alias: ArtifactAliasPolicy::Latest,
                }],
                max_traversals: None,
            },
            ActivityTransition {
                from: "approval".to_string(),
                to: "implement".to_string(),
                kind: ActivityTransitionKind::Flow,
                condition: TransitionCondition::HumanDecisionEquals {
                    activity: "approval".to_string(),
                    decision_port: "decision".to_string(),
                    value: "approved".to_string(),
                },
                artifact_bindings: vec![ArtifactBinding {
                    from_activity: Some("plan".to_string()),
                    from_port: "proposal".to_string(),
                    to_port: "approved_plan".to_string(),
                    alias: ArtifactAliasPolicy::Latest,
                }],
                max_traversals: None,
            },
            ActivityTransition {
                from: "approval".to_string(),
                to: "plan".to_string(),
                kind: ActivityTransitionKind::Flow,
                condition: TransitionCondition::HumanDecisionEquals {
                    activity: "approval".to_string(),
                    decision_port: "decision".to_string(),
                    value: "rejected".to_string(),
                },
                artifact_bindings: vec![ArtifactBinding {
                    from_activity: None,
                    from_port: "decision".to_string(),
                    to_port: "feedback".to_string(),
                    alias: ArtifactAliasPolicy::Latest,
                }],
                max_traversals: None,
            },
        ];

        validate_workflow_graph("lc", "Lifecycle", "plan", &activities, &transitions)
            .expect("approval loop should be bounded by typed decision and retry policy");
    }

    #[test]
    fn validate_workflow_graph_rejects_missing_artifact_port() {
        let activities = vec![
            activity_agent("plan", vec![], vec![output_port("proposal")]),
            activity_agent("implement", vec![input_port("approved_plan")], vec![]),
        ];
        let transitions = vec![ActivityTransition {
            from: "plan".to_string(),
            to: "implement".to_string(),
            kind: ActivityTransitionKind::Flow,
            condition: TransitionCondition::Always,
            artifact_bindings: vec![ArtifactBinding {
                from_activity: None,
                from_port: "missing".to_string(),
                to_port: "approved_plan".to_string(),
                alias: ArtifactAliasPolicy::Latest,
            }],
            max_traversals: None,
        }];

        let err = validate_workflow_graph("lc", "Lifecycle", "plan", &activities, &transitions)
            .expect_err("missing output port should fail");
        assert!(err.contains("from_port"));
    }

    #[test]
    fn validate_workflow_graph_rejects_unbounded_entry_loop() {
        let mut plan = activity_agent("plan", vec![], vec![output_port("proposal")]);
        plan.iteration_policy.max_attempts = None;
        let activities = vec![
            plan,
            activity_agent("review", vec![input_port("proposal")], vec![]),
        ];
        let transitions = vec![
            ActivityTransition {
                from: "plan".to_string(),
                to: "review".to_string(),
                kind: ActivityTransitionKind::Flow,
                condition: TransitionCondition::Always,
                artifact_bindings: vec![ArtifactBinding {
                    from_activity: None,
                    from_port: "proposal".to_string(),
                    to_port: "proposal".to_string(),
                    alias: ArtifactAliasPolicy::Latest,
                }],
                max_traversals: None,
            },
            ActivityTransition {
                from: "review".to_string(),
                to: "plan".to_string(),
                kind: ActivityTransitionKind::Flow,
                condition: TransitionCondition::Always,
                artifact_bindings: vec![],
                max_traversals: None,
            },
        ];

        let err = validate_workflow_graph("lc", "Lifecycle", "plan", &activities, &transitions)
            .expect_err("unbounded loop should fail");
        assert!(err.contains("循环 transition"));
    }

    #[test]
    fn validate_workflow_graph_rejects_unconditional_self_loop() {
        let activities = vec![activity_agent(
            "plan",
            vec![],
            vec![output_port("proposal")],
        )];
        let transitions = vec![ActivityTransition {
            from: "plan".to_string(),
            to: "plan".to_string(),
            kind: ActivityTransitionKind::Flow,
            condition: TransitionCondition::Always,
            artifact_bindings: vec![],
            max_traversals: Some(3),
        }];

        let err = validate_workflow_graph("lc", "Lifecycle", "plan", &activities, &transitions)
            .expect_err("unconditional self loop should fail");
        assert!(err.contains("无条件自环"));
    }

    #[test]
    fn activity_executor_serializes_human_kind_and_type() {
        let executor = ActivityExecutorSpec::Human(HumanActivityExecutorSpec::Approval(
            HumanApprovalExecutorSpec {
                form_schema_key: "approval.plan_review".to_string(),
                title: None,
            },
        ));

        let value = serde_json::to_value(executor).expect("serialize executor");
        assert_eq!(value["kind"], "human");
        assert_eq!(value["type"], "approval");
        assert_eq!(value["form_schema_key"], "approval.plan_review");
    }

    #[test]
    fn activity_executor_serializes_function_kind_and_type() {
        let executor = ActivityExecutorSpec::Function(FunctionActivityExecutorSpec::BashExec(
            BashExecExecutorSpec {
                command: "pnpm".to_string(),
                args: vec!["test".to_string()],
                working_directory: None,
            },
        ));

        let value = serde_json::to_value(executor).expect("serialize executor");
        assert_eq!(value["kind"], "function");
        assert_eq!(value["type"], "bash_exec");
        assert_eq!(value["command"], "pnpm");
    }

    #[test]
    fn activity_executor_serializes_agent_kind() {
        let executor = ActivityExecutorSpec::Agent(
            AgentActivityExecutorSpec::create_activity_agent("workflow.plan"),
        );

        let value = serde_json::to_value(executor).expect("serialize executor");
        assert_eq!(value["kind"], "agent");
        assert_eq!(value["procedure_key"], "workflow.plan");
        assert_eq!(value["agent_reuse_policy"], "create_activity_agent");
        assert_eq!(value["runtime_session_policy"], "create_new");
    }

    #[test]
    fn tool_capability_path_parse_short() {
        let path = ToolCapabilityPath::parse("file_read").unwrap();
        assert_eq!(path.capability, "file_read");
        assert_eq!(path.tool, None);
        assert!(!path.is_tool_level());
        assert_eq!(path.to_qualified_string(), "file_read");
    }

    #[test]
    fn tool_capability_path_parse_long() {
        let path = ToolCapabilityPath::parse("file_read::fs_grep").unwrap();
        assert_eq!(path.capability, "file_read");
        assert_eq!(path.tool.as_deref(), Some("fs_grep"));
        assert!(path.is_tool_level());
        assert_eq!(path.to_qualified_string(), "file_read::fs_grep");
    }

    #[test]
    fn tool_capability_path_parse_mcp_prefix() {
        let short = ToolCapabilityPath::parse("mcp:code_analyzer").unwrap();
        assert_eq!(short.capability, "mcp:code_analyzer");
        assert_eq!(short.tool, None);

        let long = ToolCapabilityPath::parse("mcp:code_analyzer::scan").unwrap();
        assert_eq!(long.capability, "mcp:code_analyzer");
        assert_eq!(long.tool.as_deref(), Some("scan"));
    }

    #[test]
    fn mcp_capability_helpers_trim_and_build_canonical_paths() {
        assert_eq!(
            mcp_capability_key(" code_analyzer ").unwrap(),
            "mcp:code_analyzer"
        );

        let path = mcp_tool_capability_path(" code_analyzer ", " scan ").unwrap();
        assert_eq!(path.capability, "mcp:code_analyzer");
        assert_eq!(path.tool.as_deref(), Some("scan"));
        assert_eq!(path.to_qualified_string(), "mcp:code_analyzer::scan");
    }

    #[test]
    fn mcp_capability_helpers_reject_empty_and_nested_segments() {
        assert!(mcp_capability_key("").is_err());
        assert!(mcp_capability_key("   ").is_err());
        assert!(mcp_capability_key("code::analyzer").is_err());
        assert!(mcp_tool_capability_path("code_analyzer", "").is_err());
        assert!(mcp_tool_capability_path("code_analyzer", "scan::deep").is_err());
    }

    #[test]
    fn tool_capability_path_parse_rejects_empty() {
        assert!(ToolCapabilityPath::parse("").is_err());
        assert!(ToolCapabilityPath::parse("   ").is_err());
    }

    #[test]
    fn tool_capability_path_parse_rejects_empty_segments() {
        assert!(ToolCapabilityPath::parse("::tool").is_err());
        assert!(ToolCapabilityPath::parse("cap::").is_err());
    }

    #[test]
    fn tool_capability_path_parse_rejects_multi_segment() {
        assert!(ToolCapabilityPath::parse("a::b::c").is_err());
    }

    #[test]
    fn tool_capability_path_serde_uses_qualified_string() {
        let short = ToolCapabilityPath::of_capability("file_read");
        assert_eq!(serde_json::to_string(&short).unwrap(), r#""file_read""#);

        let long = ToolCapabilityPath::of_tool("file_read", "fs_grep");
        assert_eq!(
            serde_json::to_string(&long).unwrap(),
            r#""file_read::fs_grep""#
        );

        let back: ToolCapabilityPath = serde_json::from_str(r#""file_read::fs_grep""#).unwrap();
        assert_eq!(back, long);
    }

    #[test]
    fn tool_capability_directive_add_remove() {
        let add = ToolCapabilityDirective::add_simple("file_read");
        assert!(add.is_add());
        assert!(!add.is_remove());
        assert_eq!(add.key(), "file_read");

        let remove = ToolCapabilityDirective::remove_simple("example_capability");
        assert!(!remove.is_add());
        assert!(remove.is_remove());
        assert_eq!(remove.key(), "example_capability");
    }

    #[test]
    fn tool_capability_directive_serde_roundtrip() {
        let directives = vec![
            ToolCapabilityDirective::add_simple("file_read"),
            ToolCapabilityDirective::remove_simple("example_capability"),
            ToolCapabilityDirective::add_tool("file_read", "fs_read"),
            ToolCapabilityDirective::remove_tool("file_read", "fs_grep"),
            ToolCapabilityDirective::add_simple("mcp:code_analyzer"),
        ];
        let json = serde_json::to_string(&directives).unwrap();
        let deserialized: Vec<ToolCapabilityDirective> = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, directives);
    }

    #[test]
    fn tool_capability_directive_json_shape() {
        let add_tool = ToolCapabilityDirective::add_tool("file_read", "fs_read");
        let json = serde_json::to_string(&add_tool).unwrap();
        assert_eq!(json, r#"{"add":"file_read::fs_read"}"#);

        let remove_cap = ToolCapabilityDirective::remove_simple("shell_execute");
        let json = serde_json::to_string(&remove_cap).unwrap();
        assert_eq!(json, r#"{"remove":"shell_execute"}"#);
    }

    #[test]
    fn reduce_empty_yields_empty_state() {
        let reduction = reduce_tool_capability_directives(&[]);
        assert!(reduction.slots.is_empty());
        assert!(reduction.excluded_tools.is_empty());
    }

    #[test]
    fn reduce_add_capability_sets_full_capability() {
        let directives = vec![ToolCapabilityDirective::add_simple("workflow_management")];
        let reduction = reduce_tool_capability_directives(&directives);
        assert_eq!(
            reduction.slots.get("workflow_management"),
            Some(&ToolCapabilitySlotState::FullCapability)
        );
    }

    #[test]
    fn reduce_add_tool_yields_whitelist() {
        let directives = vec![ToolCapabilityDirective::add_tool("file_read", "fs_read")];
        let reduction = reduce_tool_capability_directives(&directives);
        match reduction.slots.get("file_read") {
            Some(ToolCapabilitySlotState::ToolWhitelist(set)) => {
                assert!(set.contains("fs_read"));
            }
            other => panic!("期望 ToolWhitelist,实际: {other:?}"),
        }
    }

    #[test]
    fn reduce_remove_capability_marks_blocked() {
        let directives = vec![ToolCapabilityDirective::remove_simple("shell_execute")];
        let reduction = reduce_tool_capability_directives(&directives);
        assert_eq!(
            reduction.slots.get("shell_execute"),
            Some(&ToolCapabilitySlotState::Blocked)
        );
    }

    #[test]
    fn reduce_remove_tool_writes_excluded() {
        let directives = vec![ToolCapabilityDirective::remove_tool("file_read", "fs_grep")];
        let reduction = reduce_tool_capability_directives(&directives);
        let excluded = reduction.excluded_tools.get("file_read").unwrap();
        assert!(excluded.contains("fs_grep"));
    }

    #[test]
    fn reduce_add_tool_then_add_cap_upgrades_to_full() {
        let directives = vec![
            ToolCapabilityDirective::add_tool("file_read", "fs_read"),
            ToolCapabilityDirective::add_simple("file_read"),
        ];
        let reduction = reduce_tool_capability_directives(&directives);
        assert_eq!(
            reduction.slots.get("file_read"),
            Some(&ToolCapabilitySlotState::FullCapability)
        );
    }

    #[test]
    fn reduce_add_cap_then_remove_tool_keeps_full_plus_exclusion() {
        // FullCapability 状态下的 Remove(tool) 不降级，excluded_tools 记录屏蔽项
        let directives = vec![
            ToolCapabilityDirective::add_simple("file_read"),
            ToolCapabilityDirective::remove_tool("file_read", "fs_grep"),
        ];
        let reduction = reduce_tool_capability_directives(&directives);
        assert_eq!(
            reduction.slots.get("file_read"),
            Some(&ToolCapabilitySlotState::FullCapability)
        );
        let excluded = reduction.excluded_tools.get("file_read").unwrap();
        assert!(excluded.contains("fs_grep"));
    }

    #[test]
    fn reduce_remove_then_add_re_enables() {
        // 后来者胜
        let directives = vec![
            ToolCapabilityDirective::remove_simple("example_capability"),
            ToolCapabilityDirective::add_simple("example_capability"),
        ];
        let reduction = reduce_tool_capability_directives(&directives);
        assert_eq!(
            reduction.slots.get("example_capability"),
            Some(&ToolCapabilitySlotState::FullCapability)
        );
    }

    #[test]
    fn reduce_add_tool_then_remove_tool_drops_from_whitelist() {
        let directives = vec![
            ToolCapabilityDirective::add_tool("file_read", "fs_read"),
            ToolCapabilityDirective::add_tool("file_read", "fs_glob"),
            ToolCapabilityDirective::remove_tool("file_read", "fs_read"),
        ];
        let reduction = reduce_tool_capability_directives(&directives);
        match reduction.slots.get("file_read") {
            Some(ToolCapabilitySlotState::ToolWhitelist(set)) => {
                assert!(!set.contains("fs_read"));
                assert!(set.contains("fs_glob"));
            }
            other => panic!("期望 ToolWhitelist,实际: {other:?}"),
        }
        let excluded = reduction.excluded_tools.get("file_read").unwrap();
        assert!(excluded.contains("fs_read"));
    }

    #[test]
    fn workflow_contract_capability_config_default_empty() {
        let json = r#"{}"#;
        let contract: AgentProcedureContract = serde_json::from_str(json).unwrap();
        assert!(contract.capability_config.is_empty());

        let back = serde_json::to_string(&contract).unwrap();
        assert!(
            !back.contains("capability_config"),
            "空 capability_config 不应出现在序列化结果中: {back}"
        );
    }

    #[test]
    fn workflow_contract_tool_directives_roundtrip() {
        let contract = AgentProcedureContract {
            capability_config: CapabilityConfig {
                tool_directives: vec![
                    ToolCapabilityDirective::add_simple("workflow_management"),
                    ToolCapabilityDirective::remove_simple("shell_execute"),
                    ToolCapabilityDirective::add_tool("file_read", "fs_read"),
                ],
                ..CapabilityConfig::default()
            },
            ..AgentProcedureContract::default()
        };
        let json = serde_json::to_string(&contract).unwrap();
        assert!(json.contains("capability_config"));
        assert!(json.contains("tool_directives"));
        assert!(!json.contains("capability_directives"));
        let back: AgentProcedureContract = serde_json::from_str(&json).unwrap();
        assert_eq!(back.capability_config, contract.capability_config);
    }

    #[test]
    fn capability_config_mount_directives_roundtrip() {
        let contract = AgentProcedureContract {
            capability_config: CapabilityConfig {
                mount_directives: vec![MountDirective::AddMount {
                    mount: Mount {
                        id: "review".to_string(),
                        provider: "inline_fs".to_string(),
                        backend_id: "backend".to_string(),
                        root_ref: "inline://review".to_string(),
                        capabilities: vec![crate::common::MountCapability::Read],
                        default_write: false,
                        display_name: "Review".to_string(),
                        metadata: serde_json::Value::Null,
                    },
                }],
                ..CapabilityConfig::default()
            },
            ..AgentProcedureContract::default()
        };
        let json = serde_json::to_string(&contract).unwrap();
        assert!(json.contains("capability_config"));
        assert!(json.contains("mount_directives"));

        let back: AgentProcedureContract = serde_json::from_str(&json).unwrap();
        assert_eq!(back.capability_config, contract.capability_config);
    }

    #[test]
    fn workflow_contract_rejects_unknown_capability_fields() {
        let json = r#"{"constraints":[],"completion":{"checks":[]},"capabilities":["workflow_management"]}"#;
        let error = serde_json::from_str::<AgentProcedureContract>(json)
            .expect_err("workflow contract 未声明字段必须被拒绝");
        assert!(error.to_string().contains("unknown field"));
    }
}
