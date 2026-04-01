use std::collections::HashMap;
use std::io;
use std::sync::Arc;

use agent_client_protocol::{
    SessionInfoUpdate, SessionNotification, SessionUpdate, ToolCall, ToolCallUpdate,
};
use tokio::sync::Mutex;

use super::hub_support::{
    TurnTerminalKind, parse_executor_session_bound, parse_turn_id, parse_turn_terminal_event,
};
use super::persistence::{
    PersistedSessionEvent, SessionEventBacklog, SessionEventPage, SessionPersistence,
};
use super::types::SessionMeta;

#[derive(Clone, Default)]
pub struct MemorySessionPersistence {
    inner: Arc<Mutex<MemorySessionPersistenceState>>,
}

#[derive(Default)]
struct MemorySessionPersistenceState {
    metas: HashMap<String, SessionMeta>,
    events: HashMap<String, Vec<PersistedSessionEvent>>,
}

#[async_trait::async_trait]
impl SessionPersistence for MemorySessionPersistence {
    async fn create_session(&self, meta: &SessionMeta) -> io::Result<()> {
        let mut guard = self.inner.lock().await;
        guard.metas.insert(meta.id.clone(), meta.clone());
        guard.events.entry(meta.id.clone()).or_default();
        Ok(())
    }

    async fn get_session_meta(&self, session_id: &str) -> io::Result<Option<SessionMeta>> {
        let guard = self.inner.lock().await;
        Ok(guard.metas.get(session_id).cloned())
    }

    async fn list_sessions(&self) -> io::Result<Vec<SessionMeta>> {
        let guard = self.inner.lock().await;
        let mut metas = guard.metas.values().cloned().collect::<Vec<_>>();
        metas.sort_by_key(|meta| std::cmp::Reverse(meta.updated_at));
        Ok(metas)
    }

    async fn save_session_meta(&self, meta: &SessionMeta) -> io::Result<()> {
        let mut guard = self.inner.lock().await;
        match guard.metas.get_mut(&meta.id) {
            Some(current) => merge_session_meta(current, meta),
            None => {
                guard.metas.insert(meta.id.clone(), meta.clone());
            }
        }
        guard.events.entry(meta.id.clone()).or_default();
        Ok(())
    }

    async fn delete_session(&self, session_id: &str) -> io::Result<()> {
        let mut guard = self.inner.lock().await;
        guard.metas.remove(session_id);
        guard.events.remove(session_id);
        Ok(())
    }

    async fn append_event(
        &self,
        session_id: &str,
        notification: &SessionNotification,
    ) -> io::Result<PersistedSessionEvent> {
        let mut guard = self.inner.lock().await;
        let meta = guard.metas.get_mut(session_id).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!("session {session_id} 不存在"),
            )
        })?;
        let committed_at_ms = chrono::Utc::now().timestamp_millis();
        let event_seq = meta.last_event_seq.checked_add(1).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("session {session_id} 的 event_seq 已溢出"),
            )
        })?;
        let persisted = build_persisted_event(session_id, event_seq, committed_at_ms, notification);
        meta.last_event_seq = event_seq;
        meta.updated_at = committed_at_ms;
        apply_notification_projection(meta, notification);
        guard
            .events
            .entry(session_id.to_string())
            .or_default()
            .push(persisted.clone());
        Ok(persisted)
    }

    async fn read_backlog(
        &self,
        session_id: &str,
        after_seq: u64,
    ) -> io::Result<SessionEventBacklog> {
        let guard = self.inner.lock().await;
        let snapshot_seq = guard
            .metas
            .get(session_id)
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::NotFound,
                    format!("session {session_id} 不存在"),
                )
            })?
            .last_event_seq;
        let events = guard
            .events
            .get(session_id)
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("session {session_id} 缺少事件缓存"),
                )
            })?
            .clone()
            .into_iter()
            .filter(|event| event.event_seq > after_seq && event.event_seq <= snapshot_seq)
            .collect();
        Ok(SessionEventBacklog {
            snapshot_seq,
            events,
        })
    }

    async fn list_event_page(
        &self,
        session_id: &str,
        after_seq: u64,
        limit: u32,
    ) -> io::Result<SessionEventPage> {
        let guard = self.inner.lock().await;
        let snapshot_seq = guard
            .metas
            .get(session_id)
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::NotFound,
                    format!("session {session_id} 不存在"),
                )
            })?
            .last_event_seq;
        let mut events = guard
            .events
            .get(session_id)
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("session {session_id} 缺少事件缓存"),
                )
            })?
            .clone()
            .into_iter()
            .filter(|event| event.event_seq > after_seq)
            .collect::<Vec<_>>();
        events.sort_by_key(|event| event.event_seq);
        let limit = usize::try_from(limit.max(1))
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "分页大小超出 usize 范围"))?;
        let has_more = events.len() > limit;
        let page_events = if has_more {
            events.into_iter().take(limit).collect::<Vec<_>>()
        } else {
            events
        };
        let next_after_seq = page_events
            .last()
            .map(|event| event.event_seq)
            .unwrap_or(after_seq);
        Ok(SessionEventPage {
            snapshot_seq,
            events: page_events,
            has_more,
            next_after_seq,
        })
    }

    async fn list_all_events(&self, session_id: &str) -> io::Result<Vec<PersistedSessionEvent>> {
        let guard = self.inner.lock().await;
        Ok(guard
            .events
            .get(session_id)
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::NotFound,
                    format!("session {session_id} 不存在"),
                )
            })?
            .clone())
    }
}

fn build_persisted_event(
    session_id: &str,
    event_seq: u64,
    committed_at_ms: i64,
    notification: &SessionNotification,
) -> PersistedSessionEvent {
    PersistedSessionEvent {
        session_id: session_id.to_string(),
        event_seq,
        occurred_at_ms: committed_at_ms,
        committed_at_ms,
        session_update_type: session_update_type_name(&notification.update).to_string(),
        turn_id: notification_turn_id(notification),
        entry_index: notification_entry_index(notification),
        tool_call_id: notification_tool_call_id(notification),
        notification: notification.clone(),
    }
}

fn merge_session_meta(current: &mut SessionMeta, incoming: &SessionMeta) {
    let current_event_seq = current.last_event_seq;
    let incoming_event_seq = incoming.last_event_seq;

    current.title = incoming.title.clone();
    current.created_at = incoming.created_at;
    current.updated_at = current.updated_at.max(incoming.updated_at);
    current.last_event_seq = current.last_event_seq.max(incoming.last_event_seq);

    if incoming_event_seq >= current_event_seq {
        current.last_execution_status = incoming.last_execution_status.clone();
        current.last_turn_id = incoming.last_turn_id.clone();
        current.last_terminal_message = incoming.last_terminal_message.clone();
    }

    current.executor_config = incoming.executor_config.clone();
    current.executor_session_id = incoming.executor_session_id.clone();
    current.companion_context = incoming.companion_context.clone();
    current.visible_canvas_mount_ids = incoming.visible_canvas_mount_ids.clone();
}

pub(super) fn apply_notification_projection(
    meta: &mut SessionMeta,
    notification: &SessionNotification,
) {
    match &notification.update {
        SessionUpdate::SessionInfoUpdate(info) => apply_info_projection(meta, info),
        SessionUpdate::UserMessageChunk(chunk)
        | SessionUpdate::AgentMessageChunk(chunk)
        | SessionUpdate::AgentThoughtChunk(chunk) => {
            if let Some(turn_id) = parse_turn_id(chunk.meta.as_ref()) {
                meta.last_turn_id = Some(turn_id);
            }
        }
        SessionUpdate::ToolCall(call) => {
            if let Some(turn_id) = parse_turn_id(call.meta.as_ref()) {
                meta.last_turn_id = Some(turn_id);
            }
        }
        SessionUpdate::ToolCallUpdate(update) => {
            if let Some(turn_id) = parse_turn_id(update.meta.as_ref()) {
                meta.last_turn_id = Some(turn_id);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_client_protocol::{SessionId, SessionInfoUpdate};
    use agentdash_acp_meta::{
        AgentDashEventV1, AgentDashMetaV1, AgentDashTraceV1, merge_agentdash_meta,
    };

    fn turn_terminal_notification(
        session_id: &str,
        turn_id: &str,
        terminal_type: &str,
        message: &str,
    ) -> SessionNotification {
        let meta = merge_agentdash_meta(
            None,
            &AgentDashMetaV1::new()
                .event(AgentDashEventV1::new(terminal_type).message(Some(message.to_string())))
                .trace(AgentDashTraceV1 {
                    turn_id: Some(turn_id.to_string()),
                    ..AgentDashTraceV1::new()
                }),
        );
        SessionNotification::new(
            SessionId::new(session_id),
            SessionUpdate::SessionInfoUpdate(SessionInfoUpdate::new().meta(meta)),
        )
    }

    #[tokio::test]
    async fn save_session_meta_keeps_newer_event_projection() {
        let persistence = MemorySessionPersistence::default();
        let meta = SessionMeta {
            id: "sess-memory".to_string(),
            title: "测试".to_string(),
            created_at: 1,
            updated_at: 1,
            last_event_seq: 0,
            last_execution_status: "idle".to_string(),
            last_turn_id: None,
            last_terminal_message: None,
            executor_config: None,
            executor_session_id: None,
            companion_context: None,
            visible_canvas_mount_ids: Vec::new(),
        };
        persistence
            .create_session(&meta)
            .await
            .expect("应能创建 session");

        let mut stale = persistence
            .get_session_meta("sess-memory")
            .await
            .expect("应能读取 meta")
            .expect("session 应存在");
        stale.updated_at = 10;
        stale.last_execution_status = "running".to_string();
        stale.last_turn_id = Some("t-old".to_string());
        stale.executor_session_id = Some("exec-1".to_string());
        stale.visible_canvas_mount_ids = vec!["canvas-a".to_string()];

        persistence
            .append_event(
                "sess-memory",
                &turn_terminal_notification("sess-memory", "t-new", "turn_completed", "done"),
            )
            .await
            .expect("应能写入终态事件");
        persistence
            .save_session_meta(&stale)
            .await
            .expect("旧快照回写仍应成功");

        let merged = persistence
            .get_session_meta("sess-memory")
            .await
            .expect("应能再次读取 meta")
            .expect("session 应存在");
        assert_eq!(merged.last_event_seq, 1);
        assert_eq!(merged.last_execution_status, "completed");
        assert_eq!(merged.last_turn_id.as_deref(), Some("t-new"));
        assert_eq!(merged.last_terminal_message.as_deref(), Some("done"));
        assert_eq!(merged.executor_session_id.as_deref(), Some("exec-1"));
        assert_eq!(merged.visible_canvas_mount_ids, vec!["canvas-a"]);
    }
}

fn apply_info_projection(meta: &mut SessionMeta, info: &SessionInfoUpdate) {
    if let Some((turn_id, terminal_kind, message)) = parse_turn_terminal_event(info.meta.as_ref()) {
        meta.last_turn_id = Some(turn_id);
        meta.last_terminal_message = message;
        meta.last_execution_status = match terminal_kind {
            TurnTerminalKind::Completed => "completed",
            TurnTerminalKind::Failed => "failed",
            TurnTerminalKind::Interrupted => "interrupted",
        }
        .to_string();
        return;
    }

    if let Some(turn_id) = parse_turn_id(info.meta.as_ref()) {
        meta.last_turn_id = Some(turn_id.clone());
        if info
            .meta
            .as_ref()
            .and_then(|meta_value| parse_event_type(meta_value))
            .as_deref()
            == Some("turn_started")
        {
            meta.last_execution_status = "running".to_string();
            meta.last_turn_id = Some(turn_id);
            meta.last_terminal_message = None;
        }
    }

    if let Some(expected_turn_id) = meta.last_turn_id.as_deref()
        && let Some(executor_session_id) =
            parse_executor_session_bound(info.meta.as_ref(), expected_turn_id)
    {
        meta.executor_session_id = Some(executor_session_id);
    }
}

fn parse_event_type(meta: &agent_client_protocol::Meta) -> Option<String> {
    agentdash_acp_meta::parse_agentdash_meta(meta)
        .and_then(|parsed| parsed.event.map(|event| event.r#type))
}

pub(super) fn session_update_type_name(update: &SessionUpdate) -> &'static str {
    match update {
        SessionUpdate::UserMessageChunk(_) => "user_message_chunk",
        SessionUpdate::AgentMessageChunk(_) => "agent_message_chunk",
        SessionUpdate::AgentThoughtChunk(_) => "agent_thought_chunk",
        SessionUpdate::ToolCall(_) => "tool_call",
        SessionUpdate::ToolCallUpdate(_) => "tool_call_update",
        SessionUpdate::Plan(_) => "plan",
        SessionUpdate::SessionInfoUpdate(_) => "session_info_update",
        SessionUpdate::UsageUpdate(_) => "usage_update",
        SessionUpdate::AvailableCommandsUpdate(_) => "available_commands_update",
        SessionUpdate::CurrentModeUpdate(_) => "current_mode_update",
        SessionUpdate::ConfigOptionUpdate(_) => "config_option_update",
        _ => "unknown",
    }
}

pub(super) fn notification_turn_id(notification: &SessionNotification) -> Option<String> {
    match &notification.update {
        SessionUpdate::UserMessageChunk(chunk)
        | SessionUpdate::AgentMessageChunk(chunk)
        | SessionUpdate::AgentThoughtChunk(chunk) => parse_turn_id(chunk.meta.as_ref()),
        SessionUpdate::ToolCall(ToolCall { meta, .. })
        | SessionUpdate::ToolCallUpdate(ToolCallUpdate { meta, .. })
        | SessionUpdate::SessionInfoUpdate(SessionInfoUpdate { meta, .. }) => {
            parse_turn_id(meta.as_ref())
        }
        _ => None,
    }
}

pub(super) fn notification_entry_index(notification: &SessionNotification) -> Option<u32> {
    let meta = match &notification.update {
        SessionUpdate::UserMessageChunk(chunk)
        | SessionUpdate::AgentMessageChunk(chunk)
        | SessionUpdate::AgentThoughtChunk(chunk) => chunk.meta.as_ref(),
        SessionUpdate::ToolCall(ToolCall { meta, .. })
        | SessionUpdate::ToolCallUpdate(ToolCallUpdate { meta, .. })
        | SessionUpdate::SessionInfoUpdate(SessionInfoUpdate { meta, .. }) => meta.as_ref(),
        _ => None,
    };
    agentdash_acp_meta::parse_agentdash_meta(meta?)
        .and_then(|parsed| parsed.trace.and_then(|trace| trace.entry_index))
}

pub(super) fn notification_tool_call_id(notification: &SessionNotification) -> Option<String> {
    match &notification.update {
        SessionUpdate::ToolCall(call) => Some(call.tool_call_id.to_string()),
        SessionUpdate::ToolCallUpdate(update) => Some(update.tool_call_id.to_string()),
        _ => None,
    }
}
