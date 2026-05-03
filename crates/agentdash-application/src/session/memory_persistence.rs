use std::collections::HashMap;
use std::io;
use std::sync::Arc;

use agentdash_protocol::{BackboneEnvelope, BackboneEvent, PlatformEvent};
use tokio::sync::Mutex;

use super::hub_support::{
    parse_executor_session_bound, parse_turn_terminal_event_from_envelope,
};
use super::persistence::{
    PersistedSessionEvent, SessionEventBacklog, SessionEventPage, SessionPersistence,
};
use super::types::{ExecutionStatus, SessionBootstrapState, SessionMeta};

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
        envelope: &BackboneEnvelope,
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
        let persisted = build_persisted_event(session_id, event_seq, committed_at_ms, envelope);
        meta.last_event_seq = event_seq;
        meta.updated_at = committed_at_ms;
        apply_envelope_projection(meta, envelope);
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
    envelope: &BackboneEnvelope,
) -> PersistedSessionEvent {
    PersistedSessionEvent {
        session_id: session_id.to_string(),
        event_seq,
        occurred_at_ms: envelope.observed_at.timestamp_millis(),
        committed_at_ms,
        session_update_type: backbone_event_type_name(&envelope.event).to_string(),
        turn_id: envelope.trace.turn_id.clone(),
        entry_index: envelope.trace.entry_index,
        tool_call_id: None,
        notification: envelope.clone(),
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
    if current.bootstrap_state != SessionBootstrapState::Bootstrapped {
        current.bootstrap_state = incoming.bootstrap_state;
    }
}

pub(super) fn apply_envelope_projection(meta: &mut SessionMeta, envelope: &BackboneEnvelope) {
    if let Some(turn_id) = envelope.trace.turn_id.as_deref() {
        let turn_id = turn_id.trim();
        if !turn_id.is_empty() {
            meta.last_turn_id = Some(turn_id.to_string());
        }
    }

    match &envelope.event {
        BackboneEvent::TurnStarted(_) => {
            meta.last_execution_status = ExecutionStatus::Running;
            meta.last_terminal_message = None;
        }
        BackboneEvent::TurnCompleted(_) => {
            meta.last_execution_status = ExecutionStatus::Completed;
        }
        BackboneEvent::Error(_) => {
            meta.last_execution_status = ExecutionStatus::Failed;
        }
        BackboneEvent::Platform(PlatformEvent::ExecutorSessionBound {
            executor_session_id,
        }) => {
            meta.executor_session_id = Some(executor_session_id.clone());
        }
        BackboneEvent::Platform(PlatformEvent::SessionMetaUpdate { key, value }) => {
            if let Some((turn_id, terminal_kind, message)) =
                parse_turn_terminal_event_from_envelope(envelope)
            {
                meta.last_turn_id = Some(turn_id);
                meta.last_terminal_message = message;
                meta.last_execution_status = terminal_kind.into();
            } else if key == "executor_session_bound" {
                if let Some(esid) = value.as_str() {
                    meta.executor_session_id = Some(esid.to_string());
                }
            }
        }
        _ => {}
    }

    // 兼容：旧路径的 compat 转换后的 SessionInfoUpdate 事件仍可能经过此处
    // 使用 envelope_to_session_notification 转换后进行 ACP meta 投影
    apply_compat_info_projection(meta, envelope);
}

/// 兼容路径：对经 `envelope_to_session_notification` 产出的 ACP SessionInfoUpdate，
/// 仍从 ACP meta 里提取 turn_terminal / executor_session_bound 投影。
fn apply_compat_info_projection(meta: &mut SessionMeta, envelope: &BackboneEnvelope) {
    if let Some(notification) = agentdash_protocol::envelope_to_session_notification(envelope) {
        use agent_client_protocol::SessionUpdate;
        if let SessionUpdate::SessionInfoUpdate(info) = &notification.update {
            if let Some((turn_id, terminal_kind, message)) =
                super::hub_support::parse_turn_terminal_event(info.meta.as_ref())
            {
                meta.last_turn_id = Some(turn_id);
                meta.last_terminal_message = message;
                meta.last_execution_status = terminal_kind.into();
            }

            if let Some(expected_turn_id) = meta.last_turn_id.as_deref() {
                if let Some(executor_session_id) =
                    parse_executor_session_bound(info.meta.as_ref(), expected_turn_id)
                {
                    meta.executor_session_id = Some(executor_session_id);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::types::TitleSource;
    use super::*;
    use agentdash_protocol::{
        BackboneEnvelope, BackboneEvent, PlatformEvent, SourceInfo, TraceInfo,
    };

    fn turn_terminal_envelope(
        session_id: &str,
        turn_id: &str,
        terminal_type: &str,
        message: &str,
    ) -> BackboneEnvelope {
        let key = "turn_terminal";
        let value = serde_json::json!({
            "terminal_type": terminal_type,
            "message": message,
        });
        BackboneEnvelope::new(
            BackboneEvent::Platform(PlatformEvent::SessionMetaUpdate {
                key: key.to_string(),
                value,
            }),
            session_id,
            SourceInfo {
                connector_id: "test".to_string(),
                connector_type: "unit".to_string(),
                executor_id: None,
            },
        )
        .with_trace(TraceInfo {
            turn_id: Some(turn_id.to_string()),
            entry_index: None,
        })
    }

    #[tokio::test]
    async fn save_session_meta_keeps_newer_event_projection() {
        let persistence = MemorySessionPersistence::default();
        let meta = SessionMeta {
            id: "sess-memory".to_string(),
            title: "测试".to_string(),
            title_source: TitleSource::Auto,
            created_at: 1,
            updated_at: 1,
            last_event_seq: 0,
            last_execution_status: ExecutionStatus::Idle,
            last_turn_id: None,
            last_terminal_message: None,
            executor_config: None,
            executor_session_id: None,
            companion_context: None,
            visible_canvas_mount_ids: Vec::new(),
            bootstrap_state: SessionBootstrapState::Plain,
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
        stale.last_execution_status = ExecutionStatus::Running;
        stale.last_turn_id = Some("t-old".to_string());
        stale.executor_session_id = Some("exec-1".to_string());
        stale.visible_canvas_mount_ids = vec!["canvas-a".to_string()];

        persistence
            .append_event(
                "sess-memory",
                &turn_terminal_envelope("sess-memory", "t-new", "turn_completed", "done"),
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
        assert_eq!(merged.executor_session_id.as_deref(), Some("exec-1"));
        assert_eq!(merged.visible_canvas_mount_ids, vec!["canvas-a"]);
    }
}

pub(super) fn backbone_event_type_name(event: &BackboneEvent) -> &'static str {
    match event {
        BackboneEvent::AgentMessageDelta(_) => "agent_message_delta",
        BackboneEvent::ReasoningTextDelta(_) => "reasoning_text_delta",
        BackboneEvent::ReasoningSummaryDelta(_) => "reasoning_summary_delta",
        BackboneEvent::ItemStarted(_) => "item_started",
        BackboneEvent::ItemCompleted(_) => "item_completed",
        BackboneEvent::CommandOutputDelta(_) => "command_output_delta",
        BackboneEvent::FileChangeDelta(_) => "file_change_delta",
        BackboneEvent::McpToolCallProgress(_) => "mcp_tool_call_progress",
        BackboneEvent::TurnStarted(_) => "turn_started",
        BackboneEvent::TurnCompleted(_) => "turn_completed",
        BackboneEvent::TurnDiffUpdated(_) => "turn_diff_updated",
        BackboneEvent::TurnPlanUpdated(_) => "turn_plan_updated",
        BackboneEvent::PlanDelta(_) => "plan_delta",
        BackboneEvent::TokenUsageUpdated(_) => "token_usage_updated",
        BackboneEvent::ThreadStatusChanged(_) => "thread_status_changed",
        BackboneEvent::ContextCompacted(_) => "context_compacted",
        BackboneEvent::ApprovalRequest(_) => "approval_request",
        BackboneEvent::Error(_) => "error",
        BackboneEvent::Platform(_) => "platform_event",
    }
}
