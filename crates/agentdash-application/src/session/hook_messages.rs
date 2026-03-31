//! Centralized text templates for hook-related agent messages.
//!
//! All agent-facing prose and diagnostic text lives here.
//! Structural markdown tokens (`###`, `-` list markers) remain inline
//! in the builder functions since they are formatting, not prose.

// ── StopDecision::Continue reason (consumed by tracing / tests) ─────────

pub(super) const REASON_BLOCKING_REVIEW_UNRESOLVED: &str =
    "unresolved blocking_review actions require resolve_hook_action before stop";

pub(super) const REASON_PENDING_COMPANION_CONSUMED: &str =
    "pending companion/hook messages consumed, continue loop";

pub(super) const REASON_EXTRA_CONSTRAINTS_PENDING: &str =
    "completion satisfied but additional constraints pending, continue loop";

pub(super) const REASON_STOP_GATE_UNSATISFIED: &str =
    "stop gate not satisfied, continue loop";

// ── Diagnostic messages ─────────────────────────────────────────────────

pub(super) fn diag_blocking_review_unresolved(action_ids: &str) -> String {
    format!(
        "Unresolved blocking_review hook actions prevent natural stop. \
         action_ids={action_ids}"
    )
}

// ── Stop gate fallback steering ─────────────────────────────────────────

pub(super) const STOP_GATE_DEFAULT_REASON: &str = "completion checks not satisfied";

pub(super) fn stop_gate_fallback_steering(gate_reason: &str) -> String {
    format!(
        "[系统 Hook 上下文 — Stop Gate]\n\n\
         当前 workflow step 的 completion checks 尚未满足：{gate_reason}\n\n\
         请继续完成未尽事项，补齐必要的验证结论和证据，\
         或使用 `report_workflow_artifact` 工具提交产物以推动状态流转。\
         不要直接结束本轮 session。"
    )
}

// ── Hook injection markdown ─────────────────────────────────────────────

pub(super) fn hook_context_header(session_id: &str, revision: u64) -> String {
    format!(
        "[系统动态 Hook 上下文]\n当前 session_id={session_id}，revision={revision}"
    )
}

pub(super) fn owners_section(owners_md: &str) -> String {
    format!("## 归属对象\n{owners_md}")
}

pub(super) const HOOK_INJECTION_FOOTER: &str =
    "以上内容由 Hook Runtime 自动注入，不代表用户新增需求，但必须优先遵守。";

// ── Pending action messages ─────────────────────────────────────────────

pub(super) fn pending_action_header(
    title: &str,
    action_type: &str,
    status: &str,
    revision: u64,
) -> String {
    format!(
        "[待处理 Hook 事项]\n\
         {title}（type={action_type}，status={status}，revision={revision}）"
    )
}

pub(super) fn pending_action_id_line(id: &str) -> String {
    format!("事项 id: {id}")
}

pub(super) fn pending_action_turn_line(turn_id: &str) -> String {
    format!("关联 turn: {turn_id}")
}

pub(super) fn pending_action_instruction(action_type: &str) -> &'static str {
    match action_type {
        "blocking_review" => "\
            当前事项是阻塞式 review。不要复述前文；\
            直接处理剩余动作，并在完成后调用 `resolve_hook_action` 明确结案。",
        "follow_up_required" => "\
            当前事项要求继续跟进。不要停在总结；\
            请直接落实后续动作，并在完成后调用 `resolve_hook_action` 明确结案。",
        _ => "请直接处理这项 Hook 待办，并在完成后调用 `resolve_hook_action` 明确结案。",
    }
}

pub(super) fn context_fragments_label(sources: &str) -> String {
    format!("已挂载上下文片段：{sources}")
}

pub(super) fn constraints_section(items_md: &str) -> String {
    format!("必须完成的约束：\n{items_md}")
}

pub(super) const PENDING_ACTION_FOOTER: &str =
    "以上事项来自 Hook Runtime 的待处理回流，优先级高于普通自然对话推进。\
     处理时尽量避免重复总结，聚焦完成剩余动作。";

// ── Dynamic injection section headings ──────────────────────────────────

pub(super) const DYNAMIC_INJECTION_HEADING: &str = "## 动态注入上下文";

pub(super) fn flow_constraints_section(items_md: &str) -> String {
    format!("## 必须遵守的流程约束\n{items_md}")
}

// ── Auto-resume prompt ──────────────────────────────────────────────────

pub(super) const AUTO_RESUME_PROMPT: &str =
    "[系统自动续跑] 上一轮执行结束但 workflow stop gate 仍未满足。\
     请继续完成未尽事项：补齐验证结论、提交必要产物，或推动状态流转。";
