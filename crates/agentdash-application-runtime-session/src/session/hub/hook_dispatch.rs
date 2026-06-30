//! Hub 的 hook 调度职责。
//!
//! 集中：
//! - `emit_session_hook_trigger`（从 `session/event_bridge.rs` 迁入，顺手删 `_tx` 占位）
//! - `ensure_hook_runtime`（按需懒重建 hook snapshot runtime）
//! - `collect_runtime_context_update_injections`（PhaseNode 等 runtime context 更新）
//! - `schedule_unanchored_hook_auto_resume`（非 AgentRun runtime 的 hook auto-resume）

use super::super::auto_resume_context_frame::build_auto_resume_context_frame;
use super::super::hook_events::build_hook_trace_envelope;
use super::super::hook_injection_sink::{
    RuntimeHookInjectionSink, RuntimeInjectionSource, SessionRuntimeHookInjectionSink,
};
use super::super::hook_messages as msg;
use super::super::hub_support::session_hook_trace_decision;
use super::super::terminal_effects::{
    TerminalAutoResumePort, TerminalAutoResumeRequest, TerminalHookTriggerPort,
    TerminalHookTriggerRequest,
};
use super::SessionRuntimeInner;
use agentdash_agent_protocol::{SourceInfo, text_user_input_blocks};
use agentdash_application_ports::launch::{LaunchCommand, LaunchPlanningInput, LaunchPromptInput};
use agentdash_application_ports::runtime_session_live::RuntimeSessionMailboxAutoResumeRequest;
use agentdash_diagnostics::{Subsystem, diag};
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
                if resolution.refresh_snapshot {
                    let _ = hook_runtime
                        .refresh_from_provenance(HookRuntimeRefreshQuery {
                            provenance: RuntimeAdapterProvenance::runtime_session(
                                hook_runtime.session_id().to_string(),
                                turn_id_value.clone(),
                                "hub_hook_refresh",
                            ),
                            reason: Some(refresh_reason.to_string()),
                        })
                        .await;
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
                        block_reason: resolution.block_reason,
                        completion: resolution.completion,
                        diagnostics: resolution.diagnostics,
                        injections: trace_injections.clone(),
                    };
                    hook_runtime.append_trace(trace.clone());
                    // 活跃 in-process connector 会通过 trace_broadcast → hook_trace_rx
                    // 把同一条 trace 发回 turn stream。Hub 侧只在没有 live runtime
                    // 时兜底持久化，避免前端出现重复 Hook 卡片。
                    if !self.has_live_executor_session(session_id).await {
                        let envelope =
                            build_hook_trace_envelope(session_id, turn_id, source.clone(), &trace);
                        let _ = self.persist_notification(session_id, envelope).await;
                    }
                }
                if !trace_injections.is_empty() {
                    let sink = SessionRuntimeHookInjectionSink::new(
                        self.runtime_registry.clone(),
                        self.current_context_audit_bus().await,
                    );
                    sink.emit_injections(
                        session_id,
                        RuntimeInjectionSource::Hook(*trigger),
                        &trace_injections,
                    )
                    .await;
                }
                HookTriggerDispatchResult { effects }
            }
            Err(error) => {
                diag!(Warn, Subsystem::Hooks,

                    session_id = %session_id,
                    trigger = ?trigger,
                    error = %error,
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
        sink.emit_injections(
            session_id,
            RuntimeInjectionSource::RuntimeContextUpdate,
            &injections,
        )
        .await;
        injections
    }

    /// Processor 请求的 auto-resume 入口。
    ///
    /// PR 7c：把原先散落在 `turn_processor` 里的"计数检查 + 递增 + schedule"
    /// 三件事统一在这里处理。processor 只需发出"需要续跑"的信号，限流在 hub
    /// 侧完成，便于未来加全局限流 / per-executor 配额等策略。
    ///
    /// 返回值仅用于单测断言；业务路径 fire-and-forget。
    pub(in crate::session) async fn request_hook_auto_resume(
        &self,
        request: TerminalAutoResumeRequest,
    ) -> Result<bool, String> {
        const MAX_HOOK_AUTO_RESUMES: u32 = 2;
        let session_id = request.session_id.clone();

        // 原子：读取当前计数 + 若未超限则递增。
        let decision = self
            .runtime_registry
            .increment_auto_resume_if_allowed(&session_id, MAX_HOOK_AUTO_RESUMES)
            .await;

        if decision {
            diag!(Info, Subsystem::Hooks,

                session_id = %session_id,
                "Hook auto-resume: stop gate unsatisfied, scheduling retry"
            );
            match self.try_enqueue_hook_auto_resume_mailbox(&request).await {
                AutoResumeMailboxRoute::Routed => {}
                AutoResumeMailboxRoute::NoAnchor => {
                    self.schedule_unanchored_hook_auto_resume(session_id)
                }
                AutoResumeMailboxRoute::Failed(error) => {
                    self.runtime_registry
                        .release_auto_resume_reservation(&session_id)
                        .await;
                    return Err(error);
                }
            }
            Ok(true)
        } else {
            diag!(Warn, Subsystem::Hooks,

                session_id = %session_id,
                max = MAX_HOOK_AUTO_RESUMES,
                "Hook auto-resume: 达到上限，放弃续跑"
            );
            Ok(false)
        }
    }

    async fn try_enqueue_hook_auto_resume_mailbox(
        &self,
        request: &TerminalAutoResumeRequest,
    ) -> AutoResumeMailboxRoute {
        let Some(port) = self.mailbox_runtime_port.read().await.clone() else {
            return AutoResumeMailboxRoute::NoAnchor;
        };
        match port
            .accept_hook_auto_resume_effect(RuntimeSessionMailboxAutoResumeRequest {
                session_id: request.session_id.clone(),
                effect_id: request.effect_id,
                source_turn_id: request.turn_id.clone(),
                terminal_event_seq: request.terminal_event_seq,
                input: text_user_input_blocks(msg::AUTO_RESUME_PROMPT),
            })
            .await
        {
            Ok(true) => {
                if let Some(frame) = build_auto_resume_context_frame(
                    "hook_before_stop_continue",
                    msg::AUTO_RESUME_PROMPT,
                ) {
                    let _ = self
                        .emit_context_frame(&request.session_id, None, &frame)
                        .await;
                }
                AutoResumeMailboxRoute::Routed
            }
            Ok(false) => AutoResumeMailboxRoute::NoAnchor,
            Err(error) => {
                diag!(Warn, Subsystem::Hooks,

                    session_id = %request.session_id,
                    effect_id = %request.effect_id,
                    error = %error,
                    payload = ?request.payload,
                    "Hook auto-resume mailbox envelope 创建失败"
                );
                AutoResumeMailboxRoute::Failed(error.to_string())
            }
        }
    }

    /// Hook auto-resume: schedule a delayed follow-up prompt in a separate task.
    /// Uses fire-and-forget to avoid awaiting `start_prompt` directly inside
    /// the stream-processing spawn block (whose Future is not Send).
    ///
    /// **关键对齐**：auto-resume 与 HTTP 主通道必须经过同一条 provider，
    /// 否则 owner context / MCP / capability_state / context_bundle 会漂移，
    /// Agent 失去工作流背景 → 复读上一轮。因此这里固定走 strict launch：
    /// provider 缺失/失败时直接放弃本次 auto-resume，禁止裸请求降级。
    pub(crate) fn schedule_unanchored_hook_auto_resume(&self, session_id: String) {
        let hub = self.clone();
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            let command = LaunchCommand::hook_auto_resume_input(LaunchPromptInput::from_text(
                msg::AUTO_RESUME_PROMPT,
            ));
            if let Some(frame) = build_auto_resume_context_frame(
                "hook_before_stop_continue",
                msg::AUTO_RESUME_PROMPT,
            ) {
                let _ = hub.emit_context_frame(&session_id, None, &frame).await;
            }

            if let Err(e) = hub
                .launch_service()
                .launch_command(&session_id, command, LaunchPlanningInput::default())
                .await
            {
                diag!(Warn, Subsystem::Hooks,

                    session_id = %session_id,
                    error = %e,
                    "Hook auto-resume launch 失败"
                );
            }
        });
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

#[async_trait::async_trait]
impl TerminalAutoResumePort for SessionRuntimeInner {
    async fn request_hook_auto_resume(
        &self,
        request: TerminalAutoResumeRequest,
    ) -> Result<bool, String> {
        SessionRuntimeInner::request_hook_auto_resume(self, request).await
    }
}

enum AutoResumeMailboxRoute {
    Routed,
    NoAnchor,
    Failed(String),
}
