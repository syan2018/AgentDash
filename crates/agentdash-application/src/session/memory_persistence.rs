use std::collections::HashMap;
use std::io;
use std::sync::Arc;

use agentdash_agent_protocol::{BackboneEnvelope, BackboneEvent, PlatformEvent};
use tokio::sync::Mutex;

use super::hub_support::parse_turn_terminal_event_from_envelope;
use super::persistence::{
    CompactionProjectionCommitResult, NewCompactionProjectionCommit, PersistedSessionEvent,
    SessionCompactionRecord, SessionEventBacklog, SessionEventPage, SessionLineageRecord,
    SessionLineageRelationKind, SessionLineageStatus, SessionPersistence,
    SessionProjectionHeadRecord, SessionProjectionSegmentRecord,
};
use super::runtime_commands::{RuntimeCommandRecord, RuntimeCommandStatus};
use super::terminal_effects::{
    NewTerminalEffectRecord, TerminalEffectRecord, TerminalEffectStatus,
};
use super::types::{
    ExecutionStatus, PendingCapabilityStateTransition, SessionBootstrapState, SessionMeta,
};

#[derive(Clone, Default)]
pub struct MemorySessionPersistence {
    inner: Arc<Mutex<MemorySessionPersistenceState>>,
}

#[derive(Default)]
struct MemorySessionPersistenceState {
    metas: HashMap<String, SessionMeta>,
    events: HashMap<String, Vec<PersistedSessionEvent>>,
    terminal_effects: Vec<TerminalEffectRecord>,
    runtime_commands: Vec<RuntimeCommandRecord>,
    compactions: Vec<SessionCompactionRecord>,
    projection_segments: Vec<SessionProjectionSegmentRecord>,
    projection_heads: HashMap<(String, String), SessionProjectionHeadRecord>,
    lineage: Vec<SessionLineageRecord>,
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
        guard
            .terminal_effects
            .retain(|effect| effect.session_id != session_id);
        guard
            .runtime_commands
            .retain(|command| command.session_id != session_id);
        guard
            .compactions
            .retain(|compaction| compaction.session_id != session_id);
        guard
            .projection_segments
            .retain(|segment| segment.session_id != session_id);
        guard.projection_heads.retain(|(id, _), _| id != session_id);
        guard.lineage.retain(|edge| {
            edge.child_session_id != session_id && edge.parent_session_id != session_id
        });
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

    async fn insert_terminal_effect(
        &self,
        effect: NewTerminalEffectRecord,
    ) -> io::Result<TerminalEffectRecord> {
        let mut guard = self.inner.lock().await;
        if !guard.metas.contains_key(&effect.session_id) {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("session {} 不存在", effect.session_id),
            ));
        }
        let now = chrono::Utc::now().timestamp_millis();
        let record = TerminalEffectRecord {
            id: uuid::Uuid::new_v4(),
            session_id: effect.session_id,
            turn_id: effect.turn_id,
            terminal_event_seq: effect.terminal_event_seq,
            effect_type: effect.effect_type,
            payload: effect.payload,
            status: TerminalEffectStatus::Pending,
            attempt_count: 0,
            created_at_ms: now,
            updated_at_ms: now,
            last_error: None,
        };
        guard.terminal_effects.push(record.clone());
        Ok(record)
    }

    async fn mark_terminal_effect_running(&self, effect_id: uuid::Uuid) -> io::Result<()> {
        self.update_terminal_effect(effect_id, |effect, now| {
            effect.status = TerminalEffectStatus::Running;
            effect.attempt_count = effect.attempt_count.saturating_add(1);
            effect.updated_at_ms = now;
            effect.last_error = None;
        })
        .await
    }

    async fn mark_terminal_effect_succeeded(&self, effect_id: uuid::Uuid) -> io::Result<()> {
        self.update_terminal_effect(effect_id, |effect, now| {
            effect.status = TerminalEffectStatus::Succeeded;
            effect.updated_at_ms = now;
            effect.last_error = None;
        })
        .await
    }

    async fn mark_terminal_effect_failed(
        &self,
        effect_id: uuid::Uuid,
        error: String,
    ) -> io::Result<()> {
        self.update_terminal_effect(effect_id, |effect, now| {
            effect.status = TerminalEffectStatus::Failed;
            effect.updated_at_ms = now;
            effect.last_error = Some(error);
        })
        .await
    }

    async fn mark_terminal_effect_dead_letter(
        &self,
        effect_id: uuid::Uuid,
        error: String,
    ) -> io::Result<()> {
        self.update_terminal_effect(effect_id, |effect, now| {
            effect.status = TerminalEffectStatus::DeadLetter;
            effect.updated_at_ms = now;
            effect.last_error = Some(error);
        })
        .await
    }

    async fn list_terminal_effects_by_status(
        &self,
        statuses: &[TerminalEffectStatus],
        limit: u32,
    ) -> io::Result<Vec<TerminalEffectRecord>> {
        let guard = self.inner.lock().await;
        let limit = usize::try_from(limit.max(1))
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "分页大小超出 usize 范围"))?;
        let mut records = guard
            .terminal_effects
            .iter()
            .filter(|effect| statuses.contains(&effect.status))
            .cloned()
            .collect::<Vec<_>>();
        records.sort_by_key(|effect| (effect.updated_at_ms, effect.created_at_ms));
        records.truncate(limit);
        Ok(records)
    }

    async fn upsert_runtime_command_request(
        &self,
        session_id: &str,
        transition: PendingCapabilityStateTransition,
    ) -> io::Result<RuntimeCommandRecord> {
        let mut guard = self.inner.lock().await;
        if !guard.metas.contains_key(session_id) {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("session {session_id} 不存在"),
            ));
        }
        let now = chrono::Utc::now().timestamp_millis();
        for command in guard.runtime_commands.iter_mut().filter(|command| {
            command.session_id == session_id
                && command.phase_node == transition.phase_node
                && command.status == RuntimeCommandStatus::Requested
        }) {
            command.status = RuntimeCommandStatus::Failed;
            command.updated_at_ms = now;
            command.failed_at_ms = Some(now);
            command.last_error = Some("superseded_by_new_requested_command".to_string());
        }
        let record = RuntimeCommandRecord {
            id: uuid::Uuid::new_v4(),
            session_id: session_id.to_string(),
            transition_id: transition.id.clone(),
            phase_node: transition.phase_node.clone(),
            status: RuntimeCommandStatus::Requested,
            transition,
            created_at_ms: now,
            updated_at_ms: now,
            applied_at_ms: None,
            failed_at_ms: None,
            last_error: None,
        };
        guard.runtime_commands.push(record.clone());
        Ok(record)
    }

    async fn list_requested_runtime_commands(
        &self,
        session_id: &str,
    ) -> io::Result<Vec<RuntimeCommandRecord>> {
        let guard = self.inner.lock().await;
        let mut records = guard
            .runtime_commands
            .iter()
            .filter(|command| {
                command.session_id == session_id
                    && command.status == RuntimeCommandStatus::Requested
            })
            .cloned()
            .collect::<Vec<_>>();
        records.sort_by_key(|command| command.created_at_ms);
        Ok(records)
    }

    async fn mark_runtime_commands_applied(&self, command_ids: &[uuid::Uuid]) -> io::Result<()> {
        self.update_runtime_commands(command_ids, RuntimeCommandStatus::Applied, None)
            .await
    }

    async fn mark_runtime_commands_failed(
        &self,
        command_ids: &[uuid::Uuid],
        error: String,
    ) -> io::Result<()> {
        self.update_runtime_commands(command_ids, RuntimeCommandStatus::Failed, Some(error))
            .await
    }

    async fn list_runtime_commands_by_status(
        &self,
        statuses: &[RuntimeCommandStatus],
        limit: u32,
    ) -> io::Result<Vec<RuntimeCommandRecord>> {
        let guard = self.inner.lock().await;
        let limit = usize::try_from(limit.max(1))
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "分页大小超出 usize 范围"))?;
        let mut records = guard
            .runtime_commands
            .iter()
            .filter(|command| statuses.contains(&command.status))
            .cloned()
            .collect::<Vec<_>>();
        records.sort_by_key(|command| (command.updated_at_ms, command.created_at_ms));
        records.truncate(limit);
        Ok(records)
    }

    async fn get_compaction(
        &self,
        session_id: &str,
        compaction_id: &str,
    ) -> io::Result<Option<SessionCompactionRecord>> {
        let guard = self.inner.lock().await;
        Ok(guard
            .compactions
            .iter()
            .find(|record| record.session_id == session_id && record.id == compaction_id)
            .cloned())
    }

    async fn list_compactions(
        &self,
        session_id: &str,
        projection_kind: &str,
    ) -> io::Result<Vec<SessionCompactionRecord>> {
        let guard = self.inner.lock().await;
        let mut records = guard
            .compactions
            .iter()
            .filter(|record| {
                record.session_id == session_id && record.projection_kind == projection_kind
            })
            .cloned()
            .collect::<Vec<_>>();
        records.sort_by_key(|record| record.projection_version);
        Ok(records)
    }

    async fn list_projection_segments(
        &self,
        session_id: &str,
        projection_kind: &str,
        projection_version: u64,
    ) -> io::Result<Vec<SessionProjectionSegmentRecord>> {
        let guard = self.inner.lock().await;
        let mut segments = guard
            .projection_segments
            .iter()
            .filter(|segment| {
                segment.session_id == session_id
                    && segment.projection_kind == projection_kind
                    && segment.projection_version == projection_version
            })
            .cloned()
            .collect::<Vec<_>>();
        segments.sort_by_key(|segment| segment.sort_order);
        Ok(segments)
    }

    async fn read_projection_head(
        &self,
        session_id: &str,
        projection_kind: &str,
    ) -> io::Result<Option<SessionProjectionHeadRecord>> {
        let guard = self.inner.lock().await;
        Ok(guard
            .projection_heads
            .get(&projection_head_key(session_id, projection_kind))
            .cloned())
    }

    async fn upsert_projection_head(&self, head: SessionProjectionHeadRecord) -> io::Result<()> {
        let mut guard = self.inner.lock().await;
        if !guard.metas.contains_key(&head.session_id) {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("session {} 不存在", head.session_id),
            ));
        }
        guard.projection_heads.insert(
            projection_head_key(&head.session_id, &head.projection_kind),
            head,
        );
        Ok(())
    }

    async fn commit_compaction_projection(
        &self,
        session_id: &str,
        commit: NewCompactionProjectionCommit,
    ) -> io::Result<CompactionProjectionCommitResult> {
        let mut guard = self.inner.lock().await;
        if !guard.metas.contains_key(session_id) {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("session {session_id} 不存在"),
            ));
        }
        validate_commit_session(session_id, &commit)?;
        if guard
            .compactions
            .iter()
            .any(|record| record.session_id == session_id && record.id == commit.compaction.id)
        {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                format!("compaction {} 已存在", commit.compaction.id),
            ));
        }
        for segment in &commit.segments {
            if guard
                .projection_segments
                .iter()
                .any(|record| record.session_id == session_id && record.id == segment.id)
            {
                return Err(io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    format!("projection segment {} 已存在", segment.id),
                ));
            }
        }
        let committed_at_ms = chrono::Utc::now().timestamp_millis();
        let meta = guard.metas.get_mut(session_id).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!("session {session_id} 不存在"),
            )
        })?;
        let event_seq = meta.last_event_seq.checked_add(1).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("session {session_id} 的 event_seq 已溢出"),
            )
        })?;
        let persisted = build_persisted_event(
            session_id,
            event_seq,
            committed_at_ms,
            &commit.completed_event,
        );
        meta.last_event_seq = event_seq;
        meta.updated_at = committed_at_ms;
        apply_envelope_projection(meta, &commit.completed_event);
        guard
            .events
            .entry(session_id.to_string())
            .or_default()
            .push(persisted.clone());

        let mut compaction = commit.compaction;
        compaction.completed_event_seq = Some(event_seq);
        compaction.completed_at_ms = compaction.completed_at_ms.or(Some(committed_at_ms));
        let mut head = commit.head;
        head.head_event_seq = event_seq;
        head.updated_by_event_seq = Some(event_seq);
        head.updated_at_ms = if head.updated_at_ms == 0 {
            committed_at_ms
        } else {
            head.updated_at_ms
        };

        guard.compactions.push(compaction.clone());
        guard
            .projection_segments
            .extend(commit.segments.iter().cloned());
        guard.projection_heads.insert(
            projection_head_key(&head.session_id, &head.projection_kind),
            head.clone(),
        );

        Ok(CompactionProjectionCommitResult {
            event: persisted,
            compaction,
            segments: commit.segments,
            head,
        })
    }

    async fn upsert_session_lineage(&self, record: SessionLineageRecord) -> io::Result<()> {
        let mut guard = self.inner.lock().await;
        if record.child_session_id == record.parent_session_id {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "session lineage 不能指向自身",
            ));
        }
        if !guard.metas.contains_key(&record.child_session_id) {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("child session {} 不存在", record.child_session_id),
            ));
        }
        if !guard.metas.contains_key(&record.parent_session_id) {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("parent session {} 不存在", record.parent_session_id),
            ));
        }
        let mut current = Some(record.parent_session_id.clone());
        while let Some(session_id) = current {
            if session_id == record.child_session_id {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "session lineage 不能形成环",
                ));
            }
            current = guard
                .lineage
                .iter()
                .find(|edge| {
                    edge.child_session_id == session_id
                        && edge.child_session_id != record.child_session_id
                })
                .map(|edge| edge.parent_session_id.clone());
        }
        match guard
            .lineage
            .iter_mut()
            .find(|edge| edge.child_session_id == record.child_session_id)
        {
            Some(existing) => *existing = record,
            None => guard.lineage.push(record),
        }
        Ok(())
    }

    async fn get_session_lineage(
        &self,
        child_session_id: &str,
    ) -> io::Result<Option<SessionLineageRecord>> {
        let guard = self.inner.lock().await;
        Ok(guard
            .lineage
            .iter()
            .find(|edge| edge.child_session_id == child_session_id)
            .cloned())
    }

    async fn list_session_children(
        &self,
        parent_session_id: &str,
        relation_kind: Option<SessionLineageRelationKind>,
        status: Option<SessionLineageStatus>,
    ) -> io::Result<Vec<SessionLineageRecord>> {
        let guard = self.inner.lock().await;
        let mut children = guard
            .lineage
            .iter()
            .filter(|edge| {
                edge.parent_session_id == parent_session_id
                    && lineage_matches(edge, relation_kind, status)
            })
            .cloned()
            .collect::<Vec<_>>();
        sort_lineage_edges(&mut children);
        Ok(children)
    }

    async fn list_session_ancestors(
        &self,
        child_session_id: &str,
    ) -> io::Result<Vec<SessionLineageRecord>> {
        let guard = self.inner.lock().await;
        let mut ancestors = Vec::new();
        let mut current = child_session_id.to_string();
        while let Some(edge) = guard
            .lineage
            .iter()
            .find(|edge| edge.child_session_id == current)
        {
            ancestors.push(edge.clone());
            current = edge.parent_session_id.clone();
        }
        Ok(ancestors)
    }

    async fn list_session_descendants(
        &self,
        root_session_id: &str,
        relation_kind: Option<SessionLineageRelationKind>,
        status: Option<SessionLineageStatus>,
    ) -> io::Result<Vec<SessionLineageRecord>> {
        let guard = self.inner.lock().await;
        let mut result = Vec::new();
        let mut frontier = vec![root_session_id.to_string()];
        while !frontier.is_empty() {
            let mut next = Vec::new();
            for parent_id in frontier {
                let mut children = guard
                    .lineage
                    .iter()
                    .filter(|edge| {
                        edge.parent_session_id == parent_id
                            && lineage_matches(edge, relation_kind, status)
                    })
                    .cloned()
                    .collect::<Vec<_>>();
                sort_lineage_edges(&mut children);
                next.extend(children.iter().map(|edge| edge.child_session_id.clone()));
                result.extend(children);
            }
            frontier = next;
        }
        Ok(result)
    }

    async fn set_session_lineage_status(
        &self,
        child_session_id: &str,
        status: SessionLineageStatus,
        updated_at_ms: i64,
    ) -> io::Result<()> {
        let mut guard = self.inner.lock().await;
        let edge = guard
            .lineage
            .iter_mut()
            .find(|edge| edge.child_session_id == child_session_id)
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::NotFound,
                    format!("session lineage child {child_session_id} 不存在"),
                )
            })?;
        edge.status = status;
        edge.updated_at_ms = updated_at_ms;
        Ok(())
    }
}

impl MemorySessionPersistence {
    async fn update_terminal_effect(
        &self,
        effect_id: uuid::Uuid,
        update: impl FnOnce(&mut TerminalEffectRecord, i64),
    ) -> io::Result<()> {
        let mut guard = self.inner.lock().await;
        let now = chrono::Utc::now().timestamp_millis();
        let effect = guard
            .terminal_effects
            .iter_mut()
            .find(|effect| effect.id == effect_id)
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::NotFound,
                    format!("terminal effect {effect_id} 不存在"),
                )
            })?;
        update(effect, now);
        Ok(())
    }

    async fn update_runtime_commands(
        &self,
        command_ids: &[uuid::Uuid],
        status: RuntimeCommandStatus,
        error: Option<String>,
    ) -> io::Result<()> {
        let mut guard = self.inner.lock().await;
        let now = chrono::Utc::now().timestamp_millis();
        for command_id in command_ids {
            let command = guard
                .runtime_commands
                .iter_mut()
                .find(|command| command.id == *command_id)
                .ok_or_else(|| {
                    io::Error::new(
                        io::ErrorKind::NotFound,
                        format!("runtime command {command_id} 不存在"),
                    )
                })?;
            command.status = status;
            command.updated_at_ms = now;
            match status {
                RuntimeCommandStatus::Applied => {
                    command.applied_at_ms = Some(now);
                    command.last_error = None;
                }
                RuntimeCommandStatus::Failed => {
                    command.failed_at_ms = Some(now);
                    command.last_error = error.clone();
                }
                RuntimeCommandStatus::Requested => {}
            }
        }
        Ok(())
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

fn lineage_matches(
    edge: &SessionLineageRecord,
    relation_kind: Option<SessionLineageRelationKind>,
    status: Option<SessionLineageStatus>,
) -> bool {
    relation_kind
        .map(|kind| edge.relation_kind == kind)
        .unwrap_or(true)
        && status
            .map(|expected| edge.status == expected)
            .unwrap_or(true)
}

fn sort_lineage_edges(edges: &mut [SessionLineageRecord]) {
    edges.sort_by(|left, right| {
        (
            left.created_at_ms,
            left.updated_at_ms,
            left.child_session_id.as_str(),
        )
            .cmp(&(
                right.created_at_ms,
                right.updated_at_ms,
                right.child_session_id.as_str(),
            ))
    });
}

fn projection_head_key(session_id: &str, projection_kind: &str) -> (String, String) {
    (session_id.to_string(), projection_kind.to_string())
}

fn validate_commit_session(
    session_id: &str,
    commit: &NewCompactionProjectionCommit,
) -> io::Result<()> {
    if commit.compaction.session_id != session_id || commit.head.session_id != session_id {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("compaction projection commit session_id 不一致: {session_id}"),
        ));
    }
    if commit
        .segments
        .iter()
        .any(|segment| segment.session_id != session_id)
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("projection segment session_id 不一致: {session_id}"),
        ));
    }
    if commit.compaction.projection_kind != commit.head.projection_kind {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "compaction projection kind {} 与 head kind {} 不一致",
                commit.compaction.projection_kind, commit.head.projection_kind
            ),
        ));
    }
    if commit.compaction.projection_version != commit.head.projection_version {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "compaction projection version {} 与 head version {} 不一致",
                commit.compaction.projection_version, commit.head.projection_version
            ),
        ));
    }
    if commit.head.active_compaction_id.as_deref() != Some(commit.compaction.id.as_str()) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "projection head active_compaction_id 必须指向当前 compaction {}",
                commit.compaction.id
            ),
        ));
    }
    let compaction_range = source_range_pair(
        "session_compactions",
        commit.compaction.source_start_event_seq,
        commit.compaction.source_end_event_seq,
    )?;
    for segment in &commit.segments {
        if segment.projection_kind != commit.compaction.projection_kind {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "projection segment {} kind {} 与 compaction kind {} 不一致",
                    segment.id, segment.projection_kind, commit.compaction.projection_kind
                ),
            ));
        }
        if segment.projection_version != commit.compaction.projection_version {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "projection segment {} version {} 与 compaction version {} 不一致",
                    segment.id, segment.projection_version, commit.compaction.projection_version
                ),
            ));
        }
        if segment.generated_by_compaction_id.as_deref() != Some(commit.compaction.id.as_str()) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "projection segment {} 必须归属于 compaction {}",
                    segment.id, commit.compaction.id
                ),
            ));
        }
        let segment_range = source_range_pair(
            "session_projection_segments",
            segment.source_start_event_seq,
            segment.source_end_event_seq,
        )?;
        match (compaction_range, segment_range) {
            (Some((compaction_start, compaction_end)), Some((segment_start, segment_end)))
                if segment_start < compaction_start || segment_end > compaction_end =>
            {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!(
                        "projection segment {} source range 不在 compaction {} source range 内",
                        segment.id, commit.compaction.id
                    ),
                ));
            }
            (None, Some(_)) | (Some(_), None) => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!(
                        "projection segment {} source range 与 compaction {} 不一致",
                        segment.id, commit.compaction.id
                    ),
                ));
            }
            _ => {}
        }
    }
    Ok(())
}

fn source_range_pair(
    label: &str,
    start: Option<u64>,
    end: Option<u64>,
) -> io::Result<Option<(u64, u64)>> {
    match (start, end) {
        (Some(start), Some(end)) if start <= end => Ok(Some((start, end))),
        (Some(start), Some(end)) => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("{label} source range 非法: {start}>{end}"),
        )),
        (None, None) => Ok(None),
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("{label} source range 必须同时包含 start/end"),
        )),
    }
}

fn merge_session_meta(current: &mut SessionMeta, incoming: &SessionMeta) {
    let current_event_seq = current.last_event_seq;
    let incoming_event_seq = incoming.last_event_seq;

    current.title = incoming.title.clone();
    current.title_source = incoming.title_source;
    current.created_at = incoming.created_at;
    current.updated_at = current.updated_at.max(incoming.updated_at);
    current.last_event_seq = current.last_event_seq.max(incoming.last_event_seq);

    if incoming_event_seq >= current_event_seq {
        current.last_execution_status = incoming.last_execution_status;
        current.last_turn_id = incoming.last_turn_id.clone();
        current.last_terminal_message = incoming.last_terminal_message.clone();
    }

    current.executor_config = incoming.executor_config.clone();
    current.executor_session_id = incoming.executor_session_id.clone();
    current.companion_context = incoming.companion_context.clone();
    current.tab_layout = incoming.tab_layout.clone();
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
}

#[cfg(test)]
mod tests {
    use super::super::TerminalEffectType;
    use super::super::types::{RuntimeCapabilityTransition, TitleSource};
    use super::*;
    use agentdash_agent_protocol::{
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

    fn memory_session_meta(id: &str) -> SessionMeta {
        SessionMeta {
            id: id.to_string(),
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
            tab_layout: None,
            visible_canvas_mount_ids: Vec::new(),
            bootstrap_state: SessionBootstrapState::Plain,
        }
    }

    fn lineage_record(
        child: &str,
        parent: &str,
        relation_kind: SessionLineageRelationKind,
        status: SessionLineageStatus,
        created_at_ms: i64,
    ) -> SessionLineageRecord {
        SessionLineageRecord {
            child_session_id: child.to_string(),
            parent_session_id: parent.to_string(),
            relation_kind,
            fork_point_event_seq: Some(7),
            fork_point_ref_json: serde_json::json!({ "turn_id": "turn-1", "entry_index": 0 }),
            fork_point_compaction_id: None,
            status,
            created_at_ms,
            updated_at_ms: created_at_ms,
            metadata_json: serde_json::json!({}),
        }
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
            tab_layout: None,
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
        stale.tab_layout = Some(serde_json::json!({
            "tabs": [{"type_id": "session", "uri": "session://main", "title": "Session", "pinned": true}],
            "active_tab_uri": "session://main"
        }));
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
        assert_eq!(
            merged
                .tab_layout
                .as_ref()
                .and_then(|layout| layout.get("active_tab_uri"))
                .and_then(|value| value.as_str()),
            Some("session://main")
        );
        assert_eq!(merged.visible_canvas_mount_ids, vec!["canvas-a"]);
    }

    #[tokio::test]
    async fn terminal_effect_outbox_tracks_attempt_status_and_delete() {
        let persistence = MemorySessionPersistence::default();
        let meta = SessionMeta {
            id: "sess-effects".to_string(),
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
            tab_layout: None,
            visible_canvas_mount_ids: Vec::new(),
            bootstrap_state: SessionBootstrapState::Plain,
        };
        persistence
            .create_session(&meta)
            .await
            .expect("应能创建 session");

        let record = persistence
            .insert_terminal_effect(NewTerminalEffectRecord {
                session_id: "sess-effects".to_string(),
                turn_id: "turn-1".to_string(),
                terminal_event_seq: 1,
                effect_type: TerminalEffectType::HookAutoResume,
                payload: serde_json::json!({ "reason": "test" }),
            })
            .await
            .expect("应能写入 outbox");
        assert_eq!(record.status, TerminalEffectStatus::Pending);
        assert_eq!(record.attempt_count, 0);

        persistence
            .mark_terminal_effect_running(record.id)
            .await
            .expect("应能标记 running");
        let running = persistence
            .list_terminal_effects_by_status(&[TerminalEffectStatus::Running], 10)
            .await
            .expect("应能查询 running");
        assert_eq!(running.len(), 1);
        assert_eq!(running[0].attempt_count, 1);

        persistence
            .mark_terminal_effect_failed(record.id, "boom".to_string())
            .await
            .expect("应能标记 failed");
        let failed = persistence
            .list_terminal_effects_by_status(&[TerminalEffectStatus::Failed], 10)
            .await
            .expect("应能查询 failed");
        assert_eq!(failed[0].last_error.as_deref(), Some("boom"));

        persistence
            .delete_session("sess-effects")
            .await
            .expect("应能删除 session");
        let remaining = persistence
            .list_terminal_effects_by_status(&[TerminalEffectStatus::Failed], 10)
            .await
            .expect("应能查询 outbox");
        assert!(remaining.is_empty());
    }

    #[tokio::test]
    async fn runtime_command_store_supersedes_and_marks_applied() {
        let persistence = MemorySessionPersistence::default();
        let meta = SessionMeta {
            id: "sess-runtime-command".to_string(),
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
            tab_layout: None,
            visible_canvas_mount_ids: Vec::new(),
            bootstrap_state: SessionBootstrapState::Plain,
        };
        persistence
            .create_session(&meta)
            .await
            .expect("应能创建 session");

        let transition = |id: &str| PendingCapabilityStateTransition {
            id: id.to_string(),
            run_id: uuid::Uuid::new_v4(),
            lifecycle_key: "dev".to_string(),
            phase_node: "review".to_string(),
            capability_keys: std::collections::BTreeSet::new(),
            transition: RuntimeCapabilityTransition::default(),
            created_at: 1,
            source_turn_id: None,
        };
        let first = persistence
            .upsert_runtime_command_request("sess-runtime-command", transition("cmd-1"))
            .await
            .expect("应能写入第一条 command");
        let second = persistence
            .upsert_runtime_command_request("sess-runtime-command", transition("cmd-2"))
            .await
            .expect("应能写入第二条 command");

        let pending = persistence
            .list_requested_runtime_commands("sess-runtime-command")
            .await
            .expect("应能查询 requested command");
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].id, second.id);

        let failed = persistence
            .list_runtime_commands_by_status(&[RuntimeCommandStatus::Failed], 10)
            .await
            .expect("应能查询 failed command");
        assert_eq!(failed.len(), 1);
        assert_eq!(failed[0].id, first.id);
        assert_eq!(
            failed[0].last_error.as_deref(),
            Some("superseded_by_new_requested_command")
        );
        let payload = serde_json::to_value(&pending[0].transition)
            .expect("runtime command transition should serialize");
        assert!(payload.get("transition").is_some());
        assert!(payload.get("state").is_none());

        persistence
            .mark_runtime_commands_applied(&[second.id])
            .await
            .expect("应能标记 applied");
        let applied = persistence
            .list_runtime_commands_by_status(&[RuntimeCommandStatus::Applied], 10)
            .await
            .expect("应能查询 applied command");
        assert_eq!(applied.len(), 1);
        assert_eq!(applied[0].transition_id, "cmd-2");
    }

    #[tokio::test]
    async fn session_lineage_queries_are_stable_and_filterable() {
        let persistence = MemorySessionPersistence::default();
        for id in ["root", "child-a", "child-b", "grand"] {
            persistence
                .create_session(&memory_session_meta(id))
                .await
                .expect("应能创建 session");
        }

        persistence
            .upsert_session_lineage(lineage_record(
                "child-a",
                "root",
                SessionLineageRelationKind::Fork,
                SessionLineageStatus::Open,
                20,
            ))
            .await
            .expect("应能写入 fork edge");
        persistence
            .upsert_session_lineage(lineage_record(
                "child-b",
                "root",
                SessionLineageRelationKind::Companion,
                SessionLineageStatus::Open,
                10,
            ))
            .await
            .expect("应能写入 companion edge");
        persistence
            .upsert_session_lineage(lineage_record(
                "grand",
                "child-b",
                SessionLineageRelationKind::Fork,
                SessionLineageStatus::Open,
                30,
            ))
            .await
            .expect("应能写入 grand edge");

        let children = persistence
            .list_session_children("root", None, Some(SessionLineageStatus::Open))
            .await
            .expect("应能查询 direct children");
        assert_eq!(
            children
                .iter()
                .map(|edge| edge.child_session_id.as_str())
                .collect::<Vec<_>>(),
            vec!["child-b", "child-a"]
        );

        let fork_children = persistence
            .list_session_children(
                "root",
                Some(SessionLineageRelationKind::Fork),
                Some(SessionLineageStatus::Open),
            )
            .await
            .expect("应能按 relation 查询 children");
        assert_eq!(fork_children.len(), 1);
        assert_eq!(fork_children[0].child_session_id, "child-a");

        let ancestors = persistence
            .list_session_ancestors("grand")
            .await
            .expect("应能查询 ancestors");
        assert_eq!(
            ancestors
                .iter()
                .map(|edge| edge.child_session_id.as_str())
                .collect::<Vec<_>>(),
            vec!["grand", "child-b"]
        );

        let descendants = persistence
            .list_session_descendants("root", None, Some(SessionLineageStatus::Open))
            .await
            .expect("应能查询 descendants");
        assert_eq!(
            descendants
                .iter()
                .map(|edge| edge.child_session_id.as_str())
                .collect::<Vec<_>>(),
            vec!["child-b", "child-a", "grand"]
        );

        persistence
            .set_session_lineage_status("child-b", SessionLineageStatus::Closed, 40)
            .await
            .expect("应能关闭 lineage edge");
        let open_descendants = persistence
            .list_session_descendants("root", None, Some(SessionLineageStatus::Open))
            .await
            .expect("应能查询 open descendants");
        assert_eq!(
            open_descendants
                .iter()
                .map(|edge| edge.child_session_id.as_str())
                .collect::<Vec<_>>(),
            vec!["child-a"]
        );
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
        BackboneEvent::ExecutorContextCompacted(_) => "executor_context_compacted",
        BackboneEvent::ApprovalRequest(_) => "approval_request",
        BackboneEvent::Error(_) => "error",
        BackboneEvent::Platform(_) => "platform_event",
    }
}
