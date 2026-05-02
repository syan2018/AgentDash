use agentdash_protocol::{
    BackboneEnvelope, BackboneEvent, HookTraceCompletion, HookTraceData, HookTraceDiagnostic,
    HookTraceInjection, HookTracePayload, HookTraceSeverity, PlatformEvent, SourceInfo, TraceInfo,
};

use crate::{HookTraceEntry, HookTrigger};

pub fn build_hook_trace_envelope(
    session_id: &str,
    turn_id: Option<&str>,
    source: SourceInfo,
    entry: &HookTraceEntry,
) -> BackboneEnvelope {
    let injections: Vec<HookTraceInjection> = entry
        .injections
        .iter()
        .map(|inj| HookTraceInjection {
            slot: Some(inj.slot.clone()),
            source: Some(inj.source.clone()),
            content: Some(inj.content.clone()),
        })
        .collect();
    let diagnostics: Vec<HookTraceDiagnostic> = entry
        .diagnostics
        .iter()
        .map(|item| HookTraceDiagnostic {
            code: Some(item.code.clone()),
            message: Some(item.message.clone()),
        })
        .collect();
    let completion = entry.completion.as_ref().map(|status| HookTraceCompletion {
        mode: status.mode.clone(),
        satisfied: status.satisfied,
        advanced: status.advanced,
        reason: status.reason.clone(),
    });
    let data = HookTraceData {
        trigger: entry.trigger.clone(),
        decision: entry.decision.clone(),
        sequence: entry.sequence,
        revision: entry.revision,
        severity: hook_event_severity(entry),
        tool_name: entry.tool_name.clone(),
        tool_call_id: entry.tool_call_id.clone(),
        subagent_type: entry.subagent_type.clone(),
        matched_rule_keys: entry.matched_rule_keys.clone(),
        refresh_snapshot: entry.refresh_snapshot,
        block_reason: entry.block_reason.clone(),
        completion,
        diagnostic_codes: entry
            .diagnostics
            .iter()
            .map(|item| item.code.clone())
            .collect::<Vec<_>>(),
        diagnostics,
        injections,
    };

    let payload = HookTracePayload {
        event_type: Some(format!(
            "hook:{}:{}",
            hook_trigger_key(&entry.trigger),
            normalize_event_decision(&entry.decision)
        )),
        message: Some(describe_hook_trace(entry)),
        data: Some(data),
    };

    BackboneEnvelope::new(
        BackboneEvent::Platform(PlatformEvent::HookTrace(payload)),
        session_id,
        source,
    )
    .with_trace(TraceInfo {
        turn_id: turn_id.map(ToString::to_string),
        entry_index: None,
    })
}

fn hook_event_severity(entry: &HookTraceEntry) -> HookTraceSeverity {
    if entry.block_reason.is_some() || matches!(entry.decision.as_str(), "deny" | "blocked") {
        return HookTraceSeverity::Error;
    }
    if matches!(
        entry.decision.as_str(),
        "ask" | "rewrite" | "steering_injected" | "continue"
    ) {
        return HookTraceSeverity::Warning;
    }
    if matches!(entry.decision.as_str(), "step_advanced") {
        return HookTraceSeverity::Success;
    }
    HookTraceSeverity::Info
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
        HookTrigger::CapabilityChanged => "capability_changed",
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

    fn sample_source() -> SourceInfo {
        SourceInfo {
            connector_id: "pi-agent".to_string(),
            connector_type: "local_executor".to_string(),
            executor_id: Some("PI_AGENT".to_string()),
        }
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
            let envelope = build_hook_trace_envelope(
                "sess-1",
                Some("t-1"),
                sample_source(),
                &silent_entry(decision),
            );
            assert!(
                matches!(
                    envelope.event,
                    BackboneEvent::Platform(PlatformEvent::HookTrace(_))
                ),
                "should produce HookTrace event for decision: {decision}"
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
        let envelope = build_hook_trace_envelope("sess-1", Some("t-1"), sample_source(), &entry);
        assert!(matches!(
            envelope.event,
            BackboneEvent::Platform(PlatformEvent::HookTrace(_))
        ));
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

        let envelope = build_hook_trace_envelope("sess-1", Some("t-1"), sample_source(), &entry);
        match &envelope.event {
            BackboneEvent::Platform(PlatformEvent::HookTrace(payload)) => {
                assert!(payload.event_type.as_deref().unwrap().starts_with("hook:"));
                let data = payload.data.as_ref().unwrap();
                assert_eq!(data.decision, "continue");
                assert_eq!(data.severity, HookTraceSeverity::Warning);
            }
            other => panic!("unexpected event: {other:?}"),
        }
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
        let envelope = build_hook_trace_envelope("sess-1", Some("t-1"), sample_source(), &entry);
        match &envelope.event {
            BackboneEvent::Platform(PlatformEvent::HookTrace(payload)) => {
                let data = payload.data.as_ref().unwrap();
                assert_eq!(data.injections.len(), 1);
                assert_eq!(
                    data.injections[0].slot.as_deref(),
                    Some("companion_agents")
                );
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }
}
