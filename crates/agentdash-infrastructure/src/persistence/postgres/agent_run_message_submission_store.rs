use agentdash_application_ports::agent_run_message_submission::{
    AgentRunAcceptedDeliveryKind, AgentRunMailboxAcceptedSettlement,
    AgentRunMailboxAcceptedSettlementResult, AgentRunMailboxDeliverySettlementPort,
    AgentRunMailboxDeliverySettlementResult, AgentRunMailboxFailedSettlement,
    AgentRunMessageAcceptanceResults, AgentRunMessageSubmissionAdmission,
    AgentRunMessageSubmissionCompletion, AgentRunMessageSubmissionReservation,
    AgentRunMessageSubmissionStore, CompleteAgentRunMessageSubmission,
    NewAgentRunMessageSubmission,
};
use agentdash_domain::agent_run_mailbox::{
    AgentRunMailboxMessage, MailboxMessageStatus, NewAgentRunMailboxMessage,
};
use agentdash_domain::common::error::DomainError;
use agentdash_domain::workflow::{
    AgentRunAcceptedRefs, AgentRunCommandReceipt, AgentRunCommandReceiptRepository,
    AgentRunCommandStatus,
};
use chrono::Utc;
use serde_json::Value;
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

use super::agent_run_command_receipt_repository::PostgresAgentRunCommandReceiptRepository;
use super::agent_run_mailbox_repository::{AgentRunMailboxMessageRow, MAILBOX_COLS};
use super::json_document::to_optional_jsonb;
use super::{db_err, sql_err_for};

pub struct PostgresAgentRunMessageSubmissionStore {
    pool: PgPool,
    receipts: PostgresAgentRunCommandReceiptRepository,
}

impl PostgresAgentRunMessageSubmissionStore {
    pub fn new(pool: PgPool) -> Self {
        Self {
            receipts: PostgresAgentRunCommandReceiptRepository::new(pool.clone()),
            pool,
        }
    }

    async fn require_receipt(&self, id: Uuid) -> Result<AgentRunCommandReceipt, DomainError> {
        self.receipts
            .get(id)
            .await?
            .ok_or_else(|| DomainError::NotFound {
                entity: "agent_run_command_receipt",
                id: id.to_string(),
            })
    }
}

#[derive(sqlx::FromRow)]
struct SubmissionReceiptStateRow {
    id: String,
    command_kind: String,
    request_digest: String,
    status: String,
    mailbox_message_id: Option<String>,
    run_id: Option<String>,
    agent_id: Option<String>,
    frame_id: Option<String>,
    frame_revision: Option<i32>,
    runtime_thread_id: Option<String>,
    runtime_operation_id: Option<String>,
    acceptance_results_json: Option<Value>,
    result_json: Option<Value>,
    error_message: Option<String>,
}

impl SubmissionReceiptStateRow {
    fn id(&self) -> Result<Uuid, DomainError> {
        parse_uuid(&self.id, "agent_run_message_submission.receipt_id")
    }

    fn mailbox_message_id(&self) -> Result<Option<Uuid>, DomainError> {
        self.mailbox_message_id
            .as_deref()
            .map(|value| parse_uuid(value, "agent_run_message_submission.mailbox_message_id"))
            .transpose()
    }

    fn accepted_refs(&self) -> Result<Option<AgentRunAcceptedRefs>, DomainError> {
        let (Some(run_id), Some(agent_id)) = (self.run_id.as_deref(), self.agent_id.as_deref())
        else {
            return Ok(None);
        };
        Ok(Some(AgentRunAcceptedRefs {
            run_id: parse_uuid(run_id, "agent_run_message_submission.run_id")?,
            agent_id: parse_uuid(agent_id, "agent_run_message_submission.agent_id")?,
            frame_id: self
                .frame_id
                .as_deref()
                .map(|value| parse_uuid(value, "agent_run_message_submission.frame_id"))
                .transpose()?,
            frame_revision: self.frame_revision,
            runtime_thread_id: self.runtime_thread_id.clone(),
            runtime_operation_id: self.runtime_operation_id.clone(),
        }))
    }
}

const SUBMISSION_RECEIPT_STATE_COLS: &str = "id,command_kind,request_digest,status,mailbox_message_id,run_id,agent_id,frame_id,frame_revision,runtime_thread_id,runtime_operation_id,acceptance_results_json,result_json,error_message";

#[async_trait::async_trait]
impl AgentRunMessageSubmissionStore for PostgresAgentRunMessageSubmissionStore {
    async fn load_receipt(
        &self,
        receipt_id: Uuid,
    ) -> Result<Option<AgentRunCommandReceipt>, DomainError> {
        self.receipts.get(receipt_id).await
    }

    async fn load_receipt_by_mailbox_message(
        &self,
        mailbox_message_id: Uuid,
    ) -> Result<Option<AgentRunCommandReceipt>, DomainError> {
        let receipt_id: Option<String> = sqlx::query_scalar(
            "SELECT id FROM agent_run_product_command_receipts WHERE mailbox_message_id=$1",
        )
        .bind(mailbox_message_id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| sql_err_for("agent_run_product_command_receipts", error))?;
        match receipt_id {
            Some(receipt_id) => {
                let receipt_id =
                    parse_uuid(&receipt_id, "agent_run_message_submission.receipt_id")?;
                self.receipts.get(receipt_id).await
            }
            None => Ok(None),
        }
    }

    async fn reserve(
        &self,
        receipt: agentdash_domain::workflow::NewAgentRunCommandReceipt,
    ) -> Result<AgentRunMessageSubmissionReservation, DomainError> {
        let mut tx = self.pool.begin().await.map_err(db_err)?;
        let receipt_id = Uuid::new_v4();
        let now = Utc::now();
        let inserted = sqlx::query(
            "INSERT INTO agent_run_product_command_receipts \
             (id,scope_kind,scope_key,command_kind,client_command_id,request_digest,status,created_at,updated_at) \
             VALUES ($1,$2,$3,$4,$5,$6,'pending',$7,$7) \
             ON CONFLICT (scope_kind,scope_key,client_command_id) DO NOTHING",
        )
        .bind(receipt_id.to_string())
        .bind(&receipt.scope_kind)
        .bind(&receipt.scope_key)
        .bind(receipt.command_kind.as_str())
        .bind(&receipt.client_command_id)
        .bind(&receipt.request_digest)
        .bind(now)
        .execute(&mut *tx)
        .await
        .map_err(|error| sql_err_for("agent_run_product_command_receipts", error))?
        .rows_affected()
            == 1;
        let state = select_receipt_for_update(
            &mut tx,
            &receipt.scope_kind,
            &receipt.scope_key,
            &receipt.client_command_id,
        )
        .await?;
        validate_receipt_identity(&state, &receipt)?;
        let receipt_id = state.id()?;
        if state.status != AgentRunCommandStatus::Pending.as_str() && state.result_json.is_none() {
            return Err(DomainError::InvalidConfig(format!(
                "settled product command receipt {receipt_id} has no immutable result"
            )));
        }
        tx.commit().await.map_err(db_err)?;
        if inserted {
            Ok(AgentRunMessageSubmissionReservation::Created { receipt_id })
        } else {
            let receipt = self.require_receipt(receipt_id).await?;
            if receipt.status == AgentRunCommandStatus::Pending {
                Ok(AgentRunMessageSubmissionReservation::ReconcileRequired { receipt })
            } else {
                Ok(AgentRunMessageSubmissionReservation::Replay { receipt })
            }
        }
    }

    async fn abandon_reservation(&self, receipt_id: Uuid) -> Result<bool, DomainError> {
        Ok(sqlx::query(
            "DELETE FROM agent_run_product_command_receipts \
             WHERE id=$1 AND status='pending' AND mailbox_message_id IS NULL \
             AND acceptance_results_json IS NULL AND result_json IS NULL",
        )
        .bind(receipt_id.to_string())
        .execute(&self.pool)
        .await
        .map_err(|error| sql_err_for("agent_run_product_command_receipts", error))?
        .rows_affected()
            == 1)
    }

    async fn fail_reservation(
        &self,
        receipt_id: Uuid,
        error_message: String,
    ) -> Result<AgentRunCommandReceipt, DomainError> {
        let mut tx = self.pool.begin().await.map_err(db_err)?;
        let state = sqlx::query_as::<_, SubmissionReceiptStateRow>(&format!(
            "SELECT {SUBMISSION_RECEIPT_STATE_COLS} FROM agent_run_product_command_receipts \
             WHERE id=$1 FOR UPDATE"
        ))
        .bind(receipt_id.to_string())
        .fetch_optional(&mut *tx)
        .await
        .map_err(|error| sql_err_for("agent_run_product_command_receipts", error))?
        .ok_or_else(|| DomainError::NotFound {
            entity: "agent_run_command_receipt",
            id: receipt_id.to_string(),
        })?;
        if state.status == AgentRunCommandStatus::Pending.as_str()
            && state.mailbox_message_id.is_none()
            && state.acceptance_results_json.is_none()
        {
            let now = Utc::now();
            sqlx::query(
                "UPDATE agent_run_product_command_receipts SET \
                 status='terminal_failed',error_message=$1,result_json=$2,updated_at=$3,\
                 failed_at=COALESCE(failed_at,$3) WHERE id=$4 AND status='pending'",
            )
            .bind(&error_message)
            .bind(serde_json::json!({ "reservation_failed": true }))
            .bind(now)
            .bind(receipt_id.to_string())
            .execute(&mut *tx)
            .await
            .map_err(|error| sql_err_for("agent_run_product_command_receipts", error))?;
        } else {
            if state.status != AgentRunCommandStatus::TerminalFailed.as_str()
                || state.error_message.as_deref() != Some(error_message.as_str())
            {
                return Err(completion_conflict(receipt_id));
            }
        }
        tx.commit().await.map_err(db_err)?;
        self.require_receipt(receipt_id).await
    }

    async fn admit(
        &self,
        mut submission: NewAgentRunMessageSubmission,
    ) -> Result<AgentRunMessageSubmissionAdmission, DomainError> {
        let mut tx = self.pool.begin().await.map_err(db_err)?;
        let receipt_id = Uuid::new_v4();
        let now = Utc::now();
        let acceptance_results_json = serde_json::to_value(&submission.acceptance_results)
            .map_err(|error| DomainError::InvalidConfig(error.to_string()))?;
        let inserted = if submission.reserved_receipt_id.is_none() {
            sqlx::query(
                "INSERT INTO agent_run_product_command_receipts \
                 (id,scope_kind,scope_key,command_kind,client_command_id,request_digest,status,acceptance_results_json,created_at,updated_at) \
                 VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$9) \
                 ON CONFLICT (scope_kind,scope_key,client_command_id) DO NOTHING",
            )
            .bind(receipt_id.to_string())
            .bind(&submission.receipt.scope_kind)
            .bind(&submission.receipt.scope_key)
            .bind(submission.receipt.command_kind.as_str())
            .bind(&submission.receipt.client_command_id)
            .bind(&submission.receipt.request_digest)
            .bind(AgentRunCommandStatus::Pending.as_str())
            .bind(&acceptance_results_json)
            .bind(now)
            .execute(&mut *tx)
            .await
            .map_err(|error| sql_err_for("agent_run_product_command_receipts", error))?
            .rows_affected()
                == 1
        } else {
            false
        };

        let mut receipt = select_receipt_for_update(
            &mut tx,
            &submission.receipt.scope_kind,
            &submission.receipt.scope_key,
            &submission.receipt.client_command_id,
        )
        .await?;
        if let Some(reserved_receipt_id) = submission.reserved_receipt_id {
            if receipt.id()? != reserved_receipt_id {
                return Err(DomainError::Conflict {
                    entity: "agent_run_command_receipt",
                    constraint: "reservation_owner",
                    message: "message admission does not own the reserved receipt".to_string(),
                });
            }
            if receipt.status == AgentRunCommandStatus::Pending.as_str()
                && receipt.acceptance_results_json.is_none()
            {
                sqlx::query(
                    "UPDATE agent_run_product_command_receipts SET acceptance_results_json=$1,updated_at=$2 \
                     WHERE id=$3 AND status='pending' AND acceptance_results_json IS NULL",
                )
                .bind(&acceptance_results_json)
                .bind(now)
                .bind(reserved_receipt_id.to_string())
                .execute(&mut *tx)
                .await
                .map_err(|error| sql_err_for("agent_run_product_command_receipts", error))?;
                receipt.acceptance_results_json = Some(acceptance_results_json.clone());
            }
        }
        validate_claim(&receipt, &submission)?;
        let receipt_id = receipt.id()?;

        if receipt.status != AgentRunCommandStatus::Pending.as_str() {
            if receipt.result_json.is_none() {
                return Err(DomainError::InvalidConfig(format!(
                    "settled product command receipt {receipt_id} has no immutable result"
                )));
            }
            tx.commit().await.map_err(db_err)?;
            return Ok(AgentRunMessageSubmissionAdmission::Replay {
                receipt: self.require_receipt(receipt_id).await?,
            });
        }
        if receipt.result_json.is_some() {
            return Err(DomainError::InvalidConfig(format!(
                "pending product command receipt {receipt_id} already contains an observable result"
            )));
        }

        let mailbox_message = if let Some(message_id) = receipt.mailbox_message_id()? {
            select_mailbox_for_update(&mut tx, message_id).await?
        } else {
            let message_id = submission.mailbox_message.id.unwrap_or_else(Uuid::new_v4);
            submission.mailbox_message.id = Some(message_id);
            let message = insert_mailbox_message(&mut tx, submission.mailbox_message).await?;
            sqlx::query(
                "UPDATE agent_run_product_command_receipts \
                 SET mailbox_message_id=$1,updated_at=$2 WHERE id=$3",
            )
            .bind(message_id.to_string())
            .bind(now)
            .bind(receipt_id.to_string())
            .execute(&mut *tx)
            .await
            .map_err(|error| sql_err_for("agent_run_product_command_receipts", error))?;
            message
        };

        tx.commit().await.map_err(db_err)?;
        if inserted || submission.reserved_receipt_id.is_some() {
            Ok(AgentRunMessageSubmissionAdmission::Created {
                receipt_id,
                mailbox_message,
            })
        } else {
            Ok(AgentRunMessageSubmissionAdmission::ReconcileRequired {
                receipt: self.require_receipt(receipt_id).await?,
                mailbox_message,
            })
        }
    }

    async fn complete_submission(
        &self,
        completion: CompleteAgentRunMessageSubmission,
    ) -> Result<AgentRunMessageSubmissionCompletion, DomainError> {
        complete_product_result(
            &self.pool,
            &self.receipts,
            completion.receipt_id,
            completion.mailbox_message_id,
            ProductCompletion::Accepted {
                accepted_refs: completion.accepted_refs,
                result_json: completion.result_json,
            },
        )
        .await
    }
}

#[async_trait::async_trait]
impl AgentRunMailboxDeliverySettlementPort for PostgresAgentRunMessageSubmissionStore {
    async fn settle_delivery_failed(
        &self,
        failure: AgentRunMailboxFailedSettlement,
    ) -> Result<AgentRunMailboxDeliverySettlementResult, DomainError> {
        let now = Utc::now();
        let mut tx = self.pool.begin().await.map_err(db_err)?;
        let attached_receipt = sqlx::query_as::<_, SubmissionReceiptStateRow>(&format!(
            "SELECT {SUBMISSION_RECEIPT_STATE_COLS} FROM agent_run_product_command_receipts \
             WHERE mailbox_message_id=$1 FOR UPDATE"
        ))
        .bind(failure.mailbox_message_id.to_string())
        .fetch_optional(&mut *tx)
        .await
        .map_err(|error| sql_err_for("agent_run_product_command_receipts", error))?;
        let message = sqlx::query_as::<_, AgentRunMailboxMessageRow>(&format!(
            "UPDATE agent_run_mailbox_messages SET \
             status='failed',last_error=$1,claim_token=NULL,claimed_at=NULL,claim_expires_at=NULL,\
             reconcile_required=false,consumed_at=COALESCE(consumed_at,$2),updated_at=$2,\
             payload_json=CASE WHEN origin='user' AND retain_payload=false THEN NULL ELSE payload_json END,\
             launch_planning_input=CASE WHEN origin='user' AND retain_payload=false THEN NULL ELSE launch_planning_input END \
             WHERE id=$3 AND claim_token=$4 AND status='consuming' \
             RETURNING {MAILBOX_COLS}"
        ))
        .bind(&failure.error_message)
        .bind(now)
        .bind(failure.mailbox_message_id.to_string())
        .bind(failure.claim_token.to_string())
        .fetch_optional(&mut *tx)
        .await
        .map_err(|error| sql_err_for("agent_run_mailbox_messages", error))?
        .ok_or_else(|| DomainError::Conflict {
            entity: "agent_run_mailbox_message",
            constraint: "delivery_failure_claim",
            message: "mailbox claim no longer owns delivery failure settlement".to_string(),
        })?
        .try_into()?;
        if let Some(receipt) = attached_receipt {
            let receipt_id = receipt.id()?;
            if receipt.status == AgentRunCommandStatus::Pending.as_str() {
                let acceptance_results: AgentRunMessageAcceptanceResults =
                    serde_json::from_value(receipt.acceptance_results_json.ok_or_else(|| {
                        DomainError::InvalidConfig(format!(
                            "pending product command receipt {receipt_id} has no delivery results"
                        ))
                    })?)
                    .map_err(|error| DomainError::InvalidConfig(error.to_string()))?;
                sqlx::query(
                    "UPDATE agent_run_product_command_receipts SET \
                     status='terminal_failed',error_message=$1,result_json=$2,acceptance_results_json=NULL,\
                     updated_at=$3,failed_at=COALESCE(failed_at,$3) WHERE id=$4 AND status='pending'",
                )
                .bind(&failure.error_message)
                .bind(acceptance_results.failed)
                .bind(now)
                .bind(receipt_id.to_string())
                .execute(&mut *tx)
                .await
                .map_err(|error| sql_err_for("agent_run_product_command_receipts", error))?;
            }
        }
        tx.commit().await.map_err(db_err)?;
        Ok(AgentRunMailboxDeliverySettlementResult { message })
    }

    async fn settle_delivery_accepted(
        &self,
        settlement: AgentRunMailboxAcceptedSettlement,
    ) -> Result<AgentRunMailboxAcceptedSettlementResult, DomainError> {
        let mailbox_status = match settlement.delivery_kind {
            AgentRunAcceptedDeliveryKind::Started => MailboxMessageStatus::Dispatched,
            AgentRunAcceptedDeliveryKind::Steered => MailboxMessageStatus::Steered,
        };
        let operation_id = settlement
            .accepted_refs
            .runtime_operation_id
            .clone()
            .ok_or_else(|| {
                DomainError::InvalidConfig(
                    "accepted mailbox settlement requires runtime_operation_id".to_string(),
                )
            })?;
        let now = Utc::now();
        let mut tx = self.pool.begin().await.map_err(db_err)?;
        // All cross-table paths lock product receipt before mailbox. Keeping a
        // single order avoids admit/complete racing settlement into a deadlock.
        let attached_receipt = sqlx::query_as::<_, SubmissionReceiptStateRow>(&format!(
            "SELECT {SUBMISSION_RECEIPT_STATE_COLS} FROM agent_run_product_command_receipts \
             WHERE mailbox_message_id=$1 FOR UPDATE"
        ))
        .bind(settlement.mailbox_message_id.to_string())
        .fetch_optional(&mut *tx)
        .await
        .map_err(|error| sql_err_for("agent_run_product_command_receipts", error))?;
        let message = sqlx::query_as::<_, AgentRunMailboxMessageRow>(&format!(
            "UPDATE agent_run_mailbox_messages SET \
             status=$1,accepted_runtime_operation_id=$2,last_error=NULL,\
             claim_token=NULL,claimed_at=NULL,claim_expires_at=NULL,\
             reconcile_required=false,consumed_at=COALESCE(consumed_at,$3),updated_at=$3,\
             payload_json=CASE WHEN origin='user' AND retain_payload=false THEN NULL ELSE payload_json END,\
             launch_planning_input=CASE WHEN origin='user' AND retain_payload=false THEN NULL ELSE launch_planning_input END \
             WHERE id=$4 AND claim_token=$5 AND status='consuming' RETURNING {MAILBOX_COLS}"
        ))
        .bind(mailbox_status.as_str())
        .bind(operation_id)
        .bind(now)
        .bind(settlement.mailbox_message_id.to_string())
        .bind(settlement.claim_token.to_string())
        .fetch_optional(&mut *tx)
        .await
        .map_err(|error| sql_err_for("agent_run_mailbox_messages", error))?
        .ok_or_else(|| DomainError::Conflict {
            entity: "agent_run_mailbox_message",
            constraint: "runtime_operation_claim",
            message: "mailbox claim no longer owns runtime operation acceptance".to_string(),
        })?
        .try_into()?;

        if let Some(receipt) = attached_receipt {
            let receipt_id = receipt.id()?;
            if receipt.status == AgentRunCommandStatus::Pending.as_str() {
                let acceptance_results: AgentRunMessageAcceptanceResults =
                    serde_json::from_value(receipt.acceptance_results_json.ok_or_else(|| {
                        DomainError::InvalidConfig(format!(
                            "pending product command receipt {receipt_id} has no acceptance results"
                        ))
                    })?)
                    .map_err(|error| DomainError::InvalidConfig(error.to_string()))?;
                let result_json = match settlement.delivery_kind {
                    AgentRunAcceptedDeliveryKind::Started => acceptance_results.started,
                    AgentRunAcceptedDeliveryKind::Steered => acceptance_results.steered,
                };
                settle_pending_accepted_receipt(
                    &mut tx,
                    receipt_id,
                    &settlement.accepted_refs,
                    &result_json,
                    now,
                )
                .await?;
            }
        }
        tx.commit().await.map_err(db_err)?;

        Ok(AgentRunMailboxAcceptedSettlementResult { message })
    }
}

async fn select_receipt_for_update(
    tx: &mut Transaction<'_, Postgres>,
    scope_kind: &str,
    scope_key: &str,
    client_command_id: &str,
) -> Result<SubmissionReceiptStateRow, DomainError> {
    sqlx::query_as::<_, SubmissionReceiptStateRow>(&format!(
        "SELECT {SUBMISSION_RECEIPT_STATE_COLS} FROM agent_run_product_command_receipts \
         WHERE scope_kind=$1 AND scope_key=$2 AND client_command_id=$3 FOR UPDATE"
    ))
    .bind(scope_kind)
    .bind(scope_key)
    .bind(client_command_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(|error| sql_err_for("agent_run_product_command_receipts", error))?
    .ok_or_else(|| DomainError::NotFound {
        entity: "agent_run_command_receipt",
        id: client_command_id.to_string(),
    })
}

fn validate_claim(
    receipt: &SubmissionReceiptStateRow,
    submission: &NewAgentRunMessageSubmission,
) -> Result<(), DomainError> {
    validate_receipt_identity(receipt, &submission.receipt)?;
    if receipt.status == AgentRunCommandStatus::Pending.as_str() {
        let expected = serde_json::to_value(&submission.acceptance_results)
            .map_err(|error| DomainError::InvalidConfig(error.to_string()))?;
        if receipt.acceptance_results_json.as_ref() != Some(&expected) {
            return Err(DomainError::Conflict {
                entity: "agent_run_command_receipt",
                constraint: "acceptance_results",
                message: format!(
                    "client_command_id `{}` has different pending acceptance results",
                    submission.receipt.client_command_id
                ),
            });
        }
    }
    Ok(())
}

fn validate_receipt_identity(
    receipt: &SubmissionReceiptStateRow,
    expected: &agentdash_domain::workflow::NewAgentRunCommandReceipt,
) -> Result<(), DomainError> {
    if receipt.request_digest != expected.request_digest
        || receipt.command_kind != expected.command_kind.as_str()
    {
        return Err(DomainError::Conflict {
            entity: "agent_run_command_receipt",
            constraint: "request_digest",
            message: format!(
                "client_command_id `{}` 已用于不同请求",
                expected.client_command_id
            ),
        });
    }
    Ok(())
}

async fn insert_mailbox_message(
    tx: &mut Transaction<'_, Postgres>,
    message: NewAgentRunMailboxMessage,
) -> Result<AgentRunMailboxMessage, DomainError> {
    let id = message.id.ok_or_else(|| {
        DomainError::InvalidConfig(
            "message submission requires preallocated mailbox id".to_string(),
        )
    })?;
    let now = Utc::now();
    let target_lock = format!("{}:{}", message.run_id, message.agent_id);
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
        .bind(target_lock)
        .execute(&mut **tx)
        .await
        .map_err(|error| sql_err_for("agent_run_mailbox_messages", error))?;
    let order_key: Option<i64> = sqlx::query_scalar(
        "SELECT MAX(order_key) FROM agent_run_mailbox_messages WHERE run_id=$1 AND agent_id=$2",
    )
    .bind(message.run_id.to_string())
    .bind(message.agent_id.to_string())
    .fetch_one(&mut **tx)
    .await
    .map_err(|error| sql_err_for("agent_run_mailbox_messages", error))?;
    let source_metadata = to_optional_jsonb(
        message.source.metadata.as_ref(),
        "agent_run_mailbox_messages.source_metadata",
    )?;
    sqlx::query_as::<_, AgentRunMailboxMessageRow>(&format!(
        "INSERT INTO agent_run_mailbox_messages \
         (id,run_id,agent_id,origin,source_namespace,source_kind,source_ref,\
          source_correlation_ref,source_actor,source_route,source_display_label_key,source_metadata,\
          delivery,delivery_json,barrier,drain_mode,status,priority,order_key,source_dedup_key,\
          delivery_request_digest,payload_json,launch_planning_input,preview,has_images,retain_payload,\
           created_at,updated_at) \
          VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20,$21,$22,$23,$24,$25,$26,$27,$27) \
         RETURNING {MAILBOX_COLS}"
    ))
    .bind(id.to_string())
    .bind(message.run_id.to_string())
    .bind(message.agent_id.to_string())
    .bind(message.origin.as_str())
    .bind(message.source.namespace)
    .bind(message.source.kind)
    .bind(message.source.source_ref)
    .bind(message.source.correlation_ref)
    .bind(message.source.actor)
    .bind(message.source.route)
    .bind(message.source.display_label_key)
    .bind(source_metadata)
    .bind(message.delivery.kind())
    .bind(message.delivery.to_json())
    .bind(message.barrier.as_str())
    .bind(message.drain_mode.as_str())
    .bind(MailboxMessageStatus::Queued.as_str())
    .bind(message.priority)
    .bind(order_key.unwrap_or(0) + 1)
    .bind(message.source_dedup_key)
    .bind(message.delivery_request_digest)
    .bind(message.payload_json)
    .bind(message.launch_planning_input)
    .bind(message.preview)
    .bind(message.has_images)
    .bind(message.retain_payload)
    .bind(now)
    .fetch_one(&mut **tx)
    .await
    .map_err(|error| sql_err_for("agent_run_mailbox_messages", error))?
    .try_into()
}

async fn select_mailbox_for_update(
    tx: &mut Transaction<'_, Postgres>,
    id: Uuid,
) -> Result<AgentRunMailboxMessage, DomainError> {
    sqlx::query_as::<_, AgentRunMailboxMessageRow>(&format!(
        "SELECT {MAILBOX_COLS} FROM agent_run_mailbox_messages WHERE id=$1 FOR UPDATE"
    ))
    .bind(id.to_string())
    .fetch_optional(&mut **tx)
    .await
    .map_err(|error| sql_err_for("agent_run_mailbox_messages", error))?
    .ok_or_else(|| DomainError::NotFound {
        entity: "agent_run_mailbox_message",
        id: id.to_string(),
    })?
    .try_into()
}

enum ProductCompletion {
    Accepted {
        accepted_refs: AgentRunAcceptedRefs,
        result_json: Value,
    },
}

async fn complete_product_result(
    pool: &PgPool,
    receipts: &PostgresAgentRunCommandReceiptRepository,
    receipt_id: Uuid,
    mailbox_message_id: Uuid,
    completion: ProductCompletion,
) -> Result<AgentRunMessageSubmissionCompletion, DomainError> {
    let mut tx = pool.begin().await.map_err(db_err)?;
    let state = sqlx::query_as::<_, SubmissionReceiptStateRow>(&format!(
        "SELECT {SUBMISSION_RECEIPT_STATE_COLS} FROM agent_run_product_command_receipts \
         WHERE id=$1 AND mailbox_message_id=$2 FOR UPDATE"
    ))
    .bind(receipt_id.to_string())
    .bind(mailbox_message_id.to_string())
    .fetch_optional(&mut *tx)
    .await
    .map_err(|error| sql_err_for("agent_run_product_command_receipts", error))?
    .ok_or_else(|| DomainError::Conflict {
        entity: "agent_run_command_receipt",
        constraint: "mailbox_message_link",
        message: "receipt is not attached to the submitted mailbox message".to_string(),
    })?;

    let replayed = match completion {
        ProductCompletion::Accepted {
            accepted_refs,
            result_json,
        } => {
            if state.status == AgentRunCommandStatus::Pending.as_str() {
                settle_pending_accepted_receipt(
                    &mut tx,
                    receipt_id,
                    &accepted_refs,
                    &result_json,
                    Utc::now(),
                )
                .await?;
                false
            } else if state.status == AgentRunCommandStatus::TerminalFailed.as_str() {
                true
            } else if state.status == AgentRunCommandStatus::Accepted.as_str()
                && state
                    .accepted_refs()?
                    .is_some_and(|refs| refs.runtime_operation_id.is_some())
            {
                // Runtime delivery froze the first-observable slot first.
                // Queued projection loses the race and replays that result.
                true
            } else if state.status == AgentRunCommandStatus::Accepted.as_str()
                && state.accepted_refs()?.as_ref() == Some(&accepted_refs)
                && state.result_json.as_ref() == Some(&result_json)
            {
                true
            } else {
                return Err(completion_conflict(receipt_id));
            }
        }
    };
    tx.commit().await.map_err(db_err)?;
    let receipt = receipts
        .get(receipt_id)
        .await?
        .ok_or_else(|| DomainError::NotFound {
            entity: "agent_run_command_receipt",
            id: receipt_id.to_string(),
        })?;
    Ok(if replayed {
        AgentRunMessageSubmissionCompletion::Replayed { receipt }
    } else {
        AgentRunMessageSubmissionCompletion::Completed { receipt }
    })
}

async fn settle_pending_accepted_receipt(
    tx: &mut Transaction<'_, Postgres>,
    receipt_id: Uuid,
    accepted_refs: &AgentRunAcceptedRefs,
    result_json: &Value,
    now: chrono::DateTime<Utc>,
) -> Result<(), DomainError> {
    let updated = sqlx::query(
        "UPDATE agent_run_product_command_receipts SET \
         status='accepted',run_id=$1,agent_id=$2,frame_id=$3,frame_revision=$4,\
         runtime_thread_id=$5,runtime_operation_id=$6,result_json=$7,acceptance_results_json=NULL,error_message=NULL,\
         updated_at=$8,accepted_at=COALESCE(accepted_at,$8),failed_at=NULL \
         WHERE id=$9 AND status='pending'",
    )
    .bind(accepted_refs.run_id.to_string())
    .bind(accepted_refs.agent_id.to_string())
    .bind(accepted_refs.frame_id.map(|id| id.to_string()))
    .bind(accepted_refs.frame_revision)
    .bind(&accepted_refs.runtime_thread_id)
    .bind(&accepted_refs.runtime_operation_id)
    .bind(result_json)
    .bind(now)
    .bind(receipt_id.to_string())
    .execute(&mut **tx)
    .await
    .map_err(|error| sql_err_for("agent_run_product_command_receipts", error))?;
    if updated.rows_affected() != 1 {
        return Err(completion_conflict(receipt_id));
    }
    Ok(())
}

fn completion_conflict(receipt_id: Uuid) -> DomainError {
    DomainError::Conflict {
        entity: "agent_run_command_receipt",
        constraint: "immutable_result",
        message: format!("receipt {receipt_id} already has a different observable result"),
    }
}

fn parse_uuid(value: &str, field: &'static str) -> Result<Uuid, DomainError> {
    Uuid::parse_str(value)
        .map_err(|error| DomainError::InvalidConfig(format!("{field} UUID 无效: {error}")))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use agentdash_application_ports::agent_run_message_submission::{
        AgentRunAcceptedDeliveryKind, AgentRunMailboxAcceptedSettlement,
        AgentRunMailboxDeliverySettlementPort, AgentRunMailboxFailedSettlement,
        AgentRunMessageAcceptanceResults, AgentRunMessageSubmissionAdmission,
        AgentRunMessageSubmissionReservation, AgentRunMessageSubmissionStore,
        CompleteAgentRunMessageSubmission, NewAgentRunMessageSubmission,
    };
    use agentdash_domain::agent_run_mailbox::{
        AgentRunMailboxClaimRequest, AgentRunMailboxRepository, ConsumptionBarrier,
        MailboxDelivery, MailboxDrainMode, MailboxMessageOrigin, MailboxMessageStatus,
        MailboxSourceIdentity, NewAgentRunMailboxMessage,
    };
    use agentdash_domain::common::error::DomainError;
    use agentdash_domain::workflow::{
        AgentRunAcceptedRefs, AgentRunCommandKind, AgentRunCommandStatus, NewAgentRunCommandReceipt,
    };
    use serde_json::json;
    use sqlx::postgres::PgConnectOptions;
    use uuid::Uuid;

    use super::PostgresAgentRunMessageSubmissionStore;
    use crate::persistence::postgres::PostgresAgentRunMailboxRepository;

    async fn test_pool() -> (
        sqlx::PgPool,
        Option<crate::postgres_runtime::PostgresRuntime>,
    ) {
        if crate::persistence::postgres::test_database_url().is_some() {
            return (
                crate::persistence::postgres::test_pg_pool("agent run message submission")
                    .await
                    .expect("configured PostgreSQL test pool"),
                None,
            );
        }
        let data_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/message-submission-postgres-tests");
        let runtime = crate::postgres_runtime::PostgresRuntime::resolve_embedded_at_data_root(
            "message-submission-tests",
            59,
            data_root,
        )
        .await
        .expect("start isolated embedded PostgreSQL");
        let database_name = format!("message_submission_{}", Uuid::new_v4().simple());
        sqlx::query(&format!("CREATE DATABASE {database_name}"))
            .execute(&runtime.pool)
            .await
            .expect("create isolated submission database");
        let options: PgConnectOptions = runtime
            .pool
            .connect_options()
            .as_ref()
            .clone()
            .database(&database_name);
        let pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(8)
            .connect_with(options)
            .await
            .expect("connect isolated submission database");
        crate::migration::run_postgres_migrations(&pool)
            .await
            .expect("run migrations");
        (pool, Some(runtime))
    }

    async fn insert_agent_run(pool: &sqlx::PgPool) -> (Uuid, Uuid) {
        let project_id = Uuid::new_v4();
        let run_id = Uuid::new_v4();
        let agent_id = Uuid::new_v4();
        sqlx::query("INSERT INTO projects (id,name,description,config,created_at,updated_at) VALUES ($1,'submission test','','{}',now(),now())")
            .bind(project_id.to_string()).execute(pool).await.expect("insert project");
        sqlx::query("INSERT INTO lifecycle_runs (id,project_id,topology,orchestrations,status,execution_log,created_at,updated_at,last_activity_at) VALUES ($1,$2,'plain',$3,'ready',$4,now(),now(),now())")
            .bind(run_id.to_string()).bind(project_id.to_string()).bind(json!([])).bind(json!([]))
            .execute(pool).await.expect("insert run");
        sqlx::query("INSERT INTO lifecycle_agents (id,run_id,project_id,source,status,created_at,updated_at) VALUES ($1,$2,$3,'unknown','idle',now(),now())")
            .bind(agent_id.to_string()).bind(run_id.to_string()).bind(project_id.to_string())
            .execute(pool).await.expect("insert agent");
        (run_id, agent_id)
    }

    fn runtime_operation_coordinates() -> (String, String) {
        let suffix = Uuid::new_v4().simple().to_string();
        let thread_id = format!("thread-submission-{suffix}");
        let operation_id = format!("operation-submission-{suffix}");
        (thread_id, operation_id)
    }

    fn delivery_results() -> AgentRunMessageAcceptanceResults {
        AgentRunMessageAcceptanceResults {
            started: json!({"outcome":"launched","command_receipt":{"duplicate":false}}),
            steered: json!({"outcome":"steered","command_receipt":{"duplicate":false}}),
            failed: json!({"outcome":"failed","command_receipt":{"duplicate":false}}),
        }
    }

    fn submission(
        run_id: Uuid,
        agent_id: Uuid,
        client_command_id: &str,
        digest: &str,
        retain_payload: bool,
    ) -> NewAgentRunMessageSubmission {
        NewAgentRunMessageSubmission {
            receipt: NewAgentRunCommandReceipt {
                scope_kind: "agent_run".to_string(),
                scope_key: format!("{run_id}:{agent_id}"),
                command_kind: AgentRunCommandKind::MessageSubmit,
                client_command_id: client_command_id.to_string(),
                request_digest: digest.to_string(),
            },
            reserved_receipt_id: None,
            mailbox_message: NewAgentRunMailboxMessage {
                id: Some(Uuid::new_v4()),
                run_id,
                agent_id,
                origin: MailboxMessageOrigin::User,
                source: MailboxSourceIdentity::composer(),
                delivery: MailboxDelivery::LaunchOrContinueTurn,
                barrier: ConsumptionBarrier::ImmediateIfIdle,
                drain_mode: MailboxDrainMode::One,
                priority: 0,
                source_dedup_key: Some(format!("submission:{client_command_id}")),
                delivery_request_digest: format!("sha256:{digest}"),
                payload_json: Some(json!([{"type":"text","text":"secret"}])),
                launch_planning_input: Some(json!({"input":"secret"})),
                preview: "secret".to_string(),
                has_images: false,
                retain_payload,
            },
            acceptance_results: delivery_results(),
        }
    }

    async fn claim(
        mailbox: &PostgresAgentRunMailboxRepository,
        run_id: Uuid,
        agent_id: Uuid,
    ) -> (Uuid, uuid::Uuid) {
        let token = Uuid::new_v4();
        let message = mailbox
            .claim_next(AgentRunMailboxClaimRequest {
                run_id,
                agent_id,
                barriers: vec![ConsumptionBarrier::ImmediateIfIdle],
                drain_mode: Some(MailboxDrainMode::One),
                limit: 1,
                claim_token: token,
                claim_expires_at: chrono::Utc::now() + chrono::Duration::seconds(60),
            })
            .await
            .expect("claim message")
            .into_iter()
            .next()
            .expect("claimed message");
        (message.id, token)
    }

    #[tokio::test]
    async fn submission_uow_preserves_idempotency_settlement_locking_and_retention() {
        let (pool, _runtime) = test_pool().await;
        let store = Arc::new(PostgresAgentRunMessageSubmissionStore::new(pool.clone()));
        let mailbox = PostgresAgentRunMailboxRepository::new(pool.clone());
        let (run_id, agent_id) = insert_agent_run(&pool).await;

        let same_a = submission(run_id, agent_id, "same", "sha256:same", true);
        let same_b = submission(run_id, agent_id, "same", "sha256:same", true);
        let (left, right) = tokio::join!(store.admit(same_a), store.admit(same_b));
        let admissions = [
            left.expect("left admission"),
            right.expect("right admission"),
        ];
        let message_ids = admissions
            .iter()
            .map(|admission| match admission {
                AgentRunMessageSubmissionAdmission::Created {
                    mailbox_message, ..
                }
                | AgentRunMessageSubmissionAdmission::ReconcileRequired {
                    mailbox_message, ..
                } => mailbox_message.id,
                AgentRunMessageSubmissionAdmission::Replay { .. } => {
                    panic!("pending concurrent admission cannot replay")
                }
            })
            .collect::<Vec<_>>();
        assert_eq!(message_ids[0], message_ids[1]);
        let receipt_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM agent_run_product_command_receipts WHERE scope_key=$1 AND client_command_id='same'")
            .bind(format!("{run_id}:{agent_id}")).fetch_one(&pool).await.expect("count receipts");
        let message_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM agent_run_mailbox_messages WHERE source_dedup_key='submission:same'")
            .fetch_one(&pool).await.expect("count messages");
        assert_eq!((receipt_count, message_count), (1, 1));

        let conflict = store
            .admit(submission(
                run_id,
                agent_id,
                "same",
                "sha256:different",
                true,
            ))
            .await
            .expect_err("same command id with another digest must conflict");
        assert!(matches!(conflict, DomainError::Conflict { .. }));

        let rejected_receipt = NewAgentRunCommandReceipt {
            scope_kind: "agent_run".to_string(),
            scope_key: format!("{run_id}:{agent_id}"),
            command_kind: AgentRunCommandKind::MessageSubmit,
            client_command_id: "mutable-guard-retry".to_string(),
            request_digest: "sha256:stable-semantic-payload".to_string(),
        };
        let first_reservation = store
            .reserve(rejected_receipt.clone())
            .await
            .expect("reserve before mutable guard");
        let AgentRunMessageSubmissionReservation::Created {
            receipt_id: rejected_receipt_id,
        } = first_reservation
        else {
            panic!("first semantic command must reserve")
        };
        assert!(
            store
                .abandon_reservation(rejected_receipt_id)
                .await
                .expect("abandon side-effect-free rejection")
        );
        assert!(matches!(
            store
                .reserve(rejected_receipt)
                .await
                .expect("same semantic command with refreshed guard can reserve again"),
            AgentRunMessageSubmissionReservation::Created { .. }
        ));

        let broken = store
            .admit(submission(
                Uuid::new_v4(),
                Uuid::new_v4(),
                "rollback",
                "sha256:rollback",
                true,
            ))
            .await
            .expect_err("mailbox foreign key must fail the whole admission");
        assert!(!broken.to_string().is_empty());
        let partial_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM agent_run_product_command_receipts WHERE client_command_id='rollback'")
            .fetch_one(&pool).await.expect("count partial receipts");
        assert_eq!(partial_count, 0);

        let queued = match &admissions[0] {
            AgentRunMessageSubmissionAdmission::Created {
                receipt_id,
                mailbox_message,
            } => (*receipt_id, mailbox_message.id),
            AgentRunMessageSubmissionAdmission::ReconcileRequired {
                receipt,
                mailbox_message,
            } => (receipt.id, mailbox_message.id),
            AgentRunMessageSubmissionAdmission::Replay { .. } => unreachable!(),
        };
        let queued_json = json!({"outcome":"queued","mailbox_message_id":queued.1});
        store
            .complete_submission(CompleteAgentRunMessageSubmission {
                receipt_id: queued.0,
                mailbox_message_id: queued.1,
                accepted_refs: AgentRunAcceptedRefs {
                    run_id,
                    agent_id,
                    frame_id: None,
                    frame_revision: None,
                    runtime_thread_id: None,
                    runtime_operation_id: None,
                },
                result_json: queued_json.clone(),
            })
            .await
            .expect("freeze queued result");
        let replay = store
            .admit(submission(run_id, agent_id, "same", "sha256:same", true))
            .await
            .expect("replay queued result");
        let AgentRunMessageSubmissionAdmission::Replay { receipt } = replay else {
            panic!("settled command must replay")
        };
        assert_eq!(receipt.result_json, Some(queued_json));
        mailbox
            .delete_message(queued.1)
            .await
            .expect("remove queued fixture before delivery tests");

        let accepted = store
            .admit(submission(
                run_id,
                agent_id,
                "accepted",
                "sha256:accepted",
                false,
            ))
            .await
            .expect("admit accepted fixture");
        let accepted_message_id = match accepted {
            AgentRunMessageSubmissionAdmission::Created {
                mailbox_message, ..
            } => mailbox_message.id,
            other => panic!("unexpected admission: {other:?}"),
        };
        let (claimed_id, claim_token) = claim(&mailbox, run_id, agent_id).await;
        assert_eq!(claimed_id, accepted_message_id);
        let (thread_id, operation_id) = runtime_operation_coordinates();
        let accepted_result = store
            .settle_delivery_accepted(AgentRunMailboxAcceptedSettlement {
                mailbox_message_id: accepted_message_id,
                claim_token,
                delivery_kind: AgentRunAcceptedDeliveryKind::Started,
                accepted_refs: AgentRunAcceptedRefs {
                    run_id,
                    agent_id,
                    frame_id: None,
                    frame_revision: None,
                    runtime_thread_id: Some(thread_id),
                    runtime_operation_id: Some(operation_id),
                },
            })
            .await
            .expect("settle accepted delivery");
        assert_eq!(
            accepted_result.message.status,
            MailboxMessageStatus::Dispatched
        );
        assert_eq!(
            store
                .load_receipt_by_mailbox_message(accepted_message_id)
                .await
                .expect("load accepted product receipt")
                .expect("accepted product receipt")
                .result_json,
            Some(delivery_results().started)
        );
        assert!(accepted_result.message.payload_json.is_none());
        assert!(accepted_result.message.launch_planning_input.is_none());

        let failed = store
            .admit(submission(
                run_id,
                agent_id,
                "failed",
                "sha256:failed",
                true,
            ))
            .await
            .expect("admit failed fixture");
        let failed_message_id = match failed {
            AgentRunMessageSubmissionAdmission::Created {
                mailbox_message, ..
            } => mailbox_message.id,
            other => panic!("unexpected admission: {other:?}"),
        };
        let (claimed_id, claim_token) = claim(&mailbox, run_id, agent_id).await;
        assert_eq!(claimed_id, failed_message_id);
        let failed_result = store
            .settle_delivery_failed(AgentRunMailboxFailedSettlement {
                mailbox_message_id: failed_message_id,
                claim_token,
                error_message: "permanent provider failure".to_string(),
            })
            .await
            .expect("settle failed delivery");
        assert_eq!(failed_result.message.status, MailboxMessageStatus::Failed);
        let failed_receipt = store
            .load_receipt_by_mailbox_message(failed_message_id)
            .await
            .expect("load failed product receipt")
            .expect("failed product receipt");
        assert_eq!(failed_receipt.status, AgentRunCommandStatus::TerminalFailed);
        assert_eq!(failed_receipt.result_json, Some(delivery_results().failed));
        assert_eq!(
            failed_receipt.error_message.as_deref(),
            Some("permanent provider failure")
        );
        let failed_replay = store
            .admit(submission(
                run_id,
                agent_id,
                "failed",
                "sha256:failed",
                true,
            ))
            .await
            .expect("replay failed delivery");
        let AgentRunMessageSubmissionAdmission::Replay { receipt } = failed_replay else {
            panic!("failed delivery must replay")
        };
        assert_eq!(
            receipt.error_message.as_deref(),
            Some("permanent provider failure")
        );

        let race = store
            .admit(submission(run_id, agent_id, "race", "sha256:race", true))
            .await
            .expect("admit race fixture");
        let (race_receipt_id, race_message_id) = match race {
            AgentRunMessageSubmissionAdmission::Created {
                receipt_id,
                mailbox_message,
            } => (receipt_id, mailbox_message.id),
            other => panic!("unexpected admission: {other:?}"),
        };
        let (claimed_id, claim_token) = claim(&mailbox, run_id, agent_id).await;
        assert_eq!(claimed_id, race_message_id);
        let (thread_id, operation_id) = runtime_operation_coordinates();
        let complete_store = store.clone();
        let settle_store = store.clone();
        let race_result = tokio::time::timeout(Duration::from_secs(5), async move {
            tokio::join!(
                complete_store.complete_submission(CompleteAgentRunMessageSubmission {
                    receipt_id: race_receipt_id,
                    mailbox_message_id: race_message_id,
                    accepted_refs: AgentRunAcceptedRefs {
                        run_id,
                        agent_id,
                        frame_id: None,
                        frame_revision: None,
                        runtime_thread_id: None,
                        runtime_operation_id: None,
                    },
                    result_json: json!({"outcome":"queued"}),
                }),
                settle_store.settle_delivery_accepted(AgentRunMailboxAcceptedSettlement {
                    mailbox_message_id: race_message_id,
                    claim_token,
                    delivery_kind: AgentRunAcceptedDeliveryKind::Started,
                    accepted_refs: AgentRunAcceptedRefs {
                        run_id,
                        agent_id,
                        frame_id: None,
                        frame_revision: None,
                        runtime_thread_id: Some(thread_id),
                        runtime_operation_id: Some(operation_id),
                    },
                })
            )
        })
        .await;
        let (queued_completion, delivery_settlement) =
            race_result.expect("receipt→mailbox lock order must not deadlock");
        let queued_completion = queued_completion
            .expect("queued loser/winner must replay or complete without conflict");
        let delivery_settlement =
            delivery_settlement.expect("delivery loser/winner must settle without conflict");
        assert_eq!(delivery_settlement.message.id, race_message_id);
        let delivery_receipt = store
            .load_receipt_by_mailbox_message(race_message_id)
            .await
            .expect("load race delivery receipt")
            .expect("delivery settlement must freeze the attached product receipt");
        let queued_receipt = match queued_completion {
            agentdash_application_ports::agent_run_message_submission::AgentRunMessageSubmissionCompletion::Completed { receipt }
            | agentdash_application_ports::agent_run_message_submission::AgentRunMessageSubmissionCompletion::Replayed { receipt } => receipt,
        };
        assert_eq!(queued_receipt.result_json, delivery_receipt.result_json);
        let final_receipt = store
            .require_receipt(race_receipt_id)
            .await
            .expect("load race winner");
        assert!(matches!(
            final_receipt.result_json,
            Some(ref result)
                if result == &json!({"outcome":"queued"})
                    || result == &delivery_results().started
        ));

        let reserved_receipt = NewAgentRunCommandReceipt {
            scope_kind: "project_agent_run_start".to_string(),
            scope_key: format!("{}:{}", Uuid::new_v4(), Uuid::new_v4()),
            command_kind: AgentRunCommandKind::ProjectAgentStart,
            client_command_id: "reserved-start".to_string(),
            request_digest: "sha256:reserved-start".to_string(),
        };
        let reservation = store
            .reserve(reserved_receipt.clone())
            .await
            .expect("reserve project start");
        let AgentRunMessageSubmissionReservation::Created { receipt_id } = reservation else {
            panic!("first reservation must be created")
        };
        let mut reserved_submission = submission(
            run_id,
            agent_id,
            "reserved-start",
            "sha256:reserved-start",
            true,
        );
        reserved_submission.receipt = reserved_receipt;
        reserved_submission.reserved_receipt_id = Some(receipt_id);
        let attached = store
            .admit(reserved_submission)
            .await
            .expect("attach reserved receipt");
        assert!(matches!(
            attached,
            AgentRunMessageSubmissionAdmission::Created { receipt_id: id, .. } if id == receipt_id
        ));

        let failed_reservation_receipt = NewAgentRunCommandReceipt {
            scope_kind: "project_agent_run_start".to_string(),
            scope_key: format!("{}:{}", Uuid::new_v4(), Uuid::new_v4()),
            command_kind: AgentRunCommandKind::ProjectAgentStart,
            client_command_id: "failed-reservation".to_string(),
            request_digest: "sha256:failed-reservation".to_string(),
        };
        let AgentRunMessageSubmissionReservation::Created { receipt_id } = store
            .reserve(failed_reservation_receipt.clone())
            .await
            .expect("reserve failing project start")
        else {
            panic!("first failure reservation must be created")
        };
        store
            .fail_reservation(receipt_id, "launch failed".to_string())
            .await
            .expect("terminalize failed reservation");
        let replay = store
            .reserve(failed_reservation_receipt)
            .await
            .expect("replay failed reservation");
        let AgentRunMessageSubmissionReservation::Replay { receipt } = replay else {
            panic!("terminal reservation must replay")
        };
        assert_eq!(receipt.error_message.as_deref(), Some("launch failed"));
    }
}
