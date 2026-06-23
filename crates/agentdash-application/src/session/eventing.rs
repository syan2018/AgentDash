use std::{io, sync::Arc};

use agentdash_agent_protocol::{
    BackboneEnvelope, BackboneEvent, PlatformEvent, SourceInfo, TraceInfo, UserInputBlock,
    codex_app_server_protocol as codex,
};
use agentdash_agent_types::{AgentDashNativeThreadItem, AgentDashThreadItem, MessageRef};
use agentdash_spi::SESSION_PROJECTION_KIND_MODEL_CONTEXT;
use agentdash_spi::hooks::ContextFrame;
use tokio::sync::broadcast;

use super::compaction_context_frame::build_compaction_context_frame;
use super::context_projector::ContextProjector;
use super::context_usage_projection::{
    SessionContextProjectionReadModel, SessionContextUsageItem,
    build_session_context_projection_read_model, context_usage_items_from_context_frame,
};
use super::continuation::build_raw_projected_transcript_from_events;
use super::hub_support::SessionEventSubscription;
use super::persistence::{
    CompactionProjectionCommitResult, NewCompactionProjectionCommit, PersistedSessionEvent,
    SessionCompactionRecord, SessionCompactionStatus, SessionEventPage,
    SessionProjectionHeadRecord, SessionProjectionSegmentRecord, SessionStoreSet,
};
use super::runtime_registry::SessionRuntimeRegistry;
use super::types::TitleSource;

const SESSION_EVENT_APPEND_GUARD_MAX_BYTES: usize = 256 * 1024;
const SESSION_EVENT_APPEND_GUARD_FIELD_REPLACEMENT_MAX_BYTES: usize = 16 * 1024;
const SESSION_EVENT_APPEND_GUARD_POLICY: &str = "drop_known_output_fields_v1";

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
            .map_err(Into::into)
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
        self.persist_notification_inner(session_id, envelope, true)
            .await
    }

    pub(crate) async fn persist_notification_deferred_broadcast(
        &self,
        session_id: &str,
        envelope: BackboneEnvelope,
    ) -> io::Result<PersistedSessionEvent> {
        self.persist_notification_inner(session_id, envelope, false)
            .await
    }

    pub(crate) async fn broadcast_persisted_event(
        &self,
        session_id: &str,
        mut event: PersistedSessionEvent,
    ) {
        match bound_envelope_for_append(event.notification.clone()) {
            Ok(notification) => {
                event.notification = notification;
            }
            Err(error) => {
                tracing::warn!(
                    session_id = %session_id,
                    event_seq = event.event_seq,
                    error = %error,
                    "SessionEventingService broadcast guard 无法测量 BackboneEnvelope，继续发送已持久化事件"
                );
            }
        }
        let tx = self.runtime_registry.touch_and_sender(session_id).await;
        let _ = tx.send(event);
    }

    async fn persist_notification_inner(
        &self,
        session_id: &str,
        envelope: BackboneEnvelope,
        broadcast: bool,
    ) -> io::Result<PersistedSessionEvent> {
        let envelope = bound_envelope_for_append(envelope)?;
        if let Some(result) = self
            .maybe_commit_compaction_projection(session_id, envelope.clone())
            .await?
        {
            if broadcast {
                self.broadcast_persisted_event(session_id, result.event.clone())
                    .await;
            }
            if let BackboneEvent::Platform(PlatformEvent::SessionMetaUpdate { key, value }) =
                &result.event.notification.event
                && key == "context_compacted"
                && let Some(frame) = build_compaction_context_frame(value)
            {
                let _ = self
                    .persist_context_frame_direct(
                        session_id,
                        result.event.turn_id.as_deref(),
                        &frame,
                    )
                    .await;
            }
            self.project_source_session_title(session_id, &result.event)
                .await?;
            return Ok(result.event);
        }
        let persisted = self
            .stores
            .events
            .append_event(session_id, &envelope)
            .await?;
        self.advance_model_projection_head(session_id, &persisted)
            .await?;
        if broadcast {
            self.broadcast_persisted_event(session_id, persisted.clone())
                .await;
        }
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

    pub(crate) async fn emit_user_input_submitted(
        &self,
        session_id: &str,
        turn_id: &str,
        item_id: &str,
        submission_kind: agentdash_agent_protocol::UserInputSubmissionKind,
        input: Vec<UserInputBlock>,
    ) -> io::Result<PersistedSessionEvent> {
        let envelope = super::hub_support::build_user_input_submitted_envelope(
            session_id,
            &self.connector_source(None),
            turn_id,
            item_id,
            submission_kind,
            input,
        );
        self.persist_notification(session_id, envelope).await
    }

    pub async fn build_projected_transcript(
        &self,
        session_id: &str,
    ) -> io::Result<agentdash_agent_types::ProjectedTranscript> {
        ContextProjector::new(self.stores.clone())
            .build_projected_transcript(session_id)
            .await
    }

    pub async fn build_agent_context_envelope(
        &self,
        session_id: &str,
    ) -> io::Result<agentdash_agent_types::AgentContextEnvelope> {
        ContextProjector::new(self.stores.clone())
            .build_model_context(session_id)
            .await
    }

    pub async fn build_context_projection_read_model(
        &self,
        session_id: &str,
    ) -> io::Result<SessionContextProjectionReadModel> {
        let envelope = self.build_agent_context_envelope(session_id).await?;
        let context_items = self
            .build_context_usage_items(session_id, envelope.head_event_seq)
            .await?;
        Ok(build_session_context_projection_read_model(
            envelope,
            context_items,
        ))
    }

    pub async fn build_context_usage_items(
        &self,
        session_id: &str,
        head_event_seq: u64,
    ) -> io::Result<Vec<SessionContextUsageItem>> {
        let events = self.stores.events.list_all_events(session_id).await?;
        let mut frames = Vec::new();
        let mut seen_frame_ids = std::collections::HashSet::new();
        for event in events
            .iter()
            .filter(|event| event.event_seq <= head_event_seq)
            .rev()
        {
            let BackboneEvent::Platform(PlatformEvent::SessionMetaUpdate { key, value }) =
                &event.notification.event
            else {
                continue;
            };
            if key != "context_frame" {
                continue;
            }
            let Ok(frame) = serde_json::from_value::<ContextFrame>(value.clone()) else {
                continue;
            };
            if !seen_frame_ids.insert(frame.id.clone()) {
                continue;
            }
            frames.push((event.event_seq, event.turn_id.clone(), frame));
        }
        frames.reverse();
        let mut items = Vec::new();
        for (event_seq, turn_id, frame) in frames {
            items.extend(context_usage_items_from_context_frame(
                &frame,
                Some(event_seq),
                turn_id,
            ));
        }
        Ok(items)
    }

    async fn maybe_commit_compaction_projection(
        &self,
        session_id: &str,
        envelope: BackboneEnvelope,
    ) -> io::Result<Option<CompactionProjectionCommitResult>> {
        let Some(value) = context_compacted_value(&envelope) else {
            return Ok(None);
        };
        let Some(summary) = value
            .get("summary")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToString::to_string)
        else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "context_compacted 缺少 summary，拒绝提交 projection",
            ));
        };

        let events = self.stores.events.list_all_events(session_id).await?;
        let base_head_event_seq = latest_event_seq(&events);
        let projection_version = self
            .stores
            .projections
            .read_projection_head(session_id, SESSION_PROJECTION_KIND_MODEL_CONTEXT)
            .await?
            .map(|head| head.projection_version.saturating_add(1))
            .unwrap_or(1);
        let completed_event_seq = base_head_event_seq.saturating_add(1);

        let lifecycle_item_id = value
            .get("lifecycle_item_id")
            .and_then(serde_json::Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .map(ToString::to_string)
            .unwrap_or_else(|| format!("context-compaction-{completed_event_seq}"));
        let compaction_id = format!("compaction-{lifecycle_item_id}");
        let segment_id = format!("{compaction_id}-summary");

        let raw_transcript = build_raw_projected_transcript_from_events(&events);
        let messages_compacted = value
            .get("messages_compacted")
            .and_then(serde_json::Value::as_u64)
            .and_then(|value| u32::try_from(value).ok())
            .unwrap_or_default();
        let boundary_ref = value
            .get("compacted_until_ref")
            .cloned()
            .and_then(|value| serde_json::from_value::<MessageRef>(value).ok())
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    "context_compacted 缺少 compacted_until_ref，拒绝提交 projection",
                )
            })?;
        let Some(first_kept_ref_value) = value.get("first_kept_ref").cloned() else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "context_compacted 缺少 first_kept_ref，拒绝提交 projection",
            ));
        };
        let first_kept_ref = serde_json::from_value::<Option<MessageRef>>(first_kept_ref_value)
            .map_err(|error| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("context_compacted first_kept_ref 非法: {error}"),
                )
            })?;
        let source_end_event_seq =
            resolve_message_ref_source_event_seq(&raw_transcript.entries, &boundary_ref)
                .ok_or_else(|| {
                    io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!(
                            "context_compacted boundary {}:{} 不在当前 transcript 中",
                            boundary_ref.turn_id, boundary_ref.entry_index
                        ),
                    )
                })?;
        let source_start_event_seq = raw_transcript
            .entries
            .iter()
            .filter_map(projected_entry_source_event_seq)
            .find(|seq| *seq <= source_end_event_seq)
            .or(Some(source_end_event_seq));
        let first_kept_event_seq = match first_kept_ref.as_ref() {
            Some(first_kept_ref) => Some(
                resolve_message_ref_source_event_seq(&raw_transcript.entries, first_kept_ref)
                    .ok_or_else(|| {
                        io::Error::new(
                            io::ErrorKind::InvalidData,
                            format!(
                                "context_compacted first_kept_ref {}:{} 不在当前 transcript 中",
                                first_kept_ref.turn_id, first_kept_ref.entry_index
                            ),
                        )
                    })?,
            ),
            None => source_end_event_seq.checked_add(1),
        };
        let start_event_seq = find_compaction_started_event_seq(&events, &lifecycle_item_id)
            .unwrap_or(base_head_event_seq);
        let tokens_before = value
            .get("tokens_before")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or_default();
        let newly_compacted_messages = value
            .get("newly_compacted_messages")
            .and_then(serde_json::Value::as_u64)
            .and_then(|value| u32::try_from(value).ok());
        let timestamp_ms = value
            .get("timestamp_ms")
            .and_then(serde_json::Value::as_u64)
            .and_then(|value| i64::try_from(value).ok())
            .unwrap_or_else(|| chrono::Utc::now().timestamp_millis());

        let trigger = value
            .get("trigger")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("auto")
            .to_string();
        let reason = value
            .get("reason")
            .and_then(serde_json::Value::as_str)
            .map(ToString::to_string)
            .or_else(|| Some("token_pressure".to_string()));
        let phase = value
            .get("phase")
            .and_then(serde_json::Value::as_str)
            .map(ToString::to_string)
            .or_else(|| Some("pre_provider".to_string()));
        let strategy = value
            .get("strategy")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("summary_prefix")
            .to_string();
        let budget_scope = value
            .get("budget_scope")
            .and_then(serde_json::Value::as_str)
            .map(ToString::to_string)
            .or_else(|| Some("model_context".to_string()));

        let mut completed_event = envelope;
        enrich_context_compacted_commit_value(
            &mut completed_event,
            ContextCompactedCommitEnrichment {
                compaction_id: &compaction_id,
                projection_version,
                source_start_event_seq,
                source_end_event_seq,
                first_kept_event_seq,
                trigger: &trigger,
                phase: phase.as_deref(),
                strategy: &strategy,
            },
        );

        let commit = NewCompactionProjectionCommit {
            completed_event,
            compaction: SessionCompactionRecord {
                id: compaction_id.clone(),
                session_id: session_id.to_string(),
                projection_kind: SESSION_PROJECTION_KIND_MODEL_CONTEXT.to_string(),
                projection_version,
                lifecycle_item_id: lifecycle_item_id.clone(),
                start_event_seq,
                completed_event_seq: None,
                failed_event_seq: None,
                status: SessionCompactionStatus::ProjectionCommitted,
                trigger: trigger.clone(),
                reason,
                phase: phase.clone(),
                strategy: strategy.clone(),
                budget_scope,
                base_head_event_seq: Some(base_head_event_seq),
                source_start_event_seq,
                source_end_event_seq: Some(source_end_event_seq),
                first_kept_event_seq,
                summary: summary.clone(),
                replacement_projection_json: serde_json::json!({
                    "projection_kind": SESSION_PROJECTION_KIND_MODEL_CONTEXT,
                    "projection_version": projection_version,
                    "summary_segment_id": segment_id.clone(),
                    "source_start_event_seq": source_start_event_seq,
                    "source_end_event_seq": source_end_event_seq,
                    "first_kept_event_seq": first_kept_event_seq,
                    "compacted_until_ref": boundary_ref.clone(),
                    "first_kept_ref": first_kept_ref.clone(),
                }),
                token_stats_json: serde_json::json!({
                    "tokens_before": tokens_before,
                    "messages_compacted": messages_compacted,
                    "newly_compacted_messages": newly_compacted_messages,
                }),
                diagnostics_json: serde_json::json!({}),
                created_by: Some("agent".to_string()),
                created_at_ms: timestamp_ms,
                completed_at_ms: None,
            },
            segments: vec![SessionProjectionSegmentRecord {
                id: segment_id,
                session_id: session_id.to_string(),
                projection_kind: SESSION_PROJECTION_KIND_MODEL_CONTEXT.to_string(),
                projection_version,
                sort_order: 0,
                segment_type: "summary_chunk".to_string(),
                origin: "projection".to_string(),
                synthetic: true,
                source_start_event_seq,
                source_end_event_seq: Some(source_end_event_seq),
                source_refs_json: serde_json::json!({
                    "compacted_until_ref": boundary_ref.clone(),
                    "first_kept_ref": first_kept_ref.clone(),
                    "messages_compacted": messages_compacted,
                    "newly_compacted_messages": newly_compacted_messages,
                }),
                generated_by_compaction_id: Some(compaction_id.clone()),
                content_json: serde_json::json!({
                    "role": "system",
                    "content": summary.clone(),
                }),
                token_estimate: Some(estimate_text_tokens(&summary)),
                created_at_ms: timestamp_ms,
            }],
            head: SessionProjectionHeadRecord {
                session_id: session_id.to_string(),
                projection_kind: SESSION_PROJECTION_KIND_MODEL_CONTEXT.to_string(),
                projection_version,
                head_event_seq: completed_event_seq,
                active_compaction_id: Some(compaction_id),
                updated_by_event_seq: None,
                updated_at_ms: 0,
            },
        };

        Ok(self
            .stores
            .projections
            .commit_compaction_projection(session_id, commit)
            .await
            .map(Some)?)
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
        let envelope = bound_envelope_for_append(envelope.clone())?;
        let persisted = self
            .stores
            .events
            .append_event(session_id, &envelope)
            .await?;
        self.advance_model_projection_head(session_id, &persisted)
            .await?;
        let tx = self.runtime_registry.touch_and_sender(session_id).await;
        let _ = tx.send(persisted.clone());
        Ok(persisted)
    }

    async fn advance_model_projection_head(
        &self,
        session_id: &str,
        persisted: &PersistedSessionEvent,
    ) -> io::Result<()> {
        if matches!(
            &persisted.notification.event,
            BackboneEvent::ExecutorContextCompacted(_)
        ) {
            return Ok(());
        }
        let Some(mut head) = self
            .stores
            .projections
            .read_projection_head(session_id, SESSION_PROJECTION_KIND_MODEL_CONTEXT)
            .await?
        else {
            return Ok(());
        };
        if persisted.event_seq <= head.head_event_seq {
            return Ok(());
        }
        head.head_event_seq = persisted.event_seq;
        head.updated_by_event_seq = Some(persisted.event_seq);
        head.updated_at_ms = persisted.committed_at_ms;
        self.stores
            .projections
            .upsert_projection_head(head)
            .await
            .map_err(Into::into)
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

fn context_compacted_value(envelope: &BackboneEnvelope) -> Option<&serde_json::Value> {
    match &envelope.event {
        BackboneEvent::Platform(PlatformEvent::SessionMetaUpdate { key, value })
            if key == "context_compacted" =>
        {
            Some(value)
        }
        _ => None,
    }
}

fn bound_envelope_for_append(mut envelope: BackboneEnvelope) -> io::Result<BackboneEnvelope> {
    let original_bytes = serialized_envelope_len(&envelope)?;
    if original_bytes <= SESSION_EVENT_APPEND_GUARD_MAX_BYTES {
        return Ok(envelope);
    }

    let truncated_fields = bound_known_output_fields(&mut envelope, original_bytes);
    let bounded_bytes = serialized_envelope_len(&envelope)?;
    if truncated_fields > 0 {
        tracing::warn!(
            session_id = %envelope.session_id,
            event_type = backbone_event_type_name_for_guard(&envelope.event),
            turn_id = envelope.trace.turn_id.as_deref(),
            entry_index = envelope.trace.entry_index,
            original_bytes,
            bounded_bytes,
            truncated_fields,
            policy = SESSION_EVENT_APPEND_GUARD_POLICY,
            "SessionEventingService append guard 裁切了超大 BackboneEnvelope"
        );
    } else {
        tracing::warn!(
            session_id = %envelope.session_id,
            event_type = backbone_event_type_name_for_guard(&envelope.event),
            turn_id = envelope.trace.turn_id.as_deref(),
            entry_index = envelope.trace.entry_index,
            original_bytes,
            bounded_bytes,
            policy = SESSION_EVENT_APPEND_GUARD_POLICY,
            "SessionEventingService append guard 发现超大 BackboneEnvelope，但没有匹配到已知输出字段"
        );
    }
    Ok(envelope)
}

fn serialized_envelope_len(envelope: &BackboneEnvelope) -> io::Result<usize> {
    serde_json::to_vec(envelope)
        .map(|bytes| bytes.len())
        .map_err(|error| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("BackboneEnvelope 序列化失败: {error}"),
            )
        })
}

fn bound_known_output_fields(envelope: &mut BackboneEnvelope, original_bytes: usize) -> usize {
    let mut truncated_fields = 0;
    match &mut envelope.event {
        BackboneEvent::ItemCompleted(completed) => {
            truncated_fields += bound_thread_item_outputs(
                &mut completed.item,
                original_bytes,
                "item_completed.item",
            );
        }
        BackboneEvent::CommandOutputDelta(delta) => {
            truncated_fields += replace_if_large(
                &mut delta.delta,
                original_bytes,
                "command_output_delta.delta",
            );
        }
        BackboneEvent::McpToolCallProgress(progress) => {
            truncated_fields += replace_if_large(
                &mut progress.message,
                original_bytes,
                "mcp_tool_call_progress.message",
            );
        }
        BackboneEvent::Platform(PlatformEvent::TerminalOutput { data, .. }) => {
            truncated_fields +=
                replace_if_large(data, original_bytes, "platform.terminal_output.data");
        }
        _ => {}
    }
    truncated_fields
}

fn bound_thread_item_outputs(
    item: &mut AgentDashThreadItem,
    envelope_original_bytes: usize,
    field_prefix: &str,
) -> usize {
    match item {
        AgentDashThreadItem::Codex(item) => {
            bound_codex_thread_item_outputs(item, envelope_original_bytes, field_prefix)
        }
        AgentDashThreadItem::AgentDash(item) => {
            bound_agentdash_thread_item_outputs(item, envelope_original_bytes, field_prefix)
        }
    }
}

fn bound_codex_thread_item_outputs(
    item: &mut codex::ThreadItem,
    envelope_original_bytes: usize,
    field_prefix: &str,
) -> usize {
    match item {
        codex::ThreadItem::CommandExecution {
            id,
            aggregated_output,
            ..
        } => aggregated_output
            .as_mut()
            .map(|output| {
                replace_if_large(
                    output,
                    envelope_original_bytes,
                    &format!("{field_prefix}.commandExecution({id}).aggregatedOutput"),
                )
            })
            .unwrap_or_default(),
        codex::ThreadItem::DynamicToolCall {
            id, content_items, ..
        } => content_items
            .as_mut()
            .map(|items| {
                bound_content_items(
                    items,
                    envelope_original_bytes,
                    &format!("{field_prefix}.dynamicToolCall({id}).contentItems"),
                )
            })
            .unwrap_or_default(),
        codex::ThreadItem::McpToolCall {
            id, result, error, ..
        } => {
            let mut truncated_fields = 0;
            if let Some(result) = result.as_mut() {
                truncated_fields += replace_json_vec_if_large(
                    &mut result.content,
                    envelope_original_bytes,
                    &format!("{field_prefix}.mcpToolCall({id}).result.content"),
                );
                truncated_fields += replace_json_value_if_large(
                    &mut result.structured_content,
                    envelope_original_bytes,
                    &format!("{field_prefix}.mcpToolCall({id}).result.structuredContent"),
                );
                truncated_fields += replace_json_value_if_large(
                    &mut result.meta,
                    envelope_original_bytes,
                    &format!("{field_prefix}.mcpToolCall({id}).result._meta"),
                );
            }
            if let Some(error) = error.as_mut() {
                truncated_fields += replace_if_large(
                    &mut error.message,
                    envelope_original_bytes,
                    &format!("{field_prefix}.mcpToolCall({id}).error.message"),
                );
            }
            truncated_fields
        }
        _ => 0,
    }
}

fn bound_agentdash_thread_item_outputs(
    item: &mut AgentDashNativeThreadItem,
    envelope_original_bytes: usize,
    field_prefix: &str,
) -> usize {
    match item {
        AgentDashNativeThreadItem::ShellExec {
            id,
            aggregated_output,
            ..
        } => aggregated_output
            .as_mut()
            .map(|output| {
                replace_if_large(
                    output,
                    envelope_original_bytes,
                    &format!("{field_prefix}.shellExec({id}).aggregatedOutput"),
                )
            })
            .unwrap_or_default(),
        AgentDashNativeThreadItem::FsRead {
            id, content_items, ..
        } => content_items
            .as_mut()
            .map(|items| {
                bound_content_items(
                    items,
                    envelope_original_bytes,
                    &format!("{field_prefix}.fsRead({id}).contentItems"),
                )
            })
            .unwrap_or_default(),
        AgentDashNativeThreadItem::FsGrep {
            id, content_items, ..
        } => content_items
            .as_mut()
            .map(|items| {
                bound_content_items(
                    items,
                    envelope_original_bytes,
                    &format!("{field_prefix}.fsGrep({id}).contentItems"),
                )
            })
            .unwrap_or_default(),
        AgentDashNativeThreadItem::FsGlob {
            id, content_items, ..
        } => content_items
            .as_mut()
            .map(|items| {
                bound_content_items(
                    items,
                    envelope_original_bytes,
                    &format!("{field_prefix}.fsGlob({id}).contentItems"),
                )
            })
            .unwrap_or_default(),
    }
}

fn bound_content_items(
    items: &mut Vec<codex::DynamicToolCallOutputContentItem>,
    envelope_original_bytes: usize,
    field_prefix: &str,
) -> usize {
    if let Ok(bytes) = serde_json::to_vec(items)
        && bytes.len() > SESSION_EVENT_APPEND_GUARD_FIELD_REPLACEMENT_MAX_BYTES
    {
        *items = vec![codex::DynamicToolCallOutputContentItem::InputText {
            text: append_guard_diagnostic_text(field_prefix, envelope_original_bytes, bytes.len()),
        }];
        return 1;
    }

    let mut truncated_fields = 0;
    for (index, item) in items.iter_mut().enumerate() {
        match item {
            codex::DynamicToolCallOutputContentItem::InputText { text } => {
                truncated_fields += replace_if_large(
                    text,
                    envelope_original_bytes,
                    &format!("{field_prefix}[{index}].text"),
                );
            }
            codex::DynamicToolCallOutputContentItem::InputImage { image_url } => {
                truncated_fields += replace_if_large(
                    image_url,
                    envelope_original_bytes,
                    &format!("{field_prefix}[{index}].imageUrl"),
                );
            }
        }
    }
    truncated_fields
}

fn bound_json_values(
    values: &mut [serde_json::Value],
    envelope_original_bytes: usize,
    field_prefix: &str,
) -> usize {
    values
        .iter_mut()
        .enumerate()
        .map(|(index, value)| {
            replace_json_string_leaves(
                value,
                envelope_original_bytes,
                &format!("{field_prefix}[{index}]"),
            )
        })
        .sum()
}

fn replace_json_vec_if_large(
    values: &mut Vec<serde_json::Value>,
    envelope_original_bytes: usize,
    field_path: &str,
) -> usize {
    let Ok(bytes) = serde_json::to_vec(values) else {
        return 0;
    };
    if bytes.len() <= SESSION_EVENT_APPEND_GUARD_FIELD_REPLACEMENT_MAX_BYTES {
        return bound_json_values(values, envelope_original_bytes, field_path);
    }
    *values = vec![serde_json::json!({
        "type": "text",
        "text": append_guard_diagnostic_text(field_path, envelope_original_bytes, bytes.len()),
    })];
    1
}

fn replace_json_value_if_large(
    value: &mut Option<serde_json::Value>,
    envelope_original_bytes: usize,
    field_path: &str,
) -> usize {
    let Some(current) = value.as_mut() else {
        return 0;
    };
    let Ok(bytes) = serde_json::to_vec(current) else {
        return 0;
    };
    if bytes.len() <= SESSION_EVENT_APPEND_GUARD_FIELD_REPLACEMENT_MAX_BYTES {
        return replace_json_string_leaves(current, envelope_original_bytes, field_path);
    }
    *current = append_guard_diagnostic_value(field_path, envelope_original_bytes, bytes.len());
    1
}

fn replace_json_string_leaves(
    value: &mut serde_json::Value,
    envelope_original_bytes: usize,
    field_path: &str,
) -> usize {
    match value {
        serde_json::Value::String(text) => {
            replace_if_large(text, envelope_original_bytes, field_path)
        }
        serde_json::Value::Array(items) => items
            .iter_mut()
            .enumerate()
            .map(|(index, item)| {
                replace_json_string_leaves(
                    item,
                    envelope_original_bytes,
                    &format!("{field_path}[{index}]"),
                )
            })
            .sum(),
        serde_json::Value::Object(map) => map
            .iter_mut()
            .map(|(key, item)| {
                replace_json_string_leaves(
                    item,
                    envelope_original_bytes,
                    &format!("{field_path}.{key}"),
                )
            })
            .sum(),
        _ => 0,
    }
}

fn replace_if_large(text: &mut String, envelope_original_bytes: usize, field_path: &str) -> usize {
    let original_field_bytes = text.len();
    if original_field_bytes <= SESSION_EVENT_APPEND_GUARD_FIELD_REPLACEMENT_MAX_BYTES {
        return 0;
    }
    *text = append_guard_diagnostic_text(field_path, envelope_original_bytes, original_field_bytes);
    1
}

fn append_guard_diagnostic_text(
    field_path: &str,
    envelope_original_bytes: usize,
    original_field_bytes: usize,
) -> String {
    format!(
        "[session_eventing_append_guard] output omitted before persistence; field={field_path}; policy={SESSION_EVENT_APPEND_GUARD_POLICY}; envelope_original_bytes={envelope_original_bytes}; field_original_bytes={original_field_bytes}; inline_bytes=0"
    )
}

fn append_guard_diagnostic_value(
    field_path: &str,
    envelope_original_bytes: usize,
    original_field_bytes: usize,
) -> serde_json::Value {
    serde_json::json!({
        "type": "session_eventing_append_guard",
        "policy": SESSION_EVENT_APPEND_GUARD_POLICY,
        "field": field_path,
        "envelope_original_bytes": envelope_original_bytes,
        "field_original_bytes": original_field_bytes,
        "inline_bytes": 0,
    })
}

fn backbone_event_type_name_for_guard(event: &BackboneEvent) -> &'static str {
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
        BackboneEvent::UserInputSubmitted(_) => "user_input_submitted",
        BackboneEvent::TurnPlanUpdated(_) => "turn_plan_updated",
        BackboneEvent::PlanDelta(_) => "plan_delta",
        BackboneEvent::TokenUsageUpdated(_) => "token_usage_updated",
        BackboneEvent::ThreadStatusChanged(_) => "thread_status_changed",
        BackboneEvent::ExecutorContextCompacted(_) => "executor_context_compacted",
        BackboneEvent::ApprovalRequest(_) => "approval_request",
        BackboneEvent::Error(_) => "error",
        BackboneEvent::Platform(_) => "platform_event",
    }
}

fn latest_event_seq(events: &[PersistedSessionEvent]) -> u64 {
    events
        .iter()
        .map(|event| event.event_seq)
        .max()
        .unwrap_or_default()
}

fn resolve_message_ref_source_event_seq(
    entries: &[agentdash_agent_types::ProjectedEntry],
    message_ref: &MessageRef,
) -> Option<u64> {
    entries
        .iter()
        .find(|entry| &entry.message_ref == message_ref)
        .and_then(projected_entry_source_event_seq)
}

fn projected_entry_source_event_seq(entry: &agentdash_agent_types::ProjectedEntry) -> Option<u64> {
    entry
        .source_event_seq
        .or_else(|| entry.source_range.as_ref().map(|range| range.end_event_seq))
}

fn find_compaction_started_event_seq(
    events: &[PersistedSessionEvent],
    lifecycle_item_id: &str,
) -> Option<u64> {
    events
        .iter()
        .rev()
        .find_map(|event| match &event.notification.event {
            BackboneEvent::ItemStarted(started)
                if matches!(
                    started.item.as_codex(),
                    Some(codex::ThreadItem::ContextCompaction { id }) if id == lifecycle_item_id
                ) =>
            {
                Some(event.event_seq)
            }
            _ => None,
        })
}

struct ContextCompactedCommitEnrichment<'a> {
    compaction_id: &'a str,
    projection_version: u64,
    source_start_event_seq: Option<u64>,
    source_end_event_seq: u64,
    first_kept_event_seq: Option<u64>,
    trigger: &'a str,
    phase: Option<&'a str>,
    strategy: &'a str,
}

fn enrich_context_compacted_commit_value(
    envelope: &mut BackboneEnvelope,
    enrichment: ContextCompactedCommitEnrichment<'_>,
) {
    let BackboneEvent::Platform(PlatformEvent::SessionMetaUpdate { value, .. }) =
        &mut envelope.event
    else {
        return;
    };
    let Some(obj) = value.as_object_mut() else {
        return;
    };
    obj.insert(
        "compaction_id".to_string(),
        serde_json::Value::String(enrichment.compaction_id.to_string()),
    );
    obj.insert(
        "projection_kind".to_string(),
        serde_json::Value::String(SESSION_PROJECTION_KIND_MODEL_CONTEXT.to_string()),
    );
    obj.insert(
        "projection_version".to_string(),
        serde_json::json!(enrichment.projection_version),
    );
    obj.insert(
        "source_start_event_seq".to_string(),
        serde_json::to_value(enrichment.source_start_event_seq).unwrap_or(serde_json::Value::Null),
    );
    obj.insert(
        "source_end_event_seq".to_string(),
        serde_json::json!(enrichment.source_end_event_seq),
    );
    obj.insert(
        "first_kept_event_seq".to_string(),
        serde_json::to_value(enrichment.first_kept_event_seq).unwrap_or(serde_json::Value::Null),
    );
    obj.insert(
        "trigger".to_string(),
        serde_json::Value::String(enrichment.trigger.to_string()),
    );
    if let Some(phase) = enrichment.phase {
        obj.insert(
            "phase".to_string(),
            serde_json::Value::String(phase.to_string()),
        );
    }
    obj.insert(
        "strategy".to_string(),
        serde_json::Value::String(enrichment.strategy.to_string()),
    );
}

fn estimate_text_tokens(value: &str) -> u64 {
    let chars = u64::try_from(value.chars().count()).unwrap_or(u64::MAX);
    chars.saturating_add(3) / 4 + 4
}

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, path::PathBuf, sync::Arc};

    use agentdash_agent_protocol::backbone::item::ItemCompletedNotification;
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
        types::{ExecutionStatus, SessionMeta},
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
            last_delivery_status: ExecutionStatus::Idle,
            last_turn_id: None,
            last_terminal_message: None,
            executor_session_id: Some("thread-1".to_string()),
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
            test_source_info(),
        )
    }

    fn test_source_info() -> SourceInfo {
        SourceInfo {
            connector_id: "codex-bridge".to_string(),
            connector_type: "local_executor".to_string(),
            executor_id: Some("CODEX".to_string()),
        }
    }

    fn context_compacted_envelope(session_id: &str, value: serde_json::Value) -> BackboneEnvelope {
        BackboneEnvelope::new(
            BackboneEvent::Platform(PlatformEvent::SessionMetaUpdate {
                key: "context_compacted".to_string(),
                value,
            }),
            session_id,
            test_source_info(),
        )
        .with_trace(TraceInfo {
            turn_id: Some("turn-compact".to_string()),
            entry_index: None,
        })
    }

    fn executor_context_compacted_envelope(session_id: &str) -> BackboneEnvelope {
        BackboneEnvelope::new(
            BackboneEvent::ExecutorContextCompacted(codex::ContextCompactedNotification {
                thread_id: "thread-1".to_string(),
                turn_id: "turn-1".to_string(),
            }),
            session_id,
            test_source_info(),
        )
    }

    fn oversized_dynamic_tool_completed_envelope(
        session_id: &str,
        sentinel: &str,
    ) -> BackboneEnvelope {
        let huge_output = format!(
            "{}{}{}",
            sentinel,
            "x".repeat(SESSION_EVENT_APPEND_GUARD_MAX_BYTES + 32 * 1024),
            sentinel
        );
        let item = codex::ThreadItem::DynamicToolCall {
            id: "tool-item-1".to_string(),
            namespace: None,
            tool: "huge_output".to_string(),
            arguments: serde_json::json!({ "mode": "test" }),
            status: codex::DynamicToolCallStatus::Completed,
            content_items: Some(vec![codex::DynamicToolCallOutputContentItem::InputText {
                text: huge_output,
            }]),
            success: Some(true),
            duration_ms: None,
        };
        BackboneEnvelope::new(
            BackboneEvent::ItemCompleted(ItemCompletedNotification::new(
                item, "thread-1", "turn-1",
            )),
            session_id,
            test_source_info(),
        )
        .with_trace(TraceInfo {
            turn_id: Some("turn-1".to_string()),
            entry_index: Some(7),
        })
    }

    fn oversized_terminal_output_envelope(session_id: &str, sentinel: &str) -> BackboneEnvelope {
        let huge_output = format!(
            "{}{}{}",
            sentinel,
            "t".repeat(SESSION_EVENT_APPEND_GUARD_MAX_BYTES + 16 * 1024),
            sentinel
        );
        BackboneEnvelope::new(
            BackboneEvent::Platform(PlatformEvent::TerminalOutput {
                terminal_id: "term-large".to_string(),
                data: huge_output,
            }),
            session_id,
            test_source_info(),
        )
        .with_trace(TraceInfo {
            turn_id: Some("turn-terminal".to_string()),
            entry_index: Some(3),
        })
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

    #[tokio::test]
    async fn context_compacted_missing_summary_or_boundary_is_not_persisted() {
        let session_id = "sess-bad-context-compaction";
        let persistence = Arc::new(MemorySessionPersistence::default());
        let stores = SessionStoreSet::from_persistence(persistence);
        stores
            .meta
            .create_session(&test_meta(session_id, TitleSource::Auto))
            .await
            .expect("create session");
        let service = test_eventing_service(stores.clone());

        let missing_summary = service
            .persist_notification(
                session_id,
                context_compacted_envelope(
                    session_id,
                    serde_json::json!({
                        "messages_compacted": 2,
                        "compacted_until_ref": { "turn_id": "turn-1", "entry_index": 0 },
                        "first_kept_ref": null,
                    }),
                ),
            )
            .await;
        assert!(matches!(
            missing_summary,
            Err(error) if error.kind() == io::ErrorKind::InvalidData
        ));

        let missing_first_kept = service
            .persist_notification(
                session_id,
                context_compacted_envelope(
                    session_id,
                    serde_json::json!({
                        "summary": "历史摘要",
                        "messages_compacted": 2,
                        "compacted_until_ref": { "turn_id": "turn-1", "entry_index": 0 },
                    }),
                ),
            )
            .await;
        assert!(matches!(
            missing_first_kept,
            Err(error) if error.kind() == io::ErrorKind::InvalidData
        ));

        let events = stores
            .events
            .list_all_events(session_id)
            .await
            .expect("read events");
        assert!(events.is_empty());
        let head = stores
            .projections
            .read_projection_head(session_id, SESSION_PROJECTION_KIND_MODEL_CONTEXT)
            .await
            .expect("read projection head");
        assert!(head.is_none());
    }

    #[tokio::test]
    async fn executor_context_compacted_is_telemetry_and_does_not_advance_projection_head() {
        let session_id = "sess-external-compact";
        let persistence = Arc::new(MemorySessionPersistence::default());
        let stores = SessionStoreSet::from_persistence(persistence);
        stores
            .meta
            .create_session(&test_meta(session_id, TitleSource::Auto))
            .await
            .expect("create session");
        stores
            .projections
            .upsert_projection_head(SessionProjectionHeadRecord {
                session_id: session_id.to_string(),
                projection_kind: SESSION_PROJECTION_KIND_MODEL_CONTEXT.to_string(),
                projection_version: 1,
                head_event_seq: 0,
                active_compaction_id: Some("compaction-existing".to_string()),
                updated_by_event_seq: None,
                updated_at_ms: 1,
            })
            .await
            .expect("seed projection head");
        let service = test_eventing_service(stores.clone());

        let persisted = service
            .persist_notification(session_id, executor_context_compacted_envelope(session_id))
            .await
            .expect("persist external telemetry");
        assert_eq!(persisted.session_update_type, "executor_context_compacted");

        let head = stores
            .projections
            .read_projection_head(session_id, SESSION_PROJECTION_KIND_MODEL_CONTEXT)
            .await
            .expect("read projection head")
            .expect("projection head exists");
        assert_eq!(head.head_event_seq, 0);
        assert_eq!(head.updated_by_event_seq, None);
    }

    #[tokio::test]
    async fn append_guard_bounds_oversized_tool_completed_event_before_store_and_broadcast() {
        let session_id = "sess-append-guard-tool";
        let sentinel = "SENTINEL_TOOL_OUTPUT_SHOULD_NOT_PERSIST";
        let persistence = Arc::new(MemorySessionPersistence::default());
        let stores = SessionStoreSet::from_persistence(persistence);
        stores
            .meta
            .create_session(&test_meta(session_id, TitleSource::Auto))
            .await
            .expect("create session");
        let service = test_eventing_service(stores.clone());
        let mut broadcast_rx = service.ensure_session(session_id).await;

        let persisted = service
            .persist_notification(
                session_id,
                oversized_dynamic_tool_completed_envelope(session_id, sentinel),
            )
            .await
            .expect("persist oversized event");

        let persisted_json =
            serde_json::to_string(&persisted.notification).expect("serialize persisted event");
        assert!(
            persisted_json.len() < SESSION_EVENT_APPEND_GUARD_MAX_BYTES,
            "persisted event should be bounded, got {} bytes",
            persisted_json.len()
        );
        assert!(!persisted_json.contains(sentinel));
        assert!(persisted_json.contains("session_eventing_append_guard"));
        assert_eq!(persisted.turn_id.as_deref(), Some("turn-1"));
        assert_eq!(persisted.entry_index, Some(7));
        assert!(matches!(
            &persisted.notification.event,
            BackboneEvent::ItemCompleted(completed)
                if completed.item.id() == "tool-item-1"
        ));

        let backlog = service
            .subscribe_after(session_id, 0)
            .await
            .expect("read backlog");
        assert_eq!(backlog.backlog.len(), 1);
        let backlog_json =
            serde_json::to_string(&backlog.backlog[0].notification).expect("serialize backlog");
        assert!(!backlog_json.contains(sentinel));
        assert!(backlog_json.contains("session_eventing_append_guard"));

        let broadcasted = broadcast_rx.recv().await.expect("receive broadcast");
        let broadcast_json =
            serde_json::to_string(&broadcasted.notification).expect("serialize broadcast");
        assert!(!broadcast_json.contains(sentinel));
        assert!(broadcast_json.contains("session_eventing_append_guard"));
    }

    #[tokio::test]
    async fn append_guard_bounds_oversized_terminal_output_before_store_and_backlog() {
        let session_id = "sess-append-guard-terminal";
        let sentinel = "SENTINEL_TERMINAL_OUTPUT_SHOULD_NOT_PERSIST";
        let persistence = Arc::new(MemorySessionPersistence::default());
        let stores = SessionStoreSet::from_persistence(persistence);
        stores
            .meta
            .create_session(&test_meta(session_id, TitleSource::Auto))
            .await
            .expect("create session");
        let service = test_eventing_service(stores);

        let persisted = service
            .persist_notification(
                session_id,
                oversized_terminal_output_envelope(session_id, sentinel),
            )
            .await
            .expect("persist oversized terminal event");

        let persisted_json =
            serde_json::to_string(&persisted.notification).expect("serialize persisted event");
        assert!(
            persisted_json.len() < SESSION_EVENT_APPEND_GUARD_MAX_BYTES,
            "persisted terminal event should be bounded, got {} bytes",
            persisted_json.len()
        );
        assert!(!persisted_json.contains(sentinel));
        assert!(persisted_json.contains("session_eventing_append_guard"));
        assert_eq!(persisted.session_update_type, "platform_event");
        assert_eq!(persisted.turn_id.as_deref(), Some("turn-terminal"));
        assert_eq!(persisted.entry_index, Some(3));

        let backlog = service
            .subscribe_after(session_id, 0)
            .await
            .expect("read backlog");
        assert_eq!(backlog.backlog.len(), 1);
        let backlog_json =
            serde_json::to_string(&backlog.backlog[0].notification).expect("serialize backlog");
        assert!(!backlog_json.contains(sentinel));
        assert!(backlog_json.contains("session_eventing_append_guard"));
    }

    #[tokio::test]
    async fn append_guard_leaves_small_events_unchanged() {
        let session_id = "sess-append-guard-small";
        let persistence = Arc::new(MemorySessionPersistence::default());
        let stores = SessionStoreSet::from_persistence(persistence);
        stores
            .meta
            .create_session(&test_meta(session_id, TitleSource::Auto))
            .await
            .expect("create session");
        let service = test_eventing_service(stores);
        let envelope = BackboneEnvelope::new(
            BackboneEvent::Platform(PlatformEvent::TerminalOutput {
                terminal_id: "term-1".to_string(),
                data: "hello\n".to_string(),
            }),
            session_id,
            test_source_info(),
        )
        .with_trace(TraceInfo {
            turn_id: Some("turn-small".to_string()),
            entry_index: Some(2),
        });
        let original_json = serde_json::to_value(&envelope).expect("serialize original");

        let persisted = service
            .persist_notification(session_id, envelope)
            .await
            .expect("persist small event");

        assert_eq!(
            serde_json::to_value(&persisted.notification).expect("serialize persisted"),
            original_json
        );
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
