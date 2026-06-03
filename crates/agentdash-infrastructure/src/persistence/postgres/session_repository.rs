use agentdash_agent_protocol::BackboneEnvelope;
use agentdash_spi::session_persistence::{
    AgentFrameTransitionRecord, CompactionProjectionCommitResult, NewCompactionProjectionCommit,
    NewTerminalEffectRecord, PersistedSessionEvent, RuntimeCommandRecord, RuntimeCommandStatus,
    RuntimeDeliveryCommand, SessionCompactionRecord, SessionCompactionStore, SessionEventBacklog,
    SessionEventPage, SessionEventStore, SessionLineageRecord, SessionLineageRelationKind,
    SessionLineageStatus, SessionLineageStore, SessionMeta, SessionMetaStore,
    SessionProjectionHeadRecord, SessionProjectionSegmentRecord, SessionProjectionStore,
    SessionRuntimeCommandStore, SessionStoreError, SessionStoreResult, SessionTerminalEffectStore,
    TerminalEffectRecord, TerminalEffectStatus,
};
use sqlx::{PgPool, Row};

use crate::persistence::session_core::{
    backbone_event_type_name, compaction_from_row, encode_optional_u64_as_i64, encode_u64_as_i64,
    json_string, lineage_from_row, map_meta_row, optional_json_string, parse_non_negative_u64,
    persisted_event_from_row, projection_from_envelope, projection_head_from_row,
    projection_segment_from_row, runtime_command_from_row, sqlx_to_session_store_error,
    terminal_effect_from_row, title_source_to_str, validate_commit_session,
};

pub struct PostgresSessionRepository {
    pool: PgPool,
}

impl PostgresSessionRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn initialize(&self) -> SessionStoreResult<()> {
        crate::migration::assert_postgres_tables_ready(
            &self.pool,
            &[
                "sessions",
                "session_compactions",
                "session_events",
                "session_lineage",
                "session_projection_heads",
                "session_projection_segments",
                "session_terminal_effects",
                "agent_frame_transitions",
                "session_runtime_commands",
            ],
        )
        .await
        .map_err(|err| SessionStoreError::Database(err.to_string()))
    }

    async fn update_terminal_effect_status(
        &self,
        effect_id: uuid::Uuid,
        status: TerminalEffectStatus,
        updated_at_ms: i64,
        increment_attempt: bool,
        last_error: Option<String>,
    ) -> SessionStoreResult<()> {
        let result = sqlx::query(
            r#"
            UPDATE session_terminal_effects
            SET status = $1,
                attempt_count = attempt_count + $2,
                updated_at_ms = $3,
                last_error = $4
            WHERE id = $5
            "#,
        )
        .bind(status.as_str())
        .bind(if increment_attempt { 1_i64 } else { 0_i64 })
        .bind(updated_at_ms)
        .bind(last_error)
        .bind(effect_id.to_string())
        .execute(&self.pool)
        .await
        .map_err(sqlx_to_session_store_error)?;
        if result.rows_affected() == 0 {
            return Err(SessionStoreError::NotFound(format!(
                "terminal effect {effect_id} 不存在"
            )));
        }
        Ok(())
    }

    async fn update_runtime_commands_status(
        &self,
        command_ids: &[uuid::Uuid],
        status: RuntimeCommandStatus,
        error: Option<String>,
    ) -> SessionStoreResult<()> {
        if command_ids.is_empty() {
            return Ok(());
        }
        let now = chrono::Utc::now().timestamp_millis();
        let (applied_at_ms, failed_at_ms, last_error) = match status {
            RuntimeCommandStatus::Applied => (Some(now), None, None),
            RuntimeCommandStatus::Failed => (None, Some(now), error),
            RuntimeCommandStatus::Requested => (None, None, None),
        };
        let id_strings: Vec<String> = command_ids.iter().map(|id| id.to_string()).collect();
        let result = sqlx::query(
            r#"
            UPDATE session_runtime_commands
            SET status = $1,
                updated_at_ms = $2,
                applied_at_ms = COALESCE($3, applied_at_ms),
                failed_at_ms = COALESCE($4, failed_at_ms),
                last_error = $5
            WHERE id = ANY($6)
            "#,
        )
        .bind(status.as_str())
        .bind(now)
        .bind(applied_at_ms)
        .bind(failed_at_ms)
        .bind(last_error)
        .bind(&id_strings)
        .execute(&self.pool)
        .await
        .map_err(sqlx_to_session_store_error)?;
        if (result.rows_affected() as usize) != command_ids.len() {
            return Err(SessionStoreError::NotFound(format!(
                "部分 runtime command 不存在: 命中 {} / 期望 {}",
                result.rows_affected(),
                command_ids.len()
            )));
        }
        Ok(())
    }

    async fn require_snapshot_seq(&self, session_id: &str) -> SessionStoreResult<u64> {
        self.get_session_meta(session_id)
            .await?
            .map(|meta| meta.last_event_seq)
            .ok_or_else(|| SessionStoreError::NotFound(format!("session {session_id} 不存在")))
    }
}

fn validate_runtime_delivery_command(
    delivery: &RuntimeDeliveryCommand,
    frame_transition: &AgentFrameTransitionRecord,
) -> SessionStoreResult<()> {
    if delivery.frame_transition_id != frame_transition.id {
        return Err(SessionStoreError::InvalidInput(format!(
            "runtime delivery frame_transition_id {} 与 frame transition {} 不一致",
            delivery.frame_transition_id, frame_transition.id
        )));
    }
    if delivery.target_frame_id != frame_transition.target_frame_id {
        return Err(SessionStoreError::InvalidInput(format!(
            "runtime delivery target_frame_id {} 与 frame transition target {} 不一致",
            delivery.target_frame_id, frame_transition.target_frame_id
        )));
    }
    Ok(())
}

#[async_trait::async_trait]
impl SessionMetaStore for PostgresSessionRepository {
    async fn create_session(&self, meta: &SessionMeta) -> SessionStoreResult<()> {
        let last_event_seq = encode_u64_as_i64(meta.last_event_seq, "sessions.last_event_seq")?;
        let executor_config_json =
            optional_json_string(meta.executor_config.as_ref(), "executor_config_json")?;
        let tab_layout_json = optional_json_string(meta.tab_layout.as_ref(), "tab_layout_json")?;
        sqlx::query(
            r#"
            INSERT INTO sessions (
                id, title, title_source, project_id, created_at, updated_at, last_event_seq, last_delivery_status,
                last_turn_id, last_terminal_message, executor_config_json,
                executor_session_id, tab_layout_json
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
            "#,
        )
        .bind(&meta.id)
        .bind(&meta.title)
        .bind(title_source_to_str(meta.title_source))
        .bind(&meta.project_id)
        .bind(meta.created_at)
        .bind(meta.updated_at)
        .bind(last_event_seq)
        .bind(meta.last_delivery_status.to_string())
        .bind(&meta.last_turn_id)
        .bind(&meta.last_terminal_message)
        .bind(executor_config_json)
        .bind(&meta.executor_session_id)
        .bind(tab_layout_json)
        .execute(&self.pool)
        .await
        .map_err(sqlx_to_session_store_error)?;
        Ok(())
    }

    async fn get_session_meta(&self, session_id: &str) -> SessionStoreResult<Option<SessionMeta>> {
        let row = sqlx::query(
            r#"
            SELECT id, title, title_source, project_id, created_at, updated_at, last_event_seq, last_delivery_status,
                   last_turn_id, last_terminal_message, executor_config_json,
                   executor_session_id, tab_layout_json
            FROM sessions
            WHERE id = $1
            "#,
        )
        .bind(session_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(sqlx_to_session_store_error)?;
        row.as_ref().map(map_meta_row).transpose()
    }

    async fn list_sessions(&self) -> SessionStoreResult<Vec<SessionMeta>> {
        let rows = sqlx::query(
            r#"
            SELECT id, title, title_source, project_id, created_at, updated_at, last_event_seq, last_delivery_status,
                   last_turn_id, last_terminal_message, executor_config_json,
                   executor_session_id, tab_layout_json
            FROM sessions
            ORDER BY updated_at DESC
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(sqlx_to_session_store_error)?;
        rows.iter().map(map_meta_row).collect()
    }

    async fn save_session_meta(&self, meta: &SessionMeta) -> SessionStoreResult<()> {
        let last_event_seq = encode_u64_as_i64(meta.last_event_seq, "sessions.last_event_seq")?;
        let executor_config_json =
            optional_json_string(meta.executor_config.as_ref(), "executor_config_json")?;
        let tab_layout_json = optional_json_string(meta.tab_layout.as_ref(), "tab_layout_json")?;
        sqlx::query(
            r#"
            INSERT INTO sessions (
                id, title, title_source, project_id, created_at, updated_at, last_event_seq, last_delivery_status,
                last_turn_id, last_terminal_message, executor_config_json,
                executor_session_id, tab_layout_json
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
            ON CONFLICT(id) DO UPDATE SET
                title = excluded.title,
                title_source = excluded.title_source,
                project_id = COALESCE(excluded.project_id, sessions.project_id),
                created_at = excluded.created_at,
                updated_at = GREATEST(sessions.updated_at, excluded.updated_at),
                last_event_seq = GREATEST(sessions.last_event_seq, excluded.last_event_seq),
                last_delivery_status = CASE
                    WHEN excluded.last_event_seq >= sessions.last_event_seq
                        THEN excluded.last_delivery_status
                    ELSE sessions.last_delivery_status
                END,
                last_turn_id = CASE
                    WHEN excluded.last_event_seq >= sessions.last_event_seq
                        THEN excluded.last_turn_id
                    ELSE sessions.last_turn_id
                END,
                last_terminal_message = CASE
                    WHEN excluded.last_event_seq >= sessions.last_event_seq
                        THEN excluded.last_terminal_message
                    ELSE sessions.last_terminal_message
                END,
                executor_config_json = excluded.executor_config_json,
                executor_session_id = excluded.executor_session_id,
                tab_layout_json = excluded.tab_layout_json
            "#,
        )
        .bind(&meta.id)
        .bind(&meta.title)
        .bind(title_source_to_str(meta.title_source))
        .bind(&meta.project_id)
        .bind(meta.created_at)
        .bind(meta.updated_at)
        .bind(last_event_seq)
        .bind(meta.last_delivery_status.to_string())
        .bind(&meta.last_turn_id)
        .bind(&meta.last_terminal_message)
        .bind(executor_config_json)
        .bind(&meta.executor_session_id)
        .bind(tab_layout_json)
        .execute(&self.pool)
        .await
        .map_err(sqlx_to_session_store_error)?;
        Ok(())
    }

    async fn delete_session(&self, session_id: &str) -> SessionStoreResult<()> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(sqlx_to_session_store_error)?;
        sqlx::query("DELETE FROM session_events WHERE session_id = $1")
            .bind(session_id)
            .execute(&mut *tx)
            .await
            .map_err(sqlx_to_session_store_error)?;
        sqlx::query("DELETE FROM session_terminal_effects WHERE session_id = $1")
            .bind(session_id)
            .execute(&mut *tx)
            .await
            .map_err(sqlx_to_session_store_error)?;
        sqlx::query("DELETE FROM session_runtime_commands WHERE session_id = $1")
            .bind(session_id)
            .execute(&mut *tx)
            .await
            .map_err(sqlx_to_session_store_error)?;
        sqlx::query(
            "DELETE FROM session_lineage WHERE child_session_id = $1 OR parent_session_id = $1",
        )
        .bind(session_id)
        .execute(&mut *tx)
        .await
        .map_err(sqlx_to_session_store_error)?;
        sqlx::query("DELETE FROM session_projection_heads WHERE session_id = $1")
            .bind(session_id)
            .execute(&mut *tx)
            .await
            .map_err(sqlx_to_session_store_error)?;
        sqlx::query("DELETE FROM session_projection_segments WHERE session_id = $1")
            .bind(session_id)
            .execute(&mut *tx)
            .await
            .map_err(sqlx_to_session_store_error)?;
        sqlx::query("DELETE FROM session_compactions WHERE session_id = $1")
            .bind(session_id)
            .execute(&mut *tx)
            .await
            .map_err(sqlx_to_session_store_error)?;
        sqlx::query("DELETE FROM sessions WHERE id = $1")
            .bind(session_id)
            .execute(&mut *tx)
            .await
            .map_err(sqlx_to_session_store_error)?;
        tx.commit().await.map_err(sqlx_to_session_store_error)?;
        Ok(())
    }
}

#[async_trait::async_trait]
impl SessionEventStore for PostgresSessionRepository {
    async fn append_event(
        &self,
        session_id: &str,
        envelope: &BackboneEnvelope,
    ) -> SessionStoreResult<PersistedSessionEvent> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(sqlx_to_session_store_error)?;
        let committed_at_ms = chrono::Utc::now().timestamp_millis();
        let seq_row = sqlx::query(
            r#"
            UPDATE sessions
            SET last_event_seq = last_event_seq + 1
            WHERE id = $1
            RETURNING last_event_seq
            "#,
        )
        .bind(session_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(sqlx_to_session_store_error)?
        .ok_or_else(|| SessionStoreError::NotFound(format!("session {session_id} 不存在")))?;
        let event_seq_i64: i64 = seq_row
            .try_get("last_event_seq")
            .map_err(sqlx_to_session_store_error)?;
        let event_seq = parse_non_negative_u64(event_seq_i64, "sessions.last_event_seq")?;
        let projection = projection_from_envelope(envelope);
        let persisted = PersistedSessionEvent {
            session_id: session_id.to_string(),
            event_seq,
            occurred_at_ms: committed_at_ms,
            committed_at_ms,
            session_update_type: backbone_event_type_name(&envelope.event).to_string(),
            turn_id: projection.turn_id.clone(),
            entry_index: projection.entry_index,
            tool_call_id: projection.tool_call_id.clone(),
            notification: envelope.clone(),
        };
        let notification_json = json_string(&persisted.notification, "notification_json")?;
        let event_seq_db = encode_u64_as_i64(event_seq, "session_events.event_seq")?;

        sqlx::query(
            r#"
            INSERT INTO session_events (
                session_id, event_seq, occurred_at_ms, committed_at_ms,
                session_update_type, turn_id, entry_index, tool_call_id, notification_json
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            "#,
        )
        .bind(session_id)
        .bind(event_seq_db)
        .bind(persisted.occurred_at_ms)
        .bind(persisted.committed_at_ms)
        .bind(&persisted.session_update_type)
        .bind(&persisted.turn_id)
        .bind(persisted.entry_index.map(i64::from))
        .bind(&persisted.tool_call_id)
        .bind(notification_json)
        .execute(&mut *tx)
        .await
        .map_err(sqlx_to_session_store_error)?;

        sqlx::query(
            r#"
            UPDATE sessions
            SET
                updated_at = $1,
                last_delivery_status = COALESCE($2, last_delivery_status),
                last_turn_id = COALESCE($3, last_turn_id),
                last_terminal_message = CASE
                    WHEN $4 THEN NULL
                    WHEN $5 IS NOT NULL THEN $6
                    ELSE last_terminal_message
                END,
                executor_session_id = COALESCE($7, executor_session_id)
            WHERE id = $8
            "#,
        )
        .bind(committed_at_ms)
        .bind(&projection.last_delivery_status)
        .bind(&projection.turn_id)
        .bind(projection.clear_terminal_message)
        .bind(&projection.last_terminal_message)
        .bind(&projection.last_terminal_message)
        .bind(&projection.executor_session_id)
        .bind(session_id)
        .execute(&mut *tx)
        .await
        .map_err(sqlx_to_session_store_error)?;

        tx.commit().await.map_err(sqlx_to_session_store_error)?;
        Ok(persisted)
    }

    async fn read_backlog(
        &self,
        session_id: &str,
        after_seq: u64,
    ) -> SessionStoreResult<SessionEventBacklog> {
        let snapshot_seq = self.require_snapshot_seq(session_id).await?;
        let after_seq_db = encode_u64_as_i64(after_seq, "session_events.after_seq")?;
        let snapshot_seq_db = encode_u64_as_i64(snapshot_seq, "sessions.last_event_seq")?;
        let rows = sqlx::query(
            r#"
            SELECT session_id, event_seq, occurred_at_ms, committed_at_ms,
                   session_update_type, turn_id, entry_index, tool_call_id, notification_json
            FROM session_events
            WHERE session_id = $1 AND event_seq > $2 AND event_seq <= $3
            ORDER BY event_seq ASC
            "#,
        )
        .bind(session_id)
        .bind(after_seq_db)
        .bind(snapshot_seq_db)
        .fetch_all(&self.pool)
        .await
        .map_err(sqlx_to_session_store_error)?;

        let mut events = Vec::with_capacity(rows.len());
        for row in rows {
            events.push(persisted_event_from_row(&row)?);
        }

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
    ) -> SessionStoreResult<SessionEventPage> {
        let snapshot_seq = self.require_snapshot_seq(session_id).await?;
        let take = limit.max(1);
        let after_seq_db = encode_u64_as_i64(after_seq, "session_events.after_seq")?;
        let take_usize = usize::try_from(take)
            .map_err(|_| SessionStoreError::InvalidData("分页大小超出 usize 范围".to_string()))?;
        let rows = sqlx::query(
            r#"
            SELECT session_id, event_seq, occurred_at_ms, committed_at_ms,
                   session_update_type, turn_id, entry_index, tool_call_id, notification_json
            FROM session_events
            WHERE session_id = $1 AND event_seq > $2
            ORDER BY event_seq ASC
            LIMIT $3
            "#,
        )
        .bind(session_id)
        .bind(after_seq_db)
        .bind(i64::from(take) + 1)
        .fetch_all(&self.pool)
        .await
        .map_err(sqlx_to_session_store_error)?;

        let has_more = rows.len() > take_usize;
        let mut events = Vec::new();
        for row in rows.into_iter().take(take_usize) {
            events.push(persisted_event_from_row(&row)?);
        }
        let next_after_seq = events
            .last()
            .map(|event| event.event_seq)
            .unwrap_or(after_seq);
        Ok(SessionEventPage {
            snapshot_seq,
            events,
            has_more,
            next_after_seq,
        })
    }

    async fn list_all_events(
        &self,
        session_id: &str,
    ) -> SessionStoreResult<Vec<PersistedSessionEvent>> {
        let rows = sqlx::query(
            r#"
            SELECT session_id, event_seq, occurred_at_ms, committed_at_ms,
                   session_update_type, turn_id, entry_index, tool_call_id, notification_json
            FROM session_events
            WHERE session_id = $1
            ORDER BY event_seq ASC
            "#,
        )
        .bind(session_id)
        .fetch_all(&self.pool)
        .await
        .map_err(sqlx_to_session_store_error)?;

        let mut events = Vec::with_capacity(rows.len());
        for row in rows {
            events.push(persisted_event_from_row(&row)?);
        }
        Ok(events)
    }
}

#[async_trait::async_trait]
impl SessionTerminalEffectStore for PostgresSessionRepository {
    async fn insert_terminal_effect(
        &self,
        effect: NewTerminalEffectRecord,
    ) -> SessionStoreResult<TerminalEffectRecord> {
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
        let terminal_event_seq = encode_u64_as_i64(
            record.terminal_event_seq,
            "session_terminal_effects.terminal_event_seq",
        )?;
        let payload_json = json_string(&record.payload, "session_terminal_effects.payload_json")?;
        sqlx::query(
            r#"
            INSERT INTO session_terminal_effects (
                id, session_id, turn_id, terminal_event_seq, effect_type, payload_json,
                status, attempt_count, created_at_ms, updated_at_ms, last_error
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
            "#,
        )
        .bind(record.id.to_string())
        .bind(&record.session_id)
        .bind(&record.turn_id)
        .bind(terminal_event_seq)
        .bind(record.effect_type.as_str())
        .bind(payload_json)
        .bind(record.status.as_str())
        .bind(i64::from(record.attempt_count))
        .bind(record.created_at_ms)
        .bind(record.updated_at_ms)
        .bind(&record.last_error)
        .execute(&self.pool)
        .await
        .map_err(sqlx_to_session_store_error)?;
        Ok(record)
    }

    async fn mark_terminal_effect_running(&self, effect_id: uuid::Uuid) -> SessionStoreResult<()> {
        let now = chrono::Utc::now().timestamp_millis();
        self.update_terminal_effect_status(
            effect_id,
            TerminalEffectStatus::Running,
            now,
            true,
            None,
        )
        .await
    }

    async fn mark_terminal_effect_succeeded(
        &self,
        effect_id: uuid::Uuid,
    ) -> SessionStoreResult<()> {
        let now = chrono::Utc::now().timestamp_millis();
        self.update_terminal_effect_status(
            effect_id,
            TerminalEffectStatus::Succeeded,
            now,
            false,
            None,
        )
        .await
    }

    async fn mark_terminal_effect_failed(
        &self,
        effect_id: uuid::Uuid,
        error: String,
    ) -> SessionStoreResult<()> {
        let now = chrono::Utc::now().timestamp_millis();
        self.update_terminal_effect_status(
            effect_id,
            TerminalEffectStatus::Failed,
            now,
            false,
            Some(error),
        )
        .await
    }

    async fn mark_terminal_effect_dead_letter(
        &self,
        effect_id: uuid::Uuid,
        error: String,
    ) -> SessionStoreResult<()> {
        let now = chrono::Utc::now().timestamp_millis();
        self.update_terminal_effect_status(
            effect_id,
            TerminalEffectStatus::DeadLetter,
            now,
            false,
            Some(error),
        )
        .await
    }

    async fn list_terminal_effects_by_status(
        &self,
        statuses: &[TerminalEffectStatus],
        limit: u32,
    ) -> SessionStoreResult<Vec<TerminalEffectRecord>> {
        if statuses.is_empty() {
            return Ok(Vec::new());
        }
        let status_filter: Vec<&str> = statuses.iter().map(|status| status.as_str()).collect();
        let limit = i64::from(limit.max(1));
        let rows = sqlx::query(
            r#"
            SELECT id, session_id, turn_id, terminal_event_seq, effect_type, payload_json,
                   status, attempt_count, created_at_ms, updated_at_ms, last_error
            FROM session_terminal_effects
            WHERE status = ANY($1)
            ORDER BY updated_at_ms ASC, created_at_ms ASC
            LIMIT $2
            "#,
        )
        .bind(&status_filter)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(sqlx_to_session_store_error)?;
        rows.iter().map(terminal_effect_from_row).collect()
    }
}

#[async_trait::async_trait]
impl SessionRuntimeCommandStore for PostgresSessionRepository {
    async fn upsert_runtime_delivery_command(
        &self,
        delivery_runtime_session_id: &str,
        delivery: RuntimeDeliveryCommand,
        frame_transition: AgentFrameTransitionRecord,
    ) -> SessionStoreResult<RuntimeCommandRecord> {
        validate_runtime_delivery_command(&delivery, &frame_transition)?;
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(sqlx_to_session_store_error)?;
        let now = chrono::Utc::now().timestamp_millis();
        sqlx::query(
            r#"
            UPDATE session_runtime_commands
            SET status = $1,
                updated_at_ms = $2,
                failed_at_ms = $3,
                last_error = $4
            WHERE session_id = $5 AND phase_node = $6 AND status = $7
            "#,
        )
        .bind(RuntimeCommandStatus::Failed.as_str())
        .bind(now)
        .bind(now)
        .bind("superseded_by_new_requested_command")
        .bind(delivery_runtime_session_id)
        .bind(&frame_transition.phase_node)
        .bind(RuntimeCommandStatus::Requested.as_str())
        .execute(&mut *tx)
        .await
        .map_err(sqlx_to_session_store_error)?;

        let capability_keys_json = json_string(
            &frame_transition.capability_keys,
            "agent_frame_transitions.capability_keys_json",
        )?;
        let transition_json = json_string(
            &frame_transition.transition,
            "agent_frame_transitions.transition_json",
        )?;
        sqlx::query(
            r#"
            INSERT INTO agent_frame_transitions (
                id, target_frame_id, run_id, lifecycle_key, phase_node,
                capability_keys_json, transition_json, source_turn_id, created_at_ms
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            ON CONFLICT(id) DO UPDATE SET
                target_frame_id = excluded.target_frame_id,
                run_id = excluded.run_id,
                lifecycle_key = excluded.lifecycle_key,
                phase_node = excluded.phase_node,
                capability_keys_json = excluded.capability_keys_json,
                transition_json = excluded.transition_json,
                source_turn_id = excluded.source_turn_id,
                created_at_ms = excluded.created_at_ms
            "#,
        )
        .bind(&frame_transition.id)
        .bind(frame_transition.target_frame_id.to_string())
        .bind(frame_transition.run_id.to_string())
        .bind(&frame_transition.lifecycle_key)
        .bind(&frame_transition.phase_node)
        .bind(capability_keys_json)
        .bind(transition_json)
        .bind(&frame_transition.source_turn_id)
        .bind(frame_transition.created_at_ms)
        .execute(&mut *tx)
        .await
        .map_err(sqlx_to_session_store_error)?;

        let record = RuntimeCommandRecord {
            id: uuid::Uuid::new_v4(),
            session_id: delivery_runtime_session_id.to_string(),
            frame_transition_id: frame_transition.id.clone(),
            phase_node: frame_transition.phase_node.clone(),
            status: RuntimeCommandStatus::Requested,
            delivery,
            frame_transition,
            created_at_ms: now,
            updated_at_ms: now,
            applied_at_ms: None,
            failed_at_ms: None,
            last_error: None,
        };
        let payload_json = json_string(&record.delivery, "session_runtime_commands.payload_json")?;
        sqlx::query(
            r#"
            INSERT INTO session_runtime_commands (
                id, session_id, frame_transition_id, phase_node, status, payload_json,
                created_at_ms, updated_at_ms, applied_at_ms, failed_at_ms, last_error
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
            "#,
        )
        .bind(record.id.to_string())
        .bind(&record.session_id)
        .bind(&record.frame_transition_id)
        .bind(&record.phase_node)
        .bind(record.status.as_str())
        .bind(payload_json)
        .bind(record.created_at_ms)
        .bind(record.updated_at_ms)
        .bind(record.applied_at_ms)
        .bind(record.failed_at_ms)
        .bind(&record.last_error)
        .execute(&mut *tx)
        .await
        .map_err(sqlx_to_session_store_error)?;
        tx.commit().await.map_err(sqlx_to_session_store_error)?;
        Ok(record)
    }

    async fn list_requested_runtime_commands(
        &self,
        session_id: &str,
    ) -> SessionStoreResult<Vec<RuntimeCommandRecord>> {
        let rows = sqlx::query(
            r#"
            SELECT c.id, c.session_id, c.frame_transition_id, c.phase_node, c.status, c.payload_json,
                   c.created_at_ms, c.updated_at_ms, c.applied_at_ms, c.failed_at_ms, c.last_error,
                   t.id AS frame_transition_record_id,
                   t.target_frame_id AS frame_transition_target_frame_id,
                   t.run_id AS frame_transition_run_id,
                   t.lifecycle_key AS frame_transition_lifecycle_key,
                   t.phase_node AS frame_transition_phase_node,
                   t.capability_keys_json AS frame_transition_capability_keys_json,
                   t.transition_json AS frame_transition_transition_json,
                   t.source_turn_id AS frame_transition_source_turn_id,
                   t.created_at_ms AS frame_transition_created_at_ms
            FROM session_runtime_commands c
            JOIN agent_frame_transitions t ON t.id = c.frame_transition_id
            WHERE c.session_id = $1 AND c.status = $2
            ORDER BY c.created_at_ms ASC
            "#,
        )
        .bind(session_id)
        .bind(RuntimeCommandStatus::Requested.as_str())
        .fetch_all(&self.pool)
        .await
        .map_err(sqlx_to_session_store_error)?;
        rows.iter().map(runtime_command_from_row).collect()
    }

    async fn mark_runtime_commands_applied(
        &self,
        command_ids: &[uuid::Uuid],
    ) -> SessionStoreResult<()> {
        self.update_runtime_commands_status(command_ids, RuntimeCommandStatus::Applied, None)
            .await
    }

    async fn mark_runtime_commands_failed(
        &self,
        command_ids: &[uuid::Uuid],
        error: String,
    ) -> SessionStoreResult<()> {
        self.update_runtime_commands_status(command_ids, RuntimeCommandStatus::Failed, Some(error))
            .await
    }

    async fn list_runtime_commands_by_status(
        &self,
        statuses: &[RuntimeCommandStatus],
        limit: u32,
    ) -> SessionStoreResult<Vec<RuntimeCommandRecord>> {
        if statuses.is_empty() {
            return Ok(Vec::new());
        }
        let status_filter: Vec<&str> = statuses.iter().map(|status| status.as_str()).collect();
        let limit = i64::from(limit.max(1));
        let rows = sqlx::query(
            r#"
            SELECT c.id, c.session_id, c.frame_transition_id, c.phase_node, c.status, c.payload_json,
                   c.created_at_ms, c.updated_at_ms, c.applied_at_ms, c.failed_at_ms, c.last_error,
                   t.id AS frame_transition_record_id,
                   t.target_frame_id AS frame_transition_target_frame_id,
                   t.run_id AS frame_transition_run_id,
                   t.lifecycle_key AS frame_transition_lifecycle_key,
                   t.phase_node AS frame_transition_phase_node,
                   t.capability_keys_json AS frame_transition_capability_keys_json,
                   t.transition_json AS frame_transition_transition_json,
                   t.source_turn_id AS frame_transition_source_turn_id,
                   t.created_at_ms AS frame_transition_created_at_ms
            FROM session_runtime_commands c
            JOIN agent_frame_transitions t ON t.id = c.frame_transition_id
            WHERE c.status = ANY($1)
            ORDER BY c.updated_at_ms ASC, c.created_at_ms ASC
            LIMIT $2
            "#,
        )
        .bind(&status_filter)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(sqlx_to_session_store_error)?;
        rows.iter().map(runtime_command_from_row).collect()
    }
}

#[async_trait::async_trait]
impl SessionCompactionStore for PostgresSessionRepository {
    async fn get_compaction(
        &self,
        session_id: &str,
        compaction_id: &str,
    ) -> SessionStoreResult<Option<SessionCompactionRecord>> {
        let row = sqlx::query(
            r#"
            SELECT id, session_id, projection_kind, projection_version,
                   lifecycle_item_id, start_event_seq, completed_event_seq, failed_event_seq,
                   status, trigger, reason, phase, strategy, budget_scope,
                   base_head_event_seq, source_start_event_seq, source_end_event_seq,
                   first_kept_event_seq, summary, replacement_projection_json,
                   token_stats_json, diagnostics_json, created_by, created_at_ms, completed_at_ms
            FROM session_compactions
            WHERE session_id = $1 AND id = $2
            "#,
        )
        .bind(session_id)
        .bind(compaction_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(sqlx_to_session_store_error)?;
        row.as_ref().map(compaction_from_row).transpose()
    }

    async fn list_compactions(
        &self,
        session_id: &str,
        projection_kind: &str,
    ) -> SessionStoreResult<Vec<SessionCompactionRecord>> {
        let rows = sqlx::query(
            r#"
            SELECT id, session_id, projection_kind, projection_version,
                   lifecycle_item_id, start_event_seq, completed_event_seq, failed_event_seq,
                   status, trigger, reason, phase, strategy, budget_scope,
                   base_head_event_seq, source_start_event_seq, source_end_event_seq,
                   first_kept_event_seq, summary, replacement_projection_json,
                   token_stats_json, diagnostics_json, created_by, created_at_ms, completed_at_ms
            FROM session_compactions
            WHERE session_id = $1 AND projection_kind = $2
            ORDER BY projection_version ASC, created_at_ms ASC
            "#,
        )
        .bind(session_id)
        .bind(projection_kind)
        .fetch_all(&self.pool)
        .await
        .map_err(sqlx_to_session_store_error)?;
        rows.iter().map(compaction_from_row).collect()
    }
}

#[async_trait::async_trait]
impl SessionProjectionStore for PostgresSessionRepository {
    async fn list_projection_segments(
        &self,
        session_id: &str,
        projection_kind: &str,
        projection_version: u64,
    ) -> SessionStoreResult<Vec<SessionProjectionSegmentRecord>> {
        let projection_version = encode_u64_as_i64(
            projection_version,
            "session_projection_segments.projection_version",
        )?;
        let rows = sqlx::query(
            r#"
            SELECT id, session_id, projection_kind, projection_version, sort_order,
                   segment_type, origin, synthetic, source_start_event_seq, source_end_event_seq,
                   source_refs_json, generated_by_compaction_id, content_json, token_estimate,
                   created_at_ms
            FROM session_projection_segments
            WHERE session_id = $1 AND projection_kind = $2 AND projection_version = $3
            ORDER BY sort_order ASC
            "#,
        )
        .bind(session_id)
        .bind(projection_kind)
        .bind(projection_version)
        .fetch_all(&self.pool)
        .await
        .map_err(sqlx_to_session_store_error)?;
        rows.iter().map(projection_segment_from_row).collect()
    }

    async fn read_projection_head(
        &self,
        session_id: &str,
        projection_kind: &str,
    ) -> SessionStoreResult<Option<SessionProjectionHeadRecord>> {
        let row = sqlx::query(
            r#"
            SELECT session_id, projection_kind, projection_version, head_event_seq,
                   active_compaction_id, updated_by_event_seq, updated_at_ms
            FROM session_projection_heads
            WHERE session_id = $1 AND projection_kind = $2
            "#,
        )
        .bind(session_id)
        .bind(projection_kind)
        .fetch_optional(&self.pool)
        .await
        .map_err(sqlx_to_session_store_error)?;
        row.as_ref().map(projection_head_from_row).transpose()
    }

    async fn upsert_projection_head(
        &self,
        head: SessionProjectionHeadRecord,
    ) -> SessionStoreResult<()> {
        let projection_version = encode_u64_as_i64(
            head.projection_version,
            "session_projection_heads.projection_version",
        )?;
        let head_event_seq = encode_u64_as_i64(
            head.head_event_seq,
            "session_projection_heads.head_event_seq",
        )?;
        let updated_by_event_seq = encode_optional_u64_as_i64(
            head.updated_by_event_seq,
            "session_projection_heads.updated_by_event_seq",
        )?;
        sqlx::query(
            r#"
            INSERT INTO session_projection_heads (
                session_id, projection_kind, projection_version, head_event_seq,
                active_compaction_id, updated_by_event_seq, updated_at_ms
            ) VALUES ($1, $2, $3, $4, $5, $6, $7)
            ON CONFLICT(session_id, projection_kind) DO UPDATE SET
                projection_version = excluded.projection_version,
                head_event_seq = excluded.head_event_seq,
                active_compaction_id = excluded.active_compaction_id,
                updated_by_event_seq = excluded.updated_by_event_seq,
                updated_at_ms = excluded.updated_at_ms
            "#,
        )
        .bind(&head.session_id)
        .bind(&head.projection_kind)
        .bind(projection_version)
        .bind(head_event_seq)
        .bind(&head.active_compaction_id)
        .bind(updated_by_event_seq)
        .bind(head.updated_at_ms)
        .execute(&self.pool)
        .await
        .map_err(sqlx_to_session_store_error)?;
        Ok(())
    }

    async fn commit_compaction_projection(
        &self,
        session_id: &str,
        commit: NewCompactionProjectionCommit,
    ) -> SessionStoreResult<CompactionProjectionCommitResult> {
        validate_commit_session(session_id, &commit)?;
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(sqlx_to_session_store_error)?;
        let committed_at_ms = chrono::Utc::now().timestamp_millis();
        let seq_row = sqlx::query(
            r#"
            UPDATE sessions
            SET last_event_seq = last_event_seq + 1
            WHERE id = $1
            RETURNING last_event_seq
            "#,
        )
        .bind(session_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(sqlx_to_session_store_error)?
        .ok_or_else(|| SessionStoreError::NotFound(format!("session {session_id} 不存在")))?;
        let event_seq_i64: i64 = seq_row
            .try_get("last_event_seq")
            .map_err(sqlx_to_session_store_error)?;
        let event_seq = parse_non_negative_u64(event_seq_i64, "sessions.last_event_seq")?;
        let projection = projection_from_envelope(&commit.completed_event);
        let persisted = PersistedSessionEvent {
            session_id: session_id.to_string(),
            event_seq,
            occurred_at_ms: committed_at_ms,
            committed_at_ms,
            session_update_type: backbone_event_type_name(&commit.completed_event.event)
                .to_string(),
            turn_id: projection.turn_id.clone(),
            entry_index: projection.entry_index,
            tool_call_id: projection.tool_call_id.clone(),
            notification: commit.completed_event.clone(),
        };
        let notification_json = json_string(&persisted.notification, "notification_json")?;
        let event_seq_db = encode_u64_as_i64(event_seq, "session_events.event_seq")?;
        sqlx::query(
            r#"
            INSERT INTO session_events (
                session_id, event_seq, occurred_at_ms, committed_at_ms,
                session_update_type, turn_id, entry_index, tool_call_id, notification_json
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            "#,
        )
        .bind(session_id)
        .bind(event_seq_db)
        .bind(persisted.occurred_at_ms)
        .bind(persisted.committed_at_ms)
        .bind(&persisted.session_update_type)
        .bind(&persisted.turn_id)
        .bind(persisted.entry_index.map(i64::from))
        .bind(&persisted.tool_call_id)
        .bind(notification_json)
        .execute(&mut *tx)
        .await
        .map_err(sqlx_to_session_store_error)?;

        sqlx::query(
            r#"
            UPDATE sessions
            SET
                updated_at = $1,
                last_delivery_status = COALESCE($2, last_delivery_status),
                last_turn_id = COALESCE($3, last_turn_id),
                last_terminal_message = CASE
                    WHEN $4 THEN NULL
                    WHEN $5 IS NOT NULL THEN $6
                    ELSE last_terminal_message
                END,
                executor_session_id = COALESCE($7, executor_session_id)
            WHERE id = $8
            "#,
        )
        .bind(committed_at_ms)
        .bind(&projection.last_delivery_status)
        .bind(&projection.turn_id)
        .bind(projection.clear_terminal_message)
        .bind(&projection.last_terminal_message)
        .bind(&projection.last_terminal_message)
        .bind(&projection.executor_session_id)
        .bind(session_id)
        .execute(&mut *tx)
        .await
        .map_err(sqlx_to_session_store_error)?;

        let mut compaction = commit.compaction;
        compaction.completed_event_seq = Some(event_seq);
        compaction.completed_at_ms = compaction.completed_at_ms.or(Some(committed_at_ms));
        insert_compaction_row(&mut tx, &compaction).await?;

        for segment in &commit.segments {
            insert_projection_segment_row(&mut tx, segment).await?;
        }

        let mut head = commit.head;
        head.head_event_seq = event_seq;
        head.updated_by_event_seq = Some(event_seq);
        head.updated_at_ms = if head.updated_at_ms == 0 {
            committed_at_ms
        } else {
            head.updated_at_ms
        };
        upsert_projection_head_row(&mut tx, &head).await?;

        tx.commit().await.map_err(sqlx_to_session_store_error)?;
        Ok(CompactionProjectionCommitResult {
            event: persisted,
            compaction,
            segments: commit.segments,
            head,
        })
    }
}

#[async_trait::async_trait]
impl SessionLineageStore for PostgresSessionRepository {
    async fn upsert_session_lineage(&self, record: SessionLineageRecord) -> SessionStoreResult<()> {
        if record.child_session_id == record.parent_session_id {
            return Err(SessionStoreError::InvalidInput(
                "session lineage 不能指向自身".to_string(),
            ));
        }
        let cycle = sqlx::query_scalar::<_, i64>(
            r#"
            WITH RECURSIVE parents(session_id) AS (
                SELECT $2::TEXT
                UNION ALL
                SELECT session_lineage.parent_session_id
                FROM session_lineage
                JOIN parents ON session_lineage.child_session_id = parents.session_id
                WHERE session_lineage.child_session_id <> $1
            )
            SELECT 1
            FROM parents
            WHERE session_id = $1
            LIMIT 1
            "#,
        )
        .bind(&record.child_session_id)
        .bind(&record.parent_session_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(sqlx_to_session_store_error)?;
        if cycle.is_some() {
            return Err(SessionStoreError::InvalidInput(
                "session lineage 不能形成环".to_string(),
            ));
        }
        let fork_point_event_seq = encode_optional_u64_as_i64(
            record.fork_point_event_seq,
            "session_lineage.fork_point_event_seq",
        )?;
        let fork_point_ref_json = json_string(
            &record.fork_point_ref_json,
            "session_lineage.fork_point_ref_json",
        )?;
        let metadata_json = json_string(&record.metadata_json, "session_lineage.metadata_json")?;
        sqlx::query(
            r#"
            INSERT INTO session_lineage (
                child_session_id, parent_session_id, relation_kind,
                fork_point_event_seq, fork_point_ref_json, fork_point_compaction_id,
                status, created_at_ms, updated_at_ms, metadata_json
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
            ON CONFLICT(child_session_id) DO UPDATE SET
                parent_session_id = excluded.parent_session_id,
                relation_kind = excluded.relation_kind,
                fork_point_event_seq = excluded.fork_point_event_seq,
                fork_point_ref_json = excluded.fork_point_ref_json,
                fork_point_compaction_id = excluded.fork_point_compaction_id,
                status = excluded.status,
                created_at_ms = excluded.created_at_ms,
                updated_at_ms = excluded.updated_at_ms,
                metadata_json = excluded.metadata_json
            "#,
        )
        .bind(&record.child_session_id)
        .bind(&record.parent_session_id)
        .bind(record.relation_kind.as_str())
        .bind(fork_point_event_seq)
        .bind(fork_point_ref_json)
        .bind(&record.fork_point_compaction_id)
        .bind(record.status.as_str())
        .bind(record.created_at_ms)
        .bind(record.updated_at_ms)
        .bind(metadata_json)
        .execute(&self.pool)
        .await
        .map_err(sqlx_to_session_store_error)?;
        Ok(())
    }

    async fn get_session_lineage(
        &self,
        child_session_id: &str,
    ) -> SessionStoreResult<Option<SessionLineageRecord>> {
        let row = sqlx::query(
            r#"
            SELECT child_session_id, parent_session_id, relation_kind, fork_point_event_seq,
                   fork_point_ref_json, fork_point_compaction_id, status, created_at_ms,
                   updated_at_ms, metadata_json
            FROM session_lineage
            WHERE child_session_id = $1
            "#,
        )
        .bind(child_session_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(sqlx_to_session_store_error)?;
        row.as_ref().map(lineage_from_row).transpose()
    }

    async fn list_session_children(
        &self,
        parent_session_id: &str,
        relation_kind: Option<SessionLineageRelationKind>,
        status: Option<SessionLineageStatus>,
    ) -> SessionStoreResult<Vec<SessionLineageRecord>> {
        let rows = sqlx::query(
            r#"
            SELECT child_session_id, parent_session_id, relation_kind, fork_point_event_seq,
                   fork_point_ref_json, fork_point_compaction_id, status, created_at_ms,
                   updated_at_ms, metadata_json
            FROM session_lineage
            WHERE parent_session_id = $1
              AND ($2 IS NULL OR relation_kind = $2)
              AND ($3 IS NULL OR status = $3)
            ORDER BY created_at_ms ASC, updated_at_ms ASC, child_session_id ASC
            "#,
        )
        .bind(parent_session_id)
        .bind(relation_kind.map(SessionLineageRelationKind::as_str))
        .bind(status.map(SessionLineageStatus::as_str))
        .fetch_all(&self.pool)
        .await
        .map_err(sqlx_to_session_store_error)?;
        rows.iter().map(lineage_from_row).collect()
    }

    async fn list_session_ancestors(
        &self,
        child_session_id: &str,
    ) -> SessionStoreResult<Vec<SessionLineageRecord>> {
        let rows = sqlx::query(
            r#"
            WITH RECURSIVE lineage_path AS (
                SELECT child_session_id, parent_session_id, relation_kind, fork_point_event_seq,
                       fork_point_ref_json, fork_point_compaction_id, status, created_at_ms,
                       updated_at_ms, metadata_json, 0 AS depth
                FROM session_lineage
                WHERE child_session_id = $1
                UNION ALL
                SELECT parent.child_session_id, parent.parent_session_id, parent.relation_kind,
                       parent.fork_point_event_seq, parent.fork_point_ref_json,
                       parent.fork_point_compaction_id, parent.status, parent.created_at_ms,
                       parent.updated_at_ms, parent.metadata_json, lineage_path.depth + 1
                FROM session_lineage parent
                JOIN lineage_path ON parent.child_session_id = lineage_path.parent_session_id
            )
            SELECT child_session_id, parent_session_id, relation_kind, fork_point_event_seq,
                   fork_point_ref_json, fork_point_compaction_id, status, created_at_ms,
                   updated_at_ms, metadata_json
            FROM lineage_path
            ORDER BY depth ASC
            "#,
        )
        .bind(child_session_id)
        .fetch_all(&self.pool)
        .await
        .map_err(sqlx_to_session_store_error)?;
        rows.iter().map(lineage_from_row).collect()
    }

    async fn list_session_descendants(
        &self,
        root_session_id: &str,
        relation_kind: Option<SessionLineageRelationKind>,
        status: Option<SessionLineageStatus>,
    ) -> SessionStoreResult<Vec<SessionLineageRecord>> {
        let rows = sqlx::query(
            r#"
            WITH RECURSIVE lineage_tree AS (
                SELECT child_session_id, parent_session_id, relation_kind, fork_point_event_seq,
                       fork_point_ref_json, fork_point_compaction_id, status, created_at_ms,
                       updated_at_ms, metadata_json, 1 AS depth
                FROM session_lineage
                WHERE parent_session_id = $1
                  AND ($2 IS NULL OR relation_kind = $2)
                  AND ($3 IS NULL OR status = $3)
                UNION ALL
                SELECT child.child_session_id, child.parent_session_id, child.relation_kind,
                       child.fork_point_event_seq, child.fork_point_ref_json,
                       child.fork_point_compaction_id, child.status, child.created_at_ms,
                       child.updated_at_ms, child.metadata_json, lineage_tree.depth + 1
                FROM session_lineage child
                JOIN lineage_tree ON child.parent_session_id = lineage_tree.child_session_id
                WHERE ($2 IS NULL OR child.relation_kind = $2)
                  AND ($3 IS NULL OR child.status = $3)
            )
            SELECT child_session_id, parent_session_id, relation_kind, fork_point_event_seq,
                   fork_point_ref_json, fork_point_compaction_id, status, created_at_ms,
                   updated_at_ms, metadata_json
            FROM lineage_tree
            ORDER BY depth ASC, created_at_ms ASC, updated_at_ms ASC, child_session_id ASC
            "#,
        )
        .bind(root_session_id)
        .bind(relation_kind.map(SessionLineageRelationKind::as_str))
        .bind(status.map(SessionLineageStatus::as_str))
        .fetch_all(&self.pool)
        .await
        .map_err(sqlx_to_session_store_error)?;
        rows.iter().map(lineage_from_row).collect()
    }

    async fn set_session_lineage_status(
        &self,
        child_session_id: &str,
        status: SessionLineageStatus,
        updated_at_ms: i64,
    ) -> SessionStoreResult<()> {
        let result = sqlx::query(
            r#"
            UPDATE session_lineage
            SET status = $1, updated_at_ms = $2
            WHERE child_session_id = $3
            "#,
        )
        .bind(status.as_str())
        .bind(updated_at_ms)
        .bind(child_session_id)
        .execute(&self.pool)
        .await
        .map_err(sqlx_to_session_store_error)?;
        if result.rows_affected() == 0 {
            return Err(SessionStoreError::NotFound(format!(
                "session lineage child {child_session_id} 不存在"
            )));
        }
        Ok(())
    }
}

async fn insert_compaction_row(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    record: &SessionCompactionRecord,
) -> SessionStoreResult<()> {
    let projection_version = encode_u64_as_i64(
        record.projection_version,
        "session_compactions.projection_version",
    )?;
    let start_event_seq = encode_u64_as_i64(
        record.start_event_seq,
        "session_compactions.start_event_seq",
    )?;
    let completed_event_seq = encode_optional_u64_as_i64(
        record.completed_event_seq,
        "session_compactions.completed_event_seq",
    )?;
    let failed_event_seq = encode_optional_u64_as_i64(
        record.failed_event_seq,
        "session_compactions.failed_event_seq",
    )?;
    let base_head_event_seq = encode_optional_u64_as_i64(
        record.base_head_event_seq,
        "session_compactions.base_head_event_seq",
    )?;
    let source_start_event_seq = encode_optional_u64_as_i64(
        record.source_start_event_seq,
        "session_compactions.source_start_event_seq",
    )?;
    let source_end_event_seq = encode_optional_u64_as_i64(
        record.source_end_event_seq,
        "session_compactions.source_end_event_seq",
    )?;
    let first_kept_event_seq = encode_optional_u64_as_i64(
        record.first_kept_event_seq,
        "session_compactions.first_kept_event_seq",
    )?;
    let replacement_projection_json = json_string(
        &record.replacement_projection_json,
        "session_compactions.replacement_projection_json",
    )?;
    let token_stats_json = json_string(
        &record.token_stats_json,
        "session_compactions.token_stats_json",
    )?;
    let diagnostics_json = json_string(
        &record.diagnostics_json,
        "session_compactions.diagnostics_json",
    )?;
    sqlx::query(
        r#"
        INSERT INTO session_compactions (
            id, session_id, projection_kind, projection_version,
            lifecycle_item_id, start_event_seq, completed_event_seq, failed_event_seq,
            status, trigger, reason, phase, strategy, budget_scope,
            base_head_event_seq, source_start_event_seq, source_end_event_seq,
            first_kept_event_seq, summary, replacement_projection_json,
            token_stats_json, diagnostics_json, created_by, created_at_ms, completed_at_ms
        ) VALUES (
            $1, $2, $3, $4,
            $5, $6, $7, $8,
            $9, $10, $11, $12, $13, $14,
            $15, $16, $17,
            $18, $19, $20,
            $21, $22, $23, $24, $25
        )
        "#,
    )
    .bind(&record.id)
    .bind(&record.session_id)
    .bind(&record.projection_kind)
    .bind(projection_version)
    .bind(&record.lifecycle_item_id)
    .bind(start_event_seq)
    .bind(completed_event_seq)
    .bind(failed_event_seq)
    .bind(record.status.as_str())
    .bind(&record.trigger)
    .bind(&record.reason)
    .bind(&record.phase)
    .bind(&record.strategy)
    .bind(&record.budget_scope)
    .bind(base_head_event_seq)
    .bind(source_start_event_seq)
    .bind(source_end_event_seq)
    .bind(first_kept_event_seq)
    .bind(&record.summary)
    .bind(replacement_projection_json)
    .bind(token_stats_json)
    .bind(diagnostics_json)
    .bind(&record.created_by)
    .bind(record.created_at_ms)
    .bind(record.completed_at_ms)
    .execute(&mut **tx)
    .await
    .map_err(sqlx_to_session_store_error)?;
    Ok(())
}

async fn insert_projection_segment_row(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    segment: &SessionProjectionSegmentRecord,
) -> SessionStoreResult<()> {
    let projection_version = encode_u64_as_i64(
        segment.projection_version,
        "session_projection_segments.projection_version",
    )?;
    let sort_order =
        encode_u64_as_i64(segment.sort_order, "session_projection_segments.sort_order")?;
    let source_start_event_seq = encode_optional_u64_as_i64(
        segment.source_start_event_seq,
        "session_projection_segments.source_start_event_seq",
    )?;
    let source_end_event_seq = encode_optional_u64_as_i64(
        segment.source_end_event_seq,
        "session_projection_segments.source_end_event_seq",
    )?;
    let token_estimate = encode_optional_u64_as_i64(
        segment.token_estimate,
        "session_projection_segments.token_estimate",
    )?;
    let source_refs_json = json_string(
        &segment.source_refs_json,
        "session_projection_segments.source_refs_json",
    )?;
    let content_json = json_string(
        &segment.content_json,
        "session_projection_segments.content_json",
    )?;
    sqlx::query(
        r#"
        INSERT INTO session_projection_segments (
            id, session_id, projection_kind, projection_version, sort_order,
            segment_type, origin, synthetic, source_start_event_seq, source_end_event_seq,
            source_refs_json, generated_by_compaction_id, content_json, token_estimate,
            created_at_ms
        ) VALUES (
            $1, $2, $3, $4, $5,
            $6, $7, $8, $9, $10,
            $11, $12, $13, $14,
            $15
        )
        "#,
    )
    .bind(&segment.id)
    .bind(&segment.session_id)
    .bind(&segment.projection_kind)
    .bind(projection_version)
    .bind(sort_order)
    .bind(&segment.segment_type)
    .bind(&segment.origin)
    .bind(segment.synthetic)
    .bind(source_start_event_seq)
    .bind(source_end_event_seq)
    .bind(source_refs_json)
    .bind(&segment.generated_by_compaction_id)
    .bind(content_json)
    .bind(token_estimate)
    .bind(segment.created_at_ms)
    .execute(&mut **tx)
    .await
    .map_err(sqlx_to_session_store_error)?;
    Ok(())
}

async fn upsert_projection_head_row(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    head: &SessionProjectionHeadRecord,
) -> SessionStoreResult<()> {
    let projection_version = encode_u64_as_i64(
        head.projection_version,
        "session_projection_heads.projection_version",
    )?;
    let head_event_seq = encode_u64_as_i64(
        head.head_event_seq,
        "session_projection_heads.head_event_seq",
    )?;
    let updated_by_event_seq = encode_optional_u64_as_i64(
        head.updated_by_event_seq,
        "session_projection_heads.updated_by_event_seq",
    )?;
    sqlx::query(
        r#"
        INSERT INTO session_projection_heads (
            session_id, projection_kind, projection_version, head_event_seq,
            active_compaction_id, updated_by_event_seq, updated_at_ms
        ) VALUES ($1, $2, $3, $4, $5, $6, $7)
        ON CONFLICT(session_id, projection_kind) DO UPDATE SET
            projection_version = excluded.projection_version,
            head_event_seq = excluded.head_event_seq,
            active_compaction_id = excluded.active_compaction_id,
            updated_by_event_seq = excluded.updated_by_event_seq,
            updated_at_ms = excluded.updated_at_ms
        "#,
    )
    .bind(&head.session_id)
    .bind(&head.projection_kind)
    .bind(projection_version)
    .bind(head_event_seq)
    .bind(&head.active_compaction_id)
    .bind(updated_by_event_seq)
    .bind(head.updated_at_ms)
    .execute(&mut **tx)
    .await
    .map_err(sqlx_to_session_store_error)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persistence::postgres::test_pg_pool;
    use agentdash_agent_protocol::codex_app_server_protocol as codex;
    use agentdash_agent_protocol::{
        BackboneEvent, ItemCompletedNotification, PlatformEvent, SourceInfo, TraceInfo,
    };
    use agentdash_spi::session_persistence::{
        ExecutionStatus, SessionCompactionRecord, SessionCompactionStatus,
        SessionProjectionHeadRecord, SessionProjectionSegmentRecord, TitleSource,
    };
    use chrono::Utc;

    fn turn_terminal_envelope(
        session_id: &str,
        turn_id: &str,
        terminal_type: &str,
        message: &str,
    ) -> BackboneEnvelope {
        let status = match terminal_type {
            "turn_completed" => codex::TurnStatus::Completed,
            "turn_failed" => codex::TurnStatus::Failed,
            "turn_interrupted" => codex::TurnStatus::Interrupted,
            _ => codex::TurnStatus::Completed,
        };
        let error = if terminal_type == "turn_failed" {
            Some(codex::TurnError {
                message: message.to_string(),
                codex_error_info: None,
                additional_details: None,
            })
        } else {
            None
        };
        BackboneEnvelope {
            event: BackboneEvent::TurnCompleted(codex::TurnCompletedNotification {
                thread_id: session_id.to_string(),
                turn: codex::Turn {
                    id: turn_id.to_string(),
                    items: Vec::new(),
                    items_view: codex::TurnItemsView::NotLoaded,
                    status,
                    error,
                    started_at: None,
                    completed_at: Some(Utc::now().timestamp()),
                    duration_ms: None,
                },
            }),
            session_id: session_id.to_string(),
            source: SourceInfo {
                connector_id: "test".to_string(),
                connector_type: "test".to_string(),
                executor_id: None,
            },
            trace: TraceInfo {
                turn_id: Some(turn_id.to_string()),
                entry_index: None,
            },
            observed_at: Utc::now(),
        }
    }

    fn session_meta(id: &str) -> SessionMeta {
        SessionMeta {
            id: id.to_string(),
            title: "测试".to_string(),
            title_source: TitleSource::Auto,
            project_id: None,
            created_at: 1,
            updated_at: 1,
            last_event_seq: 0,
            last_delivery_status: ExecutionStatus::Idle,
            last_turn_id: None,
            last_terminal_message: None,
            executor_config: None,
            executor_session_id: None,

            tab_layout: None,
        }
    }

    fn context_compaction_completed_envelope(
        session_id: &str,
        turn_id: &str,
        item_id: &str,
    ) -> BackboneEnvelope {
        BackboneEnvelope::new(
            BackboneEvent::ItemCompleted(ItemCompletedNotification::new(
                codex::ThreadItem::ContextCompaction {
                    id: item_id.to_string(),
                },
                session_id.to_string(),
                turn_id.to_string(),
            )),
            session_id,
            SourceInfo {
                connector_id: "test".to_string(),
                connector_type: "test".to_string(),
                executor_id: None,
            },
        )
        .with_trace(TraceInfo {
            turn_id: Some(turn_id.to_string()),
            entry_index: None,
        })
    }

    fn compaction_commit(
        session_id: &str,
        compaction_id: &str,
        segment_id: &str,
        projection_version: u64,
    ) -> NewCompactionProjectionCommit {
        let now = Utc::now().timestamp_millis();
        NewCompactionProjectionCommit {
            completed_event: context_compaction_completed_envelope(
                session_id,
                "turn-compact",
                "compact-item-1",
            ),
            compaction: SessionCompactionRecord {
                id: compaction_id.to_string(),
                session_id: session_id.to_string(),
                projection_kind: "model_context".to_string(),
                projection_version,
                lifecycle_item_id: "compact-item-1".to_string(),
                start_event_seq: 1,
                completed_event_seq: None,
                failed_event_seq: None,
                status: SessionCompactionStatus::ProjectionCommitted,
                trigger: "auto".to_string(),
                reason: Some("token_pressure".to_string()),
                phase: Some("pre_provider".to_string()),
                strategy: "summary_prefix".to_string(),
                budget_scope: Some("model_context".to_string()),
                base_head_event_seq: Some(0),
                source_start_event_seq: Some(1),
                source_end_event_seq: Some(8),
                first_kept_event_seq: Some(9),
                summary: "压缩摘要".to_string(),
                replacement_projection_json: serde_json::json!({
                    "segments": [segment_id]
                }),
                token_stats_json: serde_json::json!({
                    "before": 48000,
                    "after": 12000
                }),
                diagnostics_json: serde_json::json!({}),
                created_by: Some("agent".to_string()),
                created_at_ms: now,
                completed_at_ms: None,
            },
            segments: vec![SessionProjectionSegmentRecord {
                id: segment_id.to_string(),
                session_id: session_id.to_string(),
                projection_kind: "model_context".to_string(),
                projection_version,
                sort_order: 0,
                segment_type: "summary_chunk".to_string(),
                origin: "projection".to_string(),
                synthetic: true,
                source_start_event_seq: Some(1),
                source_end_event_seq: Some(8),
                source_refs_json: serde_json::json!([]),
                generated_by_compaction_id: Some(compaction_id.to_string()),
                content_json: serde_json::json!({
                    "role": "system",
                    "content": "压缩摘要"
                }),
                token_estimate: Some(256),
                created_at_ms: now,
            }],
            head: SessionProjectionHeadRecord {
                session_id: session_id.to_string(),
                projection_kind: "model_context".to_string(),
                projection_version,
                head_event_seq: 9,
                active_compaction_id: Some(compaction_id.to_string()),
                updated_by_event_seq: None,
                updated_at_ms: 0,
            },
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
    async fn append_event_assigns_monotonic_event_seq() {
        let Some(pool) = test_pg_pool("session_repository").await else {
            return;
        };
        let repo = PostgresSessionRepository::new(pool);
        repo.initialize().await.expect("应能初始化 session 表");

        let meta = SessionMeta {
            id: "sess-1".to_string(),
            title: "测试".to_string(),
            title_source: TitleSource::Auto,
            project_id: None,
            created_at: 1,
            updated_at: 1,
            last_event_seq: 0,
            last_delivery_status: ExecutionStatus::Idle,
            last_turn_id: None,
            last_terminal_message: None,
            executor_config: None,
            executor_session_id: None,

            tab_layout: None,
        };
        repo.create_session(&meta).await.expect("应能创建 session");

        let envelope = BackboneEnvelope::new(
            BackboneEvent::Platform(PlatformEvent::SessionMetaUpdate {
                key: "test".to_string(),
                value: serde_json::Value::Null,
            }),
            "sess-1",
            SourceInfo {
                connector_id: "test".to_string(),
                connector_type: "test".to_string(),
                executor_id: None,
            },
        );
        let first = repo
            .append_event("sess-1", &envelope)
            .await
            .expect("应能写入第一条事件");
        let second = repo
            .append_event("sess-1", &envelope)
            .await
            .expect("应能写入第二条事件");

        assert_eq!(first.event_seq, 1);
        assert_eq!(second.event_seq, 2);
        assert_eq!(
            repo.get_session_meta("sess-1")
                .await
                .expect("应能读取 session meta")
                .expect("session 应存在")
                .last_event_seq,
            2
        );
    }

    #[tokio::test]
    async fn stale_save_session_meta_does_not_roll_back_event_projection() {
        let Some(pool) = test_pg_pool("session_repository").await else {
            return;
        };
        let repo = PostgresSessionRepository::new(pool);
        repo.initialize().await.expect("应能初始化 session 表");
        let session_id = format!(
            "sess-stale-{}",
            chrono::Utc::now()
                .timestamp_nanos_opt()
                .expect("当前时间应可表示为纳秒时间戳")
        );

        let meta = SessionMeta {
            id: session_id.clone(),
            title: "测试".to_string(),
            title_source: TitleSource::Auto,
            project_id: None,
            created_at: 1,
            updated_at: 1,
            last_event_seq: 0,
            last_delivery_status: ExecutionStatus::Idle,
            last_turn_id: None,
            last_terminal_message: None,
            executor_config: None,
            executor_session_id: None,

            tab_layout: None,
        };
        repo.create_session(&meta).await.expect("应能创建 session");

        let mut stale = repo
            .get_session_meta(&session_id)
            .await
            .expect("应能读取 session meta")
            .expect("session 应存在");
        stale.updated_at = 10;
        stale.last_delivery_status = ExecutionStatus::Running;
        stale.last_turn_id = Some("t-old".to_string());
        stale.executor_session_id = Some("exec-1".to_string());
        stale.tab_layout = Some(serde_json::json!({
            "tabs": [{"type_id": "session", "uri": "session://main", "title": "Session", "pinned": true}],
            "active_tab_uri": "session://main"
        }));

        let terminal = turn_terminal_envelope(&session_id, "t-new", "turn_completed", "done");
        repo.append_event(&session_id, &terminal)
            .await
            .expect("应能写入终态事件");

        repo.save_session_meta(&stale)
            .await
            .expect("旧快照回写仍应成功");

        let merged = repo
            .get_session_meta(&session_id)
            .await
            .expect("应能再次读取 session meta")
            .expect("session 应存在");

        assert_eq!(merged.last_event_seq, 1);
        assert_eq!(merged.last_delivery_status, ExecutionStatus::Completed);
        assert_eq!(merged.last_turn_id.as_deref(), Some("t-new"));
        assert_eq!(merged.last_terminal_message.as_deref(), Some("done"));
        assert_eq!(merged.executor_session_id.as_deref(), Some("exec-1"));
        assert_eq!(
            merged
                .tab_layout
                .as_ref()
                .and_then(|value| value.get("active_tab_uri"))
                .and_then(|value| value.as_str()),
            Some("session://main")
        );
    }

    #[tokio::test]
    async fn compaction_projection_commit_persists_checkpoint_segments_and_head() {
        let Some(pool) = test_pg_pool("session_compaction_projection").await else {
            return;
        };
        let repo = PostgresSessionRepository::new(pool);
        repo.initialize().await.expect("应能初始化 session 表");
        let session_id = format!(
            "sess-compact-{}",
            chrono::Utc::now()
                .timestamp_nanos_opt()
                .expect("当前时间应可表示为纳秒时间戳")
        );
        repo.create_session(&session_meta(&session_id))
            .await
            .expect("应能创建 session");
        let compaction_id = format!("{session_id}-compaction-1");
        let segment_id = format!("{session_id}-segment-1");

        let result = repo
            .commit_compaction_projection(
                &session_id,
                compaction_commit(&session_id, &compaction_id, &segment_id, 1),
            )
            .await
            .expect("应能原子提交 compaction projection");

        assert_eq!(result.event.event_seq, 1);
        assert_eq!(result.compaction.completed_event_seq, Some(1));
        assert_eq!(result.head.updated_by_event_seq, Some(1));

        let stored = repo
            .get_compaction(&session_id, &compaction_id)
            .await
            .expect("应能查询 compaction")
            .expect("compaction 应存在");
        assert_eq!(stored.summary, "压缩摘要");
        assert_eq!(stored.status, SessionCompactionStatus::ProjectionCommitted);

        let segments = repo
            .list_projection_segments(&session_id, "model_context", 1)
            .await
            .expect("应能查询 projection segments");
        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0].segment_type, "summary_chunk");

        let head = repo
            .read_projection_head(&session_id, "model_context")
            .await
            .expect("应能查询 projection head")
            .expect("projection head 应存在");
        assert_eq!(
            head.active_compaction_id.as_deref(),
            Some(compaction_id.as_str())
        );
        assert_eq!(head.projection_version, 1);
        assert_eq!(head.head_event_seq, result.event.event_seq);
        assert_eq!(head.updated_by_event_seq, Some(result.event.event_seq));
    }

    #[tokio::test]
    async fn session_lineage_queries_are_stable_and_filterable() {
        let Some(pool) = test_pg_pool("session_lineage").await else {
            return;
        };
        let repo = PostgresSessionRepository::new(pool);
        repo.initialize().await.expect("应能初始化 session 表");
        let suffix = chrono::Utc::now()
            .timestamp_nanos_opt()
            .expect("当前时间应可表示为纳秒时间戳");
        let root = format!("root-{suffix}");
        let child_a = format!("child-a-{suffix}");
        let child_b = format!("child-b-{suffix}");
        let grand = format!("grand-{suffix}");
        for id in [&root, &child_a, &child_b, &grand] {
            repo.create_session(&session_meta(id))
                .await
                .expect("应能创建 session");
        }

        repo.upsert_session_lineage(lineage_record(
            &child_a,
            &root,
            SessionLineageRelationKind::Fork,
            SessionLineageStatus::Open,
            20,
        ))
        .await
        .expect("应能写入 fork edge");
        repo.upsert_session_lineage(lineage_record(
            &child_b,
            &root,
            SessionLineageRelationKind::Companion,
            SessionLineageStatus::Open,
            10,
        ))
        .await
        .expect("应能写入 companion edge");
        repo.upsert_session_lineage(lineage_record(
            &grand,
            &child_b,
            SessionLineageRelationKind::Fork,
            SessionLineageStatus::Open,
            30,
        ))
        .await
        .expect("应能写入 grand edge");

        let children = repo
            .list_session_children(&root, None, Some(SessionLineageStatus::Open))
            .await
            .expect("应能查询 direct children");
        assert_eq!(
            children
                .iter()
                .map(|edge| edge.child_session_id.as_str())
                .collect::<Vec<_>>(),
            vec![child_b.as_str(), child_a.as_str()]
        );

        let fork_children = repo
            .list_session_children(
                &root,
                Some(SessionLineageRelationKind::Fork),
                Some(SessionLineageStatus::Open),
            )
            .await
            .expect("应能按 relation 查询 children");
        assert_eq!(fork_children.len(), 1);
        assert_eq!(fork_children[0].child_session_id.as_str(), child_a.as_str());

        let ancestors = repo
            .list_session_ancestors(&grand)
            .await
            .expect("应能查询 ancestors");
        assert_eq!(
            ancestors
                .iter()
                .map(|edge| edge.child_session_id.as_str())
                .collect::<Vec<_>>(),
            vec![grand.as_str(), child_b.as_str()]
        );

        let descendants = repo
            .list_session_descendants(&root, None, Some(SessionLineageStatus::Open))
            .await
            .expect("应能查询 descendants");
        assert_eq!(
            descendants
                .iter()
                .map(|edge| edge.child_session_id.as_str())
                .collect::<Vec<_>>(),
            vec![child_b.as_str(), child_a.as_str(), grand.as_str()]
        );

        repo.set_session_lineage_status(&child_b, SessionLineageStatus::Closed, 40)
            .await
            .expect("应能关闭 lineage edge");
        let open_descendants = repo
            .list_session_descendants(&root, None, Some(SessionLineageStatus::Open))
            .await
            .expect("应能查询 open descendants");
        assert_eq!(
            open_descendants
                .iter()
                .map(|edge| edge.child_session_id.as_str())
                .collect::<Vec<_>>(),
            vec![child_a.as_str()]
        );
    }
}
