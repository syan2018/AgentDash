use std::{collections::HashSet, io};

use agent_client_protocol::{ContentBlock, Meta};
use agentdash_protocol::{BackboneEnvelope, BackboneEvent, PlatformEvent, SourceInfo, TraceInfo};
use agentdash_spi::{ExecutionSessionFrame, FlowCapabilities};
use tokio::sync::broadcast;

use agentdash_acp_meta::{parse_agentdash_meta, AgentDashSourceV1};
use agentdash_spi::hooks::{HookResolution, HookTrigger, SharedHookSessionRuntime};

use super::persistence::PersistedSessionEvent;
use super::types::{SessionExecutionState, SessionMeta};

pub(super) fn build_user_message_envelopes(
    session_id: &str,
    source: &SourceInfo,
    turn_id: &str,
    user_blocks: &[ContentBlock],
) -> Vec<BackboneEnvelope> {
    user_blocks
        .iter()
        .enumerate()
        .map(|(index, block)| {
            let value = serde_json::to_value(block).unwrap_or(serde_json::Value::Null);
            BackboneEnvelope::new(
                BackboneEvent::Platform(PlatformEvent::SessionMetaUpdate {
                    key: "user_message_chunk".to_string(),
                    value,
                }),
                session_id,
                source.clone(),
            )
            .with_trace(TraceInfo {
                turn_id: Some(turn_id.to_string()),
                entry_index: Some(index as u32),
            })
        })
        .collect()
}

/// 兼容入口：接受旧 `AgentDashSourceV1` 并转换为 `SourceInfo`。
pub(super) fn build_user_message_notifications(
    session_id: &str,
    source: &AgentDashSourceV1,
    turn_id: &str,
    user_blocks: &[ContentBlock],
) -> Vec<BackboneEnvelope> {
    let source_info = SourceInfo {
        connector_id: source.connector_id.clone(),
        connector_type: source.connector_type.clone(),
        executor_id: source.executor_id.clone(),
    };
    build_user_message_envelopes(session_id, &source_info, turn_id, user_blocks)
}

pub(super) fn build_turn_started_envelope(
    session_id: &str,
    source: &SourceInfo,
    turn_id: &str,
) -> BackboneEnvelope {
    use agentdash_protocol::codex_app_server_protocol as codex;
    BackboneEnvelope::new(
        BackboneEvent::TurnStarted(codex::TurnStartedNotification {
            thread_id: session_id.to_string(),
            turn: codex::Turn {
                id: turn_id.to_string(),
                items: Vec::new(),
                status: codex::TurnStatus::InProgress,
                error: None,
                started_at: Some(chrono::Utc::now().timestamp()),
                completed_at: None,
                duration_ms: None,
            },
        }),
        session_id,
        source.clone(),
    )
    .with_trace(TraceInfo {
        turn_id: Some(turn_id.to_string()),
        entry_index: None,
    })
}

pub(super) fn build_turn_lifecycle_envelope(
    session_id: &str,
    source: &SourceInfo,
    turn_id: &str,
    event_type: &str,
    severity: &str,
    message: Option<String>,
) -> BackboneEnvelope {
    let value = serde_json::json!({
        "event_type": event_type,
        "severity": severity,
        "message": message,
    });
    BackboneEnvelope::new(
        BackboneEvent::Platform(PlatformEvent::SessionMetaUpdate {
            key: "turn_lifecycle".to_string(),
            value,
        }),
        session_id,
        source.clone(),
    )
    .with_trace(TraceInfo {
        turn_id: Some(turn_id.to_string()),
        entry_index: None,
    })
}

pub(super) fn build_turn_terminal_notification(
    session_id: &str,
    source: &SourceInfo,
    turn_id: &str,
    terminal_kind: TurnTerminalKind,
    message: Option<String>,
) -> BackboneEnvelope {
    build_turn_terminal_envelope(session_id, source, turn_id, terminal_kind, message)
}

pub(super) fn build_turn_terminal_envelope(
    session_id: &str,
    source: &SourceInfo,
    turn_id: &str,
    terminal_kind: TurnTerminalKind,
    message: Option<String>,
) -> BackboneEnvelope {
    let value = serde_json::json!({
        "terminal_type": terminal_kind.event_type(),
        "message": message,
    });
    BackboneEnvelope::new(
        BackboneEvent::Platform(PlatformEvent::SessionMetaUpdate {
            key: "turn_terminal".to_string(),
            value,
        }),
        session_id,
        source.clone(),
    )
    .with_trace(TraceInfo {
        turn_id: Some(turn_id.to_string()),
        entry_index: None,
    })
}

pub(super) fn parse_executor_session_bound(
    meta: Option<&Meta>,
    expected_turn_id: &str,
) -> Option<String> {
    let parsed = parse_agentdash_meta(meta?)?;
    let trace = parsed.trace?;
    let turn_id = trace.turn_id?;
    if turn_id != expected_turn_id {
        return None;
    }

    let event = parsed.event?;
    if event.r#type != "executor_session_bound" {
        return None;
    }

    if let Some(data) = event.data
        && let Some(session_id) = data
            .get("executor_session_id")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
    {
        return Some(session_id.to_string());
    }

    event
        .message
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

/// 从 BackboneEnvelope 直接提取 executor_session_id（新路径）。
pub(super) fn parse_executor_session_bound_from_envelope(
    envelope: &BackboneEnvelope,
) -> Option<String> {
    match &envelope.event {
        BackboneEvent::Platform(PlatformEvent::ExecutorSessionBound {
            executor_session_id,
        }) => {
            let trimmed = executor_session_id.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        }
        _ => None,
    }
}

pub(super) fn parse_turn_id(meta: Option<&Meta>) -> Option<String> {
    parse_agentdash_meta(meta?)
        .and_then(|parsed| parsed.trace.and_then(|trace| trace.turn_id))
        .map(|turn_id| turn_id.trim().to_string())
        .filter(|turn_id| !turn_id.is_empty())
}

pub(super) fn parse_turn_terminal_event(
    meta: Option<&Meta>,
) -> Option<(String, TurnTerminalKind, Option<String>)> {
    let parsed = parse_agentdash_meta(meta?)?;
    let trace = parsed.trace?;
    let turn_id = trace.turn_id?;
    let event = parsed.event?;

    match event.r#type.as_str() {
        "turn_completed" => Some((turn_id, TurnTerminalKind::Completed, event.message)),
        "turn_failed" => Some((turn_id, TurnTerminalKind::Failed, event.message)),
        "turn_interrupted" => Some((turn_id, TurnTerminalKind::Interrupted, event.message)),
        _ => None,
    }
}

/// 从 BackboneEnvelope 直接解析 turn terminal 事件（新路径）。
pub(super) fn parse_turn_terminal_event_from_envelope(
    envelope: &BackboneEnvelope,
) -> Option<(String, TurnTerminalKind, Option<String>)> {
    let turn_id = envelope.trace.turn_id.as_deref()?.trim();
    if turn_id.is_empty() {
        return None;
    }

    match &envelope.event {
        BackboneEvent::Platform(PlatformEvent::SessionMetaUpdate { key, value })
            if key == "turn_terminal" =>
        {
            let terminal_type = value
                .get("terminal_type")
                .and_then(serde_json::Value::as_str)?;
            let kind = match terminal_type {
                "turn_completed" => TurnTerminalKind::Completed,
                "turn_failed" => TurnTerminalKind::Failed,
                "turn_interrupted" => TurnTerminalKind::Interrupted,
                _ => return None,
            };
            let message = value
                .get("message")
                .and_then(serde_json::Value::as_str)
                .map(ToString::to_string);
            Some((turn_id.to_string(), kind, message))
        }
        BackboneEvent::TurnCompleted(_) => {
            Some((turn_id.to_string(), TurnTerminalKind::Completed, None))
        }
        _ => None,
    }
}

pub(super) fn build_session_runtime(
    tx: broadcast::Sender<PersistedSessionEvent>,
) -> SessionRuntime {
    SessionRuntime {
        tx,
        running: false,
        hook_session: None,
        current_turn: None,
        hook_auto_resume_count: 0,
        last_activity_at: chrono::Utc::now().timestamp_millis(),
    }
}

/// Session 级运行态（跨 turn 存活直到进程退出或 session 被删除）。
///
/// Per-turn 字段（`turn_id` / `cancel_requested` / `processor_tx` / `session_frame`
/// / `flow_capabilities` / `relay_mcp_server_names`）统一下沉到
/// [`TurnExecution`]；`SessionRuntime` 只持 session 级信息。
pub(super) struct SessionRuntime {
    pub tx: broadcast::Sender<PersistedSessionEvent>,
    /// 是否有 turn 在跑。语义上等价 `current_turn.is_some()`，
    /// 保留作为显式 bool 以便外部只读路径快速判断。
    pub running: bool,
    /// Session 级 hook runtime（跨 turn 共享）。
    pub hook_session: Option<SharedHookSessionRuntime>,
    /// 当前活跃 turn 的执行态；无活跃 turn 时为 `None`。
    pub current_turn: Option<TurnExecution>,
    /// Hook 驱动的 auto-resume 计数器（session 级：跨 auto-resume 链累积，
    /// 新 turn 起始不清零），用于限流防止 hook 指令死循环。
    pub hook_auto_resume_count: u32,
    /// 最近一次事件活动的时间戳（毫秒），用于 stall 检测。
    pub last_activity_at: i64,
}

/// Per-turn 执行态。生命周期 = 一次 `start_prompt` → terminal。
#[derive(Clone)]
pub(super) struct TurnExecution {
    /// 当前 turn 标识。
    pub turn_id: String,
    /// Session 级执行环境快照（turn_id / working_dir / env / executor_config /
    /// mcp_servers / vfs / identity）。放在这里的动机是 MCP 热更新路径需要
    /// 拿到 turn 生效的 session frame 重建工具集。
    pub session_frame: ExecutionSessionFrame,
    /// 标记走 relay 的 MCP server 名集合（用于工具构建时分流）。
    pub relay_mcp_server_names: HashSet<String>,
    /// Turn 级 capability 集合（per-prompt 下发）。
    /// 保留在这里方便 MCP 热更新时直接重建 `ExecutionTurnFrame.flow_capabilities`。
    pub flow_capabilities: FlowCapabilities,
    /// 取消请求标记。hub.cancel 置 true；processor / adapter 读它决定发
    /// `Interrupted` 还是 `Completed/Failed` 终态。
    pub cancel_requested: bool,
    /// 活跃 turn 的事件处理 channel（由 SessionTurnProcessor 持有接收端）。
    /// relay 和 cloud-native 路径共用此通道发送 turn 事件。
    pub processor_tx: Option<tokio::sync::mpsc::UnboundedSender<super::turn_processor::TurnEvent>>,
}

impl TurnExecution {
    /// 新建一个 turn 执行态，其余字段由 `prompt_pipeline` 在组装完成后填入。
    pub fn new(
        turn_id: String,
        session_frame: ExecutionSessionFrame,
        relay_mcp_server_names: HashSet<String>,
        flow_capabilities: FlowCapabilities,
    ) -> Self {
        Self {
            turn_id,
            session_frame,
            relay_mcp_server_names,
            flow_capabilities,
            cancel_requested: false,
            processor_tx: None,
        }
    }
}

pub struct SessionEventSubscription {
    pub snapshot_seq: u64,
    pub backlog: Vec<PersistedSessionEvent>,
    pub rx: broadcast::Receiver<PersistedSessionEvent>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TurnTerminalKind {
    Completed,
    Failed,
    Interrupted,
}

impl TurnTerminalKind {
    pub fn event_type(self) -> &'static str {
        match self {
            Self::Completed => "turn_completed",
            Self::Failed => "turn_failed",
            Self::Interrupted => "turn_interrupted",
        }
    }

    pub fn severity(self) -> &'static str {
        match self {
            Self::Completed => "info",
            Self::Failed => "error",
            Self::Interrupted => "warning",
        }
    }

    pub fn state_tag(self) -> &'static str {
        match self {
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Interrupted => "interrupted",
        }
    }
}

/// 从 SessionMeta 的持久化字段派生 SessionExecutionState。
pub(super) fn meta_to_execution_state(
    meta: &SessionMeta,
    session_id: &str,
) -> io::Result<SessionExecutionState> {
    match meta.last_execution_status.as_str() {
        "idle" => Ok(SessionExecutionState::Idle),
        "completed" => Ok(SessionExecutionState::Completed {
            turn_id: meta.last_turn_id.clone().ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("session {session_id} 的 completed 状态缺少 last_turn_id"),
                )
            })?,
        }),
        "failed" => Ok(SessionExecutionState::Failed {
            turn_id: meta.last_turn_id.clone().ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("session {session_id} 的 failed 状态缺少 last_turn_id"),
                )
            })?,
            message: meta.last_terminal_message.clone(),
        }),
        "interrupted" => Ok(SessionExecutionState::Interrupted {
            turn_id: meta.last_turn_id.clone(),
            message: meta.last_terminal_message.clone(),
        }),
        "running" => {
            tracing::warn!(
                session_id,
                "meta 显示 running 但内存 map 无记录，视为 interrupted"
            );
            Ok(SessionExecutionState::Interrupted {
                turn_id: meta.last_turn_id.clone(),
                message: None,
            })
        }
        other => Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("session {session_id} 的 last_execution_status 非法: {other}"),
        )),
    }
}

pub(super) fn session_hook_trace_decision(
    trigger: &HookTrigger,
    resolution: &HookResolution,
) -> &'static str {
    match trigger {
        HookTrigger::SessionStart => {
            if resolution.refresh_snapshot {
                "baseline_refreshed"
            } else if !resolution.injections.is_empty() || !resolution.diagnostics.is_empty() {
                "baseline_initialized"
            } else {
                "noop"
            }
        }
        HookTrigger::SessionTerminal => {
            if resolution
                .completion
                .as_ref()
                .is_some_and(|completion| completion.advanced)
            {
                "step_advanced"
            } else {
                "terminal_observed"
            }
        }
        HookTrigger::CapabilityChanged => "capability_changed",
        _ => "noop",
    }
}
