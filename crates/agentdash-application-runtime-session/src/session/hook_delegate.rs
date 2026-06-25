use std::sync::Arc;

use agentdash_spi::{
    AfterToolCallEffects, AfterToolCallInput, AfterTurnInput, AgentMessage, AgentRuntimeDelegate,
    AgentRuntimeError, BeforeProviderRequestInput, BeforeStopInput, BeforeToolCallInput,
    CompactionFailureInput, CompactionParams, CompactionResult, CompactionTriggerStats,
    DynAgentRuntimeDelegate, EvaluateCompactionInput, StopDecision, StopReason, ToolCallDecision,
    TransformContextInput, TransformContextOutput, TurnControlDecision,
};
use async_trait::async_trait;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use super::hook_injection_sink::{DynRuntimeHookInjectionSink, RuntimeInjectionSource};
use super::hook_messages as msg;
use super::pending_action_context_frame::build_pending_action_context_frame;

use crate::context::{AuditTrigger, SharedContextAuditBus, emit_fragment};
use crate::hooks::hook_injection_to_fragment;

use agentdash_spi::hooks::{
    AgentFrameRuntimeSnapshot, ContextTokenStats, HookDiagnosticEntry, HookInjection,
    HookRuntimeEvaluationQuery, HookRuntimeRefreshQuery, HookTraceEntry, HookTraceTrigger,
    HookTrigger, HookTurnStartNotice, RuntimeAdapterProvenance, SharedHookRuntime,
};

const COMPACTION_FAILURE_FUSE_LIMIT: u32 = 3;

pub struct HookRuntimeDelegate {
    hook_runtime: SharedHookRuntime,
    default_mount_root_ref: Option<String>,
    audit_bus: Option<SharedContextAuditBus>,
    injection_sink: Option<DynRuntimeHookInjectionSink>,
}

impl HookRuntimeDelegate {
    #[allow(clippy::new_ret_no_self)]
    pub fn new(hook_runtime: SharedHookRuntime) -> DynAgentRuntimeDelegate {
        Self::new_with_mount_root(hook_runtime, None)
    }

    #[allow(clippy::new_ret_no_self)]
    pub fn new_with_mount_root(
        hook_runtime: SharedHookRuntime,
        default_mount_root_ref: Option<String>,
    ) -> DynAgentRuntimeDelegate {
        Self::new_with_mount_root_and_audit(hook_runtime, default_mount_root_ref, None)
    }

    #[allow(clippy::new_ret_no_self)]
    pub fn new_with_mount_root_and_audit(
        hook_runtime: SharedHookRuntime,
        default_mount_root_ref: Option<String>,
        audit_bus: Option<SharedContextAuditBus>,
    ) -> DynAgentRuntimeDelegate {
        Self::new_with_mount_root_audit_and_sink(
            hook_runtime,
            default_mount_root_ref,
            audit_bus,
            None,
        )
    }

    #[allow(clippy::new_ret_no_self)]
    pub fn new_with_mount_root_audit_and_sink(
        hook_runtime: SharedHookRuntime,
        default_mount_root_ref: Option<String>,
        audit_bus: Option<SharedContextAuditBus>,
        injection_sink: Option<DynRuntimeHookInjectionSink>,
    ) -> DynAgentRuntimeDelegate {
        Arc::new(Self {
            hook_runtime,
            default_mount_root_ref,
            audit_bus,
            injection_sink,
        })
    }

    async fn evaluate(
        &self,
        trigger: HookTrigger,
        tool_name: Option<String>,
        tool_call_id: Option<String>,
        subagent_type: Option<String>,
        payload: Option<serde_json::Value>,
        token_stats: Option<ContextTokenStats>,
    ) -> Result<EvaluatedResolution, AgentRuntimeError> {
        let snapshot = self.hook_runtime.snapshot();
        let resolution = self
            .hook_runtime
            .evaluate_from_provenance(HookRuntimeEvaluationQuery {
                provenance: RuntimeAdapterProvenance::runtime_session(
                    self.hook_runtime.session_id().to_string(),
                    None,
                    "runtime_delegate_hook_evaluate",
                ),
                trigger,
                tool_name,
                tool_call_id,
                subagent_type,
                snapshot: Some(snapshot.clone()),
                payload,
                token_stats,
            })
            .await
            .map_err(map_runtime_error)?;

        if resolution.refresh_snapshot {
            self.hook_runtime
                .refresh_from_provenance(HookRuntimeRefreshQuery {
                    provenance: RuntimeAdapterProvenance::runtime_session(
                        self.hook_runtime.session_id().to_string(),
                        None,
                        "runtime_delegate_hook_refresh",
                    ),
                    reason: Some(format!("trigger:{:?}", trigger)),
                })
                .await
                .map_err(map_runtime_error)?;
        }
        self.emit_runtime_hook_injections(trigger, &resolution.injections)
            .await;

        Ok(EvaluatedResolution {
            snapshot: self.hook_runtime.snapshot(),
            resolution,
            runtime: self.hook_runtime.runtime_snapshot(),
        })
    }

    async fn emit_runtime_hook_injections(
        &self,
        trigger: HookTrigger,
        injections: &[HookInjection],
    ) {
        if let Some(sink) = self.injection_sink.as_ref() {
            sink.emit_injections(
                self.hook_runtime.session_id(),
                RuntimeInjectionSource::Hook(trigger),
                injections,
            )
            .await;
        } else {
            self.emit_hook_injection_fragments(trigger, injections);
        }
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
                self.hook_runtime.session_id(),
                bundle_session_uuid,
                AuditTrigger::HookInjection {
                    trigger: trigger_label.clone(),
                },
                &fragment,
            );
        }
    }

    fn snapshot_model_context_window(&self) -> Option<u64> {
        self.hook_runtime
            .snapshot()
            .metadata
            .as_ref()
            .and_then(|m| m.extra.get("model_context_window"))
            .and_then(|v| v.as_u64())
            .filter(|value| *value > 0)
    }

    fn resolve_model_context_window(&self, explicit: u64, previous: &ContextTokenStats) -> u64 {
        if explicit > 0 {
            return explicit;
        }
        if let Some(context_window) = self.snapshot_model_context_window() {
            return context_window;
        }
        previous
            .effective_context_window
            .max(previous.context_window)
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
            let previous = self.hook_runtime.token_stats();
            let context_window = self.resolve_model_context_window(0, &previous);

            self.hook_runtime.update_token_stats(ContextTokenStats {
                last_input_tokens: usage.context_input_tokens(),
                current_context_tokens: usage.context_input_tokens(),
                pending_estimate_tokens: 0,
                context_window,
                effective_context_window: context_window,
                reserve_tokens: previous.reserve_tokens,
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
        let Some(trace_trigger) = trigger.trace_trigger() else {
            return;
        };
        let decision = decision.into();
        let include_injections = should_include_trace_injections(&trigger, &decision);
        let trace = HookTraceEntry {
            sequence: self.hook_runtime.next_trace_sequence(),
            timestamp_ms: chrono::Utc::now().timestamp_millis(),
            revision: evaluated.runtime.revision,
            trigger: trace_trigger,
            decision,
            tool_name,
            tool_call_id,
            subagent_type,
            matched_rule_keys: evaluated.resolution.matched_rule_keys.clone(),
            refresh_snapshot: evaluated.resolution.refresh_snapshot,
            block_reason: evaluated.resolution.block_reason.clone(),
            completion: evaluated.resolution.completion.clone(),
            diagnostics: evaluated.resolution.diagnostics.clone(),
            injections: if include_injections {
                evaluated.resolution.injections.clone()
            } else {
                Vec::new()
            },
        };
        self.hook_runtime.append_trace(trace);
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

        let last_usage = self.hook_runtime.token_stats();
        let default_keep_last_n = 20_u32;
        let default_reserve_tokens = 16_384_u64;
        let context_window = self.resolve_model_context_window(0, &last_usage);
        let effective_context_window = context_window;
        let provider_estimate = input
            .provider_visible
            .as_ref()
            .map(|stats| stats.estimated_input_tokens)
            .filter(|value| *value > 0)
            .unwrap_or(last_usage.last_input_tokens);
        let live_token_stats = ContextTokenStats {
            last_input_tokens: provider_estimate,
            current_context_tokens: provider_estimate,
            pending_estimate_tokens: 0,
            context_window,
            effective_context_window,
            reserve_tokens: default_reserve_tokens,
        };
        if provider_estimate > 0 || context_window > 0 {
            self.hook_runtime
                .update_token_stats(live_token_stats.clone());
        }
        let consecutive_failures = self.hook_runtime.compaction_failure_count();
        if consecutive_failures >= COMPACTION_FAILURE_FUSE_LIMIT {
            self.hook_runtime
                .append_diagnostics_vec(vec![HookDiagnosticEntry {
                    code: "context_compaction_fused".to_string(),
                    message: format!(
                        "上下文压缩连续失败 {consecutive_failures} 次，已暂时停止自动重试"
                    ),
                }]);
            return Ok(None);
        }
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
                Some(live_token_stats.clone()),
            )
            .await?;

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
                        input_tokens: live_token_stats.current_context_tokens,
                        context_window: live_token_stats.effective_context_window,
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
        self.hook_runtime.reset_compaction_failures();
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
                None,
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

    async fn after_compaction_failed(
        &self,
        input: CompactionFailureInput,
        _cancel: CancellationToken,
    ) -> Result<(), AgentRuntimeError> {
        let failures = self.hook_runtime.record_compaction_failure(&input.error);
        let mut diagnostics = vec![HookDiagnosticEntry {
            code: "context_compaction_failed".to_string(),
            message: format!("上下文压缩 {} 失败: {}", input.item_id, input.error),
        }];
        if failures >= COMPACTION_FAILURE_FUSE_LIMIT {
            diagnostics.push(HookDiagnosticEntry {
                code: "context_compaction_fused".to_string(),
                message: format!("上下文压缩连续失败 {failures} 次，已停止自动重试"),
            });
        }
        self.hook_runtime.append_diagnostics_vec(diagnostics);
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
                None,
            )
            .await?;

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
                steering_messages: vec![],
                blocked: Some(reason),
            });
        }

        let turn_start_messages = collect_turn_start_injection_messages(
            self.hook_runtime.as_ref(),
            &evaluated.snapshot,
            &self.hook_runtime.runtime_snapshot(),
        );
        if should_trace_user_prompt_context_injection(
            &evaluated.runtime,
            &evaluated.resolution.injections,
            turn_start_messages.consumed,
        ) {
            self.record_trace(
                HookTrigger::UserPromptSubmit,
                "context_injected",
                None,
                None,
                None,
                &evaluated,
            );
        } else if evaluated.resolution.injections.is_empty() && turn_start_messages.consumed == 0 {
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
        if let Some(ref new_text) = evaluated.resolution.transformed_message
            && let Some(last_user) = messages.iter_mut().rev().find(|m| m.is_user())
        {
            last_user.replace_user_text(new_text);
        }

        // TurnStart 统一消费 turn-start notice / pending action 这类暂存注入事件；
        // 通用 hook injections 不再隐式桥接为 inline user message。
        messages.extend(turn_start_messages.steering);
        messages.extend(turn_start_messages.follow_up);

        Ok(TransformContextOutput {
            steering_messages: messages,
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
                None,
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
                None,
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
                None,
            )
            .await?;
        self.record_trace(HookTrigger::AfterTurn, "noop", None, None, None, &evaluated);

        Ok(TurnControlDecision {
            steering: Vec::new(),
            follow_up: Vec::new(),
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
        let evaluated = self
            .evaluate(
                HookTrigger::BeforeStop,
                None,
                None,
                None,
                Some(serde_json::json!({
                    "message_count": input.context.messages.len(),
                })),
                None,
            )
            .await?;

        // BeforeStop 只做 gate：暂存 runtime 事件统一留到下一次 TurnStart 注入。
        let unresolved_runtime_events = self.hook_runtime.unresolved_pending_actions();
        let completion_satisfied = evaluated
            .resolution
            .completion
            .as_ref()
            .is_some_and(|completion| completion.satisfied);
        let has_completion_gate = evaluated.resolution.completion.is_some();

        if unresolved_runtime_events.is_empty() && (!has_completion_gate || completion_satisfied) {
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

        let allow_empty_continue = true;

        self.record_trace(
            HookTrigger::BeforeStop,
            "continue",
            None,
            None,
            None,
            &evaluated,
        );
        Ok(StopDecision::Continue {
            steering: Vec::new(),
            follow_up: Vec::new(),
            reason: Some(if !unresolved_runtime_events.is_empty() {
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
        let previous = self.hook_runtime.token_stats();
        let context_window = self.resolve_model_context_window(input.context_window, &previous);
        let reserve_tokens = if input.reserve_tokens > 0 {
            input.reserve_tokens
        } else {
            previous.reserve_tokens
        };
        let token_stats = ContextTokenStats {
            last_input_tokens: input.estimated_input_tokens,
            current_context_tokens: input.estimated_input_tokens,
            pending_estimate_tokens: 0,
            context_window,
            effective_context_window: context_window,
            reserve_tokens,
        };
        self.hook_runtime.update_token_stats(token_stats.clone());
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
                    "estimated_input_tokens": input.estimated_input_tokens,
                    "context_window": context_window,
                    "reserve_tokens": reserve_tokens,
                })),
                Some(token_stats),
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
    snapshot: agentdash_spi::hooks::AgentFrameHookSnapshot,
    resolution: agentdash_spi::hooks::HookResolution,
    runtime: AgentFrameRuntimeSnapshot,
}

#[derive(Default)]
struct TurnStartInjectionMessages {
    steering: Vec<AgentMessage>,
    follow_up: Vec<AgentMessage>,
    consumed: usize,
}

fn should_include_trace_injections(trigger: &HookTrigger, decision: &str) -> bool {
    matches!(
        (trigger, decision),
        (HookTrigger::UserPromptSubmit, "context_injected")
    )
}

fn should_trace_user_prompt_context_injection(
    runtime: &AgentFrameRuntimeSnapshot,
    injections: &[HookInjection],
    pending_consumed: usize,
) -> bool {
    if pending_consumed > 0 {
        return false;
    }
    if injections.is_empty() {
        return false;
    }

    let previous = runtime
        .trace
        .iter()
        .rev()
        .find(|entry| matches!(entry.trigger, HookTraceTrigger::UserPromptSubmit));

    match previous {
        Some(entry) => entry.injections != injections,
        None => true,
    }
}

fn collect_turn_start_injection_messages(
    hook_runtime: &dyn agentdash_spi::hooks::HookRuntimeAccess,
    snapshot: &agentdash_spi::hooks::AgentFrameHookSnapshot,
    runtime: &AgentFrameRuntimeSnapshot,
) -> TurnStartInjectionMessages {
    let mut messages = TurnStartInjectionMessages::default();
    let turn_start_notices = collect_turn_start_notice_messages(hook_runtime);
    messages.consumed += turn_start_notices.len();
    messages.steering.extend(turn_start_notices);

    let actions = hook_runtime.collect_pending_actions_for_injection();
    messages.consumed += actions.len();
    for action in actions {
        let Some(frame) = build_pending_action_context_frame(snapshot, &action, runtime) else {
            continue;
        };
        let notice = HookTurnStartNotice {
            id: frame.id.clone(),
            created_at_ms: frame.created_at_ms,
            source: frame.source.clone(),
            content: frame.rendered_text.clone(),
            context_frame: Some(frame.clone()),
        };
        let message = AgentMessage::user(format_turn_start_notice_frame(
            &notice,
            &frame.rendered_text,
        ));
        if action.is_follow_up() {
            messages.follow_up.push(message);
        } else {
            messages.steering.push(message);
        }
    }
    messages
}

fn collect_turn_start_notice_messages(
    hook_runtime: &dyn agentdash_spi::hooks::HookRuntimeAccess,
) -> Vec<AgentMessage> {
    let notices = hook_runtime.collect_turn_start_notices_for_injection();
    if notices.is_empty() {
        return Vec::new();
    }
    let frames = notices
        .into_iter()
        .filter_map(|notice| {
            let content = notice
                .context_frame
                .as_ref()
                .map(|frame| frame.rendered_text.as_str())
                .unwrap_or(notice.content.as_str())
                .trim()
                .to_string();
            (!content.is_empty()).then(|| format_turn_start_notice_frame(&notice, &content))
        })
        .collect::<Vec<_>>();
    if frames.is_empty() {
        return Vec::new();
    }
    let body = if frames.len() == 1 {
        frames.into_iter().next().unwrap_or_default()
    } else {
        format!(
            "[CTX Frame Batch]\nframe_count: {}\n\n{}",
            frames.len(),
            frames.join("\n\n---\n\n")
        )
    };
    vec![AgentMessage::user(body)]
}

fn format_turn_start_notice_frame(notice: &HookTurnStartNotice, content: &str) -> String {
    if let Some(frame) = notice.context_frame.as_ref() {
        return format!(
            "[CTX Frame]\nframe_id: {}\nkind: {}\nsource: {}\ndelivery: {}\nchannel: {}\nrole: {}\n\n{}",
            frame.id,
            frame.kind,
            frame.source.as_key(),
            frame.delivery_status,
            frame.delivery_channel,
            frame.message_role,
            content
        );
    }
    format!(
        "[CTX Notice]\nnotice_id: {}\nsource: {}\n\n{}",
        notice.id,
        notice.source.as_key(),
        content
    )
}

fn map_runtime_error(error: agentdash_spi::hooks::HookError) -> AgentRuntimeError {
    AgentRuntimeError::Runtime(error.to_string())
}

#[cfg(test)]
mod tests {
    use std::{
        collections::BTreeSet,
        sync::{
            Arc, Mutex, RwLock,
            atomic::{AtomicU32, AtomicU64, Ordering},
        },
    };

    use agentdash_spi::{
        AgentContext, AgentMessage, BeforeProviderRequestInput, CompactionFailureInput,
        CompactionResult, StopDecision, StopReason, TokenUsage,
    };
    use async_trait::async_trait;
    use tokio_util::sync::CancellationToken;

    use super::HookRuntimeDelegate;
    use crate::context::{AuditFilter, InMemoryContextAuditBus, SharedContextAuditBus};
    use crate::session::hook_injection_sink::{RuntimeHookInjectionSink, RuntimeInjectionSource};
    use agentdash_spi::hooks::{
        AgentFrameHookEvaluationQuery, AgentFrameHookRefreshQuery, AgentFrameHookSnapshot,
        AgentFrameHookSnapshotQuery, AgentFrameRuntimeSnapshot, ContextTokenStats,
        ExecutionHookProvider, HookCompactionDecision, HookCompletionStatus, HookControlTarget,
        HookDiagnosticEntry, HookError, HookInjection, HookPendingAction,
        HookPendingActionResolutionKind, HookPendingActionStatus, HookResolution,
        HookRuntimeAccess, HookRuntimeEvaluationQuery, HookRuntimeRefreshQuery, HookTraceEntry,
        HookTraceTrigger, HookTrigger, HookTurnStartNotice, NoopExecutionHookProvider,
        RuntimeEventSource, SessionSnapshotMetadata, SetDelta,
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

    #[derive(Clone)]
    struct AfterTurnInjectionProvider;

    #[derive(Default)]
    struct RecordingInjectionSink {
        records: Mutex<Vec<(String, RuntimeInjectionSource, Vec<HookInjection>)>>,
    }

    struct TestHookRuntime {
        runtime_session_id: String,
        control_target: HookControlTarget,
        provider: Arc<dyn ExecutionHookProvider>,
        snapshot: RwLock<AgentFrameHookSnapshot>,
        diagnostics: RwLock<Vec<HookDiagnosticEntry>>,
        trace: RwLock<Vec<HookTraceEntry>>,
        pending_actions: RwLock<Vec<HookPendingAction>>,
        turn_start_notices: RwLock<Vec<HookTurnStartNotice>>,
        token_stats: RwLock<ContextTokenStats>,
        compaction_failure_count: AtomicU32,
        capabilities: RwLock<BTreeSet<String>>,
        revision: AtomicU64,
        trace_sequence: AtomicU64,
    }

    impl std::fmt::Debug for TestHookRuntime {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("TestHookRuntime")
                .field("runtime_session_id", &self.runtime_session_id)
                .field("control_target", &self.control_target)
                .field("revision", &self.revision())
                .finish()
        }
    }

    impl TestHookRuntime {
        fn new_test_runtime(
            runtime_session_id: String,
            provider: Arc<dyn ExecutionHookProvider>,
            snapshot: AgentFrameHookSnapshot,
        ) -> Self {
            Self {
                runtime_session_id,
                control_target: HookControlTarget {
                    run_id: uuid::Uuid::new_v4(),
                    agent_id: uuid::Uuid::new_v4(),
                    frame_id: uuid::Uuid::new_v4(),
                },
                provider,
                diagnostics: RwLock::new(snapshot.diagnostics.clone()),
                snapshot: RwLock::new(snapshot),
                trace: RwLock::new(Vec::new()),
                pending_actions: RwLock::new(Vec::new()),
                turn_start_notices: RwLock::new(Vec::new()),
                token_stats: RwLock::new(ContextTokenStats::default()),
                compaction_failure_count: AtomicU32::new(0),
                capabilities: RwLock::new(BTreeSet::new()),
                revision: AtomicU64::new(1),
                trace_sequence: AtomicU64::new(0),
            }
        }

        fn append_diagnostics_inner<I>(&self, entries: I)
        where
            I: IntoIterator<Item = HookDiagnosticEntry>,
        {
            let mut guard = self
                .diagnostics
                .write()
                .expect("hook diagnostics write lock poisoned");
            for entry in entries {
                if guard.iter().any(|existing| {
                    existing.code == entry.code && existing.message == entry.message
                }) {
                    continue;
                }
                guard.push(entry);
            }
        }
    }

    #[async_trait]
    impl HookRuntimeAccess for TestHookRuntime {
        fn session_id(&self) -> &str {
            &self.runtime_session_id
        }

        fn control_target(&self) -> HookControlTarget {
            self.control_target.clone()
        }

        fn snapshot(&self) -> AgentFrameHookSnapshot {
            self.snapshot
                .read()
                .expect("hook snapshot read lock poisoned")
                .clone()
        }

        fn diagnostics(&self) -> Vec<HookDiagnosticEntry> {
            self.diagnostics
                .read()
                .expect("hook diagnostics read lock poisoned")
                .clone()
        }

        fn revision(&self) -> u64 {
            self.revision.load(Ordering::SeqCst)
        }

        fn trace(&self) -> Vec<HookTraceEntry> {
            self.trace
                .read()
                .expect("hook trace read lock poisoned")
                .clone()
        }

        fn pending_actions(&self) -> Vec<HookPendingAction> {
            self.pending_actions
                .read()
                .expect("hook pending actions read lock poisoned")
                .clone()
        }

        fn runtime_snapshot(&self) -> AgentFrameRuntimeSnapshot {
            AgentFrameRuntimeSnapshot {
                runtime_adapter_session_id: self.runtime_session_id.clone(),
                revision: self.revision(),
                snapshot: self.snapshot(),
                diagnostics: self.diagnostics(),
                trace: self.trace(),
                pending_actions: self.pending_actions(),
            }
        }

        async fn refresh_from_provenance(
            &self,
            query: HookRuntimeRefreshQuery,
        ) -> Result<AgentFrameHookSnapshot, HookError> {
            let snapshot = self
                .provider
                .refresh_frame_snapshot(AgentFrameHookRefreshQuery {
                    target: self.control_target(),
                    provenance: query.provenance,
                    reason: query.reason,
                })
                .await?;
            self.replace_snapshot(snapshot.clone());
            Ok(snapshot)
        }

        async fn evaluate_from_provenance(
            &self,
            query: HookRuntimeEvaluationQuery,
        ) -> Result<HookResolution, HookError> {
            let mut resolution = self
                .provider
                .evaluate_frame_hook(AgentFrameHookEvaluationQuery {
                    target: self.control_target(),
                    provenance: query.provenance,
                    trigger: query.trigger,
                    tool_name: query.tool_name,
                    tool_call_id: query.tool_call_id,
                    subagent_type: query.subagent_type,
                    snapshot: query.snapshot,
                    payload: query.payload,
                    token_stats: Some(self.token_stats()),
                })
                .await?;
            self.append_diagnostics_inner(resolution.diagnostics.clone());
            resolution.pending_execution_log.clear();
            Ok(resolution)
        }

        fn replace_snapshot(&self, snapshot: AgentFrameHookSnapshot) {
            *self
                .snapshot
                .write()
                .expect("hook snapshot write lock poisoned") = snapshot.clone();
            self.append_diagnostics_inner(snapshot.diagnostics);
            self.revision.fetch_add(1, Ordering::SeqCst);
        }

        fn append_diagnostics_vec(&self, entries: Vec<HookDiagnosticEntry>) {
            self.append_diagnostics_inner(entries);
        }

        fn append_trace(&self, trace: HookTraceEntry) {
            self.trace
                .write()
                .expect("hook trace write lock poisoned")
                .push(trace);
        }

        fn next_trace_sequence(&self) -> u64 {
            self.trace_sequence.fetch_add(1, Ordering::SeqCst) + 1
        }

        fn enqueue_pending_action(&self, action: HookPendingAction) {
            self.pending_actions
                .write()
                .expect("hook pending actions write lock poisoned")
                .push(HookPendingAction {
                    status: HookPendingActionStatus::Pending,
                    last_injected_at_ms: None,
                    resolved_at_ms: None,
                    resolution_kind: None,
                    resolution_note: None,
                    resolution_turn_id: None,
                    ..action
                });
        }

        fn collect_pending_actions_for_injection(&self) -> Vec<HookPendingAction> {
            let mut guard = self
                .pending_actions
                .write()
                .expect("hook pending actions write lock poisoned");
            let actions = guard
                .iter_mut()
                .filter(|action| action.status == HookPendingActionStatus::Pending)
                .map(|action| {
                    action.last_injected_at_ms = Some(chrono::Utc::now().timestamp_millis());
                    action.clone()
                })
                .collect();
            actions
        }

        fn enqueue_turn_start_notice(&self, notice: HookTurnStartNotice) {
            self.turn_start_notices
                .write()
                .expect("hook turn-start notices write lock poisoned")
                .push(notice);
        }

        fn collect_turn_start_notices_for_injection(&self) -> Vec<HookTurnStartNotice> {
            let mut guard = self
                .turn_start_notices
                .write()
                .expect("hook turn-start notices write lock poisoned");
            let notices = guard.clone();
            guard.clear();
            notices
        }

        fn unresolved_pending_actions(&self) -> Vec<HookPendingAction> {
            self.pending_actions
                .read()
                .expect("hook pending actions read lock poisoned")
                .iter()
                .filter(|action| action.is_unresolved())
                .cloned()
                .collect()
        }

        fn unresolved_blocking_actions(&self) -> Vec<HookPendingAction> {
            self.unresolved_pending_actions()
                .into_iter()
                .filter(HookPendingAction::is_blocking)
                .collect()
        }

        fn resolve_pending_action(
            &self,
            action_id: &str,
            resolution_kind: HookPendingActionResolutionKind,
            note: Option<String>,
            turn_id: Option<String>,
        ) -> Option<HookPendingAction> {
            let mut guard = self
                .pending_actions
                .write()
                .expect("hook pending actions write lock poisoned");
            let action = guard.iter_mut().find(|action| action.id == action_id)?;
            action.status = HookPendingActionStatus::Resolved;
            action.resolution_kind = Some(resolution_kind);
            action.resolution_note = note;
            action.resolution_turn_id = turn_id;
            Some(action.clone())
        }

        fn update_token_stats(&self, stats: ContextTokenStats) {
            *self
                .token_stats
                .write()
                .expect("token stats write lock poisoned") = stats;
        }

        fn token_stats(&self) -> ContextTokenStats {
            self.token_stats
                .read()
                .expect("token stats read lock poisoned")
                .clone()
        }

        fn record_compaction_failure(&self, _error: &str) -> u32 {
            self.compaction_failure_count.fetch_add(1, Ordering::SeqCst) + 1
        }

        fn reset_compaction_failures(&self) {
            self.compaction_failure_count.store(0, Ordering::SeqCst);
        }

        fn compaction_failure_count(&self) -> u32 {
            self.compaction_failure_count.load(Ordering::SeqCst)
        }

        fn current_capabilities(&self) -> BTreeSet<String> {
            self.capabilities
                .read()
                .expect("capabilities read lock poisoned")
                .clone()
        }

        fn update_capabilities(&self, new_caps: BTreeSet<String>) -> Option<SetDelta> {
            let mut guard = self
                .capabilities
                .write()
                .expect("capabilities write lock poisoned");
            let delta = SetDelta::compute(&guard, &new_caps);
            if delta.is_empty() {
                return None;
            }
            *guard = new_caps;
            Some(delta)
        }
    }

    #[async_trait]
    impl RuntimeHookInjectionSink for RecordingInjectionSink {
        async fn emit_injections(
            &self,
            session_id: &str,
            source: RuntimeInjectionSource,
            injections: &[HookInjection],
        ) {
            self.records
                .lock()
                .expect("recording sink lock poisoned")
                .push((session_id.to_string(), source, injections.to_vec()));
        }
    }

    #[async_trait]
    impl ExecutionHookProvider for CompletionSatisfiedProvider {
        async fn load_frame_snapshot(
            &self,
            query: AgentFrameHookSnapshotQuery,
        ) -> Result<AgentFrameHookSnapshot, HookError> {
            Ok(AgentFrameHookSnapshot {
                runtime_adapter_session_id: query.provenance.runtime_session_id.unwrap_or_default(),
                ..AgentFrameHookSnapshot::default()
            })
        }

        async fn refresh_frame_snapshot(
            &self,
            query: AgentFrameHookRefreshQuery,
        ) -> Result<AgentFrameHookSnapshot, HookError> {
            Ok(AgentFrameHookSnapshot {
                runtime_adapter_session_id: query.provenance.runtime_session_id.unwrap_or_default(),
                ..AgentFrameHookSnapshot::default()
            })
        }

        async fn evaluate_frame_hook(
            &self,
            query: AgentFrameHookEvaluationQuery,
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
        async fn load_frame_snapshot(
            &self,
            query: AgentFrameHookSnapshotQuery,
        ) -> Result<AgentFrameHookSnapshot, HookError> {
            Ok(AgentFrameHookSnapshot {
                runtime_adapter_session_id: query.provenance.runtime_session_id.unwrap_or_default(),
                ..AgentFrameHookSnapshot::default()
            })
        }

        async fn refresh_frame_snapshot(
            &self,
            query: AgentFrameHookRefreshQuery,
        ) -> Result<AgentFrameHookSnapshot, HookError> {
            Ok(AgentFrameHookSnapshot {
                runtime_adapter_session_id: query.provenance.runtime_session_id.unwrap_or_default(),
                ..AgentFrameHookSnapshot::default()
            })
        }

        async fn evaluate_frame_hook(
            &self,
            query: AgentFrameHookEvaluationQuery,
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
        async fn load_frame_snapshot(
            &self,
            query: AgentFrameHookSnapshotQuery,
        ) -> Result<AgentFrameHookSnapshot, HookError> {
            Ok(AgentFrameHookSnapshot {
                runtime_adapter_session_id: query.provenance.runtime_session_id.unwrap_or_default(),
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
                ..AgentFrameHookSnapshot::default()
            })
        }

        async fn refresh_frame_snapshot(
            &self,
            query: AgentFrameHookRefreshQuery,
        ) -> Result<AgentFrameHookSnapshot, HookError> {
            self.load_frame_snapshot(AgentFrameHookSnapshotQuery {
                target: query.target,
                provenance: query.provenance,
            })
            .await
        }

        async fn evaluate_frame_hook(
            &self,
            query: AgentFrameHookEvaluationQuery,
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
        async fn load_frame_snapshot(
            &self,
            query: AgentFrameHookSnapshotQuery,
        ) -> Result<AgentFrameHookSnapshot, HookError> {
            Ok(AgentFrameHookSnapshot {
                runtime_adapter_session_id: query.provenance.runtime_session_id.unwrap_or_default(),
                ..AgentFrameHookSnapshot::default()
            })
        }

        async fn refresh_frame_snapshot(
            &self,
            query: AgentFrameHookRefreshQuery,
        ) -> Result<AgentFrameHookSnapshot, HookError> {
            self.load_frame_snapshot(AgentFrameHookSnapshotQuery {
                target: query.target,
                provenance: query.provenance,
            })
            .await
        }

        async fn evaluate_frame_hook(
            &self,
            query: AgentFrameHookEvaluationQuery,
        ) -> Result<HookResolution, HookError> {
            if !matches!(query.trigger, HookTrigger::UserPromptSubmit) {
                return Ok(HookResolution::default());
            }

            Ok(HookResolution {
                diagnostics: vec![HookDiagnosticEntry {
                    code: "active_workflow_resolved".to_string(),
                    message: "命中活跃 workflow".to_string(),
                }],
                injections: vec![HookInjection {
                    slot: "workflow".to_string(),
                    content: "## Workflow\n- step: implement".to_string(),
                    source: "builtin:workflow".to_string(),
                }],
                ..HookResolution::default()
            })
        }
    }

    #[allow(deprecated)]
    #[async_trait]
    impl ExecutionHookProvider for AfterTurnInjectionProvider {
        async fn load_frame_snapshot(
            &self,
            query: AgentFrameHookSnapshotQuery,
        ) -> Result<AgentFrameHookSnapshot, HookError> {
            Ok(AgentFrameHookSnapshot {
                runtime_adapter_session_id: query.provenance.runtime_session_id.unwrap_or_default(),
                ..AgentFrameHookSnapshot::default()
            })
        }

        async fn refresh_frame_snapshot(
            &self,
            query: AgentFrameHookRefreshQuery,
        ) -> Result<AgentFrameHookSnapshot, HookError> {
            self.load_frame_snapshot(AgentFrameHookSnapshotQuery {
                target: query.target,
                provenance: query.provenance,
            })
            .await
        }

        async fn evaluate_frame_hook(
            &self,
            query: AgentFrameHookEvaluationQuery,
        ) -> Result<HookResolution, HookError> {
            if !matches!(query.trigger, HookTrigger::AfterTurn) {
                return Ok(HookResolution::default());
            }
            Ok(HookResolution {
                injections: vec![HookInjection {
                    slot: "workflow".to_string(),
                    content: "## Runtime Status\n- phase: project_agent".to_string(),
                    source: "builtin:test_after_turn".to_string(),
                }],
                ..HookResolution::default()
            })
        }
    }

    #[tokio::test]
    async fn before_stop_is_blocked_until_blocking_review_action_is_resolved() {
        let hook_runtime = Arc::new(TestHookRuntime::new_test_runtime(
            "sess-hook".to_string(),
            Arc::new(CompletionSatisfiedProvider),
            AgentFrameHookSnapshot {
                runtime_adapter_session_id: "sess-hook".to_string(),
                ..AgentFrameHookSnapshot::default()
            },
        ));
        hook_runtime.enqueue_pending_action(HookPendingAction {
            id: "blocking-1".to_string(),
            created_at_ms: 1_710_000_000_000,
            title: "Companion review 需要处理".to_string(),
            summary: "请先确认是否采纳 review 结论".to_string(),
            action_type: "blocking_review".to_string(),
            turn_id: Some("turn-parent-1".to_string()),
            source: RuntimeEventSource::CompanionResult,
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
        let delegate = HookRuntimeDelegate::new(hook_runtime.clone());

        let first = delegate
            .before_stop(
                agentdash_spi::BeforeStopInput {
                    context: AgentContext {
                        system_prompt: "test".to_string(),
                        messages: vec![],
                        message_refs: vec![],
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
                        .is_some_and(|value| value.contains("pending companion"))
                );
            }
            StopDecision::Stop => panic!("存在 blocking_review action 时不应允许 stop"),
        }

        let action = hook_runtime
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
                        message_refs: vec![],
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
        let hook_runtime = Arc::new(TestHookRuntime::new_test_runtime(
            "sess-hook".to_string(),
            Arc::new(CompletionBlockedProvider),
            AgentFrameHookSnapshot {
                runtime_adapter_session_id: "sess-hook".to_string(),
                ..AgentFrameHookSnapshot::default()
            },
        ));
        let delegate = HookRuntimeDelegate::new(hook_runtime);

        let result = delegate
            .before_stop(
                agentdash_spi::BeforeStopInput {
                    context: AgentContext {
                        system_prompt: "test".to_string(),
                        messages: vec![],
                        message_refs: vec![],
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
        let snapshot = provider
            .load_frame_snapshot(AgentFrameHookSnapshotQuery {
                target: HookControlTarget {
                    run_id: uuid::Uuid::nil(),
                    agent_id: uuid::Uuid::nil(),
                    frame_id: uuid::Uuid::nil(),
                },
                provenance: agentdash_spi::hooks::RuntimeAdapterProvenance::runtime_session(
                    "sess-hook".to_string(),
                    None,
                    "test",
                ),
            })
            .await
            .expect("snapshot should load");
        let hook_runtime = Arc::new(TestHookRuntime::new_test_runtime(
            "sess-hook".to_string(),
            Arc::new(provider.clone()),
            snapshot,
        ));
        hook_runtime.update_token_stats(ContextTokenStats {
            last_input_tokens: 50_000,
            current_context_tokens: 50_000,
            pending_estimate_tokens: 0,
            context_window: 64_000,
            effective_context_window: 64_000,
            reserve_tokens: 16_384,
        });
        let delegate = HookRuntimeDelegate::new(hook_runtime);

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
                                    cache_read_input: 0,
                                    cache_creation_input: 0,
                                    output: 1_200,
                                }),
                                timestamp: None,
                            },
                        ],
                        message_refs: vec![],
                        tools: vec![],
                    },
                    provider_visible: Some(agentdash_spi::ProviderVisibleContextStats {
                        system_prompt_len: 4,
                        message_count: 2,
                        tool_count: 0,
                        estimated_input_tokens: 50_000,
                    }),
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
    async fn before_provider_request_uses_runtime_context_window_when_input_is_zero() {
        let provider = RecordingCompactionProvider::default();
        let snapshot = provider
            .load_frame_snapshot(AgentFrameHookSnapshotQuery {
                target: HookControlTarget {
                    run_id: uuid::Uuid::nil(),
                    agent_id: uuid::Uuid::nil(),
                    frame_id: uuid::Uuid::nil(),
                },
                provenance: agentdash_spi::hooks::RuntimeAdapterProvenance::runtime_session(
                    "sess-hook".to_string(),
                    None,
                    "test",
                ),
            })
            .await
            .expect("snapshot should load");
        let hook_runtime = Arc::new(TestHookRuntime::new_test_runtime(
            "sess-hook".to_string(),
            Arc::new(provider.clone()),
            snapshot,
        ));
        let delegate = HookRuntimeDelegate::new(hook_runtime.clone());

        delegate
            .on_before_provider_request(
                BeforeProviderRequestInput {
                    system_prompt_len: 12,
                    message_count: 3,
                    tool_count: 2,
                    estimated_input_tokens: 42_000,
                    context_window: 0,
                    reserve_tokens: 0,
                },
                CancellationToken::new(),
            )
            .await
            .expect("before provider request should be observed");

        let stats = hook_runtime.token_stats();
        assert_eq!(stats.last_input_tokens, 42_000);
        assert_eq!(stats.current_context_tokens, 42_000);
        assert_eq!(stats.context_window, 64_000);
        assert_eq!(stats.effective_context_window, 64_000);
        assert_eq!(
            provider
                .triggers
                .lock()
                .expect("triggers lock poisoned")
                .last(),
            Some(&HookTrigger::BeforeProviderRequest)
        );
    }

    #[tokio::test]
    async fn after_compaction_emits_after_compact_hook_payload() {
        let provider = RecordingCompactionProvider::default();
        let snapshot = provider
            .load_frame_snapshot(AgentFrameHookSnapshotQuery {
                target: HookControlTarget {
                    run_id: uuid::Uuid::nil(),
                    agent_id: uuid::Uuid::nil(),
                    frame_id: uuid::Uuid::nil(),
                },
                provenance: agentdash_spi::hooks::RuntimeAdapterProvenance::runtime_session(
                    "sess-hook".to_string(),
                    None,
                    "test",
                ),
            })
            .await
            .expect("snapshot should load");
        let hook_runtime = Arc::new(TestHookRuntime::new_test_runtime(
            "sess-hook".to_string(),
            Arc::new(provider.clone()),
            snapshot,
        ));
        let delegate = HookRuntimeDelegate::new(hook_runtime);

        delegate
            .after_compaction(
                CompactionResult {
                    messages: vec![AgentMessage::compaction_summary("summary body", 48_000, 6)],
                    message_refs: vec![None],
                    summary_message: AgentMessage::compaction_summary("summary body", 48_000, 6),
                    compacted_until_ref: agentdash_spi::MessageRef {
                        turn_id: "turn-1".to_string(),
                        entry_index: 0,
                    },
                    first_kept_ref: None,
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
    async fn repeated_compaction_failures_fuse_future_auto_compaction() {
        let provider = RecordingCompactionProvider::default();
        let snapshot = provider
            .load_frame_snapshot(AgentFrameHookSnapshotQuery {
                target: HookControlTarget {
                    run_id: uuid::Uuid::nil(),
                    agent_id: uuid::Uuid::nil(),
                    frame_id: uuid::Uuid::nil(),
                },
                provenance: agentdash_spi::hooks::RuntimeAdapterProvenance::runtime_session(
                    "sess-hook".to_string(),
                    None,
                    "test",
                ),
            })
            .await
            .expect("snapshot should load");
        let hook_runtime = Arc::new(TestHookRuntime::new_test_runtime(
            "sess-hook".to_string(),
            Arc::new(provider.clone()),
            snapshot,
        ));
        let delegate = HookRuntimeDelegate::new(hook_runtime.clone());

        for index in 1..=3 {
            delegate
                .after_compaction_failed(
                    CompactionFailureInput {
                        item_id: format!("compact-{index}"),
                        error: "summary_empty".to_string(),
                    },
                    CancellationToken::new(),
                )
                .await
                .expect("failure should be recorded");
        }

        let decision = delegate
            .evaluate_compaction(
                agentdash_spi::EvaluateCompactionInput {
                    context: AgentContext {
                        system_prompt: "test".to_string(),
                        messages: vec![AgentMessage::user("旧消息")],
                        message_refs: vec![],
                        tools: vec![],
                    },
                    provider_visible: Some(agentdash_spi::ProviderVisibleContextStats {
                        system_prompt_len: 4,
                        message_count: 1,
                        tool_count: 0,
                        estimated_input_tokens: 50_000,
                    }),
                },
                CancellationToken::new(),
            )
            .await
            .expect("evaluate_compaction should succeed");

        assert!(decision.is_none(), "连续失败后不应继续触发自动压缩");
        assert_eq!(hook_runtime.compaction_failure_count(), 3);
        assert!(
            provider
                .triggers
                .lock()
                .expect("triggers lock poisoned")
                .is_empty(),
            "熔断后不应继续执行 before_compact hook"
        );
        assert!(
            hook_runtime
                .diagnostics()
                .iter()
                .any(|entry| entry.code == "context_compaction_fused")
        );

        delegate
            .after_compaction(
                CompactionResult {
                    messages: vec![AgentMessage::compaction_summary("summary body", 48_000, 6)],
                    message_refs: vec![None],
                    summary_message: AgentMessage::compaction_summary("summary body", 48_000, 6),
                    compacted_until_ref: agentdash_spi::MessageRef {
                        turn_id: "turn-1".to_string(),
                        entry_index: 0,
                    },
                    first_kept_ref: None,
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
            .expect("success should reset failure fuse");

        assert_eq!(hook_runtime.compaction_failure_count(), 0);
    }

    #[tokio::test]
    async fn transform_context_deduplicates_static_companion_injection_trace() {
        let hook_runtime = Arc::new(TestHookRuntime::new_test_runtime(
            "sess-hook".to_string(),
            Arc::new(StaticCompanionContextProvider),
            AgentFrameHookSnapshot {
                runtime_adapter_session_id: "sess-hook".to_string(),
                ..AgentFrameHookSnapshot::default()
            },
        ));
        let delegate = HookRuntimeDelegate::new(hook_runtime.clone());

        let input = agentdash_spi::TransformContextInput {
            context: AgentContext {
                system_prompt: "test".to_string(),
                messages: vec![AgentMessage::user("hello")],
                message_refs: vec![],
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

        // 通用 injections 不再走 inline user message，只保留原始用户输入。
        assert_eq!(first.steering_messages.len(), 1);
        assert_eq!(second.steering_messages.len(), 1);

        let submit_traces = hook_runtime
            .runtime_snapshot()
            .trace
            .into_iter()
            .filter(|trace| matches!(trace.trigger, HookTraceTrigger::UserPromptSubmit))
            .collect::<Vec<_>>();
        assert_eq!(
            submit_traces.len(),
            1,
            "static companion injection should not produce duplicate trace events"
        );
        assert_eq!(submit_traces[0].decision, "context_injected");
    }

    #[tokio::test]
    async fn transform_context_consumes_turn_start_notices_once() {
        let hook_runtime = Arc::new(TestHookRuntime::new_test_runtime(
            "sess-hook".to_string(),
            Arc::new(NoopExecutionHookProvider),
            AgentFrameHookSnapshot {
                runtime_adapter_session_id: "sess-hook".to_string(),
                ..AgentFrameHookSnapshot::default()
            },
        ));
        hook_runtime.enqueue_turn_start_notice(HookTurnStartNotice {
            id: "notice-1".to_string(),
            created_at_ms: 1,
            source: RuntimeEventSource::RuntimeContextUpdate,
            content: "## Capability Update\n- tool schema refreshed".to_string(),
            context_frame: None,
        });
        let delegate = HookRuntimeDelegate::new(hook_runtime.clone());
        let input = agentdash_spi::TransformContextInput {
            context: AgentContext {
                system_prompt: "test".to_string(),
                messages: vec![AgentMessage::user("hello")],
                message_refs: vec![],
                tools: vec![],
            },
        };

        let first = delegate
            .transform_context(input.clone(), CancellationToken::new())
            .await
            .expect("first transform_context should succeed");
        let first_text = first
            .steering_messages
            .iter()
            .filter_map(|message| match message {
                AgentMessage::User { content, .. } => Some(
                    content
                        .iter()
                        .filter_map(|part| part.extract_text())
                        .collect::<Vec<_>>()
                        .join("\n"),
                ),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n");
        assert!(first_text.contains("notice-1"));
        assert!(first_text.contains("tool schema refreshed"));

        let second = delegate
            .transform_context(input, CancellationToken::new())
            .await
            .expect("second transform_context should succeed");
        let second_text = second
            .steering_messages
            .iter()
            .filter_map(|message| match message {
                AgentMessage::User { content, .. } => Some(
                    content
                        .iter()
                        .filter_map(|part| part.extract_text())
                        .collect::<Vec<_>>()
                        .join("\n"),
                ),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n");
        assert!(!second_text.contains("notice-1"));
    }

    #[tokio::test]
    async fn transform_context_injects_project_mcp_tool_schema_prompt_text() {
        let hook_runtime = Arc::new(TestHookRuntime::new_test_runtime(
            "sess-hook".to_string(),
            Arc::new(NoopExecutionHookProvider),
            AgentFrameHookSnapshot {
                runtime_adapter_session_id: "sess-hook".to_string(),
                ..AgentFrameHookSnapshot::default()
            },
        ));
        hook_runtime.enqueue_turn_start_notice(HookTurnStartNotice {
            id: "notice-mcp-schema".to_string(),
            created_at_ms: 1,
            source: RuntimeEventSource::RuntimeContextUpdate,
            content: [
                "## Tool Schema Delta",
                "### `mcp_code_analyzer_scan_repo`",
                "capability: `mcp:code-analyzer`；source: `mcp:code-analyzer`；path: `mcp:code-analyzer::scan_repo`",
                "扫描仓库结构",
                "参数说明：",
                "- `root` (required, string): 扫描根目录",
            ]
            .join("\n\n"),
            context_frame: None,
        });
        let delegate = HookRuntimeDelegate::new(hook_runtime);

        let output = delegate
            .transform_context(
                agentdash_spi::TransformContextInput {
                    context: AgentContext {
                        system_prompt: "test".to_string(),
                        messages: vec![AgentMessage::user("hello")],
                        message_refs: vec![],
                        tools: vec![],
                    },
                },
                CancellationToken::new(),
            )
            .await
            .expect("transform_context should succeed");

        let injected_text = output
            .steering_messages
            .iter()
            .filter_map(|message| match message {
                AgentMessage::User { content, .. } => Some(
                    content
                        .iter()
                        .filter_map(|part| part.extract_text())
                        .collect::<Vec<_>>()
                        .join("\n"),
                ),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n");

        assert!(injected_text.contains("notice-mcp-schema"));
        assert!(injected_text.contains("mcp_code_analyzer_scan_repo"));
        assert!(injected_text.contains("source: `mcp:code-analyzer`"));
        assert!(injected_text.contains("path: `mcp:code-analyzer::scan_repo`"));
        assert!(injected_text.contains("`root` (required, string)"));
    }

    #[tokio::test]
    async fn transform_context_emits_hook_injection_fragments_to_audit_bus() {
        let hook_runtime = Arc::new(TestHookRuntime::new_test_runtime(
            "sess-hook".to_string(),
            Arc::new(StaticCompanionContextProvider),
            AgentFrameHookSnapshot {
                runtime_adapter_session_id: "sess-hook".to_string(),
                ..AgentFrameHookSnapshot::default()
            },
        ));
        let audit_bus: SharedContextAuditBus = Arc::new(InMemoryContextAuditBus::new(100));
        let delegate = HookRuntimeDelegate::new_with_mount_root_and_audit(
            hook_runtime,
            None,
            Some(audit_bus.clone()),
        );

        delegate
            .transform_context(
                agentdash_spi::TransformContextInput {
                    context: AgentContext {
                        system_prompt: "test".to_string(),
                        messages: vec![AgentMessage::user("hello")],
                        message_refs: vec![],
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

    #[tokio::test]
    async fn after_turn_does_not_emit_inline_hook_steering_or_trace_injections() {
        let hook_runtime = Arc::new(TestHookRuntime::new_test_runtime(
            "sess-hook".to_string(),
            Arc::new(AfterTurnInjectionProvider),
            AgentFrameHookSnapshot {
                runtime_adapter_session_id: "sess-hook".to_string(),
                ..AgentFrameHookSnapshot::default()
            },
        ));
        let delegate = HookRuntimeDelegate::new(hook_runtime.clone());

        let result = delegate
            .after_turn(
                agentdash_spi::AfterTurnInput {
                    context: AgentContext {
                        system_prompt: "test".to_string(),
                        messages: vec![],
                        message_refs: vec![],
                        tools: vec![],
                    },
                    message: AgentMessage::assistant("ok"),
                    tool_results: vec![],
                },
                CancellationToken::new(),
            )
            .await
            .expect("after_turn should succeed");

        assert!(
            result.steering.is_empty(),
            "after_turn 不应再输出通用 inline hook 注入",
        );

        let trace = hook_runtime
            .runtime_snapshot()
            .trace
            .into_iter()
            .find(|entry| matches!(entry.trigger, HookTraceTrigger::AfterTurn))
            .expect("should record after_turn trace");
        assert!(
            trace.injections.is_empty(),
            "after_turn trace 不应携带通用注入内容",
        );
    }

    #[tokio::test]
    async fn after_turn_routes_hook_injections_through_runtime_sink() {
        let hook_runtime = Arc::new(TestHookRuntime::new_test_runtime(
            "sess-hook".to_string(),
            Arc::new(AfterTurnInjectionProvider),
            AgentFrameHookSnapshot {
                runtime_adapter_session_id: "sess-hook".to_string(),
                ..AgentFrameHookSnapshot::default()
            },
        ));
        let sink = Arc::new(RecordingInjectionSink::default());
        let delegate = HookRuntimeDelegate::new_with_mount_root_audit_and_sink(
            hook_runtime,
            None,
            None,
            Some(sink.clone()),
        );

        delegate
            .after_turn(
                agentdash_spi::AfterTurnInput {
                    context: AgentContext {
                        system_prompt: "test".to_string(),
                        messages: vec![],
                        message_refs: vec![],
                        tools: vec![],
                    },
                    message: AgentMessage::assistant("ok"),
                    tool_results: vec![],
                },
                CancellationToken::new(),
            )
            .await
            .expect("after_turn should succeed");

        let records = sink.records.lock().expect("recording sink lock poisoned");
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].0, "sess-hook");
        assert!(matches!(
            records[0].1,
            RuntimeInjectionSource::Hook(HookTrigger::AfterTurn)
        ));
        assert_eq!(records[0].2.len(), 1);
        assert_eq!(records[0].2[0].slot, "workflow");
        assert!(records[0].2[0].content.contains("phase: project_agent"));
    }

    #[test]
    fn pending_action_message_does_not_reference_specific_tools() {
        let snapshot = AgentFrameHookSnapshot {
            runtime_adapter_session_id: "sess-hook".to_string(),
            ..AgentFrameHookSnapshot::default()
        };
        let runtime = TestHookRuntime::new_test_runtime(
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
            source: RuntimeEventSource::CompanionResult,
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

        let frame = super::build_pending_action_context_frame(&snapshot, &action, &runtime)
            .expect("应该生成 pending action context frame");
        let notice = HookTurnStartNotice {
            id: frame.id.clone(),
            created_at_ms: frame.created_at_ms,
            source: frame.source.clone(),
            content: frame.rendered_text.clone(),
            context_frame: Some(frame.clone()),
        };
        let text = super::format_turn_start_notice_frame(&notice, &frame.rendered_text);

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
        assert!(
            text.contains("kind: pending_action"),
            "消息应包含 pending_action kind"
        );
    }
}
