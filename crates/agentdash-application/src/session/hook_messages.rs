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

pub(super) fn context_fragments_label(sources: &str) -> String {
    format!("已挂载上下文片段：{sources}")
}

pub(super) fn pending_action_injections_section(items_md: &str) -> String {
    format!("关联注入片段：\n{items_md}")
}

pub(super) const PENDING_ACTION_FOOTER: &str = "以上事项来自 Hook Runtime 的待处理回流，优先级高于普通自然对话推进。\
     处理时尽量避免重复总结，聚焦完成剩余动作。";

// ── Auto-resume prompt ──────────────────────────────────────────────────

// 刻意设计为不诱导 Agent 复述/总结上一轮：
// - 不提"上一轮执行结束"（这是 LLM 被动触发 recap 的关键词）
// - 不要求 Agent 做任何"汇报"，只要求继续推进 workflow step
// - 语气贴近人类说"继续"，给 LLM 的信号是"直接动手"而不是"先回顾再动手"
pub(super) const AUTO_RESUME_PROMPT: &str =
    "继续推进当前 workflow step，直接执行未完成的动作或补齐证据。不要重复总结已发生的内容。";
