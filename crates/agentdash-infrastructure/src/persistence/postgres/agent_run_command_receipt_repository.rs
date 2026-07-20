use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

use agentdash_diagnostics::{DiagnosticErrorContext, Subsystem, diag_error};
use agentdash_domain::common::error::DomainError;
use agentdash_domain::workflow::{
    AgentRunAcceptedRefs, AgentRunCommandClaim, AgentRunCommandReceipt,
    AgentRunCommandReceiptRepository, AgentRunCommandStatus, NewAgentRunCommandReceipt,
};

use super::{db_err, sql_err_for};

pub struct PostgresAgentRunCommandReceiptRepository {
    pool: PgPool,
}

impl PostgresAgentRunCommandReceiptRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn initialize(&self) -> Result<(), DomainError> {
        crate::migration::assert_postgres_tables_ready(
            &self.pool,
            &["agent_run_product_command_receipts"],
        )
        .await
    }

    async fn find_by_scope_command(
        &self,
        scope_kind: &str,
        scope_key: &str,
        client_command_id: &str,
    ) -> Result<Option<AgentRunCommandReceipt>, DomainError> {
        sqlx::query_as::<_, AgentRunCommandReceiptRow>(&format!(
            "SELECT {RECEIPT_COLS} FROM agent_run_product_command_receipts \
             WHERE scope_kind = $1 AND scope_key = $2 AND client_command_id = $3"
        ))
        .bind(scope_kind)
        .bind(scope_key)
        .bind(client_command_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| sql_err_for("agent_run_product_command_receipts", error))?
        .map(TryInto::try_into)
        .transpose()
    }
}

const RECEIPT_COLS: &str = "id,scope_kind,scope_key,command_kind,client_command_id,request_digest,status,mailbox_message_id,run_id,agent_id,frame_id,frame_revision,runtime_thread_id,runtime_operation_id,result_json,error_message,created_at,updated_at,accepted_at,failed_at";

#[async_trait::async_trait]
impl AgentRunCommandReceiptRepository for PostgresAgentRunCommandReceiptRepository {
    async fn claim(
        &self,
        receipt: NewAgentRunCommandReceipt,
    ) -> Result<AgentRunCommandClaim, DomainError> {
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
            return Ok(AgentRunCommandClaim::Duplicate(existing));
        }

        let now = chrono::Utc::now();
        let created = AgentRunCommandReceipt {
            id: Uuid::new_v4(),
            scope_kind: receipt.scope_kind,
            scope_key: receipt.scope_key,
            command_kind: receipt.command_kind,
            client_command_id: receipt.client_command_id,
            request_digest: receipt.request_digest,
            status: AgentRunCommandStatus::Pending,
            mailbox_message_id: None,
            accepted_refs: None,
            result_json: None,
            error_message: None,
            created_at: now,
            updated_at: now,
            accepted_at: None,
            failed_at: None,
        };

        let result = sqlx::query(
            "INSERT INTO agent_run_product_command_receipts \
             (id,scope_kind,scope_key,command_kind,client_command_id,request_digest,status,created_at,updated_at) \
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)",
        )
        .bind(created.id.to_string())
        .bind(&created.scope_kind)
        .bind(&created.scope_key)
        .bind(created.command_kind.as_str())
        .bind(&created.client_command_id)
        .bind(&created.request_digest)
        .bind(created.status.as_str())
        .bind(created.created_at)
        .bind(created.updated_at)
        .execute(&self.pool)
        .await;

        match result {
            Ok(_) => Ok(AgentRunCommandClaim::Created(created)),
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
                    return Ok(AgentRunCommandClaim::Duplicate(existing));
                }
                log_command_receipt_db_error(
                    "claim_insert",
                    Some(created.id),
                    Some(&created.scope_kind),
                    Some(&created.scope_key),
                    Some(created.command_kind.as_str()),
                    Some(&created.client_command_id),
                    &error,
                );
                Err(sql_err_for("agent_run_product_command_receipts", error))
            }
        }
    }

    async fn mark_accepted(
        &self,
        id: Uuid,
        accepted_refs: AgentRunAcceptedRefs,
    ) -> Result<AgentRunCommandReceipt, DomainError> {
        let now = chrono::Utc::now();
        sqlx::query_as::<_, AgentRunCommandReceiptRow>(&format!(
            "UPDATE agent_run_product_command_receipts SET \
             status=$1,run_id=$2,agent_id=$3,frame_id=$4,frame_revision=$5,runtime_thread_id=$6,\
             runtime_operation_id=$7,error_message=NULL,updated_at=$8,accepted_at=COALESCE(accepted_at,$8),failed_at=NULL \
             WHERE id=$9 RETURNING {RECEIPT_COLS}"
        ))
        .bind(AgentRunCommandStatus::Accepted.as_str())
        .bind(accepted_refs.run_id.to_string())
        .bind(accepted_refs.agent_id.to_string())
        .bind(accepted_refs.frame_id.map(|id| id.to_string()))
        .bind(accepted_refs.frame_revision)
        .bind(accepted_refs.runtime_thread_id)
        .bind(accepted_refs.runtime_operation_id)
        .bind(now)
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| sql_err_for("agent_run_product_command_receipts", error))?
        .ok_or_else(|| DomainError::NotFound {
            entity: "agent_run_command_receipt",
            id: id.to_string(),
        })?
        .try_into()
    }

    async fn attach_mailbox_message(
        &self,
        id: Uuid,
        mailbox_message_id: Uuid,
    ) -> Result<AgentRunCommandReceipt, DomainError> {
        sqlx::query_as::<_, AgentRunCommandReceiptRow>(&format!(
            "UPDATE agent_run_product_command_receipts SET mailbox_message_id=$1,updated_at=$2 \
             WHERE id=$3 RETURNING {RECEIPT_COLS}"
        ))
        .bind(mailbox_message_id.to_string())
        .bind(chrono::Utc::now())
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| sql_err_for("agent_run_product_command_receipts", error))?
        .ok_or_else(|| DomainError::NotFound {
            entity: "agent_run_command_receipt",
            id: id.to_string(),
        })?
        .try_into()
    }

    async fn store_result_json(
        &self,
        id: Uuid,
        result_json: Value,
    ) -> Result<AgentRunCommandReceipt, DomainError> {
        sqlx::query_as::<_, AgentRunCommandReceiptRow>(&format!(
            "UPDATE agent_run_product_command_receipts SET result_json=$1,updated_at=$2 \
             WHERE id=$3 RETURNING {RECEIPT_COLS}"
        ))
        .bind(result_json)
        .bind(chrono::Utc::now())
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| sql_err_for("agent_run_product_command_receipts", error))?
        .ok_or_else(|| DomainError::NotFound {
            entity: "agent_run_command_receipt",
            id: id.to_string(),
        })?
        .try_into()
    }

    async fn accept_with_result(
        &self,
        id: Uuid,
        accepted_refs: AgentRunAcceptedRefs,
        result_json: Value,
    ) -> Result<AgentRunCommandReceipt, DomainError> {
        let now = chrono::Utc::now();
        sqlx::query_as::<_, AgentRunCommandReceiptRow>(&format!(
            "UPDATE agent_run_product_command_receipts SET \
             status=$1,run_id=$2,agent_id=$3,frame_id=$4,frame_revision=$5,runtime_thread_id=$6,\
             runtime_operation_id=$7,result_json=$8,error_message=NULL,\
             updated_at=$9,accepted_at=COALESCE(accepted_at,$9),failed_at=NULL \
             WHERE id=$10 RETURNING {RECEIPT_COLS}"
        ))
        .bind(AgentRunCommandStatus::Accepted.as_str())
        .bind(accepted_refs.run_id.to_string())
        .bind(accepted_refs.agent_id.to_string())
        .bind(accepted_refs.frame_id.map(|id| id.to_string()))
        .bind(accepted_refs.frame_revision)
        .bind(accepted_refs.runtime_thread_id)
        .bind(accepted_refs.runtime_operation_id)
        .bind(result_json)
        .bind(now)
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| sql_err_for("agent_run_product_command_receipts", error))?
        .ok_or_else(|| DomainError::NotFound {
            entity: "agent_run_command_receipt",
            id: id.to_string(),
        })?
        .try_into()
    }

    async fn mark_terminal_failed(
        &self,
        id: Uuid,
        error_message: String,
    ) -> Result<AgentRunCommandReceipt, DomainError> {
        let now = chrono::Utc::now();
        sqlx::query_as::<_, AgentRunCommandReceiptRow>(&format!(
            "UPDATE agent_run_product_command_receipts SET \
             status=$1,error_message=$2,updated_at=$3,failed_at=COALESCE(failed_at,$3) \
             WHERE id=$4 RETURNING {RECEIPT_COLS}"
        ))
        .bind(AgentRunCommandStatus::TerminalFailed.as_str())
        .bind(error_message)
        .bind(now)
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| sql_err_for("agent_run_product_command_receipts", error))?
        .ok_or_else(|| DomainError::NotFound {
            entity: "agent_run_command_receipt",
            id: id.to_string(),
        })?
        .try_into()
    }

    async fn fail_with_result(
        &self,
        id: Uuid,
        error_message: String,
        result_json: Value,
    ) -> Result<AgentRunCommandReceipt, DomainError> {
        let now = chrono::Utc::now();
        sqlx::query_as::<_, AgentRunCommandReceiptRow>(&format!(
            "UPDATE agent_run_product_command_receipts SET \
             status=$1,error_message=$2,result_json=$3,updated_at=$4,failed_at=COALESCE(failed_at,$4) \
             WHERE id=$5 RETURNING {RECEIPT_COLS}"
        ))
        .bind(AgentRunCommandStatus::TerminalFailed.as_str())
        .bind(error_message)
        .bind(result_json)
        .bind(now)
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| sql_err_for("agent_run_product_command_receipts", error))?
        .ok_or_else(|| DomainError::NotFound {
            entity: "agent_run_command_receipt",
            id: id.to_string(),
        })?
        .try_into()
    }

    async fn get(&self, id: Uuid) -> Result<Option<AgentRunCommandReceipt>, DomainError> {
        sqlx::query_as::<_, AgentRunCommandReceiptRow>(&format!(
            "SELECT {RECEIPT_COLS} FROM agent_run_product_command_receipts WHERE id = $1"
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
        entity: "agent_run_command_receipt",
        constraint: "request_digest",
        message: format!("client_command_id `{client_command_id}` 已用于不同请求"),
    }
}

fn log_command_receipt_db_error(
    stage: &'static str,
    receipt_id: Option<Uuid>,
    scope_kind: Option<&str>,
    scope_key: Option<&str>,
    command_kind: Option<&str>,
    client_command_id: Option<&str>,
    error: &sqlx::Error,
) {
    let receipt_id = receipt_id
        .map(|value| value.to_string())
        .unwrap_or_else(|| "unavailable".to_string());
    let scope_kind = scope_kind.unwrap_or("unavailable");
    let scope_key = scope_key.unwrap_or("unavailable");
    let command_kind = command_kind.unwrap_or("unavailable");
    let client_command_id = client_command_id.unwrap_or("unavailable");
    let db_code = database_error_code(error);
    let db_constraint = database_error_constraint(error);
    let context = DiagnosticErrorContext::new("agent_run.command_receipt", stage);

    diag_error!(Error, Subsystem::AgentRun,
        context = &context,
        error = error,
        table = "agent_run_product_command_receipts",
        receipt_id = %receipt_id,
        scope_kind = %scope_kind,
        scope_key = %scope_key,
        command_kind = %command_kind,
        client_command_id = %client_command_id,
        db_code = %db_code,
        db_constraint = %db_constraint,
        "AgentRun command receipt database operation failed"
    );
}

fn database_error_code(error: &sqlx::Error) -> String {
    match error {
        sqlx::Error::Database(error) => error
            .code()
            .map(|code| code.into_owned())
            .unwrap_or_else(|| "unavailable".to_string()),
        _ => "unavailable".to_string(),
    }
}

fn database_error_constraint(error: &sqlx::Error) -> String {
    match error {
        sqlx::Error::Database(error) => error
            .constraint()
            .map(str::to_string)
            .unwrap_or_else(|| "unavailable".to_string()),
        _ => "unavailable".to_string(),
    }
}

#[derive(sqlx::FromRow)]
struct AgentRunCommandReceiptRow {
    id: String,
    scope_kind: String,
    scope_key: String,
    command_kind: String,
    client_command_id: String,
    request_digest: String,
    status: String,
    mailbox_message_id: Option<String>,
    run_id: Option<String>,
    agent_id: Option<String>,
    frame_id: Option<String>,
    frame_revision: Option<i32>,
    runtime_thread_id: Option<String>,
    runtime_operation_id: Option<String>,
    result_json: Option<Value>,
    error_message: Option<String>,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
    accepted_at: Option<chrono::DateTime<chrono::Utc>>,
    failed_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl TryFrom<AgentRunCommandReceiptRow> for AgentRunCommandReceipt {
    type Error = DomainError;

    fn try_from(row: AgentRunCommandReceiptRow) -> Result<Self, Self::Error> {
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
            (Some(run_id), Some(agent_id)) => Some(AgentRunAcceptedRefs {
                run_id,
                agent_id,
                frame_id,
                frame_revision: row.frame_revision,
                runtime_thread_id: row.runtime_thread_id,
                runtime_operation_id: row.runtime_operation_id,
            }),
            (None, None) => None,
            _ => {
                return Err(DomainError::InvalidConfig(
                    "agent_run_product_command_receipts accepted refs 不完整".to_string(),
                ));
            }
        };

        Ok(AgentRunCommandReceipt {
            id: parse_uuid(&row.id, "agent_run_command_receipt")?,
            scope_kind: row.scope_kind,
            scope_key: row.scope_key,
            command_kind: row.command_kind.as_str().try_into()?,
            client_command_id: row.client_command_id,
            request_digest: row.request_digest,
            status: AgentRunCommandStatus::try_from(row.status.as_str())?,
            mailbox_message_id: row
                .mailbox_message_id
                .as_deref()
                .map(|raw| parse_uuid(raw, "agent_run_mailbox_message"))
                .transpose()?,
            accepted_refs,
            result_json: row.result_json,
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
    use agentdash_domain::workflow::AgentRunCommandKind;
    use serde_json::json;

    fn new_receipt(command_id: &str, digest: &str) -> NewAgentRunCommandReceipt {
        NewAgentRunCommandReceipt {
            scope_kind: "agent_run_message".to_string(),
            scope_key: "run:agent".to_string(),
            command_kind: AgentRunCommandKind::MessageSubmit,
            client_command_id: command_id.to_string(),
            request_digest: digest.to_string(),
        }
    }

    fn runtime_operation_coordinates(suffix: &str) -> (String, String) {
        (
            format!("thread-receipt-{suffix}"),
            format!("operation-receipt-{suffix}"),
        )
    }

    #[tokio::test]
    async fn command_receipt_claim_duplicate_and_conflict_roundtrip() {
        let Some(pool) = test_pg_pool("agent_run_command_receipt").await else {
            return;
        };
        let repo = PostgresAgentRunCommandReceiptRepository::new(pool);
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
        let Some(pool) = test_pg_pool("agent_run_command_receipt_state").await else {
            return;
        };
        let repo = PostgresAgentRunCommandReceiptRepository::new(pool.clone());
        repo.initialize().await.expect("initialize");

        let claim = repo
            .claim(new_receipt("cmd-2", "sha256:first"))
            .await
            .expect("claim");
        let project_id = Uuid::new_v4();
        let run_id = Uuid::new_v4();
        let agent_id = Uuid::new_v4();
        let (runtime_thread_id, runtime_operation_id) = runtime_operation_coordinates("accepted");

        sqlx::query(
            "INSERT INTO lifecycle_runs \
             (id,project_id,topology,orchestrations,status,execution_log,created_at,updated_at,last_activity_at) \
             VALUES ($1,$2,'plain',$3,'ready',$4,now(),now(),now())",
        )
        .bind(run_id.to_string())
        .bind(project_id.to_string())
        .bind(json!([]))
        .bind(json!([]))
        .execute(&pool)
        .await
        .expect("insert run");
        sqlx::query(
            "INSERT INTO lifecycle_agents \
             (id,run_id,project_id,source,status,created_at,updated_at) \
             VALUES ($1,$2,$3,'unknown','idle',now(),now())",
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
                AgentRunAcceptedRefs {
                    run_id,
                    agent_id,
                    frame_id: None,
                    frame_revision: None,
                    runtime_thread_id: Some(runtime_thread_id.clone()),
                    runtime_operation_id: Some(runtime_operation_id.clone()),
                },
            )
            .await
            .expect("accepted");
        assert_eq!(accepted.status, AgentRunCommandStatus::Accepted);
        assert_eq!(
            accepted
                .accepted_refs
                .as_ref()
                .and_then(|refs| refs.runtime_operation_id.clone()),
            Some(runtime_operation_id)
        );

        let failed = repo
            .mark_terminal_failed(claim.receipt().id, "failed".to_string())
            .await
            .expect("failed");
        assert_eq!(failed.status, AgentRunCommandStatus::TerminalFailed);
        assert_eq!(failed.error_message.as_deref(), Some("failed"));

        let blocked_claim = repo
            .claim(new_receipt("cmd-blocked", "sha256:blocked"))
            .await
            .expect("claim blocked command");
        let blocked_result = json!({"outcome": "blocked", "message": "active turn"});
        let blocked = repo
            .fail_with_result(
                blocked_claim.receipt().id,
                "active turn".to_string(),
                blocked_result.clone(),
            )
            .await
            .expect("store blocked result atomically");
        assert_eq!(blocked.status, AgentRunCommandStatus::TerminalFailed);
        assert_eq!(blocked.result_json, Some(blocked_result.clone()));
        let blocked_replay = repo
            .claim(new_receipt("cmd-blocked", "sha256:blocked"))
            .await
            .expect("replay blocked command");
        assert!(blocked_replay.duplicate());
        assert_eq!(blocked_replay.receipt().result_json, Some(blocked_result));

        let mismatch_claim = repo
            .claim(new_receipt("cmd-mismatch", "sha256:mismatch"))
            .await
            .expect("claim mismatched command");
        let mismatch = repo
            .mark_accepted(
                mismatch_claim.receipt().id,
                AgentRunAcceptedRefs {
                    run_id,
                    agent_id,
                    frame_id: None,
                    frame_revision: None,
                    runtime_thread_id: None,
                    runtime_operation_id: accepted
                        .accepted_refs
                        .as_ref()
                        .and_then(|refs| refs.runtime_operation_id.clone()),
                },
            )
            .await;
        assert!(
            mismatch.is_err(),
            "Product receipt must reject an operation coordinate without its Runtime thread"
        );
    }
}
