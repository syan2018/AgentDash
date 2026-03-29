use std::sync::Arc;

use agentdash_spi::lifecycle::{
    AfterToolCallEffects, AfterToolCallInput, AfterTurnInput, AgentMessage, AgentRuntimeDelegate,
    AgentRuntimeError, BeforeStopInput, BeforeToolCallInput, DynAgentRuntimeDelegate, StopDecision,
    ToolCallDecision, TransformContextInput, TransformContextOutput, TurnControlDecision,
};
use async_trait::async_trait;
use tokio_util::sync::CancellationToken;

use agentdash_spi::hooks::{
    HookConstraint, HookContextFragment, HookDiagnosticEntry, HookEvaluationQuery,
    HookPendingAction, HookPendingActionStatus, HookSessionRuntimeSnapshot, HookTraceEntry,
    HookTrigger, SessionHookRefreshQuery, SharedHookSessionRuntime,
};

pub struct HookRuntimeDelegate {
    hook_session: SharedHookSessionRuntime,
}

impl HookRuntimeDelegate {
    #[allow(clippy::new_ret_no_self)]
    pub fn new(hook_session: SharedHookSessionRuntime) -> DynAgentRuntimeDelegate {
        Arc::new(Self { hook_session })
    }

    async fn evaluate(
        &self,
        trigger: HookTrigger,
        tool_name: Option<String>,
        tool_call_id: Option<String>,
        subagent_type: Option<String>,
        payload: Option<serde_json::Value>,
    ) -> Result<EvaluatedResolution, AgentRuntimeError> {
        let snapshot = self.hook_session.snapshot();
        let resolution = self
            .hook_session
            .evaluate(HookEvaluationQuery {
                session_id: self.hook_session.session_id().to_string(),
                trigger: trigger.clone(),
                turn_id: None,
                tool_name,
                tool_call_id,
                subagent_type,
                snapshot: Some(snapshot.clone()),
                payload,
            })
            .await
            .map_err(map_runtime_error)?;

        if resolution.refresh_snapshot {
            self.hook_session
                .refresh(SessionHookRefreshQuery {
                    session_id: self.hook_session.session_id().to_string(),
                    turn_id: None,
                    reason: Some(format!("trigger:{:?}", trigger)),
                })
                .await
                .map_err(map_runtime_error)?;
        }

        Ok(EvaluatedResolution {
            snapshot: self.hook_session.snapshot(),
            resolution,
            runtime: self.hook_session.runtime_snapshot(),
        })
    }

    fn record_trace(
        &self,
        trigger: HookTrigger,
        decision: impl Into<String>,
        tool_name: Option<String>,
        tool_call_id: Option<String>,
        subagent_type: Option<String>,
        evaluated: &EvaluatedResolution,
    ) {
        let trace = HookTraceEntry {
            sequence: self.hook_session.next_trace_sequence(),
            timestamp_ms: chrono::Utc::now().timestamp_millis(),
            revision: evaluated.runtime.revision,
            trigger,
            decision: decision.into(),
            tool_name,
            tool_call_id,
            subagent_type,
            matched_rule_keys: evaluated.resolution.matched_rule_keys.clone(),
            refresh_snapshot: evaluated.resolution.refresh_snapshot,
            block_reason: evaluated.resolution.block_reason.clone(),
            completion: evaluated.resolution.completion.clone(),
            diagnostics: evaluated.resolution.diagnostics.clone(),
        };
        self.hook_session.append_trace(trace);
    }
}

#[async_trait]
impl AgentRuntimeDelegate for HookRuntimeDelegate {
    async fn transform_context(
        &self,
        input: TransformContextInput,
        _cancel: CancellationToken,
    ) -> Result<TransformContextOutput, AgentRuntimeError> {
        let evaluated = self
            .evaluate(
                HookTrigger::UserPromptSubmit,
                None,
                None,
                None,
                Some(serde_json::json!({
                    "message_count": input.context.messages.len(),
                })),
            )
            .await?;
        let pending_messages = collect_pending_hook_messages(
            self.hook_session.as_ref(),
            &evaluated.snapshot,
            &self.hook_session.runtime_snapshot(),
        );
        self.record_trace(
            HookTrigger::UserPromptSubmit,
            if evaluated.resolution.context_fragments.is_empty()
                && evaluated.resolution.constraints.is_empty()
                && pending_messages.consumed == 0
            {
                "noop"
            } else {
                "context_injected"
            },
            None,
            None,
            None,
            &evaluated,
        );
        let mut messages = input.context.messages;
        if let Some(message) = build_hook_injection_message(
            &evaluated.snapshot,
            &evaluated.resolution,
            &evaluated.runtime,
        ) {
            messages.push(message);
        }
        messages.extend(pending_messages.steering);
        messages.extend(pending_messages.follow_up);
        Ok(TransformContextOutput { messages })
    }

    async fn before_tool_call(
        &self,
        input: BeforeToolCallInput,
        _cancel: CancellationToken,
    ) -> Result<ToolCallDecision, AgentRuntimeError> {
        let tool_name = input.tool_call.name.clone();
        let tool_call_id = input.tool_call.id.clone();
        let evaluated = self
            .evaluate(
                HookTrigger::BeforeTool,
                Some(tool_name.clone()),
                Some(tool_call_id.clone()),
                None,
                Some(serde_json::json!({
                    "args": input.args,
                })),
            )
            .await?;

        if let Some(reason) = evaluated.resolution.block_reason.clone() {
            self.record_trace(
                HookTrigger::BeforeTool,
                "deny",
                Some(tool_name),
                Some(tool_call_id),
                None,
                &evaluated,
            );
            return Ok(ToolCallDecision::Deny { reason });
        }
        if let Some(approval_request) = evaluated.resolution.approval_request.clone() {
            self.record_trace(
                HookTrigger::BeforeTool,
                "ask",
                Some(tool_name),
                Some(tool_call_id),
                None,
                &evaluated,
            );
            return Ok(ToolCallDecision::Ask {
                reason: approval_request.reason,
                args: evaluated.resolution.rewritten_tool_input.clone(),
                details: approval_request.details,
            });
        }
        if let Some(args) = evaluated.resolution.rewritten_tool_input.clone() {
            self.record_trace(
                HookTrigger::BeforeTool,
                "rewrite",
                Some(tool_name),
                Some(tool_call_id),
                None,
                &evaluated,
            );
            return Ok(ToolCallDecision::Rewrite { args, note: None });
        }
        self.record_trace(
            HookTrigger::BeforeTool,
            "allow",
            Some(tool_name),
            Some(tool_call_id),
            None,
            &evaluated,
        );
        Ok(ToolCallDecision::Allow)
    }

    async fn after_tool_call(
        &self,
        input: AfterToolCallInput,
        _cancel: CancellationToken,
    ) -> Result<AfterToolCallEffects, AgentRuntimeError> {
        let tool_name = input.tool_call.name.clone();
        let tool_call_id = input.tool_call.id.clone();
        let evaluated = self
            .evaluate(
                HookTrigger::AfterTool,
                Some(tool_name.clone()),
                Some(tool_call_id.clone()),
                None,
                Some(serde_json::json!({
                    "args": input.args,
                    "result": input.result,
                    "is_error": input.is_error,
                })),
            )
            .await?;
        self.record_trace(
            HookTrigger::AfterTool,
            if evaluated.resolution.refresh_snapshot {
                "refresh_requested"
            } else {
                "effects_applied"
            },
            Some(tool_name),
            Some(tool_call_id),
            None,
            &evaluated,
        );

        Ok(AfterToolCallEffects {
            refresh_snapshot: evaluated.resolution.refresh_snapshot,
            diagnostics: evaluated
                .resolution
                .diagnostics
                .into_iter()
                .map(|entry| entry.summary)
                .collect(),
            ..AfterToolCallEffects::default()
        })
    }

    async fn after_turn(
        &self,
        input: AfterTurnInput,
        _cancel: CancellationToken,
    ) -> Result<TurnControlDecision, AgentRuntimeError> {
        let evaluated = self
            .evaluate(
                HookTrigger::AfterTurn,
                None,
                None,
                None,
                Some(serde_json::json!({
                    "assistant_message": input.message,
                    "tool_results": input.tool_results,
                })),
            )
            .await?;
        let mut steering = build_hook_steering_messages(
            &evaluated.snapshot,
            &evaluated.resolution.context_fragments,
            &evaluated.resolution.constraints,
            &evaluated.runtime,
        );
        let pending_messages = collect_pending_hook_messages(
            self.hook_session.as_ref(),
            &evaluated.snapshot,
            &self.hook_session.runtime_snapshot(),
        );
        let has_runtime_output = !evaluated.resolution.context_fragments.is_empty()
            || !evaluated.resolution.constraints.is_empty()
            || pending_messages.consumed > 0;
        self.record_trace(
            HookTrigger::AfterTurn,
            if has_runtime_output {
                "steering_injected"
            } else {
                "noop"
            },
            None,
            None,
            None,
            &evaluated,
        );
        steering.extend(pending_messages.steering);

        Ok(TurnControlDecision {
            steering,
            follow_up: pending_messages.follow_up,
            refresh_snapshot: evaluated.resolution.refresh_snapshot,
            diagnostics: evaluated
                .resolution
                .diagnostics
                .into_iter()
                .map(|entry| entry.summary)
                .collect(),
        })
    }

    async fn before_stop(
        &self,
        input: BeforeStopInput,
        _cancel: CancellationToken,
    ) -> Result<StopDecision, AgentRuntimeError> {
        let mut evaluated = self
            .evaluate(
                HookTrigger::BeforeStop,
                None,
                None,
                None,
                Some(serde_json::json!({
                    "message_count": input.context.messages.len(),
                })),
            )
            .await?;

        let mut steering = build_hook_steering_messages(
            &evaluated.snapshot,
            &evaluated.resolution.context_fragments,
            &evaluated.resolution.constraints,
            &evaluated.runtime,
        );
        let pending_messages = collect_pending_hook_messages(
            self.hook_session.as_ref(),
            &evaluated.snapshot,
            &self.hook_session.runtime_snapshot(),
        );
        let unresolved_blocking_actions = self.hook_session.unresolved_blocking_actions();
        if !unresolved_blocking_actions.is_empty() {
            let unresolved_ids = unresolved_blocking_actions
                .iter()
                .map(|action| action.id.clone())
                .collect::<Vec<_>>();
            evaluated
                .resolution
                .matched_rule_keys
                .push("runtime_pending_action:blocking_review:stop_gate".to_string());
            evaluated.resolution.diagnostics.push(HookDiagnosticEntry {
                code: "pending_action_blocking_review_unresolved".to_string(),
                summary: "当前仍有未结案的 blocking_review hook action，不能自然结束。".to_string(),
                detail: Some(format!("action_ids={}", unresolved_ids.join(","))),
                source_summary: vec!["session_runtime:pending_actions".to_string()],
                source_refs: Vec::new(),
            });
        }
        steering.extend(pending_messages.steering.clone());
        if pending_messages.consumed == 0 && !unresolved_blocking_actions.is_empty() {
            steering.extend(build_blocking_action_reminders(
                &evaluated.snapshot,
                &unresolved_blocking_actions,
                &self.hook_session.runtime_snapshot(),
            ));
        }
        let completion_satisfied = evaluated
            .resolution
            .completion
            .as_ref()
            .is_some_and(|completion| completion.satisfied);
        let has_completion_gate = evaluated.resolution.completion.is_some();
        let blocking_review_pending = !unresolved_blocking_actions.is_empty();

        if steering.is_empty()
            && pending_messages.follow_up.is_empty()
            && !blocking_review_pending
            && (!has_completion_gate || completion_satisfied)
        {
            self.record_trace(
                HookTrigger::BeforeStop,
                "stop",
                None,
                None,
                None,
                &evaluated,
            );
            return Ok(StopDecision::Stop);
        }

        self.record_trace(
            HookTrigger::BeforeStop,
            "continue",
            None,
            None,
            None,
            &evaluated,
        );
        Ok(StopDecision::Continue {
            steering,
            follow_up: pending_messages.follow_up,
            reason: Some(if blocking_review_pending {
                "hook runtime 仍有未结案的 blocking_review 事项，需先处理并调用 resolve_hook_action 明确结案。"
                    .to_string()
            } else if pending_messages.consumed > 0 {
                "hook runtime 收到待处理的 companion/hook 回流，继续 loop".to_string()
            } else if completion_satisfied {
                "hook runtime 尚有额外约束待处理，继续 loop".to_string()
            } else {
                "hook runtime 尚未满足 stop gate，继续 loop".to_string()
            }),
        })
    }
}

struct EvaluatedResolution {
    snapshot: agentdash_spi::hooks::SessionHookSnapshot,
    resolution: agentdash_spi::hooks::HookResolution,
    runtime: HookSessionRuntimeSnapshot,
}

#[derive(Default)]
struct PendingHookMessages {
    steering: Vec<AgentMessage>,
    follow_up: Vec<AgentMessage>,
    consumed: usize,
}

fn collect_pending_hook_messages(
    hook_session: &dyn agentdash_spi::hooks::HookSessionRuntimeAccess,
    snapshot: &agentdash_spi::hooks::SessionHookSnapshot,
    runtime: &HookSessionRuntimeSnapshot,
) -> PendingHookMessages {
    let actions = hook_session.collect_pending_actions_for_injection();
    let consumed = actions.len();
    if consumed == 0 {
        return PendingHookMessages::default();
    }

    let mut messages = PendingHookMessages {
        consumed,
        ..PendingHookMessages::default()
    };
    for action in actions {
        let Some(message) = build_pending_action_message(snapshot, &action, runtime) else {
            continue;
        };
        if action.action_type == "follow_up_required" {
            messages.follow_up.push(message);
        } else {
            messages.steering.push(message);
        }
    }
    messages
}

fn build_blocking_action_reminders(
    snapshot: &agentdash_spi::hooks::SessionHookSnapshot,
    actions: &[HookPendingAction],
    runtime: &HookSessionRuntimeSnapshot,
) -> Vec<AgentMessage> {
    actions
        .iter()
        .filter_map(|action| build_pending_action_message(snapshot, action, runtime))
        .collect()
}

fn build_hook_injection_message(
    snapshot: &agentdash_spi::hooks::SessionHookSnapshot,
    resolution: &agentdash_spi::hooks::HookResolution,
    runtime: &HookSessionRuntimeSnapshot,
) -> Option<AgentMessage> {
    let content = build_hook_markdown(
        snapshot,
        &resolution.context_fragments,
        &resolution.constraints,
        runtime,
    )?;
    Some(AgentMessage::user(content))
}

fn build_hook_steering_messages(
    snapshot: &agentdash_spi::hooks::SessionHookSnapshot,
    fragments: &[HookContextFragment],
    constraints: &[HookConstraint],
    runtime: &HookSessionRuntimeSnapshot,
) -> Vec<AgentMessage> {
    build_hook_markdown(snapshot, fragments, constraints, runtime)
        .map(|content| vec![AgentMessage::user(content)])
        .unwrap_or_default()
}

fn build_hook_markdown(
    snapshot: &agentdash_spi::hooks::SessionHookSnapshot,
    fragments: &[HookContextFragment],
    constraints: &[HookConstraint],
    runtime: &HookSessionRuntimeSnapshot,
) -> Option<String> {
    if fragments.is_empty() && constraints.is_empty() {
        return None;
    }

    let mut sections = Vec::new();

    sections.push(format!(
        "[系统动态 Hook 上下文]\n当前 session_id={}，revision={}",
        snapshot.session_id, runtime.revision
    ));

    if !snapshot.owners.is_empty() {
        sections.push(format!(
            "## 归属对象\n{}",
            snapshot
                .owners
                .iter()
                .map(|owner| format!(
                    "- {}: {} {}",
                    owner.owner_type,
                    owner.label.as_deref().unwrap_or("??"),
                    owner.owner_id
                ))
                .collect::<Vec<_>>()
                .join("\n")
        ));
    }

    append_hook_markdown_body(&mut sections, fragments, constraints);

    sections
        .push("以上内容由 Hook Runtime 自动注入，不代表用户新增需求，但必须优先遵守。".to_string());

    Some(sections.join("\n\n"))
}

fn build_pending_action_message(
    snapshot: &agentdash_spi::hooks::SessionHookSnapshot,
    action: &HookPendingAction,
    runtime: &HookSessionRuntimeSnapshot,
) -> Option<AgentMessage> {
    if action.summary.trim().is_empty()
        && action.context_fragments.is_empty()
        && action.constraints.is_empty()
    {
        return None;
    }

    let mut sections = vec![format!(
        "[待处理 Hook 事项]\n{}（type={}，status={}，revision={}）",
        action.title,
        action.action_type,
        pending_action_status_label(action.status),
        runtime.revision
    )];
    sections.push(format!("事项 id: {}", action.id));
    if !action.summary.trim().is_empty() {
        sections.push(action.summary.trim().to_string());
    }
    if let Some(turn_id) = action.turn_id.as_deref() {
        sections.push(format!("关联 turn: {turn_id}"));
    }
    sections.push(match action.action_type.as_str() {
        "blocking_review" => {
            "当前事项是阻塞式 review。不要复述前文；直接处理剩余动作，并在完成后调用 `resolve_hook_action` 明确结案。"
                .to_string()
        }
        "follow_up_required" => {
            "当前事项要求继续跟进。不要停在总结；请直接落实后续动作，并在完成后调用 `resolve_hook_action` 明确结案。"
                .to_string()
        }
        _ => {
            "请直接处理这项 Hook 待办，并在完成后调用 `resolve_hook_action` 明确结案。"
                .to_string()
        }
    });
    if !snapshot.owners.is_empty() {
        sections.push(format!(
            "## 归属对象\n{}",
            snapshot
                .owners
                .iter()
                .map(|owner| format!(
                    "- {}: {} {}",
                    owner.owner_type,
                    owner.label.as_deref().unwrap_or("??"),
                    owner.owner_id
                ))
                .collect::<Vec<_>>()
                .join("\n")
        ));
    }
    if !action.context_fragments.is_empty() {
        sections.push(format!(
            "已挂载上下文片段：{}",
            action
                .context_fragments
                .iter()
                .map(|fragment| fragment.label.as_str())
                .collect::<Vec<_>>()
                .join("，")
        ));
    }
    if !action.constraints.is_empty() {
        sections.push(format!(
            "必须完成的约束：\n{}",
            action
                .constraints
                .iter()
                .map(|constraint| format!("- {}", constraint.description))
                .collect::<Vec<_>>()
                .join("\n")
        ));
    }
    sections.push(
        "以上事项来自 Hook Runtime 的待处理回流，优先级高于普通自然对话推进。处理时尽量避免重复总结，聚焦完成剩余动作。".to_string(),
    );

    Some(AgentMessage::user(sections.join("\n\n")))
}

fn append_hook_markdown_body(
    sections: &mut Vec<String>,
    fragments: &[HookContextFragment],
    constraints: &[HookConstraint],
) {
    let mut fragment_lines = Vec::new();
    if !fragments.is_empty() {
        fragment_lines.push("## 动态注入上下文".to_string());
        for fragment in fragments {
            fragment_lines.push(format!("### {}", fragment.label));
            fragment_lines.push(fragment.content.clone());
            fragment_lines.push(String::new());
        }
    }
    if !fragment_lines.is_empty() {
        while fragment_lines.last().is_some_and(|line| line.is_empty()) {
            fragment_lines.pop();
        }
        sections.push(fragment_lines.join("\n"));
    }

    if !constraints.is_empty() {
        sections.push(format!(
            "## 必须遵守的流程约束\n{}",
            constraints
                .iter()
                .map(|constraint| format!("- {}", constraint.description))
                .collect::<Vec<_>>()
                .join("\n")
        ));
    }
}

fn map_runtime_error(error: agentdash_spi::hooks::HookError) -> AgentRuntimeError {
    AgentRuntimeError::Runtime(error.to_string())
}

fn pending_action_status_label(status: HookPendingActionStatus) -> &'static str {
    match status {
        HookPendingActionStatus::Pending => "pending",
        HookPendingActionStatus::Injected => "injected",
        HookPendingActionStatus::Resolved => "resolved",
        HookPendingActionStatus::Dismissed => "dismissed",
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use agentdash_spi::lifecycle::{AgentContext, StopDecision};
    use async_trait::async_trait;
    use tokio_util::sync::CancellationToken;

    use super::HookRuntimeDelegate;
    use crate::session::HookSessionRuntime;
    use agentdash_spi::hooks::{
        ExecutionHookProvider, HookCompletionStatus, HookContextFragment, HookError,
        HookEvaluationQuery, HookPendingAction, HookPendingActionResolutionKind, HookResolution,
        HookSessionRuntimeAccess, HookTrigger, NoopExecutionHookProvider, SessionHookRefreshQuery,
        SessionHookSnapshot, SessionHookSnapshotQuery,
    };

    #[derive(Clone)]
    struct CompletionSatisfiedProvider;

    #[async_trait]
    impl ExecutionHookProvider for CompletionSatisfiedProvider {
        async fn load_session_snapshot(
            &self,
            query: SessionHookSnapshotQuery,
        ) -> Result<SessionHookSnapshot, HookError> {
            Ok(SessionHookSnapshot {
                session_id: query.session_id,
                ..SessionHookSnapshot::default()
            })
        }

        async fn refresh_session_snapshot(
            &self,
            query: SessionHookRefreshQuery,
        ) -> Result<SessionHookSnapshot, HookError> {
            Ok(SessionHookSnapshot {
                session_id: query.session_id,
                ..SessionHookSnapshot::default()
            })
        }

        async fn evaluate_hook(
            &self,
            query: HookEvaluationQuery,
        ) -> Result<HookResolution, HookError> {
            Ok(HookResolution {
                completion: matches!(query.trigger, HookTrigger::BeforeStop).then_some(
                    HookCompletionStatus {
                        mode: "test_completion".to_string(),
                        satisfied: true,
                        advanced: false,
                        reason: "已满足基础 completion 条件".to_string(),
                    },
                ),
                ..HookResolution::default()
            })
        }
    }

    #[tokio::test]
    async fn before_stop_is_blocked_until_blocking_review_action_is_resolved() {
        let hook_session = Arc::new(HookSessionRuntime::new(
            "sess-hook".to_string(),
            Arc::new(CompletionSatisfiedProvider),
            SessionHookSnapshot {
                session_id: "sess-hook".to_string(),
                ..SessionHookSnapshot::default()
            },
        ));
        hook_session.enqueue_pending_action(HookPendingAction {
            id: "blocking-1".to_string(),
            created_at_ms: 1_710_000_000_000,
            title: "Companion review 需要处理".to_string(),
            summary: "请先确认是否采纳 review 结论".to_string(),
            action_type: "blocking_review".to_string(),
            turn_id: Some("turn-parent-1".to_string()),
            source_trigger: HookTrigger::SubagentResult,
            status: agentdash_spi::hooks::HookPendingActionStatus::Pending,
            last_injected_at_ms: None,
            resolved_at_ms: None,
            resolution_kind: None,
            resolution_note: None,
            resolution_turn_id: None,
            context_fragments: vec![HookContextFragment {
                slot: "workflow".to_string(),
                label: "subagent_blocking_review".to_string(),
                content: "必须先处理 review 结果。".to_string(),
                source_summary: vec![],
                source_refs: vec![],
            }],
            constraints: vec![],
        });
        let delegate = HookRuntimeDelegate::new(hook_session.clone());

        let first = delegate
            .before_stop(
                agentdash_spi::lifecycle::BeforeStopInput {
                    context: AgentContext {
                        system_prompt: "test".to_string(),
                        messages: vec![],
                        tools: vec![],
                    },
                },
                CancellationToken::new(),
            )
            .await
            .expect("before_stop 应返回 continue");

        match first {
            StopDecision::Continue { reason, .. } => {
                assert!(
                    reason
                        .as_deref()
                        .is_some_and(|value| value.contains("blocking_review"))
                );
            }
            StopDecision::Stop => panic!("存在 blocking_review action 时不应允许 stop"),
        }

        let action = hook_session
            .resolve_pending_action(
                "blocking-1",
                HookPendingActionResolutionKind::Adopted,
                Some("主 session 已吸收 review 结果".to_string()),
                Some("turn-parent-1".to_string()),
            )
            .expect("应该能 resolve blocking action");
        assert!(matches!(
            action.status,
            agentdash_spi::hooks::HookPendingActionStatus::Resolved
        ));

        let second = delegate
            .before_stop(
                agentdash_spi::lifecycle::BeforeStopInput {
                    context: AgentContext {
                        system_prompt: "test".to_string(),
                        messages: vec![],
                        tools: vec![],
                    },
                },
                CancellationToken::new(),
            )
            .await
            .expect("resolve 后应允许 stop");

        assert!(matches!(second, StopDecision::Stop));
    }

    #[test]
    fn pending_action_message_requires_explicit_resolution_tool() {
        let snapshot = SessionHookSnapshot {
            session_id: "sess-hook".to_string(),
            ..SessionHookSnapshot::default()
        };
        let runtime = HookSessionRuntime::new(
            "sess-hook".to_string(),
            Arc::new(NoopExecutionHookProvider),
            snapshot.clone(),
        )
        .runtime_snapshot();
        let action = HookPendingAction {
            id: "follow-up-1".to_string(),
            created_at_ms: 1_710_000_000_000,
            title: "需要继续跟进".to_string(),
            summary: "补充 follow-up".to_string(),
            action_type: "follow_up_required".to_string(),
            turn_id: Some("turn-1".to_string()),
            source_trigger: HookTrigger::SubagentResult,
            status: agentdash_spi::hooks::HookPendingActionStatus::Injected,
            last_injected_at_ms: Some(1_710_000_100_000),
            resolved_at_ms: None,
            resolution_kind: None,
            resolution_note: None,
            resolution_turn_id: None,
            context_fragments: vec![HookContextFragment {
                slot: "workflow".to_string(),
                label: "follow_up".to_string(),
                content: "继续落实下一步".to_string(),
                source_summary: vec![],
                source_refs: vec![],
            }],
            constraints: vec![],
        };

        let message = super::build_pending_action_message(&snapshot, &action, &runtime)
            .expect("应该生成 pending action 消息");
        let text = match message {
            agentdash_spi::lifecycle::AgentMessage::User { content, .. } => content
                .iter()
                .filter_map(|part| part.extract_text())
                .collect::<Vec<_>>()
                .join("\n"),
            other => panic!("期望 User 消息，实际为 {other:?}"),
        };

        assert!(text.contains("resolve_hook_action"));
        assert!(text.contains("follow_up_required"));
        assert!(text.contains("status=injected"));
    }
}
