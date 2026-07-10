use agentdash_diagnostics::{DiagnosticErrorContext, Subsystem, diag, diag_error};
use std::{collections::HashMap, io, sync::Arc};

use agentdash_agent_protocol::{
    AgentDashNativeThreadItem, AgentDashThreadItem, BackboneEnvelope, BackboneEvent, PlatformEvent,
    SessionRewindReason, SessionRewound, SourceInfo, TraceInfo, UserInputBlock,
    codex_app_server_protocol as codex,
};
use agentdash_agent_types::{AgentContextEnvelope, AgentMessage, MessageRef};
use agentdash_domain::workflow::ManualContextCompactionRequestRepository;
use agentdash_spi::SESSION_PROJECTION_KIND_MODEL_CONTEXT;
use agentdash_spi::hooks::trace::{
    HookTraceStorageDisposition, hook_trace_payload_storage_disposition,
};
use agentdash_spi::hooks::{ContextDeliveryRecord, ContextFrame};
use tokio::sync::broadcast;

use super::compaction_context_frame::build_compaction_context_frame;
use super::context_projector::ContextProjector;
use super::context_usage_projection::{
    SessionContextProjectionReadModel, SessionContextUsageItem,
    build_session_context_projection_read_model, context_usage_items_from_context_frame,
};
use super::hub_support::{
    SessionEventSubscription, TurnTerminalKind, parse_turn_terminal_event_from_envelope,
};
use super::persistence::{
    CompactionProjectionCommitResult, NewCompactionProjectionCommit, PersistedSessionEvent,
    SessionCompactionRecord, SessionCompactionStatus, SessionEventPage, SessionEventingStores,
    SessionProjectionHeadRecord, SessionProjectionSegmentRecord,
};
use super::runtime_registry::SessionRuntimeRegistry;
use super::transcript_restore::build_raw_projected_transcript_from_events;

const SESSION_EVENT_APPEND_GUARD_MAX_BYTES: usize = 256 * 1024;
const SESSION_EVENT_APPEND_GUARD_FIELD_REPLACEMENT_MAX_BYTES: usize = 16 * 1024;
const SESSION_EVENT_APPEND_GUARD_MAX_JSON_DEPTH: usize = 64;
const SESSION_EVENT_APPEND_GUARD_MAX_JSON_NODES: usize = 4096;
const SESSION_EVENT_APPEND_GUARD_MAX_FIELD_PATH_KEY_CHARS: usize = 96;
const SESSION_EVENT_APPEND_GUARD_POLICY: &str = "drop_known_output_fields_v1";

/// 进程级 ephemeral epoch：本进程启动时确定一次（启动毫秒）。
/// 后端重启会得到新值，前端据此判定 `ephemeral_seq` 游标是否失效并重置。
/// 仅运行时使用 SystemTime（不在任何 workflow 脚本中）。
fn ephemeral_runtime_epoch() -> u64 {
    use std::sync::OnceLock;
    use std::time::{SystemTime, UNIX_EPOCH};
    static EPOCH: OnceLock<u64> = OnceLock::new();
    *EPOCH.get_or_init(|| {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0)
    })
}

#[derive(Clone)]
pub struct SessionEventingService {
    stores: SessionEventingStores,
    runtime_registry: SessionRuntimeRegistry,
    connector: Arc<dyn agentdash_spi::AgentConnector>,
    workspace_title_port:
        Option<Arc<dyn agentdash_application_ports::workspace_title::WorkspaceTitlePort>>,
    manual_context_compaction_request_repo:
        Option<Arc<dyn ManualContextCompactionRequestRepository>>,
}

impl SessionEventingService {
    pub(super) fn new(
        stores: SessionEventingStores,
        runtime_registry: SessionRuntimeRegistry,
        connector: Arc<dyn agentdash_spi::AgentConnector>,
    ) -> Self {
        Self {
            stores,
            runtime_registry,
            connector,
            workspace_title_port: None,
            manual_context_compaction_request_repo: None,
        }
    }

    pub(super) fn with_workspace_title_port(
        mut self,
        port: Option<Arc<dyn agentdash_application_ports::workspace_title::WorkspaceTitlePort>>,
    ) -> Self {
        self.workspace_title_port = port;
        self
    }

    pub(super) fn with_manual_context_compaction_request_repo(
        mut self,
        repo: Option<Arc<dyn ManualContextCompactionRequestRepository>>,
    ) -> Self {
        self.manual_context_compaction_request_repo = repo;
        self
    }

    async fn update_workspace_title(
        &self,
        session_id: &str,
        title: &str,
        title_source: &str,
    ) -> bool {
        let Some(port) = self.workspace_title_port.as_ref() else {
            return false;
        };
        port.update_workspace_title(session_id, title.to_string(), title_source)
            .await
            .unwrap_or_default()
    }

    pub(crate) async fn update_workspace_title_public(
        &self,
        session_id: &str,
        title: &str,
        title_source: &str,
    ) -> bool {
        self.update_workspace_title(session_id, title, title_source)
            .await
    }

    pub async fn ensure_session(
        &self,
        session_id: &str,
    ) -> broadcast::Receiver<PersistedSessionEvent> {
        self.runtime_registry.subscribe(session_id).await
    }

    /// 进程级 ephemeral epoch（启动时确定一次）。NDJSON `Connected` 携带此值，
    /// 前端据此判定后端是否重启以决定是否重置 `lastEphemeralSeq`。
    pub fn ephemeral_epoch(&self) -> u64 {
        ephemeral_runtime_epoch()
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
        // 先 ensure_session（订阅 rx）再 snapshot_ephemeral：保证 rx 在快照之前已订阅，
        // 快照与后续 live 广播的重叠由 ephemeral_seq 在前端去重消解，不会丢事件。
        let rx = self.ensure_session(session_id).await;
        let ephemeral_backlog = self.runtime_registry.snapshot_ephemeral(session_id).await;
        let backlog = self
            .stores
            .events
            .read_backlog(session_id, after_seq)
            .await?;
        Ok(SessionEventSubscription {
            snapshot_seq: backlog.snapshot_seq,
            backlog: backlog.events,
            ephemeral_backlog,
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

    pub async fn persist_notification(
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
                let context = DiagnosticErrorContext::new(
                    "session.eventing.broadcast_persisted_event",
                    "bound_envelope_for_broadcast",
                );
                diag_error!(
                    Warn,
                    Subsystem::AgentRun,
                    context = &context,
                    error = &error,
                    session_id = %session_id,
                    event_seq = event.event_seq,
                    event_kind = event.session_update_type.as_str(),
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
        if is_ephemeral_event(&envelope.event) {
            let now = chrono::Utc::now().timestamp_millis();
            let event = PersistedSessionEvent {
                session_id: session_id.to_string(),
                // 占位 0；push_ephemeral 会分配单调 ephemeral_seq 写入此字段。
                event_seq: 0,
                occurred_at_ms: now,
                committed_at_ms: now,
                session_update_type: backbone_event_type_name_for_guard(&envelope.event)
                    .to_string(),
                turn_id: envelope.trace.turn_id.clone(),
                entry_index: envelope.trace.entry_index,
                tool_call_id: ephemeral_tool_call_id(&envelope),
                ephemeral: true,
                notification: envelope,
            };
            // 先 push（分配 ephemeral_seq + 入 buffer），再 broadcast 带 seq 的事件。
            // 顺序保证：live 订阅者与 reconnect 快照看到的是同一个带 seq 的 envelope。
            let event = self
                .runtime_registry
                .push_ephemeral(session_id, event)
                .await;
            if broadcast {
                self.broadcast_persisted_event(session_id, event.clone())
                    .await;
            }
            return Ok(event);
        }
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
        // 终态助手消息 / reasoning 落 durable 后，剪除 buffer 中同 item_id 的在途 delta：
        // 避免 reconnect 时 durable backlog 已 SET 全文、ephemeral 快照又补发 in-flight delta 脏化（P1-b）。
        // 与 turn-terminal 的 clear_ephemeral、P2 epoch 不冲突：这里只精剪该消息的 delta。
        if let Some(item_id) = finalized_assistant_item_id(&persisted) {
            self.runtime_registry
                .prune_ephemeral_by_item_id(session_id, &item_id)
                .await;
        }
        // turn 收尾（terminal）/ rewind 后清空 ephemeral buffer：该 turn 的终态正文 / reasoning
        // 已 durable（Step 0），in-flight 进度态不再需要补发，避免跨 turn 累积。
        if should_clear_ephemeral_on_durable(&persisted) {
            self.runtime_registry.clear_ephemeral(session_id).await;
        }
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

        // Executor session guard: verify the title comes from the correct executor.
        let meta = self.stores.meta.get_session_meta(session_id).await?;
        if let Some(ref meta) = meta
            && let (Some(expected), Some(actual)) = (
                meta.executor_session_id.as_deref(),
                executor_session_id.as_deref(),
            )
            && expected != actual
        {
            diag!(Warn, Subsystem::AgentRun,

                operation = "session.eventing.project_source_session_title",
                stage = "executor_session_guard",
                session_id = %session_id,
                event_kind = "source_session_title_updated",
                source = %source,
                expected_executor_session_id = %expected,
                actual_executor_session_id = %actual,
                "忽略不属于当前 executor session 的来源标题"
            );
            return Ok(());
        }

        // Write title to workspace (LifecycleAgent) via port.
        let updated = self
            .update_workspace_title(session_id, title, "source")
            .await;
        if !updated {
            return Ok(());
        }

        // Emit event for frontend refresh.
        let envelope = BackboneEnvelope::new(
            BackboneEvent::Platform(PlatformEvent::SessionMetaUpdate {
                key: "session_meta_updated".to_string(),
                value: serde_json::json!({
                    "title": title,
                    "title_source": "source",
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

    pub(crate) async fn emit_context_delivery_record(
        &self,
        session_id: &str,
        turn_id: Option<&str>,
        record: &ContextDeliveryRecord,
    ) -> io::Result<PersistedSessionEvent> {
        let envelope = build_context_delivery_record_envelope(
            session_id,
            turn_id,
            record,
            self.connector_source(None),
        )?;
        self.persist_notification(session_id, envelope).await
    }

    pub async fn emit_user_input_submitted(
        &self,
        session_id: &str,
        turn_id: &str,
        item_id: &str,
        submission_kind: agentdash_agent_protocol::UserInputSubmissionKind,
        input_source: agentdash_agent_protocol::UserInputSource,
        input: Vec<UserInputBlock>,
    ) -> io::Result<PersistedSessionEvent> {
        let envelope = super::hub_support::build_user_input_submitted_envelope(
            session_id,
            &self.connector_source(None),
            turn_id,
            item_id,
            submission_kind,
            input_source,
            input,
        );
        self.persist_notification(session_id, envelope).await
    }

    pub async fn build_projected_transcript(
        &self,
        session_id: &str,
    ) -> io::Result<agentdash_agent_types::ProjectedTranscript> {
        Ok(self
            .build_agent_context_envelope(session_id)
            .await?
            .into_projected_transcript())
    }

    pub async fn build_agent_context_envelope(
        &self,
        session_id: &str,
    ) -> io::Result<agentdash_agent_types::AgentContextEnvelope> {
        let mut envelope = ContextProjector::new(self.stores.projection_stores())
            .build_model_context(session_id)
            .await?;
        self.apply_session_rewind_boundary(session_id, &mut envelope)
            .await?;
        Ok(envelope)
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn persist_session_rewound_marker(
        &self,
        session_id: &str,
        source: &SourceInfo,
        discarded_turn_id: &str,
        reason: &str,
        message: Option<String>,
        terminal_event_seq: u64,
        broadcast: bool,
    ) -> io::Result<PersistedSessionEvent> {
        let events = self.stores.events.list_all_events(session_id).await?;
        let stable = latest_stable_terminal_before(&events, terminal_event_seq);
        let discarded_entry_index =
            latest_agent_loop_entry_index_before(&events, discarded_turn_id, terminal_event_seq);
        let message = bounded_session_rewound_message(message);
        let envelope = BackboneEnvelope::new(
            BackboneEvent::Platform(PlatformEvent::SessionRewound(SessionRewound {
                discarded_turn_id: discarded_turn_id.to_string(),
                discarded_entry_index,
                stable_event_seq: stable
                    .as_ref()
                    .map(|boundary| boundary.event_seq)
                    .unwrap_or_default(),
                stable_turn_id: stable.map(|boundary| boundary.turn_id),
                reason: session_rewind_reason_from_str(reason),
                replacement_turn_id: None,
                message,
            })),
            session_id,
            source.clone(),
        )
        .with_trace(TraceInfo {
            turn_id: Some(discarded_turn_id.to_string()),
            entry_index: None,
        });
        self.persist_notification_inner(session_id, envelope, broadcast)
            .await
    }

    async fn apply_session_rewind_boundary(
        &self,
        session_id: &str,
        envelope: &mut AgentContextEnvelope,
    ) -> io::Result<()> {
        let events = self.stores.events.list_all_events(session_id).await?;
        let boundaries = session_rewind_boundaries(&events);
        if boundaries.is_empty() {
            return Ok(());
        }
        envelope.messages.retain(|message| {
            let Some(boundary) = boundaries.get(&message.message_ref.turn_id) else {
                return true;
            };
            if Some(message.message_ref.entry_index) != boundary.discarded_entry_index {
                return true;
            }
            !matches!(
                message.message,
                AgentMessage::Assistant { .. } | AgentMessage::ToolResult { .. }
            )
        });
        envelope.token_estimate = None;
        Ok(())
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
            .unwrap_or("token_pressure")
            .to_string();
        let phase = value
            .get("phase")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("pre_provider")
            .to_string();
        let strategy = value
            .get("strategy")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("summary_prefix")
            .to_string();
        let implementation = value
            .get("implementation")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("local_summary")
            .to_string();
        let request_id = value
            .get("request_id")
            .and_then(serde_json::Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .map(ToString::to_string);
        let request_id_for_status = request_id.clone();
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
                reason: &reason,
                phase: &phase,
                strategy: &strategy,
                implementation: &implementation,
                request_id: request_id.as_deref(),
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
                reason: Some(reason.clone()),
                phase: Some(phase.clone()),
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
                    "trigger": trigger.clone(),
                    "reason": reason.clone(),
                    "phase": phase.clone(),
                    "strategy": strategy.clone(),
                    "implementation": implementation.clone(),
                    "request_id": request_id.clone(),
                }),
                token_stats_json: serde_json::json!({
                    "tokens_before": tokens_before,
                    "messages_compacted": messages_compacted,
                    "newly_compacted_messages": newly_compacted_messages,
                }),
                diagnostics_json: serde_json::json!({
                    "trigger": trigger.clone(),
                    "reason": reason.clone(),
                    "phase": phase.clone(),
                    "strategy": strategy.clone(),
                    "implementation": implementation.clone(),
                    "request_id": request_id.clone(),
                }),
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
                    "trigger": trigger,
                    "reason": reason,
                    "phase": phase,
                    "strategy": strategy,
                    "implementation": implementation,
                    "request_id": request_id.clone(),
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
                active_compaction_id: Some(compaction_id.clone()),
                updated_by_event_seq: None,
                updated_at_ms: 0,
            },
        };

        let commit_result = match self
            .stores
            .projections
            .commit_compaction_projection(session_id, commit)
            .await
        {
            Ok(result) => result,
            Err(error) => {
                self.mark_manual_compaction_failed_after_projection_error(
                    request_id_for_status.as_deref(),
                    error.to_string(),
                )
                .await;
                return Err(error.into());
            }
        };

        self.mark_manual_compaction_completed_after_projection_commit(
            request_id_for_status.as_deref(),
            &compaction_id,
            projection_version,
            &lifecycle_item_id,
            &boundary_ref,
            first_kept_ref.as_ref(),
            source_start_event_seq,
            source_end_event_seq,
            first_kept_event_seq,
        )
        .await;

        Ok(Some(commit_result))
    }

    async fn mark_manual_compaction_completed_after_projection_commit(
        &self,
        request_id: Option<&str>,
        compaction_id: &str,
        projection_version: u64,
        lifecycle_item_id: &str,
        compacted_until_ref: &MessageRef,
        first_kept_ref: Option<&MessageRef>,
        source_start_event_seq: Option<u64>,
        source_end_event_seq: u64,
        first_kept_event_seq: Option<u64>,
    ) {
        let Some(repo) = self.manual_context_compaction_request_repo.as_ref() else {
            return;
        };
        let Some(request_id) = request_id else {
            return;
        };
        let Ok(request_uuid) = uuid::Uuid::parse_str(request_id) else {
            diag!(
                Warn,
                Subsystem::SessionLaunch,
                request_id = %request_id,
                "context_compacted request_id 不是 UUID，跳过 manual request completed 写回"
            );
            return;
        };

        let compacted_until_ref_value = serde_json::to_value(compacted_until_ref).ok();
        let first_kept_ref_value = serde_json::to_value(first_kept_ref).ok();
        let result_metadata = serde_json::json!({
            "status": "completed",
            "compaction_id": compaction_id,
            "projection_version": projection_version,
            "lifecycle_item_id": lifecycle_item_id,
            "source_start_event_seq": source_start_event_seq,
            "source_end_event_seq": source_end_event_seq,
            "first_kept_event_seq": first_kept_event_seq,
        });

        if let Err(error) = repo
            .mark_completed(
                request_uuid,
                compaction_id.to_string(),
                compacted_until_ref_value,
                first_kept_ref_value,
                Some(result_metadata),
            )
            .await
        {
            let context = DiagnosticErrorContext::new(
                "session.eventing",
                "manual_context_compaction_completed",
            );
            diag_error!(
                Warn,
                Subsystem::SessionLaunch,
                context = &context,
                error = &error,
                request_id = %request_id,
                compaction_id = %compaction_id,
                "manual context compaction request completed 写回失败"
            );
        }
    }

    async fn mark_manual_compaction_failed_after_projection_error(
        &self,
        request_id: Option<&str>,
        error_message: String,
    ) {
        let Some(repo) = self.manual_context_compaction_request_repo.as_ref() else {
            return;
        };
        let Some(request_id) = request_id else {
            return;
        };
        let Ok(request_uuid) = uuid::Uuid::parse_str(request_id) else {
            return;
        };
        if let Err(error) = repo
            .mark_failed(
                request_uuid,
                Some(serde_json::json!({
                    "status": "failed",
                    "reason": "projection_commit_failed",
                    "error": error_message,
                })),
            )
            .await
        {
            let context = DiagnosticErrorContext::new(
                "session.eventing",
                "manual_context_compaction_projection_failed",
            );
            diag_error!(
                Warn,
                Subsystem::SessionLaunch,
                context = &context,
                error = &error,
                request_id = %request_id,
                "manual context compaction request failed 写回失败"
            );
        }
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

const SESSION_REWOUND_MESSAGE_LIMIT: usize = 512;

fn bounded_session_rewound_message(message: Option<String>) -> Option<String> {
    let text = message?;
    let collapsed = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let trimmed = collapsed.trim();
    if trimmed.is_empty() {
        return None;
    }
    let lower = trimmed.to_ascii_lowercase();
    if let Some(html_start) = lower
        .find("<html")
        .or_else(|| lower.find("<!doctype"))
        .or_else(|| lower.find("<head"))
    {
        let prefix = trimmed[..html_start].trim().trim_end_matches(':').trim();
        if prefix.is_empty() {
            return Some("HTML error response body omitted".to_string());
        }
        return Some(format!("{prefix}; HTML error response body omitted"));
    }
    Some(bounded_text(trimmed, SESSION_REWOUND_MESSAGE_LIMIT))
}

fn bounded_text(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let mut bounded = text.chars().take(max_chars).collect::<String>();
    bounded.push_str("...");
    bounded
}

fn build_context_delivery_record_envelope(
    session_id: &str,
    turn_id: Option<&str>,
    record: &ContextDeliveryRecord,
    source: SourceInfo,
) -> io::Result<BackboneEnvelope> {
    let value = serde_json::to_value(record).map_err(|error| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("context delivery record 序列化失败: {error}"),
        )
    })?;
    Ok(BackboneEnvelope::new(
        BackboneEvent::Platform(PlatformEvent::SessionMetaUpdate {
            key: "context_delivery_record".to_string(),
            value,
        }),
        session_id,
        source,
    )
    .with_trace(TraceInfo {
        turn_id: turn_id.map(ToString::to_string),
        entry_index: None,
    }))
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

#[derive(Debug, Clone, PartialEq, Eq)]
struct StableTerminalBoundary {
    event_seq: u64,
    turn_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SessionRewindBoundary {
    discarded_turn_id: String,
    discarded_entry_index: Option<u32>,
    stable_event_seq: u64,
}

fn session_rewind_reason_from_str(reason: &str) -> SessionRewindReason {
    match reason {
        "provider_retry" => SessionRewindReason::ProviderRetry,
        "provider_failure" => SessionRewindReason::ProviderFailure,
        _ => SessionRewindReason::RuntimeFailure,
    }
}

fn latest_stable_terminal_before(
    events: &[PersistedSessionEvent],
    event_seq: u64,
) -> Option<StableTerminalBoundary> {
    events
        .iter()
        .filter(|event| event.event_seq < event_seq)
        .filter_map(|event| {
            let (turn_id, kind, _, _) =
                parse_turn_terminal_event_from_envelope(&event.notification)?;
            (kind == TurnTerminalKind::Completed).then_some(StableTerminalBoundary {
                event_seq: event.event_seq,
                turn_id,
            })
        })
        .max_by_key(|boundary| boundary.event_seq)
}

fn session_rewind_boundaries(
    events: &[PersistedSessionEvent],
) -> HashMap<String, SessionRewindBoundary> {
    let mut boundaries_by_turn: HashMap<String, (u64, SessionRewindBoundary)> = HashMap::new();
    for (mut boundary, event_seq) in events.iter().filter_map(parse_session_rewound_marker) {
        if boundary.discarded_entry_index.is_none() {
            boundary.discarded_entry_index = latest_agent_loop_entry_index_before(
                events,
                &boundary.discarded_turn_id,
                event_seq,
            );
        }
        let entry = boundaries_by_turn
            .entry(boundary.discarded_turn_id.clone())
            .or_insert((event_seq, boundary.clone()));
        if event_seq >= entry.0 {
            *entry = (event_seq, boundary);
        }
    }

    if !boundaries_by_turn.is_empty() {
        return boundaries_by_turn
            .into_values()
            .map(|(_, boundary)| (boundary.discarded_turn_id.clone(), boundary))
            .collect();
    }

    latest_failed_terminal_rewind_boundary(events)
        .into_iter()
        .map(|boundary| (boundary.discarded_turn_id.clone(), boundary))
        .collect()
}

fn latest_failed_terminal_rewind_boundary(
    events: &[PersistedSessionEvent],
) -> Option<SessionRewindBoundary> {
    let mut stable = StableTerminalBoundary {
        event_seq: 0,
        turn_id: String::new(),
    };
    let mut latest_terminal: Option<(u64, String, TurnTerminalKind, u64)> = None;
    for event in events {
        let Some((turn_id, kind, _message, _diagnostic)) =
            parse_turn_terminal_event_from_envelope(&event.notification)
        else {
            continue;
        };
        latest_terminal = Some((event.event_seq, turn_id.clone(), kind, stable.event_seq));
        if kind == TurnTerminalKind::Completed {
            stable = StableTerminalBoundary {
                event_seq: event.event_seq,
                turn_id,
            };
        }
    }

    let (_event_seq, discarded_turn_id, kind, stable_event_seq) = latest_terminal?;
    matches!(
        kind,
        TurnTerminalKind::Failed | TurnTerminalKind::Interrupted | TurnTerminalKind::Lost
    )
    .then_some(SessionRewindBoundary {
        discarded_entry_index: latest_agent_loop_entry_index_before(
            events,
            &discarded_turn_id,
            _event_seq,
        ),
        discarded_turn_id,
        stable_event_seq,
    })
}

fn parse_session_rewound_marker(
    event: &PersistedSessionEvent,
) -> Option<(SessionRewindBoundary, u64)> {
    let (discarded_turn_id, discarded_entry_index, stable_event_seq) =
        match &event.notification.event {
            BackboneEvent::Platform(PlatformEvent::SessionRewound(marker)) => (
                marker.discarded_turn_id.clone(),
                marker.discarded_entry_index,
                marker.stable_event_seq,
            ),
            BackboneEvent::Platform(PlatformEvent::SessionMetaUpdate { key, value })
                if key == "session_rewound" =>
            {
                let discarded_turn_id = value
                    .get("discarded_turn_id")
                    .and_then(serde_json::Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())?
                    .to_string();
                let stable_event_seq = value
                    .get("stable_event_seq")
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or_default();
                let discarded_entry_index = value
                    .get("discarded_entry_index")
                    .and_then(serde_json::Value::as_u64)
                    .and_then(|value| u32::try_from(value).ok());
                (discarded_turn_id, discarded_entry_index, stable_event_seq)
            }
            _ => return None,
        };
    Some((
        SessionRewindBoundary {
            discarded_turn_id,
            discarded_entry_index,
            stable_event_seq,
        },
        event.event_seq,
    ))
}

fn latest_agent_loop_entry_index_before(
    events: &[PersistedSessionEvent],
    turn_id: &str,
    event_seq: u64,
) -> Option<u32> {
    events
        .iter()
        .filter(|event| event.event_seq < event_seq)
        .filter(|event| event.turn_id.as_deref() == Some(turn_id))
        .filter(|event| is_agent_context_output_event(&event.notification.event))
        .filter_map(|event| event.entry_index)
        .max()
}

fn is_agent_context_output_event(event: &BackboneEvent) -> bool {
    matches!(
        event,
        BackboneEvent::AgentMessageDelta(_)
            | BackboneEvent::ReasoningTextDelta(_)
            | BackboneEvent::ReasoningSummaryDelta(_)
            | BackboneEvent::ItemStarted(_)
            | BackboneEvent::ItemUpdated(_)
            | BackboneEvent::ItemCompleted(_)
            | BackboneEvent::CommandOutputDelta(_)
            | BackboneEvent::FileChangeDelta(_)
            | BackboneEvent::McpToolCallProgress(_)
    )
}

fn bound_envelope_for_append(mut envelope: BackboneEnvelope) -> io::Result<BackboneEnvelope> {
    let original_bytes = serialized_envelope_len(&envelope)?;
    if original_bytes <= SESSION_EVENT_APPEND_GUARD_MAX_BYTES {
        return Ok(envelope);
    }

    let truncated_fields = bound_known_output_fields(&mut envelope, original_bytes);
    let bounded_bytes = serialized_envelope_len(&envelope)?;
    if truncated_fields > 0 {
        diag!(Warn, Subsystem::AgentRun,

            operation = "session.eventing.append_guard",
            stage = "truncate_known_output_fields",
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
        diag!(Warn, Subsystem::AgentRun,

            operation = "session.eventing.append_guard",
            stage = "oversized_after_guard",
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
    let mut remaining_nodes = SESSION_EVENT_APPEND_GUARD_MAX_JSON_NODES;
    replace_json_string_leaves_bounded(
        value,
        envelope_original_bytes,
        field_path,
        0,
        &mut remaining_nodes,
    )
}

fn replace_json_string_leaves_bounded(
    value: &mut serde_json::Value,
    envelope_original_bytes: usize,
    field_path: &str,
    depth: usize,
    remaining_nodes: &mut usize,
) -> usize {
    if depth >= SESSION_EVENT_APPEND_GUARD_MAX_JSON_DEPTH || *remaining_nodes == 0 {
        *value = append_guard_diagnostic_value(field_path, envelope_original_bytes, 0);
        return 1;
    }
    *remaining_nodes = remaining_nodes.saturating_sub(1);

    match value {
        serde_json::Value::String(text) => {
            replace_if_large(text, envelope_original_bytes, field_path)
        }
        serde_json::Value::Array(items) => items
            .iter_mut()
            .enumerate()
            .map(|(index, item)| {
                replace_json_string_leaves_bounded(
                    item,
                    envelope_original_bytes,
                    &format!("{field_path}[{index}]"),
                    depth + 1,
                    remaining_nodes,
                )
            })
            .sum(),
        serde_json::Value::Object(map) => map
            .iter_mut()
            .map(|(key, item)| {
                replace_json_string_leaves_bounded(
                    item,
                    envelope_original_bytes,
                    &format!("{field_path}.{}", bounded_append_guard_field_path_key(key)),
                    depth + 1,
                    remaining_nodes,
                )
            })
            .sum(),
        _ => 0,
    }
}

fn bounded_append_guard_field_path_key(key: &str) -> String {
    let mut chars = key.chars();
    let preview = chars
        .by_ref()
        .take(SESSION_EVENT_APPEND_GUARD_MAX_FIELD_PATH_KEY_CHARS)
        .collect::<String>();
    if chars.next().is_some() {
        format!("{preview}...")
    } else {
        key.to_string()
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
        BackboneEvent::ItemUpdated(_) => "item_updated",
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
        BackboneEvent::Platform(PlatformEvent::ProviderAttemptStatus(_)) => {
            "provider_attempt_status"
        }
        BackboneEvent::Platform(PlatformEvent::SessionRewound(_)) => "session_rewound",
        BackboneEvent::Platform(_) => "platform_event",
    }
}

/// 进度态事件分类：仅这些类为 ephemeral（不入 durable session_events，仅 live 广播）。
/// 其余一律 durable（白名单 durable、默认 durable）。
fn is_ephemeral_event(event: &BackboneEvent) -> bool {
    if let BackboneEvent::Platform(PlatformEvent::HookTrace(payload)) = event {
        return matches!(
            hook_trace_payload_storage_disposition(payload),
            HookTraceStorageDisposition::Ephemeral
        );
    }

    matches!(
        event,
        BackboneEvent::AgentMessageDelta(_)
            | BackboneEvent::ReasoningTextDelta(_)
            | BackboneEvent::ReasoningSummaryDelta(_)
            | BackboneEvent::CommandOutputDelta(_)
            | BackboneEvent::FileChangeDelta(_)
            | BackboneEvent::McpToolCallProgress(_)
            | BackboneEvent::ItemUpdated(_)
            | BackboneEvent::Platform(PlatformEvent::ProviderAttemptStatus(_))
    )
}

/// 持久化某条 durable 事件后，是否应清空 per-session ephemeral buffer。
/// 命中：turn terminal（completed/failed/interrupted/lost）与 session_rewound。
fn should_clear_ephemeral_on_durable(persisted: &PersistedSessionEvent) -> bool {
    if parse_turn_terminal_event_from_envelope(&persisted.notification).is_some() {
        return true;
    }
    matches!(
        &persisted.notification.event,
        BackboneEvent::Platform(PlatformEvent::SessionRewound(_))
    )
}

/// 若 durable 事件是终态助手消息（ItemCompleted AgentMessage / Reasoning），返回其 item_id。
/// 该 item_id 与对应 ephemeral text/reasoning delta 的 `item_id` 同源（同一气泡），
/// 用于剪除 buffer 中已被终态覆盖的在途 delta（P1-b）。
fn finalized_assistant_item_id(persisted: &PersistedSessionEvent) -> Option<String> {
    let BackboneEvent::ItemCompleted(notification) = &persisted.notification.event else {
        return None;
    };
    match &notification.item {
        AgentDashThreadItem::Codex(codex::ThreadItem::AgentMessage { id, .. }) => Some(id.clone()),
        AgentDashThreadItem::Codex(codex::ThreadItem::Reasoning { id, .. }) => Some(id.clone()),
        _ => None,
    }
}

fn ephemeral_tool_call_id(envelope: &BackboneEnvelope) -> Option<String> {
    match &envelope.event {
        BackboneEvent::ItemUpdated(n) => n.item.tool_call_id().map(ToString::to_string),
        _ => None,
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
    reason: &'a str,
    phase: &'a str,
    strategy: &'a str,
    implementation: &'a str,
    request_id: Option<&'a str>,
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
    obj.insert(
        "reason".to_string(),
        serde_json::Value::String(enrichment.reason.to_string()),
    );
    obj.insert(
        "phase".to_string(),
        serde_json::Value::String(enrichment.phase.to_string()),
    );
    obj.insert(
        "strategy".to_string(),
        serde_json::Value::String(enrichment.strategy.to_string()),
    );
    obj.insert(
        "implementation".to_string(),
        serde_json::Value::String(enrichment.implementation.to_string()),
    );
    obj.insert(
        "request_id".to_string(),
        serde_json::to_value(enrichment.request_id).unwrap_or(serde_json::Value::Null),
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
    use agentdash_domain::workflow::{
        ManualContextCompactionRequestRepository, ManualContextCompactionRequestStatus,
        ManualContextCompactionRequestedMode, NewManualContextCompactionRequest,
    };
    use agentdash_spi::{
        AgentConnector, AgentInfo, ConnectorCapabilities, ConnectorError, ConnectorType,
        ExecutionContext, ExecutionStream, HookTraceEntry, HookTraceTrigger, PromptPayload,
    };
    use agentdash_test_support::workflow::MemoryManualContextCompactionRequestRepository;
    use tokio::sync::Mutex;
    use tokio_stream::wrappers::ReceiverStream;
    use uuid::Uuid;

    use super::*;
    use crate::session::{
        FixtureRuntimeTraceStore,
        persistence::SessionStoreSet,
        types::{ExecutionStatus, SessionMeta},
    };

    fn test_eventing_service(stores: SessionStoreSet) -> SessionEventingService {
        SessionEventingService::new(
            stores.eventing_stores(),
            SessionRuntimeRegistry::new(Arc::new(Mutex::new(HashMap::new()))),
            Arc::new(NoopConnector),
        )
    }

    #[test]
    fn session_rewound_message_omits_html_error_body() {
        let message = "Codex API 返回 403 Forbidden: <html><head><style>body{display:flex}</style></head><body><svg>very long icon</svg><p>Unable to load site</p></body></html>";

        let bounded = bounded_session_rewound_message(Some(message.to_string()))
            .expect("message should be retained");

        assert_eq!(
            bounded,
            "Codex API 返回 403 Forbidden; HTML error response body omitted"
        );
        assert!(!bounded.contains("<html"));
        assert!(!bounded.contains("<svg"));
    }

    fn test_meta(session_id: &str) -> SessionMeta {
        SessionMeta {
            id: session_id.to_string(),
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

    #[test]
    fn context_delivery_record_envelope_uses_record_key_and_turn_trace() {
        let record = agentdash_spi::hooks::ContextDeliveryRecord {
            record_id: "context-delivery-record-session-1-turn-1".to_string(),
            runtime_session_id: "session-1".to_string(),
            turn_id: "turn-1".to_string(),
            applied_frame: agentdash_spi::hooks::ContextDeliveryAppliedFrame {
                agent_id: uuid::Uuid::nil(),
                frame_id: uuid::Uuid::nil(),
                frame_revision: 1,
                pending_frame_id: None,
                pending_frame_revision: None,
            },
            connector_input: agentdash_spi::hooks::ContextDeliveryConnectorInput {
                connector_id: "codex-bridge".to_string(),
                executor_id: "CODEX".to_string(),
                working_directory: "F:/workspace".to_string(),
                target_agent: agentdash_spi::hooks::ContextDeliveryTarget::default(),
            },
            delivery_plan_id: Some("plan-1".to_string()),
            context_frame_ids: vec!["identity-1".to_string()],
            emitted_context_frame_ids: vec!["identity-1".to_string()],
            created_at_ms: 1234,
        };

        let envelope = build_context_delivery_record_envelope(
            "session-1",
            Some("turn-1"),
            &record,
            test_source_info(),
        )
        .expect("record envelope");

        assert_eq!(envelope.trace.turn_id.as_deref(), Some("turn-1"));
        let BackboneEvent::Platform(PlatformEvent::SessionMetaUpdate { key, value }) =
            &envelope.event
        else {
            panic!("expected session meta update");
        };
        assert_eq!(key, "context_delivery_record");
        let roundtrip: agentdash_spi::hooks::ContextDeliveryRecord =
            serde_json::from_value(value.clone()).expect("record payload");
        assert_eq!(roundtrip, record);
    }

    fn assistant_delta_envelope(session_id: &str, turn_id: &str, text: &str) -> BackboneEnvelope {
        BackboneEnvelope::new(
            BackboneEvent::AgentMessageDelta(codex::AgentMessageDeltaNotification {
                delta: text.to_string(),
                thread_id: session_id.to_string(),
                turn_id: turn_id.to_string(),
                item_id: format!("{turn_id}:assistant:0"),
            }),
            session_id,
            test_source_info(),
        )
        .with_trace(TraceInfo {
            turn_id: Some(turn_id.to_string()),
            entry_index: Some(1),
        })
    }

    fn hook_trace_entry(decision: &str) -> HookTraceEntry {
        HookTraceEntry {
            sequence: 1,
            timestamp_ms: 1,
            revision: 1,
            trigger: HookTraceTrigger::BeforeTool,
            decision: decision.to_string(),
            tool_name: None,
            tool_call_id: None,
            subagent_type: None,
            matched_rule_keys: Vec::new(),
            refresh_snapshot: false,
            effects_applied: false,
            block_reason: None,
            completion: None,
            diagnostics: Vec::new(),
            injections: Vec::new(),
        }
    }

    fn final_assistant_message_envelope(
        session_id: &str,
        turn_id: &str,
        text: &str,
    ) -> BackboneEnvelope {
        final_assistant_message_envelope_at_entry(session_id, turn_id, 1, text)
    }

    fn final_assistant_message_envelope_at_entry(
        session_id: &str,
        turn_id: &str,
        entry_index: u32,
        text: &str,
    ) -> BackboneEnvelope {
        let item: AgentDashThreadItem = codex::ThreadItem::AgentMessage {
            id: format!("{turn_id}:{entry_index}:msg"),
            text: text.to_string(),
            phase: None,
            memory_citation: None,
        }
        .into();
        BackboneEnvelope::new(
            BackboneEvent::ItemCompleted(ItemCompletedNotification::new(
                item,
                session_id.to_string(),
                turn_id.to_string(),
            )),
            session_id,
            test_source_info(),
        )
        .with_trace(TraceInfo {
            turn_id: Some(turn_id.to_string()),
            entry_index: Some(entry_index),
        })
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
    async fn source_session_title_persists_event() {
        let session_id = "sess-source-title";
        let persistence = Arc::new(FixtureRuntimeTraceStore::default());
        let stores = SessionStoreSet::from_runtime_trace_test_store(persistence);
        stores
            .meta
            .create_session(&test_meta(session_id))
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

        // The SourceSessionTitleUpdated event is persisted to the journal.
        // Workspace title update goes through WorkspaceTitlePort (not wired in unit test).
        let events = stores
            .events
            .list_all_events(session_id)
            .await
            .expect("read events");
        assert!(!events.is_empty());
    }

    #[tokio::test]
    async fn source_session_title_ignores_preview_title() {
        let session_id = "sess-preview-title";
        let persistence = Arc::new(FixtureRuntimeTraceStore::default());
        let stores = SessionStoreSet::from_runtime_trace_test_store(persistence);
        stores
            .meta
            .create_session(&test_meta(session_id))
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

        // Preview title (matches source_title_preview_envelope pattern) should not trigger extra events.
        let events = stores
            .events
            .list_all_events(session_id)
            .await
            .expect("read events");
        // Only the original event is persisted; no workspace update event without port.
        assert_eq!(events.len(), 1);
    }

    #[tokio::test]
    async fn context_compacted_missing_summary_or_boundary_is_not_persisted() {
        let session_id = "sess-bad-context-compaction";
        let persistence = Arc::new(FixtureRuntimeTraceStore::default());
        let stores = SessionStoreSet::from_runtime_trace_test_store(persistence);
        stores
            .meta
            .create_session(&test_meta(session_id))
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
    async fn context_compacted_defaults_provenance_metadata_for_projection_commit() {
        let session_id = "sess-context-compaction-default-provenance";
        let persistence = Arc::new(FixtureRuntimeTraceStore::default());
        let stores = SessionStoreSet::from_runtime_trace_test_store(persistence);
        stores
            .meta
            .create_session(&test_meta(session_id))
            .await
            .expect("create session");
        let service = test_eventing_service(stores.clone());

        service
            .persist_notification(
                session_id,
                final_assistant_message_envelope_at_entry(
                    session_id,
                    "turn-1",
                    0,
                    "answer before compaction",
                ),
            )
            .await
            .expect("persist assistant");

        let persisted = service
            .persist_notification(
                session_id,
                context_compacted_envelope(
                    session_id,
                    serde_json::json!({
                        "lifecycle_item_id": "compact-defaults",
                        "summary": "历史摘要",
                        "tokens_before": 48_000,
                        "messages_compacted": 1,
                        "newly_compacted_messages": 1,
                        "compacted_until_ref": { "turn_id": "turn-1", "entry_index": 0 },
                        "first_kept_ref": null,
                    }),
                ),
            )
            .await
            .expect("persist context compacted");

        let BackboneEvent::Platform(PlatformEvent::SessionMetaUpdate { value, .. }) =
            &persisted.notification.event
        else {
            panic!("expected context_compacted platform event");
        };
        assert_eq!(value["trigger"], "auto");
        assert_eq!(value["reason"], "token_pressure");
        assert_eq!(value["phase"], "pre_provider");
        assert_eq!(value["strategy"], "summary_prefix");
        assert_eq!(value["implementation"], "local_summary");
        assert!(value["request_id"].is_null());

        let compactions = stores
            .compactions
            .list_compactions(session_id, SESSION_PROJECTION_KIND_MODEL_CONTEXT)
            .await
            .expect("list compactions");
        let compaction = compactions
            .first()
            .expect("compaction record should be committed");
        assert_eq!(compaction.trigger, "auto");
        assert_eq!(compaction.reason.as_deref(), Some("token_pressure"));
        assert_eq!(compaction.phase.as_deref(), Some("pre_provider"));
        assert_eq!(compaction.strategy, "summary_prefix");
        assert_eq!(
            compaction.diagnostics_json["implementation"],
            "local_summary"
        );
        assert!(compaction.diagnostics_json["request_id"].is_null());
    }

    #[tokio::test]
    async fn manual_context_compacted_commit_marks_request_completed_with_projection_refs() {
        let session_id = "sess-manual-context-compaction-completed";
        let persistence = Arc::new(FixtureRuntimeTraceStore::default());
        let stores = SessionStoreSet::from_runtime_trace_test_store(persistence);
        stores
            .meta
            .create_session(&test_meta(session_id))
            .await
            .expect("create session");
        let manual_requests = Arc::new(MemoryManualContextCompactionRequestRepository::default());
        let request = manual_requests
            .create_requested(NewManualContextCompactionRequest {
                session_id: session_id.to_string(),
                run_id: Uuid::new_v4(),
                agent_id: Uuid::new_v4(),
                command_receipt_id: Uuid::new_v4(),
                requested_mode: ManualContextCompactionRequestedMode::CompactOnly,
                keep_last_n: Some(1),
                reserve_tokens: Some(16_384),
                request_metadata: Some(serde_json::json!({
                    "source": "test",
                })),
            })
            .await
            .expect("create manual request");
        manual_requests
            .mark_consumed(request.id, "turn-compact".to_string())
            .await
            .expect("mark consumed");
        let service = test_eventing_service(stores.clone())
            .with_manual_context_compaction_request_repo(Some(manual_requests.clone()));

        service
            .persist_notification(
                session_id,
                final_assistant_message_envelope_at_entry(
                    session_id,
                    "turn-1",
                    0,
                    "answer before manual compaction",
                ),
            )
            .await
            .expect("persist assistant");

        service
            .persist_notification(
                session_id,
                context_compacted_envelope(
                    session_id,
                    serde_json::json!({
                        "lifecycle_item_id": "compact-manual",
                        "summary": "手动压缩摘要",
                        "tokens_before": 48_000,
                        "messages_compacted": 1,
                        "newly_compacted_messages": 1,
                        "compacted_until_ref": { "turn_id": "turn-1", "entry_index": 0 },
                        "first_kept_ref": null,
                        "trigger": "manual",
                        "reason": "user_requested",
                        "phase": "standalone_compact_turn",
                        "strategy": "summary_prefix",
                        "implementation": "local_summary",
                        "request_id": request.id.to_string(),
                    }),
                ),
            )
            .await
            .expect("persist context compacted");

        let stored_request = manual_requests
            .get_by_id(request.id)
            .await
            .expect("load manual request")
            .expect("manual request should exist");
        assert_eq!(
            stored_request.status,
            ManualContextCompactionRequestStatus::Completed
        );
        assert_eq!(
            stored_request.completed_compaction_id.as_deref(),
            Some("compaction-compact-manual")
        );
        assert_eq!(
            stored_request
                .compacted_until_ref
                .as_ref()
                .and_then(|value| value.get("turn_id"))
                .and_then(serde_json::Value::as_str),
            Some("turn-1")
        );
        assert!(
            stored_request
                .first_kept_ref
                .as_ref()
                .is_some_and(serde_json::Value::is_null)
        );
        assert_eq!(
            stored_request
                .result_metadata
                .as_ref()
                .and_then(|value| value.get("status"))
                .and_then(serde_json::Value::as_str),
            Some("completed")
        );

        let compactions = stores
            .compactions
            .list_compactions(session_id, SESSION_PROJECTION_KIND_MODEL_CONTEXT)
            .await
            .expect("list compactions");
        assert_eq!(compactions.len(), 1);
        assert_eq!(compactions[0].id, "compaction-compact-manual");
        assert_eq!(compactions[0].trigger, "manual");
        let head = stores
            .projections
            .read_projection_head(session_id, SESSION_PROJECTION_KIND_MODEL_CONTEXT)
            .await
            .expect("read projection head")
            .expect("projection head should be committed");
        assert_eq!(
            head.active_compaction_id.as_deref(),
            Some("compaction-compact-manual")
        );
    }

    #[tokio::test]
    async fn executor_context_compacted_is_telemetry_and_does_not_advance_projection_head() {
        let session_id = "sess-external-compact";
        let persistence = Arc::new(FixtureRuntimeTraceStore::default());
        let stores = SessionStoreSet::from_runtime_trace_test_store(persistence);
        stores
            .meta
            .create_session(&test_meta(session_id))
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
    async fn session_rewind_marker_excludes_failed_turn_from_model_context() {
        let session_id = "sess-rewind-projection";
        let persistence = Arc::new(FixtureRuntimeTraceStore::default());
        let stores = SessionStoreSet::from_runtime_trace_test_store(persistence);
        stores
            .meta
            .create_session(&test_meta(session_id))
            .await
            .expect("create session");
        let service = test_eventing_service(stores.clone());
        let source = test_source_info();

        service
            .emit_user_input_submitted(
                session_id,
                "turn-stable",
                "turn-stable:user-input:0",
                agentdash_agent_protocol::UserInputSubmissionKind::Prompt,
                agentdash_agent_protocol::UserInputSource::core_composer(),
                vec![codex::UserInput::Text {
                    text: "stable prompt".to_string(),
                    text_elements: Vec::new(),
                }],
            )
            .await
            .expect("persist stable user input");
        service
            .persist_notification(
                session_id,
                final_assistant_message_envelope(session_id, "turn-stable", "stable answer"),
            )
            .await
            .expect("persist stable assistant");
        let stable_terminal = service
            .persist_notification(
                session_id,
                crate::session::hub_support::build_turn_terminal_envelope_with_timing(
                    session_id,
                    &source,
                    "turn-stable",
                    TurnTerminalKind::Completed,
                    None,
                    None,
                    None,
                ),
            )
            .await
            .expect("persist stable terminal");

        service
            .emit_user_input_submitted(
                session_id,
                "turn-failed",
                "turn-failed:user-input:0",
                agentdash_agent_protocol::UserInputSubmissionKind::Prompt,
                agentdash_agent_protocol::UserInputSource::core_composer(),
                vec![codex::UserInput::Text {
                    text: "failed prompt".to_string(),
                    text_elements: Vec::new(),
                }],
            )
            .await
            .expect("persist failed user input");
        service
            .persist_notification(
                session_id,
                final_assistant_message_envelope_at_entry(
                    session_id,
                    "turn-failed",
                    0,
                    "completed loop answer",
                ),
            )
            .await
            .expect("persist completed loop assistant");
        service
            .persist_notification(
                session_id,
                final_assistant_message_envelope_at_entry(
                    session_id,
                    "turn-failed",
                    1,
                    "partial failed answer",
                ),
            )
            .await
            .expect("persist failed assistant");
        let failed_terminal = service
            .persist_notification(
                session_id,
                crate::session::hub_support::build_turn_terminal_envelope_with_timing(
                    session_id,
                    &source,
                    "turn-failed",
                    TurnTerminalKind::Failed,
                    Some("provider disconnected".to_string()),
                    None,
                    None,
                ),
            )
            .await
            .expect("persist failed terminal");
        let marker = service
            .persist_session_rewound_marker(
                session_id,
                &source,
                "turn-failed",
                "runtime_failure",
                Some("provider disconnected".to_string()),
                failed_terminal.event_seq,
                true,
            )
            .await
            .expect("persist rewind marker");

        assert_eq!(failed_terminal.event_seq, stable_terminal.event_seq + 4);
        assert_eq!(marker.event_seq, failed_terminal.event_seq + 1);
        match &marker.notification.event {
            BackboneEvent::Platform(PlatformEvent::SessionRewound(marker)) => {
                assert_eq!(marker.discarded_turn_id, "turn-failed");
                assert_eq!(marker.discarded_entry_index, Some(1));
                assert_eq!(marker.stable_event_seq, stable_terminal.event_seq);
                assert_eq!(marker.stable_turn_id.as_deref(), Some("turn-stable"));
                assert_eq!(marker.reason, SessionRewindReason::RuntimeFailure);
            }
            event => panic!("expected session_rewound marker, got {event:?}"),
        }

        let transcript = service
            .build_projected_transcript(session_id)
            .await
            .expect("build transcript");
        let rendered = transcript
            .into_messages()
            .into_iter()
            .filter_map(|message| message.first_text().map(ToString::to_string))
            .collect::<Vec<_>>()
            .join("\n");

        assert!(rendered.contains("stable prompt"));
        assert!(rendered.contains("stable answer"));
        assert!(rendered.contains("failed prompt"));
        assert!(rendered.contains("completed loop answer"));
        assert!(!rendered.contains("partial failed answer"));
    }

    #[tokio::test]
    async fn multiple_session_rewind_markers_exclude_all_failed_turns_from_model_context() {
        let session_id = "sess-multiple-rewind-projection";
        let persistence = Arc::new(FixtureRuntimeTraceStore::default());
        let stores = SessionStoreSet::from_runtime_trace_test_store(persistence);
        stores
            .meta
            .create_session(&test_meta(session_id))
            .await
            .expect("create session");
        let service = test_eventing_service(stores.clone());
        let source = test_source_info();

        service
            .emit_user_input_submitted(
                session_id,
                "turn-stable",
                "turn-stable:user-input:0",
                agentdash_agent_protocol::UserInputSubmissionKind::Prompt,
                agentdash_agent_protocol::UserInputSource::core_composer(),
                vec![codex::UserInput::Text {
                    text: "stable prompt".to_string(),
                    text_elements: Vec::new(),
                }],
            )
            .await
            .expect("persist stable user input");
        service
            .persist_notification(
                session_id,
                final_assistant_message_envelope(session_id, "turn-stable", "stable answer"),
            )
            .await
            .expect("persist stable assistant");
        service
            .persist_notification(
                session_id,
                crate::session::hub_support::build_turn_terminal_envelope_with_timing(
                    session_id,
                    &source,
                    "turn-stable",
                    TurnTerminalKind::Completed,
                    None,
                    None,
                    None,
                ),
            )
            .await
            .expect("persist stable terminal");

        for failed_turn in ["turn-failed-a", "turn-failed-b"] {
            service
                .emit_user_input_submitted(
                    session_id,
                    failed_turn,
                    &format!("{failed_turn}:user-input:0"),
                    agentdash_agent_protocol::UserInputSubmissionKind::Prompt,
                    agentdash_agent_protocol::UserInputSource::core_composer(),
                    vec![codex::UserInput::Text {
                        text: format!("{failed_turn} prompt"),
                        text_elements: Vec::new(),
                    }],
                )
                .await
                .expect("persist failed user input");
            service
                .persist_notification(
                    session_id,
                    final_assistant_message_envelope_at_entry(
                        session_id,
                        failed_turn,
                        0,
                        &format!("{failed_turn} completed loop answer"),
                    ),
                )
                .await
                .expect("persist completed loop assistant");
            service
                .persist_notification(
                    session_id,
                    final_assistant_message_envelope_at_entry(
                        session_id,
                        failed_turn,
                        1,
                        &format!("{failed_turn} partial answer"),
                    ),
                )
                .await
                .expect("persist failed assistant");
            let failed_terminal = service
                .persist_notification(
                    session_id,
                    crate::session::hub_support::build_turn_terminal_envelope_with_timing(
                        session_id,
                        &source,
                        failed_turn,
                        TurnTerminalKind::Failed,
                        Some("provider disconnected".to_string()),
                        None,
                        None,
                    ),
                )
                .await
                .expect("persist failed terminal");
            service
                .persist_session_rewound_marker(
                    session_id,
                    &source,
                    failed_turn,
                    "runtime_failure",
                    Some("provider disconnected".to_string()),
                    failed_terminal.event_seq,
                    true,
                )
                .await
                .expect("persist rewind marker");
        }

        let transcript = service
            .build_projected_transcript(session_id)
            .await
            .expect("build transcript");
        let rendered = transcript
            .into_messages()
            .into_iter()
            .filter_map(|message| message.first_text().map(ToString::to_string))
            .collect::<Vec<_>>()
            .join("\n");

        assert!(rendered.contains("stable prompt"));
        assert!(rendered.contains("stable answer"));
        assert!(rendered.contains("turn-failed-a prompt"));
        assert!(rendered.contains("turn-failed-a completed loop answer"));
        assert!(!rendered.contains("turn-failed-a partial answer"));
        assert!(rendered.contains("turn-failed-b prompt"));
        assert!(rendered.contains("turn-failed-b completed loop answer"));
        assert!(!rendered.contains("turn-failed-b partial answer"));
    }

    #[tokio::test]
    async fn append_guard_bounds_oversized_tool_completed_event_before_store_and_broadcast() {
        let session_id = "sess-append-guard-tool";
        let sentinel = "SENTINEL_TOOL_OUTPUT_SHOULD_NOT_PERSIST";
        let persistence = Arc::new(FixtureRuntimeTraceStore::default());
        let stores = SessionStoreSet::from_runtime_trace_test_store(persistence);
        stores
            .meta
            .create_session(&test_meta(session_id))
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
        let persistence = Arc::new(FixtureRuntimeTraceStore::default());
        let stores = SessionStoreSet::from_runtime_trace_test_store(persistence);
        stores
            .meta
            .create_session(&test_meta(session_id))
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

    #[test]
    fn append_guard_json_leaf_scan_is_depth_bounded() {
        let mut deep = serde_json::json!("leaf");
        for _ in 0..128 {
            deep = serde_json::json!([deep]);
        }

        let truncated = replace_json_string_leaves(&mut deep, 512, "root");
        let rendered = serde_json::to_string(&deep).expect("serialize bounded json");

        assert_eq!(truncated, 1);
        assert!(rendered.contains("session_eventing_append_guard"));
        assert!(rendered.contains("root"));
    }

    #[tokio::test]
    async fn append_guard_leaves_small_events_unchanged() {
        let session_id = "sess-append-guard-small";
        let persistence = Arc::new(FixtureRuntimeTraceStore::default());
        let stores = SessionStoreSet::from_runtime_trace_test_store(persistence);
        stores
            .meta
            .create_session(&test_meta(session_id))
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

    #[tokio::test]
    async fn ephemeral_event_broadcasts_without_durable_append() {
        let session_id = "sess-ephemeral-delta";
        let persistence = Arc::new(FixtureRuntimeTraceStore::default());
        let stores = SessionStoreSet::from_runtime_trace_test_store(persistence);
        stores
            .meta
            .create_session(&test_meta(session_id))
            .await
            .expect("create session");
        let service = test_eventing_service(stores);

        let mut rx = service.ensure_session(session_id).await;

        let persisted = service
            .persist_notification(
                session_id,
                assistant_delta_envelope(session_id, "turn-eph", "hello"),
            )
            .await
            .expect("persist ephemeral delta");

        assert!(persisted.ephemeral, "delta should be classified ephemeral");
        // event_seq 现承载单调 ephemeral_seq（首条为 1，不再是占位 0）。
        assert_eq!(persisted.event_seq, 1);
        assert_eq!(persisted.session_update_type, "agent_message_delta");

        // 不进 durable session_events。
        let backlog = service
            .subscribe_after(session_id, 0)
            .await
            .expect("read backlog");
        assert!(
            backlog.backlog.is_empty(),
            "ephemeral delta must not be appended to durable log"
        );
        // 进 ephemeral buffer，subscribe_after 快照可补发。
        assert_eq!(backlog.ephemeral_backlog.len(), 1);
        assert_eq!(backlog.ephemeral_backlog[0].event_seq, 1);
        assert!(backlog.ephemeral_backlog[0].ephemeral);

        // 仍 live 广播，且带 ephemeral=true。
        let received = rx
            .try_recv()
            .expect("broadcast ephemeral delta to subscriber");
        assert!(received.ephemeral);
        assert_eq!(received.event_seq, 1);
        assert_eq!(received.session_update_type, "agent_message_delta");
    }

    #[tokio::test]
    async fn provider_attempt_status_is_live_only() {
        let session_id = "sess-ephemeral-provider-status";
        let persistence = Arc::new(FixtureRuntimeTraceStore::default());
        let stores = SessionStoreSet::from_runtime_trace_test_store(persistence);
        stores
            .meta
            .create_session(&test_meta(session_id))
            .await
            .expect("create session");
        let service = test_eventing_service(stores);
        let mut rx = service.ensure_session(session_id).await;

        let persisted = service
            .persist_notification(
                session_id,
                BackboneEnvelope::new(
                    BackboneEvent::Platform(PlatformEvent::ProviderAttemptStatus(
                        agentdash_agent_protocol::ProviderAttemptStatus {
                            turn_id: "turn-provider".to_string(),
                            phase: agentdash_agent_protocol::ProviderAttemptPhase::ConnectedWaitingFirstDelta,
                            attempt: 1,
                            max_attempts: 3,
                            will_retry: false,
                            delay_ms: None,
                            reason_code: None,
                            message: None,
                            provider: None,
                            model: None,
                        },
                    )),
                    session_id,
                    test_source_info(),
                )
                .with_trace(TraceInfo {
                    turn_id: Some("turn-provider".to_string()),
                    entry_index: None,
                }),
            )
            .await
            .expect("persist provider status");

        assert!(persisted.ephemeral);
        assert_eq!(persisted.event_seq, 1);
        assert_eq!(persisted.session_update_type, "provider_attempt_status");

        let backlog = service
            .subscribe_after(session_id, 0)
            .await
            .expect("read backlog");
        assert!(backlog.backlog.is_empty());
        assert_eq!(backlog.ephemeral_backlog.len(), 1);
        assert_eq!(
            backlog.ephemeral_backlog[0].session_update_type,
            "provider_attempt_status"
        );

        let received = rx
            .try_recv()
            .expect("broadcast provider attempt status to subscriber");
        assert!(received.ephemeral);
        assert_eq!(received.session_update_type, "provider_attempt_status");
    }

    #[tokio::test]
    async fn matched_silent_hook_trace_is_live_only() {
        let session_id = "sess-ephemeral-hook-trace";
        let persistence = Arc::new(FixtureRuntimeTraceStore::default());
        let stores = SessionStoreSet::from_runtime_trace_test_store(persistence);
        stores
            .meta
            .create_session(&test_meta(session_id))
            .await
            .expect("create session");
        let service = test_eventing_service(stores);
        let mut rx = service.ensure_session(session_id).await;
        let entry = HookTraceEntry {
            matched_rule_keys: vec!["workflow:silent_observer".to_string()],
            ..hook_trace_entry("observed")
        };
        let envelope = agentdash_spi::build_hook_trace_envelope(
            session_id,
            Some("turn-hook"),
            test_source_info(),
            &entry,
        )
        .expect("matched silent trace should be emitted live");

        let persisted = service
            .persist_notification(session_id, envelope)
            .await
            .expect("persist matched silent hook trace");

        assert!(persisted.ephemeral);
        assert_eq!(persisted.event_seq, 1);
        assert_eq!(persisted.session_update_type, "platform_event");

        let backlog = service
            .subscribe_after(session_id, 0)
            .await
            .expect("read backlog");
        assert!(backlog.backlog.is_empty());
        assert_eq!(backlog.ephemeral_backlog.len(), 1);

        let received = rx
            .try_recv()
            .expect("broadcast ephemeral hook trace to subscriber");
        assert!(received.ephemeral);
        assert_eq!(received.event_seq, 1);
    }

    #[tokio::test]
    async fn actionful_hook_trace_is_durable() {
        let session_id = "sess-durable-hook-trace";
        let persistence = Arc::new(FixtureRuntimeTraceStore::default());
        let stores = SessionStoreSet::from_runtime_trace_test_store(persistence);
        stores
            .meta
            .create_session(&test_meta(session_id))
            .await
            .expect("create session");
        let service = test_eventing_service(stores);
        let envelope = agentdash_spi::build_hook_trace_envelope(
            session_id,
            Some("turn-hook"),
            test_source_info(),
            &hook_trace_entry("deny"),
        )
        .expect("actionful trace should build durable envelope");

        let persisted = service
            .persist_notification(session_id, envelope)
            .await
            .expect("persist actionful hook trace");

        assert!(!persisted.ephemeral);
        let backlog = service
            .subscribe_after(session_id, 0)
            .await
            .expect("read backlog");
        assert_eq!(backlog.backlog.len(), 1);
        assert!(backlog.ephemeral_backlog.is_empty());
    }

    #[tokio::test]
    async fn ephemeral_seq_is_monotonic_and_cleared_on_turn_terminal() {
        let session_id = "sess-ephemeral-clear";
        let persistence = Arc::new(FixtureRuntimeTraceStore::default());
        let stores = SessionStoreSet::from_runtime_trace_test_store(persistence);
        stores
            .meta
            .create_session(&test_meta(session_id))
            .await
            .expect("create session");
        let service = test_eventing_service(stores);
        let source = test_source_info();

        let first = service
            .persist_notification(
                session_id,
                assistant_delta_envelope(session_id, "turn-eph", "hello "),
            )
            .await
            .expect("persist delta 1");
        let second = service
            .persist_notification(
                session_id,
                assistant_delta_envelope(session_id, "turn-eph", "world"),
            )
            .await
            .expect("persist delta 2");
        assert_eq!(first.event_seq, 1);
        assert_eq!(second.event_seq, 2);

        // 快照可补发两条累积 delta。
        let before = service
            .subscribe_after(session_id, 0)
            .await
            .expect("snapshot before terminal");
        assert_eq!(before.ephemeral_backlog.len(), 2);

        // turn terminal（durable）落库后清空 ephemeral buffer。
        service
            .persist_notification(
                session_id,
                crate::session::hub_support::build_turn_terminal_envelope_with_timing(
                    session_id,
                    &source,
                    "turn-eph",
                    TurnTerminalKind::Completed,
                    None,
                    None,
                    None,
                ),
            )
            .await
            .expect("persist turn terminal");

        let after = service
            .subscribe_after(session_id, 0)
            .await
            .expect("snapshot after terminal");
        assert!(
            after.ephemeral_backlog.is_empty(),
            "ephemeral buffer must be cleared after turn terminal"
        );

        // 计数器不重置：清空后新 delta 的 seq 继续递增。
        let third = service
            .persist_notification(
                session_id,
                assistant_delta_envelope(session_id, "turn-eph2", "next"),
            )
            .await
            .expect("persist delta 3");
        assert_eq!(third.event_seq, 3);
    }

    #[tokio::test]
    async fn durable_event_still_appends() {
        let session_id = "sess-durable-still-appends";
        let persistence = Arc::new(FixtureRuntimeTraceStore::default());
        let stores = SessionStoreSet::from_runtime_trace_test_store(persistence);
        stores
            .meta
            .create_session(&test_meta(session_id))
            .await
            .expect("create session");
        let service = test_eventing_service(stores);

        let persisted = service
            .persist_notification(
                session_id,
                BackboneEnvelope::new(
                    BackboneEvent::Platform(PlatformEvent::TerminalOutput {
                        terminal_id: "term-1".to_string(),
                        data: "hello\n".to_string(),
                    }),
                    session_id,
                    test_source_info(),
                ),
            )
            .await
            .expect("persist durable event");

        assert!(!persisted.ephemeral);
        assert!(persisted.event_seq >= 1);

        let backlog = service
            .subscribe_after(session_id, 0)
            .await
            .expect("read backlog");
        assert_eq!(backlog.backlog.len(), 1);
        assert!(!backlog.backlog[0].ephemeral);
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
