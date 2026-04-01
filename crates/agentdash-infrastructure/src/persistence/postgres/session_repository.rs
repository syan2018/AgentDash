use std::io;

use agent_client_protocol::{SessionNotification, SessionUpdate};
use agentdash_acp_meta::parse_agentdash_meta;
use agentdash_application::session::{
    PersistedSessionEvent, SessionEventBacklog, SessionEventPage, SessionMeta, SessionPersistence,
};
use sqlx::{PgPool, Row};

pub struct SqliteSessionRepository {
    pool: PgPool,
}

impl SqliteSessionRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn initialize(&self) -> io::Result<()> {
        for statement in [
            r#"
            CREATE TABLE IF NOT EXISTS sessions (
                id TEXT PRIMARY KEY,
                title TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL,
                last_event_seq INTEGER NOT NULL DEFAULT 0,
                last_execution_status TEXT NOT NULL DEFAULT 'idle',
                last_turn_id TEXT,
                last_terminal_message TEXT,
                executor_config_json TEXT,
                executor_session_id TEXT,
                companion_context_json TEXT,
                visible_canvas_mount_ids_json TEXT
            )
            "#,
            r#"
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
            )
            "#,
            r#"
            CREATE INDEX IF NOT EXISTS idx_session_events_session_seq
                ON session_events(session_id, event_seq)
            "#,
            r#"
            CREATE INDEX IF NOT EXISTS idx_session_events_session_turn
                ON session_events(session_id, turn_id)
            "#,
            r#"
            CREATE INDEX IF NOT EXISTS idx_session_events_session_tool
                ON session_events(session_id, tool_call_id)
            "#,
        ] {
            sqlx::query(statement)
                .execute(&self.pool)
                .await
                .map_err(sqlx_to_io)?;
        }

        let alter_result =
            sqlx::query("ALTER TABLE sessions ADD COLUMN visible_canvas_mount_ids_json TEXT")
                .execute(&self.pool)
                .await;
        if let Err(error) = alter_result {
            let duplicate_column = match &error {
                sqlx::Error::Database(db_error) => db_error.code().as_deref() == Some("42701"),
                _ => false,
            };
            let message = error.to_string().to_ascii_lowercase();
            if !duplicate_column
                && !message.contains("duplicate column name")
                && !message.contains("already exists")
                && !message.contains("attribute")
                && !message.contains("已经存在")
            {
                return Err(sqlx_to_io(error));
            }
        }

        Ok(())
    }

    fn map_meta_row(row: &sqlx::postgres::PgRow) -> SessionMeta {
        SessionMeta {
            id: row.get::<String, _>("id"),
            title: row.get::<String, _>("title"),
            created_at: row.get::<i64, _>("created_at"),
            updated_at: row.get::<i64, _>("updated_at"),
            last_event_seq: row.get::<i64, _>("last_event_seq").max(0) as u64,
            last_execution_status: row.get::<String, _>("last_execution_status"),
            last_turn_id: row.get::<Option<String>, _>("last_turn_id"),
            last_terminal_message: row.get::<Option<String>, _>("last_terminal_message"),
            executor_config: row
                .get::<Option<String>, _>("executor_config_json")
                .and_then(|value| serde_json::from_str(&value).ok()),
            executor_session_id: row.get::<Option<String>, _>("executor_session_id"),
            companion_context: row
                .get::<Option<String>, _>("companion_context_json")
                .and_then(|value| serde_json::from_str(&value).ok()),
            visible_canvas_mount_ids: row
                .get::<Option<String>, _>("visible_canvas_mount_ids_json")
                .and_then(|value| serde_json::from_str(&value).ok())
                .unwrap_or_default(),
        }
    }

    fn persisted_event_from_row(row: &sqlx::postgres::PgRow) -> io::Result<PersistedSessionEvent> {
        let notification_json = row.get::<String, _>("notification_json");
        let notification = serde_json::from_str::<SessionNotification>(&notification_json)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error.to_string()))?;
        Ok(PersistedSessionEvent {
            session_id: row.get::<String, _>("session_id"),
            event_seq: row.get::<i64, _>("event_seq").max(0) as u64,
            occurred_at_ms: row.get::<i64, _>("occurred_at_ms"),
            committed_at_ms: row.get::<i64, _>("committed_at_ms"),
            session_update_type: row.get::<String, _>("session_update_type"),
            turn_id: row.get::<Option<String>, _>("turn_id"),
            entry_index: row
                .get::<Option<i64>, _>("entry_index")
                .map(|value| value.max(0) as u32),
            tool_call_id: row.get::<Option<String>, _>("tool_call_id"),
            notification,
        })
    }
}

#[async_trait::async_trait]
impl SessionPersistence for SqliteSessionRepository {
    async fn create_session(&self, meta: &SessionMeta) -> io::Result<()> {
        sqlx::query(
            r#"
            INSERT INTO sessions (
                id, title, created_at, updated_at, last_event_seq, last_execution_status,
                last_turn_id, last_terminal_message, executor_config_json,
                executor_session_id, companion_context_json, visible_canvas_mount_ids_json
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
            "#,
        )
        .bind(&meta.id)
        .bind(&meta.title)
        .bind(meta.created_at)
        .bind(meta.updated_at)
        .bind(i64::try_from(meta.last_event_seq).unwrap_or(i64::MAX))
        .bind(&meta.last_execution_status)
        .bind(&meta.last_turn_id)
        .bind(&meta.last_terminal_message)
        .bind(meta.executor_config.as_ref().map(json_string))
        .bind(&meta.executor_session_id)
        .bind(meta.companion_context.as_ref().map(json_string))
        .bind(Some(json_string(&meta.visible_canvas_mount_ids)))
        .execute(&self.pool)
        .await
        .map_err(sqlx_to_io)?;
        Ok(())
    }

    async fn get_session_meta(&self, session_id: &str) -> io::Result<Option<SessionMeta>> {
        let row = sqlx::query(
            r#"
            SELECT id, title, created_at, updated_at, last_event_seq, last_execution_status,
                   last_turn_id, last_terminal_message, executor_config_json,
                   executor_session_id, companion_context_json, visible_canvas_mount_ids_json
            FROM sessions
            WHERE id = $1
            "#,
        )
        .bind(session_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(sqlx_to_io)?;
        Ok(row.as_ref().map(Self::map_meta_row))
    }

    async fn list_sessions(&self) -> io::Result<Vec<SessionMeta>> {
        let rows = sqlx::query(
            r#"
            SELECT id, title, created_at, updated_at, last_event_seq, last_execution_status,
                   last_turn_id, last_terminal_message, executor_config_json,
                   executor_session_id, companion_context_json, visible_canvas_mount_ids_json
            FROM sessions
            ORDER BY updated_at DESC
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(sqlx_to_io)?;
        Ok(rows.iter().map(Self::map_meta_row).collect())
    }

    async fn save_session_meta(&self, meta: &SessionMeta) -> io::Result<()> {
        sqlx::query(
            r#"
            INSERT INTO sessions (
                id, title, created_at, updated_at, last_event_seq, last_execution_status,
                last_turn_id, last_terminal_message, executor_config_json,
                executor_session_id, companion_context_json, visible_canvas_mount_ids_json
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
            ON CONFLICT(id) DO UPDATE SET
                title = excluded.title,
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
                executor_config_json = COALESCE(excluded.executor_config_json, sessions.executor_config_json),
                executor_session_id = COALESCE(excluded.executor_session_id, sessions.executor_session_id),
                companion_context_json = COALESCE(excluded.companion_context_json, sessions.companion_context_json),
                visible_canvas_mount_ids_json = CASE
                    WHEN excluded.visible_canvas_mount_ids_json IS NULL
                        OR excluded.visible_canvas_mount_ids_json = '[]'
                        THEN sessions.visible_canvas_mount_ids_json
                    ELSE excluded.visible_canvas_mount_ids_json
                END
            "#,
        )
        .bind(&meta.id)
        .bind(&meta.title)
        .bind(meta.created_at)
        .bind(meta.updated_at)
        .bind(i64::try_from(meta.last_event_seq).unwrap_or(i64::MAX))
        .bind(&meta.last_execution_status)
        .bind(&meta.last_turn_id)
        .bind(&meta.last_terminal_message)
        .bind(meta.executor_config.as_ref().map(json_string))
        .bind(&meta.executor_session_id)
        .bind(meta.companion_context.as_ref().map(json_string))
        .bind(Some(json_string(&meta.visible_canvas_mount_ids)))
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
        notification: &SessionNotification,
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
        let event_seq = u64::try_from(event_seq_i64).unwrap_or(0);
        let projection = projection_from_notification(notification);
        let persisted = PersistedSessionEvent {
            session_id: session_id.to_string(),
            event_seq,
            occurred_at_ms: committed_at_ms,
            committed_at_ms,
            session_update_type: session_update_type_name(&notification.update).to_string(),
            turn_id: projection.turn_id.clone(),
            entry_index: projection.entry_index,
            tool_call_id: projection.tool_call_id.clone(),
            notification: notification.clone(),
        };

        sqlx::query(
            r#"
            INSERT INTO session_events (
                session_id, event_seq, occurred_at_ms, committed_at_ms,
                session_update_type, turn_id, entry_index, tool_call_id, notification_json
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            "#,
        )
        .bind(session_id)
        .bind(i64::try_from(event_seq).unwrap_or(i64::MAX))
        .bind(persisted.occurred_at_ms)
        .bind(persisted.committed_at_ms)
        .bind(&persisted.session_update_type)
        .bind(&persisted.turn_id)
        .bind(persisted.entry_index.map(i64::from))
        .bind(&persisted.tool_call_id)
        .bind(json_string(&persisted.notification))
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
        let snapshot_seq = self
            .get_session_meta(session_id)
            .await?
            .map(|meta| meta.last_event_seq)
            .unwrap_or(0);
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
        .bind(i64::try_from(after_seq).unwrap_or(i64::MAX))
        .bind(i64::try_from(snapshot_seq).unwrap_or(i64::MAX))
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
        let snapshot_seq = self
            .get_session_meta(session_id)
            .await?
            .map(|meta| meta.last_event_seq)
            .unwrap_or(0);
        let take = limit.max(1);
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
        .bind(i64::try_from(after_seq).unwrap_or(i64::MAX))
        .bind(i64::from(take) + 1)
        .fetch_all(&self.pool)
        .await
        .map_err(sqlx_to_io)?;

        let has_more = rows.len() > usize::try_from(take).unwrap_or(usize::MAX);
        let mut events = Vec::new();
        for row in rows
            .into_iter()
            .take(usize::try_from(take).unwrap_or(usize::MAX))
        {
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
}

fn json_string<T: serde::Serialize>(value: &T) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "null".to_string())
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

fn projection_from_notification(notification: &SessionNotification) -> SessionProjection {
    let turn_id = turn_id_from_update(&notification.update);
    let entry_index = entry_index_from_update(&notification.update);
    let tool_call_id = tool_call_id_from_update(&notification.update);

    let mut projection = SessionProjection {
        last_execution_status: None,
        turn_id,
        last_terminal_message: None,
        clear_terminal_message: false,
        executor_session_id: None,
        entry_index,
        tool_call_id,
    };

    if let SessionUpdate::SessionInfoUpdate(info) = &notification.update {
        if let Some((kind, message)) = turn_terminal_from_meta(info.meta.as_ref()) {
            projection.last_execution_status = Some(kind.to_string());
            projection.last_terminal_message = message;
            return projection;
        }

        if event_type_from_meta(info.meta.as_ref()).as_deref() == Some("turn_started") {
            projection.last_execution_status = Some("running".to_string());
            projection.clear_terminal_message = true;
        }

        if let Some(executor_session_id) = executor_session_from_info(info) {
            projection.executor_session_id = Some(executor_session_id);
        }
    }

    projection
}

fn turn_id_from_update(update: &SessionUpdate) -> Option<String> {
    let meta = update_meta(update);
    parse_agentdash_meta(meta?).and_then(|parsed| parsed.trace.and_then(|trace| trace.turn_id))
}

fn entry_index_from_update(update: &SessionUpdate) -> Option<u32> {
    let meta = update_meta(update);
    parse_agentdash_meta(meta?).and_then(|parsed| parsed.trace.and_then(|trace| trace.entry_index))
}

fn tool_call_id_from_update(update: &SessionUpdate) -> Option<String> {
    match update {
        SessionUpdate::ToolCall(call) => Some(call.tool_call_id.to_string()),
        SessionUpdate::ToolCallUpdate(update) => Some(update.tool_call_id.to_string()),
        _ => None,
    }
}

fn session_update_type_name(update: &SessionUpdate) -> &'static str {
    match update {
        SessionUpdate::UserMessageChunk(_) => "user_message_chunk",
        SessionUpdate::AgentMessageChunk(_) => "agent_message_chunk",
        SessionUpdate::AgentThoughtChunk(_) => "agent_thought_chunk",
        SessionUpdate::ToolCall(_) => "tool_call",
        SessionUpdate::ToolCallUpdate(_) => "tool_call_update",
        SessionUpdate::Plan(_) => "plan",
        SessionUpdate::SessionInfoUpdate(_) => "session_info_update",
        SessionUpdate::UsageUpdate(_) => "usage_update",
        SessionUpdate::AvailableCommandsUpdate(_) => "available_commands_update",
        SessionUpdate::CurrentModeUpdate(_) => "current_mode_update",
        SessionUpdate::ConfigOptionUpdate(_) => "config_option_update",
        _ => "unknown",
    }
}

fn update_meta(update: &SessionUpdate) -> Option<&agent_client_protocol::Meta> {
    match update {
        SessionUpdate::UserMessageChunk(chunk)
        | SessionUpdate::AgentMessageChunk(chunk)
        | SessionUpdate::AgentThoughtChunk(chunk) => chunk.meta.as_ref(),
        SessionUpdate::ToolCall(call) => call.meta.as_ref(),
        SessionUpdate::ToolCallUpdate(update) => update.meta.as_ref(),
        SessionUpdate::SessionInfoUpdate(info) => info.meta.as_ref(),
        _ => None,
    }
}

fn event_type_from_meta(meta: Option<&agent_client_protocol::Meta>) -> Option<String> {
    parse_agentdash_meta(meta?).and_then(|parsed| parsed.event.map(|event| event.r#type))
}

fn turn_terminal_from_meta(
    meta: Option<&agent_client_protocol::Meta>,
) -> Option<(&'static str, Option<String>)> {
    let parsed = parse_agentdash_meta(meta?)?;
    let event = parsed.event?;
    match event.r#type.as_str() {
        "turn_completed" => Some(("completed", event.message)),
        "turn_failed" => Some(("failed", event.message)),
        "turn_interrupted" => Some(("interrupted", event.message)),
        _ => None,
    }
}

fn executor_session_from_info(info: &agent_client_protocol::SessionInfoUpdate) -> Option<String> {
    let parsed = parse_agentdash_meta(info.meta.as_ref()?)?;
    let event = parsed.event?;
    if event.r#type != "executor_session_bound" {
        return None;
    }
    event
        .data
        .and_then(|data| {
            data.get("executor_session_id")
                .and_then(serde_json::Value::as_str)
                .map(ToString::to_string)
        })
        .or(event.message)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_client_protocol::{SessionId, SessionInfoUpdate, SessionNotification, SessionUpdate};
    use agentdash_acp_meta::{
        AgentDashEventV1, AgentDashMetaV1, AgentDashTraceV1, merge_agentdash_meta,
    };
    use sqlx::PgPool;

    fn turn_terminal_notification(
        session_id: &str,
        turn_id: &str,
        terminal_type: &str,
        message: &str,
    ) -> SessionNotification {
        let meta = merge_agentdash_meta(
            None,
            &AgentDashMetaV1::new()
                .event(AgentDashEventV1::new(terminal_type).message(Some(message.to_string())))
                .trace(AgentDashTraceV1 {
                    turn_id: Some(turn_id.to_string()),
                    ..AgentDashTraceV1::new()
                }),
        );
        SessionNotification::new(
            SessionId::new(session_id),
            SessionUpdate::SessionInfoUpdate(SessionInfoUpdate::new().meta(meta)),
        )
    }

    #[tokio::test]
    async fn append_event_assigns_monotonic_event_seq() {
        let database_url =
            std::env::var("TEST_DATABASE_URL").expect("运行测试前需设置 TEST_DATABASE_URL");
        let pool = PgPool::connect(&database_url)
            .await
            .expect("应能连接测试 PostgreSQL");
        let repo = SqliteSessionRepository::new(pool);
        repo.initialize().await.expect("应能初始化 session 表");

        let meta = SessionMeta {
            id: "sess-1".to_string(),
            title: "测试".to_string(),
            created_at: 1,
            updated_at: 1,
            last_event_seq: 0,
            last_execution_status: "idle".to_string(),
            last_turn_id: None,
            last_terminal_message: None,
            executor_config: None,
            executor_session_id: None,
            companion_context: None,
            visible_canvas_mount_ids: Vec::new(),
        };
        repo.create_session(&meta).await.expect("应能创建 session");

        let notification = SessionNotification::new(
            SessionId::new("sess-1"),
            SessionUpdate::SessionInfoUpdate(SessionInfoUpdate::new()),
        );
        let first = repo
            .append_event("sess-1", &notification)
            .await
            .expect("应能写入第一条事件");
        let second = repo
            .append_event("sess-1", &notification)
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
        let database_url =
            std::env::var("TEST_DATABASE_URL").expect("运行测试前需设置 TEST_DATABASE_URL");
        let pool = PgPool::connect(&database_url)
            .await
            .expect("应能连接测试 PostgreSQL");
        let repo = SqliteSessionRepository::new(pool);
        repo.initialize().await.expect("应能初始化 session 表");
        let session_id = format!(
            "sess-stale-{}",
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
        );

        let meta = SessionMeta {
            id: session_id.clone(),
            title: "测试".to_string(),
            created_at: 1,
            updated_at: 1,
            last_event_seq: 0,
            last_execution_status: "idle".to_string(),
            last_turn_id: None,
            last_terminal_message: None,
            executor_config: None,
            executor_session_id: None,
            companion_context: None,
            visible_canvas_mount_ids: Vec::new(),
        };
        repo.create_session(&meta).await.expect("应能创建 session");

        let mut stale = repo
            .get_session_meta(&session_id)
            .await
            .expect("应能读取 session meta")
            .expect("session 应存在");
        stale.updated_at = 10;
        stale.last_execution_status = "running".to_string();
        stale.last_turn_id = Some("t-old".to_string());
        stale.executor_session_id = Some("exec-1".to_string());
        stale.visible_canvas_mount_ids = vec!["canvas-a".to_string()];

        let terminal = turn_terminal_notification(&session_id, "t-new", "turn_completed", "done");
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
        assert_eq!(merged.last_execution_status, "completed");
        assert_eq!(merged.last_turn_id.as_deref(), Some("t-new"));
        assert_eq!(merged.last_terminal_message.as_deref(), Some("done"));
        assert_eq!(merged.executor_session_id.as_deref(), Some("exec-1"));
        assert_eq!(merged.visible_canvas_mount_ids, vec!["canvas-a"]);
    }
}
