use sqlx::PgPool;
use uuid::Uuid;

use agentdash_domain::common::error::DomainError;
use agentdash_domain::routine::{
    Routine, RoutineExecution, RoutineExecutionRepository, RoutineExecutionStatus,
    RoutineRepository,
};

// ────────────────────────────── Routine ──────────────────────────────

pub struct PostgresRoutineRepository {
    pool: PgPool,
}

impl PostgresRoutineRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn initialize(&self) -> Result<(), DomainError> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS routines (
                id TEXT PRIMARY KEY,
                project_id TEXT NOT NULL,
                name TEXT NOT NULL,
                prompt_template TEXT NOT NULL,
                agent_id TEXT NOT NULL,
                trigger_config TEXT NOT NULL,
                session_strategy TEXT NOT NULL,
                enabled BOOLEAN NOT NULL DEFAULT TRUE,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                last_fired_at TEXT,
                UNIQUE(project_id, name)
            );

            CREATE INDEX IF NOT EXISTS idx_routines_project ON routines(project_id);
            CREATE INDEX IF NOT EXISTS idx_routines_enabled ON routines(enabled);
            "#,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        Ok(())
    }
}

#[derive(sqlx::FromRow)]
struct RoutineRow {
    id: String,
    project_id: String,
    name: String,
    prompt_template: String,
    agent_id: String,
    trigger_config: String,
    session_strategy: String,
    enabled: bool,
    created_at: String,
    updated_at: String,
    last_fired_at: Option<String>,
}

impl TryFrom<RoutineRow> for Routine {
    type Error = DomainError;

    fn try_from(row: RoutineRow) -> Result<Self, Self::Error> {
        Ok(Routine {
            id: parse_uuid(&row.id, "routines.id")?,
            project_id: parse_uuid(&row.project_id, "routines.project_id")?,
            name: row.name,
            prompt_template: row.prompt_template,
            agent_id: parse_uuid(&row.agent_id, "routines.agent_id")?,
            trigger_config: parse_json_column(&row.trigger_config, "routines.trigger_config")?,
            session_strategy: parse_json_column(
                &row.session_strategy,
                "routines.session_strategy",
            )?,
            enabled: row.enabled,
            created_at: super::parse_pg_timestamp_checked(&row.created_at, "routines.created_at")?,
            updated_at: super::parse_pg_timestamp_checked(&row.updated_at, "routines.updated_at")?,
            last_fired_at: row
                .last_fired_at
                .as_deref()
                .map(|ts| super::parse_pg_timestamp_checked(ts, "routines.last_fired_at"))
                .transpose()?,
        })
    }
}

#[async_trait::async_trait]
impl RoutineRepository for PostgresRoutineRepository {
    async fn create(&self, routine: &Routine) -> Result<(), DomainError> {
        let trigger_config_json =
            serialize_json_column(&routine.trigger_config, "routines.trigger_config")?;
        let session_strategy_json =
            serialize_json_column(&routine.session_strategy, "routines.session_strategy")?;

        sqlx::query(
            "INSERT INTO routines (id, project_id, name, prompt_template, agent_id, trigger_config, session_strategy, enabled, created_at, updated_at, last_fired_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)",
        )
        .bind(routine.id.to_string())
        .bind(routine.project_id.to_string())
        .bind(&routine.name)
        .bind(&routine.prompt_template)
        .bind(routine.agent_id.to_string())
        .bind(trigger_config_json)
        .bind(session_strategy_json)
        .bind(routine.enabled)
        .bind(routine.created_at.to_rfc3339())
        .bind(routine.updated_at.to_rfc3339())
        .bind(routine.last_fired_at.map(|t| t.to_rfc3339()))
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;
        Ok(())
    }

    async fn get_by_id(&self, id: Uuid) -> Result<Option<Routine>, DomainError> {
        let row: Option<RoutineRow> = sqlx::query_as(
            "SELECT id, project_id, name, prompt_template, agent_id, trigger_config, session_strategy, enabled, created_at, updated_at, last_fired_at
             FROM routines WHERE id = $1",
        )
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;
        row.map(Routine::try_from).transpose()
    }

    async fn list_by_project(&self, project_id: Uuid) -> Result<Vec<Routine>, DomainError> {
        let rows: Vec<RoutineRow> = sqlx::query_as(
            "SELECT id, project_id, name, prompt_template, agent_id, trigger_config, session_strategy, enabled, created_at, updated_at, last_fired_at
             FROM routines WHERE project_id = $1 ORDER BY name",
        )
        .bind(project_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;
        rows.into_iter().map(Routine::try_from).collect()
    }

    async fn list_enabled_by_trigger_type(
        &self,
        trigger_type: &str,
    ) -> Result<Vec<Routine>, DomainError> {
        // 使用 PostgreSQL JSONB 包含运算符，比 TEXT LIKE 更可靠
        let containment =
            serde_json::json!({"type": trigger_type}).to_string();
        let rows: Vec<RoutineRow> = sqlx::query_as(
            "SELECT id, project_id, name, prompt_template, agent_id, trigger_config, session_strategy, enabled, created_at, updated_at, last_fired_at
             FROM routines WHERE enabled = TRUE AND trigger_config::jsonb @> $1::jsonb",
        )
        .bind(containment)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;
        rows.into_iter().map(Routine::try_from).collect()
    }

    async fn update(&self, routine: &Routine) -> Result<(), DomainError> {
        let trigger_config_json =
            serialize_json_column(&routine.trigger_config, "routines.trigger_config")?;
        let session_strategy_json =
            serialize_json_column(&routine.session_strategy, "routines.session_strategy")?;

        sqlx::query(
            "UPDATE routines SET name=$2, prompt_template=$3, agent_id=$4, trigger_config=$5, session_strategy=$6, enabled=$7, updated_at=$8, last_fired_at=$9
             WHERE id=$1",
        )
        .bind(routine.id.to_string())
        .bind(&routine.name)
        .bind(&routine.prompt_template)
        .bind(routine.agent_id.to_string())
        .bind(trigger_config_json)
        .bind(session_strategy_json)
        .bind(routine.enabled)
        .bind(routine.updated_at.to_rfc3339())
        .bind(routine.last_fired_at.map(|t| t.to_rfc3339()))
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;
        Ok(())
    }

    async fn delete(&self, id: Uuid) -> Result<(), DomainError> {
        sqlx::query("DELETE FROM routines WHERE id = $1")
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;
        Ok(())
    }

    async fn find_by_endpoint_id(&self, endpoint_id: &str) -> Result<Option<Routine>, DomainError> {
        // 使用 PostgreSQL JSONB 包含运算符精确匹配 endpoint_id
        let containment =
            serde_json::json!({"endpoint_id": endpoint_id}).to_string();
        let row: Option<RoutineRow> = sqlx::query_as(
            "SELECT id, project_id, name, prompt_template, agent_id, trigger_config, session_strategy, enabled, created_at, updated_at, last_fired_at
             FROM routines WHERE trigger_config::jsonb @> $1::jsonb LIMIT 1",
        )
        .bind(containment)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;
        row.map(Routine::try_from).transpose()
    }
}

// ────────────────────────── RoutineExecution ──────────────────────────

pub struct PostgresRoutineExecutionRepository {
    pool: PgPool,
}

impl PostgresRoutineExecutionRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn initialize(&self) -> Result<(), DomainError> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS routine_executions (
                id TEXT PRIMARY KEY,
                routine_id TEXT NOT NULL,
                trigger_source TEXT NOT NULL,
                trigger_payload TEXT,
                resolved_prompt TEXT,
                session_id TEXT,
                status TEXT NOT NULL DEFAULT 'pending',
                started_at TEXT NOT NULL,
                completed_at TEXT,
                error TEXT,
                entity_key TEXT
            );

            CREATE INDEX IF NOT EXISTS idx_routine_exec_routine ON routine_executions(routine_id);
            CREATE INDEX IF NOT EXISTS idx_routine_exec_status ON routine_executions(routine_id, status);
            CREATE INDEX IF NOT EXISTS idx_routine_exec_entity ON routine_executions(routine_id, entity_key);
            "#,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        Ok(())
    }
}

#[derive(sqlx::FromRow)]
struct ExecutionRow {
    id: String,
    routine_id: String,
    trigger_source: String,
    trigger_payload: Option<String>,
    resolved_prompt: Option<String>,
    session_id: Option<String>,
    status: String,
    started_at: String,
    completed_at: Option<String>,
    error: Option<String>,
    entity_key: Option<String>,
}

impl TryFrom<ExecutionRow> for RoutineExecution {
    type Error = DomainError;

    fn try_from(row: ExecutionRow) -> Result<Self, Self::Error> {
        Ok(RoutineExecution {
            id: parse_uuid(&row.id, "routine_executions.id")?,
            routine_id: parse_uuid(&row.routine_id, "routine_executions.routine_id")?,
            trigger_source: row.trigger_source,
            trigger_payload: row
                .trigger_payload
                .as_deref()
                .map(|s| parse_json_column(s, "routine_executions.trigger_payload"))
                .transpose()?,
            resolved_prompt: row.resolved_prompt,
            session_id: row.session_id,
            status: parse_execution_status(&row.status)?,
            started_at: super::parse_pg_timestamp_checked(
                &row.started_at,
                "routine_executions.started_at",
            )?,
            completed_at: row
                .completed_at
                .as_deref()
                .map(|ts| super::parse_pg_timestamp_checked(ts, "routine_executions.completed_at"))
                .transpose()?,
            error: row.error,
            entity_key: row.entity_key,
        })
    }
}

#[async_trait::async_trait]
impl RoutineExecutionRepository for PostgresRoutineExecutionRepository {
    async fn create(&self, execution: &RoutineExecution) -> Result<(), DomainError> {
        let trigger_payload_json = execution
            .trigger_payload
            .as_ref()
            .map(|v| serialize_json_column(v, "routine_executions.trigger_payload"))
            .transpose()?;

        sqlx::query(
            "INSERT INTO routine_executions (id, routine_id, trigger_source, trigger_payload, resolved_prompt, session_id, status, started_at, completed_at, error, entity_key)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)",
        )
        .bind(execution.id.to_string())
        .bind(execution.routine_id.to_string())
        .bind(&execution.trigger_source)
        .bind(trigger_payload_json)
        .bind(&execution.resolved_prompt)
        .bind(&execution.session_id)
        .bind(status_to_str(execution.status))
        .bind(execution.started_at.to_rfc3339())
        .bind(execution.completed_at.map(|t| t.to_rfc3339()))
        .bind(&execution.error)
        .bind(&execution.entity_key)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;
        Ok(())
    }

    async fn get_by_id(&self, id: Uuid) -> Result<Option<RoutineExecution>, DomainError> {
        let row: Option<ExecutionRow> = sqlx::query_as(
            "SELECT id, routine_id, trigger_source, trigger_payload, resolved_prompt, session_id, status, started_at, completed_at, error, entity_key
             FROM routine_executions WHERE id = $1",
        )
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;
        row.map(RoutineExecution::try_from).transpose()
    }

    async fn update(&self, execution: &RoutineExecution) -> Result<(), DomainError> {
        let trigger_payload_json = execution
            .trigger_payload
            .as_ref()
            .map(|v| serialize_json_column(v, "routine_executions.trigger_payload"))
            .transpose()?;

        sqlx::query(
            "UPDATE routine_executions SET trigger_payload=$2, resolved_prompt=$3, session_id=$4, status=$5, completed_at=$6, error=$7, entity_key=$8
             WHERE id=$1",
        )
        .bind(execution.id.to_string())
        .bind(trigger_payload_json)
        .bind(&execution.resolved_prompt)
        .bind(&execution.session_id)
        .bind(status_to_str(execution.status))
        .bind(execution.completed_at.map(|t| t.to_rfc3339()))
        .bind(&execution.error)
        .bind(&execution.entity_key)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;
        Ok(())
    }

    async fn list_by_routine(
        &self,
        routine_id: Uuid,
        limit: u32,
        offset: u32,
    ) -> Result<Vec<RoutineExecution>, DomainError> {
        let rows: Vec<ExecutionRow> = sqlx::query_as(
            "SELECT id, routine_id, trigger_source, trigger_payload, resolved_prompt, session_id, status, started_at, completed_at, error, entity_key
             FROM routine_executions WHERE routine_id = $1 ORDER BY started_at DESC LIMIT $2 OFFSET $3",
        )
        .bind(routine_id.to_string())
        .bind(limit as i64)
        .bind(offset as i64)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;
        rows.into_iter().map(RoutineExecution::try_from).collect()
    }

    async fn find_latest_by_entity_key(
        &self,
        routine_id: Uuid,
        entity_key: &str,
    ) -> Result<Option<RoutineExecution>, DomainError> {
        let row: Option<ExecutionRow> = sqlx::query_as(
            "SELECT id, routine_id, trigger_source, trigger_payload, resolved_prompt, session_id, status, started_at, completed_at, error, entity_key
             FROM routine_executions WHERE routine_id = $1 AND entity_key = $2 ORDER BY started_at DESC LIMIT 1",
        )
        .bind(routine_id.to_string())
        .bind(entity_key)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;
        row.map(RoutineExecution::try_from).transpose()
    }
}

// ────────────────────────── Helpers ──────────────────────────

fn parse_uuid(raw: &str, field: &str) -> Result<Uuid, DomainError> {
    Uuid::parse_str(raw).map_err(|e| DomainError::InvalidConfig(format!("{field}: {e}")))
}

fn parse_json_column<T: serde::de::DeserializeOwned>(
    raw: &str,
    field: &str,
) -> Result<T, DomainError> {
    serde_json::from_str(raw)
        .map_err(|error| DomainError::InvalidConfig(format!("{field}: {error}")))
}

fn serialize_json_column<T: serde::Serialize>(
    value: &T,
    field: &str,
) -> Result<String, DomainError> {
    serde_json::to_string(value)
        .map_err(|error| DomainError::InvalidConfig(format!("{field}: {error}")))
}

fn parse_execution_status(raw: &str) -> Result<RoutineExecutionStatus, DomainError> {
    match raw {
        "pending" => Ok(RoutineExecutionStatus::Pending),
        "running" => Ok(RoutineExecutionStatus::Running),
        "completed" => Ok(RoutineExecutionStatus::Completed),
        "failed" => Ok(RoutineExecutionStatus::Failed),
        "skipped" => Ok(RoutineExecutionStatus::Skipped),
        other => Err(DomainError::InvalidConfig(format!(
            "routine_executions.status: unknown value `{other}`"
        ))),
    }
}

fn status_to_str(status: RoutineExecutionStatus) -> &'static str {
    match status {
        RoutineExecutionStatus::Pending => "pending",
        RoutineExecutionStatus::Running => "running",
        RoutineExecutionStatus::Completed => "completed",
        RoutineExecutionStatus::Failed => "failed",
        RoutineExecutionStatus::Skipped => "skipped",
    }
}
