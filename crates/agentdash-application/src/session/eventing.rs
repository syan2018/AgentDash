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
        Ok(persisted)
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

        let persisted = self
            .stores
            .events
            .append_event(session_id, &envelope)
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
