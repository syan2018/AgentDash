use std::io;

use agentdash_agent_types::{
    AgentContextEnvelope, AgentInputMessage, ProjectedEntry, ProjectedTranscript, ProjectionKind,
    estimate_message_tokens,
};
use agentdash_spi::{
    SESSION_PROJECTION_KIND_MODEL_CONTEXT, SessionCompactionRecord, SessionCompactionStatus,
    SessionProjectionHeadRecord, SessionProjectionSegmentRecord,
};

use super::compaction_checkpoint::{
    CompactionCheckpointError, projection_entries_from_checkpoint_records,
    suffix_start_event_seq_from_compaction,
};
use super::persistence::{ContextProjectionStores, PersistedSessionEvent};
use super::transcript_restore::build_raw_projected_transcript_from_filtered_events;

#[derive(Clone)]
pub struct ContextProjector {
    stores: ContextProjectionStores,
}

impl ContextProjector {
    pub(in crate::session) fn new(stores: ContextProjectionStores) -> Self {
        Self { stores }
    }

    pub async fn build_model_context(&self, session_id: &str) -> io::Result<AgentContextEnvelope> {
        let head = self
            .stores
            .projections
            .read_projection_head(session_id, SESSION_PROJECTION_KIND_MODEL_CONTEXT)
            .await?;

        match head {
            Some(head) => self.build_from_projection_head(session_id, head).await,
            None => {
                // 无 projection head：通过 raw 事件重建完整 transcript。
                let events = self.list_events_from(session_id, 0).await?;
                let transcript = build_raw_projected_transcript_from_filtered_events(events.iter());
                let token_estimate = entries_token_estimate(&transcript.entries);
                Ok(envelope_from_transcript(
                    session_id,
                    0,
                    latest_event_seq(&events),
                    None,
                    token_estimate,
                    transcript,
                ))
            }
        }
    }

    async fn list_events_from(
        &self,
        session_id: &str,
        from_seq: u64,
    ) -> io::Result<Vec<PersistedSessionEvent>> {
        Ok(self
            .stores
            .events
            .list_events_from(session_id, from_seq)
            .await?)
    }

    pub async fn build_model_context_at_event(
        &self,
        session_id: &str,
        head_event_seq: u64,
    ) -> io::Result<AgentContextEnvelope> {
        let head = self
            .stores
            .projections
            .read_projection_head(session_id, SESSION_PROJECTION_KIND_MODEL_CONTEXT)
            .await?;

        if let Some(mut head) = head {
            if let Some(active_compaction_id) = head.active_compaction_id.as_deref() {
                let compaction = self
                    .stores
                    .compactions
                    .get_compaction(session_id, active_compaction_id)
                    .await?;
                if compaction
                    .as_ref()
                    .is_some_and(|record| compaction_covers_head(record, head_event_seq))
                {
                    head.head_event_seq = head_event_seq;
                    return self.build_from_projection_head(session_id, head).await;
                }
            } else {
                head.head_event_seq = head_event_seq;
                return self.build_from_projection_head(session_id, head).await;
            }
        }

        // 无可用 head：通过 raw 事件重建后按 <= head_event_seq 过滤。
        let events = self.list_events_from(session_id, 0).await?;
        Ok(envelope_from_transcript(
            session_id,
            0,
            head_event_seq,
            None,
            None,
            build_raw_projected_transcript_from_filtered_events(
                events
                    .iter()
                    .filter(|event| event.event_seq <= head_event_seq),
            ),
        ))
    }

    pub async fn build_model_context_from_compaction(
        &self,
        session_id: &str,
        compaction_id: &str,
        head_event_seq: Option<u64>,
    ) -> io::Result<AgentContextEnvelope> {
        let compaction = self
            .stores
            .compactions
            .get_compaction(session_id, compaction_id)
            .await?
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::NotFound,
                    format!("compaction {compaction_id} 不存在"),
                )
            })?;
        validate_active_compaction(&compaction)?;
        // 仅当无显式 head 且 compaction 缺少边界时才需要最新 seq 作 fallback。
        let resolved_head_event_seq = match head_event_seq
            .or(compaction.completed_event_seq)
            .or(compaction.source_end_event_seq)
        {
            Some(seq) => seq,
            None => latest_event_seq(&self.list_events_from(session_id, 0).await?),
        };
        let head = SessionProjectionHeadRecord {
            session_id: session_id.to_string(),
            projection_kind: SESSION_PROJECTION_KIND_MODEL_CONTEXT.to_string(),
            projection_version: compaction.projection_version,
            head_event_seq: resolved_head_event_seq,
            active_compaction_id: Some(compaction_id.to_string()),
            updated_by_event_seq: compaction.completed_event_seq,
            updated_at_ms: compaction
                .completed_at_ms
                .unwrap_or(compaction.created_at_ms),
        };
        self.build_from_projection_head(session_id, head).await
    }

    pub async fn build_projected_transcript(
        &self,
        session_id: &str,
    ) -> io::Result<ProjectedTranscript> {
        Ok(self
            .build_model_context(session_id)
            .await?
            .into_projected_transcript())
    }

    async fn build_from_projection_head(
        &self,
        session_id: &str,
        head: SessionProjectionHeadRecord,
    ) -> io::Result<AgentContextEnvelope> {
        let Some(active_compaction_id) = head.active_compaction_id.as_deref() else {
            // 无 active compaction：prefix 即 0..head，无法只读 suffix，仍需全量后过滤。
            let events = self.list_events_from(session_id, 0).await?;
            let transcript = build_raw_projected_transcript_from_filtered_events(
                events
                    .iter()
                    .filter(|event| event.event_seq <= head.head_event_seq),
            );
            let token_estimate = entries_token_estimate(&transcript.entries);
            return Ok(envelope_from_transcript(
                session_id,
                head.projection_version,
                head.head_event_seq,
                None,
                token_estimate,
                transcript,
            ));
        };

        let compaction = self
            .stores
            .compactions
            .get_compaction(session_id, active_compaction_id)
            .await?
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("projection head 指向不存在的 compaction {active_compaction_id}"),
                )
            })?;
        validate_active_compaction(&compaction)?;

        let segments = self
            .stores
            .projections
            .list_projection_segments(
                session_id,
                SESSION_PROJECTION_KIND_MODEL_CONTEXT,
                head.projection_version,
            )
            .await?;

        let mut entries = projection_entries_from_checkpoint_records(&compaction, &segments)
            .map_err(checkpoint_error_to_io)?;
        let suffix_start_event_seq =
            suffix_start_event_seq_from_compaction(&compaction, head.head_event_seq)
                .map_err(checkpoint_error_to_io)?;
        // 主收益场景：prefix 来自 materialized checkpoint，只需从 DB 读 suffix。
        let suffix_events = self
            .list_events_from(session_id, suffix_start_event_seq)
            .await?;
        let suffix = build_raw_projected_transcript_from_filtered_events(
            suffix_events
                .iter()
                .filter(|event| event.event_seq <= head.head_event_seq),
        );
        let token_estimate = token_estimate(&segments, &entries, &suffix.entries);
        entries.extend(suffix.entries);

        Ok(envelope_from_entries(
            session_id,
            head.projection_version,
            head.head_event_seq,
            Some(active_compaction_id.to_string()),
            token_estimate,
            entries,
        ))
    }
}

fn validate_active_compaction(compaction: &SessionCompactionRecord) -> io::Result<()> {
    if compaction.status != SessionCompactionStatus::ProjectionCommitted {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "active compaction {} 状态不是 projection_committed",
                compaction.id
            ),
        ));
    }
    Ok(())
}

fn checkpoint_error_to_io(error: CompactionCheckpointError) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, error)
}

fn compaction_covers_head(compaction: &SessionCompactionRecord, head_event_seq: u64) -> bool {
    if compaction.strategy == "fork_initial_projection" {
        return true;
    }
    compaction
        .source_end_event_seq
        .map(|source_end| source_end <= head_event_seq)
        .unwrap_or(true)
}

fn envelope_from_transcript(
    session_id: &str,
    projection_version: u64,
    head_event_seq: u64,
    active_compaction_id: Option<String>,
    token_estimate: Option<u64>,
    transcript: ProjectedTranscript,
) -> AgentContextEnvelope {
    envelope_from_entries(
        session_id,
        projection_version,
        head_event_seq,
        active_compaction_id,
        token_estimate,
        transcript.entries,
    )
}

fn envelope_from_entries(
    session_id: &str,
    projection_version: u64,
    head_event_seq: u64,
    active_compaction_id: Option<String>,
    token_estimate: Option<u64>,
    entries: Vec<ProjectedEntry>,
) -> AgentContextEnvelope {
    AgentContextEnvelope {
        session_id: session_id.to_string(),
        projection_kind: ProjectionKind::ModelContext,
        projection_version,
        head_event_seq,
        active_compaction_id,
        token_estimate,
        messages: entries.into_iter().map(AgentInputMessage::from).collect(),
    }
}

fn token_estimate(
    segments: &[SessionProjectionSegmentRecord],
    projection_entries: &[ProjectedEntry],
    suffix_entries: &[ProjectedEntry],
) -> Option<u64> {
    let mut total = 0_u64;
    let mut has_estimate = false;
    for segment in segments {
        if let Some(value) = segment.token_estimate {
            has_estimate = true;
            total = total.saturating_add(value);
        }
    }
    if !has_estimate && !projection_entries.is_empty() {
        has_estimate = true;
        total = total.saturating_add(entries_token_total(projection_entries));
    }
    for entry in suffix_entries {
        has_estimate = true;
        total = total.saturating_add(estimate_message_tokens(&entry.message));
    }
    has_estimate.then_some(total)
}

fn entries_token_estimate(entries: &[ProjectedEntry]) -> Option<u64> {
    if entries.is_empty() {
        return None;
    }
    Some(entries_token_total(entries))
}

fn entries_token_total(entries: &[ProjectedEntry]) -> u64 {
    entries.iter().fold(0_u64, |total, entry| {
        total.saturating_add(estimate_message_tokens(&entry.message))
    })
}

fn latest_event_seq(events: &[PersistedSessionEvent]) -> u64 {
    events
        .iter()
        .map(|event| event.event_seq)
        .max()
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::Mutex as StdMutex;

    use agentdash_agent_protocol::{
        BackboneEnvelope, BackboneEvent, SourceInfo, TraceInfo, UserInputSubmissionKind,
        UserInputSubmittedNotification, codex_app_server_protocol as codex,
    };
    use agentdash_agent_types::AgentMessage;
    use agentdash_spi::session_persistence::SessionStoreResult;
    use agentdash_spi::{
        SESSION_PROJECTION_KIND_MODEL_CONTEXT, SessionCompactionStatus, SessionEventBacklog,
        SessionEventPage, SessionEventStore,
    };
    use async_trait::async_trait;

    use super::super::memory_persistence::MemoryRuntimeTraceStore;
    use super::super::persistence::{
        NewCompactionProjectionCommit, SessionCompactionRecord, SessionCompactionStore,
        SessionMetaStore, SessionProjectionSegmentRecord, SessionProjectionStore, SessionStoreSet,
    };
    use super::super::types::{ExecutionStatus, SessionMeta};
    use super::*;

    /// 包裹真实事件存储，记录每次读取请求的起始 seq，用于断言 suffix-only 读取。
    struct ReadSpyEventStore {
        inner: Arc<dyn SessionEventStore>,
        list_all_calls: StdMutex<u32>,
        list_from_seqs: StdMutex<Vec<u64>>,
    }

    impl ReadSpyEventStore {
        fn new(inner: Arc<dyn SessionEventStore>) -> Self {
            Self {
                inner,
                list_all_calls: StdMutex::new(0),
                list_from_seqs: StdMutex::new(Vec::new()),
            }
        }

        fn list_all_calls(&self) -> u32 {
            *self.list_all_calls.lock().unwrap()
        }

        fn list_from_seqs(&self) -> Vec<u64> {
            self.list_from_seqs.lock().unwrap().clone()
        }
    }

    #[async_trait]
    impl SessionEventStore for ReadSpyEventStore {
        async fn append_event(
            &self,
            session_id: &str,
            envelope: &BackboneEnvelope,
        ) -> SessionStoreResult<PersistedSessionEvent> {
            self.inner.append_event(session_id, envelope).await
        }

        async fn read_backlog(
            &self,
            session_id: &str,
            after_seq: u64,
        ) -> SessionStoreResult<SessionEventBacklog> {
            self.inner.read_backlog(session_id, after_seq).await
        }

        async fn list_event_page(
            &self,
            session_id: &str,
            after_seq: u64,
            limit: u32,
        ) -> SessionStoreResult<SessionEventPage> {
            self.inner
                .list_event_page(session_id, after_seq, limit)
                .await
        }

        async fn list_all_events(
            &self,
            session_id: &str,
        ) -> SessionStoreResult<Vec<PersistedSessionEvent>> {
            *self.list_all_calls.lock().unwrap() += 1;
            self.inner.list_all_events(session_id).await
        }

        async fn list_events_from(
            &self,
            session_id: &str,
            from_seq: u64,
        ) -> SessionStoreResult<Vec<PersistedSessionEvent>> {
            self.list_from_seqs.lock().unwrap().push(from_seq);
            self.inner.list_events_from(session_id, from_seq).await
        }
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

    fn source_info() -> SourceInfo {
        SourceInfo {
            connector_id: "codex-bridge".to_string(),
            connector_type: "local_executor".to_string(),
            executor_id: Some("CODEX".to_string()),
        }
    }

    fn user_input_envelope(session_id: &str, turn_id: &str, text: &str) -> BackboneEnvelope {
        BackboneEnvelope::new(
            BackboneEvent::UserInputSubmitted(UserInputSubmittedNotification {
                thread_id: session_id.to_string(),
                turn_id: turn_id.to_string(),
                item_id: format!("{turn_id}:user-input:0"),
                submission_kind: UserInputSubmissionKind::Prompt,
                content: vec![codex::UserInput::Text {
                    text: text.to_string(),
                    text_elements: Vec::new(),
                }],
            }),
            session_id,
            source_info(),
        )
        .with_trace(TraceInfo {
            turn_id: Some(turn_id.to_string()),
            entry_index: Some(0),
        })
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
            source_info(),
        )
        .with_trace(TraceInfo {
            turn_id: Some(turn_id.to_string()),
            entry_index: Some(1),
        })
    }

    /// 构造一条 active compaction：prefix 已 materialize 成 context_envelope segment，
    /// suffix 仍是 raw 事件。返回 (compaction_id, suffix_start_event_seq)。
    fn commit_compaction(
        prefix_messages: Vec<agentdash_agent_types::AgentInputMessage>,
        first_kept_event_seq: u64,
        source_end_event_seq: u64,
    ) -> NewCompactionProjectionCommit {
        let session_id = "sess-ctx";
        let compaction_id = "compaction-1".to_string();
        let segment_id = format!("{compaction_id}-context");
        let completed_event = BackboneEnvelope::new(
            BackboneEvent::Platform(agentdash_agent_protocol::PlatformEvent::SessionMetaUpdate {
                key: "context_compacted".to_string(),
                value: serde_json::json!({ "compaction_id": compaction_id }),
            }),
            session_id,
            source_info(),
        );
        NewCompactionProjectionCommit {
            completed_event,
            compaction: SessionCompactionRecord {
                id: compaction_id.clone(),
                session_id: session_id.to_string(),
                projection_kind: SESSION_PROJECTION_KIND_MODEL_CONTEXT.to_string(),
                projection_version: 1,
                lifecycle_item_id: "compact-item".to_string(),
                start_event_seq: 1,
                completed_event_seq: None,
                failed_event_seq: None,
                status: SessionCompactionStatus::ProjectionCommitted,
                trigger: "auto".to_string(),
                reason: Some("token_pressure".to_string()),
                phase: Some("pre_provider".to_string()),
                strategy: "summary_prefix".to_string(),
                budget_scope: Some(SESSION_PROJECTION_KIND_MODEL_CONTEXT.to_string()),
                base_head_event_seq: Some(source_end_event_seq),
                source_start_event_seq: Some(1),
                source_end_event_seq: Some(source_end_event_seq),
                first_kept_event_seq: Some(first_kept_event_seq),
                summary: "prefix summary".to_string(),
                replacement_projection_json: serde_json::json!({}),
                token_stats_json: serde_json::json!({ "messages_compacted": 2 }),
                diagnostics_json: serde_json::json!({}),
                created_by: Some("agent".to_string()),
                created_at_ms: 1000,
                completed_at_ms: Some(2000),
            },
            segments: vec![SessionProjectionSegmentRecord {
                id: segment_id,
                session_id: session_id.to_string(),
                projection_kind: SESSION_PROJECTION_KIND_MODEL_CONTEXT.to_string(),
                projection_version: 1,
                sort_order: 0,
                segment_type: "context_envelope".to_string(),
                origin: "projection".to_string(),
                synthetic: true,
                source_start_event_seq: Some(1),
                source_end_event_seq: Some(source_end_event_seq),
                source_refs_json: serde_json::json!({ "messages_compacted": 2 }),
                generated_by_compaction_id: Some(compaction_id.clone()),
                content_json: serde_json::json!({ "messages": prefix_messages }),
                token_estimate: Some(128),
                created_at_ms: 1500,
            }],
            head: SessionProjectionHeadRecord {
                session_id: session_id.to_string(),
                projection_kind: SESSION_PROJECTION_KIND_MODEL_CONTEXT.to_string(),
                projection_version: 1,
                head_event_seq: 0,
                active_compaction_id: Some(compaction_id),
                updated_by_event_seq: None,
                updated_at_ms: 0,
            },
        }
    }

    async fn seed_session_with_compaction(
        persistence: &Arc<MemoryRuntimeTraceStore>,
    ) -> (SessionStoreSet, Arc<ReadSpyEventStore>) {
        let session_id = "sess-ctx";
        persistence
            .create_session(&test_meta(session_id))
            .await
            .expect("create session");

        // 待压缩 prefix 仍保留逐条 delta（会被 compaction materialize 取代）。
        persistence
            .append_event(
                session_id,
                &user_input_envelope(session_id, "turn-1", "old q"),
            )
            .await
            .expect("seed prefix user input");
        persistence
            .append_event(
                session_id,
                &assistant_delta_envelope(session_id, "turn-1", "old answer"),
            )
            .await
            .expect("seed prefix assistant");

        // compaction：materialize 一条 prefix 助手消息，first_kept=3（suffix 从 3 起）。
        let prefix_messages = vec![agentdash_agent_types::AgentInputMessage {
            message_ref: agentdash_agent_types::MessageRef {
                turn_id: "turn-1".to_string(),
                entry_index: 0,
            },
            projection_kind: agentdash_agent_types::ProjectionKind::ModelContext,
            message: AgentMessage::assistant("compacted prefix answer"),
            origin: agentdash_agent_types::ProjectionOrigin::Event,
            synthetic: false,
            source_event_seq: Some(2),
            source_range: None,
            projection_segment_id: None,
            provenance: serde_json::json!({}),
        }];
        let commit = commit_compaction(prefix_messages, 3, 2);
        persistence
            .commit_compaction_projection(session_id, commit)
            .await
            .expect("commit compaction");
        // commit 事件占用 event_seq=3；head_event_seq 被推进到 3。

        // suffix：新 turn 的 user input + assistant（event_seq >= 4），不依赖 prefix delta。
        persistence
            .append_event(
                session_id,
                &user_input_envelope(session_id, "turn-2", "new q"),
            )
            .await
            .expect("seed suffix user input");
        persistence
            .append_event(
                session_id,
                &assistant_delta_envelope(session_id, "turn-2", "new answer"),
            )
            .await
            .expect("seed suffix assistant");

        // 把 head 推进到覆盖 suffix。
        let mut head = persistence
            .read_projection_head(session_id, SESSION_PROJECTION_KIND_MODEL_CONTEXT)
            .await
            .expect("read head")
            .expect("head exists");
        head.head_event_seq = 5;
        persistence
            .upsert_projection_head(head)
            .await
            .expect("advance head");

        let spy = Arc::new(ReadSpyEventStore::new(persistence.clone()));
        let stores = SessionStoreSet {
            meta: persistence.clone(),
            events: spy.clone(),
            terminal_effects: persistence.clone(),
            runtime_commands: persistence.clone(),
            compactions: persistence.clone(),
            projections: persistence.clone(),
            lineage: persistence.clone(),
        };
        (stores, spy)
    }

    #[tokio::test]
    async fn build_model_context_reads_only_suffix_with_active_compaction() {
        let persistence = Arc::new(MemoryRuntimeTraceStore::default());
        let (stores, spy) = seed_session_with_compaction(&persistence).await;
        let projector = ContextProjector::new(stores.projection_stores());

        let envelope = projector
            .build_model_context("sess-ctx")
            .await
            .expect("build model context");

        // suffix 起点 = first_kept_event_seq = 3，绝不应回到全量读取。
        assert_eq!(
            spy.list_all_calls(),
            0,
            "active compaction 路径不应全量读取"
        );
        let from_seqs = spy.list_from_seqs();
        assert_eq!(from_seqs, vec![3], "应只读 suffix_start=3 起的区间");

        // prefix 来自 materialized segment，suffix 来自新 turn 事件。
        let texts: Vec<String> = envelope
            .messages
            .iter()
            .map(|m| format!("{:?}", m.message))
            .collect();
        let joined = texts.join("\n");
        assert!(
            joined.contains("compacted prefix answer"),
            "应包含 materialize 的 prefix: {joined}"
        );
        assert!(
            joined.contains("new q"),
            "应包含 suffix user input: {joined}"
        );
        assert!(
            joined.contains("new answer"),
            "应包含 suffix assistant: {joined}"
        );
        // prefix delta 已被 materialize 取代，不应重复出现。
        assert!(
            !joined.contains("old answer"),
            "suffix-only 读取不应带回被压缩的 prefix delta: {joined}"
        );
    }

    #[tokio::test]
    async fn build_model_context_equivalent_to_full_read_reference() {
        // 改造后产物必须与"全量读取后过滤"的参考实现等价。
        let persistence = Arc::new(MemoryRuntimeTraceStore::default());
        let (stores, _spy) = seed_session_with_compaction(&persistence).await;
        let projector = ContextProjector::new(stores.projection_stores());
        let actual = projector
            .build_model_context("sess-ctx")
            .await
            .expect("build model context");

        // 参考：手动用全量 events 走相同 checkpoint + suffix 过滤逻辑。
        let all_events = persistence
            .list_all_events("sess-ctx")
            .await
            .expect("list all");
        let head = persistence
            .read_projection_head("sess-ctx", SESSION_PROJECTION_KIND_MODEL_CONTEXT)
            .await
            .expect("read head")
            .expect("head");
        let compaction = persistence
            .get_compaction("sess-ctx", head.active_compaction_id.as_deref().unwrap())
            .await
            .expect("get compaction")
            .expect("compaction");
        let segments = persistence
            .list_projection_segments(
                "sess-ctx",
                SESSION_PROJECTION_KIND_MODEL_CONTEXT,
                head.projection_version,
            )
            .await
            .expect("segments");
        let mut entries = projection_entries_from_checkpoint_records(&compaction, &segments)
            .expect("checkpoint entries");
        let suffix_start =
            suffix_start_event_seq_from_compaction(&compaction, head.head_event_seq).unwrap();
        let suffix = build_raw_projected_transcript_from_filtered_events(all_events.iter().filter(
            |event| event.event_seq >= suffix_start && event.event_seq <= head.head_event_seq,
        ));
        entries.extend(suffix.entries);
        let expected: Vec<_> = entries
            .into_iter()
            .map(agentdash_agent_types::AgentInputMessage::from)
            .collect();

        assert_eq!(actual.messages, expected, "改造后 messages 必须逐条等价");
    }
}
