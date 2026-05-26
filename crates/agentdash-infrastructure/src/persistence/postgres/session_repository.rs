use std::io;

use agentdash_agent_protocol::codex_app_server_protocol::ThreadItem;
use agentdash_agent_protocol::{BackboneEnvelope, BackboneEvent, PlatformEvent};
use agentdash_spi::session_persistence::{
    CompactionProjectionCommitResult, ExecutionStatus, NewCompactionProjectionCommit,
    PersistedSessionEvent, RuntimeCommandRecord, RuntimeCommandStatus, SessionBootstrapState,
    SessionCompactionRecord, SessionCompactionStatus, SessionEventBacklog, SessionEventPage,
    SessionLineageRecord, SessionLineageRelationKind, SessionLineageStatus, SessionMeta,
    SessionPersistence, SessionProjectionHeadRecord, SessionProjectionSegmentRecord,
    TerminalEffectRecord, TerminalEffectStatus, TitleSource,
};
use agentdash_spi::session_persistence::{
    NewTerminalEffectRecord, PendingCapabilityStateTransition, TerminalEffectType,
};
use sqlx::{PgPool, Row};

pub struct PostgresSessionRepository {
    pool: PgPool,
}

impl PostgresSessionRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn initialize(&self) -> io::Result<()> {
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
                "session_runtime_commands",
            ],
        )
        .await
        .map_err(|err| io::Error::other(err.to_string()))
    }

    fn map_meta_row(row: &sqlx::postgres::PgRow) -> io::Result<SessionMeta> {
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
            tab_layout: parse_optional_json_column(
                row.get::<Option<String>, _>("tab_layout_json"),
                "tab_layout_json",
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

    fn persisted_event_from_row(row: &sqlx::postgres::PgRow) -> io::Result<PersistedSessionEvent> {
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

    fn terminal_effect_from_row(row: &sqlx::postgres::PgRow) -> io::Result<TerminalEffectRecord> {
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
        .map_err(sqlx_to_io)?;
        if result.rows_affected() == 0 {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("terminal effect {effect_id} 不存在"),
            ));
        }
        Ok(())
    }

    fn runtime_command_from_row(row: &sqlx::postgres::PgRow) -> io::Result<RuntimeCommandRecord> {
        let id_raw = row.get::<String, _>("id");
        let id = uuid::Uuid::parse_str(&id_raw)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error.to_string()))?;
        let payload_json = row.get::<String, _>("payload_json");
        let transition = serde_json::from_str::<PendingCapabilityStateTransition>(&payload_json)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error.to_string()))?;
        Ok(RuntimeCommandRecord {
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

    fn compaction_from_row(row: &sqlx::postgres::PgRow) -> io::Result<SessionCompactionRecord> {
        Ok(SessionCompactionRecord {
            id: row.get::<String, _>("id"),
            session_id: row.get::<String, _>("session_id"),
            projection_kind: row.get::<String, _>("projection_kind"),
            projection_version: parse_non_negative_u64(
                row.get::<i64, _>("projection_version"),
                "session_compactions.projection_version",
            )?,
            lifecycle_item_id: row.get::<String, _>("lifecycle_item_id"),
            start_event_seq: parse_non_negative_u64(
                row.get::<i64, _>("start_event_seq"),
                "session_compactions.start_event_seq",
            )?,
            completed_event_seq: parse_optional_non_negative_u64(
                row.get::<Option<i64>, _>("completed_event_seq"),
                "session_compactions.completed_event_seq",
            )?,
            failed_event_seq: parse_optional_non_negative_u64(
                row.get::<Option<i64>, _>("failed_event_seq"),
                "session_compactions.failed_event_seq",
            )?,
            status: parse_compaction_status(
                row.get::<String, _>("status"),
                "session_compactions.status",
            )?,
            trigger: row.get::<String, _>("trigger"),
            reason: row.get::<Option<String>, _>("reason"),
            phase: row.get::<Option<String>, _>("phase"),
            strategy: row.get::<String, _>("strategy"),
            budget_scope: row.get::<Option<String>, _>("budget_scope"),
            base_head_event_seq: parse_optional_non_negative_u64(
                row.get::<Option<i64>, _>("base_head_event_seq"),
                "session_compactions.base_head_event_seq",
            )?,
            source_start_event_seq: parse_optional_non_negative_u64(
                row.get::<Option<i64>, _>("source_start_event_seq"),
                "session_compactions.source_start_event_seq",
            )?,
            source_end_event_seq: parse_optional_non_negative_u64(
                row.get::<Option<i64>, _>("source_end_event_seq"),
                "session_compactions.source_end_event_seq",
            )?,
            first_kept_event_seq: parse_optional_non_negative_u64(
                row.get::<Option<i64>, _>("first_kept_event_seq"),
                "session_compactions.first_kept_event_seq",
            )?,
            summary: row.get::<String, _>("summary"),
            replacement_projection_json: parse_json_column(
                row.get::<String, _>("replacement_projection_json"),
                "session_compactions.replacement_projection_json",
            )?,
            token_stats_json: parse_json_column(
                row.get::<String, _>("token_stats_json"),
                "session_compactions.token_stats_json",
            )?,
            diagnostics_json: parse_json_column(
                row.get::<String, _>("diagnostics_json"),
                "session_compactions.diagnostics_json",
            )?,
            created_by: row.get::<Option<String>, _>("created_by"),
            created_at_ms: row.get::<i64, _>("created_at_ms"),
            completed_at_ms: row.get::<Option<i64>, _>("completed_at_ms"),
        })
    }

    fn projection_segment_from_row(
        row: &sqlx::postgres::PgRow,
    ) -> io::Result<SessionProjectionSegmentRecord> {
        Ok(SessionProjectionSegmentRecord {
            id: row.get::<String, _>("id"),
            session_id: row.get::<String, _>("session_id"),
            projection_kind: row.get::<String, _>("projection_kind"),
            projection_version: parse_non_negative_u64(
                row.get::<i64, _>("projection_version"),
                "session_projection_segments.projection_version",
            )?,
            sort_order: parse_non_negative_u64(
                row.get::<i64, _>("sort_order"),
                "session_projection_segments.sort_order",
            )?,
            segment_type: row.get::<String, _>("segment_type"),
            origin: row.get::<String, _>("origin"),
            synthetic: row.get::<bool, _>("synthetic"),
            source_start_event_seq: parse_optional_non_negative_u64(
                row.get::<Option<i64>, _>("source_start_event_seq"),
                "session_projection_segments.source_start_event_seq",
            )?,
            source_end_event_seq: parse_optional_non_negative_u64(
                row.get::<Option<i64>, _>("source_end_event_seq"),
                "session_projection_segments.source_end_event_seq",
            )?,
            source_refs_json: parse_json_column(
                row.get::<String, _>("source_refs_json"),
                "session_projection_segments.source_refs_json",
            )?,
            generated_by_compaction_id: row.get::<Option<String>, _>("generated_by_compaction_id"),
            content_json: parse_json_column(
                row.get::<String, _>("content_json"),
                "session_projection_segments.content_json",
            )?,
            token_estimate: parse_optional_non_negative_u64(
                row.get::<Option<i64>, _>("token_estimate"),
                "session_projection_segments.token_estimate",
            )?,
            created_at_ms: row.get::<i64, _>("created_at_ms"),
        })
    }

    fn projection_head_from_row(
        row: &sqlx::postgres::PgRow,
    ) -> io::Result<SessionProjectionHeadRecord> {
        Ok(SessionProjectionHeadRecord {
            session_id: row.get::<String, _>("session_id"),
            projection_kind: row.get::<String, _>("projection_kind"),
            projection_version: parse_non_negative_u64(
                row.get::<i64, _>("projection_version"),
                "session_projection_heads.projection_version",
            )?,
            head_event_seq: parse_non_negative_u64(
                row.get::<i64, _>("head_event_seq"),
                "session_projection_heads.head_event_seq",
            )?,
            active_compaction_id: row.get::<Option<String>, _>("active_compaction_id"),
            updated_by_event_seq: parse_optional_non_negative_u64(
                row.get::<Option<i64>, _>("updated_by_event_seq"),
                "session_projection_heads.updated_by_event_seq",
            )?,
            updated_at_ms: row.get::<i64, _>("updated_at_ms"),
        })
    }

    fn lineage_from_row(row: &sqlx::postgres::PgRow) -> io::Result<SessionLineageRecord> {
        Ok(SessionLineageRecord {
            child_session_id: row.get::<String, _>("child_session_id"),
            parent_session_id: row.get::<String, _>("parent_session_id"),
            relation_kind: parse_lineage_relation_kind(
                row.get::<String, _>("relation_kind"),
                "session_lineage.relation_kind",
            )?,
            fork_point_event_seq: parse_optional_non_negative_u64(
                row.get::<Option<i64>, _>("fork_point_event_seq"),
                "session_lineage.fork_point_event_seq",
            )?,
            fork_point_ref_json: parse_json_column(
                row.get::<String, _>("fork_point_ref_json"),
                "session_lineage.fork_point_ref_json",
            )?,
            fork_point_compaction_id: row.get::<Option<String>, _>("fork_point_compaction_id"),
            status: parse_lineage_status(row.get::<String, _>("status"), "session_lineage.status")?,
            created_at_ms: row.get::<i64, _>("created_at_ms"),
            updated_at_ms: row.get::<i64, _>("updated_at_ms"),
            metadata_json: parse_json_column(
                row.get::<String, _>("metadata_json"),
                "session_lineage.metadata_json",
            )?,
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
        .map_err(sqlx_to_io)?;
        if (result.rows_affected() as usize) != command_ids.len() {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!(
                    "部分 runtime command 不存在: 命中 {} / 期望 {}",
                    result.rows_affected(),
                    command_ids.len()
                ),
            ));
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
impl SessionPersistence for PostgresSessionRepository {
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
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15)
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
            WHERE id = $1
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
                   executor_session_id, companion_context_json, tab_layout_json, visible_canvas_mount_ids_json,
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
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15)
            ON CONFLICT(id) DO UPDATE SET
                title = excluded.title,
                title_source = excluded.title_source,
                created_at = excluded.created_at,
                updated_at = GREATEST(sessions.updated_at, excluded.updated_at),
                last_event_seq = GREATEST(sessions.last_event_seq, excluded.last_event_seq),
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
        sqlx::query("DELETE FROM session_events WHERE session_id = $1")
            .bind(session_id)
            .execute(&mut *tx)
            .await
            .map_err(sqlx_to_io)?;
        sqlx::query("DELETE FROM session_terminal_effects WHERE session_id = $1")
            .bind(session_id)
            .execute(&mut *tx)
            .await
            .map_err(sqlx_to_io)?;
        sqlx::query("DELETE FROM session_runtime_commands WHERE session_id = $1")
            .bind(session_id)
            .execute(&mut *tx)
            .await
            .map_err(sqlx_to_io)?;
        sqlx::query(
            "DELETE FROM session_lineage WHERE child_session_id = $1 OR parent_session_id = $1",
        )
        .bind(session_id)
        .execute(&mut *tx)
        .await
        .map_err(sqlx_to_io)?;
        sqlx::query("DELETE FROM session_projection_heads WHERE session_id = $1")
            .bind(session_id)
            .execute(&mut *tx)
            .await
            .map_err(sqlx_to_io)?;
        sqlx::query("DELETE FROM session_projection_segments WHERE session_id = $1")
            .bind(session_id)
            .execute(&mut *tx)
            .await
            .map_err(sqlx_to_io)?;
        sqlx::query("DELETE FROM session_compactions WHERE session_id = $1")
            .bind(session_id)
            .execute(&mut *tx)
            .await
            .map_err(sqlx_to_io)?;
        sqlx::query("DELETE FROM sessions WHERE id = $1")
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
        .map_err(sqlx_to_io)?
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!("session {session_id} 不存在"),
            )
        })?;
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
        .map_err(sqlx_to_io)?;

        sqlx::query(
            r#"
            UPDATE sessions
            SET
                updated_at = $1,
                last_execution_status = COALESCE($2, last_execution_status),
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
            WHERE session_id = $1 AND event_seq > $2 AND event_seq <= $3
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
            WHERE session_id = $1
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
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
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
            WHERE session_id = $1 AND status = $2
            ORDER BY created_at_ms ASC
            "#,
        )
        .bind(session_id)
        .bind(RuntimeCommandStatus::Requested.as_str())
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
    ) -> io::Result<Vec<RuntimeCommandRecord>> {
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
            WHERE session_id = $1 AND id = $2
            "#,
        )
        .bind(session_id)
        .bind(compaction_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(sqlx_to_io)?;
        row.as_ref().map(Self::compaction_from_row).transpose()
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
            WHERE session_id = $1 AND projection_kind = $2
            ORDER BY projection_version ASC, created_at_ms ASC
            "#,
        )
        .bind(session_id)
        .bind(projection_kind)
        .fetch_all(&self.pool)
        .await
        .map_err(sqlx_to_io)?;
        rows.iter().map(Self::compaction_from_row).collect()
    }

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
            WHERE session_id = $1 AND projection_kind = $2 AND projection_version = $3
            ORDER BY sort_order ASC
            "#,
        )
        .bind(session_id)
        .bind(projection_kind)
        .bind(projection_version)
        .fetch_all(&self.pool)
        .await
        .map_err(sqlx_to_io)?;
        rows.iter().map(Self::projection_segment_from_row).collect()
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
            WHERE session_id = $1 AND projection_kind = $2
            "#,
        )
        .bind(session_id)
        .bind(projection_kind)
        .fetch_optional(&self.pool)
        .await
        .map_err(sqlx_to_io)?;
        row.as_ref().map(Self::projection_head_from_row).transpose()
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
        .map_err(sqlx_to_io)?
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!("session {session_id} 不存在"),
            )
        })?;
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
        .map_err(sqlx_to_io)?;

        sqlx::query(
            r#"
            UPDATE sessions
            SET
                updated_at = $1,
                last_execution_status = COALESCE($2, last_execution_status),
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
            WHERE child_session_id = $1
            "#,
        )
        .bind(child_session_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(sqlx_to_io)?;
        row.as_ref().map(Self::lineage_from_row).transpose()
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
        .map_err(sqlx_to_io)?;
        rows.iter().map(Self::lineage_from_row).collect()
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
        .map_err(sqlx_to_io)?;
        rows.iter().map(Self::lineage_from_row).collect()
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
        .map_err(sqlx_to_io)?;
        rows.iter().map(Self::lineage_from_row).collect()
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
            SET status = $1, updated_at_ms = $2
            WHERE child_session_id = $3
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

async fn insert_compaction_row(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
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
    .map_err(sqlx_to_io)?;
    Ok(())
}

async fn insert_projection_segment_row(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
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
    .map_err(sqlx_to_io)?;
    Ok(())
}

async fn upsert_projection_head_row(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
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
    .map_err(sqlx_to_io)?;
    Ok(())
}

fn title_source_to_str(source: TitleSource) -> &'static str {
    match source {
        TitleSource::Auto => "auto",
        TitleSource::Source => "source",
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

fn parse_compaction_status(value: String, field: &str) -> io::Result<SessionCompactionStatus> {
    SessionCompactionStatus::try_from(value.as_str())
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, format!("{field}: {error}")))
}

fn parse_lineage_relation_kind(
    value: String,
    field: &str,
) -> io::Result<SessionLineageRelationKind> {
    SessionLineageRelationKind::try_from(value.as_str())
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, format!("{field}: {error}")))
}

fn parse_lineage_status(value: String, field: &str) -> io::Result<SessionLineageStatus> {
    SessionLineageStatus::try_from(value.as_str())
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, format!("{field}: {error}")))
}

fn parse_title_source(value: String, field: &str) -> io::Result<TitleSource> {
    match value.as_str() {
        "auto" => Ok(TitleSource::Auto),
        "source" => Ok(TitleSource::Source),
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

fn encode_optional_u64_as_i64(value: Option<u64>, field: &str) -> io::Result<Option<i64>> {
    value
        .map(|inner| encode_u64_as_i64(inner, field))
        .transpose()
}

fn parse_non_negative_u64(value: i64, field: &str) -> io::Result<u64> {
    u64::try_from(value).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("{field} 不能为负数: {value}"),
        )
    })
}

fn parse_optional_non_negative_u64(value: Option<i64>, field: &str) -> io::Result<Option<u64>> {
    value
        .map(|inner| parse_non_negative_u64(inner, field))
        .transpose()
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

fn parse_json_column(raw: String, column: &str) -> io::Result<serde_json::Value> {
    serde_json::from_str(&raw).map_err(|error| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("解析 {column} 失败: {error}"),
        )
    })
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
        BackboneEvent::ExecutorContextCompacted(_) => "executor_context_compacted",
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
    use crate::persistence::postgres::test_pg_pool;
    use agentdash_agent_protocol::codex_app_server_protocol as codex;
    use agentdash_agent_protocol::{SourceInfo, TraceInfo};
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
        BackboneEnvelope::new(
            BackboneEvent::ItemCompleted(codex::ItemCompletedNotification {
                item: codex::ThreadItem::ContextCompaction {
                    id: item_id.to_string(),
                },
                thread_id: session_id.to_string(),
                turn_id: turn_id.to_string(),
            }),
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
            .get_session_meta(&session_id)
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
        assert_eq!(merged.last_execution_status, ExecutionStatus::Completed);
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
        assert_eq!(merged.visible_canvas_mount_ids, vec!["canvas-a"]);
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
