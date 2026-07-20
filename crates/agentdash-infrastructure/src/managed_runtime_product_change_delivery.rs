use std::{
    collections::BTreeMap,
    sync::{Arc, RwLock},
};

use agentdash_agent_runtime_contract::{ManagedRuntimePlatformChange, RuntimeChangeSequence};
use agentdash_application_agentrun::agent_run::{
    AgentRunProductRuntimeBinding, AgentRunProductRuntimeChange,
    AgentRunProductRuntimeChangeObserver,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{PgPool, Row};
use uuid::Uuid;

#[derive(Debug)]
struct ProductChangeClaim {
    target_run_id: String,
    target_agent_id: String,
    binding: AgentRunProductRuntimeBinding,
    sequence: RuntimeChangeSequence,
    change: ManagedRuntimePlatformChange,
    consumer_name: String,
    owner: String,
    token: Uuid,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
struct ProductChangeDeliveryConsumerState {
    #[serde(default)]
    delivered_sequence: u64,
    #[serde(default)]
    claim: Option<ProductChangeDeliveryClaimState>,
    #[serde(default)]
    attempt_count: u64,
    #[serde(default)]
    last_error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct ProductChangeDeliveryClaimState {
    owner: String,
    token: Uuid,
    expires_at_ms: u64,
}

type ProductChangeDeliveryState = BTreeMap<String, ProductChangeDeliveryConsumerState>;

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

    async fn claim(
        &self,
        consumer_name: &str,
        limit: usize,
    ) -> Result<Vec<ProductChangeClaim>, String> {
        if consumer_name.trim().is_empty() || limit == 0 {
            return Err(
                "Runtime Product change delivery requires a consumer and positive limit".to_owned(),
            );
        }
        let limit = i64::try_from(limit)
            .map_err(|_| "Runtime Product change delivery limit exceeds BIGINT".to_owned())?;
        let mut tx = self.pool.begin().await.map_err(db_error)?;
        let now_ms = database_time_ms(&mut tx).await?;
        let expires_at_ms = now_ms
            .checked_add(self.lease_duration_ms)
            .ok_or_else(|| "Runtime Product change delivery lease overflowed".to_owned())?;
        let token = Uuid::new_v4();
        let rows = sqlx::query(
            "SELECT product_binding.target_run_id,
                    product_binding.target_agent_id,
                    product_binding.binding,
                    product_binding.change_delivery_state,
                    next_change.outbox_entry
             FROM agent_run_product_runtime_binding product_binding
             JOIN agent_runtime_state_revision runtime
               ON runtime.thread_id=product_binding.runtime_thread_id
             CROSS JOIN LATERAL (
                 SELECT entry.value AS outbox_entry
                 FROM jsonb_array_elements(
                     COALESCE(runtime.facts->'outbox', '[]'::JSONB)
                 ) entry
                 WHERE (entry.value->>'sequence')::NUMERIC(20,0) >
                       COALESCE(
                           product_binding.change_delivery_state
                               -> $1 ->> 'delivered_sequence',
                           '0'
                       )::NUMERIC(20,0)
                 ORDER BY (entry.value->>'sequence')::NUMERIC(20,0)
                 LIMIT 1
             ) next_change
             WHERE product_binding.binding
                       -> 'source_binding' ->> 'activated_at_revision' IS NOT NULL
               AND COALESCE(
                   product_binding.change_delivery_state
                       -> $1 -> 'claim' ->> 'expires_at_ms',
                   '0'
               )::NUMERIC(20,0) <= $2::TEXT::NUMERIC(20,0)
             ORDER BY product_binding.runtime_thread_id
             FOR UPDATE OF product_binding SKIP LOCKED
             LIMIT $3",
        )
        .bind(consumer_name)
        .bind(now_ms.to_string())
        .bind(limit)
        .fetch_all(&mut *tx)
        .await
        .map_err(db_error)?;

        let mut claims = Vec::with_capacity(rows.len());
        for row in rows {
            let target_run_id = row
                .try_get::<String, _>("target_run_id")
                .map_err(db_error)?;
            let target_agent_id = row
                .try_get::<String, _>("target_agent_id")
                .map_err(db_error)?;
            let binding = serde_json::from_value::<AgentRunProductRuntimeBinding>(
                row.try_get::<Value, _>("binding").map_err(db_error)?,
            )
            .map_err(|error| format!("decode Product Runtime binding: {error}"))?;
            let outbox_entry = row.try_get::<Value, _>("outbox_entry").map_err(db_error)?;
            let sequence = decode_sequence(outbox_entry.get("sequence"))?;
            let change = serde_json::from_value::<ManagedRuntimePlatformChange>(
                outbox_entry
                    .get("change")
                    .cloned()
                    .ok_or_else(|| "Runtime Product outbox entry omitted change".to_owned())?,
            )
            .map_err(|error| format!("decode committed Runtime Product change: {error}"))?;
            if change.thread_id != binding.runtime_thread_id || change.sequence != sequence {
                return Err(
                    "Runtime Product change coordinates drifted from canonical binding/outbox"
                        .to_owned(),
                );
            }

            let mut delivery_state = decode_delivery_state(
                row.try_get::<Value, _>("change_delivery_state")
                    .map_err(db_error)?,
            )?;
            let consumer = delivery_state.entry(consumer_name.to_owned()).or_default();
            if sequence.0 <= consumer.delivered_sequence {
                return Err(
                    "Runtime Product change claim did not advance the consumer cursor".to_owned(),
                );
            }
            consumer.claim = Some(ProductChangeDeliveryClaimState {
                owner: self.claim_owner.clone(),
                token,
                expires_at_ms,
            });
            consumer.attempt_count = consumer
                .attempt_count
                .checked_add(1)
                .ok_or_else(|| "Runtime Product change attempt count overflowed".to_owned())?;
            consumer.last_error = None;
            persist_delivery_state(&mut tx, &target_run_id, &target_agent_id, &delivery_state)
                .await?;
            claims.push(ProductChangeClaim {
                target_run_id,
                target_agent_id,
                binding,
                sequence,
                change,
                consumer_name: consumer_name.to_owned(),
                owner: self.claim_owner.clone(),
                token,
            });
        }
        tx.commit().await.map_err(db_error)?;
        Ok(claims)
    }

    async fn ack(&self, claim: &ProductChangeClaim) -> Result<(), String> {
        self.finish_claim(claim, None).await
    }

    async fn release(&self, claim: &ProductChangeClaim, error: &str) -> Result<(), String> {
        self.finish_claim(claim, Some(error)).await
    }

    async fn finish_claim(
        &self,
        claim: &ProductChangeClaim,
        error: Option<&str>,
    ) -> Result<(), String> {
        let mut tx = self.pool.begin().await.map_err(db_error)?;
        let now_ms = database_time_ms(&mut tx).await?;
        let state = sqlx::query_scalar::<_, Value>(
            "SELECT change_delivery_state
             FROM agent_run_product_runtime_binding
             WHERE target_run_id=$1 AND target_agent_id=$2
             FOR UPDATE",
        )
        .bind(&claim.target_run_id)
        .bind(&claim.target_agent_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(db_error)?
        .ok_or_else(|| "Runtime Product change delivery claim is stale".to_owned())?;
        let mut delivery_state = decode_delivery_state(state)?;
        let consumer = delivery_state
            .get_mut(&claim.consumer_name)
            .ok_or_else(|| {
                "Runtime Product change delivery consumer claim is missing".to_owned()
            })?;
        let active_claim = consumer
            .claim
            .as_ref()
            .filter(|active| {
                active.owner == claim.owner
                    && active.token == claim.token
                    && (error.is_some() || active.expires_at_ms > now_ms)
            })
            .ok_or_else(|| "Runtime Product change delivery claim is stale".to_owned())?;
        if active_claim.expires_at_ms == 0 {
            return Err("Runtime Product change delivery claim has no lease".to_owned());
        }
        consumer.claim = None;
        match error {
            Some(error) => consumer.last_error = Some(error.to_owned()),
            None => {
                if claim.sequence.0 <= consumer.delivered_sequence {
                    return Err(
                        "Runtime Product change acknowledgement did not advance its cursor"
                            .to_owned(),
                    );
                }
                consumer.delivered_sequence = claim.sequence.0;
                consumer.last_error = None;
            }
        }
        persist_delivery_state(
            &mut tx,
            &claim.target_run_id,
            &claim.target_agent_id,
            &delivery_state,
        )
        .await?;
        tx.commit().await.map_err(db_error)
    }
}

pub(crate) struct ManagedRuntimeProductChangeConsumer {
    delivery: PostgresManagedRuntimeProductChangeDelivery,
    observers: RwLock<Vec<Arc<dyn AgentRunProductRuntimeChangeObserver>>>,
}

impl ManagedRuntimeProductChangeConsumer {
    pub(crate) fn new(
        delivery: PostgresManagedRuntimeProductChangeDelivery,
        observer: Arc<dyn AgentRunProductRuntimeChangeObserver>,
    ) -> Self {
        Self {
            delivery,
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
        let observers = self
            .observers
            .read()
            .map_err(|_| "Runtime Product change observer registry lock poisoned".to_owned())?
            .clone();
        let mut delivered = 0;
        let mut first_error = None;
        for observer in observers {
            let consumer_name = observer.consumer_name();
            let claims = self.delivery.claim(consumer_name, limit).await?;
            for claim in claims {
                let input = AgentRunProductRuntimeChange {
                    binding: claim.binding.clone(),
                    change: claim.change.clone(),
                };
                match observer.observe_product_runtime_change(&input).await {
                    Ok(_) => {
                        if let Err(error) = self.delivery.ack(&claim).await {
                            first_error.get_or_insert(error);
                        } else {
                            delivered += 1;
                        }
                    }
                    Err(error) => {
                        let observer_error = format!(
                            "Runtime Product change observer `{consumer_name}` failed: {error}"
                        );
                        let release = self.delivery.release(&claim, &observer_error).await;
                        first_error.get_or_insert_with(|| match release {
                            Ok(()) => observer_error,
                            Err(release_error) => {
                                format!("{observer_error}; release failed: {release_error}")
                            }
                        });
                    }
                }
            }
        }
        match first_error {
            Some(error) => Err(error),
            None => Ok(delivered),
        }
    }
}

async fn persist_delivery_state(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    target_run_id: &str,
    target_agent_id: &str,
    delivery_state: &ProductChangeDeliveryState,
) -> Result<(), String> {
    let state = serde_json::to_value(delivery_state)
        .map_err(|error| format!("encode Runtime Product change delivery state: {error}"))?;
    let result = sqlx::query(
        "UPDATE agent_run_product_runtime_binding
         SET change_delivery_state=$3
         WHERE target_run_id=$1 AND target_agent_id=$2",
    )
    .bind(target_run_id)
    .bind(target_agent_id)
    .bind(state)
    .execute(&mut **tx)
    .await
    .map_err(db_error)?;
    if result.rows_affected() != 1 {
        return Err("Runtime Product change delivery binding disappeared".to_owned());
    }
    Ok(())
}

fn decode_delivery_state(value: Value) -> Result<ProductChangeDeliveryState, String> {
    serde_json::from_value(value)
        .map_err(|error| format!("decode Runtime Product change delivery state: {error}"))
}

fn decode_sequence(value: Option<&Value>) -> Result<RuntimeChangeSequence, String> {
    let sequence = match value {
        Some(Value::Number(value)) => value.as_u64(),
        Some(Value::String(value)) => value.parse::<u64>().ok(),
        _ => None,
    }
    .ok_or_else(|| "Runtime Product outbox sequence is outside the u64 domain".to_owned())?;
    Ok(RuntimeChangeSequence(sequence))
}

async fn database_time_ms(tx: &mut sqlx::Transaction<'_, sqlx::Postgres>) -> Result<u64, String> {
    sqlx::query_scalar::<_, String>(
        "SELECT FLOOR(EXTRACT(EPOCH FROM clock_timestamp()) * 1000)::TEXT",
    )
    .fetch_one(&mut **tx)
    .await
    .map_err(db_error)?
    .parse::<u64>()
    .map_err(|_| "Runtime Product change database clock exceeded u64".to_owned())
}

fn db_error(error: sqlx::Error) -> String {
    format!("Runtime Product change delivery persistence failed: {error}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentdash_agent_runtime_contract::{
        ManagedRuntimeChangeDelta, ManagedRuntimeLifecycleStatus, RuntimeProjectionRevision,
        RuntimeThreadId,
    };

    #[test]
    fn delivery_state_keeps_independent_consumer_cursors() {
        let mut state = ProductChangeDeliveryState::new();
        state.insert(
            "thread_name".to_owned(),
            ProductChangeDeliveryConsumerState {
                delivered_sequence: 7,
                attempt_count: 1,
                ..Default::default()
            },
        );
        state.insert(
            "terminal".to_owned(),
            ProductChangeDeliveryConsumerState {
                delivered_sequence: 3,
                attempt_count: 2,
                last_error: Some("retry".to_owned()),
                ..Default::default()
            },
        );

        let decoded = decode_delivery_state(
            serde_json::to_value(&state).expect("encode Product delivery state"),
        )
        .expect("decode Product delivery state");
        assert_eq!(decoded["thread_name"].delivered_sequence, 7);
        assert_eq!(decoded["terminal"].delivered_sequence, 3);
        assert_eq!(decoded["terminal"].last_error.as_deref(), Some("retry"));
    }

    #[tokio::test]
    async fn runtime_outbox_without_product_binding_is_not_delivery_work() {
        let (pool, _runtime) = isolated_delivery_pool().await;
        let thread_id =
            RuntimeThreadId::new(format!("unbound-runtime-{}", Uuid::new_v4())).expect("thread");
        let change = ManagedRuntimePlatformChange {
            thread_id: thread_id.clone(),
            sequence: RuntimeChangeSequence(1),
            revision: RuntimeProjectionRevision(1),
            delta: ManagedRuntimeChangeDelta::RuntimeLifecycleChanged {
                lifecycle: ManagedRuntimeLifecycleStatus::Active,
            },
        };
        sqlx::query(
            "INSERT INTO agent_runtime_state_revision(thread_id,revision,facts)
             VALUES ($1,1,$2)",
        )
        .bind(thread_id.as_str())
        .bind(serde_json::json!({
            "outbox": [{
                "sequence": 1,
                "operation_id": null,
                "change": change,
            }]
        }))
        .execute(&pool)
        .await
        .expect("seed unbound canonical Runtime outbox");

        let delivery =
            PostgresManagedRuntimeProductChangeDelivery::new(pool, "test-worker", 30_000)
                .expect("delivery");
        assert!(
            delivery
                .claim("terminal_projection", 16)
                .await
                .expect("claim Product work")
                .is_empty(),
            "a Runtime thread without an activated Product binding has no Product consumer work"
        );
    }

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
}
