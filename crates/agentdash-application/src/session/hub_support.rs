use agent_client_protocol::{
    ContentBlock, ContentChunk, Meta, SessionId, SessionInfoUpdate, SessionNotification,
    SessionUpdate,
};
use tokio::sync::broadcast;

use agentdash_acp_meta::{
    AgentDashEventV1, AgentDashMetaV1, AgentDashSourceV1, AgentDashTraceV1, merge_agentdash_meta,
    parse_agentdash_meta,
};
use agentdash_spi::hooks::{HookResolution, HookTrigger, SharedHookSessionRuntime};

use super::types::{SessionExecutionState, SessionMeta};

pub(super) fn build_user_message_notifications(
    session_id: &str,
    source: &AgentDashSourceV1,
    turn_id: &str,
    user_blocks: &[ContentBlock],
) -> Vec<SessionNotification> {
    user_blocks
        .iter()
        .enumerate()
        .map(|(index, block)| {
            let mut trace = AgentDashTraceV1::new();
            trace.turn_id = Some(turn_id.to_string());
            trace.entry_index = Some(index as u32);

            let agentdash = AgentDashMetaV1::new()
                .source(Some(source.clone()))
                .trace(Some(trace));
            let meta = merge_agentdash_meta(None, &agentdash);

            let chunk = ContentChunk::new(block.clone()).meta(meta);
            SessionNotification::new(
                SessionId::new(session_id),
                SessionUpdate::UserMessageChunk(chunk),
            )
        })
        .collect()
}

pub(super) fn build_turn_lifecycle_notification(
    session_id: &str,
    source: &AgentDashSourceV1,
    turn_id: &str,
    event_type: &str,
    severity: &str,
    message: Option<String>,
) -> SessionNotification {
    let mut trace = AgentDashTraceV1::new();
    trace.turn_id = Some(turn_id.to_string());

    let mut event = AgentDashEventV1::new(event_type);
    event.severity = Some(severity.to_string());
    event.message = message;

    let agentdash = AgentDashMetaV1::new()
        .source(Some(source.clone()))
        .trace(Some(trace))
        .event(Some(event));
    let meta = merge_agentdash_meta(None, &agentdash);

    let info = SessionInfoUpdate::new().meta(meta);
    SessionNotification::new(
        SessionId::new(session_id),
        SessionUpdate::SessionInfoUpdate(info),
    )
}

pub(super) fn build_turn_terminal_notification(
    session_id: &str,
    source: &AgentDashSourceV1,
    turn_id: &str,
    terminal_kind: TurnTerminalKind,
    message: Option<String>,
) -> SessionNotification {
    build_turn_lifecycle_notification(
        session_id,
        source,
        turn_id,
        terminal_kind.event_type(),
        terminal_kind.severity(),
        message,
    )
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

pub(super) fn build_session_runtime(tx: broadcast::Sender<SessionNotification>) -> SessionRuntime {
    SessionRuntime {
        tx,
        running: false,
        current_turn_id: None,
        cancel_requested: false,
        hook_session: None,
    }
}

pub(super) struct SessionRuntime {
    pub tx: broadcast::Sender<SessionNotification>,
    pub running: bool,
    pub current_turn_id: Option<String>,
    pub cancel_requested: bool,
    pub hook_session: Option<SharedHookSessionRuntime>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum TurnTerminalKind {
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
) -> SessionExecutionState {
    match meta.last_execution_status.as_str() {
        "idle" => SessionExecutionState::Idle,
        "completed" => SessionExecutionState::Completed {
            turn_id: meta.last_turn_id.clone().unwrap_or_default(),
        },
        "failed" => SessionExecutionState::Failed {
            turn_id: meta.last_turn_id.clone().unwrap_or_default(),
            message: meta.last_terminal_message.clone(),
        },
        "interrupted" => SessionExecutionState::Interrupted {
            turn_id: meta.last_turn_id.clone(),
            message: meta.last_terminal_message.clone(),
        },
        "running" => {
            tracing::warn!(
                session_id,
                "meta 显示 running 但内存 map 无记录，视为 interrupted"
            );
            SessionExecutionState::Interrupted {
                turn_id: meta.last_turn_id.clone(),
                message: None,
            }
        }
        other => {
            unreachable!("last_execution_status 出现了非法值: {other:?}，这是 SessionHub 的 bug")
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
            } else if !resolution.context_fragments.is_empty()
                || !resolution.constraints.is_empty()
                || !resolution.policies.is_empty()
                || !resolution.diagnostics.is_empty()
            {
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
        _ => "noop",
    }
}
