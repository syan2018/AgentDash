use std::sync::{Arc, RwLock};

use agentdash_agent_runtime_contract::{
    ManagedRuntimePlatformChange, RuntimeChangeSequence, RuntimeThreadId,
};
use agentdash_application_agentrun::agent_run::{
    AgentRunProductRuntimeChange, AgentRunProductRuntimeChangeObserver,
};
use serde_json::Value;
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::PostgresAgentRunProductRuntimeBindingRepository;

#[derive(Debug)]
struct ProductChangeClaim {
    thread_id: RuntimeThreadId,
    sequence: RuntimeChangeSequence,
    change: ManagedRuntimePlatformChange,
    owner: String,
    token: Uuid,
}

#[derive(Clone)]
pub(crate) struct PostgresManagedRuntimeProductChangeDelivery {
    pool: PgPool,
    claim_owner: String,
    lease_duration_ms: u64,
}

impl PostgresManagedRuntimeProductChangeDelivery {
    pub(crate) fn new(
        pool: PgPool,
        claim_owner: impl Into<String>,
        lease_duration_ms: u64,
    ) -> Result<Self, String> {
        let claim_owner = claim_owner.into();
        if claim_owner.trim().is_empty() || lease_duration_ms == 0 {
            return Err(
                "Runtime Product change delivery requires an owner and positive lease".to_owned(),
            );
        }
        Ok(Self {
            pool,
            claim_owner,
            lease_duration_ms,
        })
    }

    async fn claim(&self, limit: usize) -> Result<Vec<ProductChangeClaim>, String> {
        if limit == 0 {
            return Err("Runtime Product change delivery limit must be positive".to_owned());
        }
        let limit = i64::try_from(limit)
            .map_err(|_| "Runtime Product change delivery limit exceeds BIGINT".to_owned())?;
        let lease_duration_ms = i64::try_from(self.lease_duration_ms)
            .map_err(|_| "Runtime Product change lease exceeds BIGINT".to_owned())?;
        let token = Uuid::new_v4();
        let mut tx = self.pool.begin().await.map_err(db_error)?;
        sqlx::query(
            "INSERT INTO agent_runtime_product_change_delivery(thread_id,sequence)
             SELECT thread_id,sequence FROM agent_runtime_outbox
             ON CONFLICT (thread_id,sequence) DO NOTHING",
        )
        .execute(&mut *tx)
        .await
        .map_err(db_error)?;
        let rows = sqlx::query(
            "WITH claimable AS (
                 SELECT delivery.thread_id,delivery.sequence
                 FROM agent_runtime_product_change_delivery delivery
                 WHERE (
                     delivery.status='pending'
                     OR (
                         delivery.status='claimed'
                         AND delivery.claim_expires_at <= NOW()
                     )
                 )
                 AND NOT EXISTS (
                     SELECT 1
                     FROM agent_runtime_product_change_delivery earlier
                     WHERE earlier.thread_id=delivery.thread_id
                       AND earlier.sequence < delivery.sequence
                       AND earlier.status <> 'delivered'
                 )
                 ORDER BY delivery.thread_id,delivery.sequence
                 FOR UPDATE OF delivery SKIP LOCKED
                 LIMIT $1
             ),
             claimed AS (
                 UPDATE agent_runtime_product_change_delivery delivery
                 SET status='claimed',
                     claim_owner=$2,
                     claim_token=$3,
                     claim_expires_at=NOW() + ($4 * INTERVAL '1 millisecond'),
                     attempt_count=delivery.attempt_count + 1,
                     last_error=NULL
                 FROM claimable
                 WHERE delivery.thread_id=claimable.thread_id
                   AND delivery.sequence=claimable.sequence
                 RETURNING delivery.thread_id,delivery.sequence
             )
             SELECT claimed.thread_id,claimed.sequence::TEXT AS sequence,outbox.change
             FROM claimed
             JOIN agent_runtime_outbox outbox
               ON outbox.thread_id=claimed.thread_id
              AND outbox.sequence=claimed.sequence
             ORDER BY claimed.thread_id,claimed.sequence",
        )
        .bind(limit)
        .bind(&self.claim_owner)
        .bind(token)
        .bind(lease_duration_ms)
        .fetch_all(&mut *tx)
        .await
        .map_err(db_error)?;
        tx.commit().await.map_err(db_error)?;

        rows.into_iter()
            .map(|row| {
                let thread_id =
                    RuntimeThreadId::new(row.try_get::<String, _>("thread_id").map_err(db_error)?)
                        .map_err(|error| error.to_string())?;
                let sequence = RuntimeChangeSequence(
                    row.try_get::<String, _>("sequence")
                        .map_err(db_error)?
                        .parse::<u64>()
                        .map_err(|_| {
                            "Runtime Product change sequence is outside the canonical u64 domain"
                                .to_owned()
                        })?,
                );
                let change = serde_json::from_value::<ManagedRuntimePlatformChange>(
                    row.try_get::<Value, _>("change").map_err(db_error)?,
                )
                .map_err(|error| format!("decode committed Runtime Product change: {error}"))?;
                if change.thread_id != thread_id || change.sequence != sequence {
                    return Err(
                        "Runtime Product change delivery coordinates drifted from outbox"
                            .to_owned(),
                    );
                }
                Ok(ProductChangeClaim {
                    thread_id,
                    sequence,
                    change,
                    owner: self.claim_owner.clone(),
                    token,
                })
            })
            .collect()
    }

    async fn ack(&self, claim: &ProductChangeClaim) -> Result<(), String> {
        let result = sqlx::query(
            "UPDATE agent_runtime_product_change_delivery
             SET status='delivered',
                 claim_owner=NULL,
                 claim_token=NULL,
                 claim_expires_at=NULL,
                 last_error=NULL,
                 delivered_at=NOW()
             WHERE thread_id=$1 AND sequence=$2::NUMERIC(20,0)
               AND status='claimed'
               AND claim_owner=$3
               AND claim_token=$4
               AND claim_expires_at > NOW()",
        )
        .bind(claim.thread_id.as_str())
        .bind(sequence_decimal(claim.sequence))
        .bind(&claim.owner)
        .bind(claim.token)
        .execute(&self.pool)
        .await
        .map_err(db_error)?;
        if result.rows_affected() != 1 {
            return Err("Runtime Product change delivery claim is stale".to_owned());
        }
        Ok(())
    }

    async fn release(&self, claim: &ProductChangeClaim, error: &str) -> Result<(), String> {
        let result = sqlx::query(
            "UPDATE agent_runtime_product_change_delivery
             SET status='pending',
                 claim_owner=NULL,
                 claim_token=NULL,
                 claim_expires_at=NULL,
                 last_error=$5
             WHERE thread_id=$1 AND sequence=$2::NUMERIC(20,0)
               AND status='claimed'
               AND claim_owner=$3
               AND claim_token=$4",
        )
        .bind(claim.thread_id.as_str())
        .bind(sequence_decimal(claim.sequence))
        .bind(&claim.owner)
        .bind(claim.token)
        .bind(error)
        .execute(&self.pool)
        .await
        .map_err(db_error)?;
        if result.rows_affected() != 1 {
            return Err("Runtime Product change delivery release claim is stale".to_owned());
        }
        Ok(())
    }
}

pub(crate) struct ManagedRuntimeProductChangeConsumer {
    delivery: PostgresManagedRuntimeProductChangeDelivery,
    bindings: Arc<PostgresAgentRunProductRuntimeBindingRepository>,
    observers: RwLock<Vec<Arc<dyn AgentRunProductRuntimeChangeObserver>>>,
}

impl ManagedRuntimeProductChangeConsumer {
    pub(crate) fn new(
        delivery: PostgresManagedRuntimeProductChangeDelivery,
        bindings: Arc<PostgresAgentRunProductRuntimeBindingRepository>,
        observer: Arc<dyn AgentRunProductRuntimeChangeObserver>,
    ) -> Self {
        Self {
            delivery,
            bindings,
            observers: RwLock::new(vec![observer]),
        }
    }

    pub(crate) fn register_observer(
        &self,
        observer: Arc<dyn AgentRunProductRuntimeChangeObserver>,
    ) -> Result<(), String> {
        let mut observers = self
            .observers
            .write()
            .map_err(|_| "Runtime Product change observer registry lock poisoned".to_owned())?;
        if observers
            .iter()
            .any(|registered| registered.consumer_name() == observer.consumer_name())
        {
            return Err(format!(
                "Runtime Product change observer `{}` is already registered",
                observer.consumer_name()
            ));
        }
        observers.push(observer);
        Ok(())
    }

    pub(crate) async fn drain(&self, limit: usize) -> Result<usize, String> {
        let claims = self.delivery.claim(limit).await?;
        let mut delivered = 0;
        let mut first_error = None;
        for claim in claims {
            let result = self.dispatch(&claim).await;
            match result {
                Ok(()) => {
                    if let Err(error) = self.delivery.ack(&claim).await {
                        first_error.get_or_insert(error);
                    } else {
                        delivered += 1;
                    }
                }
                Err(error) => {
                    let release = self.delivery.release(&claim, &error).await;
                    first_error.get_or_insert_with(|| match release {
                        Ok(()) => error,
                        Err(release_error) => format!("{error}; release failed: {release_error}"),
                    });
                }
            }
        }
        match first_error {
            Some(error) => Err(error),
            None => Ok(delivered),
        }
    }

    async fn dispatch(&self, claim: &ProductChangeClaim) -> Result<(), String> {
        let binding = self
            .bindings
            .load_product_binding_by_runtime_thread(&claim.thread_id)
            .await?
            .ok_or_else(|| {
                format!(
                    "Runtime thread {} has no final AgentRun Product binding",
                    claim.thread_id
                )
            })?;
        let input = AgentRunProductRuntimeChange {
            binding,
            change: claim.change.clone(),
        };
        let observers = self
            .observers
            .read()
            .map_err(|_| "Runtime Product change observer registry lock poisoned".to_owned())?
            .clone();
        for observer in observers {
            observer
                .observe_product_runtime_change(&input)
                .await
                .map_err(|error| {
                    format!(
                        "Runtime Product change observer `{}` failed: {error}",
                        observer.consumer_name()
                    )
                })?;
        }
        Ok(())
    }
}

fn sequence_decimal(sequence: RuntimeChangeSequence) -> String {
    sequence.0.to_string()
}

fn db_error(error: sqlx::Error) -> String {
    format!("Runtime Product change delivery persistence failed: {error}")
}

#[cfg(test)]
mod tests {
    use agentdash_agent_runtime_contract::{
        ManagedRuntimeChangeDelta, ManagedRuntimeLifecycleStatus, RuntimeProjectionRevision,
    };

    use super::*;

    async fn isolated_delivery_pool() -> (PgPool, Option<crate::postgres_runtime::PostgresRuntime>)
    {
        if crate::persistence::postgres::test_database_url().is_some() {
            return (
                crate::persistence::postgres::test_pg_pool("Runtime Product change delivery")
                    .await
                    .expect("configured PostgreSQL test pool"),
                None,
            );
        }
        let data_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/runtime-product-change-delivery-postgres-tests");
        let runtime = crate::postgres_runtime::PostgresRuntime::resolve_embedded_at_data_root(
            "runtime-product-change-delivery-tests",
            8,
            data_root,
        )
        .await
        .expect("start embedded PostgreSQL for Runtime Product change delivery tests");
        let database_name = format!("runtime_product_delivery_{}", Uuid::new_v4().simple());
        sqlx::query(&format!("CREATE DATABASE {database_name}"))
            .execute(&runtime.pool)
            .await
            .expect("create isolated Runtime Product change delivery database");
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
            .expect("connect isolated Runtime Product change delivery database");
        crate::migration::run_postgres_migrations(&pool)
            .await
            .expect("migrate isolated Runtime Product change delivery database");
        crate::migration::assert_postgres_schema_ready(&pool)
            .await
            .expect("Runtime Product change delivery schema readiness");
        (pool, Some(runtime))
    }

    fn change(thread_id: RuntimeThreadId, sequence: u64) -> ManagedRuntimePlatformChange {
        ManagedRuntimePlatformChange {
            thread_id,
            sequence: RuntimeChangeSequence(sequence),
            revision: RuntimeProjectionRevision(sequence),
            delta: ManagedRuntimeChangeDelta::RuntimeLifecycleChanged {
                lifecycle: ManagedRuntimeLifecycleStatus::Active,
            },
        }
    }

    #[tokio::test]
    async fn delivery_claims_one_ordered_change_per_thread_and_fences_stale_tokens() {
        let (pool, _runtime) = isolated_delivery_pool().await;
        let thread_id =
            RuntimeThreadId::new(format!("runtime-delivery-{}", Uuid::new_v4())).expect("thread");
        sqlx::query(
            "INSERT INTO agent_runtime_state_revision(thread_id,revision,facts)
             VALUES ($1,1,'{}'::JSONB)",
        )
        .bind(thread_id.as_str())
        .execute(&pool)
        .await
        .expect("seed Runtime state");
        for sequence in [1_u64, 2] {
            let change = serde_json::to_value(change(thread_id.clone(), sequence))
                .expect("encode Runtime change");
            sqlx::query(
                "INSERT INTO agent_runtime_change(thread_id,sequence,operation_id,change)
                 VALUES ($1,$2,NULL,$3)",
            )
            .bind(thread_id.as_str())
            .bind(i64::try_from(sequence).expect("sequence"))
            .bind(&change)
            .execute(&pool)
            .await
            .expect("seed Runtime change");
            sqlx::query(
                "INSERT INTO agent_runtime_outbox(thread_id,sequence,operation_id,change)
                 VALUES ($1,$2,NULL,$3)",
            )
            .bind(thread_id.as_str())
            .bind(i64::try_from(sequence).expect("sequence"))
            .bind(change)
            .execute(&pool)
            .await
            .expect("seed Runtime outbox");
        }

        let delivery =
            PostgresManagedRuntimeProductChangeDelivery::new(pool.clone(), "worker-a", 30_000)
                .expect("delivery");
        let first = delivery.claim(16).await.expect("claim first");
        assert_eq!(first.len(), 1);
        assert_eq!(first[0].sequence, RuntimeChangeSequence(1));
        delivery
            .release(&first[0], "retry")
            .await
            .expect("release first");

        let retried = delivery.claim(16).await.expect("reclaim first");
        assert_eq!(retried.len(), 1);
        assert_eq!(retried[0].sequence, RuntimeChangeSequence(1));
        assert!(
            delivery.ack(&first[0]).await.is_err(),
            "released claim token must be stale after reclaim"
        );
        delivery.ack(&retried[0]).await.expect("ack first");

        let second = delivery.claim(16).await.expect("claim second");
        assert_eq!(second.len(), 1);
        assert_eq!(second[0].sequence, RuntimeChangeSequence(2));
        delivery.ack(&second[0]).await.expect("ack second");
        assert!(delivery.claim(16).await.expect("drained").is_empty());

        sqlx::query("DELETE FROM agent_runtime_state_revision WHERE thread_id=$1")
            .bind(thread_id.as_str())
            .execute(&pool)
            .await
            .expect("cleanup Runtime delivery fixture");
    }
}
