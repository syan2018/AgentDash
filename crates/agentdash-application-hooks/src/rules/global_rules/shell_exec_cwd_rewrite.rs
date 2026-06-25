use agentdash_spi::{HookDiagnosticEntry, HookResolution};

use super::super::{HookEvaluationContext, NormalizedHookRule};

pub(super) const REGISTRY_ITEM: NormalizedHookRule = NormalizedHookRule {
    key: "tool:shell_exec:rewrite_absolute_cwd",
    trigger: agentdash_spi::HookTrigger::BeforeTool,
    matches: matches_rule,
    apply: apply_rule,
};

fn matches_rule(ctx: &HookEvaluationContext<'_>) -> bool {
    let Some(tool_name) = ctx.query.tool_name.as_deref() else {
        return false;
    };
    tool_name.ends_with("shell_exec")
        && super::super::super::shell_exec_rewritten_args(ctx.query.payload.as_ref()).is_some()
}

fn apply_rule(ctx: &HookEvaluationContext<'_>, resolution: &mut HookResolution) {
    let Some(rewritten_args) =
        super::super::super::shell_exec_rewritten_args(ctx.query.payload.as_ref())
    else {
        return;
    };
    let rewritten_cwd = rewritten_args
        .get("cwd")
        .and_then(serde_json::Value::as_str)
        .unwrap_or(".")
        .to_string();

    resolution.rewritten_tool_input = Some(rewritten_args);
    resolution.diagnostics.push(HookDiagnosticEntry {
        code: "before_tool_shell_exec_cwd_rewritten".to_string(),
        message: format!(
            "Hook 已把 shell_exec 的绝对 cwd 改写为相对 workspace root 的路径 (rewritten_cwd={rewritten_cwd})"
        ),
    });
}
