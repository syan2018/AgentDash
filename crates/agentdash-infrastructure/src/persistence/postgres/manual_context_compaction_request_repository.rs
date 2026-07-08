use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

use agentdash_domain::common::error::DomainError;
use agentdash_domain::workflow::{
    ManualContextCompactionRequest, ManualContextCompactionRequestRepository,
    ManualContextCompactionRequestStatus, ManualContextCompactionRequestedMode,
    NewManualContextCompactionRequest,
};

use super::sql_err_for;

pub struct PostgresManualContextCompactionRequestRepository {
    pool: PgPool,
}

impl PostgresManualContextCompactionRequestRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    async fn update_status(
        &self,
        id: Uuid,
        status: ManualContextCompactionRequestStatus,
        consumed_turn_id: Option<String>,
        completed_compaction_id: Option<String>,
        compacted_until_ref: Option<Value>,
        first_kept_ref: Option<Value>,
        result_metadata: Option<Value>,
    ) -> Result<ManualContextCompactionRequest, DomainError> {
        sqlx::query_as::<_, ManualContextCompactionRequestRow>(&format!(
            "UPDATE runtime_session_compaction_requests SET \
             status=$1,consumed_turn_id=COALESCE($2,consumed_turn_id),\
             completed_compaction_id=COALESCE($3,completed_compaction_id),\
             compacted_until_ref=COALESCE($4,compacted_until_ref),\
             first_kept_ref=COALESCE($5,first_kept_ref),\
             result_metadata=COALESCE($6,result_metadata),updated_at=$7 \
             WHERE id=$8 RETURNING {COMPACTION_REQUEST_COLS}"
        ))
        .bind(status.as_str())
        .bind(consumed_turn_id)
        .bind(completed_compaction_id)
        .bind(compacted_until_ref)
        .bind(first_kept_ref)
        .bind(result_metadata)
        .bind(chrono::Utc::now())
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| sql_err_for("runtime_session_compaction_requests", error))?
        .ok_or_else(|| DomainError::NotFound {
            entity: "runtime_session_compaction_request",
            id: id.to_string(),
        })?
        .try_into()
    }
}

const COMPACTION_REQUEST_COLS: &str = "id,session_id,run_id,agent_id,command_receipt_id,status,requested_mode,keep_last_n,reserve_tokens,request_metadata,result_metadata,requested_at,updated_at,consumed_turn_id,completed_compaction_id,compacted_until_ref,first_kept_ref";

#[async_trait::async_trait]
impl ManualContextCompactionRequestRepository for PostgresManualContextCompactionRequestRepository {
    async fn create_requested(
        &self,
        request: NewManualContextCompactionRequest,
    ) -> Result<ManualContextCompactionRequest, DomainError> {
        let now = chrono::Utc::now();
        let id = Uuid::new_v4();
        let inserted = sqlx::query_as::<_, ManualContextCompactionRequestRow>(&format!(
            "INSERT INTO runtime_session_compaction_requests \
             (id,session_id,run_id,agent_id,command_receipt_id,status,requested_mode,\
              keep_last_n,reserve_tokens,request_metadata,requested_at,updated_at) \
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12) \
             ON CONFLICT (command_receipt_id) DO NOTHING \
             RETURNING {COMPACTION_REQUEST_COLS}"
        ))
        .bind(id.to_string())
        .bind(&request.session_id)
        .bind(request.run_id.to_string())
        .bind(request.agent_id.to_string())
        .bind(request.command_receipt_id.to_string())
        .bind(ManualContextCompactionRequestStatus::Requested.as_str())
        .bind(request.requested_mode.as_str())
        .bind(request.keep_last_n)
        .bind(request.reserve_tokens)
        .bind(request.request_metadata)
        .bind(now)
        .bind(now)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| sql_err_for("runtime_session_compaction_requests", error))?
        .map(TryInto::try_into)
        .transpose()?;

        if let Some(created) = inserted {
            return Ok(created);
        }

        self.get_by_command_receipt(request.command_receipt_id)
            .await?
            .ok_or_else(|| DomainError::Conflict {
                entity: "runtime_session_compaction_request",
                constraint: "command_receipt_id",
                message:
                    "context compact request insert conflicted but no existing request was found"
                        .to_string(),
            })
    }

    async fn get_by_command_receipt(
        &self,
        command_receipt_id: Uuid,
    ) -> Result<Option<ManualContextCompactionRequest>, DomainError> {
        sqlx::query_as::<_, ManualContextCompactionRequestRow>(&format!(
            "SELECT {COMPACTION_REQUEST_COLS} FROM runtime_session_compaction_requests \
             WHERE command_receipt_id = $1"
        ))
        .bind(command_receipt_id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| sql_err_for("runtime_session_compaction_requests", error))?
        .map(TryInto::try_into)
        .transpose()
    }

    async fn get_by_id(
        &self,
        id: Uuid,
    ) -> Result<Option<ManualContextCompactionRequest>, DomainError> {
        sqlx::query_as::<_, ManualContextCompactionRequestRow>(&format!(
            "SELECT {COMPACTION_REQUEST_COLS} FROM runtime_session_compaction_requests \
             WHERE id = $1"
        ))
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| sql_err_for("runtime_session_compaction_requests", error))?
        .map(TryInto::try_into)
        .transpose()
    }

    async fn find_requested_by_session(
        &self,
        session_id: &str,
    ) -> Result<Option<ManualContextCompactionRequest>, DomainError> {
        sqlx::query_as::<_, ManualContextCompactionRequestRow>(&format!(
            "SELECT {COMPACTION_REQUEST_COLS} FROM runtime_session_compaction_requests \
             WHERE session_id = $1 AND status = $2 ORDER BY requested_at ASC LIMIT 1"
        ))
        .bind(session_id)
        .bind(ManualContextCompactionRequestStatus::Requested.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| sql_err_for("runtime_session_compaction_requests", error))?
        .map(TryInto::try_into)
        .transpose()
    }

    async fn mark_consumed(
        &self,
        id: Uuid,
        turn_id: String,
    ) -> Result<ManualContextCompactionRequest, DomainError> {
        self.update_status(
            id,
            ManualContextCompactionRequestStatus::Consumed,
            Some(turn_id),
            None,
            None,
            None,
            None,
        )
        .await
    }

    async fn mark_completed(
        &self,
        id: Uuid,
        compaction_id: String,
        compacted_until_ref: Option<Value>,
        first_kept_ref: Option<Value>,
        result_metadata: Option<Value>,
    ) -> Result<ManualContextCompactionRequest, DomainError> {
        self.update_status(
            id,
            ManualContextCompactionRequestStatus::Completed,
            None,
            Some(compaction_id),
            compacted_until_ref,
            first_kept_ref,
            result_metadata,
        )
        .await
    }

    async fn mark_noop(
        &self,
        id: Uuid,
        result_metadata: Option<Value>,
    ) -> Result<ManualContextCompactionRequest, DomainError> {
        self.update_status(
            id,
            ManualContextCompactionRequestStatus::Noop,
            None,
            None,
            None,
            None,
            result_metadata,
        )
        .await
    }

    async fn mark_failed(
        &self,
        id: Uuid,
        result_metadata: Option<Value>,
    ) -> Result<ManualContextCompactionRequest, DomainError> {
        self.update_status(
            id,
            ManualContextCompactionRequestStatus::Failed,
            None,
            None,
            None,
            None,
            result_metadata,
        )
        .await
    }
}

#[derive(sqlx::FromRow)]
struct ManualContextCompactionRequestRow {
    id: String,
    session_id: String,
    run_id: String,
    agent_id: String,
    command_receipt_id: String,
    status: String,
    requested_mode: String,
    keep_last_n: Option<i32>,
    reserve_tokens: Option<i32>,
    request_metadata: Option<Value>,
    result_metadata: Option<Value>,
    requested_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
    consumed_turn_id: Option<String>,
    completed_compaction_id: Option<String>,
    compacted_until_ref: Option<Value>,
    first_kept_ref: Option<Value>,
}

impl TryFrom<ManualContextCompactionRequestRow> for ManualContextCompactionRequest {
    type Error = DomainError;

    fn try_from(row: ManualContextCompactionRequestRow) -> Result<Self, Self::Error> {
        Ok(Self {
            id: parse_uuid(&row.id, "runtime_session_compaction_request")?,
            session_id: row.session_id,
            run_id: parse_uuid(&row.run_id, "lifecycle_run")?,
            agent_id: parse_uuid(&row.agent_id, "lifecycle_agent")?,
            command_receipt_id: parse_uuid(&row.command_receipt_id, "agent_run_command_receipt")?,
            status: ManualContextCompactionRequestStatus::try_from(row.status.as_str())?,
            requested_mode: ManualContextCompactionRequestedMode::try_from(
                row.requested_mode.as_str(),
            )?,
            keep_last_n: row.keep_last_n,
            reserve_tokens: row.reserve_tokens,
            request_metadata: row.request_metadata,
            result_metadata: row.result_metadata,
            requested_at: row.requested_at,
            updated_at: row.updated_at,
            consumed_turn_id: row.consumed_turn_id,
            completed_compaction_id: row.completed_compaction_id,
            compacted_until_ref: row.compacted_until_ref,
            first_kept_ref: row.first_kept_ref,
        })
    }
}

fn parse_uuid(raw: &str, entity: &'static str) -> Result<Uuid, DomainError> {
    raw.parse()
        .map_err(|_| DomainError::InvalidConfig(format!("{entity} id 无效: {raw}")))
}
