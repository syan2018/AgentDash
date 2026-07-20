use sqlx::PgPool;
use uuid::Uuid;

use agentdash_domain::common::error::DomainError;
use agentdash_domain::routine::{
    Routine, RoutineDispatchRefs, RoutineExecution, RoutineExecutionRepository,
    RoutineExecutionStatus, RoutineRepository,
};
use agentdash_domain::workflow::{AgentRuntimeRefs, OrchestrationBindingRefs};

use super::json_document::{from_jsonb, from_optional_jsonb, to_jsonb, to_optional_jsonb};

// ────────────────────────────── Routine ──────────────────────────────

pub struct PostgresRoutineRepository {
    pool: PgPool,
}

impl PostgresRoutineRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn initialize(&self) -> Result<(), DomainError> {
        crate::migration::assert_postgres_tables_ready(&self.pool, &["routines"]).await
    }
}

#[derive(sqlx::FromRow)]
struct RoutineRow {
    id: String,
    project_id: String,
    name: String,
    prompt_template: String,
    project_agent_id: String,
    trigger_config: serde_json::Value,
    dispatch_strategy: serde_json::Value,
    enabled: bool,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
    last_fired_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl TryFrom<RoutineRow> for Routine {
    type Error = DomainError;

    fn try_from(row: RoutineRow) -> Result<Self, Self::Error> {
        Ok(Routine {
            id: parse_uuid(&row.id, "routines.id")?,
            project_id: parse_uuid(&row.project_id, "routines.project_id")?,
            name: row.name,
            prompt_template: row.prompt_template,
            project_agent_id: parse_uuid(&row.project_agent_id, "routines.project_agent_id")?,
            trigger_config: from_jsonb(row.trigger_config, "routines.trigger_config")?,
            dispatch_strategy: from_jsonb(row.dispatch_strategy, "routines.dispatch_strategy")?,
            enabled: row.enabled,
            created_at: row.created_at,
            updated_at: row.updated_at,
            last_fired_at: row.last_fired_at,
        })
    }
}

#[async_trait::async_trait]
impl RoutineRepository for PostgresRoutineRepository {
    async fn create(&self, routine: &Routine) -> Result<(), DomainError> {
        sqlx::query(
            "INSERT INTO routines (id, project_id, name, prompt_template, project_agent_id, trigger_config, dispatch_strategy, enabled, created_at, updated_at, last_fired_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)",
        )
        .bind(routine.id.to_string())
        .bind(routine.project_id.to_string())
        .bind(&routine.name)
        .bind(&routine.prompt_template)
        .bind(routine.project_agent_id.to_string())
        .bind(to_jsonb(
            &routine.trigger_config,
            "routines.trigger_config",
        )?)
        .bind(to_jsonb(
            &routine.dispatch_strategy,
            "routines.dispatch_strategy",
        )?)
        .bind(routine.enabled)
        .bind(routine.created_at)
        .bind(routine.updated_at)
        .bind(routine.last_fired_at)
        .execute(&self.pool)
        .await
        .map_err(super::db_err)?;
        Ok(())
    }

    async fn get_by_id(&self, id: Uuid) -> Result<Option<Routine>, DomainError> {
        let row: Option<RoutineRow> = sqlx::query_as(
            "SELECT id, project_id, name, prompt_template, project_agent_id, trigger_config, dispatch_strategy, enabled, created_at, updated_at, last_fired_at
             FROM routines WHERE id = $1",
        )
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(super::db_err)?;
        row.map(Routine::try_from).transpose()
    }

    async fn list_by_project(&self, project_id: Uuid) -> Result<Vec<Routine>, DomainError> {
        let rows: Vec<RoutineRow> = sqlx::query_as(
            "SELECT id, project_id, name, prompt_template, project_agent_id, trigger_config, dispatch_strategy, enabled, created_at, updated_at, last_fired_at
             FROM routines WHERE project_id = $1 ORDER BY name",
        )
        .bind(project_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(super::db_err)?;
        rows.into_iter().map(Routine::try_from).collect()
    }

    async fn list_enabled_by_trigger_type(
        &self,
        trigger_type: &str,
    ) -> Result<Vec<Routine>, DomainError> {
        let rows: Vec<RoutineRow> = sqlx::query_as(
            "SELECT id, project_id, name, prompt_template, project_agent_id, trigger_config, dispatch_strategy, enabled, created_at, updated_at, last_fired_at
             FROM routines WHERE enabled = TRUE AND trigger_config @> $1",
        )
        .bind(serde_json::json!({"type": trigger_type}))
        .fetch_all(&self.pool)
        .await
        .map_err(super::db_err)?;
        rows.into_iter().map(Routine::try_from).collect()
    }

    async fn update(&self, routine: &Routine) -> Result<(), DomainError> {
        sqlx::query(
            "UPDATE routines SET name=$2, prompt_template=$3, project_agent_id=$4, trigger_config=$5, dispatch_strategy=$6, enabled=$7, updated_at=$8, last_fired_at=$9
             WHERE id=$1",
        )
        .bind(routine.id.to_string())
        .bind(&routine.name)
        .bind(&routine.prompt_template)
        .bind(routine.project_agent_id.to_string())
        .bind(to_jsonb(
            &routine.trigger_config,
            "routines.trigger_config",
        )?)
        .bind(to_jsonb(
            &routine.dispatch_strategy,
            "routines.dispatch_strategy",
        )?)
        .bind(routine.enabled)
        .bind(routine.updated_at)
        .bind(routine.last_fired_at)
        .execute(&self.pool)
        .await
        .map_err(super::db_err)?;
        Ok(())
    }

    async fn delete(&self, id: Uuid) -> Result<(), DomainError> {
        sqlx::query("DELETE FROM routines WHERE id = $1")
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .map_err(super::db_err)?;
        Ok(())
    }

    async fn find_by_endpoint_id(&self, endpoint_id: &str) -> Result<Option<Routine>, DomainError> {
        let row: Option<RoutineRow> = sqlx::query_as(
            "SELECT id, project_id, name, prompt_template, project_agent_id, trigger_config, dispatch_strategy, enabled, created_at, updated_at, last_fired_at
             FROM routines WHERE trigger_config @> $1 LIMIT 1",
        )
        .bind(serde_json::json!({"endpoint_id": endpoint_id}))
        .fetch_optional(&self.pool)
        .await
        .map_err(super::db_err)?;
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
        crate::migration::assert_postgres_tables_ready(&self.pool, &["routine_executions"]).await
    }
}

#[derive(sqlx::FromRow)]
struct ExecutionRow {
    id: String,
    routine_id: String,
    trigger_source: String,
    trigger_payload: Option<serde_json::Value>,
    resolved_prompt: Option<String>,
    dispatch_run_id: Option<String>,
    dispatch_agent_id: Option<String>,
    dispatch_frame_id: Option<String>,
    dispatch_orchestration_id: Option<String>,
    dispatch_node_path: Option<String>,
    dispatch_input_handoff: Option<serde_json::Value>,
    status: String,
    started_at: chrono::DateTime<chrono::Utc>,
    completed_at: Option<chrono::DateTime<chrono::Utc>>,
    error: Option<String>,
    entity_key: Option<String>,
}

impl TryFrom<ExecutionRow> for RoutineExecution {
    type Error = DomainError;

    fn try_from(row: ExecutionRow) -> Result<Self, Self::Error> {
        let dispatch_refs = match (
            row.dispatch_run_id,
            row.dispatch_agent_id,
            row.dispatch_frame_id,
            row.dispatch_orchestration_id,
            row.dispatch_node_path,
        ) {
            (Some(run_id), Some(agent_id), Some(frame_id), orchestration_id, node_path) => {
                let orchestration_binding = match (orchestration_id, node_path) {
                    (Some(orchestration_id), Some(node_path)) => {
                        Some(OrchestrationBindingRefs::new(
                            parse_uuid(
                                &orchestration_id,
                                "routine_executions.dispatch_orchestration_id",
                            )?,
                            node_path,
                            1,
                        ))
                    }
                    _ => None,
                };
                let refs = RoutineDispatchRefs::new(AgentRuntimeRefs::new(
                    parse_uuid(&run_id, "routine_executions.dispatch_run_id")?,
                    parse_uuid(&agent_id, "routine_executions.dispatch_agent_id")?,
                    parse_uuid(&frame_id, "routine_executions.dispatch_frame_id")?,
                    orchestration_binding,
                ));
                Some(match row.dispatch_input_handoff {
                    Some(input_handoff) => refs.with_input_handoff_refs(from_jsonb(
                        input_handoff,
                        "routine_executions.dispatch_input_handoff",
                    )?),
                    None => refs,
                })
            }
            _ => None,
        };
        Ok(RoutineExecution {
            id: parse_uuid(&row.id, "routine_executions.id")?,
            routine_id: parse_uuid(&row.routine_id, "routine_executions.routine_id")?,
            trigger_source: row.trigger_source,
            trigger_payload: from_optional_jsonb(
                row.trigger_payload,
                "routine_executions.trigger_payload",
            )?,
            resolved_prompt: row.resolved_prompt,
            dispatch_refs,
            status: parse_execution_status(&row.status)?,
            started_at: row.started_at,
            completed_at: row.completed_at,
            error: row.error,
            entity_key: row.entity_key,
        })
    }
}

#[async_trait::async_trait]
impl RoutineExecutionRepository for PostgresRoutineExecutionRepository {
    async fn create(&self, execution: &RoutineExecution) -> Result<(), DomainError> {
        sqlx::query(
            "INSERT INTO routine_executions (id, routine_id, trigger_source, trigger_payload, resolved_prompt, dispatch_run_id, dispatch_agent_id, dispatch_frame_id, dispatch_orchestration_id, dispatch_node_path, dispatch_input_handoff, status, started_at, completed_at, error, entity_key)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16)",
        )
        .bind(execution.id.to_string())
        .bind(execution.routine_id.to_string())
        .bind(&execution.trigger_source)
        .bind(to_optional_jsonb(
            execution.trigger_payload.as_ref(),
            "routine_executions.trigger_payload",
        )?)
        .bind(&execution.resolved_prompt)
        .bind(execution.dispatch_refs.as_ref().map(|r| r.run_id().to_string()))
        .bind(execution.dispatch_refs.as_ref().map(|r| r.agent_id().to_string()))
        .bind(execution.dispatch_refs.as_ref().map(|r| r.frame_id().to_string()))
        .bind(execution.dispatch_refs.as_ref().and_then(|r| {
            r.orchestration_id()
                .map(|orchestration_id| orchestration_id.to_string())
        }))
        .bind(
            execution
                .dispatch_refs
                .as_ref()
                .and_then(|r| r.node_path().map(str::to_string)),
        )
        .bind(to_optional_jsonb(
            execution
                .dispatch_refs
                .as_ref()
                .and_then(|refs| refs.input_handoff_refs.as_ref()),
            "routine_executions.dispatch_input_handoff",
        )?)
        .bind(status_to_str(execution.status))
        .bind(execution.started_at)
        .bind(execution.completed_at)
        .bind(&execution.error)
        .bind(&execution.entity_key)
        .execute(&self.pool)
        .await
        .map_err(super::db_err)?;
        Ok(())
    }

    async fn get_by_id(&self, id: Uuid) -> Result<Option<RoutineExecution>, DomainError> {
        let row: Option<ExecutionRow> = sqlx::query_as(
            "SELECT id, routine_id, trigger_source, trigger_payload, resolved_prompt, dispatch_run_id, dispatch_agent_id, dispatch_frame_id, dispatch_orchestration_id, dispatch_node_path, dispatch_input_handoff, status, started_at, completed_at, error, entity_key
             FROM routine_executions WHERE id = $1",
        )
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(super::db_err)?;
        row.map(RoutineExecution::try_from).transpose()
    }

    async fn update(&self, execution: &RoutineExecution) -> Result<(), DomainError> {
        sqlx::query(
            "UPDATE routine_executions SET trigger_payload=$2, resolved_prompt=$3, dispatch_run_id=$4, dispatch_agent_id=$5, dispatch_frame_id=$6, dispatch_orchestration_id=$7, dispatch_node_path=$8, dispatch_input_handoff=$9, status=$10, completed_at=$11, error=$12, entity_key=$13
             WHERE id=$1",
        )
        .bind(execution.id.to_string())
        .bind(to_optional_jsonb(
            execution.trigger_payload.as_ref(),
            "routine_executions.trigger_payload",
        )?)
        .bind(&execution.resolved_prompt)
        .bind(execution.dispatch_refs.as_ref().map(|r| r.run_id().to_string()))
        .bind(execution.dispatch_refs.as_ref().map(|r| r.agent_id().to_string()))
        .bind(execution.dispatch_refs.as_ref().map(|r| r.frame_id().to_string()))
        .bind(execution.dispatch_refs.as_ref().and_then(|r| {
            r.orchestration_id()
                .map(|orchestration_id| orchestration_id.to_string())
        }))
        .bind(
            execution
                .dispatch_refs
                .as_ref()
                .and_then(|r| r.node_path().map(str::to_string)),
        )
        .bind(to_optional_jsonb(
            execution
                .dispatch_refs
                .as_ref()
                .and_then(|refs| refs.input_handoff_refs.as_ref()),
            "routine_executions.dispatch_input_handoff",
        )?)
        .bind(status_to_str(execution.status))
        .bind(execution.completed_at)
        .bind(&execution.error)
        .bind(&execution.entity_key)
        .execute(&self.pool)
        .await
        .map_err(super::db_err)?;
        Ok(())
    }

    async fn list_by_routine(
        &self,
        routine_id: Uuid,
        limit: u32,
        offset: u32,
    ) -> Result<Vec<RoutineExecution>, DomainError> {
        let rows: Vec<ExecutionRow> = sqlx::query_as(
            "SELECT id, routine_id, trigger_source, trigger_payload, resolved_prompt, dispatch_run_id, dispatch_agent_id, dispatch_frame_id, dispatch_orchestration_id, dispatch_node_path, dispatch_input_handoff, status, started_at, completed_at, error, entity_key
             FROM routine_executions WHERE routine_id = $1 ORDER BY started_at DESC LIMIT $2 OFFSET $3",
        )
        .bind(routine_id.to_string())
        .bind(limit as i64)
        .bind(offset as i64)
        .fetch_all(&self.pool)
        .await
        .map_err(super::db_err)?;
        rows.into_iter().map(RoutineExecution::try_from).collect()
    }

    async fn list_recoverable(&self, limit: u32) -> Result<Vec<RoutineExecution>, DomainError> {
        let rows: Vec<ExecutionRow> = sqlx::query_as(
            "SELECT id, routine_id, trigger_source, trigger_payload, resolved_prompt, dispatch_run_id, dispatch_agent_id, dispatch_frame_id, dispatch_orchestration_id, dispatch_node_path, dispatch_input_handoff, status, started_at, completed_at, error, entity_key
             FROM routine_executions
             WHERE status = 'pending' AND dispatch_run_id IS NOT NULL
             ORDER BY started_at
             LIMIT $1",
        )
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await
        .map_err(super::db_err)?;
        rows.into_iter().map(RoutineExecution::try_from).collect()
    }

    async fn find_by_runtime_operation_id(
        &self,
        runtime_operation_id: &str,
    ) -> Result<Option<RoutineExecution>, DomainError> {
        let row: Option<ExecutionRow> = sqlx::query_as(
            "SELECT id, routine_id, trigger_source, trigger_payload, resolved_prompt, dispatch_run_id, dispatch_agent_id, dispatch_frame_id, dispatch_orchestration_id, dispatch_node_path, dispatch_input_handoff, status, started_at, completed_at, error, entity_key
             FROM routine_executions
             WHERE dispatch_input_handoff->>'runtime_operation_id' = $1
             LIMIT 1",
        )
        .bind(runtime_operation_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(super::db_err)?;
        row.map(RoutineExecution::try_from).transpose()
    }

    async fn find_latest_by_entity_key(
        &self,
        routine_id: Uuid,
        entity_key: &str,
    ) -> Result<Option<RoutineExecution>, DomainError> {
        let row: Option<ExecutionRow> = sqlx::query_as(
            "SELECT id, routine_id, trigger_source, trigger_payload, resolved_prompt, dispatch_run_id, dispatch_agent_id, dispatch_frame_id, dispatch_orchestration_id, dispatch_node_path, dispatch_input_handoff, status, started_at, completed_at, error, entity_key
             FROM routine_executions WHERE routine_id = $1 AND entity_key = $2 ORDER BY started_at DESC LIMIT 1",
        )
        .bind(routine_id.to_string())
        .bind(entity_key)
        .fetch_optional(&self.pool)
        .await
        .map_err(super::db_err)?;
        row.map(RoutineExecution::try_from).transpose()
    }
}

// ────────────────────────── Helpers ──────────────────────────

fn parse_uuid(raw: &str, field: &str) -> Result<Uuid, DomainError> {
    Uuid::parse_str(raw).map_err(|e| DomainError::InvalidConfig(format!("{field}: {e}")))
}

fn parse_execution_status(raw: &str) -> Result<RoutineExecutionStatus, DomainError> {
    match raw {
        "pending" => Ok(RoutineExecutionStatus::Pending),
        "dispatched" => Ok(RoutineExecutionStatus::Dispatched),
        "completed" => Ok(RoutineExecutionStatus::Completed),
        "failed" => Ok(RoutineExecutionStatus::Failed),
        "interrupted" => Ok(RoutineExecutionStatus::Interrupted),
        "skipped" => Ok(RoutineExecutionStatus::Skipped),
        other => Err(DomainError::InvalidConfig(format!(
            "routine_executions.status: unknown value `{other}`"
        ))),
    }
}

fn status_to_str(status: RoutineExecutionStatus) -> &'static str {
    match status {
        RoutineExecutionStatus::Pending => "pending",
        RoutineExecutionStatus::Dispatched => "dispatched",
        RoutineExecutionStatus::Completed => "completed",
        RoutineExecutionStatus::Failed => "failed",
        RoutineExecutionStatus::Interrupted => "interrupted",
        RoutineExecutionStatus::Skipped => "skipped",
    }
}
