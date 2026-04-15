use agent_client_protocol::{SessionId, SessionInfoUpdate, SessionNotification, SessionUpdate};
use agentdash_acp_meta::{
    AgentDashEventV1, AgentDashMetaV1, AgentDashSourceV1, AgentDashTraceV1, merge_agentdash_meta,
};
use serde_json::json;

use crate::{HookTraceEntry, HookTrigger};

pub fn build_hook_trace_notification(
    session_id: &str,
    turn_id: Option<&str>,
    source: AgentDashSourceV1,
    entry: &HookTraceEntry,
) -> Option<SessionNotification> {
    let mut trace = AgentDashTraceV1::new();
    trace.turn_id = turn_id.map(ToString::to_string);

    let mut event = AgentDashEventV1::new("hook_event");
    event.severity = Some(hook_event_severity(entry).to_string());
    event.code = Some(format!(
        "hook:{}:{}",
        hook_trigger_key(&entry.trigger),
        normalize_event_decision(&entry.decision)
    ));
    event.message = Some(describe_hook_trace(entry));
    let injections_data: Vec<serde_json::Value> = entry
        .injections
        .iter()
        .map(|inj| {
            json!({
                "slot": inj.slot,
                "source": inj.source,
                "content": inj.content,
            })
        })
        .collect();
    event.data = Some(json!({
        "trigger": hook_trigger_key(&entry.trigger),
        "decision": entry.decision,
        "sequence": entry.sequence,
        "revision": entry.revision,
        "tool_name": entry.tool_name,
        "tool_call_id": entry.tool_call_id,
        "subagent_type": entry.subagent_type,
        "matched_rule_keys": entry.matched_rule_keys,
        "refresh_snapshot": entry.refresh_snapshot,
        "block_reason": entry.block_reason,
        "completion": entry.completion,
        "diagnostic_codes": entry.diagnostics.iter().map(|item| item.code.clone()).collect::<Vec<_>>(),
        "diagnostics": entry.diagnostics,
        "injections": injections_data,
    }));

    let agentdash = AgentDashMetaV1::new()
        .source(Some(source))
        .trace(Some(trace))
        .event(Some(event));

    Some(SessionNotification::new(
        SessionId::new(session_id.to_string()),
        SessionUpdate::SessionInfoUpdate(SessionInfoUpdate::new().meta(
            merge_agentdash_meta(None, &agentdash).expect("构造 hook trace ACP Meta 不应失败"),
        )),
    ))
}

fn hook_event_severity(entry: &HookTraceEntry) -> &'static str {
    if entry.block_reason.is_some() || matches!(entry.decision.as_str(), "deny" | "blocked") {
        return "error";
    }
    if matches!(
        entry.decision.as_str(),
        "ask" | "rewrite" | "steering_injected" | "continue"
    ) {
        return "warning";
    }
    if matches!(entry.decision.as_str(), "step_advanced") {
        return "success";
    }
    "info"
}

fn normalize_event_decision(decision: &str) -> String {
    decision
        .trim()
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect()
}

pub fn hook_trigger_key(trigger: &HookTrigger) -> &'static str {
    match trigger {
        HookTrigger::SessionStart => "session_start",
        HookTrigger::UserPromptSubmit => "user_prompt_submit",
        HookTrigger::BeforeTool => "before_tool",
        HookTrigger::AfterTool => "after_tool",
        HookTrigger::AfterTurn => "after_turn",
        HookTrigger::BeforeStop => "before_stop",
        HookTrigger::SessionTerminal => "session_terminal",
        HookTrigger::BeforeSubagentDispatch => "before_subagent_dispatch",
        HookTrigger::AfterSubagentDispatch => "after_subagent_dispatch",
        HookTrigger::SubagentResult => "subagent_result",
        HookTrigger::BeforeCompact => "before_compact",
        HookTrigger::AfterCompact => "after_compact",
        HookTrigger::BeforeProviderRequest => "before_provider_request",
    }
}

fn describe_hook_trace(entry: &HookTraceEntry) -> String {
    match (&entry.trigger, entry.decision.as_str()) {
        (HookTrigger::SessionStart, "baseline_initialized") => {
            "Hook 已完成当前会话的 baseline 初始化".to_string()
        }
        (HookTrigger::SessionStart, "baseline_refreshed") => {
            "Hook 在会话启动阶段请求并完成了 baseline 刷新".to_string()
        }
        (HookTrigger::UserPromptSubmit, "context_injected") => {
            "Hook 已为当前输入注入动态上下文".to_string()
        }
        (HookTrigger::BeforeTool, "ask") => "Hook 要求当前工具调用先经过审批".to_string(),
        (HookTrigger::BeforeTool, "deny") => "Hook 阻止了当前工具调用".to_string(),
        (HookTrigger::BeforeTool, "rewrite") => "Hook 改写了当前工具调用参数".to_string(),
        (HookTrigger::AfterTool, "refresh_requested") => {
            "Hook 在工具执行后请求刷新运行时快照".to_string()
        }
        (HookTrigger::AfterTurn, "steering_injected") => {
            "Hook 在本轮结束后追加了新的流程约束".to_string()
        }
        (HookTrigger::BeforeStop, "continue") => {
            if entry
                .completion
                .as_ref()
                .is_some_and(|completion| completion.satisfied)
            {
                "Hook 认为阶段条件已满足，但仍要求继续处理剩余约束".to_string()
            } else {
                "Hook 阻止了当前结束并要求继续执行".to_string()
            }
        }
        (HookTrigger::BeforeStop, "stop") => "Hook 允许当前会话自然结束".to_string(),
        (HookTrigger::SessionTerminal, "step_advanced") => {
            "Hook 在会话结束后推进了 workflow step".to_string()
        }
        (HookTrigger::SessionTerminal, "terminal_observed") => {
            "Hook 已记录当前会话终态".to_string()
        }
        (HookTrigger::BeforeSubagentDispatch, "deny") => {
            "Hook 阻止了当前 subagent 派发".to_string()
        }
        (HookTrigger::BeforeSubagentDispatch, "allow") => {
            "Hook 放行了当前 subagent 派发".to_string()
        }
        (HookTrigger::AfterSubagentDispatch, _) => "Hook 已记录 subagent 派发结果".to_string(),
        (HookTrigger::SubagentResult, _) => "Hook 已接收 subagent 返回结果".to_string(),
        (HookTrigger::UserPromptSubmit, "blocked") => "Hook 阻止了当前用户输入".to_string(),
        (HookTrigger::BeforeProviderRequest, "observed") => {
            "Hook 已观测到 LLM API 请求即将发出".to_string()
        }
        _ => format!(
            "Hook 在 {} 阶段产生了 {} 决策",
            hook_trigger_key(&entry.trigger),
            entry.decision
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{HookCompletionStatus, HookDiagnosticEntry, HookInjection};

    fn sample_source() -> AgentDashSourceV1 {
        let mut source = AgentDashSourceV1::new("pi-agent", "local_executor");
        source.executor_id = Some("PI_AGENT".to_string());
        source
    }

    fn silent_entry(decision: &str) -> HookTraceEntry {
        HookTraceEntry {
            sequence: 1,
            timestamp_ms: 1,
            revision: 1,
            trigger: HookTrigger::BeforeTool,
            decision: decision.to_string(),
            tool_name: None,
            tool_call_id: None,
            subagent_type: None,
            matched_rule_keys: Vec::new(),
            refresh_snapshot: false,
            block_reason: None,
            completion: None,
            diagnostics: Vec::new(),
            injections: Vec::new(),
        }
    }

    #[test]
    fn emit_all_events_including_silent() {
        for decision in [
            "allow",
            "noop",
            "stop",
            "terminal_observed",
            "refresh_requested",
        ] {
            let notification = build_hook_trace_notification(
                "sess-1",
                Some("t-1"),
                sample_source(),
                &silent_entry(decision),
            );
            assert!(
                notification.is_some(),
                "should emit even silent decision: {decision}"
            );
        }
    }

    #[test]
    fn emit_stop_with_completion() {
        let entry = HookTraceEntry {
            trigger: HookTrigger::BeforeStop,
            completion: Some(HookCompletionStatus {
                mode: "checklist_passed".to_string(),
                satisfied: true,
                advanced: true,
                reason: "已完成".to_string(),
            }),
            ..silent_entry("stop")
        };
        let notification =
            build_hook_trace_notification("sess-1", Some("t-1"), sample_source(), &entry);
        assert!(notification.is_some());
    }

    #[test]
    fn emit_before_stop_continue_trace() {
        let entry = HookTraceEntry {
            sequence: 3,
            timestamp_ms: 1,
            revision: 2,
            trigger: HookTrigger::BeforeStop,
            decision: "continue".to_string(),
            tool_name: None,
            tool_call_id: None,
            subagent_type: None,
            matched_rule_keys: vec!["workflow_completion:checklist_pending:stop_gate".to_string()],
            refresh_snapshot: false,
            block_reason: None,
            completion: Some(HookCompletionStatus {
                mode: "checklist_passed".to_string(),
                satisfied: false,
                advanced: false,
                reason: "未满足".to_string(),
            }),
            diagnostics: vec![HookDiagnosticEntry {
                code: "before_stop_checklist_pending".to_string(),
                message: "需要继续执行".to_string(),
            }],
            injections: Vec::new(),
        };

        let notification =
            build_hook_trace_notification("sess-1", Some("t-1"), sample_source(), &entry)
                .expect("should emit notification");
        let value = serde_json::to_value(notification).expect("serialize notification");
        assert_eq!(
            value
                .get("update")
                .and_then(|item| item.get("_meta"))
                .and_then(|item| item.get("agentdash"))
                .and_then(|item| item.get("event"))
                .and_then(|item| item.get("type"))
                .and_then(serde_json::Value::as_str),
            Some("hook_event")
        );
        assert_eq!(
            value
                .get("update")
                .and_then(|item| item.get("_meta"))
                .and_then(|item| item.get("agentdash"))
                .and_then(|item| item.get("event"))
                .and_then(|item| item.get("severity"))
                .and_then(serde_json::Value::as_str),
            Some("warning")
        );
    }

    #[test]
    fn injections_included_in_data() {
        let entry = HookTraceEntry {
            trigger: HookTrigger::UserPromptSubmit,
            decision: "context_injected".to_string(),
            injections: vec![HookInjection {
                slot: "companion_agents".to_string(),
                content: "## Companion Agents\n- reviewer".to_string(),
                source: "builtin:companion_agents".to_string(),
            }],
            ..silent_entry("context_injected")
        };
        let notification =
            build_hook_trace_notification("sess-1", Some("t-1"), sample_source(), &entry)
                .expect("should emit");
        let value = serde_json::to_value(notification).unwrap();
        let data = value.pointer("/update/_meta/agentdash/event/data").unwrap();
        let injections = data.get("injections").and_then(|v| v.as_array()).unwrap();
        assert_eq!(injections.len(), 1);
        assert_eq!(injections[0]["slot"], "companion_agents");
    }
}
