use sqlx::PgPool;
use uuid::Uuid;

use agentdash_domain::common::error::DomainError;
use agentdash_domain::workflow::{
    AgentRunDeliveryAcceptedRefs, AgentRunDeliveryCommandClaim, AgentRunDeliveryCommandReceipt,
    AgentRunDeliveryCommandReceiptRepository, AgentRunDeliveryCommandStatus,
    NewAgentRunDeliveryCommandReceipt,
};

use super::{db_err, sql_err_for};

pub struct PostgresAgentRunDeliveryCommandReceiptRepository {
    pool: PgPool,
}

impl PostgresAgentRunDeliveryCommandReceiptRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn initialize(&self) -> Result<(), DomainError> {
        crate::migration::assert_postgres_tables_ready(
            &self.pool,
            &["agent_run_delivery_command_receipts"],
        )
        .await
    }

    async fn find_by_scope_command(
        &self,
        scope_kind: &str,
        scope_key: &str,
        client_command_id: &str,
    ) -> Result<Option<AgentRunDeliveryCommandReceipt>, DomainError> {
        sqlx::query_as::<_, AgentRunDeliveryCommandReceiptRow>(&format!(
            "SELECT {RECEIPT_COLS} FROM agent_run_delivery_command_receipts \
             WHERE scope_kind = $1 AND scope_key = $2 AND client_command_id = $3"
        ))
        .bind(scope_kind)
        .bind(scope_key)
        .bind(client_command_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| sql_err_for("agent_run_delivery_command_receipts", error))?
        .map(TryInto::try_into)
        .transpose()
    }
}

const RECEIPT_COLS: &str = "id,scope_kind,scope_key,client_command_id,request_digest,status,run_id,agent_id,frame_id,frame_revision,runtime_session_id,turn_id,error_message,created_at,updated_at,accepted_at,failed_at";

#[async_trait::async_trait]
impl AgentRunDeliveryCommandReceiptRepository for PostgresAgentRunDeliveryCommandReceiptRepository {
    async fn claim(
        &self,
        receipt: NewAgentRunDeliveryCommandReceipt,
    ) -> Result<AgentRunDeliveryCommandClaim, DomainError> {
        if let Some(existing) = self
            .find_by_scope_command(
                &receipt.scope_kind,
                &receipt.scope_key,
                &receipt.client_command_id,
            )
            .await?
        {
            if existing.request_digest != receipt.request_digest {
                return Err(digest_conflict(&receipt.client_command_id));
            }
            return Ok(AgentRunDeliveryCommandClaim::Duplicate(existing));
        }

        let now = chrono::Utc::now();
        let created = AgentRunDeliveryCommandReceipt {
            id: Uuid::new_v4(),
            scope_kind: receipt.scope_kind,
            scope_key: receipt.scope_key,
            client_command_id: receipt.client_command_id,
            request_digest: receipt.request_digest,
            status: AgentRunDeliveryCommandStatus::Pending,
            accepted_refs: None,
            error_message: None,
            created_at: now,
            updated_at: now,
            accepted_at: None,
            failed_at: None,
        };

        let result = sqlx::query(
            "INSERT INTO agent_run_delivery_command_receipts \
             (id,scope_kind,scope_key,client_command_id,request_digest,status,created_at,updated_at) \
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8)",
        )
        .bind(created.id.to_string())
        .bind(&created.scope_kind)
        .bind(&created.scope_key)
        .bind(&created.client_command_id)
        .bind(&created.request_digest)
        .bind(created.status.as_str())
        .bind(created.created_at)
        .bind(created.updated_at)
        .execute(&self.pool)
        .await;

        match result {
            Ok(_) => Ok(AgentRunDeliveryCommandClaim::Created(created)),
            Err(error) => {
                if let sqlx::Error::Database(db_error) = &error
                    && db_error.code().as_deref() == Some("23505")
                    && let Some(existing) = self
                        .find_by_scope_command(
                            &created.scope_kind,
                            &created.scope_key,
                            &created.client_command_id,
                        )
                        .await?
                {
                    if existing.request_digest != created.request_digest {
                        return Err(digest_conflict(&created.client_command_id));
                    }
                    return Ok(AgentRunDeliveryCommandClaim::Duplicate(existing));
                }
                Err(sql_err_for("agent_run_delivery_command_receipts", error))
            }
        }
    }

    async fn mark_accepted(
        &self,
        id: Uuid,
        accepted_refs: AgentRunDeliveryAcceptedRefs,
    ) -> Result<AgentRunDeliveryCommandReceipt, DomainError> {
        let now = chrono::Utc::now();
        sqlx::query_as::<_, AgentRunDeliveryCommandReceiptRow>(&format!(
            "UPDATE agent_run_delivery_command_receipts SET \
             status=$1,run_id=$2,agent_id=$3,frame_id=$4,frame_revision=$5,runtime_session_id=$6,\
             turn_id=$7,error_message=NULL,updated_at=$8,accepted_at=COALESCE(accepted_at,$8),failed_at=NULL \
             WHERE id=$9 RETURNING {RECEIPT_COLS}"
        ))
        .bind(AgentRunDeliveryCommandStatus::Accepted.as_str())
        .bind(accepted_refs.run_id.to_string())
        .bind(accepted_refs.agent_id.to_string())
        .bind(accepted_refs.frame_id.map(|id| id.to_string()))
        .bind(accepted_refs.frame_revision)
        .bind(accepted_refs.runtime_session_id)
        .bind(accepted_refs.turn_id)
        .bind(now)
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| sql_err_for("agent_run_delivery_command_receipts", error))?
        .ok_or_else(|| DomainError::NotFound {
            entity: "agent_run_delivery_command_receipt",
            id: id.to_string(),
        })?
        .try_into()
    }

    async fn mark_terminal_failed(
        &self,
        id: Uuid,
        error_message: String,
    ) -> Result<AgentRunDeliveryCommandReceipt, DomainError> {
        let now = chrono::Utc::now();
        sqlx::query_as::<_, AgentRunDeliveryCommandReceiptRow>(&format!(
            "UPDATE agent_run_delivery_command_receipts SET \
             status=$1,error_message=$2,updated_at=$3,failed_at=COALESCE(failed_at,$3) \
             WHERE id=$4 RETURNING {RECEIPT_COLS}"
        ))
        .bind(AgentRunDeliveryCommandStatus::TerminalFailed.as_str())
        .bind(error_message)
        .bind(now)
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| sql_err_for("agent_run_delivery_command_receipts", error))?
        .ok_or_else(|| DomainError::NotFound {
            entity: "agent_run_delivery_command_receipt",
            id: id.to_string(),
        })?
        .try_into()
    }

    async fn get(&self, id: Uuid) -> Result<Option<AgentRunDeliveryCommandReceipt>, DomainError> {
        sqlx::query_as::<_, AgentRunDeliveryCommandReceiptRow>(&format!(
            "SELECT {RECEIPT_COLS} FROM agent_run_delivery_command_receipts WHERE id = $1"
        ))
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(db_err)?
        .map(TryInto::try_into)
        .transpose()
    }
}

fn digest_conflict(client_command_id: &str) -> DomainError {
    DomainError::Conflict {
        entity: "agent_run_delivery_command_receipt",
        constraint: "request_digest",
        message: format!("client_command_id `{client_command_id}` 已用于不同请求"),
    }
}

#[derive(sqlx::FromRow)]
struct AgentRunDeliveryCommandReceiptRow {
    id: String,
    scope_kind: String,
    scope_key: String,
    client_command_id: String,
    request_digest: String,
    status: String,
    run_id: Option<String>,
    agent_id: Option<String>,
    frame_id: Option<String>,
    frame_revision: Option<i32>,
    runtime_session_id: Option<String>,
    turn_id: Option<String>,
    error_message: Option<String>,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
    accepted_at: Option<chrono::DateTime<chrono::Utc>>,
    failed_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl TryFrom<AgentRunDeliveryCommandReceiptRow> for AgentRunDeliveryCommandReceipt {
    type Error = DomainError;

    fn try_from(row: AgentRunDeliveryCommandReceiptRow) -> Result<Self, Self::Error> {
        let run_id = row
            .run_id
            .as_deref()
            .map(|raw| parse_uuid(raw, "lifecycle_run"))
            .transpose()?;
        let agent_id = row
            .agent_id
            .as_deref()
            .map(|raw| parse_uuid(raw, "lifecycle_agent"))
            .transpose()?;
        let frame_id = row
            .frame_id
            .as_deref()
            .map(|raw| parse_uuid(raw, "agent_frame"))
            .transpose()?;

        let accepted_refs = match (run_id, agent_id) {
            (Some(run_id), Some(agent_id)) => Some(AgentRunDeliveryAcceptedRefs {
                run_id,
                agent_id,
                frame_id,
                frame_revision: row.frame_revision,
                runtime_session_id: row.runtime_session_id,
                turn_id: row.turn_id,
            }),
            (None, None) => None,
            _ => {
                return Err(DomainError::InvalidConfig(
                    "agent_run_delivery_command_receipts accepted refs 不完整".to_string(),
                ));
            }
        };

        Ok(AgentRunDeliveryCommandReceipt {
            id: parse_uuid(&row.id, "agent_run_delivery_command_receipt")?,
            scope_kind: row.scope_kind,
            scope_key: row.scope_key,
            client_command_id: row.client_command_id,
            request_digest: row.request_digest,
            status: AgentRunDeliveryCommandStatus::try_from(row.status.as_str())?,
            accepted_refs,
            error_message: row.error_message,
            created_at: row.created_at,
            updated_at: row.updated_at,
            accepted_at: row.accepted_at,
            failed_at: row.failed_at,
        })
    }
}

fn parse_uuid(raw: &str, entity: &'static str) -> Result<Uuid, DomainError> {
    raw.parse()
        .map_err(|_| DomainError::InvalidConfig(format!("{entity} id 无效: {raw}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persistence::postgres::test_pg_pool;

    fn new_receipt(command_id: &str, digest: &str) -> NewAgentRunDeliveryCommandReceipt {
        NewAgentRunDeliveryCommandReceipt {
            scope_kind: "agent_run_message".to_string(),
            scope_key: "run:agent".to_string(),
            client_command_id: command_id.to_string(),
            request_digest: digest.to_string(),
        }
    }

    #[tokio::test]
    async fn command_receipt_claim_duplicate_and_conflict_roundtrip() {
        let Some(pool) = test_pg_pool("agent_run_delivery_command_receipt").await else {
            return;
        };
        let repo = PostgresAgentRunDeliveryCommandReceiptRepository::new(pool);
        repo.initialize().await.expect("initialize");

        let claim = repo
            .claim(new_receipt("cmd-1", "sha256:first"))
            .await
            .expect("claim");
        assert!(!claim.duplicate());
        let id = claim.receipt().id;

        let duplicate = repo
            .claim(new_receipt("cmd-1", "sha256:first"))
            .await
            .expect("duplicate");
        assert!(duplicate.duplicate());
        assert_eq!(duplicate.receipt().id, id);

        let conflict = repo
            .claim(new_receipt("cmd-1", "sha256:second"))
            .await
            .expect_err("digest mismatch");
        assert!(matches!(conflict, DomainError::Conflict { .. }));
    }

    #[tokio::test]
    async fn command_receipt_marks_accepted_and_terminal_failed() {
        let Some(pool) = test_pg_pool("agent_run_delivery_command_receipt_state").await else {
            return;
        };
        let repo = PostgresAgentRunDeliveryCommandReceiptRepository::new(pool.clone());
        repo.initialize().await.expect("initialize");

        let claim = repo
            .claim(new_receipt("cmd-2", "sha256:first"))
            .await
            .expect("claim");
        let project_id = Uuid::new_v4();
        let run_id = Uuid::new_v4();
        let agent_id = Uuid::new_v4();

        sqlx::query(
            "INSERT INTO lifecycle_runs \
             (id,project_id,topology,context,orchestrations,view_projection,status,execution_log,created_at,updated_at,last_activity_at) \
             VALUES ($1,$2,'graphless','{}','[]',NULL,'\"ready\"','[]',now(),now(),now())",
        )
        .bind(run_id.to_string())
        .bind(project_id.to_string())
        .execute(&pool)
        .await
        .expect("insert run");
        sqlx::query(
            "INSERT INTO lifecycle_agents \
             (id,run_id,project_id,agent_kind,agent_role,status,created_at,updated_at) \
             VALUES ($1,$2,$3,'test','primary','idle',now(),now())",
        )
        .bind(agent_id.to_string())
        .bind(run_id.to_string())
        .bind(project_id.to_string())
        .execute(&pool)
        .await
        .expect("insert agent");

        let accepted = repo
            .mark_accepted(
                claim.receipt().id,
                AgentRunDeliveryAcceptedRefs {
                    run_id,
                    agent_id,
                    frame_id: None,
                    frame_revision: None,
                    runtime_session_id: None,
                    turn_id: Some("turn-1".to_string()),
                },
            )
            .await
            .expect("accepted");
        assert_eq!(accepted.status, AgentRunDeliveryCommandStatus::Accepted);
        assert_eq!(
            accepted
                .accepted_refs
                .as_ref()
                .and_then(|refs| refs.turn_id.clone()),
            Some("turn-1".to_string())
        );

        let failed = repo
            .mark_terminal_failed(claim.receipt().id, "failed".to_string())
            .await
            .expect("failed");
        assert_eq!(failed.status, AgentRunDeliveryCommandStatus::TerminalFailed);
        assert_eq!(failed.error_message.as_deref(), Some("failed"));
    }
}
