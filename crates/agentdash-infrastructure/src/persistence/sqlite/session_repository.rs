use std::io;

use agentdash_agent_protocol::codex_app_server_protocol::ThreadItem;
use agentdash_agent_protocol::{BackboneEnvelope, BackboneEvent, PlatformEvent};
use agentdash_application::session::PendingCapabilityStateTransition;
use agentdash_application::session::{
    ExecutionStatus, PendingRuntimeCommandRecord, PersistedSessionEvent, RuntimeCommandStatus,
    SessionBootstrapState, SessionEventBacklog, SessionEventPage, SessionMeta, SessionPersistence,
    TerminalEffectRecord, TerminalEffectStatus, TitleSource,
};
use agentdash_application::session::{NewTerminalEffectRecord, TerminalEffectType};
use sqlx::{Row, SqlitePool};

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
        Ok(())
    }

    fn map_meta_row(row: &sqlx::sqlite::SqliteRow) -> io::Result<SessionMeta> {
        Ok(SessionMeta {
            id: row.get::<String, _>("id"),
            title: row.get::<String, _>("title"),
            title_source: parse_title_source(
                row.get::<String, _>("title_source"),
                "sessions.title_source",
            )?,
            created_at: row.get::<i64, _>("created_at"),
            updated_at: row.get::<i64, _>("updated_at"),
            last_event_seq: parse_non_negative_u64(
                row.get::<i64, _>("last_event_seq"),
                "sessions.last_event_seq",
            )?,
            last_execution_status: parse_execution_status(
                row.get::<String, _>("last_execution_status"),
                "sessions.last_execution_status",
            )?,
            last_turn_id: row.get::<Option<String>, _>("last_turn_id"),
            last_terminal_message: row.get::<Option<String>, _>("last_terminal_message"),
            executor_config: parse_optional_json_column(
                row.get::<Option<String>, _>("executor_config_json"),
                "executor_config_json",
            )?,
            executor_session_id: row.get::<Option<String>, _>("executor_session_id"),
            companion_context: parse_optional_json_column(
                row.get::<Option<String>, _>("companion_context_json"),
                "companion_context_json",
            )?,
            visible_canvas_mount_ids: parse_optional_json_column(
                row.get::<Option<String>, _>("visible_canvas_mount_ids_json"),
                "visible_canvas_mount_ids_json",
            )?
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    "缺少 visible_canvas_mount_ids_json",
                )
            })?,
            bootstrap_state: parse_bootstrap_state(
                row.get::<String, _>("bootstrap_state"),
                "sessions.bootstrap_state",
            )?,
        })
    }

    fn persisted_event_from_row(
        row: &sqlx::sqlite::SqliteRow,
    ) -> io::Result<PersistedSessionEvent> {
        let notification_json = row.get::<String, _>("notification_json");
        let notification = serde_json::from_str::<BackboneEnvelope>(&notification_json)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error.to_string()))?;
        let event_seq_i64 = row.get::<i64, _>("event_seq");
        let event_seq = parse_non_negative_u64(event_seq_i64, "session_events.event_seq")?;
        let entry_index = row
            .get::<Option<i64>, _>("entry_index")
            .map(|value| parse_non_negative_u32(value, "session_events.entry_index"))
            .transpose()?;
        Ok(PersistedSessionEvent {
            session_id: row.get::<String, _>("session_id"),
            event_seq,
            occurred_at_ms: row.get::<i64, _>("occurred_at_ms"),
            committed_at_ms: row.get::<i64, _>("committed_at_ms"),
            session_update_type: row.get::<String, _>("session_update_type"),
            turn_id: row.get::<Option<String>, _>("turn_id"),
            entry_index,
            tool_call_id: row.get::<Option<String>, _>("tool_call_id"),
            notification,
        })
    }

    fn terminal_effect_from_row(row: &sqlx::sqlite::SqliteRow) -> io::Result<TerminalEffectRecord> {
        let id_raw = row.get::<String, _>("id");
        let id = uuid::Uuid::parse_str(&id_raw)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error.to_string()))?;
        let terminal_event_seq = parse_non_negative_u64(
            row.get::<i64, _>("terminal_event_seq"),
            "session_terminal_effects.terminal_event_seq",
        )?;
        let attempt_count = parse_non_negative_u32(
            row.get::<i64, _>("attempt_count"),
            "session_terminal_effects.attempt_count",
        )?;
        let payload_json = row.get::<String, _>("payload_json");
        let payload = serde_json::from_str::<serde_json::Value>(&payload_json)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error.to_string()))?;
        Ok(TerminalEffectRecord {
            id,
            session_id: row.get::<String, _>("session_id"),
            turn_id: row.get::<String, _>("turn_id"),
            terminal_event_seq,
            effect_type: parse_terminal_effect_type(
                row.get::<String, _>("effect_type"),
                "session_terminal_effects.effect_type",
            )?,
            payload,
            status: parse_terminal_effect_status(
                row.get::<String, _>("status"),
                "session_terminal_effects.status",
            )?,
            attempt_count,
            created_at_ms: row.get::<i64, _>("created_at_ms"),
            updated_at_ms: row.get::<i64, _>("updated_at_ms"),
            last_error: row.get::<Option<String>, _>("last_error"),
        })
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

    fn runtime_command_from_row(
        row: &sqlx::sqlite::SqliteRow,
    ) -> io::Result<PendingRuntimeCommandRecord> {
        let id_raw = row.get::<String, _>("id");
        let id = uuid::Uuid::parse_str(&id_raw)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error.to_string()))?;
        let payload_json = row.get::<String, _>("payload_json");
        let transition = serde_json::from_str::<PendingCapabilityStateTransition>(&payload_json)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error.to_string()))?;
        Ok(PendingRuntimeCommandRecord {
            id,
            session_id: row.get::<String, _>("session_id"),
            transition_id: row.get::<String, _>("transition_id"),
            phase_node: row.get::<String, _>("phase_node"),
            status: parse_runtime_command_status(
                row.get::<String, _>("status"),
                "session_runtime_commands.status",
            )?,
            transition,
            created_at_ms: row.get::<i64, _>("created_at_ms"),
            updated_at_ms: row.get::<i64, _>("updated_at_ms"),
            applied_at_ms: row.get::<Option<i64>, _>("applied_at_ms"),
            failed_at_ms: row.get::<Option<i64>, _>("failed_at_ms"),
            last_error: row.get::<Option<String>, _>("last_error"),
        })
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
                RuntimeCommandStatus::Pending => (None, None, None),
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
impl SessionPersistence for SqliteSessionRepository {
    async fn create_session(&self, meta: &SessionMeta) -> io::Result<()> {
        let last_event_seq = encode_u64_as_i64(meta.last_event_seq, "sessions.last_event_seq")?;
        let executor_config_json =
            optional_json_string(meta.executor_config.as_ref(), "executor_config_json")?;
        let companion_context_json =
            optional_json_string(meta.companion_context.as_ref(), "companion_context_json")?;
        let visible_canvas_mount_ids_json = json_string(
            &meta.visible_canvas_mount_ids,
            "visible_canvas_mount_ids_json",
        )?;
        sqlx::query(
            r#"
            INSERT INTO sessions (
                id, title, title_source, created_at, updated_at, last_event_seq, last_execution_status,
                last_turn_id, last_terminal_message, executor_config_json,
                executor_session_id, companion_context_json, visible_canvas_mount_ids_json,
                bootstrap_state
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
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
                   executor_session_id, companion_context_json, visible_canvas_mount_ids_json,
                   bootstrap_state
            FROM sessions
            WHERE id = ?
            "#,
        )
        .bind(session_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(sqlx_to_io)?;
        row.as_ref().map(Self::map_meta_row).transpose()
    }

    async fn list_sessions(&self) -> io::Result<Vec<SessionMeta>> {
        let rows = sqlx::query(
            r#"
            SELECT id, title, title_source, created_at, updated_at, last_event_seq, last_execution_status,
                   last_turn_id, last_terminal_message, executor_config_json,
                   executor_session_id, companion_context_json, visible_canvas_mount_ids_json,
                   bootstrap_state
            FROM sessions
            ORDER BY updated_at DESC
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(sqlx_to_io)?;
        rows.iter().map(Self::map_meta_row).collect()
    }

    async fn save_session_meta(&self, meta: &SessionMeta) -> io::Result<()> {
        let last_event_seq = encode_u64_as_i64(meta.last_event_seq, "sessions.last_event_seq")?;
        let executor_config_json =
            optional_json_string(meta.executor_config.as_ref(), "executor_config_json")?;
        let companion_context_json =
            optional_json_string(meta.companion_context.as_ref(), "companion_context_json")?;
        let visible_canvas_mount_ids_json = json_string(
            &meta.visible_canvas_mount_ids,
            "visible_canvas_mount_ids_json",
        )?;
        sqlx::query(
            r#"
            INSERT INTO sessions (
                id, title, title_source, created_at, updated_at, last_event_seq, last_execution_status,
                last_turn_id, last_terminal_message, executor_config_json,
                executor_session_id, companion_context_json, visible_canvas_mount_ids_json,
                bootstrap_state
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
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
        sqlx::query("DELETE FROM sessions WHERE id = ?")
            .bind(session_id)
            .execute(&mut *tx)
            .await
            .map_err(sqlx_to_io)?;
        tx.commit().await.map_err(sqlx_to_io)?;
        Ok(())
    }

    async fn append_event(
        &self,
        session_id: &str,
        envelope: &BackboneEnvelope,
    ) -> io::Result<PersistedSessionEvent> {
        let mut tx = self.pool.begin().await.map_err(sqlx_to_io)?;
        let seq_update = sqlx::query(
            r#"
            UPDATE sessions
            SET last_event_seq = last_event_seq + 1
            WHERE id = ?
            "#,
        )
        .bind(session_id)
        .execute(&mut *tx)
        .await
        .map_err(sqlx_to_io)?;
        if seq_update.rows_affected() == 0 {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("session {session_id} 不存在"),
            ));
        }
        let seq_row = sqlx::query("SELECT last_event_seq FROM sessions WHERE id = ?")
            .bind(session_id)
            .fetch_one(&mut *tx)
            .await
            .map_err(sqlx_to_io)?;
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
            events.push(Self::persisted_event_from_row(&row)?);
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
            events.push(Self::persisted_event_from_row(&row)?);
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
            events.push(Self::persisted_event_from_row(&row)?);
        }
        Ok(events)
    }

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

    async fn list_terminal_effects_by_status(
        &self,
        statuses: &[TerminalEffectStatus],
        limit: u32,
    ) -> io::Result<Vec<TerminalEffectRecord>> {
        let rows = sqlx::query(
            r#"
            SELECT id, session_id, turn_id, terminal_event_seq, effect_type, payload_json,
                   status, attempt_count, created_at_ms, updated_at_ms, last_error
            FROM session_terminal_effects
            ORDER BY updated_at_ms ASC, created_at_ms ASC
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(sqlx_to_io)?;
        let limit = usize::try_from(limit.max(1))
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "分页大小超出 usize 范围"))?;
        let mut records = Vec::new();
        for row in rows {
            let record = Self::terminal_effect_from_row(&row)?;
            if statuses.contains(&record.status) {
                records.push(record);
                if records.len() >= limit {
                    break;
                }
            }
        }
        Ok(records)
    }

    async fn upsert_pending_runtime_command(
        &self,
        session_id: &str,
        transition: PendingCapabilityStateTransition,
    ) -> io::Result<PendingRuntimeCommandRecord> {
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
        .bind("superseded_by_new_pending_command")
        .bind(session_id)
        .bind(&transition.phase_node)
        .bind(RuntimeCommandStatus::Pending.as_str())
        .execute(&mut *tx)
        .await
        .map_err(sqlx_to_io)?;

        let record = PendingRuntimeCommandRecord {
            id: uuid::Uuid::new_v4(),
            session_id: session_id.to_string(),
            transition_id: transition.id.clone(),
            phase_node: transition.phase_node.clone(),
            status: RuntimeCommandStatus::Pending,
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

    async fn list_pending_runtime_commands(
        &self,
        session_id: &str,
    ) -> io::Result<Vec<PendingRuntimeCommandRecord>> {
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
        .bind(RuntimeCommandStatus::Pending.as_str())
        .fetch_all(&self.pool)
        .await
        .map_err(sqlx_to_io)?;
        rows.iter().map(Self::runtime_command_from_row).collect()
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
    ) -> io::Result<Vec<PendingRuntimeCommandRecord>> {
        let rows = sqlx::query(
            r#"
            SELECT id, session_id, transition_id, phase_node, status, payload_json,
                   created_at_ms, updated_at_ms, applied_at_ms, failed_at_ms, last_error
            FROM session_runtime_commands
            ORDER BY updated_at_ms ASC, created_at_ms ASC
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(sqlx_to_io)?;
        let limit = usize::try_from(limit.max(1))
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "分页大小超出 usize 范围"))?;
        let mut records = Vec::new();
        for row in rows {
            let record = Self::runtime_command_from_row(&row)?;
            if statuses.contains(&record.status) {
                records.push(record);
                if records.len() >= limit {
                    break;
                }
            }
        }
        Ok(records)
    }
}

fn json_string<T: serde::Serialize>(value: &T, column: &str) -> io::Result<String> {
    serde_json::to_string(value).map_err(|error| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("序列化 {column} 失败: {error}"),
        )
    })
}

fn optional_json_string<T: serde::Serialize>(
    value: Option<&T>,
    column: &str,
) -> io::Result<Option<String>> {
    value.map(|inner| json_string(inner, column)).transpose()
}

fn title_source_to_str(source: TitleSource) -> &'static str {
    match source {
        TitleSource::Auto => "auto",
        TitleSource::User => "user",
    }
}

fn parse_execution_status(value: String, field: &str) -> io::Result<ExecutionStatus> {
    match value.as_str() {
        "idle" => Ok(ExecutionStatus::Idle),
        "running" => Ok(ExecutionStatus::Running),
        "completed" => Ok(ExecutionStatus::Completed),
        "failed" => Ok(ExecutionStatus::Failed),
        "interrupted" => Ok(ExecutionStatus::Interrupted),
        other => Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("{field} 非法: {other}"),
        )),
    }
}

fn parse_terminal_effect_type(value: String, field: &str) -> io::Result<TerminalEffectType> {
    TerminalEffectType::try_from(value.as_str())
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, format!("{field}: {error}")))
}

fn parse_terminal_effect_status(value: String, field: &str) -> io::Result<TerminalEffectStatus> {
    TerminalEffectStatus::try_from(value.as_str())
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, format!("{field}: {error}")))
}

fn parse_runtime_command_status(value: String, field: &str) -> io::Result<RuntimeCommandStatus> {
    RuntimeCommandStatus::try_from(value.as_str())
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, format!("{field}: {error}")))
}

fn parse_title_source(value: String, field: &str) -> io::Result<TitleSource> {
    match value.as_str() {
        "auto" => Ok(TitleSource::Auto),
        "user" => Ok(TitleSource::User),
        other => Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("{field} 非法: {other}"),
        )),
    }
}

fn bootstrap_state_to_str(state: SessionBootstrapState) -> &'static str {
    match state {
        SessionBootstrapState::Plain => "plain",
        SessionBootstrapState::Pending => "pending",
        SessionBootstrapState::Bootstrapped => "bootstrapped",
    }
}

fn parse_bootstrap_state(value: String, field: &str) -> io::Result<SessionBootstrapState> {
    match value.as_str() {
        "plain" => Ok(SessionBootstrapState::Plain),
        "pending" => Ok(SessionBootstrapState::Pending),
        "bootstrapped" => Ok(SessionBootstrapState::Bootstrapped),
        other => Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("{field} 非法: {other}"),
        )),
    }
}

fn encode_u64_as_i64(value: u64, field: &str) -> io::Result<i64> {
    i64::try_from(value).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("{field} 超出 i64 可表示范围: {value}"),
        )
    })
}

fn parse_non_negative_u64(value: i64, field: &str) -> io::Result<u64> {
    u64::try_from(value).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("{field} 不能为负数: {value}"),
        )
    })
}

fn parse_non_negative_u32(value: i64, field: &str) -> io::Result<u32> {
    u32::try_from(value).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("{field} 超出 u32 范围: {value}"),
        )
    })
}

fn parse_optional_json_column<T: serde::de::DeserializeOwned>(
    raw: Option<String>,
    column: &str,
) -> io::Result<Option<T>> {
    match raw {
        Some(value) => serde_json::from_str(&value).map(Some).map_err(|error| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("解析 {column} 失败: {error}"),
            )
        }),
        None => Ok(None),
    }
}

fn sqlx_to_io(error: sqlx::Error) -> io::Error {
    io::Error::other(error.to_string())
}

struct SessionProjection {
    last_execution_status: Option<String>,
    turn_id: Option<String>,
    last_terminal_message: Option<String>,
    clear_terminal_message: bool,
    executor_session_id: Option<String>,
    entry_index: Option<u32>,
    tool_call_id: Option<String>,
}

fn projection_from_envelope(envelope: &BackboneEnvelope) -> SessionProjection {
    let turn_id = envelope.trace.turn_id.clone();
    let entry_index = envelope.trace.entry_index;
    let tool_call_id = envelope_tool_call_id(envelope);

    let mut projection = SessionProjection {
        last_execution_status: None,
        turn_id,
        last_terminal_message: None,
        clear_terminal_message: false,
        executor_session_id: None,
        entry_index,
        tool_call_id,
    };

    match &envelope.event {
        BackboneEvent::TurnStarted(_) => {
            projection.last_execution_status = Some("running".to_string());
            projection.clear_terminal_message = true;
        }
        BackboneEvent::TurnCompleted(n) => {
            let status = match n.turn.status {
                agentdash_agent_protocol::codex_app_server_protocol::TurnStatus::Completed => {
                    "completed"
                }
                agentdash_agent_protocol::codex_app_server_protocol::TurnStatus::Failed => "failed",
                agentdash_agent_protocol::codex_app_server_protocol::TurnStatus::Interrupted => {
                    "interrupted"
                }
                _ => "completed",
            };
            projection.last_execution_status = Some(status.to_string());
            projection.last_terminal_message = n.turn.error.as_ref().map(|e| e.message.clone());
        }
        BackboneEvent::Error(e) => {
            projection.last_execution_status = Some("failed".to_string());
            projection.last_terminal_message = Some(e.error.message.clone());
        }
        BackboneEvent::Platform(PlatformEvent::ExecutorSessionBound {
            executor_session_id,
        }) => {
            projection.executor_session_id = Some(executor_session_id.clone());
        }
        BackboneEvent::Platform(PlatformEvent::SessionMetaUpdate { key, value })
            if key == "turn_terminal" =>
        {
            if let Some(kind) = value.get("terminal_type").and_then(|v| v.as_str()) {
                let status = match kind {
                    "turn_completed" => "completed",
                    "turn_failed" => "failed",
                    "turn_interrupted" => "interrupted",
                    _ => "completed",
                };
                projection.last_execution_status = Some(status.to_string());
            }
            projection.last_terminal_message = value
                .get("message")
                .and_then(|v| v.as_str())
                .map(String::from);
        }
        _ => {}
    }

    projection
}

fn backbone_event_type_name(event: &BackboneEvent) -> &'static str {
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
        BackboneEvent::Platform(_) => "platform",
    }
}

fn thread_item_tool_call_id(item: &ThreadItem) -> Option<String> {
    match item {
        ThreadItem::DynamicToolCall { id, .. }
        | ThreadItem::CommandExecution { id, .. }
        | ThreadItem::McpToolCall { id, .. } => Some(id.clone()),
        _ => None,
    }
}

fn envelope_tool_call_id(envelope: &BackboneEnvelope) -> Option<String> {
    match &envelope.event {
        BackboneEvent::ItemStarted(n) => thread_item_tool_call_id(&n.item),
        BackboneEvent::ItemCompleted(n) => thread_item_tool_call_id(&n.item),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentdash_agent_protocol::{SourceInfo, TraceInfo};
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
        let pool = SqlitePool::connect("sqlite::memory:")
            .await
            .expect("应能创建内存 sqlite");
        let repo = SqliteSessionRepository::new(pool);
        repo.initialize().await.expect("应能初始化 session 表");

        let meta = SessionMeta {
            id: "sess-stale".to_string(),
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
}
