use agentdash_domain::workflow::WorkflowHookRuleSpec;
use agentdash_spi::{
    HookDiagnosticEntry, HookEvaluationQuery, HookResolution, HookTrigger, SessionHookSnapshot,
};

use super::presets::domain_trigger_to_spi;
use super::script_engine::HookScriptEngine;
use super::snapshot_helpers::*;

#[path = "rules/global_rules/mod.rs"]
mod global_rules;
#[path = "rules/owner_defaults/mod.rs"]
mod owner_defaults;

pub(crate) struct HookEvaluationContext<'a> {
    pub(crate) snapshot: &'a SessionHookSnapshot,
    pub(crate) query: &'a HookEvaluationQuery,
}

pub(super) struct NormalizedHookRule {
    key: &'static str,
    trigger: HookTrigger,
    matches: fn(&HookEvaluationContext<'_>) -> bool,
    apply: fn(&HookEvaluationContext<'_>, &mut HookResolution),
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
        if resolution.block_reason.is_some() && matches!(ctx.query.trigger, HookTrigger::BeforeTool)
        {
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

fn apply_contract_hook_rules(
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
                let rule_label = rule
                    .preset
                    .as_deref()
                    .map(|p| format!("hook_rule:{}:{}", rule.key, p))
                    .unwrap_or_else(|| format!("hook_rule:{}:script", rule.key));
                resolution.matched_rule_keys.push(rule_label);
                merge_script_decision(resolution, decision);

                if resolution.block_reason.is_some()
                    && matches!(ctx.query.trigger, HookTrigger::BeforeTool)
                {
                    return;
                }
            }
            Err(err) => {
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

    use agentdash_spi::{HookInjection, HookOwnerSummary, HookTrigger, SessionHookSnapshot};

    use super::super::presets::builtin_preset_scripts;
    use super::super::test_fixtures::*;

    fn test_script_engine() -> HookScriptEngine {
        let scripts = builtin_preset_scripts();
        HookScriptEngine::new(&scripts)
    }

    #[test]
    fn before_tool_rewrites_shell_exec_absolute_cwd_to_workspace_relative() {
        let snapshot = snapshot_with_workflow("implement", "session_ended");
        let mut resolution = HookResolution::default();
        let query = HookEvaluationQuery {
            session_id: snapshot.session_id.clone(),
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
        let query = HookEvaluationQuery {
            session_id: snapshot.session_id.clone(),
            trigger: HookTrigger::BeforeStop,
            turn_id: None,
            tool_name: None,
            tool_call_id: None,
            subagent_type: None,
            snapshot: None,
            payload: None,
            token_stats: None,
        };

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
        let query = HookEvaluationQuery {
            session_id: snapshot.session_id.clone(),
            trigger: HookTrigger::BeforeStop,
            turn_id: None,
            tool_name: None,
            tool_call_id: None,
            subagent_type: None,
            snapshot: None,
            payload: None,
            token_stats: None,
        };

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
        let query = HookEvaluationQuery {
            session_id: snapshot.session_id.clone(),
            trigger: HookTrigger::BeforeStop,
            turn_id: None,
            tool_name: None,
            tool_call_id: None,
            subagent_type: None,
            snapshot: None,
            payload: None,
            token_stats: None,
        };

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
        let query = HookEvaluationQuery {
            session_id: snapshot.session_id.clone(),
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
    fn before_tool_supervised_policy_requests_approval() {
        let snapshot = snapshot_with_supervised_policy();
        let mut resolution = HookResolution::default();
        let query = HookEvaluationQuery {
            session_id: snapshot.session_id.clone(),
            trigger: HookTrigger::BeforeTool,
            turn_id: Some("turn-approval-1".to_string()),
            tool_name: Some("shell_exec".to_string()),
            tool_call_id: Some("call-shell-approval".to_string()),
            subagent_type: None,
            snapshot: None,
            payload: Some(serde_json::json!({
                "args": {
                    "cwd": ".",
                    "command": "cargo test"
                }
            })),
            token_stats: None,
        };

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
                .approval_request
                .as_ref()
                .map(|request| request.reason.as_str()),
            Some("当前会话使用 SUPERVISED 权限策略，执行 `shell_exec` 前需要用户审批。")
        );
        assert!(
            resolution
                .matched_rule_keys
                .contains(&"global_builtin:supervised:ask_tool_approval".to_string())
        );
    }

    #[test]
    fn before_subagent_dispatch_inherits_runtime_context() {
        use agentdash_domain::workflow::{
            EffectiveSessionContract, WorkflowHookRuleSpec, WorkflowHookTrigger,
        };
        let snapshot = SessionHookSnapshot {
            session_id: "sess-test".to_string(),
            sources: vec!["workflow:trellis_dev_task:check".to_string()],
            owners: vec![HookOwnerSummary {
                owner_type: agentdash_domain::session_binding::SessionOwnerType::Story,
                owner_id: uuid::Uuid::new_v4().to_string(),
                label: Some("Story A".to_string()),
                project_id: None,
                story_id: None,
                task_id: None,
            }],
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
            metadata: Some(agentdash_spi::SessionSnapshotMetadata {
                active_workflow: Some(agentdash_spi::ActiveWorkflowMeta {
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
            ..SessionHookSnapshot::default()
        };
        let mut resolution = HookResolution::default();
        let query = HookEvaluationQuery {
            session_id: snapshot.session_id.clone(),
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
    fn subagent_result_records_structured_return_channel_diagnostic() {
        use agentdash_domain::workflow::{WorkflowHookRuleSpec, WorkflowHookTrigger};
        let mut snapshot = snapshot_with_workflow("check", "checklist_passed");
        if let Some(meta) = snapshot.metadata.as_mut() {
            if let Some(aw) = meta.active_workflow.as_mut() {
                if let Some(ec) = aw.effective_contract.as_mut() {
                    ec.hook_rules.push(WorkflowHookRuleSpec {
                        key: "result_channel".to_string(),
                        trigger: WorkflowHookTrigger::SubagentResult,
                        description: "subagent result channel".to_string(),
                        preset: Some("subagent_result_channel".to_string()),
                        params: None,
                        script: None,
                        enabled: true,
                    });
                }
            }
        }
        let mut resolution = HookResolution::default();
        let query = HookEvaluationQuery {
            session_id: snapshot.session_id.clone(),
            trigger: HookTrigger::SubagentResult,
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
                .any(|k| k.contains("subagent_result_channel")),
            "expected matched_rule_keys to contain subagent_result_channel, got: {:?}\ndiagnostics: {:?}",
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
