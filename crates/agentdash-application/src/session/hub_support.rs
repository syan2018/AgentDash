use std::io;

use agentdash_protocol::ContentBlock;
use agentdash_protocol::{BackboneEnvelope, BackboneEvent, PlatformEvent, SourceInfo, TraceInfo};
use agentdash_spi::{ExecutionSessionFrame, FlowCapabilities, Vfs};
use tokio::sync::broadcast;

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


/// 从 BackboneEnvelope 直接解析 turn terminal 事件。
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

impl SessionRuntime {
    pub fn is_running(&self) -> bool {
        self.turn_state.is_running()
    }
}

pub(super) fn build_session_runtime(
    tx: broadcast::Sender<PersistedSessionEvent>,
) -> SessionRuntime {
    SessionRuntime {
        tx,
        hook_session: None,
        turn_state: TurnState::Idle,
        session_profile: None,
        hook_auto_resume_count: 0,
        last_activity_at: chrono::Utc::now().timestamp_millis(),
    }
}

/// Session 的内禀运行时配置——Init 时确立，跨 turn 持续生效。
///
/// 这些字段是 session 的固有属性（VFS、MCP、能力集），不是每轮 prompt 的
/// 请求负载。Continue 直接复用，Rehydrate 重建后覆盖。
#[derive(Clone)]
pub(super) struct SessionProfile {
    pub vfs: Vfs,
    pub mcp_servers: Vec<agentdash_spi::SessionMcpServer>,
    pub flow_capabilities: FlowCapabilities,
}

/// Session turn 的状态机。
///
/// 替代原先 `running: bool` + `current_turn: Option<TurnExecution>` 的双字段设计：
/// - `Idle`：无活跃 turn
/// - `Claimed`：已锁定 session，turn 尚未完全初始化（防止并发 prompt）
/// - `Active`：turn 正在执行
#[derive(Default)]
pub(super) enum TurnState {
    #[default]
    Idle,
    Claimed,
    Active(TurnExecution),
}

impl TurnState {
    pub fn is_running(&self) -> bool {
        matches!(self, Self::Claimed | Self::Active(_))
    }

    pub fn active_turn(&self) -> Option<&TurnExecution> {
        match self {
            Self::Active(turn) => Some(turn),
            _ => None,
        }
    }

    pub fn active_turn_mut(&mut self) -> Option<&mut TurnExecution> {
        match self {
            Self::Active(turn) => Some(turn),
            _ => None,
        }
    }
}

/// Session 级运行态（跨 turn 存活直到进程退出或 session 被删除）。
///
/// Per-turn 字段（`turn_id` / `cancel_requested` / `processor_tx` / `session_frame`
/// / `flow_capabilities`）统一下沉到
/// [`TurnExecution`]；`SessionRuntime` 只持 session 级信息。
pub(super) struct SessionRuntime {
    pub tx: broadcast::Sender<PersistedSessionEvent>,
    /// Session 级 hook runtime（跨 turn 共享）。
    pub hook_session: Option<SharedHookSessionRuntime>,
    /// Turn 状态机：Idle → Claimed → Active → Idle。
    pub turn_state: TurnState,
    /// Session 的内禀运行时配置；Init 时写入，Continue 时复用。
    pub session_profile: Option<SessionProfile>,
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
        flow_capabilities: FlowCapabilities,
    ) -> Self {
        Self {
            turn_id,
            session_frame,
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

impl From<TurnTerminalKind> for super::types::ExecutionStatus {
    fn from(kind: TurnTerminalKind) -> Self {
        match kind {
            TurnTerminalKind::Completed => Self::Completed,
            TurnTerminalKind::Failed => Self::Failed,
            TurnTerminalKind::Interrupted => Self::Interrupted,
        }
    }
}

/// 从 SessionMeta 的持久化字段派生 SessionExecutionState。
pub(super) fn meta_to_execution_state(
    meta: &SessionMeta,
    session_id: &str,
) -> io::Result<SessionExecutionState> {
    use super::types::ExecutionStatus;
    match meta.last_execution_status {
        ExecutionStatus::Idle => Ok(SessionExecutionState::Idle),
        ExecutionStatus::Completed => Ok(SessionExecutionState::Completed {
            turn_id: meta.last_turn_id.clone().ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("session {session_id} 的 completed 状态缺少 last_turn_id"),
                )
            })?,
        }),
        ExecutionStatus::Failed => Ok(SessionExecutionState::Failed {
            turn_id: meta.last_turn_id.clone().ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("session {session_id} 的 failed 状态缺少 last_turn_id"),
                )
            })?,
            message: meta.last_terminal_message.clone(),
        }),
        ExecutionStatus::Interrupted => Ok(SessionExecutionState::Interrupted {
            turn_id: meta.last_turn_id.clone(),
            message: meta.last_terminal_message.clone(),
        }),
        ExecutionStatus::Running => {
            tracing::warn!(
                session_id,
                "meta 显示 running 但内存 map 无记录，视为 interrupted"
            );
            Ok(SessionExecutionState::Interrupted {
                turn_id: meta.last_turn_id.clone(),
                message: None,
            })
        }
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
