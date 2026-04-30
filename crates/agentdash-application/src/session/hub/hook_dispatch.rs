//! Hub 的 hook 调度职责。
//!
//! 集中：
//! - `emit_session_hook_trigger`（从 `session/event_bridge.rs` 迁入，顺手删 `_tx` 占位）
//! - `ensure_hook_session_runtime`（按需懒重建 hook snapshot runtime）
//! - `emit_capability_changed_hook`（PhaseNode 等动态能力更新）
//! - `schedule_hook_auto_resume`（hook 级 auto-resume，经 augmenter 后转 prompt）

use std::sync::Arc;

use agentdash_acp_meta::AgentDashSourceV1;
use agentdash_spi::hooks::{
    HookEffect, HookEvaluationQuery, HookSessionRuntimeAccess, HookTraceEntry, HookTrigger,
    SessionHookRefreshQuery, SessionHookSnapshotQuery, SharedHookSessionRuntime,
};
use agentdash_spi::ConnectorError;
use tokio::sync::broadcast;

use super::super::hook_events::build_hook_trace_notification;
use super::super::hook_messages as msg;
use super::super::hook_runtime::HookSessionRuntime;
use super::super::hub_support::{build_session_runtime, session_hook_trace_decision};
use super::super::types::{PromptSessionRequest, UserPromptInput};
use super::SessionHub;

/// `emit_session_hook_trigger` 的入参（在 hub 内部多处构造，故暴露给 super）。
pub(in crate::session) struct HookTriggerInput<'a> {
    pub session_id: &'a str,
    pub turn_id: Option<&'a str>,
    pub trigger: HookTrigger,
    pub payload: Option<serde_json::Value>,
    pub refresh_reason: &'static str,
    pub source: AgentDashSourceV1,
}

impl SessionHub {
    /// 评估 session hook 并广播 trace 事件。返回 hook 产出的 effects 列表，
    /// 由调用方决定是否/如何执行这些副作用。
    ///
    /// 注：原 `event_bridge::emit_session_hook_trigger` 的 `_tx` 占位参数
    /// （broadcast Sender）始终未被使用，PR 6 清理时一并删除。
    pub(in crate::session) async fn emit_session_hook_trigger(
        &self,
        hook_session: &dyn HookSessionRuntimeAccess,
        input: &HookTriggerInput<'_>,
    ) -> Vec<HookEffect> {
        let HookTriggerInput {
            session_id,
            turn_id,
            ref trigger,
            ref payload,
            refresh_reason,
            ref source,
        } = *input;
        match hook_session
            .evaluate(HookEvaluationQuery {
                session_id: session_id.to_string(),
                trigger: trigger.clone(),
                turn_id: turn_id.map(ToString::to_string),
                tool_name: None,
                tool_call_id: None,
                subagent_type: None,
                snapshot: Some(hook_session.snapshot()),
                payload: payload.clone(),
                token_stats: None,
            })
            .await
        {
            Ok(resolution) => {
                if resolution.refresh_snapshot {
                    let _ = hook_session
                        .refresh(SessionHookRefreshQuery {
                            session_id: session_id.to_string(),
                            turn_id: turn_id.map(ToString::to_string),
                            reason: Some(refresh_reason.to_string()),
                        })
                        .await;
                }
                let effects = resolution.effects.clone();
                let trace = HookTraceEntry {
                    sequence: hook_session.next_trace_sequence(),
                    timestamp_ms: chrono::Utc::now().timestamp_millis(),
                    revision: hook_session.revision(),
                    trigger: trigger.clone(),
                    decision: session_hook_trace_decision(trigger, &resolution).to_string(),
                    tool_name: None,
                    tool_call_id: None,
                    subagent_type: None,
                    matched_rule_keys: resolution.matched_rule_keys,
                    refresh_snapshot: resolution.refresh_snapshot,
                    block_reason: resolution.block_reason,
                    completion: resolution.completion,
                    diagnostics: resolution.diagnostics,
                    injections: resolution.injections,
                };
                hook_session.append_trace(trace.clone());
                if let Some(notification) =
                    build_hook_trace_notification(session_id, turn_id, source.clone(), &trace)
                {
                    let _ = self.persist_notification(session_id, notification).await;
                }
                effects
            }
            Err(error) => {
                tracing::warn!(
                    session_id = %session_id,
                    trigger = ?trigger,
                    error = %error,
                    "session hook 评估失败"
                );
                Vec::new()
            }
        }
    }

    /// 触发 `CapabilityChanged` hook（PhaseNode 等动态能力更新路径使用）。
    pub async fn emit_capability_changed_hook(
        &self,
        session_id: &str,
        turn_id: Option<&str>,
        payload: serde_json::Value,
    ) {
        let hook_session = {
            let sessions = self.sessions.lock().await;
            let Some(runtime) = sessions.get(session_id) else {
                return;
            };
            let Some(hook_session) = runtime.hook_session.clone() else {
                return;
            };
            hook_session
        };

        let connector_type = match self.connector.connector_type() {
            agentdash_spi::ConnectorType::LocalExecutor => "local_executor",
            agentdash_spi::ConnectorType::RemoteAcpBackend => "remote_acp_backend",
        };
        let source = AgentDashSourceV1::new(self.connector.connector_id(), connector_type);

        let _ = self
            .emit_session_hook_trigger(
                hook_session.as_ref(),
                &HookTriggerInput {
                    session_id,
                    turn_id,
                    trigger: HookTrigger::CapabilityChanged,
                    payload: Some(payload),
                    refresh_reason: "trigger:capability_changed",
                    source,
                },
            )
            .await;
    }

    pub async fn ensure_hook_session_runtime(
        &self,
        session_id: &str,
        turn_id: Option<&str>,
    ) -> Result<Option<SharedHookSessionRuntime>, ConnectorError> {
        {
            let sessions = self.sessions.lock().await;
            if let Some(runtime) = sessions
                .get(session_id)
                .and_then(|runtime| runtime.hook_session.clone())
            {
                return Ok(Some(runtime));
            }
        }

        if self
            .persistence
            .get_session_meta(session_id)
            .await?
            .is_none()
        {
            return Ok(None);
        }

        let Some(provider) = self.hook_provider.as_ref() else {
            return Ok(None);
        };

        let snapshot = provider
            .load_session_snapshot(SessionHookSnapshotQuery {
                session_id: session_id.to_string(),
                turn_id: turn_id.map(ToString::to_string),
            })
            .await
            .map_err(|error| {
                ConnectorError::Runtime(format!("重建会话 Hook snapshot 失败: {error}"))
            })?;

        let rebuilt_runtime = Arc::new(HookSessionRuntime::new(
            session_id.to_string(),
            provider.clone(),
            snapshot,
        ));

        let mut sessions = self.sessions.lock().await;
        let runtime = sessions.entry(session_id.to_string()).or_insert_with(|| {
            let (tx, _rx) = broadcast::channel(1024);
            build_session_runtime(tx)
        });
        if runtime.hook_session.is_none() {
            runtime.hook_session = Some(rebuilt_runtime.clone());
        }
        Ok(runtime.hook_session.clone())
    }

    /// Hook auto-resume: schedule a delayed follow-up prompt in a separate task.
    /// Uses fire-and-forget to avoid awaiting `start_prompt` directly inside
    /// the stream-processing spawn block (whose Future is not Send).
    ///
    /// **关键对齐**：auto-resume 与 HTTP 主通道必须经过同一条 augmenter，
    /// 否则 owner context / MCP / flow_capabilities / context_bundle 会漂移，
    /// Agent 失去工作流背景 → 复读上一轮。因此这里先从 hub 拿 augmenter 增强，
    /// 再调 `start_prompt`；若未注入 augmenter（测试 / 非正式 embedding 场景）
    /// 也不致命，但会打 warn，提示运营侧补齐。
    pub(crate) fn schedule_hook_auto_resume(&self, session_id: String) {
        let hub = self.clone();
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            let bare_req = PromptSessionRequest::from_user_input(UserPromptInput::from_text(
                msg::AUTO_RESUME_PROMPT,
            ));

            let req = match hub
                .augment_prompt_request(&session_id, bare_req, "hook_auto_resume")
                .await
            {
                Ok(augmented) => augmented,
                Err(e) => {
                    tracing::warn!(
                        session_id = %session_id,
                        error = %e,
                        "Hook auto-resume: augment 失败，跳过本次续跑以避免发送裸请求"
                    );
                    return;
                }
            };

            if let Err(e) = hub.start_prompt(&session_id, req).await {
                tracing::warn!(
                    session_id = %session_id,
                    error = %e,
                    "Hook auto-resume failed"
                );
            }
        });
    }
}

