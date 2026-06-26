use agentdash_spi::{HookApprovalRequest, HookDiagnosticEntry, HookResolution};

use super::super::{HookEvaluationContext, NormalizedHookRule};

pub(super) const REGISTRY_ITEM: NormalizedHookRule = NormalizedHookRule {
    key: "global_builtin:supervised:ask_tool_approval",
    trigger: agentdash_spi::HookTrigger::BeforeTool,
    matches: matches_rule,
    apply: apply_rule,
};

fn matches_rule(ctx: &HookEvaluationContext<'_>) -> bool {
    let Some(tool_name) = ctx.query.tool_name.as_deref() else {
        return false;
    };
    super::super::super::snapshot_helpers::session_permission_policy(ctx.snapshot)
        .is_some_and(|policy| policy.eq_ignore_ascii_case("SUPERVISED"))
        && requires_supervised_tool_approval(tool_name)
}

fn apply_rule(ctx: &HookEvaluationContext<'_>, resolution: &mut HookResolution) {
    let tool_name = ctx.query.tool_name.as_deref().unwrap_or("unknown_tool");
    resolution.approval_request = Some(HookApprovalRequest {
        reason: format!("当前会话使用 SUPERVISED 权限策略，执行 `{tool_name}` 前需要用户审批。"),
        details: Some(serde_json::json!({
            "policy": "supervised_tool_approval",
            "permission_policy": super::super::super::snapshot_helpers::session_permission_policy(ctx.snapshot).unwrap_or("SUPERVISED"),
            "tool_name": tool_name,
        })),
    });
    resolution.diagnostics.push(HookDiagnosticEntry {
        code: "before_tool_requires_approval".to_string(),
        message: format!("Hook 要求在执行 `{tool_name}` 前进入人工审批"),
    });
}

fn requires_supervised_tool_approval(tool_name: &str) -> bool {
    let normalized = tool_name.to_ascii_lowercase();
    normalized.ends_with("shell_exec")
        || normalized.ends_with("shell")
        || normalized.ends_with("write_file")
        || normalized.ends_with("fs_apply_patch")
        || normalized.contains("delete")
        || normalized.contains("remove")
        || normalized.contains("move")
        || normalized.contains("rename")
}
