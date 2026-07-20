use agentdash_application_ports::agent_frame_hook_plan::AgentFrameHookRequirement;
use agentdash_diagnostics::{Subsystem, diag};
use agentdash_domain::workflow::WorkflowHookRuleSpec;
use agentdash_platform_spi::{
    AgentFrameHookEvaluationQuery, AgentFrameHookSnapshot, HookControlTarget, HookDiagnosticEntry,
    HookResolution, HookTrigger, RuntimeAdapterProvenance,
};

use super::presets::domain_trigger_to_spi;
use super::script_engine::HookScriptEngine;
use super::snapshot_helpers::*;

#[path = "rules/global_rules/mod.rs"]
mod global_rules;
#[path = "rules/owner_defaults/mod.rs"]
mod owner_defaults;

pub(crate) struct HookEvaluationContext<'a> {
    pub(crate) snapshot: &'a AgentFrameHookSnapshot,
    pub(crate) query: &'a HookRuleEvaluationQuery,
}

pub(crate) struct HookRuleEvaluationQuery {
    pub(crate) target: Option<HookControlTarget>,
    pub(crate) provenance: RuntimeAdapterProvenance,
    pub(crate) trigger: HookTrigger,
    pub(crate) tool_name: Option<String>,
    pub(crate) tool_call_id: Option<String>,
    pub(crate) subagent_type: Option<String>,
    pub(crate) payload: Option<serde_json::Value>,
    pub(crate) token_stats: Option<agentdash_platform_spi::ContextTokenStats>,
}

impl HookRuleEvaluationQuery {
    pub(crate) fn from_frame_query(query: AgentFrameHookEvaluationQuery) -> Self {
        Self {
            target: Some(query.target),
            provenance: query.provenance,
            trigger: query.trigger,
            tool_name: query.tool_name,
            tool_call_id: query.tool_call_id,
            subagent_type: query.subagent_type,
            payload: query.payload,
            token_stats: query.token_stats,
        }
    }

    #[cfg(test)]
    pub(crate) fn from_test_query(query: HookRuleTestInput) -> Self {
        Self {
            target: None,
            provenance: RuntimeAdapterProvenance::runtime_thread(
                query.session_id,
                query.turn_id,
                "session_hook_evaluation_adapter",
            ),
            trigger: query.trigger,
            tool_name: query.tool_name,
            tool_call_id: query.tool_call_id,
            subagent_type: query.subagent_type,
            payload: query.payload,
            token_stats: query.token_stats,
        }
    }

    pub(crate) fn runtime_thread_id(&self) -> Option<&str> {
        self.provenance.runtime_thread_id.as_deref()
    }

    pub(crate) fn turn_id(&self) -> Option<&str> {
        self.provenance.turn_id.as_deref()
    }
}

#[cfg(test)]
pub(crate) struct HookRuleTestInput {
    pub(crate) session_id: String,
    pub(crate) trigger: HookTrigger,
    pub(crate) turn_id: Option<String>,
    pub(crate) tool_name: Option<String>,
    pub(crate) tool_call_id: Option<String>,
    pub(crate) subagent_type: Option<String>,
    pub(crate) payload: Option<serde_json::Value>,
    pub(crate) token_stats: Option<agentdash_platform_spi::ContextTokenStats>,
}

pub(super) struct NormalizedHookRule {
    key: &'static str,
    trigger: HookTrigger,
    matches: fn(&HookEvaluationContext<'_>) -> bool,
    apply: fn(&HookEvaluationContext<'_>, &mut HookResolution),
}

fn should_short_circuit_after_rule(trigger: &HookTrigger, resolution: &HookResolution) -> bool {
    resolution.block_reason.is_some() && matches!(trigger, HookTrigger::BeforeTool)
}

fn render_contract_rule_label(rule: &WorkflowHookRuleSpec) -> String {
    rule.preset
        .as_deref()
        .map(|preset| format!("hook_rule:{}:{preset}", rule.key))
        .unwrap_or_else(|| format!("hook_rule:{}:script", rule.key))
}

pub(crate) fn has_applicable_hook_work(
    snapshot: &AgentFrameHookSnapshot,
    trigger: HookTrigger,
) -> bool {
    global_rules::registry_items()
        .iter()
        .any(|rule| rule.trigger == trigger)
        || active_workflow_hook_rules(snapshot)
            .iter()
            .any(|rule| rule_can_run_for_trigger(rule, trigger))
        || owner_defaults::owner_default_hook_rules(snapshot)
            .iter()
            .any(|rule| rule_can_run_for_trigger(rule, trigger))
}

/// Product-owned immutable hook rules for the current AgentFrame.
///
/// Runtime admission consumes the semantic requirements compiled from these
/// rules; it never reconstructs Product policy from Runtime events.
pub(crate) fn product_hook_rules(snapshot: &AgentFrameHookSnapshot) -> Vec<WorkflowHookRuleSpec> {
    active_workflow_hook_rules(snapshot)
        .iter()
        .chain(owner_defaults::owner_default_hook_rules(snapshot).iter())
        .filter(|rule| rule.enabled)
        .cloned()
        .collect()
}

fn rule_can_run_for_trigger(rule: &WorkflowHookRuleSpec, trigger: HookTrigger) -> bool {
    rule.enabled
        && domain_trigger_to_spi(rule.trigger) == trigger
        && (rule.preset.is_some() || rule.script.is_some())
}

pub(crate) fn apply_hook_rules(
    ctx: HookEvaluationContext<'_>,
    resolution: &mut HookResolution,
    script_engine: &HookScriptEngine,
) {
    for rule in global_rules::registry_items() {
        if rule.trigger != ctx.query.trigger {
            continue;
        }
        if !(rule.matches)(&ctx) {
            continue;
        }
        resolution.matched_rule_keys.push(rule.key.to_string());
        (rule.apply)(&ctx, resolution);
        if should_short_circuit_after_rule(&ctx.query.trigger, resolution) {
            return;
        }
    }

    let contract_rules = active_workflow_hook_rules(ctx.snapshot);
    if !contract_rules.is_empty() {
        apply_contract_hook_rules(&ctx, contract_rules, resolution, script_engine);
    }

    let owner_rules = owner_defaults::owner_default_hook_rules(ctx.snapshot);
    if !owner_rules.is_empty() {
        apply_contract_hook_rules(&ctx, &owner_rules, resolution, script_engine);
    }
}

pub(crate) fn apply_product_hook_rule(
    ctx: HookEvaluationContext<'_>,
    definition_id: &str,
    resolution: &mut HookResolution,
    script_engine: &HookScriptEngine,
) -> Result<(), String> {
    let key = definition_id
        .strip_prefix("workflow-hook:")
        .ok_or_else(|| format!("unsupported Product hook definition `{definition_id}`"))?;
    let rule = product_hook_rules(ctx.snapshot)
        .into_iter()
        .find(|rule| rule.key == key)
        .ok_or_else(|| format!("Product hook definition `{definition_id}` is not in the frame"))?;
    if domain_trigger_to_spi(rule.trigger) != ctx.query.trigger {
        return Err(format!(
            "Product hook definition `{definition_id}` does not match callback trigger `{}`",
            ctx.query.trigger.as_key()
        ));
    }
    apply_contract_hook_rules(&ctx, std::slice::from_ref(&rule), resolution, script_engine);
    Ok(())
}

pub(crate) fn apply_product_hook_event_requirements(
    ctx: HookEvaluationContext<'_>,
    requirements: &[AgentFrameHookRequirement],
    resolution: &mut HookResolution,
    script_engine: &HookScriptEngine,
) -> Result<(), String> {
    let rules = product_hook_rules(ctx.snapshot);
    for requirement in requirements {
        let definition_id = requirement.definition_id.as_str();
        let key = definition_id
            .strip_prefix("workflow-hook:")
            .ok_or_else(|| format!("unsupported Product hook definition `{definition_id}`"))?;
        let rule = rules.iter().find(|rule| rule.key == key).ok_or_else(|| {
            format!("Product hook definition `{definition_id}` is not in the frame")
        })?;
        if domain_trigger_to_spi(rule.trigger) != ctx.query.trigger {
            continue;
        }
        apply_contract_hook_rules(&ctx, std::slice::from_ref(rule), resolution, script_engine);
    }
    Ok(())
}

pub(crate) fn apply_contract_hook_rules(
    ctx: &HookEvaluationContext<'_>,
    rules: &[WorkflowHookRuleSpec],
    resolution: &mut HookResolution,
    script_engine: &HookScriptEngine,
) {
    for rule in rules {
        if !rule.enabled {
            continue;
        }
        if domain_trigger_to_spi(rule.trigger) != ctx.query.trigger {
            continue;
        }

        let script_result = if let Some(preset_key) = rule.preset.as_deref() {
            script_engine.eval_preset(preset_key, ctx, rule.params.as_ref())
        } else if let Some(script) = rule.script.as_deref() {
            script_engine.eval_script(script, ctx, rule.params.as_ref())
        } else {
            continue;
        };

        match script_result {
            Ok(decision) if !decision.is_empty() => {
                resolution
                    .matched_rule_keys
                    .push(render_contract_rule_label(rule));
                merge_script_decision(resolution, decision);

                if should_short_circuit_after_rule(&ctx.query.trigger, resolution) {
                    return;
                }
            }
            Err(err) => {
                diag!(
                    Warn,
                    Subsystem::Hooks,
                    rule_key = %rule.key,
                    trigger = ?ctx.query.trigger,
                    error = %err,
                    "hook: 规则脚本执行失败"
                );
                resolution.diagnostics.push(HookDiagnosticEntry {
                    code: "hook_script_error".to_string(),
                    message: format!("Hook 规则 `{}` 脚本执行失败: {}", rule.key, err),
                });
            }
            _ => {}
        }
    }
}

pub(crate) fn merge_script_decision(
    resolution: &mut HookResolution,
    decision: super::script_engine::ScriptDecision,
) {
    if let Some(block) = decision.block {
        resolution.block_reason = Some(block);
    }
    resolution.injections.extend(decision.inject);
    if decision.approval.is_some() {
        resolution.approval_request = decision.approval;
    }
    if decision.completion.is_some() {
        resolution.completion = decision.completion;
    }
    if decision.refresh {
        resolution.refresh_snapshot = true;
    }
    if decision.rewrite_input.is_some() {
        resolution.rewritten_tool_input = decision.rewrite_input;
    }
    resolution.diagnostics.extend(decision.diagnostics);
    resolution.effects.extend(decision.effects);
    if let Some(compaction) = decision.compaction {
        resolution.compaction = Some(compaction);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::sync::Arc;

    use agentdash_domain::workflow::{EffectiveSessionContract, WorkflowHookRuleSpec};
    use agentdash_platform_spi::{
        ActiveWorkflowMeta, AgentFrameHookSnapshot, HookInjection, HookTrigger,
    };

    use super::super::presets::builtin_preset_scripts;
    use super::super::test_fixtures::*;
    use super::super::test_script_evaluator::TestHookScriptEvaluator;

    fn test_script_engine() -> HookScriptEngine {
        let scripts = builtin_preset_scripts();
        HookScriptEngine::new(Arc::new(TestHookScriptEvaluator::new(&scripts)))
    }

    #[test]
    fn empty_snapshot_has_no_user_prompt_hook_work() {
        let snapshot = AgentFrameHookSnapshot::default();

        assert!(!has_applicable_hook_work(
            &snapshot,
            HookTrigger::UserPromptSubmit
        ));
    }

    #[test]
    fn enabled_contract_rule_makes_trigger_applicable() {
        let snapshot = AgentFrameHookSnapshot {
            metadata: Some(agentdash_platform_spi::SessionSnapshotMetadata {
                active_workflow: Some(ActiveWorkflowMeta {
                    effective_contract: Some(EffectiveSessionContract {
                        hook_rules: vec![WorkflowHookRuleSpec {
                            key: "silent_observer".to_string(),
                            trigger: agentdash_domain::workflow::WorkflowHookTrigger::BeforeProviderRequest,
                            description: String::new(),
                            preset: None,
                            params: None,
                            script: Some("#{ }".to_string()),
                            enabled: true,
                        }],
                        ..EffectiveSessionContract::default()
                    }),
                    ..ActiveWorkflowMeta::default()
                }),
                ..agentdash_platform_spi::SessionSnapshotMetadata::default()
            }),
            ..AgentFrameHookSnapshot::default()
        };

        assert!(has_applicable_hook_work(
            &snapshot,
            HookTrigger::BeforeProviderRequest
        ));
        assert!(!has_applicable_hook_work(&snapshot, HookTrigger::AfterTool));
    }

    #[test]
    fn complete_agent_definition_evaluates_only_its_exact_product_rule() {
        let snapshot = AgentFrameHookSnapshot {
            metadata: Some(agentdash_platform_spi::SessionSnapshotMetadata {
                active_workflow: Some(ActiveWorkflowMeta {
                    effective_contract: Some(EffectiveSessionContract {
                        hook_rules: vec![
                            WorkflowHookRuleSpec {
                                key: "first".to_owned(),
                                trigger:
                                    agentdash_domain::workflow::WorkflowHookTrigger::BeforeTool,
                                description: String::new(),
                                preset: None,
                                params: None,
                                script: Some("block(\"forbidden\")".to_owned()),
                                enabled: true,
                            },
                            WorkflowHookRuleSpec {
                                key: "second".to_owned(),
                                trigger:
                                    agentdash_domain::workflow::WorkflowHookTrigger::BeforeTool,
                                description: String::new(),
                                preset: None,
                                params: None,
                                script: Some("#{ block: \"blocked\" }".to_owned()),
                                enabled: true,
                            },
                        ],
                        ..EffectiveSessionContract::default()
                    }),
                    ..ActiveWorkflowMeta::default()
                }),
                ..agentdash_platform_spi::SessionSnapshotMetadata::default()
            }),
            ..AgentFrameHookSnapshot::default()
        };
        let query = HookRuleEvaluationQuery {
            target: None,
            provenance: agentdash_platform_spi::RuntimeAdapterProvenance::runtime_thread(
                "thread-a",
                None,
                "complete-agent-test",
            ),
            trigger: HookTrigger::BeforeTool,
            tool_name: Some("shell_exec".to_owned()),
            tool_call_id: Some("call-a".to_owned()),
            subagent_type: None,
            payload: None,
            token_stats: None,
        };
        let mut resolution = HookResolution::default();

        apply_product_hook_rule(
            HookEvaluationContext {
                snapshot: &snapshot,
                query: &query,
            },
            "workflow-hook:second",
            &mut resolution,
            &test_script_engine(),
        )
        .unwrap();

        assert_eq!(resolution.block_reason.as_deref(), Some("blocked"));
        assert_eq!(
            resolution.matched_rule_keys,
            vec!["hook_rule:second:script"]
        );
    }

    #[test]
    fn before_tool_rewrites_shell_exec_absolute_cwd_to_workspace_relative() {
        let snapshot = snapshot_with_workflow("implement", "session_ended");
        let mut resolution = HookResolution::default();
        let query = HookRuleTestInput {
            session_id: snapshot.runtime_adapter_runtime_thread_id.clone(),
            trigger: HookTrigger::BeforeTool,
            turn_id: None,
            tool_name: Some("shell_exec".to_string()),
            tool_call_id: Some("call-shell-1".to_string()),
            subagent_type: None,
            snapshot: None,
            payload: Some(serde_json::json!({
                "default_mount_root_ref": "/tmp/test-workspace",
                "args": {
                    "cwd": "/tmp/test-workspace/crates/agentdash-agent",
                    "command": "cargo test"
                }
            })),
            token_stats: None,
        };
        let query = HookRuleEvaluationQuery::from_test_query(query);

        let engine = test_script_engine();
        apply_hook_rules(
            HookEvaluationContext {
                snapshot: &snapshot,
                query: &query,
            },
            &mut resolution,
            &engine,
        );

        assert_eq!(
            resolution
                .rewritten_tool_input
                .as_ref()
                .and_then(|value| value.get("cwd"))
                .and_then(serde_json::Value::as_str),
            Some("crates/agentdash-agent")
        );
        assert!(
            resolution
                .matched_rule_keys
                .contains(&"tool:shell_exec:rewrite_absolute_cwd".to_string())
        );
        assert!(
            resolution
                .diagnostics
                .iter()
                .any(|entry| entry.code == "before_tool_shell_exec_cwd_rewritten")
        );
    }

    #[test]
    fn before_stop_port_output_gate_blocks_when_ports_unfulfilled() {
        let snapshot =
            snapshot_with_workflow_ports("check", "checklist_passed", &["report", "summary"], &[]);
        let mut resolution = HookResolution::default();
        let query = HookRuleTestInput {
            session_id: snapshot.runtime_adapter_runtime_thread_id.clone(),
            trigger: HookTrigger::BeforeStop,
            turn_id: None,
            tool_name: None,
            tool_call_id: None,
            subagent_type: None,
            snapshot: None,
            payload: None,
            token_stats: None,
        };
        let query = HookRuleEvaluationQuery::from_test_query(query);

        let engine = test_script_engine();
        apply_hook_rules(
            HookEvaluationContext {
                snapshot: &snapshot,
                query: &query,
            },
            &mut resolution,
            &engine,
        );

        assert!(!resolution.injections.is_empty());
        assert!(
            resolution
                .matched_rule_keys
                .iter()
                .any(|k| k.contains("port_output_gate")),
            "expected matched_rule_keys to contain port_output_gate, got: {:?}",
            resolution.matched_rule_keys
        );
    }

    #[test]
    fn before_stop_port_output_gate_blocks_when_partially_fulfilled() {
        let snapshot = snapshot_with_workflow_ports(
            "check",
            "checklist_passed",
            &["report", "summary"],
            &["report"],
        );
        let mut resolution = HookResolution::default();
        let query = HookRuleTestInput {
            session_id: snapshot.runtime_adapter_runtime_thread_id.clone(),
            trigger: HookTrigger::BeforeStop,
            turn_id: None,
            tool_name: None,
            tool_call_id: None,
            subagent_type: None,
            snapshot: None,
            payload: None,
            token_stats: None,
        };
        let query = HookRuleEvaluationQuery::from_test_query(query);

        let engine = test_script_engine();
        apply_hook_rules(
            HookEvaluationContext {
                snapshot: &snapshot,
                query: &query,
            },
            &mut resolution,
            &engine,
        );

        assert!(!resolution.injections.is_empty());
        assert!(
            resolution
                .matched_rule_keys
                .iter()
                .any(|k| k.contains("port_output_gate")),
            "expected port_output_gate to fire for partially fulfilled ports, got: {:?}",
            resolution.matched_rule_keys
        );
    }

    #[test]
    fn before_stop_port_output_gate_allows_when_all_fulfilled() {
        let snapshot = snapshot_with_workflow_ports(
            "check",
            "checklist_passed",
            &["report", "summary"],
            &["report", "summary"],
        );
        let mut resolution = HookResolution::default();
        let query = HookRuleTestInput {
            session_id: snapshot.runtime_adapter_runtime_thread_id.clone(),
            trigger: HookTrigger::BeforeStop,
            turn_id: None,
            tool_name: None,
            tool_call_id: None,
            subagent_type: None,
            snapshot: None,
            payload: None,
            token_stats: None,
        };
        let query = HookRuleEvaluationQuery::from_test_query(query);

        let engine = test_script_engine();
        apply_hook_rules(
            HookEvaluationContext {
                snapshot: &snapshot,
                query: &query,
            },
            &mut resolution,
            &engine,
        );

        assert!(resolution.injections.is_empty());
    }

    #[test]
    fn after_turn_does_not_inject_perpetual_check_phase_steering() {
        let snapshot = snapshot_with_workflow("check", "checklist_passed");
        let mut resolution = HookResolution::default();
        let query = HookRuleTestInput {
            session_id: snapshot.runtime_adapter_runtime_thread_id.clone(),
            trigger: HookTrigger::AfterTurn,
            turn_id: None,
            tool_name: None,
            tool_call_id: None,
            subagent_type: None,
            snapshot: None,
            payload: Some(serde_json::json!({
                "assistant_message": {
                    "role": "assistant",
                    "content": [{ "type": "text", "text": "检查完成，准备结束。" }]
                },
                "tool_results": []
            })),
            token_stats: None,
        };
        let query = HookRuleEvaluationQuery::from_test_query(query);

        let engine = test_script_engine();
        apply_hook_rules(
            HookEvaluationContext {
                snapshot: &snapshot,
                query: &query,
            },
            &mut resolution,
            &engine,
        );

        assert!(resolution.injections.is_empty());
        assert!(resolution.matched_rule_keys.is_empty());
    }

    #[test]
    fn before_subagent_dispatch_inherits_runtime_context() {
        use agentdash_domain::workflow::{
            EffectiveSessionContract, WorkflowHookRuleSpec, WorkflowHookTrigger,
        };
        let snapshot = AgentFrameHookSnapshot {
            runtime_adapter_runtime_thread_id: "sess-test".to_string(),
            sources: vec!["workflow:trellis_dev_task:check".to_string()],
            run_context: Some(agentdash_platform_spi::hooks::SubjectRunContext {
                scope: agentdash_platform_spi::CapabilityScope::Story,
                project_id: uuid::Uuid::new_v4(),
                story_id: Some(uuid::Uuid::new_v4()),
                task_id: None,
                story_title: None,
                task_title: None,
            }),
            injections: vec![
                HookInjection {
                    slot: "workflow".to_string(),
                    content: "step info".to_string(),
                    source: "workflow:trellis_dev_task:check".to_string(),
                },
                HookInjection {
                    slot: "constraint".to_string(),
                    content: "先验证再结束".to_string(),
                    source: "workflow:trellis_dev_task:check".to_string(),
                },
            ],
            metadata: Some(agentdash_platform_spi::SessionSnapshotMetadata {
                active_workflow: Some(agentdash_platform_spi::ActiveWorkflowMeta {
                    effective_contract: Some(EffectiveSessionContract {
                        hook_rules: vec![WorkflowHookRuleSpec {
                            key: "inherit_ctx".to_string(),
                            trigger: WorkflowHookTrigger::BeforeSubagentDispatch,
                            description: "inherit context".to_string(),
                            preset: Some("subagent_inherit_context".to_string()),
                            params: None,
                            script: None,
                            enabled: true,
                        }],
                        ..Default::default()
                    }),
                    ..Default::default()
                }),
                ..Default::default()
            }),
            ..AgentFrameHookSnapshot::default()
        };
        let mut resolution = HookResolution::default();
        let query = HookRuleTestInput {
            session_id: snapshot.runtime_adapter_runtime_thread_id.clone(),
            trigger: HookTrigger::BeforeSubagentDispatch,
            turn_id: None,
            tool_name: None,
            tool_call_id: None,
            subagent_type: Some("companion".to_string()),
            snapshot: None,
            payload: Some(serde_json::json!({
                "prompt": "请帮我 review"
            })),
            token_stats: None,
        };
        let query = HookRuleEvaluationQuery::from_test_query(query);

        let engine = test_script_engine();
        apply_hook_rules(
            HookEvaluationContext {
                snapshot: &snapshot,
                query: &query,
            },
            &mut resolution,
            &engine,
        );

        assert_eq!(resolution.injections.len(), 2);
        assert!(
            resolution
                .matched_rule_keys
                .iter()
                .any(|k| k.contains("subagent_inherit_context")),
            "expected matched_rule_keys to contain subagent_inherit_context, got: {:?}",
            resolution.matched_rule_keys
        );
    }

    #[test]
    fn companion_result_records_structured_return_channel_diagnostic() {
        use agentdash_domain::workflow::{WorkflowHookRuleSpec, WorkflowHookTrigger};
        let mut snapshot = snapshot_with_workflow("check", "checklist_passed");
        if let Some(meta) = snapshot.metadata.as_mut()
            && let Some(aw) = meta.active_workflow.as_mut()
            && let Some(ec) = aw.effective_contract.as_mut()
        {
            ec.hook_rules.push(WorkflowHookRuleSpec {
                key: "result_channel".to_string(),
                trigger: WorkflowHookTrigger::CompanionResult,
                description: "companion result channel".to_string(),
                preset: Some("companion_result_channel".to_string()),
                params: None,
                script: None,
                enabled: true,
            });
        }
        let mut resolution = HookResolution::default();
        let query = HookRuleTestInput {
            session_id: snapshot.runtime_adapter_runtime_thread_id.clone(),
            trigger: HookTrigger::CompanionResult,
            turn_id: Some("turn-parent-1".to_string()),
            tool_name: None,
            tool_call_id: None,
            subagent_type: Some("companion".to_string()),
            snapshot: None,
            payload: Some(serde_json::json!({
                "dispatch_id": "dispatch-1",
                "companion_session_id": "sess-companion-1",
                "adoption_mode": "blocking_review",
                "status": "completed",
                "summary": "子 agent 已完成 review，并附带后续建议"
            })),
            token_stats: None,
        };
        let query = HookRuleEvaluationQuery::from_test_query(query);

        let engine = test_script_engine();
        apply_hook_rules(
            HookEvaluationContext {
                snapshot: &snapshot,
                query: &query,
            },
            &mut resolution,
            &engine,
        );

        assert!(
            resolution
                .matched_rule_keys
                .iter()
                .any(|k| k.contains("companion_result_channel")),
            "expected matched_rule_keys to contain companion_result_channel, got: {:?}\ndiagnostics: {:?}",
            resolution.matched_rule_keys,
            resolution.diagnostics,
        );
        assert!(resolution.injections.len() >= 2);
        assert!(
            resolution
                .injections
                .iter()
                .any(|inj| inj.slot == "workflow")
        );
        assert!(
            resolution
                .injections
                .iter()
                .any(|inj| inj.slot == "constraint")
        );
    }
}
