use crate::workflow::{evaluate_step_completion, WorkflowCompletionSignalSet};
use agentdash_domain::workflow::{WorkflowCheckKind, WorkflowConstraintKind};
use agentdash_spi::{
    HookApprovalRequest, HookCompletionStatus, HookConstraint, HookContextFragment,
    HookDiagnosticEntry, HookEvaluationQuery, HookResolution, HookTrigger, SessionHookSnapshot,
};

use super::snapshot_helpers::*;
use super::{
    SubagentResult, build_subagent_result_context, extract_payload_str,
    extract_payload_string_list, extract_tool_arg,
    global_builtin_sources, is_report_workflow_artifact_tool, is_update_task_status_tool,
    shell_exec_rewritten_args, tool_call_failed,
};

pub(crate) struct HookEvaluationContext<'a> {
    pub(crate) snapshot: &'a SessionHookSnapshot,
    pub(crate) query: &'a HookEvaluationQuery,
}

pub(crate) struct NormalizedHookRule {
    key: &'static str,
    trigger: HookTrigger,
    matches: fn(&HookEvaluationContext<'_>) -> bool,
    apply: fn(&HookEvaluationContext<'_>, &mut HookResolution),
}

pub(crate) fn apply_hook_rules(ctx: HookEvaluationContext<'_>, resolution: &mut HookResolution) {
    for rule in hook_rule_registry() {
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
            break;
        }
    }
}

pub(crate) fn hook_rule_registry() -> &'static [NormalizedHookRule] {
    &[
        NormalizedHookRule {
            key: "tool:shell_exec:rewrite_absolute_cwd",
            trigger: HookTrigger::BeforeTool,
            matches: rule_matches_shell_exec_absolute_cwd_rewrite,
            apply: rule_apply_shell_exec_absolute_cwd_rewrite,
        },
        NormalizedHookRule {
            key: "workflow_step:implement:block_completed_status",
            trigger: HookTrigger::BeforeTool,
            matches: rule_matches_implement_completed_status_block,
            apply: rule_apply_implement_completed_status_block,
        },
        NormalizedHookRule {
            key: "workflow_step:checklist:status_signal_refresh",
            trigger: HookTrigger::BeforeTool,
            matches: rule_matches_checklist_status_signal,
            apply: rule_apply_checklist_status_signal,
        },
        NormalizedHookRule {
            key: "workflow_step:implement:block_record_artifact",
            trigger: HookTrigger::BeforeTool,
            matches: rule_matches_implement_record_artifact_block,
            apply: rule_apply_implement_record_artifact_block,
        },
        NormalizedHookRule {
            key: "global_builtin:supervised:ask_tool_approval",
            trigger: HookTrigger::BeforeTool,
            matches: rule_matches_supervised_tool_approval,
            apply: rule_apply_supervised_tool_approval,
        },
        NormalizedHookRule {
            key: "workflow_runtime:after_tool_refresh",
            trigger: HookTrigger::AfterTool,
            matches: rule_matches_after_tool_refresh,
            apply: rule_apply_after_tool_refresh,
        },
        NormalizedHookRule {
            key: "workflow_completion:session_ended:notice",
            trigger: HookTrigger::BeforeStop,
            matches: rule_matches_session_ended_notice,
            apply: rule_apply_session_ended_notice,
        },
        NormalizedHookRule {
            key: "workflow_completion:checklist_pending:stop_gate",
            trigger: HookTrigger::BeforeStop,
            matches: rule_matches_checklist_pending_gate,
            apply: rule_apply_checklist_pending_gate,
        },
        NormalizedHookRule {
            key: "task_runtime:running_status:stop_gate",
            trigger: HookTrigger::BeforeStop,
            matches: rule_matches_task_running_stop_gate,
            apply: rule_apply_task_running_stop_gate,
        },
        NormalizedHookRule {
            key: "workflow_completion:manual:notice",
            trigger: HookTrigger::BeforeStop,
            matches: rule_matches_manual_notice,
            apply: rule_apply_manual_notice,
        },
        NormalizedHookRule {
            key: "subagent_dispatch:inherit_runtime_context",
            trigger: HookTrigger::BeforeSubagentDispatch,
            matches: rule_matches_subagent_dispatch,
            apply: rule_apply_subagent_dispatch,
        },
        NormalizedHookRule {
            key: "subagent_dispatch:record_dispatch_result",
            trigger: HookTrigger::AfterSubagentDispatch,
            matches: rule_matches_subagent_dispatch_result,
            apply: rule_apply_subagent_dispatch_result,
        },
        NormalizedHookRule {
            key: "subagent_result:return_channel_recorded",
            trigger: HookTrigger::SubagentResult,
            matches: rule_matches_subagent_result,
            apply: rule_apply_subagent_result,
        },
    ]
}

pub(crate) fn rule_matches_implement_completed_status_block(ctx: &HookEvaluationContext<'_>) -> bool {
    let Some(tool_name) = ctx.query.tool_name.as_deref() else {
        return false;
    };
    is_update_task_status_tool(tool_name)
        && extract_tool_arg(ctx.query.payload.as_ref(), "status").is_some_and(|status| {
            active_workflow_denied_task_statuses(ctx.snapshot)
                .iter()
                .any(|item| item == status)
        })
}

pub(crate) fn rule_matches_shell_exec_absolute_cwd_rewrite(ctx: &HookEvaluationContext<'_>) -> bool {
    let Some(tool_name) = ctx.query.tool_name.as_deref() else {
        return false;
    };
    tool_name.ends_with("shell_exec")
        && shell_exec_rewritten_args(ctx.snapshot, ctx.query.payload.as_ref()).is_some()
}

pub(crate) fn rule_apply_shell_exec_absolute_cwd_rewrite(
    ctx: &HookEvaluationContext<'_>,
    resolution: &mut HookResolution,
) {
    let Some(rewritten_args) = shell_exec_rewritten_args(ctx.snapshot, ctx.query.payload.as_ref())
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
        summary: "Hook 已把 shell_exec 的绝对 cwd 改写为相对 workspace root 的路径".to_string(),
        detail: Some(format!("rewritten_cwd={rewritten_cwd}")),
        source_summary: vec![
            "tool:shell_exec".to_string(),
            "hook_rewrite:absolute_cwd".to_string(),
        ],
        source_refs: global_builtin_sources(),
    });
}

pub(crate) fn rule_apply_implement_completed_status_block(
    ctx: &HookEvaluationContext<'_>,
    resolution: &mut HookResolution,
) {
    resolution.block_reason = Some(
        "当前 workflow contract 禁止把 Task 直接迁移到该目标状态；请先满足当前 step 的检查与交接要求。"
            .to_string(),
    );
    resolution.diagnostics.push(HookDiagnosticEntry {
        code: "before_tool_task_status_blocked".to_string(),
        summary: "Hook 根据 workflow contract 阻止了当前 Task 状态迁移".to_string(),
        detail: extract_tool_arg(ctx.query.payload.as_ref(), "status")
            .map(|status| format!("blocked_status={status}")),
        source_summary: active_workflow_source_summary(ctx.snapshot),
        source_refs: active_workflow_source_refs(ctx.snapshot),
    });
}

pub(crate) fn rule_matches_checklist_status_signal(ctx: &HookEvaluationContext<'_>) -> bool {
    let Some(tool_name) = ctx.query.tool_name.as_deref() else {
        return false;
    };
    is_update_task_status_tool(tool_name)
        && extract_tool_arg(ctx.query.payload.as_ref(), "status").is_some_and(|status| {
            active_workflow_task_status_check_statuses(ctx.snapshot)
                .iter()
                .any(|item| item == status)
        })
}

pub(crate) fn rule_apply_checklist_status_signal(
    ctx: &HookEvaluationContext<'_>,
    resolution: &mut HookResolution,
) {
    let next_status = extract_tool_arg(ctx.query.payload.as_ref(), "status").unwrap_or("unknown");
    resolution.refresh_snapshot = true;
    resolution.diagnostics.push(HookDiagnosticEntry {
        code: "before_tool_check_status_signal".to_string(),
        summary: format!("捕获到 contract check 状态信号：Task 即将更新为 `{next_status}`"),
        detail: None,
        source_summary: active_workflow_source_summary(ctx.snapshot),
        source_refs: active_workflow_source_refs(ctx.snapshot),
    });
}

pub(crate) fn rule_matches_implement_record_artifact_block(ctx: &HookEvaluationContext<'_>) -> bool {
    let Some(tool_name) = ctx.query.tool_name.as_deref() else {
        return false;
    };
    is_report_workflow_artifact_tool(tool_name)
        && extract_tool_arg(ctx.query.payload.as_ref(), "artifact_type").is_some_and(
            |artifact_type| {
                active_workflow_denied_record_artifact_types(ctx.snapshot)
                    .iter()
                    .any(|item| item == artifact_type)
            },
        )
}

pub(crate) fn rule_matches_supervised_tool_approval(ctx: &HookEvaluationContext<'_>) -> bool {
    let Some(tool_name) = ctx.query.tool_name.as_deref() else {
        return false;
    };
    session_permission_policy(ctx.snapshot)
        .is_some_and(|policy| policy.eq_ignore_ascii_case("SUPERVISED"))
        && requires_supervised_tool_approval(tool_name)
}

pub(crate) fn rule_apply_implement_record_artifact_block(
    ctx: &HookEvaluationContext<'_>,
    resolution: &mut HookResolution,
) {
    resolution.block_reason = Some(
        "当前 workflow contract 禁止在此 step 上报该类记录产物，请先满足当前 step 的收口要求。"
            .to_string(),
    );
    resolution.diagnostics.push(HookDiagnosticEntry {
        code: "before_tool_record_artifact_blocked".to_string(),
        summary: "Hook 根据 workflow contract 阻止了当前记录产物上报".to_string(),
        detail: extract_tool_arg(ctx.query.payload.as_ref(), "artifact_type")
            .map(|artifact_type| format!("blocked_artifact_type={artifact_type}")),
        source_summary: active_workflow_source_summary(ctx.snapshot),
        source_refs: active_workflow_source_refs(ctx.snapshot),
    });
}

pub(crate) fn rule_apply_supervised_tool_approval(
    ctx: &HookEvaluationContext<'_>,
    resolution: &mut HookResolution,
) {
    let tool_name = ctx.query.tool_name.as_deref().unwrap_or("unknown_tool");
    resolution.approval_request = Some(HookApprovalRequest {
        reason: format!("当前会话使用 SUPERVISED 权限策略，执行 `{tool_name}` 前需要用户审批。"),
        details: Some(serde_json::json!({
            "policy": "supervised_tool_approval",
            "permission_policy": session_permission_policy(ctx.snapshot).unwrap_or("SUPERVISED"),
            "tool_name": tool_name,
        })),
    });
    resolution.diagnostics.push(HookDiagnosticEntry {
        code: "before_tool_requires_approval".to_string(),
        summary: format!("Hook 要求在执行 `{tool_name}` 前进入人工审批"),
        detail: Some("permission_policy=SUPERVISED".to_string()),
        source_summary: vec![
            "global_builtin:supervised_tool_approval".to_string(),
            format!("tool:{tool_name}"),
        ],
        source_refs: global_builtin_sources(),
    });
}

pub(crate) fn rule_matches_after_tool_refresh(ctx: &HookEvaluationContext<'_>) -> bool {
    let Some(tool_name) = ctx.query.tool_name.as_deref() else {
        return false;
    };
    !tool_call_failed(ctx.query.payload.as_ref())
        && (is_update_task_status_tool(tool_name) || is_report_workflow_artifact_tool(tool_name))
}

pub(crate) fn rule_apply_after_tool_refresh(ctx: &HookEvaluationContext<'_>, resolution: &mut HookResolution) {
    let tool_name = ctx.query.tool_name.as_deref().unwrap_or("unknown_tool");
    resolution.refresh_snapshot = true;
    resolution.diagnostics.push(HookDiagnosticEntry {
        code: "after_tool_runtime_refresh".to_string(),
        summary: format!("工具 `{tool_name}` 可能改变 workflow/hook 观察面，已请求刷新 snapshot"),
        detail: None,
        source_summary: active_workflow_source_summary(ctx.snapshot),
        source_refs: active_workflow_source_refs(ctx.snapshot),
    });
}

pub(crate) fn rule_matches_session_ended_notice(ctx: &HookEvaluationContext<'_>) -> bool {
    workflow_transition_policy(ctx.snapshot) == Some("session_terminal_matches")
        || (workflow_auto_completion_snapshot(ctx.snapshot)
            && active_workflow_checks(ctx.snapshot)
                .iter()
                .any(|check| check.kind == WorkflowCheckKind::SessionTerminalIn))
}

pub(crate) fn rule_apply_session_ended_notice(
    ctx: &HookEvaluationContext<'_>,
    resolution: &mut HookResolution,
) {
    resolution.diagnostics.push(HookDiagnosticEntry {
        code: "before_stop_session_ended".to_string(),
        summary: "当前 workflow step 会在 session 进入终态后自然推进".to_string(),
        detail: None,
        source_summary: vec!["workflow_transition:session_terminal_matches".to_string()],
        source_refs: active_workflow_source_refs(ctx.snapshot),
    });
    resolution.completion.get_or_insert(HookCompletionStatus {
        mode: "session_terminal_matches".to_string(),
        satisfied: false,
        advanced: false,
        reason: "当前 step 需要等待 session 真正进入终态，再由 runtime 推进".to_string(),
    });
}

pub(crate) fn rule_matches_checklist_pending_gate(ctx: &HookEvaluationContext<'_>) -> bool {
    workflow_auto_completion_snapshot(ctx.snapshot)
        && active_workflow_constraints(ctx.snapshot)
            .iter()
            .any(|constraint| constraint.kind == WorkflowConstraintKind::BlockStopUntilChecksPass)
        && active_workflow_contract(ctx.snapshot)
            .map(|contract| {
                !evaluate_step_completion(
                    Some(&contract.completion),
                    &WorkflowCompletionSignalSet {
                        task_status: active_task_status(ctx.snapshot).map(ToString::to_string),
                        checklist_evidence_present: checklist_evidence_present(ctx.snapshot),
                        ..WorkflowCompletionSignalSet::default()
                    },
                )
                .satisfied
            })
            .unwrap_or(false)
}

pub(crate) fn rule_apply_checklist_pending_gate(
    ctx: &HookEvaluationContext<'_>,
    resolution: &mut HookResolution,
) {
    resolution.context_fragments.push(HookContextFragment {
        slot: "workflow".to_string(),
        label: "before_stop_check_gate".to_string(),
        content: [
            "## Session Stop Gate",
            "- 当前 workflow step 通过 contract checks 自动推进。",
            "- 结束前请先补齐验证结论、剩余风险与必要证据，让 checks 真正满足。",
            "- 如果 contract 依赖 Task 状态或 checklist evidence，请先补齐对应信号。",
            "- 只要 checks 尚未满足，就不要直接结束本轮 session。",
        ]
        .join("\n"),
        source_summary: active_workflow_source_summary(ctx.snapshot),
        source_refs: active_workflow_source_refs(ctx.snapshot),
    });
    resolution.constraints.push(HookConstraint {
        key: "before_stop:workflow_checks_pending".to_string(),
        description:
            "当前 step 的 contract checks 还未满足；请先补齐验证结论、必要证据与状态信号，再结束 session。"
                .to_string(),
        source_summary: active_workflow_source_summary(ctx.snapshot),
        source_refs: active_workflow_source_refs(ctx.snapshot),
    });
    resolution.diagnostics.push(HookDiagnosticEntry {
        code: "before_stop_workflow_checks_pending".to_string(),
        summary: "当前 workflow step 尚未满足 contract checks，Hook 要求继续 loop".to_string(),
        detail: Some(format!(
            "current_task_status={}, checklist_evidence_present={}",
            active_task_status(ctx.snapshot).unwrap_or("unknown"),
            checklist_evidence_present(ctx.snapshot)
        )),
        source_summary: active_workflow_source_summary(ctx.snapshot),
        source_refs: active_workflow_source_refs(ctx.snapshot),
    });
}

pub(crate) fn rule_matches_task_running_stop_gate(ctx: &HookEvaluationContext<'_>) -> bool {
    active_task_status(ctx.snapshot) == Some("running")
        && !workflow_auto_completion_snapshot(ctx.snapshot)
}

pub(crate) fn rule_apply_task_running_stop_gate(
    _ctx: &HookEvaluationContext<'_>,
    resolution: &mut HookResolution,
) {
    resolution.context_fragments.push(HookContextFragment {
        slot: "workflow".to_string(),
        label: "before_stop_task_status_gate".to_string(),
        content: [
            "## Task Stop Gate",
            "- 当前 session 关联的 Task 仍处于 `running`。",
            "- 自然结束前，必须显式把 Task 迁移到新的终态或交接态。",
            "- 正常完成实现/检查时，优先更新为 `awaiting_verification`。",
            "- 如果执行失败，请显式更新为 `failed` 并说明原因。",
        ]
        .join("\n"),
        source_summary: vec!["task_status:running".to_string()],
        source_refs: Vec::new(),
    });
    resolution.constraints.push(HookConstraint {
        key: "before_stop:task_status_running".to_string(),
        description:
            "当前 Task 仍为 running；请先显式更新 Task 状态（通常为 awaiting_verification / completed / failed），再结束 session。"
                .to_string(),
        source_summary: vec!["task_status:running".to_string()],
        source_refs: Vec::new(),
    });
    resolution.diagnostics.push(HookDiagnosticEntry {
        code: "before_stop_task_status_running".to_string(),
        summary: "Task 仍处于 running，Hook 阻止当前 session 自然结束".to_string(),
        detail: Some("expected_status=awaiting_verification|completed|failed".to_string()),
        source_summary: vec!["task_status:running".to_string()],
        source_refs: Vec::new(),
    });
}

pub(crate) fn rule_matches_manual_notice(ctx: &HookEvaluationContext<'_>) -> bool {
    workflow_transition_policy(ctx.snapshot) == Some("manual")
}

pub(crate) fn rule_apply_manual_notice(ctx: &HookEvaluationContext<'_>, resolution: &mut HookResolution) {
    resolution.diagnostics.push(HookDiagnosticEntry {
        code: "before_stop_manual_phase".to_string(),
        summary: "当前 workflow step 使用 manual transition，不会由 Hook 自动推进 step".to_string(),
        detail: None,
        source_summary: active_workflow_source_summary(ctx.snapshot),
        source_refs: active_workflow_source_refs(ctx.snapshot),
    });
    resolution.completion.get_or_insert(HookCompletionStatus {
        mode: "manual".to_string(),
        satisfied: false,
        advanced: false,
        reason: "manual step 需要显式推进".to_string(),
    });
}

pub(crate) fn rule_matches_subagent_dispatch(ctx: &HookEvaluationContext<'_>) -> bool {
    ctx.query
        .subagent_type
        .as_deref()
        .is_some_and(|value| !value.trim().is_empty())
}

pub(crate) fn rule_apply_subagent_dispatch(ctx: &HookEvaluationContext<'_>, resolution: &mut HookResolution) {
    let subagent_type = ctx.query.subagent_type.as_deref().unwrap_or("companion");
    resolution
        .context_fragments
        .extend(ctx.snapshot.context_fragments.clone());
    resolution
        .constraints
        .extend(ctx.snapshot.constraints.clone());
    resolution.diagnostics.push(HookDiagnosticEntry {
        code: "before_subagent_dispatch_prepared".to_string(),
        summary: format!(
            "已为 `{subagent_type}` 准备 companion/subagent dispatch 上下文与约束继承"
        ),
        detail: workflow_step_key(ctx.snapshot).map(|step_key| format!("step={step_key}")),
        source_summary: active_workflow_source_summary(ctx.snapshot),
        source_refs: active_workflow_source_refs(ctx.snapshot),
    });
}

pub(crate) fn rule_matches_subagent_dispatch_result(ctx: &HookEvaluationContext<'_>) -> bool {
    ctx.query
        .subagent_type
        .as_deref()
        .is_some_and(|value| !value.trim().is_empty())
}

pub(crate) fn rule_apply_subagent_dispatch_result(
    ctx: &HookEvaluationContext<'_>,
    resolution: &mut HookResolution,
) {
    let subagent_type = ctx.query.subagent_type.as_deref().unwrap_or("companion");
    let companion_session_id = ctx
        .query
        .payload
        .as_ref()
        .and_then(|value| value.get("companion_session_id"))
        .and_then(serde_json::Value::as_str);
    let turn_id = ctx
        .query
        .payload
        .as_ref()
        .and_then(|value| value.get("turn_id"))
        .and_then(serde_json::Value::as_str);

    resolution.diagnostics.push(HookDiagnosticEntry {
        code: "after_subagent_dispatch_recorded".to_string(),
        summary: format!("已记录 `{subagent_type}` 的 subagent dispatch 结果"),
        detail: Some(format!(
            "companion_session_id={}, turn_id={}",
            companion_session_id.unwrap_or("-"),
            turn_id.unwrap_or("-")
        )),
        source_summary: active_workflow_source_summary(ctx.snapshot),
        source_refs: active_workflow_source_refs(ctx.snapshot),
    });
}

pub(crate) fn rule_matches_subagent_result(ctx: &HookEvaluationContext<'_>) -> bool {
    ctx.query
        .payload
        .as_ref()
        .and_then(|value| value.get("summary"))
        .and_then(serde_json::Value::as_str)
        .is_some_and(|summary| !summary.trim().is_empty())
}

pub(crate) fn rule_apply_subagent_result(ctx: &HookEvaluationContext<'_>, resolution: &mut HookResolution) {
    let subagent_type = ctx.query.subagent_type.as_deref().unwrap_or("companion");
    let summary = extract_payload_str(ctx.query.payload.as_ref(), "summary").unwrap_or("无摘要");
    let status = extract_payload_str(ctx.query.payload.as_ref(), "status").unwrap_or("completed");
    let companion_session_id =
        extract_payload_str(ctx.query.payload.as_ref(), "companion_session_id").unwrap_or("-");
    let adoption_mode =
        extract_payload_str(ctx.query.payload.as_ref(), "adoption_mode").unwrap_or("suggestion");
    let dispatch_id = extract_payload_str(ctx.query.payload.as_ref(), "dispatch_id").unwrap_or("-");
    let findings = extract_payload_string_list(ctx.query.payload.as_ref(), "findings");
    let follow_ups = extract_payload_string_list(ctx.query.payload.as_ref(), "follow_ups");
    let artifact_refs = extract_payload_string_list(ctx.query.payload.as_ref(), "artifact_refs");

    resolution.diagnostics.push(HookDiagnosticEntry {
        code: "subagent_result_recorded".to_string(),
        summary: format!("已收到 `{subagent_type}` 的回流结果：{summary}"),
        detail: Some(format!(
            "status={status}, adoption_mode={adoption_mode}, companion_session_id={companion_session_id}, dispatch_id={dispatch_id}"
        )),
        source_summary: active_workflow_source_summary(ctx.snapshot),
        source_refs: active_workflow_source_refs(ctx.snapshot),
    });

    match adoption_mode {
        "follow_up_required" | "blocking_review" => {
            let is_blocking = adoption_mode == "blocking_review";
            resolution.context_fragments.push(HookContextFragment {
                slot: "workflow".to_string(),
                label: if is_blocking {
                    "subagent_blocking_review".to_string()
                } else {
                    "subagent_follow_up_required".to_string()
                },
                content: build_subagent_result_context(&SubagentResult {
                    subagent_type,
                    summary,
                    status,
                    dispatch_id,
                    companion_session_id,
                    findings: &findings,
                    follow_ups: &follow_ups,
                    artifact_refs: &artifact_refs,
                    is_blocking,
                }),
                source_summary: active_workflow_source_summary(ctx.snapshot),
                source_refs: active_workflow_source_refs(ctx.snapshot),
            });
            resolution.constraints.push(HookConstraint {
                key: if is_blocking {
                    "subagent_result:blocking_review".to_string()
                } else {
                    "subagent_result:follow_up_required".to_string()
                },
                description: if is_blocking {
                    format!(
                        "当前 `{subagent_type}` 回流被标记为 blocking_review；主 session 必须先明确采纳/拒绝/拆解该结果，再继续其它推进或自然结束。"
                    )
                } else {
                    format!(
                        "当前 `{subagent_type}` 回流要求 follow-up；主 session 需要先吸收结果并落实下一步动作，再继续推进。"
                    )
                },
                source_summary: active_workflow_source_summary(ctx.snapshot),
                source_refs: active_workflow_source_refs(ctx.snapshot),
            });
            resolution.diagnostics.push(HookDiagnosticEntry {
                code: if is_blocking {
                    "subagent_result_blocking_review_enqueued".to_string()
                } else {
                    "subagent_result_follow_up_enqueued".to_string()
                },
                summary: if is_blocking {
                    "已把 companion 回流升级为阻塞式 review 待办，要求主 session 优先处理"
                        .to_string()
                } else {
                    "已把 companion 回流升级为 follow-up 待办，要求主 session 继续处理".to_string()
                },
                detail: Some(format!(
                    "findings={}, follow_ups={}, artifact_refs={}",
                    findings.len(),
                    follow_ups.len(),
                    artifact_refs.len()
                )),
                source_summary: active_workflow_source_summary(ctx.snapshot),
                source_refs: active_workflow_source_refs(ctx.snapshot),
            });
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    use agentdash_spi::{
        HookOwnerSummary, HookSourceLayer, HookSourceRef, HookTrigger, SessionHookSnapshot,
    };
    use crate::workflow::{evaluate_step_completion, WorkflowCompletionSignalSet};

    use super::super::test_fixtures::*;

    #[test]
    fn before_tool_blocks_completed_during_implement_phase() {
        let snapshot = snapshot_with_workflow("implement", "session_ended", Some("running"));
        let mut resolution = HookResolution::default();
        let query = HookEvaluationQuery {
            session_id: snapshot.session_id.clone(),
            trigger: HookTrigger::BeforeTool,
            turn_id: None,
            tool_name: Some("mcp_agentdash_task_tools_demo_update_task_status".to_string()),
            tool_call_id: Some("call-1".to_string()),
            subagent_type: None,
            snapshot: None,
            payload: Some(serde_json::json!({
                "args": {
                    "status": "completed"
                }
            })),
        };

        apply_hook_rules(
            HookEvaluationContext {
                snapshot: &snapshot,
                query: &query,
            },
            &mut resolution,
        );

        assert!(resolution.block_reason.is_some());
        assert!(
            resolution
                .matched_rule_keys
                .contains(&"workflow_step:implement:block_completed_status".to_string())
        );
        assert!(
            resolution
                .diagnostics
                .iter()
                .any(|entry| entry.code == "before_tool_task_status_blocked")
        );
    }

    #[test]
    fn before_tool_rewrites_shell_exec_absolute_cwd_to_workspace_relative() {
        let snapshot = snapshot_with_workflow("implement", "session_ended", Some("running"));
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
                "args": {
                    "cwd": "F:\\Projects\\AgentDash\\crates\\agentdash-agent",
                    "command": "cargo test"
                }
            })),
        };

        apply_hook_rules(
            HookEvaluationContext {
                snapshot: &snapshot,
                query: &query,
            },
            &mut resolution,
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
    fn before_stop_requires_checklist_completion_when_task_not_ready() {
        let snapshot = snapshot_with_workflow("check", "checklist_passed", Some("running"));
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
        };

        apply_hook_rules(
            HookEvaluationContext {
                snapshot: &snapshot,
                query: &query,
            },
            &mut resolution,
        );

        assert!(!resolution.context_fragments.is_empty());
        assert!(!resolution.constraints.is_empty());
        assert!(
            resolution
                .matched_rule_keys
                .contains(&"workflow_completion:checklist_pending:stop_gate".to_string())
        );
        assert!(
            resolution
                .diagnostics
                .iter()
                .any(|entry| entry.code == "before_stop_workflow_checks_pending")
        );
    }

    #[test]
    fn before_stop_requires_checklist_evidence_even_when_task_ready() {
        let snapshot =
            snapshot_with_workflow("check", "checklist_passed", Some("awaiting_verification"));
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
        };

        apply_hook_rules(
            HookEvaluationContext {
                snapshot: &snapshot,
                query: &query,
            },
            &mut resolution,
        );

        assert!(!resolution.context_fragments.is_empty());
        assert!(!resolution.constraints.is_empty());
        assert!(
            resolution
                .matched_rule_keys
                .contains(&"workflow_completion:checklist_pending:stop_gate".to_string())
        );
    }

    #[test]
    fn before_stop_allows_ready_task_with_checklist_evidence() {
        let snapshot = snapshot_with_workflow_and_evidence(
            "check",
            "checklist_passed",
            Some("awaiting_verification"),
            true,
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
        };

        apply_hook_rules(
            HookEvaluationContext {
                snapshot: &snapshot,
                query: &query,
            },
            &mut resolution,
        );

        assert!(resolution.context_fragments.is_empty());
        assert!(resolution.constraints.is_empty());
        assert!(resolution.matched_rule_keys.is_empty());
    }

    #[test]
    fn after_turn_does_not_inject_perpetual_check_phase_steering() {
        let snapshot = snapshot_with_workflow("check", "checklist_passed", Some("running"));
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
        };

        apply_hook_rules(
            HookEvaluationContext {
                snapshot: &snapshot,
                query: &query,
            },
            &mut resolution,
        );

        assert!(resolution.context_fragments.is_empty());
        assert!(resolution.constraints.is_empty());
        assert!(resolution.matched_rule_keys.is_empty());
    }

    #[test]
    fn before_stop_allows_checklist_completion_when_task_ready() {
        let snapshot = snapshot_with_workflow("check", "checklist_passed", Some("completed"));
        let contract = active_workflow_contract(&snapshot).expect("contract");
        let decision = evaluate_step_completion(
            Some(&contract.completion),
            &WorkflowCompletionSignalSet {
                task_status: active_task_status(&snapshot).map(ToString::to_string),
                checklist_evidence_present: true,
                ..WorkflowCompletionSignalSet::default()
            },
        );

        assert!(decision.satisfied);
        assert!(decision.should_complete_step);
        assert_eq!(
            decision.summary.as_deref(),
            Some("All completion checks passed")
        );
    }

    #[test]
    fn before_stop_blocks_when_task_still_running_without_active_workflow() {
        let snapshot = SessionHookSnapshot {
            session_id: "sess-task-running".to_string(),
            metadata: Some(agentdash_spi::SessionSnapshotMetadata {
                active_task: Some(agentdash_spi::ActiveTaskMeta {
                    task_id: Some("task-1".to_string()),
                    status: Some("running".to_string()),
                    ..Default::default()
                }),
                ..Default::default()
            }),
            ..SessionHookSnapshot::default()
        };
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
        };

        apply_hook_rules(
            HookEvaluationContext {
                snapshot: &snapshot,
                query: &query,
            },
            &mut resolution,
        );

        assert!(
            resolution
                .matched_rule_keys
                .contains(&"task_runtime:running_status:stop_gate".to_string())
        );
        assert!(
            resolution
                .constraints
                .iter()
                .any(|constraint| constraint.key == "before_stop:task_status_running")
        );
        assert!(
            resolution
                .diagnostics
                .iter()
                .any(|entry| entry.code == "before_stop_task_status_running")
        );
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
        };

        apply_hook_rules(
            HookEvaluationContext {
                snapshot: &snapshot,
                query: &query,
            },
            &mut resolution,
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
    fn before_subagent_dispatch_inherits_runtime_context_and_constraints() {
        let snapshot = SessionHookSnapshot {
            session_id: "sess-test".to_string(),
            sources: vec![HookSourceRef {
                layer: HookSourceLayer::Workflow,
                key: "trellis_dev_task:check".to_string(),
                label: "Workflow / Trellis Dev Workflow / Check".to_string(),
                priority: 300,
            }],
            owners: vec![HookOwnerSummary {
                owner_type: "story".to_string(),
                owner_id: Uuid::new_v4().to_string(),
                label: Some("Story A".to_string()),
                project_id: None,
                story_id: None,
                task_id: None,
            }],
            context_fragments: vec![HookContextFragment {
                slot: "workflow".to_string(),
                label: "active_workflow_step".to_string(),
                content: "step info".to_string(),
                source_summary: vec!["workflow:trellis_dev_task".to_string()],
                source_refs: vec![HookSourceRef {
                    layer: HookSourceLayer::Workflow,
                    key: "trellis_dev_task:check".to_string(),
                    label: "Workflow / Trellis Dev Workflow / Check".to_string(),
                    priority: 300,
                }],
            }],
            constraints: vec![HookConstraint {
                key: "workflow:check".to_string(),
                description: "先验证再结束".to_string(),
                source_summary: vec!["workflow_step:check".to_string()],
                source_refs: vec![HookSourceRef {
                    layer: HookSourceLayer::Workflow,
                    key: "trellis_dev_task:check".to_string(),
                    label: "Workflow / Trellis Dev Workflow / Check".to_string(),
                    priority: 300,
                }],
            }],
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
        };

        apply_hook_rules(
            HookEvaluationContext {
                snapshot: &snapshot,
                query: &query,
            },
            &mut resolution,
        );

        assert_eq!(resolution.context_fragments.len(), 1);
        assert_eq!(resolution.constraints.len(), 1);
        assert!(
            resolution
                .matched_rule_keys
                .contains(&"subagent_dispatch:inherit_runtime_context".to_string())
        );
    }

    #[test]
    fn subagent_result_records_structured_return_channel_diagnostic() {
        let snapshot =
            snapshot_with_workflow("check", "checklist_passed", Some("awaiting_verification"));
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
        };

        apply_hook_rules(
            HookEvaluationContext {
                snapshot: &snapshot,
                query: &query,
            },
            &mut resolution,
        );

        assert!(
            resolution
                .matched_rule_keys
                .contains(&"subagent_result:return_channel_recorded".to_string())
        );
        assert!(
            resolution
                .diagnostics
                .iter()
                .any(|entry| entry.code == "subagent_result_recorded"
                    && entry.summary.contains("子 agent 已完成 review"))
        );
        assert_eq!(resolution.context_fragments.len(), 1);
        assert_eq!(resolution.constraints.len(), 1);
        assert!(
            resolution.context_fragments[0]
                .label
                .contains("subagent_blocking_review")
        );
        assert!(
            resolution.constraints[0]
                .key
                .contains("subagent_result:blocking_review")
        );
        assert!(
            resolution
                .diagnostics
                .iter()
                .any(|entry| entry.code == "subagent_result_blocking_review_enqueued")
        );
    }
}
