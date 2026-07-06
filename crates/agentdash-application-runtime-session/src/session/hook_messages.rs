//! Centralized text templates for hook-related agent messages.
//!
//! All agent-facing prose and diagnostic text lives here.
//! Structural markdown tokens (`###`, `-` list markers) remain inline
//! in the builder functions since they are formatting, not prose.

// ── StopDecision::Continue reason (consumed by tracing / tests) ─────────

pub(super) const REASON_PENDING_COMPANION_CONSUMED: &str =
    "pending companion/hook messages consumed, continue loop";

pub(super) const REASON_EXTRA_CONSTRAINTS_PENDING: &str =
    "completion satisfied but additional constraints pending, continue loop";

pub(super) const REASON_STOP_GATE_UNSATISFIED: &str = "stop gate not satisfied, continue loop";

pub(super) fn owners_section(owners_md: &str) -> String {
    format!("## 归属对象\n{owners_md}")
}

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
    use agentdash_spi::action_type as at;
    if action_type == at::BLOCKING_REVIEW {
        "当前事项是阻塞式 review。不要复述前文；\
        直接处理剩余动作，并在完成后明确结案。"
    } else if action_type == at::FOLLOW_UP_REQUIRED {
        "当前事项要求继续跟进。不要停在总结；\
        请直接落实后续动作，并在完成后明确结案。"
    } else {
        "请直接处理这项 Hook 待办，并在完成后明确结案。"
    }
}

pub(super) const PENDING_ACTION_FOOTER: &str = "以上事项来自 Hook Runtime 的待处理回流，优先级高于普通自然对话推进。\
     处理时尽量避免重复总结，聚焦完成剩余动作。";
