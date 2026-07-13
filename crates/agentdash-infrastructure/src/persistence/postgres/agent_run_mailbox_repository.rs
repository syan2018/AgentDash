use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

use agentdash_domain::agent_run_mailbox::{
    AgentRunMailboxClaimRequest, AgentRunMailboxMessage, AgentRunMailboxRepository,
    AgentRunMailboxState, ConsumptionBarrier, MAILBOX_DELIVERY_RESULT_UNKNOWN, MailboxDelivery,
    MailboxDrainMode, MailboxMessageOrigin, MailboxMessageStatus, MailboxSourceIdentity,
    NewAgentRunMailboxMessage,
};
use agentdash_domain::common::error::DomainError;

use super::json_document::to_optional_jsonb;
use super::{db_err, sql_err_for};

pub struct PostgresAgentRunMailboxRepository {
    pool: PgPool,
}

impl PostgresAgentRunMailboxRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn initialize(&self) -> Result<(), DomainError> {
        crate::migration::assert_postgres_tables_ready(
            &self.pool,
            &["agent_run_mailbox_messages", "agent_run_mailbox_states"],
        )
        .await
    }

    async fn find_by_source_dedup(
        &self,
        run_id: Uuid,
        agent_id: Uuid,
        source_dedup_key: &str,
    ) -> Result<Option<AgentRunMailboxMessage>, DomainError> {
        sqlx::query_as::<_, AgentRunMailboxMessageRow>(&format!(
            "SELECT {MAILBOX_COLS} FROM agent_run_mailbox_messages \
             WHERE run_id=$1 AND agent_id=$2 AND source_dedup_key=$3"
        ))
        .bind(run_id.to_string())
        .bind(agent_id.to_string())
        .bind(source_dedup_key)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| sql_err_for("agent_run_mailbox_messages", error))?
        .map(TryInto::try_into)
        .transpose()
    }

    async fn next_order_key(&self, run_id: Uuid, agent_id: Uuid) -> Result<i64, DomainError> {
        let current: Option<i64> = sqlx::query_scalar(
            "SELECT MAX(order_key) FROM agent_run_mailbox_messages WHERE run_id=$1 AND agent_id=$2",
        )
        .bind(run_id.to_string())
        .bind(agent_id.to_string())
        .fetch_one(&self.pool)
        .await
        .map_err(|error| sql_err_for("agent_run_mailbox_messages", error))?;
        Ok(current.unwrap_or(0) + 1)
    }

    async fn rebalance_order_keys(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        run_id: Uuid,
        agent_id: Uuid,
        priority: i32,
        exclude_id: Uuid,
    ) -> Result<(), DomainError> {
        let ids: Vec<String> = sqlx::query_scalar(
            "SELECT id FROM agent_run_mailbox_messages \
             WHERE run_id=$1 AND agent_id=$2 AND priority=$3 AND id <> $4 \
               AND status NOT IN ('dispatched','steered','deleted') \
             ORDER BY order_key ASC",
        )
        .bind(run_id.to_string())
        .bind(agent_id.to_string())
        .bind(priority)
        .bind(exclude_id.to_string())
        .fetch_all(&mut **tx)
        .await
        .map_err(|e| sql_err_for("agent_run_mailbox_messages", e))?;

        for (i, msg_id) in ids.iter().enumerate() {
            let new_key = ((i as i64) + 1) * 1000;
            sqlx::query("UPDATE agent_run_mailbox_messages SET order_key=$1 WHERE id=$2")
                .bind(new_key)
                .bind(msg_id)
                .execute(&mut **tx)
                .await
                .map_err(|e| sql_err_for("agent_run_mailbox_messages", e))?;
        }
        Ok(())
    }
}

const MAILBOX_COLS: &str = "id,run_id,agent_id,origin,source_namespace,source_kind,source_ref,source_correlation_ref,source_actor,source_route,source_display_label_key,source_metadata,delivery,delivery_json,barrier,drain_mode,status,priority,order_key,source_dedup_key,accepted_runtime_operation_id,accepted_agent_run_turn_id,accepted_protocol_turn_id,claim_token,claimed_at,claim_expires_at,payload_json,executor_config_json,launch_planning_input,preview,has_images,retain_payload,attempt_count,last_error,created_at,updated_at,consumed_at,deleted_at";
const MAILBOX_COLS_M: &str = "m.id,m.run_id,m.agent_id,m.origin,m.source_namespace,m.source_kind,m.source_ref,m.source_correlation_ref,m.source_actor,m.source_route,m.source_display_label_key,m.source_metadata,m.delivery,m.delivery_json,m.barrier,m.drain_mode,m.status,m.priority,m.order_key,m.source_dedup_key,m.accepted_runtime_operation_id,m.accepted_agent_run_turn_id,m.accepted_protocol_turn_id,m.claim_token,m.claimed_at,m.claim_expires_at,m.payload_json,m.executor_config_json,m.launch_planning_input,m.preview,m.has_images,m.retain_payload,m.attempt_count,m.last_error,m.created_at,m.updated_at,m.consumed_at,m.deleted_at";
const STATE_COLS: &str =
    "run_id,agent_id,paused,pause_reason,pause_message,backend_selection_preference,updated_at";

#[async_trait::async_trait]
impl AgentRunMailboxRepository for PostgresAgentRunMailboxRepository {
    async fn list_pending_targets(&self) -> Result<Vec<(Uuid, Uuid)>, DomainError> {
        let rows = sqlx::query_as::<_, (String, String)>(
            "SELECT DISTINCT run_id,agent_id FROM agent_run_mailbox_messages \
             WHERE status = ANY (ARRAY['accepted','queued','ready_to_consume','consuming'])",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|error| sql_err_for("agent_run_mailbox_messages", error))?;
        rows.into_iter()
            .map(|(run_id, agent_id)| {
                Ok((
                    parse_uuid(&run_id, "agent_run_mailbox_pending_run")?,
                    parse_uuid(&agent_id, "agent_run_mailbox_pending_agent")?,
                ))
            })
            .collect()
    }

    async fn create_message(
        &self,
        message: NewAgentRunMailboxMessage,
    ) -> Result<AgentRunMailboxMessage, DomainError> {
        let id = message.id.unwrap_or_else(Uuid::new_v4);
        let now = Utc::now();
        let order_key = self
            .next_order_key(message.run_id, message.agent_id)
            .await?;
        let source_metadata = to_optional_jsonb(
            message.source.metadata.as_ref(),
            "agent_run_mailbox_messages.source_metadata",
        )?;
        sqlx::query_as::<_, AgentRunMailboxMessageRow>(&format!(
            "INSERT INTO agent_run_mailbox_messages \
             (id,run_id,agent_id,origin,source_namespace,source_kind,source_ref,\
              source_correlation_ref,source_actor,source_route,source_display_label_key,source_metadata,\
              delivery,delivery_json,barrier,drain_mode,status,priority,order_key,source_dedup_key,\
              payload_json,executor_config_json,launch_planning_input,\
              preview,has_images,retain_payload,created_at,updated_at) \
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20,$21,$22,$23,$24,$25,$26,$27,$28) \
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
        .bind(order_key)
        .bind(message.source_dedup_key)
        .bind(message.payload_json)
        .bind(message.executor_config_json)
        .bind(message.launch_planning_input)
        .bind(message.preview)
        .bind(message.has_images)
        .bind(message.retain_payload)
        .bind(now)
        .bind(now)
        .fetch_one(&self.pool)
        .await
        .map_err(|error| sql_err_for("agent_run_mailbox_messages", error))?
        .try_into()
    }

    async fn create_message_idempotent(
        &self,
        message: NewAgentRunMailboxMessage,
    ) -> Result<AgentRunMailboxMessage, DomainError> {
        if let Some(source_dedup_key) = message.source_dedup_key.as_deref()
            && let Some(existing) = self
                .find_by_source_dedup(message.run_id, message.agent_id, source_dedup_key)
                .await?
        {
            return Ok(existing);
        }
        match self.create_message(message.clone()).await {
            Ok(created) => Ok(created),
            Err(error) => {
                if let DomainError::Conflict { .. } = error
                    && let Some(source_dedup_key) = message.source_dedup_key.as_deref()
                    && let Some(existing) = self
                        .find_by_source_dedup(message.run_id, message.agent_id, source_dedup_key)
                        .await?
                {
                    return Ok(existing);
                }
                Err(error)
            }
        }
    }

    async fn get_message(&self, id: Uuid) -> Result<Option<AgentRunMailboxMessage>, DomainError> {
        sqlx::query_as::<_, AgentRunMailboxMessageRow>(&format!(
            "SELECT {MAILBOX_COLS} FROM agent_run_mailbox_messages WHERE id=$1"
        ))
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(db_err)?
        .map(TryInto::try_into)
        .transpose()
    }

    async fn list_messages(
        &self,
        run_id: Uuid,
        agent_id: Uuid,
    ) -> Result<Vec<AgentRunMailboxMessage>, DomainError> {
        let rows = sqlx::query_as::<_, AgentRunMailboxMessageRow>(&format!(
            "SELECT {MAILBOX_COLS} FROM agent_run_mailbox_messages \
             WHERE run_id=$1 AND agent_id=$2 \
             ORDER BY priority DESC, order_key ASC"
        ))
        .bind(run_id.to_string())
        .bind(agent_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(|error| sql_err_for("agent_run_mailbox_messages", error))?;
        rows.into_iter().map(TryInto::try_into).collect()
    }

    async fn claim_next(
        &self,
        request: AgentRunMailboxClaimRequest,
    ) -> Result<Vec<AgentRunMailboxMessage>, DomainError> {
        if request.barriers.is_empty() || request.limit <= 0 {
            return Ok(Vec::new());
        }
        let barriers = request
            .barriers
            .iter()
            .map(|barrier| barrier.as_str().to_string())
            .collect::<Vec<_>>();
        let drain_mode = request
            .drain_mode
            .map(|drain_mode| drain_mode.as_str().to_string());
        let rows = sqlx::query_as::<_, AgentRunMailboxMessageRow>(&format!(
            "WITH picked AS (\
                 SELECT id FROM agent_run_mailbox_messages \
                 WHERE run_id=$1 AND agent_id=$2 \
                   AND status = ANY (ARRAY['accepted','queued','ready_to_consume']) \
                   AND barrier = ANY($3) \
                   AND ($4::text IS NULL OR drain_mode=$4) \
                 ORDER BY priority DESC, order_key ASC \
                 LIMIT $5 \
                 FOR UPDATE SKIP LOCKED\
             ) \
             UPDATE agent_run_mailbox_messages m SET \
                 status=$6,claim_token=$7,claimed_at=$8,claim_expires_at=$9,\
                 attempt_count=attempt_count+1,updated_at=$8,last_error=NULL \
             FROM picked WHERE m.id=picked.id RETURNING {MAILBOX_COLS_M}"
        ))
        .bind(request.run_id.to_string())
        .bind(request.agent_id.to_string())
        .bind(barriers)
        .bind(drain_mode)
        .bind(request.limit)
        .bind(MailboxMessageStatus::Consuming.as_str())
        .bind(request.claim_token.to_string())
        .bind(Utc::now())
        .bind(request.claim_expires_at)
        .fetch_all(&self.pool)
        .await
        .map_err(|error| sql_err_for("agent_run_mailbox_messages", error))?;
        rows.into_iter().map(TryInto::try_into).collect()
    }

    async fn recover_expired_consuming(&self, now: DateTime<Utc>) -> Result<u64, DomainError> {
        let result = sqlx::query(
            "UPDATE agent_run_mailbox_messages SET status=$1,\
             claim_token=NULL,claimed_at=NULL,claim_expires_at=NULL,\
             last_error=$2,updated_at=$3 \
             WHERE status=$4 AND claim_expires_at IS NOT NULL AND claim_expires_at < $3",
        )
        .bind(MailboxMessageStatus::Blocked.as_str())
        .bind(MAILBOX_DELIVERY_RESULT_UNKNOWN)
        .bind(now)
        .bind(MailboxMessageStatus::Consuming.as_str())
        .execute(&self.pool)
        .await
        .map_err(|error| sql_err_for("agent_run_mailbox_messages", error))?;
        Ok(result.rows_affected())
    }

    async fn mark_message_status(
        &self,
        id: Uuid,
        claim_token: Option<Uuid>,
        status: MailboxMessageStatus,
        last_error: Option<String>,
    ) -> Result<AgentRunMailboxMessage, DomainError> {
        let now = Utc::now();
        sqlx::query_as::<_, AgentRunMailboxMessageRow>(&format!(
            "UPDATE agent_run_mailbox_messages SET \
             status=$1,last_error=$2,\
             claim_token=NULL,claimed_at=NULL,claim_expires_at=NULL,\
             consumed_at=CASE WHEN $1 = ANY (ARRAY['dispatched','steered','failed','deleted']) THEN COALESCE(consumed_at,$3) ELSE consumed_at END,\
             updated_at=$3 \
             WHERE id=$4 AND ($5::text IS NULL OR claim_token=$5) \
             RETURNING {MAILBOX_COLS}"
        ))
        .bind(status.as_str())
        .bind(last_error)
        .bind(now)
        .bind(id.to_string())
        .bind(claim_token.map(|token| token.to_string()))
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| sql_err_for("agent_run_mailbox_messages", error))?
        .ok_or_else(|| DomainError::NotFound {
            entity: "agent_run_mailbox_message",
            id: id.to_string(),
        })?
        .try_into()
    }

    async fn mark_runtime_operation_accepted(
        &self,
        id: Uuid,
        claim_token: Uuid,
        operation_id: String,
        agent_run_turn_id: Option<String>,
        protocol_turn_id: Option<String>,
    ) -> Result<AgentRunMailboxMessage, DomainError> {
        let now = Utc::now();
        sqlx::query_as::<_, AgentRunMailboxMessageRow>(&format!(
            "UPDATE agent_run_mailbox_messages SET \
             status=$1,accepted_runtime_operation_id=$2,accepted_agent_run_turn_id=$3,\
             accepted_protocol_turn_id=$4,last_error=NULL,\
             claim_token=NULL,claimed_at=NULL,claim_expires_at=NULL,\
             consumed_at=COALESCE(consumed_at,$5),updated_at=$5 \
             WHERE id=$6 AND claim_token=$7 RETURNING {MAILBOX_COLS}"
        ))
        .bind(MailboxMessageStatus::Dispatched.as_str())
        .bind(operation_id)
        .bind(agent_run_turn_id)
        .bind(protocol_turn_id)
        .bind(now)
        .bind(id.to_string())
        .bind(claim_token.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| sql_err_for("agent_run_mailbox_messages", error))?
        .ok_or_else(|| DomainError::Conflict {
            entity: "agent_run_mailbox_message",
            constraint: "runtime_operation_claim",
            message: "mailbox claim no longer owns runtime operation acceptance".to_string(),
        })?
        .try_into()
    }

    async fn update_message_policy(
        &self,
        id: Uuid,
        delivery: MailboxDelivery,
        barrier: ConsumptionBarrier,
        drain_mode: MailboxDrainMode,
        priority: i32,
    ) -> Result<AgentRunMailboxMessage, DomainError> {
        sqlx::query_as::<_, AgentRunMailboxMessageRow>(&format!(
            "UPDATE agent_run_mailbox_messages SET \
             delivery=$1,delivery_json=$2,barrier=$3,drain_mode=$4,priority=$5,\
             status=$6,claim_token=NULL,claimed_at=NULL,claim_expires_at=NULL,last_error=NULL,\
             updated_at=$7 \
             WHERE id=$8 AND status NOT IN ('dispatched','steered','deleted') \
             RETURNING {MAILBOX_COLS}"
        ))
        .bind(delivery.kind())
        .bind(delivery.to_json())
        .bind(barrier.as_str())
        .bind(drain_mode.as_str())
        .bind(priority)
        .bind(MailboxMessageStatus::Queued.as_str())
        .bind(Utc::now())
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| sql_err_for("agent_run_mailbox_messages", error))?
        .ok_or_else(|| DomainError::NotFound {
            entity: "agent_run_mailbox_message",
            id: id.to_string(),
        })?
        .try_into()
    }

    async fn delete_message(
        &self,
        id: Uuid,
    ) -> Result<Option<AgentRunMailboxMessage>, DomainError> {
        let now = Utc::now();
        sqlx::query_as::<_, AgentRunMailboxMessageRow>(&format!(
            "UPDATE agent_run_mailbox_messages SET \
             status=$1,deleted_at=COALESCE(deleted_at,$2),updated_at=$2 \
             WHERE id=$3 AND status <> $1 RETURNING {MAILBOX_COLS}"
        ))
        .bind(MailboxMessageStatus::Deleted.as_str())
        .bind(now)
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| sql_err_for("agent_run_mailbox_messages", error))?
        .map(TryInto::try_into)
        .transpose()
    }

    async fn cleanup_user_payload(&self, id: Uuid) -> Result<(), DomainError> {
        sqlx::query(
            "UPDATE agent_run_mailbox_messages SET payload_json=NULL,executor_config_json=NULL,updated_at=$1 \
             WHERE id=$2 AND origin=$3 AND retain_payload=false",
        )
        .bind(Utc::now())
        .bind(id.to_string())
        .bind(MailboxMessageOrigin::User.as_str())
        .execute(&self.pool)
        .await
        .map_err(|error| sql_err_for("agent_run_mailbox_messages", error))?;
        Ok(())
    }

    async fn pause_state(
        &self,
        run_id: Uuid,
        agent_id: Uuid,
        reason: String,
        message: Option<String>,
    ) -> Result<AgentRunMailboxState, DomainError> {
        let now = Utc::now();
        let mut tx = self.pool.begin().await.map_err(db_err)?;
        sqlx::query(
            "UPDATE agent_run_mailbox_messages SET \
             status=$1,claim_token=NULL,claimed_at=NULL,claim_expires_at=NULL,\
             last_error=COALESCE(last_error,$2),updated_at=$3 \
             WHERE run_id=$4 AND agent_id=$5 \
               AND status = ANY (ARRAY['accepted','queued','ready_to_consume','blocked'])",
        )
        .bind(MailboxMessageStatus::Paused.as_str())
        .bind(reason.clone())
        .bind(now)
        .bind(run_id.to_string())
        .bind(agent_id.to_string())
        .execute(&mut *tx)
        .await
        .map_err(|error| sql_err_for("agent_run_mailbox_messages", error))?;
        let state = sqlx::query_as::<_, AgentRunMailboxStateRow>(&format!(
            "INSERT INTO agent_run_mailbox_states \
             (run_id,agent_id,paused,pause_reason,pause_message,updated_at) \
             VALUES ($1,$2,true,$3,$4,$5) \
             ON CONFLICT (run_id,agent_id) DO UPDATE SET \
               paused=true,pause_reason=EXCLUDED.pause_reason,pause_message=EXCLUDED.pause_message,\
               updated_at=EXCLUDED.updated_at \
             RETURNING {STATE_COLS}"
        ))
        .bind(run_id.to_string())
        .bind(agent_id.to_string())
        .bind(reason)
        .bind(message)
        .bind(now)
        .fetch_one(&mut *tx)
        .await
        .map_err(|error| sql_err_for("agent_run_mailbox_states", error))?
        .try_into()?;
        tx.commit().await.map_err(db_err)?;
        Ok(state)
    }

    async fn resume_state(
        &self,
        run_id: Uuid,
        agent_id: Uuid,
    ) -> Result<AgentRunMailboxState, DomainError> {
        let now = Utc::now();
        let mut tx = self.pool.begin().await.map_err(db_err)?;
        sqlx::query(
            "UPDATE agent_run_mailbox_messages SET \
             status=$1,claim_token=NULL,claimed_at=NULL,claim_expires_at=NULL,\
             last_error=NULL,updated_at=$2 \
             WHERE run_id=$3 AND agent_id=$4 AND status=$5",
        )
        .bind(MailboxMessageStatus::Queued.as_str())
        .bind(now)
        .bind(run_id.to_string())
        .bind(agent_id.to_string())
        .bind(MailboxMessageStatus::Paused.as_str())
        .execute(&mut *tx)
        .await
        .map_err(|error| sql_err_for("agent_run_mailbox_messages", error))?;
        let state = sqlx::query_as::<_, AgentRunMailboxStateRow>(&format!(
            "INSERT INTO agent_run_mailbox_states \
             (run_id,agent_id,paused,pause_reason,pause_message,updated_at) \
             VALUES ($1,$2,false,NULL,NULL,$3) \
             ON CONFLICT (run_id,agent_id) DO UPDATE SET \
               paused=false,pause_reason=NULL,pause_message=NULL,updated_at=EXCLUDED.updated_at \
             RETURNING {STATE_COLS}"
        ))
        .bind(run_id.to_string())
        .bind(agent_id.to_string())
        .bind(now)
        .fetch_one(&mut *tx)
        .await
        .map_err(|error| sql_err_for("agent_run_mailbox_states", error))?
        .try_into()?;
        tx.commit().await.map_err(db_err)?;
        Ok(state)
    }

    async fn get_state(
        &self,
        run_id: Uuid,
        agent_id: Uuid,
    ) -> Result<Option<AgentRunMailboxState>, DomainError> {
        sqlx::query_as::<_, AgentRunMailboxStateRow>(&format!(
            "SELECT {STATE_COLS} FROM agent_run_mailbox_states WHERE run_id=$1 AND agent_id=$2"
        ))
        .bind(run_id.to_string())
        .bind(agent_id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| sql_err_for("agent_run_mailbox_states", error))?
        .map(TryInto::try_into)
        .transpose()
    }

    async fn set_backend_selection_preference(
        &self,
        run_id: Uuid,
        agent_id: Uuid,
        preference: Value,
    ) -> Result<AgentRunMailboxState, DomainError> {
        let now = Utc::now();
        sqlx::query_as::<_, AgentRunMailboxStateRow>(&format!(
            "INSERT INTO agent_run_mailbox_states \
             (run_id,agent_id,paused,pause_reason,pause_message,backend_selection_preference,updated_at) \
             VALUES ($1,$2,false,NULL,NULL,$3,$4) \
             ON CONFLICT (run_id,agent_id) DO UPDATE SET \
               backend_selection_preference=EXCLUDED.backend_selection_preference,\
               updated_at=EXCLUDED.updated_at \
             RETURNING {STATE_COLS}"
        ))
        .bind(run_id.to_string())
        .bind(agent_id.to_string())
        .bind(preference)
        .bind(now)
        .fetch_one(&self.pool)
        .await
        .map_err(|error| sql_err_for("agent_run_mailbox_states", error))?
        .try_into()
    }

    async fn move_message_after(
        &self,
        id: Uuid,
        after_id: Option<Uuid>,
        run_id: Uuid,
        agent_id: Uuid,
    ) -> Result<AgentRunMailboxMessage, DomainError> {
        let mut tx = self.pool.begin().await.map_err(db_err)?;

        let target = sqlx::query_as::<_, AgentRunMailboxMessageRow>(&format!(
            "SELECT {MAILBOX_COLS} FROM agent_run_mailbox_messages \
             WHERE id=$1 AND run_id=$2 AND agent_id=$3 FOR UPDATE"
        ))
        .bind(id.to_string())
        .bind(run_id.to_string())
        .bind(agent_id.to_string())
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| sql_err_for("agent_run_mailbox_messages", e))?
        .ok_or_else(|| DomainError::NotFound {
            entity: "agent_run_mailbox_message",
            id: id.to_string(),
        })?;

        let target_priority = target.priority;

        let new_order_key = if let Some(anchor_id) = after_id {
            let anchor = sqlx::query_as::<_, AgentRunMailboxMessageRow>(&format!(
                "SELECT {MAILBOX_COLS} FROM agent_run_mailbox_messages \
                 WHERE id=$1 AND run_id=$2 AND agent_id=$3 FOR UPDATE"
            ))
            .bind(anchor_id.to_string())
            .bind(run_id.to_string())
            .bind(agent_id.to_string())
            .fetch_optional(&mut *tx)
            .await
            .map_err(|e| sql_err_for("agent_run_mailbox_messages", e))?
            .ok_or_else(|| DomainError::NotFound {
                entity: "agent_run_mailbox_message",
                id: anchor_id.to_string(),
            })?;

            let successor_key: Option<i64> = sqlx::query_scalar(
                "SELECT MIN(order_key) FROM agent_run_mailbox_messages \
                 WHERE run_id=$1 AND agent_id=$2 AND priority=$3 \
                   AND order_key > $4 AND id <> $5 \
                   AND status NOT IN ('dispatched','steered','deleted')",
            )
            .bind(run_id.to_string())
            .bind(agent_id.to_string())
            .bind(target_priority)
            .bind(anchor.order_key)
            .bind(id.to_string())
            .fetch_one(&mut *tx)
            .await
            .map_err(|e| sql_err_for("agent_run_mailbox_messages", e))?;

            match successor_key {
                Some(succ) if succ - anchor.order_key > 1 => {
                    anchor.order_key + (succ - anchor.order_key) / 2
                }
                _ => {
                    self.rebalance_order_keys(&mut tx, run_id, agent_id, target_priority, id)
                        .await?;
                    let anchor_refreshed: i64 = sqlx::query_scalar(
                        "SELECT order_key FROM agent_run_mailbox_messages WHERE id=$1",
                    )
                    .bind(anchor_id.to_string())
                    .fetch_one(&mut *tx)
                    .await
                    .map_err(|e| sql_err_for("agent_run_mailbox_messages", e))?;

                    let succ_refreshed: Option<i64> = sqlx::query_scalar(
                        "SELECT MIN(order_key) FROM agent_run_mailbox_messages \
                         WHERE run_id=$1 AND agent_id=$2 AND priority=$3 \
                           AND order_key > $4 AND id <> $5 \
                           AND status NOT IN ('dispatched','steered','deleted')",
                    )
                    .bind(run_id.to_string())
                    .bind(agent_id.to_string())
                    .bind(target_priority)
                    .bind(anchor_refreshed)
                    .bind(id.to_string())
                    .fetch_one(&mut *tx)
                    .await
                    .map_err(|e| sql_err_for("agent_run_mailbox_messages", e))?;

                    match succ_refreshed {
                        Some(succ) => anchor_refreshed + (succ - anchor_refreshed) / 2,
                        None => anchor_refreshed + 1000,
                    }
                }
            }
        } else {
            let min_key: Option<i64> = sqlx::query_scalar(
                "SELECT MIN(order_key) FROM agent_run_mailbox_messages \
                 WHERE run_id=$1 AND agent_id=$2 AND priority=$3 AND id <> $4 \
                   AND status NOT IN ('dispatched','steered','deleted')",
            )
            .bind(run_id.to_string())
            .bind(agent_id.to_string())
            .bind(target_priority)
            .bind(id.to_string())
            .fetch_one(&mut *tx)
            .await
            .map_err(|e| sql_err_for("agent_run_mailbox_messages", e))?;

            min_key.unwrap_or(1000) - 1000
        };

        let row = sqlx::query_as::<_, AgentRunMailboxMessageRow>(&format!(
            "UPDATE agent_run_mailbox_messages SET order_key=$1, updated_at=$2 \
             WHERE id=$3 RETURNING {MAILBOX_COLS}"
        ))
        .bind(new_order_key)
        .bind(Utc::now())
        .bind(id.to_string())
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| sql_err_for("agent_run_mailbox_messages", e))?;

        tx.commit().await.map_err(db_err)?;
        row.try_into()
    }
}

#[derive(sqlx::FromRow)]
struct AgentRunMailboxMessageRow {
    id: String,
    run_id: String,
    agent_id: String,
    origin: String,
    source_namespace: String,
    source_kind: String,
    source_ref: Option<String>,
    source_correlation_ref: Option<String>,
    source_actor: String,
    source_route: Option<String>,
    source_display_label_key: String,
    source_metadata: Option<Value>,
    delivery: String,
    delivery_json: Value,
    barrier: String,
    drain_mode: String,
    status: String,
    priority: i32,
    order_key: i64,
    source_dedup_key: Option<String>,
    accepted_runtime_operation_id: Option<String>,
    accepted_agent_run_turn_id: Option<String>,
    accepted_protocol_turn_id: Option<String>,
    claim_token: Option<String>,
    claimed_at: Option<DateTime<Utc>>,
    claim_expires_at: Option<DateTime<Utc>>,
    payload_json: Option<Value>,
    executor_config_json: Option<Value>,
    launch_planning_input: Option<Value>,
    preview: String,
    has_images: bool,
    retain_payload: bool,
    attempt_count: i32,
    last_error: Option<String>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    consumed_at: Option<DateTime<Utc>>,
    deleted_at: Option<DateTime<Utc>>,
}

impl TryFrom<AgentRunMailboxMessageRow> for AgentRunMailboxMessage {
    type Error = DomainError;

    fn try_from(row: AgentRunMailboxMessageRow) -> Result<Self, Self::Error> {
        Ok(Self {
            id: parse_uuid(&row.id, "agent_run_mailbox_message")?,
            run_id: parse_uuid(&row.run_id, "lifecycle_run")?,
            agent_id: parse_uuid(&row.agent_id, "lifecycle_agent")?,
            origin: MailboxMessageOrigin::try_from(row.origin.as_str())?,
            source: MailboxSourceIdentity {
                namespace: row.source_namespace,
                kind: row.source_kind,
                source_ref: row.source_ref,
                correlation_ref: row.source_correlation_ref,
                actor: row.source_actor,
                route: row.source_route,
                display_label_key: row.source_display_label_key,
                metadata: row.source_metadata,
            },
            delivery: MailboxDelivery::from_parts(&row.delivery, row.delivery_json)?,
            barrier: ConsumptionBarrier::try_from(row.barrier.as_str())?,
            drain_mode: MailboxDrainMode::try_from(row.drain_mode.as_str())?,
            status: MailboxMessageStatus::try_from(row.status.as_str())?,
            priority: row.priority,
            order_key: row.order_key,
            source_dedup_key: row.source_dedup_key,
            accepted_runtime_operation_id: row.accepted_runtime_operation_id,
            accepted_agent_run_turn_id: row.accepted_agent_run_turn_id,
            accepted_protocol_turn_id: row.accepted_protocol_turn_id,
            claim_token: row
                .claim_token
                .as_deref()
                .map(|raw| parse_uuid(raw, "agent_run_mailbox_claim"))
                .transpose()?,
            claimed_at: row.claimed_at,
            claim_expires_at: row.claim_expires_at,
            payload_json: row.payload_json,
            executor_config_json: row.executor_config_json,
            launch_planning_input: row.launch_planning_input,
            preview: row.preview,
            has_images: row.has_images,
            retain_payload: row.retain_payload,
            attempt_count: row.attempt_count,
            last_error: row.last_error,
            created_at: row.created_at,
            updated_at: row.updated_at,
            consumed_at: row.consumed_at,
            deleted_at: row.deleted_at,
        })
    }
}

#[derive(sqlx::FromRow)]
struct AgentRunMailboxStateRow {
    run_id: String,
    agent_id: String,
    paused: bool,
    pause_reason: Option<String>,
    pause_message: Option<String>,
    backend_selection_preference: Option<Value>,
    updated_at: DateTime<Utc>,
}

impl TryFrom<AgentRunMailboxStateRow> for AgentRunMailboxState {
    type Error = DomainError;

    fn try_from(row: AgentRunMailboxStateRow) -> Result<Self, Self::Error> {
        Ok(Self {
            run_id: parse_uuid(&row.run_id, "lifecycle_run")?,
            agent_id: parse_uuid(&row.agent_id, "lifecycle_agent")?,
            paused: row.paused,
            pause_reason: row.pause_reason,
            pause_message: row.pause_message,
            backend_selection_preference: row.backend_selection_preference,
            updated_at: row.updated_at,
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
    use serde_json::json;

    async fn test_pool() -> (PgPool, Option<crate::postgres_runtime::PostgresRuntime>) {
        if crate::persistence::postgres::test_database_url().is_some() {
            return (
                crate::persistence::postgres::test_pg_pool("canonical agent run mailbox")
                    .await
                    .expect("configured PostgreSQL test pool"),
                None,
            );
        }
        let data_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/canonical-mailbox-postgres-tests");
        let runtime = crate::postgres_runtime::PostgresRuntime::resolve_embedded_at_data_root(
            "canonical-mailbox-tests",
            57,
            data_root,
        )
        .await
        .expect("start isolated embedded PostgreSQL");
        let database_name = format!("canonical_mailbox_{}", Uuid::new_v4().simple());
        sqlx::query(&format!("CREATE DATABASE {database_name}"))
            .execute(&runtime.pool)
            .await
            .expect("create isolated mailbox database");
        let options = runtime
            .pool
            .connect_options()
            .as_ref()
            .clone()
            .database(&database_name);
        let pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(4)
            .connect_with(options)
            .await
            .expect("connect isolated mailbox database");
        crate::migration::run_postgres_migrations(&pool)
            .await
            .expect("run migrations through 0065");
        crate::migration::assert_postgres_schema_ready(&pool)
            .await
            .expect("mailbox schema readiness");
        crate::migration::assert_postgres_tables_absent(
            &pool,
            &[
                "agent_run_delivery_bindings",
                "runtime_sessions",
                "runtime_session_events",
            ],
        )
        .await
        .expect("legacy RuntimeSession tables absent");
        (pool, Some(runtime))
    }

    async fn insert_agent_run(pool: &PgPool, run_id: Uuid, agent_id: Uuid) {
        let project_id = Uuid::new_v4();
        sqlx::query("INSERT INTO projects (id,name,description,config,created_at,updated_at) VALUES ($1,'mailbox test','','{}',now(),now())")
            .bind(project_id.to_string()).execute(pool).await.expect("insert project");
        sqlx::query("INSERT INTO lifecycle_runs (id,project_id,topology,orchestrations,status,execution_log,created_at,updated_at,last_activity_at) VALUES ($1,$2,'plain',$3,'ready',$4,now(),now(),now())")
            .bind(run_id.to_string()).bind(project_id.to_string()).bind(json!([])).bind(json!([]))
            .execute(pool).await.expect("insert run");
        sqlx::query("INSERT INTO lifecycle_agents (id,run_id,project_id,source,status,created_at,updated_at) VALUES ($1,$2,$3,'unknown','idle',now(),now())")
            .bind(agent_id.to_string()).bind(run_id.to_string()).bind(project_id.to_string())
            .execute(pool).await.expect("insert agent");
    }

    fn message(run_id: Uuid, agent_id: Uuid) -> NewAgentRunMailboxMessage {
        NewAgentRunMailboxMessage {
            id: None,
            run_id,
            agent_id,
            origin: MailboxMessageOrigin::User,
            source: MailboxSourceIdentity::composer(),
            delivery: MailboxDelivery::LaunchOrContinueTurn,
            barrier: ConsumptionBarrier::ImmediateIfIdle,
            drain_mode: MailboxDrainMode::One,
            priority: 0,
            source_dedup_key: Some("canonical-mailbox-message".to_string()),
            payload_json: Some(json!([{"type":"text","text":"hello"}])),
            executor_config_json: None,
            launch_planning_input: Some(json!({"command":"send"})),
            preview: "hello".to_string(),
            has_images: false,
            retain_payload: true,
        }
    }

    #[tokio::test]
    async fn canonical_mailbox_roundtrips_and_recovers_without_session_columns() {
        let (pool, _runtime) = test_pool().await;
        let repo = PostgresAgentRunMailboxRepository::new(pool.clone());
        repo.initialize()
            .await
            .expect("initialize mailbox repository");
        let run_id = Uuid::new_v4();
        let agent_id = Uuid::new_v4();
        insert_agent_run(&pool, run_id, agent_id).await;

        let created = repo
            .create_message_idempotent(message(run_id, agent_id))
            .await
            .expect("create");
        assert_eq!(
            repo.list_pending_targets()
                .await
                .expect("list recoverable targets"),
            vec![(run_id, agent_id)]
        );
        let claim_token = Uuid::new_v4();
        let claimed = repo
            .claim_next(AgentRunMailboxClaimRequest {
                run_id,
                agent_id,
                barriers: vec![ConsumptionBarrier::ImmediateIfIdle],
                drain_mode: Some(MailboxDrainMode::One),
                limit: 1,
                claim_token,
                claim_expires_at: Utc::now() - chrono::Duration::seconds(1),
            })
            .await
            .expect("claim");
        assert_eq!(claimed.len(), 1);
        assert_eq!(claimed[0].id, created.id);

        assert_eq!(
            repo.recover_expired_consuming(Utc::now())
                .await
                .expect("recover"),
            1
        );
        let recovered = repo
            .get_message(created.id)
            .await
            .expect("load")
            .expect("exists");
        assert_eq!(recovered.status, MailboxMessageStatus::Blocked);
        assert!(recovered.accepted_runtime_operation_id.is_none());
    }

    #[tokio::test]
    async fn accepted_turn_refs_migration_supports_clean_and_existing_schema_upgrade() {
        let (pool, _runtime) = test_pool().await;
        for column in ["accepted_agent_run_turn_id", "accepted_protocol_turn_id"] {
            let present: bool = sqlx::query_scalar(
                "SELECT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name='agent_run_mailbox_messages' AND column_name=$1)",
            )
            .bind(column)
            .fetch_one(&pool)
            .await
            .expect("read clean mailbox accepted ref column");
            assert!(present, "missing clean migration column {column}");
        }

        sqlx::query(
            "ALTER TABLE agent_run_mailbox_messages DROP COLUMN accepted_agent_run_turn_id, DROP COLUMN accepted_protocol_turn_id",
        )
        .execute(&pool)
        .await
        .expect("restore pre-0072 mailbox schema");
        sqlx::query("DELETE FROM _sqlx_migrations WHERE version=72")
            .execute(&pool)
            .await
            .expect("rewind 0072 migration marker");
        crate::migration::run_postgres_migrations(&pool)
            .await
            .expect("upgrade pre-0072 mailbox schema");

        let repo = PostgresAgentRunMailboxRepository::new(pool.clone());
        let run_id = Uuid::new_v4();
        let agent_id = Uuid::new_v4();
        insert_agent_run(&pool, run_id, agent_id).await;
        let created = repo
            .create_message(message(run_id, agent_id))
            .await
            .expect("create mailbox message after upgrade");
        let claim_token = Uuid::new_v4();
        repo.claim_next(AgentRunMailboxClaimRequest {
            run_id,
            agent_id,
            barriers: vec![ConsumptionBarrier::ImmediateIfIdle],
            drain_mode: Some(MailboxDrainMode::One),
            limit: 1,
            claim_token,
            claim_expires_at: Utc::now() + chrono::Duration::minutes(1),
        })
        .await
        .expect("claim mailbox message");
        seed_runtime_operation(&pool, "operation-1").await;
        let accepted = repo
            .mark_runtime_operation_accepted(
                created.id,
                claim_token,
                "operation-1".to_string(),
                Some("agent-run-turn-1".to_string()),
                Some("protocol-turn-1".to_string()),
            )
            .await
            .expect("persist accepted refs");
        assert_eq!(
            accepted.accepted_agent_run_turn_id.as_deref(),
            Some("agent-run-turn-1")
        );
        assert_eq!(
            accepted.accepted_protocol_turn_id.as_deref(),
            Some("protocol-turn-1")
        );
    }

    async fn seed_runtime_operation(pool: &PgPool, operation_id: &str) {
        let suffix = Uuid::new_v4().simple().to_string();
        let binding_id = format!("binding-{suffix}");
        let source_thread_id = format!("source-{suffix}");
        let thread_id = format!("runtime-{suffix}");
        sqlx::query(
            "INSERT INTO agent_runtime_binding (id,driver_generation,profile_digest) VALUES ($1,1,$2)",
        )
        .bind(&binding_id)
        .bind(format!("profile-{suffix}"))
        .execute(pool)
        .await
        .expect("seed runtime binding");
        sqlx::query(
            "INSERT INTO agent_runtime_source_coordinate (binding_id,source_thread_id,thread_id) VALUES ($1,$2,$3)",
        )
        .bind(&binding_id)
        .bind(&source_thread_id)
        .bind(&thread_id)
        .execute(pool)
        .await
        .expect("seed runtime source coordinate");
        sqlx::query(
            "INSERT INTO agent_runtime_thread \
             (id,revision,next_event_sequence,next_operation_sequence,status,active_turn_id,binding_id,driver_generation,source_thread_id,profile_digest,active_checkpoint_id,context_revision,settings_revision,tool_set_revision,projection) \
             VALUES ($1,0,0,2,'active',NULL,$2,1,$3,$4,NULL,0,0,0,$5)",
        )
        .bind(&thread_id)
        .bind(&binding_id)
        .bind(&source_thread_id)
        .bind(format!("profile-{suffix}"))
        .bind(json!({}))
        .execute(pool)
        .await
        .expect("seed runtime thread");
        sqlx::query(
            "INSERT INTO agent_runtime_operation \
             (id,thread_id,operation_sequence,idempotency_key,accepted_revision,status,actor,command,terminal,record) \
             VALUES ($1,$2,1,$3,0,'active',$4,$5,NULL,$6)",
        )
        .bind(operation_id)
        .bind(&thread_id)
        .bind(format!("key-{suffix}"))
        .bind(json!({"kind":"system","component":"mailbox-migration-test"}))
        .bind(json!({"kind":"turn_start","thread_id":thread_id,"input":[]}))
        .bind(json!({}))
        .execute(pool)
        .await
        .expect("seed canonical runtime operation");
    }
}
