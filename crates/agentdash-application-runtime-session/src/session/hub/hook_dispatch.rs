//! Hub 的 hook 调度职责。
//!
//! 集中：
//! - `emit_session_hook_trigger`（从 `session/event_bridge.rs` 迁入，顺手删 `_tx` 占位）
//! - `ensure_hook_runtime`（按需懒重建 hook snapshot runtime）
//! - `collect_runtime_context_update_injections`（PhaseNode 等 runtime context 更新）

use super::super::hook_events::build_hook_trace_envelope;
use super::super::hook_injection_sink::{
    RuntimeHookInjectionSink, RuntimeInjectionSource, SessionRuntimeHookInjectionSink,
};
use super::super::hub_support::session_hook_trace_decision;
use super::SessionRuntimeInner;
use crate::session::terminal_boundary::{TerminalHookTriggerPort, TerminalHookTriggerRequest};
use agentdash_agent_protocol::SourceInfo;
use agentdash_diagnostics::{DiagnosticErrorContext, Subsystem, diag_error};
use agentdash_spi::hooks::SharedHookRuntime;
use agentdash_spi::hooks::{
    HookEffect, HookInjection, HookRuntimeAccess, HookRuntimeEvaluationQuery,
    HookRuntimeRefreshQuery, HookTraceEntry, HookTrigger, RuntimeAdapterProvenance,
};

/// `emit_session_hook_trigger` 的入参（在 hub 内部多处构造，故暴露给 super）。
pub(crate) struct HookTriggerInput<'a> {
    pub session_id: &'a str,
    pub turn_id: Option<&'a str>,
    pub trigger: HookTrigger,
    pub payload: Option<serde_json::Value>,
    pub refresh_reason: &'static str,
    pub source: SourceInfo,
}

/// Hook trigger 调度结果。
///
/// `effects` 仍交由原调用点处理；`injections` 已由统一调度路径回灌
/// runtime 注入存储 / audit，返回值仅用于调用点执行 hook effect。
#[derive(Debug, Clone, Default)]
pub(crate) struct HookTriggerDispatchResult {
    pub effects: Vec<HookEffect>,
}

impl SessionRuntimeInner {
    /// 评估 session hook 并广播 trace 事件。返回 hook 产出的 effects 列表，
    /// 由调用方决定是否/如何执行这些副作用。
    ///
    /// 注：原 `event_bridge::emit_session_hook_trigger` 的 `_tx` 占位参数
    /// （broadcast Sender）始终未被使用，PR 6 清理时一并删除。
    pub(in crate::session) async fn emit_session_hook_trigger(
        &self,
        hook_runtime: &dyn HookRuntimeAccess,
        input: &HookTriggerInput<'_>,
    ) -> HookTriggerDispatchResult {
        let HookTriggerInput {
            session_id,
            turn_id,
            ref trigger,
            ref payload,
            refresh_reason,
            ref source,
        } = *input;
        let turn_id_value = turn_id.map(ToString::to_string);
        match hook_runtime
            .evaluate_from_provenance(HookRuntimeEvaluationQuery {
                provenance: RuntimeAdapterProvenance::runtime_session(
                    hook_runtime.session_id().to_string(),
                    turn_id_value.clone(),
                    "hub_hook_trigger",
                ),
                trigger: *trigger,
                tool_name: None,
                tool_call_id: None,
                subagent_type: None,
                snapshot: Some(hook_runtime.snapshot()),
                payload: payload.clone(),
                token_stats: None,
            })
            .await
        {
            Ok(resolution) => {
                if resolution.refresh_snapshot
                    && let Err(error) = hook_runtime
                        .refresh_from_provenance(HookRuntimeRefreshQuery {
                            provenance: RuntimeAdapterProvenance::runtime_session(
                                hook_runtime.session_id().to_string(),
                                turn_id_value.clone(),
                                "hub_hook_refresh",
                            ),
                            reason: Some(refresh_reason.to_string()),
                        })
                        .await
                {
                    let context =
                        DiagnosticErrorContext::new("session.hook_dispatch", "refresh_snapshot");
                    diag_error!(
                        Warn,
                        Subsystem::Hooks,
                        context = &context,
                        error = &error,
                        session_id = %session_id,
                        turn_id = %turn_id_value.as_deref().unwrap_or(""),
                        trigger = ?trigger,
                        refresh_reason,
                        "session hook snapshot refresh 失败"
                    );
                }
                let effects = resolution.effects.clone();
                let trace_injections = resolution.injections.clone();
                if let Some(trace_trigger) = trigger.trace_trigger() {
                    let trace = HookTraceEntry {
                        sequence: hook_runtime.next_trace_sequence(),
                        timestamp_ms: chrono::Utc::now().timestamp_millis(),
                        revision: hook_runtime.revision(),
                        trigger: trace_trigger,
                        decision: session_hook_trace_decision(trigger, &resolution).to_string(),
                        tool_name: None,
                        tool_call_id: None,
                        subagent_type: None,
                        matched_rule_keys: resolution.matched_rule_keys,
                        refresh_snapshot: resolution.refresh_snapshot,
                        effects_applied: !effects.is_empty(),
                        block_reason: resolution.block_reason,
                        completion: resolution.completion,
                        diagnostics: resolution.diagnostics,
                        injections: trace_injections.clone(),
                    };
                    hook_runtime.append_trace(trace.clone());
                    // 活跃 in-process connector 会通过 trace_broadcast → hook_trace_rx
                    // 把同一条 trace 发回 turn stream。Hub 侧只在没有 live runtime
                    // 时兜底持久化，避免前端出现重复 Hook 卡片。
                    if !self.has_live_executor_session(session_id).await
                        && let Some(envelope) =
                            build_hook_trace_envelope(session_id, turn_id, source.clone(), &trace)
                        && let Err(error) = self.persist_notification(session_id, envelope).await
                    {
                        let context = DiagnosticErrorContext::new(
                            "session.hook_dispatch",
                            "persist_fallback_hook_trace",
                        );
                        diag_error!(
                            Warn,
                            Subsystem::Hooks,
                            context = &context,
                            error = &error,
                            session_id = %session_id,
                            turn_id = %turn_id_value.as_deref().unwrap_or(""),
                            trigger = ?trigger,
                            trace_sequence = trace.sequence,
                            "fallback hook trace 持久化失败"
                        );
                    }
                }
                if !trace_injections.is_empty() {
                    let sink = SessionRuntimeHookInjectionSink::new(
                        self.runtime_registry.clone(),
                        self.current_context_audit_bus().await,
                    );
                    let target = hook_runtime.control_target();
                    sink.emit_injections(
                        &target.run_id.to_string(),
                        &target.agent_id.to_string(),
                        session_id,
                        RuntimeInjectionSource::Hook(*trigger),
                        &trace_injections,
                    )
                    .await;
                }
                HookTriggerDispatchResult { effects }
            }
            Err(error) => {
                let context = DiagnosticErrorContext::new("session.hook_dispatch", "evaluate_hook");
                diag_error!(
                    Warn,
                    Subsystem::Hooks,
                    context = &context,
                    error = &error,
                    session_id = %session_id,
                    turn_id = %turn_id_value.as_deref().unwrap_or(""),
                    trigger = ?trigger,
                    "session hook 评估失败"
                );
                HookTriggerDispatchResult::default()
            }
        }
    }

    /// 收集 runtime context update 对应的动态 injections。
    ///
    /// 这不是 Agent 生命周期 hook，不写 HookTrace，也不走 HookTrigger。PhaseNode 等
    /// transition 只把最新 snapshot 中的 workflow/context 注入回灌到 bundle/audit，
    /// Agent 可见文本由 turn-start notice 队列在 `transform_context` 边界统一消费。
    pub(crate) async fn collect_runtime_context_update_injections(
        &self,
        session_id: &str,
        hook_runtime: &SharedHookRuntime,
    ) -> Vec<HookInjection> {
        let injections = hook_runtime.snapshot().injections;
        if injections.is_empty() {
            return Vec::new();
        }

        let sink = SessionRuntimeHookInjectionSink::new(
            self.runtime_registry.clone(),
            self.current_context_audit_bus().await,
        );
        let target = hook_runtime.control_target();
        sink.emit_injections(
            &target.run_id.to_string(),
            &target.agent_id.to_string(),
            session_id,
            RuntimeInjectionSource::RuntimeContextUpdate,
            &injections,
        )
        .await;
        injections
    }
}

#[async_trait::async_trait]
impl TerminalHookTriggerPort for SessionRuntimeInner {
    async fn emit_terminal_hook_trigger(
        &self,
        hook_runtime: &dyn agentdash_spi::hooks::HookRuntimeAccess,
        input: TerminalHookTriggerRequest<'_>,
    ) -> Vec<HookEffect> {
        self.emit_session_hook_trigger(
            hook_runtime,
            &HookTriggerInput {
                session_id: input.session_id,
                turn_id: input.turn_id,
                trigger: input.trigger,
                payload: input.payload,
                refresh_reason: input.refresh_reason,
                source: input.source,
            },
        )
        .await
        .effects
    }
}
