use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

use agentdash_domain::agent_run_mailbox::{
    AgentRunMailboxClaimRequest, AgentRunMailboxDispatcherLease,
    AgentRunMailboxDispatcherLeaseRequest, AgentRunMailboxMessage, AgentRunMailboxPut,
    AgentRunMailboxRepository, AgentRunMailboxState, ConsumptionBarrier,
    MAILBOX_DELIVERY_RESULT_UNKNOWN, MailboxDelivery, MailboxDrainMode, MailboxMessageOrigin,
    MailboxMessageStatus, MailboxSourceIdentity, NewAgentRunMailboxMessage, SteeringStopEffect,
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

const MAILBOX_COLS: &str = "id,run_id,agent_id,delivery_runtime_thread_id,delivery_source_coordinate,delivery_binding_generation,delivery_snapshot_revision,origin,source_namespace,source_kind,source_ref,source_correlation_ref,source_actor,source_route,source_display_label_key,source_metadata,delivery,delivery_json,barrier,drain_mode,status,priority,order_key,source_dedup_key,queued_agent_run_turn_id,consuming_agent_run_turn_id,expected_active_agent_run_turn_id,accepted_agent_run_turn_id,accepted_protocol_turn_id,claim_token,claimed_at,claim_expires_at,command_receipt_id,payload_json,executor_config_json,launch_planning_input,preview,has_images,retain_payload,attempt_count,last_error,created_at,updated_at,consumed_at,deleted_at";
const MAILBOX_COLS_M: &str = "m.id,m.run_id,m.agent_id,m.delivery_runtime_thread_id,m.delivery_source_coordinate,m.delivery_binding_generation,m.delivery_snapshot_revision,m.origin,m.source_namespace,m.source_kind,m.source_ref,m.source_correlation_ref,m.source_actor,m.source_route,m.source_display_label_key,m.source_metadata,m.delivery,m.delivery_json,m.barrier,m.drain_mode,m.status,m.priority,m.order_key,m.source_dedup_key,m.queued_agent_run_turn_id,m.consuming_agent_run_turn_id,m.expected_active_agent_run_turn_id,m.accepted_agent_run_turn_id,m.accepted_protocol_turn_id,m.claim_token,m.claimed_at,m.claim_expires_at,m.command_receipt_id,m.payload_json,m.executor_config_json,m.launch_planning_input,m.preview,m.has_images,m.retain_payload,m.attempt_count,m.last_error,m.created_at,m.updated_at,m.consumed_at,m.deleted_at";
const STATE_COLS: &str = "run_id,agent_id,delivery_runtime_thread_id,paused,pause_reason,pause_message,backend_selection_preference,updated_at";

#[async_trait::async_trait]
impl AgentRunMailboxRepository for PostgresAgentRunMailboxRepository {
    async fn claim_dispatcher(
        &self,
        request: AgentRunMailboxDispatcherLeaseRequest,
    ) -> Result<Option<AgentRunMailboxDispatcherLease>, DomainError> {
        let now = Utc::now();
        sqlx::query_as::<_, AgentRunMailboxDispatcherLeaseRow>(
            "INSERT INTO agent_run_mailbox_dispatcher_leases \
             (run_id,agent_id,owner_id,lease_token,fencing_generation,expires_at,updated_at) \
             VALUES ($1,$2,$3,$4,1,$5,$6) \
             ON CONFLICT (run_id,agent_id) DO UPDATE SET \
               owner_id=EXCLUDED.owner_id,\
               lease_token=EXCLUDED.lease_token,\
               fencing_generation=agent_run_mailbox_dispatcher_leases.fencing_generation+1,\
               expires_at=EXCLUDED.expires_at,\
               updated_at=EXCLUDED.updated_at \
             WHERE agent_run_mailbox_dispatcher_leases.expires_at <= $6 \
                OR agent_run_mailbox_dispatcher_leases.owner_id = EXCLUDED.owner_id \
             RETURNING run_id,agent_id,owner_id,lease_token,fencing_generation,expires_at,updated_at",
        )
        .bind(request.run_id.to_string())
        .bind(request.agent_id.to_string())
        .bind(request.owner_id)
        .bind(request.lease_token.to_string())
        .bind(request.expires_at)
        .bind(now)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| sql_err_for("agent_run_mailbox_dispatcher_leases", error))?
        .map(TryInto::try_into)
        .transpose()
    }

    async fn renew_dispatcher(
        &self,
        lease: &AgentRunMailboxDispatcherLease,
        expires_at: DateTime<Utc>,
    ) -> Result<Option<AgentRunMailboxDispatcherLease>, DomainError> {
        sqlx::query_as::<_, AgentRunMailboxDispatcherLeaseRow>(
            "UPDATE agent_run_mailbox_dispatcher_leases SET expires_at=$6,updated_at=$7 \
             WHERE run_id=$1 AND agent_id=$2 AND owner_id=$3 AND lease_token=$4 \
               AND fencing_generation=$5 AND expires_at > $7 \
             RETURNING run_id,agent_id,owner_id,lease_token,fencing_generation,expires_at,updated_at",
        )
        .bind(lease.run_id.to_string())
        .bind(lease.agent_id.to_string())
        .bind(&lease.owner_id)
        .bind(lease.lease_token.to_string())
        .bind(lease.fencing_generation)
        .bind(expires_at)
        .bind(Utc::now())
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| sql_err_for("agent_run_mailbox_dispatcher_leases", error))?
        .map(TryInto::try_into)
        .transpose()
    }

    async fn release_dispatcher(
        &self,
        lease: &AgentRunMailboxDispatcherLease,
    ) -> Result<bool, DomainError> {
        let result = sqlx::query(
            "DELETE FROM agent_run_mailbox_dispatcher_leases \
             WHERE run_id=$1 AND agent_id=$2 AND owner_id=$3 AND lease_token=$4 \
               AND fencing_generation=$5",
        )
        .bind(lease.run_id.to_string())
        .bind(lease.agent_id.to_string())
        .bind(&lease.owner_id)
        .bind(lease.lease_token.to_string())
        .bind(lease.fencing_generation)
        .execute(&self.pool)
        .await
        .map_err(|error| sql_err_for("agent_run_mailbox_dispatcher_leases", error))?;
        Ok(result.rows_affected() == 1)
    }

    async fn create_message(
        &self,
        message: NewAgentRunMailboxMessage,
    ) -> Result<AgentRunMailboxMessage, DomainError> {
        let id = Uuid::new_v4();
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
             (id,run_id,agent_id,delivery_runtime_thread_id,delivery_source_coordinate,delivery_binding_generation,delivery_snapshot_revision,origin,source_namespace,source_kind,source_ref,\
              source_correlation_ref,source_actor,source_route,source_display_label_key,source_metadata,\
              delivery,delivery_json,barrier,drain_mode,status,priority,order_key,source_dedup_key,queued_agent_run_turn_id,\
              expected_active_agent_run_turn_id,command_receipt_id,payload_json,executor_config_json,launch_planning_input,\
              preview,has_images,retain_payload,created_at,updated_at) \
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20,$21,$22,$23,$24,$25,$26,$27,$28,$29,$30,$31,$32,$33,$34,$35) \
             RETURNING {MAILBOX_COLS}"
        ))
        .bind(id.to_string())
        .bind(message.run_id.to_string())
        .bind(message.agent_id.to_string())
        .bind(message.delivery_runtime_thread_id)
        .bind(message.delivery_source_coordinate)
        .bind(message.delivery_binding_generation)
        .bind(message.delivery_snapshot_revision)
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
        .bind(message.queued_agent_run_turn_id)
        .bind(message.expected_active_agent_run_turn_id)
        .bind(message.command_receipt_id.map(|id| id.to_string()))
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
    ) -> Result<AgentRunMailboxPut, DomainError> {
        if let Some(source_dedup_key) = message.source_dedup_key.as_deref()
            && let Some(existing) = self
                .find_by_source_dedup(message.run_id, message.agent_id, source_dedup_key)
                .await?
        {
            return Ok(AgentRunMailboxPut {
                message: existing,
                duplicate: true,
            });
        }
        match self.create_message(message.clone()).await {
            Ok(created) => Ok(AgentRunMailboxPut {
                message: created,
                duplicate: false,
            }),
            Err(error) => {
                if let DomainError::Conflict { .. } = error
                    && let Some(source_dedup_key) = message.source_dedup_key.as_deref()
                    && let Some(existing) = self
                        .find_by_source_dedup(message.run_id, message.agent_id, source_dedup_key)
                        .await?
                {
                    return Ok(AgentRunMailboxPut {
                        message: existing,
                        duplicate: true,
                    });
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

    async fn list_pending_targets(
        &self,
        limit: i64,
    ) -> Result<Vec<agentdash_domain::agent_run_target::AgentRunTarget>, DomainError> {
        let rows: Vec<(String, String)> = sqlx::query_as(
            "SELECT DISTINCT m.run_id,m.agent_id \
             FROM agent_run_mailbox_messages m \
             LEFT JOIN agent_run_mailbox_states s \
               ON s.run_id=m.run_id AND s.agent_id=m.agent_id \
             WHERE m.status = ANY (ARRAY['accepted','queued','ready_to_consume']) \
               AND m.barrier = ANY($1) \
               AND COALESCE(s.paused,FALSE)=FALSE \
             ORDER BY m.run_id,m.agent_id \
             LIMIT $2",
        )
        .bind(vec![
            ConsumptionBarrier::ImmediateIfIdle.as_str(),
            ConsumptionBarrier::AgentRunTurnBoundary.as_str(),
        ])
        .bind(limit.max(1))
        .fetch_all(&self.pool)
        .await
        .map_err(|error| sql_err_for("agent_run_mailbox_messages", error))?;
        rows.into_iter()
            .map(|(run_id, agent_id)| {
                Ok(agentdash_domain::agent_run_target::AgentRunTarget {
                    run_id: Uuid::parse_str(&run_id)
                        .map_err(|error| DomainError::InvalidConfig(error.to_string()))?,
                    agent_id: Uuid::parse_str(&agent_id)
                        .map_err(|error| DomainError::InvalidConfig(error.to_string()))?,
                })
            })
            .collect()
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
                   AND barrier = ANY($4) \
                   AND ($5::text IS NULL OR drain_mode=$5) \
                   AND NOT EXISTS (\
                       SELECT 1 FROM agent_run_mailbox_states s \
                       WHERE s.run_id=$1 AND s.agent_id=$2 AND s.paused=TRUE\
                   ) \
                 ORDER BY priority DESC, order_key ASC \
                 LIMIT $6 \
                 FOR UPDATE SKIP LOCKED\
             ) \
             UPDATE agent_run_mailbox_messages m SET \
                 delivery_runtime_thread_id=COALESCE($3,delivery_runtime_thread_id),\
                 status=$7,claim_token=$8,claimed_at=$9,claim_expires_at=$10,\
                 attempt_count=attempt_count+1,updated_at=$9,last_error=NULL \
             FROM picked WHERE m.id=picked.id RETURNING {MAILBOX_COLS_M}"
        ))
        .bind(request.run_id.to_string())
        .bind(request.agent_id.to_string())
        .bind(request.delivery_runtime_thread_id)
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
            "UPDATE agent_run_mailbox_messages SET \
             status=CASE \
                 WHEN accepted_agent_run_turn_id IS NOT NULL AND delivery=$1 THEN $2 \
                 WHEN accepted_agent_run_turn_id IS NOT NULL OR accepted_protocol_turn_id IS NOT NULL THEN $3 \
                 ELSE $4 \
             END,\
             claim_token=NULL,claimed_at=NULL,claim_expires_at=NULL,\
             last_error=CASE \
                 WHEN accepted_agent_run_turn_id IS NOT NULL OR accepted_protocol_turn_id IS NOT NULL THEN last_error \
                 ELSE $5 \
             END,\
             consumed_at=CASE \
                 WHEN accepted_agent_run_turn_id IS NOT NULL OR accepted_protocol_turn_id IS NOT NULL THEN COALESCE(consumed_at,$6) \
                 ELSE consumed_at \
             END,\
             updated_at=$6 \
             WHERE status=$7 AND claim_expires_at IS NOT NULL AND claim_expires_at < $6",
        )
        .bind(
            MailboxDelivery::SteerActiveTurn {
                stop_effect: SteeringStopEffect::None,
            }
            .kind(),
        )
        .bind(MailboxMessageStatus::Steered.as_str())
        .bind(MailboxMessageStatus::Dispatched.as_str())
        .bind(MailboxMessageStatus::Queued.as_str())
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
        accepted_agent_run_turn_id: Option<String>,
        accepted_protocol_turn_id: Option<String>,
        last_error: Option<String>,
    ) -> Result<AgentRunMailboxMessage, DomainError> {
        let now = Utc::now();
        sqlx::query_as::<_, AgentRunMailboxMessageRow>(&format!(
            "UPDATE agent_run_mailbox_messages SET \
             status=$1,accepted_agent_run_turn_id=COALESCE($2,accepted_agent_run_turn_id),\
             accepted_protocol_turn_id=COALESCE($3,accepted_protocol_turn_id),last_error=$4,\
             claim_token=CASE WHEN $1='consuming' THEN claim_token ELSE NULL END,\
             claimed_at=CASE WHEN $1='consuming' THEN claimed_at ELSE NULL END,\
             claim_expires_at=CASE WHEN $1='consuming' THEN claim_expires_at ELSE NULL END,\
             consumed_at=CASE WHEN $1 = ANY (ARRAY['dispatched','steered','failed','deleted']) THEN COALESCE(consumed_at,$5) ELSE consumed_at END,\
             updated_at=$5 \
             WHERE id=$6 AND ($7::text IS NULL OR claim_token=$7) \
             RETURNING {MAILBOX_COLS}"
        ))
        .bind(status.as_str())
        .bind(accepted_agent_run_turn_id)
        .bind(accepted_protocol_turn_id)
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

    async fn update_message_policy(
        &self,
        id: Uuid,
        delivery: MailboxDelivery,
        barrier: ConsumptionBarrier,
        drain_mode: MailboxDrainMode,
        priority: i32,
        expected_active_agent_run_turn_id: Option<String>,
    ) -> Result<AgentRunMailboxMessage, DomainError> {
        sqlx::query_as::<_, AgentRunMailboxMessageRow>(&format!(
            "UPDATE agent_run_mailbox_messages SET \
             delivery=$1,delivery_json=$2,barrier=$3,drain_mode=$4,priority=$5,\
             expected_active_agent_run_turn_id=$6,\
             status=$7,claim_token=NULL,claimed_at=NULL,claim_expires_at=NULL,last_error=NULL,\
             updated_at=$8 \
             WHERE id=$9 AND status NOT IN ('dispatched','steered','deleted') \
             RETURNING {MAILBOX_COLS}"
        ))
        .bind(delivery.kind())
        .bind(delivery.to_json())
        .bind(barrier.as_str())
        .bind(drain_mode.as_str())
        .bind(priority)
        .bind(expected_active_agent_run_turn_id)
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
        delivery_runtime_thread_id: Option<String>,
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
             (run_id,agent_id,delivery_runtime_thread_id,paused,pause_reason,pause_message,updated_at) \
             VALUES ($1,$2,$3,true,$4,$5,$6) \
             ON CONFLICT (run_id,agent_id) DO UPDATE SET \
               delivery_runtime_thread_id=EXCLUDED.delivery_runtime_thread_id,paused=true,\
               pause_reason=EXCLUDED.pause_reason,pause_message=EXCLUDED.pause_message,\
               updated_at=EXCLUDED.updated_at \
             RETURNING {STATE_COLS}"
        ))
        .bind(run_id.to_string())
        .bind(agent_id.to_string())
        .bind(delivery_runtime_thread_id)
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
        delivery_runtime_thread_id: Option<String>,
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
             (run_id,agent_id,delivery_runtime_thread_id,paused,pause_reason,pause_message,updated_at) \
             VALUES ($1,$2,$3,false,NULL,NULL,$4) \
             ON CONFLICT (run_id,agent_id) DO UPDATE SET \
               delivery_runtime_thread_id=EXCLUDED.delivery_runtime_thread_id,paused=false,\
               pause_reason=NULL,pause_message=NULL,updated_at=EXCLUDED.updated_at \
             RETURNING {STATE_COLS}"
        ))
        .bind(run_id.to_string())
        .bind(agent_id.to_string())
        .bind(delivery_runtime_thread_id)
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
        delivery_runtime_thread_id: Option<String>,
        preference: Value,
    ) -> Result<AgentRunMailboxState, DomainError> {
        let now = Utc::now();
        sqlx::query_as::<_, AgentRunMailboxStateRow>(&format!(
            "INSERT INTO agent_run_mailbox_states \
             (run_id,agent_id,delivery_runtime_thread_id,paused,pause_reason,pause_message,backend_selection_preference,updated_at) \
             VALUES ($1,$2,$3,false,NULL,NULL,$4,$5) \
             ON CONFLICT (run_id,agent_id) DO UPDATE SET \
               delivery_runtime_thread_id=EXCLUDED.delivery_runtime_thread_id,\
               backend_selection_preference=EXCLUDED.backend_selection_preference,\
               updated_at=EXCLUDED.updated_at \
             RETURNING {STATE_COLS}"
        ))
        .bind(run_id.to_string())
        .bind(agent_id.to_string())
        .bind(delivery_runtime_thread_id)
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
struct AgentRunMailboxDispatcherLeaseRow {
    run_id: String,
    agent_id: String,
    owner_id: String,
    lease_token: String,
    fencing_generation: i64,
    expires_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl TryFrom<AgentRunMailboxDispatcherLeaseRow> for AgentRunMailboxDispatcherLease {
    type Error = DomainError;

    fn try_from(row: AgentRunMailboxDispatcherLeaseRow) -> Result<Self, Self::Error> {
        Ok(Self {
            run_id: parse_uuid(&row.run_id, "lifecycle_run")?,
            agent_id: parse_uuid(&row.agent_id, "lifecycle_agent")?,
            owner_id: row.owner_id,
            lease_token: parse_uuid(&row.lease_token, "agent_run_mailbox_dispatcher_lease")?,
            fencing_generation: row.fencing_generation,
            expires_at: row.expires_at,
            updated_at: row.updated_at,
        })
    }
}

#[derive(sqlx::FromRow)]
struct AgentRunMailboxMessageRow {
    id: String,
    run_id: String,
    agent_id: String,
    delivery_runtime_thread_id: Option<String>,
    delivery_source_coordinate: Option<String>,
    delivery_binding_generation: Option<i64>,
    delivery_snapshot_revision: Option<i64>,
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
    queued_agent_run_turn_id: Option<String>,
    consuming_agent_run_turn_id: Option<String>,
    expected_active_agent_run_turn_id: Option<String>,
    accepted_agent_run_turn_id: Option<String>,
    accepted_protocol_turn_id: Option<String>,
    claim_token: Option<String>,
    claimed_at: Option<DateTime<Utc>>,
    claim_expires_at: Option<DateTime<Utc>>,
    command_receipt_id: Option<String>,
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
            delivery_runtime_thread_id: row.delivery_runtime_thread_id,
            delivery_source_coordinate: row.delivery_source_coordinate,
            delivery_binding_generation: row.delivery_binding_generation,
            delivery_snapshot_revision: row.delivery_snapshot_revision,
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
            queued_agent_run_turn_id: row.queued_agent_run_turn_id,
            consuming_agent_run_turn_id: row.consuming_agent_run_turn_id,
            expected_active_agent_run_turn_id: row.expected_active_agent_run_turn_id,
            accepted_agent_run_turn_id: row.accepted_agent_run_turn_id,
            accepted_protocol_turn_id: row.accepted_protocol_turn_id,
            claim_token: row
                .claim_token
                .as_deref()
                .map(|raw| parse_uuid(raw, "agent_run_mailbox_claim"))
                .transpose()?,
            claimed_at: row.claimed_at,
            claim_expires_at: row.claim_expires_at,
            command_receipt_id: row
                .command_receipt_id
                .as_deref()
                .map(|raw| parse_uuid(raw, "agent_run_command_receipt"))
                .transpose()?,
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
    delivery_runtime_thread_id: Option<String>,
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
            delivery_runtime_thread_id: row.delivery_runtime_thread_id,
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
    use crate::persistence::postgres::test_pg_pool;
    use serde_json::json;

    async fn insert_mailbox_refs(
        pool: &PgPool,
        run_id: Uuid,
        agent_id: Uuid,
        _runtime_thread_id: &str,
    ) {
        let project_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO projects (id,name,description,config,created_at,updated_at) \
             VALUES ($1,'mailbox test','','{}',now(),now())",
        )
        .bind(project_id.to_string())
        .execute(pool)
        .await
        .expect("insert project");
        sqlx::query(
            "INSERT INTO lifecycle_runs \
             (id,project_id,topology,orchestrations,status,execution_log,created_at,updated_at,last_activity_at) \
             VALUES ($1,$2,'plain',$3,'ready',$4,now(),now(),now())",
        )
        .bind(run_id.to_string())
        .bind(project_id.to_string())
        .bind(json!([]))
        .bind(json!([]))
        .execute(pool)
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
        .execute(pool)
        .await
        .expect("insert agent");
    }

    fn new_message(
        run_id: Uuid,
        agent_id: Uuid,
        session_id: &str,
        barrier: ConsumptionBarrier,
        drain_mode: MailboxDrainMode,
        dedup: &str,
    ) -> NewAgentRunMailboxMessage {
        NewAgentRunMailboxMessage {
            run_id,
            agent_id,
            delivery_runtime_thread_id: Some(session_id.to_string()),
            delivery_source_coordinate: None,
            delivery_binding_generation: None,
            delivery_snapshot_revision: None,
            origin: MailboxMessageOrigin::User,
            source: MailboxSourceIdentity::composer(),
            delivery: MailboxDelivery::LaunchOrContinueTurn,
            barrier,
            drain_mode,
            priority: 0,
            source_dedup_key: Some(dedup.to_string()),
            queued_agent_run_turn_id: None,
            expected_active_agent_run_turn_id: None,
            command_receipt_id: None,
            payload_json: Some(serde_json::json!([{"type":"text","text":"hello"}])),
            executor_config_json: None,
            launch_planning_input: None,
            preview: "hello".to_string(),
            has_images: false,
            retain_payload: false,
        }
    }

    #[tokio::test]
    async fn pending_target_scan_includes_input_waiting_for_turn_boundary() {
        let Some(pool) = test_pg_pool("agent_run_mailbox_pending_boundary").await else {
            return;
        };
        let repo = PostgresAgentRunMailboxRepository::new(pool.clone());
        repo.initialize().await.expect("initialize");

        let run_id = Uuid::new_v4();
        let agent_id = Uuid::new_v4();
        let session_id = format!("mailbox-session-{}", Uuid::new_v4());
        insert_mailbox_refs(&pool, run_id, agent_id, &session_id).await;
        repo.create_message(new_message(
            run_id,
            agent_id,
            &session_id,
            ConsumptionBarrier::AgentRunTurnBoundary,
            MailboxDrainMode::One,
            "turn-boundary-message",
        ))
        .await
        .expect("create boundary message");

        let targets = repo
            .list_pending_targets(10)
            .await
            .expect("list pending targets");
        assert_eq!(
            targets,
            vec![agentdash_domain::agent_run_target::AgentRunTarget { run_id, agent_id }]
        );
    }

    #[tokio::test]
    async fn source_identity_roundtrips_through_message_rows() {
        let Some(pool) = test_pg_pool("agent_run_mailbox_source_identity").await else {
            return;
        };
        let repo = PostgresAgentRunMailboxRepository::new(pool.clone());
        repo.initialize().await.expect("initialize");

        let run_id = Uuid::new_v4();
        let agent_id = Uuid::new_v4();
        let session_id = format!("mailbox-session-{}", Uuid::new_v4());
        insert_mailbox_refs(&pool, run_id, agent_id, &session_id).await;

        let expected_source = MailboxSourceIdentity::routine_trigger()
            .with_source_ref("routine-execution-1")
            .with_correlation_ref("routine-trigger-1")
            .with_route("reuse")
            .with_display_label_key("mailbox.source.routine.trigger")
            .with_metadata(serde_json::json!({
                "entity_key": "story-1",
                "trigger_source": "cron"
            }));
        let mut message = new_message(
            run_id,
            agent_id,
            &session_id,
            ConsumptionBarrier::ImmediateIfIdle,
            MailboxDrainMode::One,
            "source-identity-message",
        );
        message.source = expected_source.clone();

        let created = repo
            .create_message(message)
            .await
            .expect("create message with source identity");
        assert_eq!(created.source, expected_source);

        let loaded = repo
            .get_message(created.id)
            .await
            .expect("load message")
            .expect("message exists");
        assert_eq!(loaded.source, expected_source);

        let claimed = repo
            .claim_next(AgentRunMailboxClaimRequest {
                run_id,
                agent_id,
                delivery_runtime_thread_id: Some(session_id),
                barriers: vec![ConsumptionBarrier::ImmediateIfIdle],
                drain_mode: Some(MailboxDrainMode::One),
                limit: 1,
                claim_token: Uuid::new_v4(),
                claim_expires_at: Utc::now(),
            })
            .await
            .expect("claim message");
        assert_eq!(claimed.len(), 1);
        assert_eq!(claimed[0].source, expected_source);
    }

    #[tokio::test]
    async fn nullable_runtime_ref_claims_by_agentrun_owner_and_records_delivery_ref() {
        let Some(pool) = test_pg_pool("agent_run_mailbox_nullable_runtime_claim").await else {
            return;
        };
        let repo = PostgresAgentRunMailboxRepository::new(pool.clone());
        repo.initialize().await.expect("initialize");

        let run_id = Uuid::new_v4();
        let agent_id = Uuid::new_v4();
        let session_id = format!("mailbox-session-{}", Uuid::new_v4());
        insert_mailbox_refs(&pool, run_id, agent_id, &session_id).await;

        let mut message = new_message(
            run_id,
            agent_id,
            &session_id,
            ConsumptionBarrier::ImmediateIfIdle,
            MailboxDrainMode::One,
            "nullable-runtime-message",
        );
        message.delivery_runtime_thread_id = None;
        let created = repo
            .create_message(message)
            .await
            .expect("create message without runtime ref");
        assert!(created.delivery_runtime_thread_id.is_none());

        let claimed = repo
            .claim_next(AgentRunMailboxClaimRequest {
                run_id,
                agent_id,
                delivery_runtime_thread_id: Some(session_id.clone()),
                barriers: vec![ConsumptionBarrier::ImmediateIfIdle],
                drain_mode: Some(MailboxDrainMode::One),
                limit: 1,
                claim_token: Uuid::new_v4(),
                claim_expires_at: Utc::now(),
            })
            .await
            .expect("claim nullable-runtime message");
        assert_eq!(claimed.len(), 1);
        assert_eq!(claimed[0].id, created.id);
        assert_eq!(
            claimed[0].delivery_runtime_thread_id.as_deref(),
            Some(session_id.as_str())
        );
    }

    #[tokio::test]
    async fn pause_marks_existing_messages_paused_and_resume_requeues_them() {
        let Some(pool) = test_pg_pool("agent_run_mailbox_pause_resume").await else {
            return;
        };
        let repo = PostgresAgentRunMailboxRepository::new(pool.clone());
        repo.initialize().await.expect("initialize");

        let run_id = Uuid::new_v4();
        let agent_id = Uuid::new_v4();
        let session_id = format!("mailbox-session-{}", Uuid::new_v4());
        insert_mailbox_refs(&pool, run_id, agent_id, &session_id).await;

        let old_message = repo
            .create_message(new_message(
                run_id,
                agent_id,
                &session_id,
                ConsumptionBarrier::AgentLoopTurnBoundary,
                MailboxDrainMode::All,
                "old-message",
            ))
            .await
            .expect("create old message");

        repo.pause_state(
            run_id,
            agent_id,
            Some(session_id.clone()),
            "turn_failed".to_string(),
            Some("paused".to_string()),
        )
        .await
        .expect("pause");

        let paused = repo
            .get_message(old_message.id)
            .await
            .expect("load old message")
            .expect("old message exists");
        assert_eq!(paused.status, MailboxMessageStatus::Paused);

        let claimed_while_paused = repo
            .claim_next(AgentRunMailboxClaimRequest {
                run_id,
                agent_id,
                delivery_runtime_thread_id: Some(session_id.clone()),
                barriers: vec![ConsumptionBarrier::AgentLoopTurnBoundary],
                drain_mode: Some(MailboxDrainMode::All),
                limit: 10,
                claim_token: Uuid::new_v4(),
                claim_expires_at: Utc::now(),
            })
            .await
            .expect("claim while paused");
        assert!(claimed_while_paused.is_empty());

        repo.create_message(new_message(
            run_id,
            agent_id,
            &session_id,
            ConsumptionBarrier::ImmediateIfIdle,
            MailboxDrainMode::One,
            "fresh-message",
        ))
        .await
        .expect("create fresh message");
        let fresh_claim = repo
            .claim_next(AgentRunMailboxClaimRequest {
                run_id,
                agent_id,
                delivery_runtime_thread_id: Some(session_id.clone()),
                barriers: vec![ConsumptionBarrier::ImmediateIfIdle],
                drain_mode: Some(MailboxDrainMode::One),
                limit: 1,
                claim_token: Uuid::new_v4(),
                claim_expires_at: Utc::now(),
            })
            .await
            .expect("claim fresh message");
        assert_eq!(fresh_claim.len(), 1);

        repo.resume_state(run_id, agent_id, Some(session_id.clone()))
            .await
            .expect("resume");
        let resumed_claim = repo
            .claim_next(AgentRunMailboxClaimRequest {
                run_id,
                agent_id,
                delivery_runtime_thread_id: Some(session_id),
                barriers: vec![ConsumptionBarrier::AgentLoopTurnBoundary],
                drain_mode: Some(MailboxDrainMode::All),
                limit: 10,
                claim_token: Uuid::new_v4(),
                claim_expires_at: Utc::now(),
            })
            .await
            .expect("claim after resume");
        assert_eq!(resumed_claim.len(), 1);
        assert_eq!(resumed_claim[0].id, old_message.id);
    }

    #[tokio::test]
    async fn recover_expired_consuming_requeues_unknown_delivery_for_inspection() {
        let Some(pool) = test_pg_pool("agent_run_mailbox_recover_unknown").await else {
            return;
        };
        let repo = PostgresAgentRunMailboxRepository::new(pool.clone());
        repo.initialize().await.expect("initialize");

        let run_id = Uuid::new_v4();
        let agent_id = Uuid::new_v4();
        let session_id = format!("mailbox-session-{}", Uuid::new_v4());
        insert_mailbox_refs(&pool, run_id, agent_id, &session_id).await;

        let message = repo
            .create_message(new_message(
                run_id,
                agent_id,
                &session_id,
                ConsumptionBarrier::ImmediateIfIdle,
                MailboxDrainMode::One,
                "unknown-delivery-message",
            ))
            .await
            .expect("create message");
        let claim_token = Uuid::new_v4();
        let claimed = repo
            .claim_next(AgentRunMailboxClaimRequest {
                run_id,
                agent_id,
                delivery_runtime_thread_id: Some(session_id.clone()),
                barriers: vec![ConsumptionBarrier::ImmediateIfIdle],
                drain_mode: Some(MailboxDrainMode::One),
                limit: 1,
                claim_token,
                claim_expires_at: Utc::now(),
            })
            .await
            .expect("claim message");
        assert_eq!(claimed.len(), 1);
        assert_eq!(claimed[0].id, message.id);

        let recovered = repo
            .recover_expired_consuming(Utc::now() + chrono::Duration::seconds(1))
            .await
            .expect("recover expired consuming");
        assert!(recovered >= 1);

        let recovered_message = repo
            .get_message(message.id)
            .await
            .expect("load recovered message")
            .expect("message exists");
        assert_eq!(recovered_message.status, MailboxMessageStatus::Queued);
        assert_eq!(
            recovered_message.last_error.as_deref(),
            Some(MAILBOX_DELIVERY_RESULT_UNKNOWN)
        );
        assert!(recovered_message.claim_token.is_none());
        assert!(recovered_message.claim_expires_at.is_none());

        let reclaimed = repo
            .claim_next(AgentRunMailboxClaimRequest {
                run_id,
                agent_id,
                delivery_runtime_thread_id: Some(session_id),
                barriers: vec![ConsumptionBarrier::ImmediateIfIdle],
                drain_mode: Some(MailboxDrainMode::One),
                limit: 1,
                claim_token: Uuid::new_v4(),
                claim_expires_at: Utc::now(),
            })
            .await
            .expect("claim after recovery");
        assert_eq!(reclaimed.len(), 1);
        assert_eq!(reclaimed[0].id, message.id);
    }

    #[tokio::test]
    async fn unknown_settlement_keeps_claim_lease_until_recovery_can_inspect() {
        let Some(pool) = test_pg_pool("agent_run_mailbox_unknown_settlement").await else {
            return;
        };
        let repo = PostgresAgentRunMailboxRepository::new(pool.clone());
        repo.initialize().await.expect("initialize");

        let run_id = Uuid::new_v4();
        let agent_id = Uuid::new_v4();
        let session_id = format!("mailbox-session-{}", Uuid::new_v4());
        insert_mailbox_refs(&pool, run_id, agent_id, &session_id).await;
        let message = repo
            .create_message(new_message(
                run_id,
                agent_id,
                &session_id,
                ConsumptionBarrier::ImmediateIfIdle,
                MailboxDrainMode::One,
                "unknown-settlement-message",
            ))
            .await
            .expect("create message");
        let claim_token = Uuid::new_v4();
        let claimed = repo
            .claim_next(AgentRunMailboxClaimRequest {
                run_id,
                agent_id,
                delivery_runtime_thread_id: Some(session_id),
                barriers: vec![ConsumptionBarrier::ImmediateIfIdle],
                drain_mode: Some(MailboxDrainMode::One),
                limit: 1,
                claim_token,
                claim_expires_at: Utc::now(),
            })
            .await
            .expect("claim message");
        assert_eq!(claimed.len(), 1);

        let unknown = repo
            .mark_message_status(
                message.id,
                Some(claim_token),
                MailboxMessageStatus::Consuming,
                None,
                None,
                Some(MAILBOX_DELIVERY_RESULT_UNKNOWN.to_owned()),
            )
            .await
            .expect("record unknown delivery result");
        assert_eq!(unknown.claim_token, Some(claim_token));
        assert!(unknown.claim_expires_at.is_some());

        repo.recover_expired_consuming(Utc::now() + chrono::Duration::seconds(1))
            .await
            .expect("recover unknown delivery");
        let recovered = repo
            .get_message(message.id)
            .await
            .expect("load recovered message")
            .expect("message exists");
        assert_eq!(recovered.status, MailboxMessageStatus::Queued);
        assert!(recovered.claim_token.is_none());
    }

    #[tokio::test]
    async fn recover_expired_consuming_restores_terminal_status_with_accepted_refs() {
        let Some(pool) = test_pg_pool("agent_run_mailbox_recover_terminal").await else {
            return;
        };
        let repo = PostgresAgentRunMailboxRepository::new(pool.clone());
        repo.initialize().await.expect("initialize");

        let run_id = Uuid::new_v4();
        let agent_id = Uuid::new_v4();
        let session_id = format!("mailbox-session-{}", Uuid::new_v4());
        insert_mailbox_refs(&pool, run_id, agent_id, &session_id).await;

        let message = repo
            .create_message(new_message(
                run_id,
                agent_id,
                &session_id,
                ConsumptionBarrier::AgentLoopTurnBoundary,
                MailboxDrainMode::All,
                "accepted-delivery-message",
            ))
            .await
            .expect("create message");
        repo.update_message_policy(
            message.id,
            MailboxDelivery::SteerActiveTurn {
                stop_effect: SteeringStopEffect::None,
            },
            ConsumptionBarrier::AgentLoopTurnBoundary,
            MailboxDrainMode::All,
            0,
            Some("active-turn".to_owned()),
        )
        .await
        .expect("set steer policy");
        let claimed = repo
            .claim_next(AgentRunMailboxClaimRequest {
                run_id,
                agent_id,
                delivery_runtime_thread_id: Some(session_id.clone()),
                barriers: vec![ConsumptionBarrier::AgentLoopTurnBoundary],
                drain_mode: Some(MailboxDrainMode::All),
                limit: 1,
                claim_token: Uuid::new_v4(),
                claim_expires_at: Utc::now(),
            })
            .await
            .expect("claim message");
        assert_eq!(claimed.len(), 1);

        sqlx::query(
            "UPDATE agent_run_mailbox_messages SET \
             accepted_agent_run_turn_id=$1,accepted_protocol_turn_id=$2 \
             WHERE id=$3",
        )
        .bind("agent-run-turn-1")
        .bind("protocol-turn-1")
        .bind(message.id.to_string())
        .execute(&pool)
        .await
        .expect("seed accepted refs");

        let recovered = repo
            .recover_expired_consuming(Utc::now() + chrono::Duration::seconds(1))
            .await
            .expect("recover expired consuming");
        assert!(recovered >= 1);

        let terminal = repo
            .get_message(message.id)
            .await
            .expect("load recovered message")
            .expect("message exists");
        assert_eq!(terminal.status, MailboxMessageStatus::Steered);
        assert_eq!(
            terminal.accepted_agent_run_turn_id.as_deref(),
            Some("agent-run-turn-1")
        );
        assert_eq!(
            terminal.accepted_protocol_turn_id.as_deref(),
            Some("protocol-turn-1")
        );
        assert!(terminal.consumed_at.is_some());
        assert!(terminal.claim_token.is_none());
        assert!(terminal.claim_expires_at.is_none());
    }
}
