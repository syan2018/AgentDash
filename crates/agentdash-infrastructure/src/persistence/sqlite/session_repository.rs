use std::io;

use agentdash_agent_protocol::BackboneEnvelope;
use agentdash_spi::session_persistence::{
    CompactionProjectionCommitResult, NewCompactionProjectionCommit, PersistedSessionEvent,
    RuntimeCommandRecord, RuntimeCommandStatus, SessionCompactionRecord, SessionCompactionStore,
    SessionEventBacklog, SessionEventPage, SessionEventStore, SessionLineageRecord,
    SessionLineageRelationKind, SessionLineageStatus, SessionLineageStore, SessionMeta,
    SessionMetaStore, SessionProjectionHeadRecord, SessionProjectionSegmentRecord,
    SessionProjectionStore, SessionRuntimeCommandStore, SessionTerminalEffectStore,
    TerminalEffectRecord, TerminalEffectStatus,
};
use agentdash_spi::session_persistence::{NewTerminalEffectRecord, PendingCapabilityStateTransition};
use sqlx::{Row, SqlitePool};

use crate::persistence::session_core::{
    backbone_event_type_name, bootstrap_state_to_str, compaction_from_row, encode_optional_u64_as_i64,
    encode_u64_as_i64, json_string, lineage_from_row, map_meta_row, optional_json_string,
    parse_non_negative_u64, persisted_event_from_row, projection_from_envelope,
    projection_head_from_row, projection_segment_from_row, runtime_command_from_row, sqlx_to_io,
    terminal_effect_from_row, title_source_to_str, validate_commit_session,
};

pub struct SqliteSessionRepository {
    pool: SqlitePool,
}

impl SqliteSessionRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn initialize(&self) -> io::Result<()> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS sessions (
                id TEXT PRIMARY KEY,
                title TEXT NOT NULL,
                title_source TEXT NOT NULL DEFAULT 'auto',
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL,
                last_event_seq INTEGER NOT NULL DEFAULT 0,
                last_execution_status TEXT NOT NULL DEFAULT 'idle',
                last_turn_id TEXT,
                last_terminal_message TEXT,
                executor_config_json TEXT,
                executor_session_id TEXT,
                companion_context_json TEXT,
                tab_layout_json TEXT,
                visible_canvas_mount_ids_json TEXT NOT NULL DEFAULT '[]',
                bootstrap_state TEXT NOT NULL DEFAULT 'plain'
            );

            CREATE TABLE IF NOT EXISTS session_events (
                session_id TEXT NOT NULL,
                event_seq INTEGER NOT NULL,
                occurred_at_ms INTEGER NOT NULL,
                committed_at_ms INTEGER NOT NULL,
                session_update_type TEXT NOT NULL,
                turn_id TEXT,
                entry_index INTEGER,
                tool_call_id TEXT,
                notification_json TEXT NOT NULL,
                PRIMARY KEY (session_id, event_seq),
                FOREIGN KEY (session_id) REFERENCES sessions(id) ON DELETE CASCADE
            );

            CREATE INDEX IF NOT EXISTS idx_session_events_session_seq
                ON session_events(session_id, event_seq);
            CREATE INDEX IF NOT EXISTS idx_session_events_session_turn
                ON session_events(session_id, turn_id);
            CREATE INDEX IF NOT EXISTS idx_session_events_session_tool
                ON session_events(session_id, tool_call_id);

            CREATE TABLE IF NOT EXISTS session_terminal_effects (
                id TEXT PRIMARY KEY,
                session_id TEXT NOT NULL,
                turn_id TEXT NOT NULL,
                terminal_event_seq INTEGER NOT NULL,
                effect_type TEXT NOT NULL,
                payload_json TEXT NOT NULL,
                status TEXT NOT NULL,
                attempt_count INTEGER NOT NULL DEFAULT 0,
                created_at_ms INTEGER NOT NULL,
                updated_at_ms INTEGER NOT NULL,
                last_error TEXT,
                FOREIGN KEY (session_id) REFERENCES sessions(id) ON DELETE CASCADE
            );

            CREATE INDEX IF NOT EXISTS idx_session_terminal_effects_status_updated
                ON session_terminal_effects(status, updated_at_ms);
            CREATE INDEX IF NOT EXISTS idx_session_terminal_effects_session_turn
                ON session_terminal_effects(session_id, turn_id);
            CREATE INDEX IF NOT EXISTS idx_session_terminal_effects_terminal_event
                ON session_terminal_effects(session_id, terminal_event_seq);

            CREATE TABLE IF NOT EXISTS session_runtime_commands (
                id TEXT PRIMARY KEY,
                session_id TEXT NOT NULL,
                transition_id TEXT NOT NULL,
                phase_node TEXT NOT NULL,
                status TEXT NOT NULL,
                payload_json TEXT NOT NULL,
                created_at_ms INTEGER NOT NULL,
                updated_at_ms INTEGER NOT NULL,
                applied_at_ms INTEGER,
                failed_at_ms INTEGER,
                last_error TEXT,
                FOREIGN KEY (session_id) REFERENCES sessions(id) ON DELETE CASCADE
            );

            CREATE INDEX IF NOT EXISTS idx_session_runtime_commands_status_updated
                ON session_runtime_commands(status, updated_at_ms);
            CREATE INDEX IF NOT EXISTS idx_session_runtime_commands_session_status
                ON session_runtime_commands(session_id, status);

            CREATE TABLE IF NOT EXISTS session_compactions (
                id TEXT PRIMARY KEY,
                session_id TEXT NOT NULL,
                projection_kind TEXT NOT NULL,
                projection_version INTEGER NOT NULL,
                lifecycle_item_id TEXT NOT NULL,
                start_event_seq INTEGER NOT NULL,
                completed_event_seq INTEGER,
                failed_event_seq INTEGER,
                status TEXT NOT NULL,
                trigger TEXT NOT NULL,
                reason TEXT,
                phase TEXT,
                strategy TEXT NOT NULL,
                budget_scope TEXT,
                base_head_event_seq INTEGER,
                source_start_event_seq INTEGER,
                source_end_event_seq INTEGER,
                first_kept_event_seq INTEGER,
                summary TEXT NOT NULL DEFAULT '',
                replacement_projection_json TEXT NOT NULL DEFAULT '{}',
                token_stats_json TEXT NOT NULL DEFAULT '{}',
                diagnostics_json TEXT NOT NULL DEFAULT '{}',
                created_by TEXT,
                created_at_ms INTEGER NOT NULL,
                completed_at_ms INTEGER,
                FOREIGN KEY (session_id) REFERENCES sessions(id) ON DELETE CASCADE
            );

            CREATE INDEX IF NOT EXISTS idx_session_compactions_session_kind_status
                ON session_compactions(session_id, projection_kind, status, projection_version);
            CREATE INDEX IF NOT EXISTS idx_session_compactions_lifecycle_item
                ON session_compactions(session_id, lifecycle_item_id);
            CREATE INDEX IF NOT EXISTS idx_session_compactions_source_range
                ON session_compactions(session_id, source_start_event_seq, source_end_event_seq);

            CREATE TABLE IF NOT EXISTS session_projection_segments (
                id TEXT PRIMARY KEY,
                session_id TEXT NOT NULL,
                projection_kind TEXT NOT NULL,
                projection_version INTEGER NOT NULL,
                sort_order INTEGER NOT NULL,
                segment_type TEXT NOT NULL,
                origin TEXT NOT NULL,
                synthetic INTEGER NOT NULL DEFAULT 0,
                source_start_event_seq INTEGER,
                source_end_event_seq INTEGER,
                source_refs_json TEXT NOT NULL DEFAULT '[]',
                generated_by_compaction_id TEXT,
                content_json TEXT NOT NULL,
                token_estimate INTEGER,
                created_at_ms INTEGER NOT NULL,
                UNIQUE(session_id, projection_kind, projection_version, sort_order),
                FOREIGN KEY (session_id) REFERENCES sessions(id) ON DELETE CASCADE,
                FOREIGN KEY (generated_by_compaction_id) REFERENCES session_compactions(id) ON DELETE SET NULL
            );

            CREATE INDEX IF NOT EXISTS idx_session_projection_segments_projection
                ON session_projection_segments(session_id, projection_kind, projection_version, sort_order);
            CREATE INDEX IF NOT EXISTS idx_session_projection_segments_source_range
                ON session_projection_segments(session_id, source_start_event_seq, source_end_event_seq);

            CREATE TABLE IF NOT EXISTS session_projection_heads (
                session_id TEXT NOT NULL,
                projection_kind TEXT NOT NULL,
                projection_version INTEGER NOT NULL,
                head_event_seq INTEGER NOT NULL,
                active_compaction_id TEXT,
                updated_by_event_seq INTEGER,
                updated_at_ms INTEGER NOT NULL,
                PRIMARY KEY (session_id, projection_kind),
                FOREIGN KEY (session_id) REFERENCES sessions(id) ON DELETE CASCADE,
                FOREIGN KEY (active_compaction_id) REFERENCES session_compactions(id) ON DELETE SET NULL
            );

            CREATE INDEX IF NOT EXISTS idx_session_projection_heads_active_compaction
                ON session_projection_heads(session_id, active_compaction_id);

            CREATE TABLE IF NOT EXISTS session_lineage (
                child_session_id TEXT PRIMARY KEY,
                parent_session_id TEXT NOT NULL,
                relation_kind TEXT NOT NULL,
                fork_point_event_seq INTEGER,
                fork_point_ref_json TEXT NOT NULL DEFAULT '{}',
                fork_point_compaction_id TEXT,
                status TEXT NOT NULL,
                created_at_ms INTEGER NOT NULL,
                updated_at_ms INTEGER NOT NULL,
                metadata_json TEXT NOT NULL DEFAULT '{}',
                CHECK (child_session_id <> parent_session_id),
                FOREIGN KEY (child_session_id) REFERENCES sessions(id) ON DELETE CASCADE,
                FOREIGN KEY (parent_session_id) REFERENCES sessions(id) ON DELETE CASCADE,
                FOREIGN KEY (fork_point_compaction_id) REFERENCES session_compactions(id) ON DELETE SET NULL
            );

            CREATE INDEX IF NOT EXISTS idx_session_lineage_parent_status_kind
                ON session_lineage(parent_session_id, status, relation_kind, created_at_ms, child_session_id);
            CREATE INDEX IF NOT EXISTS idx_session_lineage_fork_point
                ON session_lineage(parent_session_id, fork_point_event_seq, fork_point_compaction_id);
            "#,
        )
        .execute(&self.pool)
        .await
        .map_err(sqlx_to_io)?;

        let _ = sqlx::query(
            "ALTER TABLE sessions ADD COLUMN bootstrap_state TEXT NOT NULL DEFAULT 'plain'",
        )
        .execute(&self.pool)
        .await;
        let _ = sqlx::query(
            "ALTER TABLE sessions ADD COLUMN title_source TEXT NOT NULL DEFAULT 'auto'",
        )
        .execute(&self.pool)
        .await;
        let _ = sqlx::query("ALTER TABLE sessions ADD COLUMN tab_layout_json TEXT")
            .execute(&self.pool)
            .await;
        sqlx::query(
            "UPDATE session_runtime_commands SET status = 'requested' WHERE status = 'pending'",
        )
        .execute(&self.pool)
        .await
        .map_err(sqlx_to_io)?;
        self.backfill_legacy_session_event_payloads().await?;
        Ok(())
    }

    async fn backfill_legacy_session_event_payloads(&self) -> io::Result<()> {
        sqlx::query(
            r#"
            UPDATE session_events
            SET notification_json = json_set(
                notification_json,
                '$.event.payload.startedAtMs',
                occurred_at_ms
            )
            WHERE session_update_type = 'item_started'
              AND json_type(notification_json, '$.event.payload.startedAtMs') IS NULL
            "#,
        )
        .execute(&self.pool)
        .await
        .map_err(sqlx_to_io)?;

        sqlx::query(
            r#"
            UPDATE session_events
            SET notification_json = json_set(
                notification_json,
                '$.event.payload.completedAtMs',
                occurred_at_ms
            )
            WHERE session_update_type = 'item_completed'
              AND json_type(notification_json, '$.event.payload.completedAtMs') IS NULL
            "#,
        )
        .execute(&self.pool)
        .await
        .map_err(sqlx_to_io)?;

        sqlx::query(
            r#"
            UPDATE session_events
            SET notification_json = json_set(
                notification_json,
                '$.event.payload.tokenUsage.context',
                json_object(
                    'providerContextTokens',
                    max(coalesce(json_extract(notification_json, '$.event.payload.tokenUsage.last.totalTokens'), 0), 0),
                    'pendingEstimateTokens',
                    0,
                    'currentContextTokens',
                    max(coalesce(json_extract(notification_json, '$.event.payload.tokenUsage.last.totalTokens'), 0), 0),
                    'cumulativeTotalTokens',
                    max(coalesce(json_extract(notification_json, '$.event.payload.tokenUsage.total.totalTokens'), 0), 0),
                    'modelContextWindow',
                    json_extract(notification_json, '$.event.payload.tokenUsage.modelContextWindow'),
                    'effectiveContextWindow',
                    json_extract(notification_json, '$.event.payload.tokenUsage.modelContextWindow'),
                    'reserveTokens',
                    0,
                    'source',
                    'provider'
                )
            )
            WHERE session_update_type = 'token_usage_updated'
              AND json_type(notification_json, '$.event.payload.tokenUsage.context') IS NULL
            "#,
        )
        .execute(&self.pool)
        .await
        .map_err(sqlx_to_io)?;

        Ok(())
    }

    async fn update_terminal_effect_status(
        &self,
        effect_id: uuid::Uuid,
        status: TerminalEffectStatus,
        updated_at_ms: i64,
        increment_attempt: bool,
        last_error: Option<String>,
    ) -> io::Result<()> {
        let result = sqlx::query(
            r#"
            UPDATE session_terminal_effects
            SET status = ?,
                attempt_count = attempt_count + ?,
                updated_at_ms = ?,
                last_error = ?
            WHERE id = ?
            "#,
        )
        .bind(status.as_str())
        .bind(if increment_attempt { 1_i64 } else { 0_i64 })
        .bind(updated_at_ms)
        .bind(last_error)
        .bind(effect_id.to_string())
        .execute(&self.pool)
        .await
        .map_err(sqlx_to_io)?;
        if result.rows_affected() == 0 {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("terminal effect {effect_id} 不存在"),
            ));
        }
        Ok(())
    }

    async fn update_runtime_commands_status(
        &self,
        command_ids: &[uuid::Uuid],
        status: RuntimeCommandStatus,
        error: Option<String>,
    ) -> io::Result<()> {
        if command_ids.is_empty() {
            return Ok(());
        }
        let now = chrono::Utc::now().timestamp_millis();
        for command_id in command_ids {
            let (applied_at_ms, failed_at_ms, last_error) = match status {
                RuntimeCommandStatus::Applied => (Some(now), None, None),
                RuntimeCommandStatus::Failed => (None, Some(now), error.clone()),
                RuntimeCommandStatus::Requested => (None, None, None),
            };
            let result = sqlx::query(
                r#"
                UPDATE session_runtime_commands
                SET status = ?,
                    updated_at_ms = ?,
                    applied_at_ms = COALESCE(?, applied_at_ms),
                    failed_at_ms = COALESCE(?, failed_at_ms),
                    last_error = ?
                WHERE id = ?
                "#,
            )
            .bind(status.as_str())
            .bind(now)
            .bind(applied_at_ms)
            .bind(failed_at_ms)
            .bind(last_error)
            .bind(command_id.to_string())
            .execute(&self.pool)
            .await
            .map_err(sqlx_to_io)?;
            if result.rows_affected() == 0 {
                return Err(io::Error::new(
                    io::ErrorKind::NotFound,
                    format!("runtime command {command_id} 不存在"),
                ));
            }
        }
        Ok(())
    }

    async fn require_snapshot_seq(&self, session_id: &str) -> io::Result<u64> {
        self.get_session_meta(session_id)
            .await?
            .map(|meta| meta.last_event_seq)
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::NotFound,
                    format!("session {session_id} 不存在"),
                )
            })
    }
}

#[async_trait::async_trait]
impl SessionMetaStore for SqliteSessionRepository {
    async fn create_session(&self, meta: &SessionMeta) -> io::Result<()> {
        let last_event_seq = encode_u64_as_i64(meta.last_event_seq, "sessions.last_event_seq")?;
        let executor_config_json =
            optional_json_string(meta.executor_config.as_ref(), "executor_config_json")?;
        let companion_context_json =
            optional_json_string(meta.companion_context.as_ref(), "companion_context_json")?;
        let tab_layout_json = optional_json_string(meta.tab_layout.as_ref(), "tab_layout_json")?;
        let visible_canvas_mount_ids_json = json_string(
            &meta.visible_canvas_mount_ids,
            "visible_canvas_mount_ids_json",
        )?;
        sqlx::query(
            r#"
            INSERT INTO sessions (
                id, title, title_source, created_at, updated_at, last_event_seq, last_execution_status,
                last_turn_id, last_terminal_message, executor_config_json,
                executor_session_id, companion_context_json, tab_layout_json, visible_canvas_mount_ids_json,
                bootstrap_state
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&meta.id)
        .bind(&meta.title)
        .bind(title_source_to_str(meta.title_source))
        .bind(meta.created_at)
        .bind(meta.updated_at)
        .bind(last_event_seq)
        .bind(meta.last_execution_status.to_string())
        .bind(&meta.last_turn_id)
        .bind(&meta.last_terminal_message)
        .bind(executor_config_json)
        .bind(&meta.executor_session_id)
        .bind(companion_context_json)
        .bind(tab_layout_json)
        .bind(visible_canvas_mount_ids_json)
        .bind(bootstrap_state_to_str(meta.bootstrap_state))
        .execute(&self.pool)
        .await
        .map_err(sqlx_to_io)?;
        Ok(())
    }

    async fn get_session_meta(&self, session_id: &str) -> io::Result<Option<SessionMeta>> {
        let row = sqlx::query(
            r#"
            SELECT id, title, title_source, created_at, updated_at, last_event_seq, last_execution_status,
                   last_turn_id, last_terminal_message, executor_config_json,
                   executor_session_id, companion_context_json, tab_layout_json, visible_canvas_mount_ids_json,
                   bootstrap_state
            FROM sessions
            WHERE id = ?
            "#,
        )
        .bind(session_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(sqlx_to_io)?;
        row.as_ref().map(map_meta_row).transpose()
    }

    async fn list_sessions(&self) -> io::Result<Vec<SessionMeta>> {
        let rows = sqlx::query(
            r#"
            SELECT id, title, title_source, created_at, updated_at, last_event_seq, last_execution_status,
                   last_turn_id, last_terminal_message, executor_config_json,
                   executor_session_id, companion_context_json, tab_layout_json, visible_canvas_mount_ids_json,
                   bootstrap_state
            FROM sessions
            ORDER BY updated_at DESC
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(sqlx_to_io)?;
        rows.iter().map(map_meta_row).collect()
    }

    async fn save_session_meta(&self, meta: &SessionMeta) -> io::Result<()> {
        let last_event_seq = encode_u64_as_i64(meta.last_event_seq, "sessions.last_event_seq")?;
        let executor_config_json =
            optional_json_string(meta.executor_config.as_ref(), "executor_config_json")?;
        let companion_context_json =
            optional_json_string(meta.companion_context.as_ref(), "companion_context_json")?;
        let tab_layout_json = optional_json_string(meta.tab_layout.as_ref(), "tab_layout_json")?;
        let visible_canvas_mount_ids_json = json_string(
            &meta.visible_canvas_mount_ids,
            "visible_canvas_mount_ids_json",
        )?;
        sqlx::query(
            r#"
            INSERT INTO sessions (
                id, title, title_source, created_at, updated_at, last_event_seq, last_execution_status,
                last_turn_id, last_terminal_message, executor_config_json,
                executor_session_id, companion_context_json, tab_layout_json, visible_canvas_mount_ids_json,
                bootstrap_state
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(id) DO UPDATE SET
                title = excluded.title,
                title_source = excluded.title_source,
                created_at = excluded.created_at,
                updated_at = MAX(sessions.updated_at, excluded.updated_at),
                last_event_seq = MAX(sessions.last_event_seq, excluded.last_event_seq),
                last_execution_status = CASE
                    WHEN excluded.last_event_seq >= sessions.last_event_seq
                        THEN excluded.last_execution_status
                    ELSE sessions.last_execution_status
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
                companion_context_json = excluded.companion_context_json,
                tab_layout_json = excluded.tab_layout_json,
                visible_canvas_mount_ids_json = excluded.visible_canvas_mount_ids_json,
                bootstrap_state = CASE
                    WHEN sessions.bootstrap_state = 'bootstrapped'
                        THEN sessions.bootstrap_state
                    ELSE excluded.bootstrap_state
                END
            "#,
        )
        .bind(&meta.id)
        .bind(&meta.title)
        .bind(title_source_to_str(meta.title_source))
        .bind(meta.created_at)
        .bind(meta.updated_at)
        .bind(last_event_seq)
        .bind(meta.last_execution_status.to_string())
        .bind(&meta.last_turn_id)
        .bind(&meta.last_terminal_message)
        .bind(executor_config_json)
        .bind(&meta.executor_session_id)
        .bind(companion_context_json)
        .bind(tab_layout_json)
        .bind(visible_canvas_mount_ids_json)
        .bind(bootstrap_state_to_str(meta.bootstrap_state))
        .execute(&self.pool)
        .await
        .map_err(sqlx_to_io)?;
        Ok(())
    }

    async fn delete_session(&self, session_id: &str) -> io::Result<()> {
        let mut tx = self.pool.begin().await.map_err(sqlx_to_io)?;
        sqlx::query("DELETE FROM session_events WHERE session_id = ?")
            .bind(session_id)
            .execute(&mut *tx)
            .await
            .map_err(sqlx_to_io)?;
        sqlx::query("DELETE FROM session_terminal_effects WHERE session_id = ?")
            .bind(session_id)
            .execute(&mut *tx)
            .await
            .map_err(sqlx_to_io)?;
        sqlx::query("DELETE FROM session_runtime_commands WHERE session_id = ?")
            .bind(session_id)
            .execute(&mut *tx)
            .await
            .map_err(sqlx_to_io)?;
        sqlx::query(
            "DELETE FROM session_lineage WHERE child_session_id = ? OR parent_session_id = ?",
        )
        .bind(session_id)
        .bind(session_id)
        .execute(&mut *tx)
        .await
        .map_err(sqlx_to_io)?;
        sqlx::query("DELETE FROM session_projection_heads WHERE session_id = ?")
            .bind(session_id)
            .execute(&mut *tx)
            .await
            .map_err(sqlx_to_io)?;
        sqlx::query("DELETE FROM session_projection_segments WHERE session_id = ?")
            .bind(session_id)
            .execute(&mut *tx)
            .await
            .map_err(sqlx_to_io)?;
        sqlx::query("DELETE FROM session_compactions WHERE session_id = ?")
            .bind(session_id)
            .execute(&mut *tx)
            .await
            .map_err(sqlx_to_io)?;
        sqlx::query("DELETE FROM sessions WHERE id = ?")
            .bind(session_id)
            .execute(&mut *tx)
            .await
            .map_err(sqlx_to_io)?;
        tx.commit().await.map_err(sqlx_to_io)?;
        Ok(())
    }

}

#[async_trait::async_trait]
impl SessionEventStore for SqliteSessionRepository {
    async fn append_event(
        &self,
        session_id: &str,
        envelope: &BackboneEnvelope,
    ) -> io::Result<PersistedSessionEvent> {
        let mut tx = self.pool.begin().await.map_err(sqlx_to_io)?;
        let seq_row = sqlx::query(
            r#"
            UPDATE sessions
            SET last_event_seq = last_event_seq + 1
            WHERE id = ?
            RETURNING last_event_seq
            "#,
        )
        .bind(session_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(sqlx_to_io)?
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!("session {session_id} 不存在"),
            )
        })?;
        let committed_at_ms = chrono::Utc::now().timestamp_millis();
        let event_seq_i64: i64 = seq_row.try_get("last_event_seq").map_err(sqlx_to_io)?;
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
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
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
        .map_err(sqlx_to_io)?;

        sqlx::query(
            r#"
            UPDATE sessions
            SET
                updated_at = ?,
                last_execution_status = COALESCE(?, last_execution_status),
                last_turn_id = COALESCE(?, last_turn_id),
                last_terminal_message = CASE
                    WHEN ? THEN NULL
                    WHEN ? IS NOT NULL THEN ?
                    ELSE last_terminal_message
                END,
                executor_session_id = COALESCE(?, executor_session_id)
            WHERE id = ?
            "#,
        )
        .bind(committed_at_ms)
        .bind(&projection.last_execution_status)
        .bind(&projection.turn_id)
        .bind(projection.clear_terminal_message)
        .bind(&projection.last_terminal_message)
        .bind(&projection.last_terminal_message)
        .bind(&projection.executor_session_id)
        .bind(session_id)
        .execute(&mut *tx)
        .await
        .map_err(sqlx_to_io)?;

        tx.commit().await.map_err(sqlx_to_io)?;
        Ok(persisted)
    }

    async fn read_backlog(
        &self,
        session_id: &str,
        after_seq: u64,
    ) -> io::Result<SessionEventBacklog> {
        let snapshot_seq = self.require_snapshot_seq(session_id).await?;
        let after_seq_db = encode_u64_as_i64(after_seq, "session_events.after_seq")?;
        let snapshot_seq_db = encode_u64_as_i64(snapshot_seq, "sessions.last_event_seq")?;
        let rows = sqlx::query(
            r#"
            SELECT session_id, event_seq, occurred_at_ms, committed_at_ms,
                   session_update_type, turn_id, entry_index, tool_call_id, notification_json
            FROM session_events
            WHERE session_id = ? AND event_seq > ? AND event_seq <= ?
            ORDER BY event_seq ASC
            "#,
        )
        .bind(session_id)
        .bind(after_seq_db)
        .bind(snapshot_seq_db)
        .fetch_all(&self.pool)
        .await
        .map_err(sqlx_to_io)?;

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
    ) -> io::Result<SessionEventPage> {
        let snapshot_seq = self.require_snapshot_seq(session_id).await?;
        let take = limit.max(1);
        let after_seq_db = encode_u64_as_i64(after_seq, "session_events.after_seq")?;
        let take_usize = usize::try_from(take)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "分页大小超出 usize 范围"))?;
        let rows = sqlx::query(
            r#"
            SELECT session_id, event_seq, occurred_at_ms, committed_at_ms,
                   session_update_type, turn_id, entry_index, tool_call_id, notification_json
            FROM session_events
            WHERE session_id = ? AND event_seq > ?
            ORDER BY event_seq ASC
            LIMIT ?
            "#,
        )
        .bind(session_id)
        .bind(after_seq_db)
        .bind(i64::from(take) + 1)
        .fetch_all(&self.pool)
        .await
        .map_err(sqlx_to_io)?;

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

    async fn list_all_events(&self, session_id: &str) -> io::Result<Vec<PersistedSessionEvent>> {
        let rows = sqlx::query(
            r#"
            SELECT session_id, event_seq, occurred_at_ms, committed_at_ms,
                   session_update_type, turn_id, entry_index, tool_call_id, notification_json
            FROM session_events
            WHERE session_id = ?
            ORDER BY event_seq ASC
            "#,
        )
        .bind(session_id)
        .fetch_all(&self.pool)
        .await
        .map_err(sqlx_to_io)?;

        let mut events = Vec::with_capacity(rows.len());
        for row in rows {
            events.push(persisted_event_from_row(&row)?);
        }
        Ok(events)
    }

}

#[async_trait::async_trait]
impl SessionTerminalEffectStore for SqliteSessionRepository {
    async fn insert_terminal_effect(
        &self,
        effect: NewTerminalEffectRecord,
    ) -> io::Result<TerminalEffectRecord> {
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
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
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
        .map_err(sqlx_to_io)?;
        Ok(record)
    }

    async fn mark_terminal_effect_running(&self, effect_id: uuid::Uuid) -> io::Result<()> {
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

    async fn mark_terminal_effect_succeeded(&self, effect_id: uuid::Uuid) -> io::Result<()> {
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
    ) -> io::Result<()> {
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
    ) -> io::Result<()> {
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
    ) -> io::Result<Vec<TerminalEffectRecord>> {
        if statuses.is_empty() {
            return Ok(Vec::new());
        }
        let placeholders = vec!["?"; statuses.len()].join(", ");
        let limit = i64::from(limit.max(1));
        let sql = format!(
            r#"
            SELECT id, session_id, turn_id, terminal_event_seq, effect_type, payload_json,
                   status, attempt_count, created_at_ms, updated_at_ms, last_error
            FROM session_terminal_effects
            WHERE status IN ({placeholders})
            ORDER BY updated_at_ms ASC, created_at_ms ASC
            LIMIT ?
            "#,
        );
        let mut query = sqlx::query(&sql);
        for status in statuses {
            query = query.bind(status.as_str());
        }
        let rows = query
            .bind(limit)
            .fetch_all(&self.pool)
            .await
            .map_err(sqlx_to_io)?;
        rows.iter().map(terminal_effect_from_row).collect()
    }

}

#[async_trait::async_trait]
impl SessionRuntimeCommandStore for SqliteSessionRepository {
    async fn upsert_runtime_command_request(
        &self,
        session_id: &str,
        transition: PendingCapabilityStateTransition,
    ) -> io::Result<RuntimeCommandRecord> {
        let mut tx = self.pool.begin().await.map_err(sqlx_to_io)?;
        let now = chrono::Utc::now().timestamp_millis();
        sqlx::query(
            r#"
            UPDATE session_runtime_commands
            SET status = ?,
                updated_at_ms = ?,
                failed_at_ms = ?,
                last_error = ?
            WHERE session_id = ? AND phase_node = ? AND status = ?
            "#,
        )
        .bind(RuntimeCommandStatus::Failed.as_str())
        .bind(now)
        .bind(now)
        .bind("superseded_by_new_requested_command")
        .bind(session_id)
        .bind(&transition.phase_node)
        .bind(RuntimeCommandStatus::Requested.as_str())
        .execute(&mut *tx)
        .await
        .map_err(sqlx_to_io)?;

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
        let payload_json =
            json_string(&record.transition, "session_runtime_commands.payload_json")?;
        sqlx::query(
            r#"
            INSERT INTO session_runtime_commands (
                id, session_id, transition_id, phase_node, status, payload_json,
                created_at_ms, updated_at_ms, applied_at_ms, failed_at_ms, last_error
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(record.id.to_string())
        .bind(&record.session_id)
        .bind(&record.transition_id)
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
        .map_err(sqlx_to_io)?;
        tx.commit().await.map_err(sqlx_to_io)?;
        Ok(record)
    }

    async fn list_requested_runtime_commands(
        &self,
        session_id: &str,
    ) -> io::Result<Vec<RuntimeCommandRecord>> {
        let rows = sqlx::query(
            r#"
            SELECT id, session_id, transition_id, phase_node, status, payload_json,
                   created_at_ms, updated_at_ms, applied_at_ms, failed_at_ms, last_error
            FROM session_runtime_commands
            WHERE session_id = ? AND status = ?
            ORDER BY created_at_ms ASC
            "#,
        )
        .bind(session_id)
        .bind(RuntimeCommandStatus::Requested.as_str())
        .fetch_all(&self.pool)
        .await
        .map_err(sqlx_to_io)?;
        rows.iter().map(runtime_command_from_row).collect()
    }

    async fn mark_runtime_commands_applied(&self, command_ids: &[uuid::Uuid]) -> io::Result<()> {
        self.update_runtime_commands_status(command_ids, RuntimeCommandStatus::Applied, None)
            .await
    }

    async fn mark_runtime_commands_failed(
        &self,
        command_ids: &[uuid::Uuid],
        error: String,
    ) -> io::Result<()> {
        self.update_runtime_commands_status(command_ids, RuntimeCommandStatus::Failed, Some(error))
            .await
    }

    async fn list_runtime_commands_by_status(
        &self,
        statuses: &[RuntimeCommandStatus],
        limit: u32,
    ) -> io::Result<Vec<RuntimeCommandRecord>> {
        if statuses.is_empty() {
            return Ok(Vec::new());
        }
        let placeholders = vec!["?"; statuses.len()].join(", ");
        let limit = i64::from(limit.max(1));
        let sql = format!(
            r#"
            SELECT id, session_id, transition_id, phase_node, status, payload_json,
                   created_at_ms, updated_at_ms, applied_at_ms, failed_at_ms, last_error
            FROM session_runtime_commands
            WHERE status IN ({placeholders})
            ORDER BY updated_at_ms ASC, created_at_ms ASC
            LIMIT ?
            "#,
        );
        let mut query = sqlx::query(&sql);
        for status in statuses {
            query = query.bind(status.as_str());
        }
        let rows = query
            .bind(limit)
            .fetch_all(&self.pool)
            .await
            .map_err(sqlx_to_io)?;
        rows.iter().map(runtime_command_from_row).collect()
    }

}

#[async_trait::async_trait]
impl SessionCompactionStore for SqliteSessionRepository {
    async fn get_compaction(
        &self,
        session_id: &str,
        compaction_id: &str,
    ) -> io::Result<Option<SessionCompactionRecord>> {
        let row = sqlx::query(
            r#"
            SELECT id, session_id, projection_kind, projection_version,
                   lifecycle_item_id, start_event_seq, completed_event_seq, failed_event_seq,
                   status, trigger, reason, phase, strategy, budget_scope,
                   base_head_event_seq, source_start_event_seq, source_end_event_seq,
                   first_kept_event_seq, summary, replacement_projection_json,
                   token_stats_json, diagnostics_json, created_by, created_at_ms, completed_at_ms
            FROM session_compactions
            WHERE session_id = ? AND id = ?
            "#,
        )
        .bind(session_id)
        .bind(compaction_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(sqlx_to_io)?;
        row.as_ref().map(compaction_from_row).transpose()
    }

    async fn list_compactions(
        &self,
        session_id: &str,
        projection_kind: &str,
    ) -> io::Result<Vec<SessionCompactionRecord>> {
        let rows = sqlx::query(
            r#"
            SELECT id, session_id, projection_kind, projection_version,
                   lifecycle_item_id, start_event_seq, completed_event_seq, failed_event_seq,
                   status, trigger, reason, phase, strategy, budget_scope,
                   base_head_event_seq, source_start_event_seq, source_end_event_seq,
                   first_kept_event_seq, summary, replacement_projection_json,
                   token_stats_json, diagnostics_json, created_by, created_at_ms, completed_at_ms
            FROM session_compactions
            WHERE session_id = ? AND projection_kind = ?
            ORDER BY projection_version ASC, created_at_ms ASC
            "#,
        )
        .bind(session_id)
        .bind(projection_kind)
        .fetch_all(&self.pool)
        .await
        .map_err(sqlx_to_io)?;
        rows.iter().map(compaction_from_row).collect()
    }

}

#[async_trait::async_trait]
impl SessionProjectionStore for SqliteSessionRepository {
    async fn list_projection_segments(
        &self,
        session_id: &str,
        projection_kind: &str,
        projection_version: u64,
    ) -> io::Result<Vec<SessionProjectionSegmentRecord>> {
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
            WHERE session_id = ? AND projection_kind = ? AND projection_version = ?
            ORDER BY sort_order ASC
            "#,
        )
        .bind(session_id)
        .bind(projection_kind)
        .bind(projection_version)
        .fetch_all(&self.pool)
        .await
        .map_err(sqlx_to_io)?;
        rows.iter().map(projection_segment_from_row).collect()
    }

    async fn read_projection_head(
        &self,
        session_id: &str,
        projection_kind: &str,
    ) -> io::Result<Option<SessionProjectionHeadRecord>> {
        let row = sqlx::query(
            r#"
            SELECT session_id, projection_kind, projection_version, head_event_seq,
                   active_compaction_id, updated_by_event_seq, updated_at_ms
            FROM session_projection_heads
            WHERE session_id = ? AND projection_kind = ?
            "#,
        )
        .bind(session_id)
        .bind(projection_kind)
        .fetch_optional(&self.pool)
        .await
        .map_err(sqlx_to_io)?;
        row.as_ref().map(projection_head_from_row).transpose()
    }

    async fn upsert_projection_head(&self, head: SessionProjectionHeadRecord) -> io::Result<()> {
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
            ) VALUES (?, ?, ?, ?, ?, ?, ?)
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
        .map_err(sqlx_to_io)?;
        Ok(())
    }

    async fn commit_compaction_projection(
        &self,
        session_id: &str,
        commit: NewCompactionProjectionCommit,
    ) -> io::Result<CompactionProjectionCommitResult> {
        validate_commit_session(session_id, &commit)?;
        let mut tx = self.pool.begin().await.map_err(sqlx_to_io)?;
        let seq_row = sqlx::query(
            r#"
            UPDATE sessions
            SET last_event_seq = last_event_seq + 1
            WHERE id = ?
            RETURNING last_event_seq
            "#,
        )
        .bind(session_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(sqlx_to_io)?
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!("session {session_id} 不存在"),
            )
        })?;
        let committed_at_ms = chrono::Utc::now().timestamp_millis();
        let event_seq_i64: i64 = seq_row.try_get("last_event_seq").map_err(sqlx_to_io)?;
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
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
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
        .map_err(sqlx_to_io)?;

        sqlx::query(
            r#"
            UPDATE sessions
            SET
                updated_at = ?,
                last_execution_status = COALESCE(?, last_execution_status),
                last_turn_id = COALESCE(?, last_turn_id),
                last_terminal_message = CASE
                    WHEN ? THEN NULL
                    WHEN ? IS NOT NULL THEN ?
                    ELSE last_terminal_message
                END,
                executor_session_id = COALESCE(?, executor_session_id)
            WHERE id = ?
            "#,
        )
        .bind(committed_at_ms)
        .bind(&projection.last_execution_status)
        .bind(&projection.turn_id)
        .bind(projection.clear_terminal_message)
        .bind(&projection.last_terminal_message)
        .bind(&projection.last_terminal_message)
        .bind(&projection.executor_session_id)
        .bind(session_id)
        .execute(&mut *tx)
        .await
        .map_err(sqlx_to_io)?;

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

        tx.commit().await.map_err(sqlx_to_io)?;
        Ok(CompactionProjectionCommitResult {
            event: persisted,
            compaction,
            segments: commit.segments,
            head,
        })
    }

}

#[async_trait::async_trait]
impl SessionLineageStore for SqliteSessionRepository {
    async fn upsert_session_lineage(&self, record: SessionLineageRecord) -> io::Result<()> {
        if record.child_session_id == record.parent_session_id {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "session lineage 不能指向自身",
            ));
        }
        let cycle = sqlx::query_scalar::<_, i64>(
            r#"
            WITH RECURSIVE parents(session_id) AS (
                SELECT ?
                UNION ALL
                SELECT session_lineage.parent_session_id
                FROM session_lineage
                JOIN parents ON session_lineage.child_session_id = parents.session_id
                WHERE session_lineage.child_session_id <> ?
            )
            SELECT 1
            FROM parents
            WHERE session_id = ?
            LIMIT 1
            "#,
        )
        .bind(&record.parent_session_id)
        .bind(&record.child_session_id)
        .bind(&record.child_session_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(sqlx_to_io)?;
        if cycle.is_some() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "session lineage 不能形成环",
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
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
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
        .map_err(sqlx_to_io)?;
        Ok(())
    }

    async fn get_session_lineage(
        &self,
        child_session_id: &str,
    ) -> io::Result<Option<SessionLineageRecord>> {
        let row = sqlx::query(
            r#"
            SELECT child_session_id, parent_session_id, relation_kind, fork_point_event_seq,
                   fork_point_ref_json, fork_point_compaction_id, status, created_at_ms,
                   updated_at_ms, metadata_json
            FROM session_lineage
            WHERE child_session_id = ?
            "#,
        )
        .bind(child_session_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(sqlx_to_io)?;
        row.as_ref().map(lineage_from_row).transpose()
    }

    async fn list_session_children(
        &self,
        parent_session_id: &str,
        relation_kind: Option<SessionLineageRelationKind>,
        status: Option<SessionLineageStatus>,
    ) -> io::Result<Vec<SessionLineageRecord>> {
        let rows = sqlx::query(
            r#"
            SELECT child_session_id, parent_session_id, relation_kind, fork_point_event_seq,
                   fork_point_ref_json, fork_point_compaction_id, status, created_at_ms,
                   updated_at_ms, metadata_json
            FROM session_lineage
            WHERE parent_session_id = ?
              AND (? IS NULL OR relation_kind = ?)
              AND (? IS NULL OR status = ?)
            ORDER BY created_at_ms ASC, updated_at_ms ASC, child_session_id ASC
            "#,
        )
        .bind(parent_session_id)
        .bind(relation_kind.map(SessionLineageRelationKind::as_str))
        .bind(relation_kind.map(SessionLineageRelationKind::as_str))
        .bind(status.map(SessionLineageStatus::as_str))
        .bind(status.map(SessionLineageStatus::as_str))
        .fetch_all(&self.pool)
        .await
        .map_err(sqlx_to_io)?;
        rows.iter().map(lineage_from_row).collect()
    }

    async fn list_session_ancestors(
        &self,
        child_session_id: &str,
    ) -> io::Result<Vec<SessionLineageRecord>> {
        let rows = sqlx::query(
            r#"
            WITH RECURSIVE lineage_path AS (
                SELECT child_session_id, parent_session_id, relation_kind, fork_point_event_seq,
                       fork_point_ref_json, fork_point_compaction_id, status, created_at_ms,
                       updated_at_ms, metadata_json, 0 AS depth
                FROM session_lineage
                WHERE child_session_id = ?
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
        .map_err(sqlx_to_io)?;
        rows.iter().map(lineage_from_row).collect()
    }

    async fn list_session_descendants(
        &self,
        root_session_id: &str,
        relation_kind: Option<SessionLineageRelationKind>,
        status: Option<SessionLineageStatus>,
    ) -> io::Result<Vec<SessionLineageRecord>> {
        let rows = sqlx::query(
            r#"
            WITH RECURSIVE lineage_tree AS (
                SELECT child_session_id, parent_session_id, relation_kind, fork_point_event_seq,
                       fork_point_ref_json, fork_point_compaction_id, status, created_at_ms,
                       updated_at_ms, metadata_json, 1 AS depth
                FROM session_lineage
                WHERE parent_session_id = ?
                  AND (? IS NULL OR relation_kind = ?)
                  AND (? IS NULL OR status = ?)
                UNION ALL
                SELECT child.child_session_id, child.parent_session_id, child.relation_kind,
                       child.fork_point_event_seq, child.fork_point_ref_json,
                       child.fork_point_compaction_id, child.status, child.created_at_ms,
                       child.updated_at_ms, child.metadata_json, lineage_tree.depth + 1
                FROM session_lineage child
                JOIN lineage_tree ON child.parent_session_id = lineage_tree.child_session_id
                WHERE (? IS NULL OR child.relation_kind = ?)
                  AND (? IS NULL OR child.status = ?)
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
        .bind(relation_kind.map(SessionLineageRelationKind::as_str))
        .bind(status.map(SessionLineageStatus::as_str))
        .bind(status.map(SessionLineageStatus::as_str))
        .bind(relation_kind.map(SessionLineageRelationKind::as_str))
        .bind(relation_kind.map(SessionLineageRelationKind::as_str))
        .bind(status.map(SessionLineageStatus::as_str))
        .bind(status.map(SessionLineageStatus::as_str))
        .fetch_all(&self.pool)
        .await
        .map_err(sqlx_to_io)?;
        rows.iter().map(lineage_from_row).collect()
    }

    async fn set_session_lineage_status(
        &self,
        child_session_id: &str,
        status: SessionLineageStatus,
        updated_at_ms: i64,
    ) -> io::Result<()> {
        let result = sqlx::query(
            r#"
            UPDATE session_lineage
            SET status = ?, updated_at_ms = ?
            WHERE child_session_id = ?
            "#,
        )
        .bind(status.as_str())
        .bind(updated_at_ms)
        .bind(child_session_id)
        .execute(&self.pool)
        .await
        .map_err(sqlx_to_io)?;
        if result.rows_affected() == 0 {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("session lineage child {child_session_id} 不存在"),
            ));
        }
        Ok(())
    }
}

async fn insert_compaction_row(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    record: &SessionCompactionRecord,
) -> io::Result<()> {
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
            ?, ?, ?, ?,
            ?, ?, ?, ?,
            ?, ?, ?, ?, ?, ?,
            ?, ?, ?,
            ?, ?, ?,
            ?, ?, ?, ?, ?
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
    .map_err(sqlx_to_io)?;
    Ok(())
}

async fn insert_projection_segment_row(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    segment: &SessionProjectionSegmentRecord,
) -> io::Result<()> {
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
            ?, ?, ?, ?, ?,
            ?, ?, ?, ?, ?,
            ?, ?, ?, ?,
            ?
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
    .bind(if segment.synthetic { 1_i64 } else { 0_i64 })
    .bind(source_start_event_seq)
    .bind(source_end_event_seq)
    .bind(source_refs_json)
    .bind(&segment.generated_by_compaction_id)
    .bind(content_json)
    .bind(token_estimate)
    .bind(segment.created_at_ms)
    .execute(&mut **tx)
    .await
    .map_err(sqlx_to_io)?;
    Ok(())
}

async fn upsert_projection_head_row(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    head: &SessionProjectionHeadRecord,
) -> io::Result<()> {
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
        ) VALUES (?, ?, ?, ?, ?, ?, ?)
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
    .map_err(sqlx_to_io)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentdash_agent_protocol::{
        BackboneEvent, ItemCompletedNotification, PlatformEvent, SourceInfo, TraceInfo,
    };
    use agentdash_spi::session_persistence::{
        ExecutionStatus, SessionBootstrapState, SessionCompactionRecord, SessionCompactionStatus,
        SessionProjectionHeadRecord, SessionProjectionSegmentRecord, TerminalEffectType, TitleSource,
    };
    use chrono::Utc;

    fn turn_terminal_envelope(
        session_id: &str,
        turn_id: &str,
        terminal_type: &str,
        message: &str,
    ) -> BackboneEnvelope {
        use agentdash_agent_protocol::codex_app_server_protocol as codex;
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

    fn context_compaction_completed_envelope(
        session_id: &str,
        turn_id: &str,
        item_id: &str,
    ) -> BackboneEnvelope {
        use agentdash_agent_protocol::codex_app_server_protocol as codex;
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
        let pool = SqlitePool::connect("sqlite::memory:")
            .await
            .expect("应能创建内存 sqlite");
        let repo = SqliteSessionRepository::new(pool);
        repo.initialize().await.expect("应能初始化 session 表");

        let meta = SessionMeta {
            id: "sess-1".to_string(),
            title: "测试".to_string(),
            title_source: TitleSource::Auto,
            project_id: None,
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
    async fn initialize_backfills_legacy_session_event_payloads() {
        let pool = SqlitePool::connect("sqlite::memory:")
            .await
            .expect("应能创建内存 sqlite");
        let repo = SqliteSessionRepository::new(pool.clone());
        repo.initialize().await.expect("应能初始化 session 表");
        repo.create_session(&session_meta("sess-legacy"))
            .await
            .expect("应能创建 session");

        let source = serde_json::json!({
            "connectorId": "test",
            "connectorType": "test"
        });
        let item_started = serde_json::json!({
            "event": {
                "type": "item_started",
                "payload": {
                    "item": {
                        "type": "dynamicToolCall",
                        "id": "tool-1",
                        "namespace": null,
                        "tool": "read",
                        "arguments": {},
                        "status": "inProgress",
                        "contentItems": null,
                        "success": null,
                        "durationMs": null
                    },
                    "threadId": "sess-legacy",
                    "turnId": "turn-1"
                }
            },
            "sessionId": "sess-legacy",
            "source": source,
            "trace": {"turnId": "turn-1"},
            "observedAt": "2026-05-28T00:00:00Z"
        });
        let item_completed = serde_json::json!({
            "event": {
                "type": "item_completed",
                "payload": {
                    "item": {
                        "type": "dynamicToolCall",
                        "id": "tool-1",
                        "namespace": null,
                        "tool": "read",
                        "arguments": {},
                        "status": "completed",
                        "contentItems": null,
                        "success": true,
                        "durationMs": null
                    },
                    "threadId": "sess-legacy",
                    "turnId": "turn-1"
                }
            },
            "sessionId": "sess-legacy",
            "source": source,
            "trace": {"turnId": "turn-1"},
            "observedAt": "2026-05-28T00:00:01Z"
        });
        let token_usage = serde_json::json!({
            "event": {
                "type": "token_usage_updated",
                "payload": {
                    "threadId": "sess-legacy",
                    "turnId": "turn-1",
                    "tokenUsage": {
                        "total": {
                            "totalTokens": 120,
                            "inputTokens": 90,
                            "cachedInputTokens": 10,
                            "outputTokens": 30,
                            "reasoningOutputTokens": 5
                        },
                        "last": {
                            "totalTokens": 12,
                            "inputTokens": 9,
                            "cachedInputTokens": 1,
                            "outputTokens": 3,
                            "reasoningOutputTokens": 0
                        },
                        "modelContextWindow": 200
                    }
                }
            },
            "sessionId": "sess-legacy",
            "source": source,
            "trace": {"turnId": "turn-1"},
            "observedAt": "2026-05-28T00:00:02Z"
        });

        for (seq, event_type, payload) in [
            (1_i64, "item_started", item_started),
            (2_i64, "item_completed", item_completed),
            (3_i64, "token_usage_updated", token_usage),
        ] {
            sqlx::query(
                r#"
                INSERT INTO session_events (
                    session_id, event_seq, occurred_at_ms, committed_at_ms,
                    session_update_type, turn_id, entry_index, tool_call_id, notification_json
                ) VALUES (?, ?, ?, ?, ?, ?, NULL, ?, ?)
                "#,
            )
            .bind("sess-legacy")
            .bind(seq)
            .bind(1_000_i64 + seq)
            .bind(1_000_i64 + seq)
            .bind(event_type)
            .bind("turn-1")
            .bind(if event_type == "token_usage_updated" {
                None
            } else {
                Some("tool-1")
            })
            .bind(payload.to_string())
            .execute(&pool)
            .await
            .expect("应能写入旧事件");
        }
        sqlx::query("UPDATE sessions SET last_event_seq = 3 WHERE id = ?")
            .bind("sess-legacy")
            .execute(&pool)
            .await
            .expect("应能更新游标");

        repo.initialize().await.expect("再次初始化应补齐旧事件");

        let page = repo
            .list_event_page("sess-legacy", 0, 10)
            .await
            .expect("补齐后应能读取历史事件");
        assert_eq!(page.events.len(), 3);
        assert!(matches!(
            page.events[0].notification.event,
            BackboneEvent::ItemStarted(_)
        ));
        assert!(matches!(
            page.events[1].notification.event,
            BackboneEvent::ItemCompleted(_)
        ));
        match &page.events[2].notification.event {
            BackboneEvent::TokenUsageUpdated(notification) => {
                assert_eq!(notification.token_usage.context.provider_context_tokens, 12);
                assert_eq!(notification.token_usage.context.current_context_tokens, 12);
                assert_eq!(
                    notification.token_usage.context.cumulative_total_tokens,
                    120
                );
                assert_eq!(
                    notification.token_usage.context.effective_context_window,
                    Some(200)
                );
            }
            other => panic!("应为 token_usage_updated，实际为 {other:?}"),
        }
    }

    #[tokio::test]
    async fn stale_save_session_meta_does_not_roll_back_event_projection() {
        let pool = SqlitePool::connect("sqlite::memory:")
            .await
            .expect("应能创建内存 sqlite");
        let repo = SqliteSessionRepository::new(pool);
        repo.initialize().await.expect("应能初始化 session 表");

        let meta = SessionMeta {
            id: "sess-stale".to_string(),
            title: "测试".to_string(),
            title_source: TitleSource::Auto,
            project_id: None,
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
        repo.create_session(&meta).await.expect("应能创建 session");

        let mut stale = repo
            .get_session_meta("sess-stale")
            .await
            .expect("应能读取 session meta")
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

        let terminal = turn_terminal_envelope("sess-stale", "t-new", "turn_completed", "done");
        repo.append_event("sess-stale", &terminal)
            .await
            .expect("应能写入终态事件");

        repo.save_session_meta(&stale)
            .await
            .expect("旧快照回写仍应成功");

        let merged = repo
            .get_session_meta("sess-stale")
            .await
            .expect("应能再次读取 session meta")
            .expect("session 应存在");

        assert_eq!(merged.last_event_seq, 1);
        assert_eq!(merged.last_execution_status, ExecutionStatus::Completed);
        assert_eq!(merged.last_turn_id.as_deref(), Some("t-new"));
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
    async fn terminal_effect_outbox_persists_status_transitions() {
        let pool = SqlitePool::connect("sqlite::memory:")
            .await
            .expect("应能创建内存 sqlite");
        let repo = SqliteSessionRepository::new(pool);
        repo.initialize().await.expect("应能初始化 session 表");

        let meta = SessionMeta {
            id: "sess-effects".to_string(),
            title: "测试".to_string(),
            title_source: TitleSource::Auto,
            project_id: None,
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
        repo.create_session(&meta).await.expect("应能创建 session");

        let record = repo
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

        repo.mark_terminal_effect_running(record.id)
            .await
            .expect("应能标记 running");
        let running = repo
            .list_terminal_effects_by_status(&[TerminalEffectStatus::Running], 10)
            .await
            .expect("应能查询 running");
        assert_eq!(running.len(), 1);
        assert_eq!(running[0].attempt_count, 1);

        repo.mark_terminal_effect_succeeded(record.id)
            .await
            .expect("应能标记 succeeded");
        let succeeded = repo
            .list_terminal_effects_by_status(&[TerminalEffectStatus::Succeeded], 10)
            .await
            .expect("应能查询 succeeded");
        assert_eq!(succeeded.len(), 1);
        assert_eq!(succeeded[0].last_error, None);
    }

    #[tokio::test]
    async fn compaction_projection_commit_persists_checkpoint_segments_and_head() {
        let pool = SqlitePool::connect("sqlite::memory:")
            .await
            .expect("应能创建内存 sqlite");
        let repo = SqliteSessionRepository::new(pool);
        repo.initialize().await.expect("应能初始化 session 表");
        repo.create_session(&session_meta("sess-compact"))
            .await
            .expect("应能创建 session");

        let result = repo
            .commit_compaction_projection(
                "sess-compact",
                compaction_commit("sess-compact", "compaction-1", "segment-1", 1),
            )
            .await
            .expect("应能原子提交 compaction projection");

        assert_eq!(result.event.event_seq, 1);
        assert_eq!(result.compaction.completed_event_seq, Some(1));
        assert_eq!(result.head.updated_by_event_seq, Some(1));

        let stored = repo
            .get_compaction("sess-compact", "compaction-1")
            .await
            .expect("应能查询 compaction")
            .expect("compaction 应存在");
        assert_eq!(stored.summary, "压缩摘要");
        assert_eq!(stored.status, SessionCompactionStatus::ProjectionCommitted);

        let segments = repo
            .list_projection_segments("sess-compact", "model_context", 1)
            .await
            .expect("应能查询 projection segments");
        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0].segment_type, "summary_chunk");

        let head = repo
            .read_projection_head("sess-compact", "model_context")
            .await
            .expect("应能查询 projection head")
            .expect("projection head 应存在");
        assert_eq!(head.active_compaction_id.as_deref(), Some("compaction-1"));
        assert_eq!(head.projection_version, 1);
        assert_eq!(head.head_event_seq, result.event.event_seq);
        assert_eq!(head.updated_by_event_seq, Some(result.event.event_seq));
    }

    #[tokio::test]
    async fn projection_store_schema_uses_session_kind_head_key_without_branch() {
        let pool = SqlitePool::connect("sqlite::memory:")
            .await
            .expect("应能创建内存 sqlite");
        let repo = SqliteSessionRepository::new(pool.clone());
        repo.initialize().await.expect("应能初始化 session 表");

        for table in [
            "session_compactions",
            "session_projection_segments",
            "session_projection_heads",
        ] {
            let columns = sqlx::query(&format!("PRAGMA table_info({table})"))
                .fetch_all(&pool)
                .await
                .expect("应能读取表结构");
            let names = columns
                .iter()
                .map(|row| row.get::<String, _>("name"))
                .collect::<Vec<_>>();
            assert!(
                !names.iter().any(|name| name == "branch_id"),
                "{table} 不应包含 branch_id"
            );
        }

        let head_columns = sqlx::query("PRAGMA table_info(session_projection_heads)")
            .fetch_all(&pool)
            .await
            .expect("应能读取 projection head 表结构");
        let primary_key_columns = head_columns
            .iter()
            .filter_map(|row| {
                let pk = row.get::<i64, _>("pk");
                (pk > 0).then_some((pk, row.get::<String, _>("name")))
            })
            .collect::<Vec<_>>();
        assert_eq!(
            primary_key_columns,
            vec![
                (1, "session_id".to_string()),
                (2, "projection_kind".to_string()),
            ]
        );
    }

    #[tokio::test]
    async fn compaction_projection_commit_rejects_segment_invariant_mismatch() {
        let pool = SqlitePool::connect("sqlite::memory:")
            .await
            .expect("应能创建内存 sqlite");
        let repo = SqliteSessionRepository::new(pool);
        repo.initialize().await.expect("应能初始化 session 表");
        repo.create_session(&session_meta("sess-invariant"))
            .await
            .expect("应能创建 session");

        let mut commit = compaction_commit("sess-invariant", "compaction-1", "segment-1", 1);
        commit.segments[0].generated_by_compaction_id = Some("other-compaction".to_string());
        let result = repo
            .commit_compaction_projection("sess-invariant", commit)
            .await;
        assert!(matches!(
            result,
            Err(error) if error.kind() == io::ErrorKind::InvalidInput
        ));

        let events = repo
            .list_all_events("sess-invariant")
            .await
            .expect("应能读取 events");
        assert!(events.is_empty());
        let head = repo
            .read_projection_head("sess-invariant", "model_context")
            .await
            .expect("应能读取 projection head");
        assert!(head.is_none());
    }

    #[tokio::test]
    async fn session_lineage_queries_are_stable_and_filterable() {
        let pool = SqlitePool::connect("sqlite::memory:")
            .await
            .expect("应能创建内存 sqlite");
        let repo = SqliteSessionRepository::new(pool);
        repo.initialize().await.expect("应能初始化 session 表");
        for id in ["root", "child-a", "child-b", "grand"] {
            repo.create_session(&session_meta(id))
                .await
                .expect("应能创建 session");
        }

        repo.upsert_session_lineage(lineage_record(
            "child-a",
            "root",
            SessionLineageRelationKind::Fork,
            SessionLineageStatus::Open,
            20,
        ))
        .await
        .expect("应能写入 fork edge");
        repo.upsert_session_lineage(lineage_record(
            "child-b",
            "root",
            SessionLineageRelationKind::Companion,
            SessionLineageStatus::Open,
            10,
        ))
        .await
        .expect("应能写入 companion edge");
        repo.upsert_session_lineage(lineage_record(
            "grand",
            "child-b",
            SessionLineageRelationKind::Fork,
            SessionLineageStatus::Open,
            30,
        ))
        .await
        .expect("应能写入 grand edge");

        let children = repo
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

        let fork_children = repo
            .list_session_children(
                "root",
                Some(SessionLineageRelationKind::Fork),
                Some(SessionLineageStatus::Open),
            )
            .await
            .expect("应能按 relation 查询 children");
        assert_eq!(fork_children.len(), 1);
        assert_eq!(fork_children[0].child_session_id, "child-a");

        let ancestors = repo
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

        let descendants = repo
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

        repo.set_session_lineage_status("child-b", SessionLineageStatus::Closed, 40)
            .await
            .expect("应能关闭 lineage edge");
        let open_descendants = repo
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

    #[tokio::test]
    async fn failed_compaction_projection_commit_keeps_active_head_unchanged() {
        let pool = SqlitePool::connect("sqlite::memory:")
            .await
            .expect("应能创建内存 sqlite");
        let repo = SqliteSessionRepository::new(pool);
        repo.initialize().await.expect("应能初始化 session 表");
        repo.create_session(&session_meta("sess-atomic"))
            .await
            .expect("应能创建 session");

        repo.commit_compaction_projection(
            "sess-atomic",
            compaction_commit("sess-atomic", "compaction-ok", "segment-dup", 1),
        )
        .await
        .expect("初次 compaction 应成功");

        let failed = repo
            .commit_compaction_projection(
                "sess-atomic",
                compaction_commit("sess-atomic", "compaction-failed", "segment-dup", 2),
            )
            .await;
        assert!(failed.is_err());

        let meta = repo
            .get_session_meta("sess-atomic")
            .await
            .expect("应能读取 meta")
            .expect("session 应存在");
        assert_eq!(meta.last_event_seq, 1);

        let head = repo
            .read_projection_head("sess-atomic", "model_context")
            .await
            .expect("应能读取 projection head")
            .expect("projection head 应存在");
        assert_eq!(head.active_compaction_id.as_deref(), Some("compaction-ok"));
        assert_eq!(head.projection_version, 1);

        let missing = repo
            .get_compaction("sess-atomic", "compaction-failed")
            .await
            .expect("应能查询失败 compaction");
        assert!(missing.is_none());
    }
}
