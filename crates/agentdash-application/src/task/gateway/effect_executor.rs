use std::sync::Arc;

use agent_client_protocol::SessionNotification;
use agentdash_spi::hooks::HookEffect;
use uuid::Uuid;

use crate::repository_set::RepositorySet;
use crate::session::PostTurnHandler;
use crate::task::restart_tracker::{RestartDecision, RestartTracker};

use super::repo_ops::*;
use agentdash_domain::task::TaskStatus;

/// Task 领域的 PostTurnHandler 实现。
///
/// - `on_event`：处理 artifact 持久化等平台级簿记
/// - `execute_effects`：消费 Hook 产出的 `task:*` effects
///
/// 与 TurnMonitor 的区别：决策逻辑由 Hook 规则声明（Rhai），
/// 本执行器仅负责按声明执行数据库操作。
pub struct TaskHookEffectExecutor {
    pub repos: RepositorySet,
    pub restart_tracker: Arc<RestartTracker>,
    pub task_id: Uuid,
    pub session_id: String,
    pub backend_id: String,
}

#[async_trait::async_trait]
impl PostTurnHandler for TaskHookEffectExecutor {
    async fn on_event(&self, session_id: &str, notification: &SessionNotification) {
        self.handle_event(session_id, notification).await;
    }

    async fn execute_effects(&self, session_id: &str, turn_id: &str, effects: &[HookEffect]) {
        for effect in effects {
            if let Err(err) = self
                .dispatch_effect(session_id, turn_id, effect)
                .await
            {
                tracing::warn!(
                    task_id = %self.task_id,
                    effect_kind = %effect.kind,
                    error = %err,
                    "Task effect 执行失败"
                );
            }
        }
    }

    fn supported_effect_kinds(&self) -> &[&str] {
        Self::SUPPORTED_KINDS
    }
}

impl TaskHookEffectExecutor {
    /// 本 executor 能处理的 effect kinds。
    /// 任何不在此列表中的 kind 会产生运行时 warning。
    pub const SUPPORTED_KINDS: &[&str] = &[
        "task:set_status",
        "task:retry",
        "task:clear_binding",
    ];

    async fn handle_event(&self, _session_id: &str, notification: &SessionNotification) {
        use agent_client_protocol::SessionUpdate;
        use crate::task::artifact::{build_tool_call_patch, build_tool_call_update_patch};
        use crate::task::meta::extract_turn_id_from_meta;

        match &notification.update {
            SessionUpdate::ToolCall(tc) => {
                let turn_id = extract_turn_id_from_meta(tc.meta.as_ref()).unwrap_or_default();
                let patch = build_tool_call_patch(tc);
                let _ = persist_tool_call_artifact(
                    &self.repos,
                    ToolCallArtifactInput {
                        task_id: self.task_id,
                        session_id: &self.session_id,
                        turn_id: &turn_id,
                        tool_call_id: &tc.tool_call_id.to_string(),
                        patch,
                        backend_id: &self.backend_id,
                        reason: "hook_event_tool_call",
                    },
                )
                .await;
            }
            SessionUpdate::ToolCallUpdate(tcu) => {
                let turn_id = extract_turn_id_from_meta(tcu.meta.as_ref()).unwrap_or_default();
                let patch = build_tool_call_update_patch(tcu);
                let _ = persist_tool_call_artifact(
                    &self.repos,
                    ToolCallArtifactInput {
                        task_id: self.task_id,
                        session_id: &self.session_id,
                        turn_id: &turn_id,
                        tool_call_id: &tcu.tool_call_id.to_string(),
                        patch,
                        backend_id: &self.backend_id,
                        reason: "hook_event_tool_call_update",
                    },
                )
                .await;
            }
            _ => {}
        }
    }

    async fn dispatch_effect(
        &self,
        _session_id: &str,
        turn_id: &str,
        effect: &HookEffect,
    ) -> Result<(), String> {
        match effect.kind.as_str() {
            "task:set_status" => self.handle_set_status(turn_id, &effect.payload).await,
            "task:retry" => self.handle_retry(turn_id, &effect.payload).await,
            "task:clear_binding" => self.handle_clear_binding(&effect.payload).await,
            other => {
                tracing::warn!(
                    task_id = %self.task_id,
                    kind = other,
                    supported = ?Self::SUPPORTED_KINDS,
                    "Unhandled effect kind — 检查 Rhai 脚本是否拼写有误或需要新增 handler"
                );
                Ok(())
            }
        }
    }

    async fn handle_set_status(
        &self,
        turn_id: &str,
        payload: &serde_json::Value,
    ) -> Result<(), String> {
        let status_str = payload
            .get("status")
            .and_then(|v| v.as_str())
            .ok_or("task:set_status missing 'status' field")?;
        let reason = payload
            .get("reason")
            .and_then(|v| v.as_str())
            .unwrap_or("hook_effect");

        let next_status = match status_str {
            "completed" => TaskStatus::Completed,
            "awaiting_verification" => TaskStatus::AwaitingVerification,
            "failed" => TaskStatus::Failed,
            "running" => TaskStatus::Running,
            "pending" => TaskStatus::Pending,
            other => return Err(format!("Unknown task status: {other}")),
        };

        let context = serde_json::json!({
            "session_id": self.session_id,
            "turn_id": turn_id,
            "execution_mode": payload.get("execution_mode"),
            "reason": reason,
            "source": "hook_effect",
        });

        update_task_status(
            &self.repos,
            self.task_id,
            &self.backend_id,
            next_status,
            reason,
            context,
        )
        .await
        .map_err(|e| e.to_string())?;

        Ok(())
    }

    async fn handle_retry(
        &self,
        turn_id: &str,
        payload: &serde_json::Value,
    ) -> Result<(), String> {
        let reason = payload
            .get("reason")
            .and_then(|v| v.as_str())
            .unwrap_or("hook_retry");

        let decision = self.restart_tracker.report_failure(self.task_id);
        match decision {
            RestartDecision::Allowed { attempt, delay } => {
                tracing::info!(
                    task_id = %self.task_id,
                    attempt,
                    delay_ms = delay.as_millis() as u64,
                    reason,
                    "Hook effect: AutoRetry allowed"
                );
                let context = serde_json::json!({
                    "session_id": self.session_id,
                    "turn_id": turn_id,
                    "retry_attempt": attempt,
                    "retry_delay_ms": delay.as_millis() as u64,
                    "reason": reason,
                    "source": "hook_effect",
                });
                update_task_status(
                    &self.repos,
                    self.task_id,
                    &self.backend_id,
                    TaskStatus::AwaitingVerification,
                    &format!("{reason}_pending_retry"),
                    context,
                )
                .await
                .map_err(|e| e.to_string())?;
                Ok(())
            }
            RestartDecision::Denied {
                attempts_exhausted,
            } => {
                tracing::warn!(
                    task_id = %self.task_id,
                    attempts_exhausted,
                    reason,
                    "Hook effect: AutoRetry exhausted, marking Failed"
                );
                let context = serde_json::json!({
                    "session_id": self.session_id,
                    "turn_id": turn_id,
                    "attempts_exhausted": attempts_exhausted,
                    "reason": reason,
                    "source": "hook_effect",
                });
                update_task_status(
                    &self.repos,
                    self.task_id,
                    &self.backend_id,
                    TaskStatus::Failed,
                    &format!("{reason}_retry_exhausted"),
                    context,
                )
                .await
                .map_err(|e| e.to_string())?;
                Ok(())
            }
        }
    }

    async fn handle_clear_binding(
        &self,
        payload: &serde_json::Value,
    ) -> Result<(), String> {
        let reason = payload
            .get("reason")
            .and_then(|v| v.as_str())
            .unwrap_or("hook_clear_binding");

        clear_task_session_binding(&self.repos, self.task_id, &self.backend_id, reason).await;
        Ok(())
    }
}
