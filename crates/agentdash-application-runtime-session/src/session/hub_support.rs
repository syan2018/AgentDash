use agentdash_agent_protocol::{
    BackboneEnvelope, BackboneEvent, PlatformEvent, SourceInfo, TraceInfo, UserInputBlock,
    UserInputSubmissionKind, UserInputSubmittedNotification,
};
use agentdash_diagnostics::{Subsystem, diag};
use agentdash_spi::{CapabilityState, ContextFragment, ExecutionSessionFrame};
use tokio::sync::broadcast;
use uuid::Uuid;

use agentdash_spi::hooks::{HookResolution, HookTrigger, SharedHookRuntime};

use super::persistence::PersistedSessionEvent;
use super::types::{SessionExecutionState, SessionMeta};

pub(super) fn build_user_input_submitted_envelope(
    session_id: &str,
    source: &SourceInfo,
    turn_id: &str,
    item_id: &str,
    submission_kind: UserInputSubmissionKind,
    input: Vec<UserInputBlock>,
) -> BackboneEnvelope {
    BackboneEnvelope::new(
        BackboneEvent::UserInputSubmitted(UserInputSubmittedNotification::new(
            session_id,
            turn_id,
            item_id,
            submission_kind,
            input,
        )),
        session_id,
        source.clone(),
    )
    .with_trace(TraceInfo {
        turn_id: Some(turn_id.to_string()),
        entry_index: Some(0),
    })
}

pub(super) fn build_turn_started_envelope(
    session_id: &str,
    source: &SourceInfo,
    turn_id: &str,
    started_at_ms: i64,
) -> BackboneEnvelope {
    use agentdash_agent_protocol::codex_app_server_protocol as codex;
    BackboneEnvelope::new(
        BackboneEvent::TurnStarted(codex::TurnStartedNotification {
            thread_id: session_id.to_string(),
            turn: codex::Turn {
                id: turn_id.to_string(),
                items: Vec::new(),
                items_view: codex::TurnItemsView::NotLoaded,
                status: codex::TurnStatus::InProgress,
                error: None,
                started_at: Some(started_at_ms.div_euclid(1000)),
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

pub(super) fn build_turn_terminal_notification_with_timing(
    session_id: &str,
    source: &SourceInfo,
    turn_id: &str,
    terminal_kind: TurnTerminalKind,
    message: Option<String>,
    timing: Option<TurnTiming>,
) -> BackboneEnvelope {
    build_turn_terminal_envelope_with_timing(
        session_id,
        source,
        turn_id,
        terminal_kind,
        message,
        timing,
    )
}

pub(super) fn build_turn_terminal_envelope(
    session_id: &str,
    source: &SourceInfo,
    turn_id: &str,
    terminal_kind: TurnTerminalKind,
    message: Option<String>,
) -> BackboneEnvelope {
    build_turn_terminal_envelope_with_timing(
        session_id,
        source,
        turn_id,
        terminal_kind,
        message,
        None,
    )
}

pub(super) fn build_turn_terminal_envelope_with_timing(
    session_id: &str,
    source: &SourceInfo,
    turn_id: &str,
    terminal_kind: TurnTerminalKind,
    message: Option<String>,
    timing: Option<TurnTiming>,
) -> BackboneEnvelope {
    let mut value = serde_json::json!({
        "terminal_type": terminal_kind.event_type(),
        "message": message,
    });
    if let Some(timing) = timing
        && let Some(object) = value.as_object_mut()
    {
        object.insert(
            "started_at_ms".to_string(),
            serde_json::Value::from(timing.started_at_ms),
        );
        object.insert(
            "completed_at_ms".to_string(),
            serde_json::Value::from(timing.completed_at_ms),
        );
        object.insert(
            "duration_ms".to_string(),
            serde_json::Value::from(timing.duration_ms),
        );
    }
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
                "turn_lost" => TurnTerminalKind::Lost,
                _ => return None,
            };
            let message = value
                .get("message")
                .and_then(serde_json::Value::as_str)
                .map(ToString::to_string);
            Some((turn_id.to_string(), kind, message))
        }
        BackboneEvent::TurnCompleted(completed) => Some((
            turn_id.to_string(),
            TurnTerminalKind::from(completed.turn.status.clone()),
            completed
                .turn
                .error
                .as_ref()
                .map(|error| error.message.clone()),
        )),
        _ => None,
    }
}

impl SessionRuntime {
    pub fn is_running(&self) -> bool {
        self.turn_state.is_running()
    }
}

/// Per-session ephemeral buffer 容量上限（条）。超出后 evict 最旧条目，防进程内存膨胀。
/// 单个长 turn 的 delta 量级远小于此；turn 收尾会清空 buffer。
pub(super) const EPHEMERAL_BUFFER_CAP: usize = 16384;

pub(super) fn build_session_runtime(
    tx: broadcast::Sender<PersistedSessionEvent>,
) -> SessionRuntime {
    SessionRuntime {
        tx,
        hook_runtime_target_cache: None,
        turn_state: TurnState::Idle,
        session_profile: None,
        hook_auto_resume_count: 0,
        last_activity_at: chrono::Utc::now().timestamp_millis(),
        ephemeral_buffer: std::collections::VecDeque::new(),
        ephemeral_seq: 0,
    }
}

/// Session 的内禀运行时配置——Init 时确立，跨 turn 持续生效。
///
/// `capability_state` 是 AgentFrame revision 的内存投影缓存，
/// 避免每次访问都反序列化 frame JSON。权威数据源始终是 AgentFrame；
/// 写入通过 `AgentFrameBuilder::with_capability_state` → frame revision，
/// 然后同步更新此缓存。
#[derive(Clone)]
pub(super) struct SessionProfile {
    pub capability_state: CapabilityState,
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
    Active(Box<TurnExecution>),
    Cancelling(Box<TurnExecution>),
}

impl TurnState {
    pub fn is_running(&self) -> bool {
        matches!(self, Self::Claimed | Self::Active(_) | Self::Cancelling(_))
    }

    pub fn is_cancelling(&self) -> bool {
        matches!(self, Self::Cancelling(_))
    }

    pub fn active_turn(&self) -> Option<&TurnExecution> {
        match self {
            Self::Active(turn) | Self::Cancelling(turn) => Some(turn.as_ref()),
            _ => None,
        }
    }

    pub fn active_turn_mut(&mut self) -> Option<&mut TurnExecution> {
        match self {
            Self::Active(turn) | Self::Cancelling(turn) => Some(turn.as_mut()),
            _ => None,
        }
    }

    pub fn request_cancel(&mut self) {
        let previous = std::mem::replace(self, Self::Idle);
        match previous {
            Self::Active(mut turn) => {
                turn.cancel_requested = true;
                *self = Self::Cancelling(turn);
            }
            Self::Cancelling(mut turn) => {
                turn.cancel_requested = true;
                *self = Self::Cancelling(turn);
            }
            other => *self = other,
        }
    }
}

/// Session 级运行态（跨 turn 存活直到进程退出或 session 被删除）。
///
/// Per-turn 字段（`turn_id` / `cancel_requested` / `processor_tx` / `session_frame`
/// / `capability_state`）统一下沉到
/// [`TurnExecution`]；`SessionRuntime` 只持 session 级信息。
pub(super) struct SessionRuntime {
    pub tx: broadcast::Sender<PersistedSessionEvent>,
    /// 当前 delivery RuntimeSession 复用的 Hook runtime target cache。
    ///
    /// 业务 owner 是 `AgentFrameHookRuntime.control_target()`；此字段保存的 runtime
    /// 只能由 target-first service 校验后进入业务路径。
    pub hook_runtime_target_cache: Option<SharedHookRuntime>,
    /// Turn 状态机：Idle → Claimed → Active → Idle。
    pub turn_state: TurnState,
    /// Session 的内禀运行时配置；Init 时写入，Continue 时复用。
    pub session_profile: Option<SessionProfile>,
    /// Hook 驱动的 auto-resume 计数器（session 级：跨 auto-resume 链累积，
    /// 新 turn 起始不清零），用于限流防止 hook 指令死循环。
    pub hook_auto_resume_count: u32,
    /// 最近一次事件活动的时间戳（毫秒），用于 stall 检测。
    pub last_activity_at: i64,
    /// Per-turn in-flight ephemeral 事件缓冲（仅内存）。承载 delta / item_updated 等
    /// 不入 durable 主日志的进度态事件，用于整页刷新 / 断线重连时补回"生成中"文本。
    /// 每条事件复用 `event_seq` 字段承载单调 `ephemeral_seq`，前端据此去重。
    /// turn 收尾（terminal）时由 `clear_ephemeral` 清空（终态正文已是 durable）。
    pub ephemeral_buffer: std::collections::VecDeque<PersistedSessionEvent>,
    /// Per-session 单调 ephemeral 序号；clear 时不重置，保证跨 turn 单调避免前端误去重。
    pub ephemeral_seq: u64,
}

/// Per-turn 执行态。生命周期 = 一次 `start_prompt` → terminal。
#[derive(Clone)]
pub(super) struct TurnExecution {
    /// 当前 turn 标识。
    pub turn_id: String,
    /// Turn accepted/started 的毫秒时间戳，用于 terminal duration 诊断和
    /// Codex Turn.durationMs 对齐。
    pub started_at_ms: i64,
    /// Session 级执行环境快照（turn_id / working_dir / env / executor_config /
    /// mcp_servers / vfs / identity）。放在这里的动机是 MCP 热更新路径需要
    /// 拿到 turn 生效的 session frame 重建工具集。
    pub session_frame: ExecutionSessionFrame,
    /// Turn 级 capability 投影缓存（AgentFrame 的内存视图）。
    /// 权威数据源是 AgentFrame revision；此字段在 adoption 已持久化 revision 时同步更新。
    pub capability_state: CapabilityState,
    /// 运行期 Hook 注入的增量片段（审计路径）。
    pub runtime_injection_fragments: Vec<ContextFragment>,
    /// 当前 turn 审计片段所属的上下文批次标识。
    pub context_audit_bundle_id: Uuid,
    /// 当前 turn 审计片段所属 session UUID。
    pub context_audit_session_id: Uuid,
    /// 取消请求标记。hub.cancel 置 true；processor / adapter 读它决定发
    /// `Interrupted` 还是 `Completed/Failed` 终态。
    pub cancel_requested: bool,
    /// 活跃 turn 的事件处理 channel（由 SessionTurnProcessor 持有接收端）。
    /// relay 和 cloud-native 路径共用此通道发送 turn 事件。
    pub processor_tx: Option<tokio::sync::mpsc::UnboundedSender<super::turn_processor::TurnEvent>>,
    /// connector stream adapter 后台任务的 abort handle。
    /// 由 `TurnSupervisor` 在 turn 释放时中止，避免 terminal 后 adapter 继续悬挂。
    pub stream_adapter_abort: Option<tokio::task::AbortHandle>,
}

impl TurnExecution {
    /// 新建一个 turn 执行态，其余字段由 launch preparation 在组装完成后填入。
    #[cfg(test)]
    pub fn new(
        turn_id: String,
        session_frame: ExecutionSessionFrame,
        capability_state: CapabilityState,
        context_audit_bundle_id: Uuid,
        context_audit_session_id: Uuid,
    ) -> Self {
        let started_at_ms = chrono::Utc::now().timestamp_millis();
        Self::new_with_started_at(
            turn_id,
            session_frame,
            capability_state,
            context_audit_bundle_id,
            context_audit_session_id,
            started_at_ms,
        )
    }

    pub fn new_with_started_at(
        turn_id: String,
        session_frame: ExecutionSessionFrame,
        capability_state: CapabilityState,
        context_audit_bundle_id: Uuid,
        context_audit_session_id: Uuid,
        started_at_ms: i64,
    ) -> Self {
        Self {
            turn_id,
            started_at_ms,
            session_frame,
            capability_state,
            runtime_injection_fragments: Vec::new(),
            context_audit_bundle_id,
            context_audit_session_id,
            cancel_requested: false,
            processor_tx: None,
            stream_adapter_abort: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct TurnTiming {
    pub started_at_ms: i64,
    pub completed_at_ms: i64,
    pub duration_ms: i64,
}

impl TurnTiming {
    pub fn complete(started_at_ms: i64, completed_at_ms: i64) -> Self {
        Self {
            started_at_ms,
            completed_at_ms,
            duration_ms: completed_at_ms.saturating_sub(started_at_ms).max(0),
        }
    }
}

pub struct SessionEventSubscription {
    pub snapshot_seq: u64,
    pub backlog: Vec<PersistedSessionEvent>,
    /// 订阅建立时刻的 ephemeral buffer 快照（in-flight 进度态事件）。
    /// 在 durable backlog + `connected` 之后、live loop 之前补发，前端按 ephemeral_seq 去重。
    pub ephemeral_backlog: Vec<PersistedSessionEvent>,
    pub rx: broadcast::Receiver<PersistedSessionEvent>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TurnTerminalKind {
    Completed,
    Failed,
    Interrupted,
    Lost,
}

impl TurnTerminalKind {
    pub fn event_type(self) -> &'static str {
        match self {
            Self::Completed => "turn_completed",
            Self::Failed => "turn_failed",
            Self::Interrupted => "turn_interrupted",
            Self::Lost => "turn_lost",
        }
    }

    pub fn severity(self) -> &'static str {
        match self {
            Self::Completed => "info",
            Self::Failed => "error",
            Self::Interrupted => "warning",
            Self::Lost => "error",
        }
    }

    pub fn state_tag(self) -> &'static str {
        match self {
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Interrupted => "interrupted",
            Self::Lost => "lost",
        }
    }
}

impl From<TurnTerminalKind> for super::types::ExecutionStatus {
    fn from(kind: TurnTerminalKind) -> Self {
        match kind {
            TurnTerminalKind::Completed => Self::Completed,
            TurnTerminalKind::Failed => Self::Failed,
            TurnTerminalKind::Interrupted => Self::Interrupted,
            TurnTerminalKind::Lost => Self::Lost,
        }
    }
}

impl From<agentdash_agent_protocol::codex_app_server_protocol::TurnStatus> for TurnTerminalKind {
    fn from(status: agentdash_agent_protocol::codex_app_server_protocol::TurnStatus) -> Self {
        match status {
            agentdash_agent_protocol::codex_app_server_protocol::TurnStatus::Completed => {
                Self::Completed
            }
            agentdash_agent_protocol::codex_app_server_protocol::TurnStatus::Failed => Self::Failed,
            agentdash_agent_protocol::codex_app_server_protocol::TurnStatus::Interrupted => {
                Self::Interrupted
            }
            agentdash_agent_protocol::codex_app_server_protocol::TurnStatus::InProgress => {
                Self::Interrupted
            }
        }
    }
}

/// 从 SessionMeta 的持久化字段派生 SessionExecutionState。
pub(crate) fn meta_to_execution_state(
    meta: &SessionMeta,
    session_id: &str,
) -> super::persistence::SessionStoreResult<SessionExecutionState> {
    use super::persistence::SessionStoreError;
    use super::types::ExecutionStatus;
    match meta.last_delivery_status {
        ExecutionStatus::Idle => Ok(SessionExecutionState::Idle),
        ExecutionStatus::Completed => Ok(SessionExecutionState::Completed {
            turn_id: meta.last_turn_id.clone().ok_or_else(|| {
                SessionStoreError::InvalidData(format!(
                    "session {session_id} 的 completed 状态缺少 last_turn_id"
                ))
            })?,
        }),
        ExecutionStatus::Failed => Ok(SessionExecutionState::Failed {
            turn_id: meta.last_turn_id.clone().ok_or_else(|| {
                SessionStoreError::InvalidData(format!(
                    "session {session_id} 的 failed 状态缺少 last_turn_id"
                ))
            })?,
            message: meta.last_terminal_message.clone(),
        }),
        ExecutionStatus::Interrupted => Ok(SessionExecutionState::Interrupted {
            turn_id: meta.last_turn_id.clone(),
            message: meta.last_terminal_message.clone(),
        }),
        ExecutionStatus::Lost => Ok(SessionExecutionState::Lost {
            turn_id: meta.last_turn_id.clone(),
            message: meta.last_terminal_message.clone(),
        }),
        ExecutionStatus::Running => {
            diag!(
                Warn,
                Subsystem::AgentRun,
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
                "refresh_requested"
            } else if !resolution.injections.is_empty() {
                "context_injected"
            } else if !resolution.diagnostics.is_empty() {
                "notified"
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
        _ => "noop",
    }
}
