use std::{io, sync::Arc};

use agentdash_agent_protocol::{
    BackboneEnvelope, BackboneEvent, PlatformEvent, SourceInfo, TraceInfo,
};
use agentdash_agent_types::{AgentMessage, MessageRef};
use agentdash_spi::hooks::ContextFrame;
use tokio::sync::broadcast;

use super::compaction_context_frame::build_compaction_context_frame;
use super::continuation::build_projected_transcript_from_events;
use super::hub_support::SessionEventSubscription;
use super::persistence::{PersistedSessionEvent, SessionEventPage, SessionStoreSet};
use super::runtime_registry::SessionRuntimeRegistry;
use super::types::TitleSource;

#[derive(Clone)]
pub struct SessionEventingService {
    stores: SessionStoreSet,
    runtime_registry: SessionRuntimeRegistry,
    connector: Arc<dyn agentdash_spi::AgentConnector>,
}

impl SessionEventingService {
    pub(super) fn new(
        stores: SessionStoreSet,
        runtime_registry: SessionRuntimeRegistry,
        connector: Arc<dyn agentdash_spi::AgentConnector>,
    ) -> Self {
        Self {
            stores,
            runtime_registry,
            connector,
        }
    }

    pub async fn ensure_session(
        &self,
        session_id: &str,
    ) -> broadcast::Receiver<PersistedSessionEvent> {
        self.runtime_registry.subscribe(session_id).await
    }

    pub async fn subscribe_with_history(
        &self,
        session_id: &str,
    ) -> io::Result<SessionEventSubscription> {
        self.subscribe_after(session_id, 0).await
    }

    pub async fn subscribe_after(
        &self,
        session_id: &str,
        after_seq: u64,
    ) -> io::Result<SessionEventSubscription> {
        let rx = self.ensure_session(session_id).await;
        let backlog = self
            .stores
            .events
            .read_backlog(session_id, after_seq)
            .await?;
        Ok(SessionEventSubscription {
            snapshot_seq: backlog.snapshot_seq,
            backlog: backlog.events,
            rx,
        })
    }

    pub async fn list_event_page(
        &self,
        session_id: &str,
        after_seq: u64,
        limit: u32,
    ) -> io::Result<SessionEventPage> {
        self.stores
            .events
            .list_event_page(session_id, after_seq, limit)
            .await
    }

    pub(crate) fn supports_source_session_title(&self) -> bool {
        self.connector.capabilities().supports_source_session_title
    }

    pub async fn inject_notification(
        &self,
        session_id: &str,
        envelope: BackboneEnvelope,
    ) -> io::Result<()> {
        let _ = self.persist_notification(session_id, envelope).await?;
        Ok(())
    }

    pub(crate) async fn persist_notification(
        &self,
        session_id: &str,
        envelope: BackboneEnvelope,
    ) -> io::Result<PersistedSessionEvent> {
        let envelope = self
            .maybe_enrich_compaction_notification(session_id, envelope)
            .await?;
        let persisted = self
            .stores
            .events
            .append_event(session_id, &envelope)
            .await?;
        let tx = self.runtime_registry.touch_and_sender(session_id).await;
        let _ = tx.send(persisted.clone());
        if let BackboneEvent::Platform(PlatformEvent::SessionMetaUpdate { key, value }) =
            &persisted.notification.event
            && key == "context_compacted"
            && let Some(frame) = build_compaction_context_frame(value)
        {
            let _ = self
                .persist_context_frame_direct(session_id, persisted.turn_id.as_deref(), &frame)
                .await;
        }
        self.project_source_session_title(session_id, &persisted)
            .await?;
        Ok(persisted)
    }

    async fn project_source_session_title(
        &self,
        session_id: &str,
        persisted: &PersistedSessionEvent,
    ) -> io::Result<()> {
        let BackboneEvent::Platform(PlatformEvent::SourceSessionTitleUpdated {
            executor_session_id,
            title,
            preview,
            source,
        }) = &persisted.notification.event
        else {
            return Ok(());
        };

        let title = title.trim();
        if title.is_empty()
            || preview
                .as_deref()
                .is_some_and(|value| value.trim() == title)
        {
            return Ok(());
        }

        let Some(mut meta) = self.stores.meta.get_session_meta(session_id).await? else {
            return Ok(());
        };
        if meta.title_source == TitleSource::User {
            return Ok(());
        }
        if let (Some(expected), Some(actual)) = (
            meta.executor_session_id.as_deref(),
            executor_session_id.as_deref(),
        ) && expected != actual
        {
            tracing::warn!(
                session_id = %session_id,
                source = %source,
                expected_executor_session_id = %expected,
                actual_executor_session_id = %actual,
                "忽略不属于当前 executor session 的来源标题"
            );
            return Ok(());
        }
        if meta.title_source == TitleSource::Source && meta.title == title {
            return Ok(());
        }

        meta.title = title.to_string();
        meta.title_source = TitleSource::Source;
        meta.updated_at = chrono::Utc::now().timestamp_millis();
        self.stores.meta.save_session_meta(&meta).await?;

        let envelope = BackboneEnvelope::new(
            BackboneEvent::Platform(PlatformEvent::SessionMetaUpdate {
                key: "session_meta_updated".to_string(),
                value: serde_json::json!({
                    "title": meta.title,
                    "title_source": meta.title_source,
                }),
            }),
            session_id,
            self.connector_source(None),
        )
        .with_trace(TraceInfo {
            turn_id: persisted.turn_id.clone(),
            entry_index: persisted.entry_index,
        });
        let _ = self
            .persist_platform_event_direct(session_id, &envelope)
            .await?;
        Ok(())
    }

    pub(crate) async fn emit_capability_state_changed(
        &self,
        session_id: &str,
        turn_id: Option<&str>,
        value: serde_json::Value,
    ) -> io::Result<PersistedSessionEvent> {
        let envelope = BackboneEnvelope::new(
            BackboneEvent::Platform(PlatformEvent::SessionMetaUpdate {
                key: "capability_state_changed".to_string(),
                value,
            }),
            session_id,
            self.connector_source(None),
        )
        .with_trace(TraceInfo {
            turn_id: turn_id.map(ToString::to_string),
            entry_index: None,
        });

        self.persist_notification(session_id, envelope).await
    }

    pub(crate) async fn emit_context_frame(
        &self,
        session_id: &str,
        turn_id: Option<&str>,
        notice: &ContextFrame,
    ) -> io::Result<PersistedSessionEvent> {
        let value = serde_json::to_value(notice).map_err(|error| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("runtime context notice 序列化失败: {error}"),
            )
        })?;
        let envelope = BackboneEnvelope::new(
            BackboneEvent::Platform(PlatformEvent::SessionMetaUpdate {
                key: "context_frame".to_string(),
                value,
            }),
            session_id,
            self.connector_source(None),
        )
        .with_trace(TraceInfo {
            turn_id: turn_id.map(ToString::to_string),
            entry_index: None,
        });

        self.persist_notification(session_id, envelope).await
    }

    pub async fn build_projected_transcript(
        &self,
        session_id: &str,
    ) -> io::Result<agentdash_agent_types::ProjectedTranscript> {
        let events = self.stores.events.list_all_events(session_id).await?;
        Ok(build_projected_transcript_from_events(&events))
    }

    async fn maybe_enrich_compaction_notification(
        &self,
        session_id: &str,
        envelope: BackboneEnvelope,
    ) -> io::Result<BackboneEnvelope> {
        let messages_compacted = match &envelope.event {
            BackboneEvent::Platform(PlatformEvent::SessionMetaUpdate { key, value })
                if key == "context_compacted" =>
            {
                value
                    .get("messages_compacted")
                    .and_then(serde_json::Value::as_u64)
                    .and_then(|value| u32::try_from(value).ok())
            }
            _ => None,
        };

        let Some(messages_compacted) = messages_compacted else {
            return Ok(envelope);
        };

        let Some(compacted_until_ref) = self
            .derive_compaction_boundary_ref(session_id, messages_compacted)
            .await?
        else {
            return Ok(envelope);
        };

        let mut enriched = envelope;
        if let BackboneEvent::Platform(PlatformEvent::SessionMetaUpdate { value, .. }) =
            &mut enriched.event
            && let Some(obj) = value.as_object_mut()
            && let Ok(ref_value) = serde_json::to_value(&compacted_until_ref)
        {
            obj.insert("compacted_until_ref".to_string(), ref_value);
        }
        Ok(enriched)
    }

    async fn derive_compaction_boundary_ref(
        &self,
        session_id: &str,
        messages_compacted: u32,
    ) -> io::Result<Option<MessageRef>> {
        if messages_compacted == 0 {
            return Ok(None);
        }

        let events = self.stores.events.list_all_events(session_id).await?;
        Ok(derive_compaction_boundary_ref_from_events(
            &events,
            messages_compacted,
        ))
    }

    async fn persist_context_frame_direct(
        &self,
        session_id: &str,
        turn_id: Option<&str>,
        frame: &ContextFrame,
    ) -> io::Result<PersistedSessionEvent> {
        let value = serde_json::to_value(frame).map_err(|error| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("context frame 序列化失败: {error}"),
            )
        })?;
        let envelope = BackboneEnvelope::new(
            BackboneEvent::Platform(PlatformEvent::SessionMetaUpdate {
                key: "context_frame".to_string(),
                value,
            }),
            session_id,
            self.connector_source(None),
        )
        .with_trace(TraceInfo {
            turn_id: turn_id.map(ToString::to_string),
            entry_index: None,
        });

        self.persist_platform_event_direct(session_id, &envelope)
            .await
    }

    async fn persist_platform_event_direct(
        &self,
        session_id: &str,
        envelope: &BackboneEnvelope,
    ) -> io::Result<PersistedSessionEvent> {
        let persisted = self
            .stores
            .events
            .append_event(session_id, envelope)
            .await?;
        let tx = self.runtime_registry.touch_and_sender(session_id).await;
        let _ = tx.send(persisted.clone());
        Ok(persisted)
    }

    fn connector_source(&self, executor_id: Option<String>) -> SourceInfo {
        let connector_type = match self.connector.connector_type() {
            agentdash_spi::ConnectorType::LocalExecutor => "local_executor",
            agentdash_spi::ConnectorType::RemoteAcpBackend => "remote_acp_backend",
        };
        SourceInfo {
            connector_id: self.connector.connector_id().to_string(),
            connector_type: connector_type.to_string(),
            executor_id,
        }
    }
}

pub(crate) fn derive_compaction_boundary_ref_from_events(
    events: &[PersistedSessionEvent],
    messages_compacted: u32,
) -> Option<MessageRef> {
    if messages_compacted == 0 {
        return None;
    }

    let transcript = build_projected_transcript_from_events(events);
    let first_entry = transcript.entries.first()?;
    let (start_index, previously_compacted, previous_boundary_ref) = match &first_entry.message {
        AgentMessage::CompactionSummary {
            messages_compacted,
            compacted_until_ref,
            ..
        } => (
            1_usize,
            usize::try_from(*messages_compacted).ok()?,
            compacted_until_ref.clone(),
        ),
        _ => (0, 0, None),
    };

    let total_compacted = usize::try_from(messages_compacted).ok()?;
    if total_compacted < previously_compacted {
        return None;
    }
    if total_compacted == previously_compacted {
        return previous_boundary_ref;
    }

    let cut = start_index.checked_add(total_compacted - previously_compacted)?;
    transcript
        .entries
        .get(cut.checked_sub(1)?)
        .map(|entry| entry.message_ref.clone())
}

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, path::PathBuf, sync::Arc};

    use agentdash_spi::{
        AgentConnector, AgentInfo, ConnectorCapabilities, ConnectorError, ConnectorType,
        ExecutionContext, ExecutionStream, PromptPayload,
    };
    use tokio::sync::Mutex;
    use tokio_stream::wrappers::ReceiverStream;

    use super::*;
    use crate::session::{
        MemorySessionPersistence,
        persistence::SessionStoreSet,
        types::{ExecutionStatus, SessionBootstrapState, SessionMeta},
    };

    fn test_eventing_service(stores: SessionStoreSet) -> SessionEventingService {
        SessionEventingService::new(
            stores,
            SessionRuntimeRegistry::new(Arc::new(Mutex::new(HashMap::new()))),
            Arc::new(NoopConnector),
        )
    }

    fn test_meta(session_id: &str, title_source: TitleSource) -> SessionMeta {
        SessionMeta {
            id: session_id.to_string(),
            title: "New session".to_string(),
            title_source,
            created_at: 1,
            updated_at: 1,
            last_event_seq: 0,
            last_execution_status: ExecutionStatus::Idle,
            last_turn_id: None,
            last_terminal_message: None,
            executor_config: None,
            executor_session_id: Some("thread-1".to_string()),
            companion_context: None,
            tab_layout: None,
            visible_canvas_mount_ids: Vec::new(),
            bootstrap_state: SessionBootstrapState::Plain,
        }
    }

    fn source_title_envelope(session_id: &str, title: &str) -> BackboneEnvelope {
        BackboneEnvelope::new(
            BackboneEvent::Platform(PlatformEvent::SourceSessionTitleUpdated {
                executor_session_id: Some("thread-1".to_string()),
                title: title.to_string(),
                preview: Some("first user prompt".to_string()),
                source: "codex".to_string(),
            }),
            session_id,
            SourceInfo {
                connector_id: "codex-bridge".to_string(),
                connector_type: "local_executor".to_string(),
                executor_id: Some("CODEX".to_string()),
            },
        )
    }

    #[tokio::test]
    async fn source_session_title_projects_to_session_meta() {
        let session_id = "sess-source-title";
        let persistence = Arc::new(MemorySessionPersistence::default());
        let stores = SessionStoreSet::from_persistence(persistence);
        stores
            .meta
            .create_session(&test_meta(session_id, TitleSource::Auto))
            .await
            .expect("create session");
        let service = test_eventing_service(stores.clone());

        service
            .persist_notification(
                session_id,
                source_title_envelope(session_id, "  Codex Title  "),
            )
            .await
            .expect("persist source title");

        let meta = stores
            .meta
            .get_session_meta(session_id)
            .await
            .expect("read session meta")
            .expect("session meta exists");
        assert_eq!(meta.title, "Codex Title");
        assert_eq!(meta.title_source, TitleSource::Source);

        let events = stores
            .events
            .list_all_events(session_id)
            .await
            .expect("read events");
        assert_eq!(events.len(), 2);
        match &events[1].notification.event {
            BackboneEvent::Platform(PlatformEvent::SessionMetaUpdate { key, value }) => {
                assert_eq!(key, "session_meta_updated");
                assert_eq!(
                    value.get("title").and_then(serde_json::Value::as_str),
                    Some("Codex Title")
                );
                assert_eq!(
                    value
                        .get("title_source")
                        .and_then(serde_json::Value::as_str),
                    Some("source")
                );
            }
            event => panic!("expected session_meta_updated event, got {event:?}"),
        }
    }

    #[tokio::test]
    async fn source_session_title_does_not_overwrite_user_title() {
        let session_id = "sess-user-title";
        let persistence = Arc::new(MemorySessionPersistence::default());
        let stores = SessionStoreSet::from_persistence(persistence);
        let mut meta = test_meta(session_id, TitleSource::User);
        meta.title = "Pinned title".to_string();
        stores
            .meta
            .create_session(&meta)
            .await
            .expect("create session");
        let service = test_eventing_service(stores.clone());

        service
            .persist_notification(session_id, source_title_envelope(session_id, "Codex Title"))
            .await
            .expect("persist source title");

        let meta = stores
            .meta
            .get_session_meta(session_id)
            .await
            .expect("read session meta")
            .expect("session meta exists");
        assert_eq!(meta.title, "Pinned title");
        assert_eq!(meta.title_source, TitleSource::User);
    }

    #[tokio::test]
    async fn source_session_title_ignores_preview_title() {
        let session_id = "sess-preview-title";
        let persistence = Arc::new(MemorySessionPersistence::default());
        let stores = SessionStoreSet::from_persistence(persistence);
        stores
            .meta
            .create_session(&test_meta(session_id, TitleSource::Auto))
            .await
            .expect("create session");
        let service = test_eventing_service(stores.clone());

        service
            .persist_notification(
                session_id,
                source_title_envelope(session_id, " first user prompt "),
            )
            .await
            .expect("persist source title");

        let meta = stores
            .meta
            .get_session_meta(session_id)
            .await
            .expect("read session meta")
            .expect("session meta exists");
        assert_eq!(meta.title, "New session");
        assert_eq!(meta.title_source, TitleSource::Auto);
    }

    struct NoopConnector;

    #[async_trait::async_trait]
    impl AgentConnector for NoopConnector {
        fn connector_id(&self) -> &'static str {
            "noop"
        }

        fn connector_type(&self) -> ConnectorType {
            ConnectorType::LocalExecutor
        }

        fn capabilities(&self) -> ConnectorCapabilities {
            ConnectorCapabilities::default()
        }

        fn list_executors(&self) -> Vec<AgentInfo> {
            Vec::new()
        }

        async fn discover_options_stream(
            &self,
            _executor: &str,
            _working_dir: Option<PathBuf>,
        ) -> Result<futures::stream::BoxStream<'static, json_patch::Patch>, ConnectorError>
        {
            Ok(Box::pin(futures::stream::empty()))
        }

        async fn prompt(
            &self,
            _session_id: &str,
            _follow_up_session_id: Option<&str>,
            _prompt: &PromptPayload,
            _context: ExecutionContext,
        ) -> Result<ExecutionStream, ConnectorError> {
            let (_tx, rx) = tokio::sync::mpsc::channel(1);
            Ok(Box::pin(ReceiverStream::new(rx)))
        }

        async fn cancel(&self, _session_id: &str) -> Result<(), ConnectorError> {
            Ok(())
        }

        async fn approve_tool_call(
            &self,
            _session_id: &str,
            _tool_call_id: &str,
        ) -> Result<(), ConnectorError> {
            Ok(())
        }

        async fn reject_tool_call(
            &self,
            _session_id: &str,
            _tool_call_id: &str,
            _reason: Option<String>,
        ) -> Result<(), ConnectorError> {
            Ok(())
        }
    }
}
