use std::sync::Arc;

use agentdash_spi::{
    AfterToolCallEffects, AfterToolCallInput, AfterTurnInput, AgentMessage, AgentRuntimeDelegate,
    AgentRuntimeError, BeforeProviderRequestInput, BeforeStopInput, BeforeToolCallInput,
    CompactionParams, CompactionResult, CompactionTriggerStats, DynAgentRuntimeDelegate,
    EvaluateCompactionInput, StopDecision, StopReason, ToolCallDecision, TransformContextInput,
    TransformContextOutput, TurnControlDecision,
};
use async_trait::async_trait;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use super::hook_messages as msg;

use crate::context::{AuditTrigger, SharedContextAuditBus, emit_fragment};
use crate::hooks::hook_injection_to_fragment;

use agentdash_spi::hooks::{
    ContextTokenStats, HookDiagnosticEntry, HookEvaluationQuery, HookInjection, HookPendingAction,
    HookPendingActionStatus, HookSessionRuntimeSnapshot, HookTraceEntry, HookTrigger,
    SessionHookRefreshQuery, SharedHookSessionRuntime,
};

pub struct HookRuntimeDelegate {
    hook_session: SharedHookSessionRuntime,
    default_mount_root_ref: Option<String>,
    audit_bus: Option<SharedContextAuditBus>,
}

impl HookRuntimeDelegate {
    #[allow(clippy::new_ret_no_self)]
    pub fn new(hook_session: SharedHookSessionRuntime) -> DynAgentRuntimeDelegate {
        Self::new_with_mount_root(hook_session, None)
    }

    #[allow(clippy::new_ret_no_self)]
    pub fn new_with_mount_root(
        hook_session: SharedHookSessionRuntime,
        default_mount_root_ref: Option<String>,
    ) -> DynAgentRuntimeDelegate {
        Self::new_with_mount_root_and_audit(hook_session, default_mount_root_ref, None)
    }

    #[allow(clippy::new_ret_no_self)]
    pub fn new_with_mount_root_and_audit(
        hook_session: SharedHookSessionRuntime,
        default_mount_root_ref: Option<String>,
        audit_bus: Option<SharedContextAuditBus>,
    ) -> DynAgentRuntimeDelegate {
        Arc::new(Self {
            hook_session,
            default_mount_root_ref,
            audit_bus,
        })
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
                token_stats: None,
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

    fn emit_hook_injection_fragments(&self, trigger: HookTrigger, injections: &[HookInjection]) {
        let Some(bus) = self.audit_bus.as_ref() else {
            return;
        };
        if injections.is_empty() {
            return;
        }

        let bundle_id = Uuid::new_v4();
        let bundle_session_uuid = Uuid::new_v4();
        let trigger_label = format!("{trigger:?}");
        for injection in injections.iter().cloned() {
            let fragment = hook_injection_to_fragment(injection);
            emit_fragment(
                bus.as_ref(),
                bundle_id,
                self.hook_session.session_id(),
                bundle_session_uuid,
                AuditTrigger::HookInjection {
                    trigger: trigger_label.clone(),
                },
                &fragment,
            );
        }
    }

    /// 从消息中提取最新的 LLM usage 并更新 session runtime 的 token stats
    fn update_token_stats_from_messages(&self, messages: &[AgentMessage]) {
        let last_usage = messages.iter().rev().find_map(|m| match m {
            AgentMessage::Assistant {
                usage: Some(usage),
                stop_reason,
                ..
            } if !matches!(stop_reason, Some(StopReason::Error | StopReason::Aborted)) => {
                Some(usage.clone())
            }
            _ => None,
        });

        if let Some(usage) = last_usage {
            let snapshot = self.hook_session.snapshot();
            let context_window = snapshot
                .metadata
                .as_ref()
                .and_then(|m| m.extra.get("model_context_window"))
                .and_then(|v| v.as_u64())
                .unwrap_or(0);

            self.hook_session.update_token_stats(ContextTokenStats {
                last_input_tokens: usage.input,
                context_window,
            });
        }
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
            injections: evaluated.resolution.injections.clone(),
        };
        self.hook_session.append_trace(trace);
    }
}

#[async_trait]
impl AgentRuntimeDelegate for HookRuntimeDelegate {
    async fn evaluate_compaction(
        &self,
        input: EvaluateCompactionInput,
        _cancel: CancellationToken,
    ) -> Result<Option<CompactionParams>, AgentRuntimeError> {
        self.update_token_stats_from_messages(&input.context.messages);

        let last_usage = self.hook_session.token_stats();
        let default_keep_last_n = 20_u32;
        let default_reserve_tokens = 16_384_u64;
        let evaluated = self
            .evaluate(
                HookTrigger::BeforeCompact,
                None,
                None,
                None,
                Some(serde_json::json!({
                    "message_count": input.context.messages.len(),
                    "has_existing_summary": input.context.messages.iter().any(|m| matches!(m, AgentMessage::CompactionSummary { .. })),
                    "default_decision": {
                        "reserve_tokens": default_reserve_tokens,
                        "keep_last_n": default_keep_last_n,
                    },
                })),
            )
            .await?;

        let snapshot = self.hook_session.snapshot();
        let context_window = snapshot
            .metadata
            .as_ref()
            .and_then(|m| m.extra.get("model_context_window"))
            .and_then(|v| v.as_u64())
            .unwrap_or(last_usage.context_window);

        let decision = match evaluated.resolution.compaction.as_ref() {
            Some(compaction) if compaction.cancel => {
                self.record_trace(
                    HookTrigger::BeforeCompact,
                    "cancel",
                    None,
                    None,
                    None,
                    &evaluated,
                );
                None
            }
            Some(compaction) => {
                self.record_trace(
                    HookTrigger::BeforeCompact,
                    "compact",
                    None,
                    None,
                    None,
                    &evaluated,
                );
                Some(CompactionParams {
                    keep_last_n: compaction.keep_last_n.unwrap_or(default_keep_last_n),
                    reserve_tokens: compaction.reserve_tokens.unwrap_or(default_reserve_tokens),
                    custom_summary: compaction.custom_summary.clone(),
                    custom_prompt: compaction.custom_prompt.clone(),
                    trigger_stats: CompactionTriggerStats {
                        input_tokens: last_usage.last_input_tokens,
                        context_window,
                        reserve_tokens: compaction.reserve_tokens.unwrap_or(default_reserve_tokens),
                    },
                })
            }
            None => {
                self.record_trace(
                    HookTrigger::BeforeCompact,
                    "noop",
                    None,
                    None,
                    None,
                    &evaluated,
                );
                None
            }
        };

        Ok(decision)
    }

    async fn after_compaction(
        &self,
        result: CompactionResult,
        _cancel: CancellationToken,
    ) -> Result<(), AgentRuntimeError> {
        let summary_length = match &result.summary_message {
            AgentMessage::CompactionSummary { summary, .. } => summary.chars().count(),
            _ => 0,
        };
        let total_compacted_messages = match &result.summary_message {
            AgentMessage::CompactionSummary {
                messages_compacted, ..
            } => *messages_compacted,
            _ => 0,
        };

        let evaluated = self
            .evaluate(
                HookTrigger::AfterCompact,
                None,
                None,
                None,
                Some(serde_json::json!({
                    "tokens_before": result.trigger_stats.input_tokens,
                    "messages_compacted": result.newly_compacted_messages,
                    "messages_compacted_total": total_compacted_messages,
                    "summary_length": summary_length,
                    "used_custom_summary": result.used_custom_summary,
                })),
            )
            .await?;

        self.record_trace(
            HookTrigger::AfterCompact,
            "notified",
            None,
            None,
            None,
            &evaluated,
        );

        Ok(())
    }

    async fn transform_context(
        &self,
        input: TransformContextInput,
        _cancel: CancellationToken,
    ) -> Result<TransformContextOutput, AgentRuntimeError> {
        // 1. 提取最新 token 使用数据并更新 session runtime
        self.update_token_stats_from_messages(&input.context.messages);

        // 2. 评估 UserPromptSubmit hook
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
        self.emit_hook_injection_fragments(
            HookTrigger::UserPromptSubmit,
            &evaluated.resolution.injections,
        );

        // 2a. block_reason — hook 要求阻止当前用户输入
        if let Some(reason) = evaluated.resolution.block_reason.clone() {
            self.record_trace(
                HookTrigger::UserPromptSubmit,
                "blocked",
                None,
                None,
                None,
                &evaluated,
            );
            return Ok(TransformContextOutput {
                messages: vec![],
                blocked: Some(reason),
            });
        }

        let pending_messages = collect_pending_hook_messages(
            self.hook_session.as_ref(),
            &evaluated.snapshot,
            &self.hook_session.runtime_snapshot(),
        );
        if should_trace_user_prompt_context_injection(
            &evaluated.runtime,
            &evaluated.resolution.injections,
            pending_messages.consumed,
        ) {
            self.record_trace(
                HookTrigger::UserPromptSubmit,
                "context_injected",
                None,
                None,
                None,
                &evaluated,
            );
        } else if evaluated.resolution.injections.is_empty() && pending_messages.consumed == 0 {
            self.record_trace(
                HookTrigger::UserPromptSubmit,
                "noop",
                None,
                None,
                None,
                &evaluated,
            );
        }

        // 3. 构建消息列表
        let mut messages = input.context.messages;

        // 3a. transformed_message — hook 改写了用户原始输入
        if let Some(ref new_text) = evaluated.resolution.transformed_message {
            if let Some(last_user) = messages.iter_mut().rev().find(|m| m.is_user()) {
                last_user.replace_user_text(new_text);
            }
        }

        if let Some(message) = build_hook_injection_message(
            &evaluated.snapshot,
            &evaluated.resolution,
            &evaluated.runtime,
        ) {
            messages.push(message);
        }
        messages.extend(pending_messages.steering);
        messages.extend(pending_messages.follow_up);

        Ok(TransformContextOutput {
            messages,
            blocked: None,
        })
    }

    async fn before_tool_call(
        &self,
        input: BeforeToolCallInput,
        _cancel: CancellationToken,
    ) -> Result<ToolCallDecision, AgentRuntimeError> {
        let tool_name = input.tool_call.name.clone();
        let tool_call_id = input.tool_call.id.clone();
        let mut payload = serde_json::json!({
            "args": input.args,
        });
        if let Some(default_mount_root_ref) = self.default_mount_root_ref.as_deref() {
            payload["default_mount_root_ref"] =
                serde_json::Value::String(default_mount_root_ref.to_string());
        }
        let evaluated = self
            .evaluate(
                HookTrigger::BeforeTool,
                Some(tool_name.clone()),
                Some(tool_call_id.clone()),
                None,
                Some(payload),
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
                .map(|entry| entry.message)
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
        self.emit_hook_injection_fragments(
            HookTrigger::AfterTurn,
            &evaluated.resolution.injections,
        );
        let mut steering = build_hook_steering_messages(
            &evaluated.snapshot,
            &evaluated.resolution.injections,
            &evaluated.runtime,
        );
        let pending_messages = collect_pending_hook_messages(
            self.hook_session.as_ref(),
            &evaluated.snapshot,
            &self.hook_session.runtime_snapshot(),
        );
        let has_runtime_output =
            !evaluated.resolution.injections.is_empty() || pending_messages.consumed > 0;
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
                .map(|entry| entry.message)
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
        self.emit_hook_injection_fragments(
            HookTrigger::BeforeStop,
            &evaluated.resolution.injections,
        );

        let mut steering = build_hook_steering_messages(
            &evaluated.snapshot,
            &evaluated.resolution.injections,
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
            evaluated.resolution.matched_rule_keys.push(format!(
                "runtime_pending_action:{}:stop_gate",
                agentdash_spi::hooks::action_type::BLOCKING_REVIEW
            ));
            evaluated.resolution.diagnostics.push(HookDiagnosticEntry {
                code: "pending_action_blocking_review_unresolved".to_string(),
                message: msg::diag_blocking_review_unresolved(&unresolved_ids.join(",")),
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

        let allow_empty_continue = steering.is_empty()
            && pending_messages.follow_up.is_empty()
            && !blocking_review_pending
            && has_completion_gate
            && !completion_satisfied;

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
                msg::REASON_BLOCKING_REVIEW_UNRESOLVED.to_string()
            } else if pending_messages.consumed > 0 {
                msg::REASON_PENDING_COMPANION_CONSUMED.to_string()
            } else if completion_satisfied {
                msg::REASON_EXTRA_CONSTRAINTS_PENDING.to_string()
            } else {
                msg::REASON_STOP_GATE_UNSATISFIED.to_string()
            }),
            allow_empty: allow_empty_continue,
        })
    }

    async fn on_before_provider_request(
        &self,
        input: BeforeProviderRequestInput,
        _cancel: CancellationToken,
    ) -> Result<(), AgentRuntimeError> {
        let evaluated = self
            .evaluate(
                HookTrigger::BeforeProviderRequest,
                None,
                None,
                None,
                Some(serde_json::json!({
                    "system_prompt_len": input.system_prompt_len,
                    "message_count": input.message_count,
                    "tool_count": input.tool_count,
                })),
            )
            .await?;
        self.record_trace(
            HookTrigger::BeforeProviderRequest,
            "observed",
            None,
            None,
            None,
            &evaluated,
        );
        Ok(())
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

fn should_trace_user_prompt_context_injection(
    runtime: &HookSessionRuntimeSnapshot,
    injections: &[HookInjection],
    pending_consumed: usize,
) -> bool {
    if pending_consumed > 0 {
        return true;
    }
    if injections.is_empty() {
        return false;
    }

    let previous = runtime
        .trace
        .iter()
        .rev()
        .find(|entry| matches!(entry.trigger, HookTrigger::UserPromptSubmit));

    match previous {
        Some(entry) => entry.injections != injections,
        None => true,
    }
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
        if action.is_follow_up() {
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
    // PR 4（04-30-session-pipeline-architecture-refactor）删除
    // `HOOK_USER_MESSAGE_SKIP_SLOTS` 的白名单去重逻辑。companion_agents 等
    // "静态上下文" slot 已经随 Bundle 进入 SP，user message 路径天然不会再
    // 承载同内容 —— 去重依靠"数据面单一来源"（Bundle），而非维护一张白名单。
    let content = build_hook_markdown(snapshot, &resolution.injections, runtime)?;
    Some(AgentMessage::user(content))
}

fn build_hook_steering_messages(
    snapshot: &agentdash_spi::hooks::SessionHookSnapshot,
    injections: &[HookInjection],
    runtime: &HookSessionRuntimeSnapshot,
) -> Vec<AgentMessage> {
    build_hook_markdown(snapshot, injections, runtime)
        .map(|content| vec![AgentMessage::user(content)])
        .unwrap_or_default()
}

fn build_hook_markdown(
    snapshot: &agentdash_spi::hooks::SessionHookSnapshot,
    injections: &[HookInjection],
    runtime: &HookSessionRuntimeSnapshot,
) -> Option<String> {
    if injections.is_empty() {
        return None;
    }

    let mut sections = Vec::new();

    sections.push(msg::hook_context_header(
        &snapshot.session_id,
        runtime.revision,
    ));

    if !snapshot.owners.is_empty() {
        sections.push(msg::owners_section(&format_owners(&snapshot.owners)));
    }

    append_hook_markdown_body(&mut sections, injections);

    sections.push(msg::HOOK_INJECTION_FOOTER.to_string());

    Some(sections.join("\n\n"))
}

fn build_pending_action_message(
    snapshot: &agentdash_spi::hooks::SessionHookSnapshot,
    action: &HookPendingAction,
    runtime: &HookSessionRuntimeSnapshot,
) -> Option<AgentMessage> {
    if action.summary.trim().is_empty() && action.injections.is_empty() {
        return None;
    }

    let mut sections = vec![msg::pending_action_header(
        &action.title,
        &action.action_type,
        pending_action_status_label(action.status),
        runtime.revision,
    )];
    sections.push(msg::pending_action_id_line(&action.id));
    if !action.summary.trim().is_empty() {
        sections.push(action.summary.trim().to_string());
    }
    if let Some(turn_id) = action.turn_id.as_deref() {
        sections.push(msg::pending_action_turn_line(turn_id));
    }
    sections.push(msg::pending_action_instruction(action.action_type.as_str()).to_string());
    if !snapshot.owners.is_empty() {
        sections.push(msg::owners_section(&format_owners(&snapshot.owners)));
    }
    let context_injections: Vec<_> = action
        .injections
        .iter()
        .filter(|i| i.slot != "constraint")
        .collect();
    let constraint_injections: Vec<_> = action
        .injections
        .iter()
        .filter(|i| i.slot == "constraint")
        .collect();
    if !context_injections.is_empty() {
        sections.push(msg::context_fragments_label(
            &context_injections
                .iter()
                .map(|injection| injection.source.as_str())
                .collect::<Vec<_>>()
                .join("，"),
        ));
    }
    if !constraint_injections.is_empty() {
        sections.push(msg::constraints_section(
            &constraint_injections
                .iter()
                .map(|injection| format!("- {}", injection.content))
                .collect::<Vec<_>>()
                .join("\n"),
        ));
    }
    sections.push(msg::PENDING_ACTION_FOOTER.to_string());

    Some(AgentMessage::user(sections.join("\n\n")))
}

fn append_hook_markdown_body(sections: &mut Vec<String>, injections: &[HookInjection]) {
    let context_injections: Vec<_> = injections
        .iter()
        .filter(|i| i.slot != "constraint")
        .collect();
    let constraint_injections: Vec<_> = injections
        .iter()
        .filter(|i| i.slot == "constraint")
        .collect();

    let mut fragment_lines = Vec::new();
    if !context_injections.is_empty() {
        fragment_lines.push(msg::DYNAMIC_INJECTION_HEADING.to_string());
        for injection in &context_injections {
            fragment_lines.push(format!("### {}", injection.source));
            fragment_lines.push(injection.content.clone());
            fragment_lines.push(String::new());
        }
    }
    if !fragment_lines.is_empty() {
        while fragment_lines.last().is_some_and(|line| line.is_empty()) {
            fragment_lines.pop();
        }
        sections.push(fragment_lines.join("\n"));
    }

    if !constraint_injections.is_empty() {
        sections.push(msg::flow_constraints_section(
            &constraint_injections
                .iter()
                .map(|injection| format!("- {}", injection.content))
                .collect::<Vec<_>>()
                .join("\n"),
        ));
    }
}

fn format_owners(owners: &[agentdash_spi::hooks::HookOwnerSummary]) -> String {
    owners
        .iter()
        .map(|o| {
            format!(
                "- {}: {} {}",
                o.owner_type,
                o.label.as_deref().unwrap_or("??"),
                o.owner_id
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn map_runtime_error(error: agentdash_spi::hooks::HookError) -> AgentRuntimeError {
    AgentRuntimeError::Runtime(error.to_string())
}

fn pending_action_status_label(status: HookPendingActionStatus) -> &'static str {
    match status {
        HookPendingActionStatus::Pending => "pending",
        HookPendingActionStatus::Resolved => "resolved",
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::Mutex;

    use agentdash_spi::{
        AgentContext, AgentMessage, CompactionResult, StopDecision, StopReason, TokenUsage,
    };
    use async_trait::async_trait;
    use tokio_util::sync::CancellationToken;

    use super::HookRuntimeDelegate;
    use crate::context::{AuditFilter, InMemoryContextAuditBus, SharedContextAuditBus};
    use crate::session::HookSessionRuntime;
    use agentdash_spi::hooks::{
        ContextTokenStats, ExecutionHookProvider, HookCompactionDecision, HookCompletionStatus,
        HookDiagnosticEntry, HookError, HookEvaluationQuery, HookInjection, HookPendingAction,
        HookPendingActionResolutionKind, HookResolution, HookSessionRuntimeAccess, HookTrigger,
        NoopExecutionHookProvider, SessionHookRefreshQuery, SessionHookSnapshot,
        SessionHookSnapshotQuery, SessionSnapshotMetadata,
    };

    #[derive(Clone)]
    struct CompletionSatisfiedProvider;

    #[derive(Clone)]
    struct CompletionBlockedProvider;

    #[derive(Clone, Default)]
    struct RecordingCompactionProvider {
        triggers: Arc<Mutex<Vec<HookTrigger>>>,
        after_payloads: Arc<Mutex<Vec<serde_json::Value>>>,
    }

    #[derive(Clone)]
    struct StaticCompanionContextProvider;

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

    #[async_trait]
    impl ExecutionHookProvider for CompletionBlockedProvider {
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
                        satisfied: false,
                        advanced: false,
                        reason: "还有校验未完成".to_string(),
                    },
                ),
                ..HookResolution::default()
            })
        }
    }

    #[async_trait]
    impl ExecutionHookProvider for RecordingCompactionProvider {
        async fn load_session_snapshot(
            &self,
            query: SessionHookSnapshotQuery,
        ) -> Result<SessionHookSnapshot, HookError> {
            Ok(SessionHookSnapshot {
                session_id: query.session_id,
                metadata: Some(SessionSnapshotMetadata {
                    active_workflow: None,
                    turn_id: None,
                    permission_policy: None,
                    working_directory: None,
                    connector_id: None,
                    executor: None,
                    extra: serde_json::Map::from_iter([(
                        "model_context_window".to_string(),
                        serde_json::json!(64_000_u64),
                    )]),
                }),
                ..SessionHookSnapshot::default()
            })
        }

        async fn refresh_session_snapshot(
            &self,
            query: SessionHookRefreshQuery,
        ) -> Result<SessionHookSnapshot, HookError> {
            self.load_session_snapshot(SessionHookSnapshotQuery {
                session_id: query.session_id,
                turn_id: query.turn_id,
            })
            .await
        }

        async fn evaluate_hook(
            &self,
            query: HookEvaluationQuery,
        ) -> Result<HookResolution, HookError> {
            self.triggers
                .lock()
                .expect("recording provider lock poisoned")
                .push(query.trigger.clone());

            Ok(match query.trigger {
                HookTrigger::BeforeCompact => HookResolution {
                    compaction: Some(HookCompactionDecision {
                        cancel: false,
                        reserve_tokens: Some(8_000),
                        keep_last_n: Some(12),
                        custom_summary: None,
                        custom_prompt: Some("自定义摘要 prompt".to_string()),
                    }),
                    ..HookResolution::default()
                },
                HookTrigger::AfterCompact => {
                    if let Some(payload) = query.payload.clone() {
                        self.after_payloads
                            .lock()
                            .expect("after payload lock poisoned")
                            .push(payload);
                    }
                    HookResolution::default()
                }
                _ => HookResolution::default(),
            })
        }
    }

    #[async_trait]
    impl ExecutionHookProvider for StaticCompanionContextProvider {
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
            self.load_session_snapshot(SessionHookSnapshotQuery {
                session_id: query.session_id,
                turn_id: query.turn_id,
            })
            .await
        }

        async fn evaluate_hook(
            &self,
            query: HookEvaluationQuery,
        ) -> Result<HookResolution, HookError> {
            if !matches!(query.trigger, HookTrigger::UserPromptSubmit) {
                return Ok(HookResolution::default());
            }

            Ok(HookResolution {
                diagnostics: vec![HookDiagnosticEntry {
                    code: "session_binding_found".to_string(),
                    message: "命中会话绑定".to_string(),
                }],
                // PR 4 之后 companion_agents 统一走 Bundle 一条路径，delegate 不再
                // 维护 HOOK_USER_MESSAGE_SKIP_SLOTS；这里仍用 workflow slot 以保持
                // 测试断言与 hook markdown 渲染逻辑的对齐。
                injections: vec![HookInjection {
                    slot: "workflow".to_string(),
                    content: "## Workflow\n- step: implement".to_string(),
                    source: "builtin:workflow".to_string(),
                }],
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
            injections: vec![HookInjection {
                slot: "workflow".to_string(),
                content: "必须先处理 review 结果。".to_string(),
                source: "subagent_blocking_review".to_string(),
            }],
        });
        let delegate = HookRuntimeDelegate::new(hook_session.clone());

        let first = delegate
            .before_stop(
                agentdash_spi::BeforeStopInput {
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
                agentdash_spi::BeforeStopInput {
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

    #[tokio::test]
    async fn before_stop_can_continue_without_fake_steering_when_only_stop_gate_blocks() {
        let hook_session = Arc::new(HookSessionRuntime::new(
            "sess-hook".to_string(),
            Arc::new(CompletionBlockedProvider),
            SessionHookSnapshot {
                session_id: "sess-hook".to_string(),
                ..SessionHookSnapshot::default()
            },
        ));
        let delegate = HookRuntimeDelegate::new(hook_session);

        let result = delegate
            .before_stop(
                agentdash_spi::BeforeStopInput {
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

        match result {
            StopDecision::Continue {
                steering,
                follow_up,
                reason,
                allow_empty,
            } => {
                assert!(steering.is_empty(), "不应再伪造 stop gate steering");
                assert!(follow_up.is_empty(), "不应附带 follow_up");
                assert!(allow_empty, "stop gate 未满足时应允许空 continue");
                assert_eq!(
                    reason.as_deref(),
                    Some(super::msg::REASON_STOP_GATE_UNSATISFIED)
                );
            }
            StopDecision::Stop => panic!("completion 未满足时不应允许 stop"),
        }
    }

    #[tokio::test]
    async fn evaluate_compaction_uses_before_compact_hook_decision() {
        let provider = RecordingCompactionProvider::default();
        let hook_session = Arc::new(HookSessionRuntime::new(
            "sess-hook".to_string(),
            Arc::new(provider.clone()),
            provider
                .load_session_snapshot(SessionHookSnapshotQuery {
                    session_id: "sess-hook".to_string(),
                    turn_id: None,
                })
                .await
                .expect("snapshot should load"),
        ));
        hook_session.update_token_stats(ContextTokenStats {
            last_input_tokens: 50_000,
            context_window: 64_000,
        });
        let delegate = HookRuntimeDelegate::new(hook_session);

        let decision = delegate
            .evaluate_compaction(
                agentdash_spi::EvaluateCompactionInput {
                    context: AgentContext {
                        system_prompt: "test".to_string(),
                        messages: vec![
                            AgentMessage::user("旧消息"),
                            AgentMessage::Assistant {
                                content: vec![agentdash_spi::ContentPart::text("旧回复")],
                                tool_calls: vec![],
                                stop_reason: Some(StopReason::Stop),
                                error_message: None,
                                usage: Some(TokenUsage {
                                    input: 50_000,
                                    output: 1_200,
                                }),
                                timestamp: None,
                            },
                        ],
                        tools: vec![],
                    },
                },
                CancellationToken::new(),
            )
            .await
            .expect("evaluate_compaction should succeed")
            .expect("before_compact should request compaction");

        assert_eq!(decision.keep_last_n, 12);
        assert_eq!(decision.reserve_tokens, 8_000);
        assert_eq!(decision.custom_prompt.as_deref(), Some("自定义摘要 prompt"));
        assert_eq!(decision.trigger_stats.input_tokens, 50_000);
        assert_eq!(decision.trigger_stats.context_window, 64_000);
        assert_eq!(
            provider
                .triggers
                .lock()
                .expect("triggers lock poisoned")
                .last(),
            Some(&HookTrigger::BeforeCompact)
        );
    }

    #[tokio::test]
    async fn after_compaction_emits_after_compact_hook_payload() {
        let provider = RecordingCompactionProvider::default();
        let hook_session = Arc::new(HookSessionRuntime::new(
            "sess-hook".to_string(),
            Arc::new(provider.clone()),
            provider
                .load_session_snapshot(SessionHookSnapshotQuery {
                    session_id: "sess-hook".to_string(),
                    turn_id: None,
                })
                .await
                .expect("snapshot should load"),
        ));
        let delegate = HookRuntimeDelegate::new(hook_session);

        delegate
            .after_compaction(
                CompactionResult {
                    messages: vec![AgentMessage::compaction_summary("summary body", 48_000, 6)],
                    summary_message: AgentMessage::compaction_summary("summary body", 48_000, 6),
                    trigger_stats: agentdash_spi::CompactionTriggerStats {
                        input_tokens: 48_000,
                        context_window: 64_000,
                        reserve_tokens: 16_384,
                    },
                    newly_compacted_messages: 3,
                    used_custom_summary: true,
                },
                CancellationToken::new(),
            )
            .await
            .expect("after_compaction should succeed");

        let payloads = provider
            .after_payloads
            .lock()
            .expect("after payload lock poisoned");
        let payload = payloads.last().expect("after_compact payload should exist");
        assert_eq!(
            payload
                .get("messages_compacted")
                .and_then(serde_json::Value::as_u64),
            Some(3)
        );
        assert_eq!(
            payload
                .get("messages_compacted_total")
                .and_then(serde_json::Value::as_u64),
            Some(6)
        );
        assert_eq!(
            payload
                .get("used_custom_summary")
                .and_then(serde_json::Value::as_bool),
            Some(true)
        );
    }

    #[tokio::test]
    async fn transform_context_deduplicates_static_companion_injection_trace() {
        let hook_session = Arc::new(HookSessionRuntime::new(
            "sess-hook".to_string(),
            Arc::new(StaticCompanionContextProvider),
            SessionHookSnapshot {
                session_id: "sess-hook".to_string(),
                ..SessionHookSnapshot::default()
            },
        ));
        let delegate = HookRuntimeDelegate::new(hook_session.clone());

        let input = agentdash_spi::TransformContextInput {
            context: AgentContext {
                system_prompt: "test".to_string(),
                messages: vec![AgentMessage::user("hello")],
                tools: vec![],
            },
        };

        let first = delegate
            .transform_context(input.clone(), CancellationToken::new())
            .await
            .expect("first transform_context should succeed");
        let second = delegate
            .transform_context(input, CancellationToken::new())
            .await
            .expect("second transform_context should succeed");

        // 注入消息仍会参与每次 LLM 请求，但 trace 不应重复刷屏。
        assert!(first.messages.len() > 1);
        assert!(second.messages.len() > 1);

        let submit_traces = hook_session
            .runtime_snapshot()
            .trace
            .into_iter()
            .filter(|trace| matches!(trace.trigger, HookTrigger::UserPromptSubmit))
            .collect::<Vec<_>>();
        assert_eq!(
            submit_traces.len(),
            1,
            "static companion injection should not produce duplicate trace events"
        );
        assert_eq!(submit_traces[0].decision, "context_injected");
    }

    #[tokio::test]
    async fn transform_context_emits_hook_injection_fragments_to_audit_bus() {
        let hook_session = Arc::new(HookSessionRuntime::new(
            "sess-hook".to_string(),
            Arc::new(StaticCompanionContextProvider),
            SessionHookSnapshot {
                session_id: "sess-hook".to_string(),
                ..SessionHookSnapshot::default()
            },
        ));
        let audit_bus: SharedContextAuditBus = Arc::new(InMemoryContextAuditBus::new(100));
        let delegate = HookRuntimeDelegate::new_with_mount_root_and_audit(
            hook_session,
            None,
            Some(audit_bus.clone()),
        );

        delegate
            .transform_context(
                agentdash_spi::TransformContextInput {
                    context: AgentContext {
                        system_prompt: "test".to_string(),
                        messages: vec![AgentMessage::user("hello")],
                        tools: vec![],
                    },
                },
                CancellationToken::new(),
            )
            .await
            .expect("transform_context should succeed");

        let events = audit_bus.query("sess-hook", &AuditFilter::default());
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].trigger.as_tag(), "hook:UserPromptSubmit");
        assert_eq!(events[0].fragment.slot, "workflow");
        assert!(events[0].fragment.content.contains("implement"));
    }

    #[test]
    fn pending_action_message_does_not_reference_specific_tools() {
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
            status: agentdash_spi::hooks::HookPendingActionStatus::Pending,
            last_injected_at_ms: Some(1_710_000_100_000),
            resolved_at_ms: None,
            resolution_kind: None,
            resolution_note: None,
            resolution_turn_id: None,
            injections: vec![HookInjection {
                slot: "workflow".to_string(),
                content: "继续落实下一步".to_string(),
                source: "follow_up".to_string(),
            }],
        };

        let message = super::build_pending_action_message(&snapshot, &action, &runtime)
            .expect("应该生成 pending action 消息");
        let text = match message {
            agentdash_spi::AgentMessage::User { content, .. } => content
                .iter()
                .filter_map(|part| part.extract_text())
                .collect::<Vec<_>>()
                .join("\n"),
            other => panic!("期望 User 消息，实际为 {other:?}"),
        };

        // 指令文本中不应引用具体工具名 — 工具名是实现细节，由调用方上下文提供
        assert!(
            !text.contains("companion_respond"),
            "pending action 指令文本不应硬编码具体工具名: {text}"
        );
        assert!(
            text.contains("follow_up_required"),
            "消息应包含 action_type 标识: {text}"
        );
        assert!(
            text.contains("status=pending"),
            "消息应包含状态标识: {text}"
        );
    }
}
