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
                "hook_rule_test",
            ),
            trigger: query.trigger,
            tool_name: query.tool_name,
            tool_call_id: query.tool_call_id,
            subagent_type: query.subagent_type,
            payload: query.payload,
            token_stats: query.token_stats,
        }
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

fn should_short_circuit_after_rule(trigger: &HookTrigger, resolution: &HookResolution) -> bool {
    resolution.block_reason.is_some() && matches!(trigger, HookTrigger::BeforeTool)
}

fn render_contract_rule_label(rule: &WorkflowHookRuleSpec) -> String {
    rule.preset
        .as_deref()
        .map(|preset| format!("hook_rule:{}:{preset}", rule.key))
        .unwrap_or_else(|| format!("hook_rule:{}:script", rule.key))
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

fn merge_script_decision(
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
