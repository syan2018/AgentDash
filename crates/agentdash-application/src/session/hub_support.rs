use agentdash_agent_protocol::{
    BackboneEnvelope, BackboneEvent, PlatformEvent, SourceInfo, TraceInfo, UserInputBlock,
    UserInputSubmissionKind, UserInputSubmittedNotification,
};
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
        hook_runtime_target_cache: None,
        turn_state: TurnState::Idle,
        session_profile: None,
        hook_auto_resume_count: 0,
        last_activity_at: chrono::Utc::now().timestamp_millis(),
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
}

impl TurnState {
    pub fn is_running(&self) -> bool {
        matches!(self, Self::Claimed | Self::Active(_))
    }

    pub fn active_turn(&self) -> Option<&TurnExecution> {
        match self {
            Self::Active(turn) => Some(turn.as_ref()),
            _ => None,
        }
    }

    pub fn active_turn_mut(&mut self) -> Option<&mut TurnExecution> {
        match self {
            Self::Active(turn) => Some(turn.as_mut()),
            _ => None,
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
    /// Turn 级 capability 投影缓存（AgentFrame 的内存视图）。
    /// 权威数据源是 AgentFrame revision；此字段由 `replace_current_capability_state`
    /// 在写入 frame revision 后同步更新。
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
    pub fn new(
        turn_id: String,
        session_frame: ExecutionSessionFrame,
        capability_state: CapabilityState,
        context_audit_bundle_id: Uuid,
        context_audit_session_id: Uuid,
    ) -> Self {
        Self {
            turn_id,
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
