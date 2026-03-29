use serde_json::{Value, json};
use uuid::Uuid;

use agent_client_protocol::{SessionId, SessionInfoUpdate, SessionNotification, SessionUpdate};
use agentdash_domain::task::{TaskExecutionMode, TaskStatus};

use crate::repository_set::RepositorySet;
use crate::task::execution::{SessionOverview, TaskExecutionError};
use crate::task::meta::{build_task_lifecycle_meta, parse_turn_event, turn_matches};
use crate::task::artifact::{build_tool_call_patch, build_tool_call_update_patch};
use crate::task::restart_tracker::RestartDecision;

use super::repo_ops::*;

/// 整合 turn 处理中反复传递的标识 / 基础设施参数
pub struct TurnEventContext<'a> {
    pub repos: &'a RepositorySet,
    pub restart_tracker: &'a crate::task::restart_tracker::RestartTracker,
    pub task_id: Uuid,
    pub session_id: &'a str,
    pub turn_id: &'a str,
    pub backend_id: &'a str,
    pub execution_mode: &'a TaskExecutionMode,
}

/// Turn 监听处理结果
pub enum TurnOutcome {
    Continue,
    Completed,
    Failed,
    NeedsRetry {
        delay: std::time::Duration,
        attempt: u32,
    },
}

/// Turn 监听主循环 — 从 session_hub 订阅会话事件并逐条处理
pub async fn run_turn_monitor(
    repos: &RepositorySet,
    session_hub: &crate::session::SessionHub,
    restart_tracker: &crate::task::restart_tracker::RestartTracker,
    task_id: Uuid,
    session_id: &str,
    turn_id: &str,
    backend_id: &str,
) -> TurnOutcome {
    let execution_mode = match repos.task_repo.get_by_id(task_id).await {
        Ok(Some(task)) => task.execution_mode,
        _ => TaskExecutionMode::Standard,
    };

    let ctx = TurnEventContext {
        repos,
        restart_tracker,
        task_id,
        session_id,
        turn_id,
        backend_id,
        execution_mode: &execution_mode,
    };

    let (history, mut rx) = session_hub.subscribe_with_history(session_id).await;

    for notification in history {
        match handle_turn_notification(&ctx, session_hub, &notification).await {
            Ok(TurnOutcome::Continue) => {}
            Ok(outcome) => return outcome,
            Err(err) => {
                tracing::error!(
                    task_id = %task_id,
                    session_id = %session_id,
                    turn_id = %turn_id,
                    "处理历史会话事件失败: {}",
                    err
                );
            }
        }
    }

    loop {
        match rx.recv().await {
            Ok(notification) => {
                match handle_turn_notification(&ctx, session_hub, &notification).await {
                    Ok(TurnOutcome::Continue) => {}
                    Ok(outcome) => return outcome,
                    Err(err) => {
                        tracing::error!(
                            task_id = %task_id,
                            session_id = %session_id,
                            turn_id = %turn_id,
                            "处理实时会话事件失败: {}",
                            err
                        );
                    }
                }
            }
            Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                tracing::warn!(
                    task_id = %task_id,
                    session_id = %session_id,
                    turn_id = %turn_id,
                    skipped = skipped,
                    "Task turn 监听落后，部分消息被跳过"
                );
            }
            Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                tracing::warn!(
                    task_id = %task_id,
                    session_id = %session_id,
                    turn_id = %turn_id,
                    "Task turn 监听通道关闭，未收到终态事件"
                );
                return resolve_failure_outcome(
                    &ctx,
                    "turn_monitor_closed",
                    None,
                )
                .await
                .unwrap_or(TurnOutcome::Failed);
            }
        }
    }
}

pub async fn handle_turn_notification(
    ctx: &TurnEventContext<'_>,
    session_hub: &crate::session::SessionHub,
    notification: &SessionNotification,
) -> Result<TurnOutcome, agentdash_domain::DomainError> {
    match &notification.update {
        SessionUpdate::ToolCall(tool_call)
            if turn_matches(tool_call.meta.as_ref(), ctx.turn_id) => {
                persist_tool_call_artifact(
                    ctx.repos,
                    ToolCallArtifactInput {
                        task_id: ctx.task_id,
                        session_id: ctx.session_id,
                        turn_id: ctx.turn_id,
                        tool_call_id: &tool_call.tool_call_id.to_string(),
                        patch: build_tool_call_patch(tool_call),
                        backend_id: ctx.backend_id,
                        reason: "tool_call",
                    },
                )
                .await?;
            }
        SessionUpdate::ToolCallUpdate(update)
            if turn_matches(update.meta.as_ref(), ctx.turn_id) => {
                persist_tool_call_artifact(
                    ctx.repos,
                    ToolCallArtifactInput {
                        task_id: ctx.task_id,
                        session_id: ctx.session_id,
                        turn_id: ctx.turn_id,
                        tool_call_id: &update.tool_call_id.to_string(),
                        patch: build_tool_call_update_patch(update),
                        backend_id: ctx.backend_id,
                        reason: "tool_call_update",
                    },
                )
                .await?;
            }
        SessionUpdate::SessionInfoUpdate(info) => {
            sync_task_executor_session_binding_from_hub(
                ctx.repos, session_hub, ctx.task_id, ctx.backend_id, ctx.session_id, ctx.turn_id,
            )
            .await?;

            if let Some((event_type, message)) = parse_turn_event(info.meta.as_ref(), ctx.turn_id) {
                match event_type.as_str() {
                    "turn_completed" => {
                        if *ctx.execution_mode == TaskExecutionMode::OneShot {
                            tracing::info!(
                                task_id = %ctx.task_id,
                                session_id = %ctx.session_id,
                                turn_id = %ctx.turn_id,
                                "Turn 完成 [OneShot]，直接标记 Completed 并清理 session"
                            );
                            let _ = update_task_status(
                                ctx.repos,
                                ctx.task_id,
                                ctx.backend_id,
                                TaskStatus::Completed,
                                "turn_completed_oneshot",
                                json!({
                                    "session_id": ctx.session_id,
                                    "turn_id": ctx.turn_id,
                                    "execution_mode": "one_shot",
                                }),
                            )
                            .await?;
                            clear_task_session_binding(
                                ctx.repos,
                                ctx.task_id,
                                ctx.backend_id,
                                "oneshot_completed",
                            )
                            .await;
                        } else {
                            ctx.restart_tracker.record_stable_start(ctx.task_id);
                            let _ = update_task_status(
                                ctx.repos,
                                ctx.task_id,
                                ctx.backend_id,
                                TaskStatus::AwaitingVerification,
                                "turn_completed",
                                json!({
                                    "session_id": ctx.session_id,
                                    "turn_id": ctx.turn_id,
                                }),
                            )
                            .await?;
                        }
                        return Ok(TurnOutcome::Completed);
                    }
                    "turn_failed" => {
                        if let Some(err_msg) = message.as_deref() {
                            persist_turn_failure_artifact(
                                ctx.repos, ctx.task_id, ctx.backend_id, ctx.session_id, ctx.turn_id, err_msg,
                            )
                            .await?;
                        }

                        return resolve_failure_outcome(
                            ctx,
                            "turn_failed",
                            message.clone(),
                        )
                        .await;
                    }
                    _ => {}
                }
            }
        }
        _ => {}
    }

    Ok(TurnOutcome::Continue)
}

pub async fn resolve_failure_outcome(
    ctx: &TurnEventContext<'_>,
    reason: &str,
    error_message: Option<String>,
) -> Result<TurnOutcome, agentdash_domain::DomainError> {
    match ctx.execution_mode {
        TaskExecutionMode::AutoRetry => {
            let decision = ctx.restart_tracker.report_failure(ctx.task_id);
            match decision {
                RestartDecision::Allowed { attempt, delay } => {
                    tracing::info!(
                        task_id = %ctx.task_id,
                        session_id = %ctx.session_id,
                        turn_id = %ctx.turn_id,
                        attempt = attempt,
                        delay_ms = delay.as_millis() as u64,
                        reason = reason,
                        "Turn 失败 [AutoRetry]，RestartTracker 允许重试"
                    );
                    let _ = update_task_status(
                        ctx.repos,
                        ctx.task_id,
                        ctx.backend_id,
                        TaskStatus::AwaitingVerification,
                        &format!("{reason}_pending_retry"),
                        json!({
                            "session_id": ctx.session_id,
                            "turn_id": ctx.turn_id,
                            "error": error_message,
                            "retry_attempt": attempt,
                            "retry_delay_ms": delay.as_millis() as u64,
                            "execution_mode": "auto_retry",
                        }),
                    )
                    .await?;
                    Ok(TurnOutcome::NeedsRetry { delay, attempt })
                }
                RestartDecision::Denied { attempts_exhausted } => {
                    tracing::warn!(
                        task_id = %ctx.task_id,
                        session_id = %ctx.session_id,
                        turn_id = %ctx.turn_id,
                        attempts_exhausted = attempts_exhausted,
                        reason = reason,
                        "Turn 失败 [AutoRetry]，已达最大重试次数，标记为 Failed"
                    );
                    let _ = update_task_status(
                        ctx.repos,
                        ctx.task_id,
                        ctx.backend_id,
                        TaskStatus::Failed,
                        reason,
                        json!({
                            "session_id": ctx.session_id,
                            "turn_id": ctx.turn_id,
                            "error": error_message,
                            "attempts_exhausted": attempts_exhausted,
                            "execution_mode": "auto_retry",
                        }),
                    )
                    .await?;
                    Ok(TurnOutcome::Failed)
                }
            }
        }
        TaskExecutionMode::OneShot => {
            tracing::info!(
                task_id = %ctx.task_id,
                session_id = %ctx.session_id,
                turn_id = %ctx.turn_id,
                reason = reason,
                "Turn 失败 [OneShot]，标记 Failed 并清理 session"
            );
            let _ = update_task_status(
                ctx.repos,
                ctx.task_id,
                ctx.backend_id,
                TaskStatus::Failed,
                &format!("{reason}_oneshot"),
                json!({
                    "session_id": ctx.session_id,
                    "turn_id": ctx.turn_id,
                    "error": error_message,
                    "execution_mode": "one_shot",
                }),
            )
            .await?;
            clear_task_session_binding(ctx.repos, ctx.task_id, ctx.backend_id, "oneshot_failed").await;
            Ok(TurnOutcome::Failed)
        }
        TaskExecutionMode::Standard => {
            tracing::info!(
                task_id = %ctx.task_id,
                session_id = %ctx.session_id,
                turn_id = %ctx.turn_id,
                reason = reason,
                "Turn 失败 [Standard]，标记为 Failed，等待人工介入"
            );
            let _ = update_task_status(
                ctx.repos,
                ctx.task_id,
                ctx.backend_id,
                TaskStatus::Failed,
                reason,
                json!({
                    "session_id": ctx.session_id,
                    "turn_id": ctx.turn_id,
                    "error": error_message,
                    "execution_mode": "standard",
                }),
            )
            .await?;
            Ok(TurnOutcome::Failed)
        }
    }
}

pub fn bridge_task_status_event_to_session_notification(
    session_id: &str,
    turn_id: &str,
    event_type: &str,
    message: &str,
    data: Value,
) -> SessionNotification {
    let meta = build_task_lifecycle_meta(turn_id, event_type, message, data);
    SessionNotification::new(
        SessionId::new(session_id.to_string()),
        SessionUpdate::SessionInfoUpdate(SessionInfoUpdate::new().meta(meta)),
    )
}

pub async fn get_session_overview(
    session_hub: &crate::session::SessionHub,
    session_id: &str,
) -> Result<Option<SessionOverview>, TaskExecutionError> {
    let meta = session_hub
        .get_session_meta(session_id)
        .await
        .map_err(map_internal_error)?;
    Ok(meta.map(|value| SessionOverview {
        title: value.title,
        updated_at: value.updated_at,
    }))
}
