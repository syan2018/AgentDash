use agentdash_agent_runtime_contract::{
    DriverBindingId, DriverThreadId, ProfileDigest, RuntimeBindingId, RuntimeDriverGeneration,
    RuntimeServiceInstanceId, RuntimeThreadId,
};
use agentdash_agent_runtime_host::{
    AgentRuntimeHostRepository, AgentServiceDefinitionId, AgentServiceInstance,
    AgentServiceOfferId, AppliedSurface, DriverLease, HostStoreError, RuntimeBinding,
    RuntimeBindingState, RuntimeDriverCoordinate, RuntimeOffer, RuntimeSourceCoordinate,
};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::Value;
use sqlx::{PgPool, Postgres, Row, Transaction};

#[derive(Clone)]
pub struct PostgresAgentRuntimeHostRepository {
    pool: PgPool,
}

impl PostgresAgentRuntimeHostRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn sql_error(error: sqlx::Error) -> HostStoreError {
    if let sqlx::Error::Database(database) = &error {
        match database.code().as_deref() {
            Some("23505") | Some("40001") => {
                return HostStoreError::Conflict {
                    entity: "agent_runtime_host",
                    id: database
                        .constraint()
                        .unwrap_or("concurrent_write")
                        .to_string(),
                    expected: None,
                    actual: None,
                };
            }
            Some("23503") | Some("23514") => {
                return HostStoreError::Invariant {
                    reason: database.message().to_string(),
                };
            }
            _ => {}
        }
    }
    HostStoreError::Persistence {
        reason: error.to_string(),
    }
}

fn encode<T: Serialize>(value: &T, coordinate: &'static str) -> Result<Value, HostStoreError> {
    serde_json::to_value(value).map_err(|error| HostStoreError::Persistence {
        reason: format!("cannot encode {coordinate}: {error}"),
    })
}

fn decode<T: DeserializeOwned>(
    value: Value,
    coordinate: &'static str,
) -> Result<T, HostStoreError> {
    serde_json::from_value(value).map_err(|error| HostStoreError::Persistence {
        reason: format!("cannot decode {coordinate}: {error}"),
    })
}

fn binding_state(state: RuntimeBindingState) -> &'static str {
    match state {
        RuntimeBindingState::Pending => "pending",
        RuntimeBindingState::Active => "active",
        RuntimeBindingState::Desynchronized => "desynchronized",
        RuntimeBindingState::Lost => "lost",
        RuntimeBindingState::Closed => "closed",
        RuntimeBindingState::Failed => "failed",
    }
}

fn u64_to_i64(value: u64, coordinate: &'static str) -> Result<i64, HostStoreError> {
    i64::try_from(value).map_err(|_| HostStoreError::Invariant {
        reason: format!("{coordinate} exceeds PostgreSQL bigint"),
    })
}

fn u32_to_i32(value: u32, coordinate: &'static str) -> Result<i32, HostStoreError> {
    i32::try_from(value).map_err(|_| HostStoreError::Invariant {
        reason: format!("{coordinate} exceeds PostgreSQL integer"),
    })
}

async fn locked_binding(
    transaction: &mut Transaction<'_, Postgres>,
    binding_id: &RuntimeBindingId,
) -> Result<RuntimeBinding, HostStoreError> {
    let row = sqlx::query(
        "SELECT binding FROM agent_runtime_host_binding WHERE binding_id=$1 FOR UPDATE",
    )
    .bind(binding_id.as_str())
    .fetch_optional(&mut **transaction)
    .await
    .map_err(sql_error)?
    .ok_or_else(|| HostStoreError::NotFound {
        entity: "agent_runtime_host_binding",
        id: binding_id.to_string(),
    })?;
    decode(row.get("binding"), "agent_runtime_host_binding.binding")
}

#[async_trait]
impl AgentRuntimeHostRepository for PostgresAgentRuntimeHostRepository {
    async fn load_instance(
        &self,
        id: &RuntimeServiceInstanceId,
    ) -> Result<Option<AgentServiceInstance>, HostStoreError> {
        let row = sqlx::query(
            "SELECT config,credentials,placement,observed_state,definition_id,definition_build_digest,desired_state,revision \
             FROM agent_runtime_service_instance WHERE id=$1",
        )
        .bind(id.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(sql_error)?;
        row.map(|row| {
            let desired_state = match row.get::<String, _>("desired_state").as_str() {
                "active" => agentdash_agent_runtime_host::ServiceInstanceDesiredState::Active,
                "inactive" => agentdash_agent_runtime_host::ServiceInstanceDesiredState::Inactive,
                other => {
                    return Err(HostStoreError::Persistence {
                        reason: format!(
                            "invalid agent_runtime_service_instance.desired_state {other}"
                        ),
                    });
                }
            };
            Ok(AgentServiceInstance {
                id: id.clone(),
                definition_id: AgentServiceDefinitionId::new(row.get::<String, _>("definition_id"))
                    .map_err(|error| HostStoreError::Persistence {
                        reason: error.to_string(),
                    })?,
                definition_build_digest: row.get("definition_build_digest"),
                config: row.get("config"),
                credentials: decode(
                    row.get("credentials"),
                    "agent_runtime_service_instance.credentials",
                )?,
                placement: decode(
                    row.get("placement"),
                    "agent_runtime_service_instance.placement",
                )?,
                desired_state,
                observed_state: decode(
                    row.get("observed_state"),
                    "agent_runtime_service_instance.observed_state",
                )?,
                revision: u64::try_from(row.get::<i64, _>("revision")).map_err(|error| {
                    HostStoreError::Persistence {
                        reason: error.to_string(),
                    }
                })?,
            })
        })
        .transpose()
    }

    async fn put_instance(
        &self,
        mut instance: AgentServiceInstance,
        expected_revision: Option<u64>,
    ) -> Result<AgentServiceInstance, HostStoreError> {
        let mut transaction = self.pool.begin().await.map_err(sql_error)?;
        let actual = sqlx::query(
            "SELECT revision FROM agent_runtime_service_instance WHERE id=$1 FOR UPDATE",
        )
        .bind(instance.id.as_str())
        .fetch_optional(&mut *transaction)
        .await
        .map_err(sql_error)?
        .map(|row| row.get::<i64, _>("revision") as u64);
        if actual != expected_revision {
            return Err(HostStoreError::Conflict {
                entity: "agent_runtime_service_instance",
                id: instance.id.to_string(),
                expected: expected_revision,
                actual,
            });
        }
        instance.revision = actual.map_or(1, |revision| revision + 1);
        sqlx::query(
            "INSERT INTO agent_runtime_service_instance \
             (id,definition_id,definition_build_digest,revision,config,credentials,placement,desired_state,observed_state) \
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9) \
             ON CONFLICT (id) DO UPDATE SET definition_id=EXCLUDED.definition_id, \
             definition_build_digest=EXCLUDED.definition_build_digest,revision=EXCLUDED.revision, \
             config=EXCLUDED.config,credentials=EXCLUDED.credentials,placement=EXCLUDED.placement, \
             desired_state=EXCLUDED.desired_state,observed_state=EXCLUDED.observed_state,updated_at=now()",
        )
        .bind(instance.id.as_str())
        .bind(instance.definition_id.as_str())
        .bind(&instance.definition_build_digest)
        .bind(u64_to_i64(instance.revision, "service instance revision")?)
        .bind(instance.config.clone())
        .bind(encode(&instance.credentials, "agent_runtime_service_instance.credentials")?)
        .bind(encode(&instance.placement, "agent_runtime_service_instance.placement")?)
        .bind(match instance.desired_state {
            agentdash_agent_runtime_host::ServiceInstanceDesiredState::Active => "active",
            agentdash_agent_runtime_host::ServiceInstanceDesiredState::Inactive => "inactive",
        })
        .bind(encode(&instance.observed_state, "agent_runtime_service_instance.observed_state")?)
        .execute(&mut *transaction)
        .await
        .map_err(sql_error)?;
        sqlx::query(
            "INSERT INTO agent_runtime_service_instance_revision \
             (service_instance_id,revision,instance_snapshot) VALUES ($1,$2,$3)",
        )
        .bind(instance.id.as_str())
        .bind(u64_to_i64(instance.revision, "service instance revision")?)
        .bind(encode(
            &instance,
            "agent_runtime_service_instance_revision.instance_snapshot",
        )?)
        .execute(&mut *transaction)
        .await
        .map_err(sql_error)?;
        transaction.commit().await.map_err(sql_error)?;
        Ok(instance)
    }

    async fn next_generation(
        &self,
        instance_id: &RuntimeServiceInstanceId,
        expected_revision: u64,
    ) -> Result<RuntimeDriverGeneration, HostStoreError> {
        let row = sqlx::query(
            "UPDATE agent_runtime_service_instance SET active_generation=active_generation+1,updated_at=now() \
             WHERE id=$1 AND revision=$2 RETURNING active_generation",
        )
        .bind(instance_id.as_str())
        .bind(u64_to_i64(expected_revision, "expected service instance revision")?)
        .fetch_optional(&self.pool)
        .await
        .map_err(sql_error)?
        .ok_or_else(|| HostStoreError::Conflict {
            entity: "agent_runtime_service_instance",
            id: instance_id.to_string(),
            expected: Some(expected_revision),
            actual: None,
        })?;
        Ok(RuntimeDriverGeneration(
            row.get::<i64, _>("active_generation") as u64,
        ))
    }

    async fn commit_activation(
        &self,
        instance: AgentServiceInstance,
        offer: RuntimeOffer,
    ) -> Result<(), HostStoreError> {
        let mut transaction = self.pool.begin().await.map_err(sql_error)?;
        let row = sqlx::query(
            "SELECT revision,active_generation FROM agent_runtime_service_instance WHERE id=$1 FOR UPDATE",
        )
        .bind(instance.id.as_str())
        .fetch_optional(&mut *transaction)
        .await
        .map_err(sql_error)?
        .ok_or_else(|| HostStoreError::NotFound {
            entity: "agent_runtime_service_instance",
            id: instance.id.to_string(),
        })?;
        let actual_revision = row.get::<i64, _>("revision") as u64;
        let actual_generation = row.get::<i64, _>("active_generation") as u64;
        if actual_revision != instance.revision || actual_generation != offer.generation.0 {
            return Err(HostStoreError::Conflict {
                entity: "agent_runtime_service_activation",
                id: instance.id.to_string(),
                expected: Some(instance.revision),
                actual: Some(actual_revision),
            });
        }
        let previous_offers = sqlx::query(
            "SELECT id,offer FROM agent_runtime_offer WHERE service_instance_id=$1 AND available=true FOR UPDATE",
        )
        .bind(instance.id.as_str())
        .fetch_all(&mut *transaction)
        .await
        .map_err(sql_error)?;
        for row in previous_offers {
            let previous_id: String = row.get("id");
            let mut previous: RuntimeOffer = decode(row.get("offer"), "agent_runtime_offer.offer")?;
            previous.available = false;
            sqlx::query(
                "UPDATE agent_runtime_offer SET available=false,offer=$2,updated_at=now() WHERE id=$1",
            )
            .bind(previous_id)
            .bind(encode(&previous, "agent_runtime_offer.offer")?)
            .execute(&mut *transaction)
            .await
            .map_err(sql_error)?;
        }
        sqlx::query(
            "INSERT INTO agent_runtime_service_activation \
             (service_instance_id,instance_revision,driver_generation,protocol_revision,effective_profile,profile_digest,conformance_evidence,instance_snapshot) \
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8)",
        )
        .bind(instance.id.as_str())
        .bind(u64_to_i64(instance.revision, "activation instance revision")?)
        .bind(u64_to_i64(offer.generation.0, "activation generation")?)
        .bind(u32_to_i32(offer.protocol_revision, "activation protocol revision")?)
        .bind(encode(&offer.effective_profile, "agent_runtime_service_activation.effective_profile")?)
        .bind(offer.profile_digest.as_str())
        .bind(encode(&offer.conformance, "agent_runtime_service_activation.conformance_evidence")?)
        .bind(encode(
            &instance,
            "agent_runtime_service_activation.instance_snapshot",
        )?)
        .execute(&mut *transaction)
        .await
        .map_err(sql_error)?;
        sqlx::query(
            "INSERT INTO agent_runtime_offer \
             (id,service_instance_id,instance_revision,driver_generation,profile_digest,available,offer) \
             VALUES ($1,$2,$3,$4,$5,$6,$7)",
        )
        .bind(offer.id.as_str())
        .bind(offer.service_instance_id.as_str())
        .bind(u64_to_i64(offer.instance_revision, "offer instance revision")?)
        .bind(u64_to_i64(offer.generation.0, "offer generation")?)
        .bind(offer.profile_digest.as_str())
        .bind(offer.available)
        .bind(encode(&offer, "agent_runtime_offer.offer")?)
        .execute(&mut *transaction)
        .await
        .map_err(sql_error)?;
        sqlx::query(
            "UPDATE agent_runtime_service_instance SET observed_state=$2,updated_at=now() WHERE id=$1",
        )
        .bind(instance.id.as_str())
        .bind(encode(&instance.observed_state, "agent_runtime_service_instance.observed_state")?)
        .execute(&mut *transaction)
        .await
        .map_err(sql_error)?;
        transaction.commit().await.map_err(sql_error)
    }

    async fn load_activation_instance(
        &self,
        instance_id: &RuntimeServiceInstanceId,
        generation: RuntimeDriverGeneration,
    ) -> Result<Option<AgentServiceInstance>, HostStoreError> {
        let row = sqlx::query(
            "SELECT instance_snapshot FROM agent_runtime_service_activation \
             WHERE service_instance_id=$1 AND driver_generation=$2",
        )
        .bind(instance_id.as_str())
        .bind(u64_to_i64(generation.0, "activation generation")?)
        .fetch_optional(&self.pool)
        .await
        .map_err(sql_error)?;
        row.map(|row| {
            decode(
                row.get("instance_snapshot"),
                "agent_runtime_service_activation.instance_snapshot",
            )
        })
        .transpose()
    }

    async fn load_offer(
        &self,
        id: &AgentServiceOfferId,
    ) -> Result<Option<RuntimeOffer>, HostStoreError> {
        let row = sqlx::query("SELECT offer FROM agent_runtime_offer WHERE id=$1")
            .bind(id.as_str())
            .fetch_optional(&self.pool)
            .await
            .map_err(sql_error)?;
        row.map(|row| decode(row.get("offer"), "agent_runtime_offer.offer"))
            .transpose()
    }

    async fn list_offers(&self) -> Result<Vec<RuntimeOffer>, HostStoreError> {
        sqlx::query("SELECT offer FROM agent_runtime_offer ORDER BY id")
            .fetch_all(&self.pool)
            .await
            .map_err(sql_error)?
            .into_iter()
            .map(|row| decode(row.get("offer"), "agent_runtime_offer.offer"))
            .collect()
    }

    async fn disable_offers(
        &self,
        instance_id: &RuntimeServiceInstanceId,
    ) -> Result<(), HostStoreError> {
        let mut transaction = self.pool.begin().await.map_err(sql_error)?;
        let rows = sqlx::query(
            "SELECT id,offer FROM agent_runtime_offer WHERE service_instance_id=$1 FOR UPDATE",
        )
        .bind(instance_id.as_str())
        .fetch_all(&mut *transaction)
        .await
        .map_err(sql_error)?;
        for row in rows {
            let id: String = row.get("id");
            let mut offer: RuntimeOffer = decode(row.get("offer"), "agent_runtime_offer.offer")?;
            offer.available = false;
            sqlx::query(
                "UPDATE agent_runtime_offer SET available=false,offer=$2,updated_at=now() WHERE id=$1",
            )
            .bind(id)
            .bind(encode(&offer, "agent_runtime_offer.offer")?)
            .execute(&mut *transaction)
            .await
            .map_err(sql_error)?;
        }
        transaction.commit().await.map_err(sql_error)
    }

    async fn set_observed_state(
        &self,
        instance_id: &RuntimeServiceInstanceId,
        expected_revision: u64,
        observed: agentdash_agent_runtime_host::ServiceInstanceObservedState,
    ) -> Result<(), HostStoreError> {
        let result = sqlx::query(
            "UPDATE agent_runtime_service_instance SET observed_state=$3,updated_at=now() \
             WHERE id=$1 AND revision=$2",
        )
        .bind(instance_id.as_str())
        .bind(u64_to_i64(
            expected_revision,
            "expected service instance revision",
        )?)
        .bind(encode(
            &observed,
            "agent_runtime_service_instance.observed_state",
        )?)
        .execute(&self.pool)
        .await
        .map_err(sql_error)?;
        if result.rows_affected() != 1 {
            return Err(HostStoreError::Conflict {
                entity: "agent_runtime_service_instance",
                id: instance_id.to_string(),
                expected: Some(expected_revision),
                actual: None,
            });
        }
        Ok(())
    }

    async fn reserve_binding(&self, binding: RuntimeBinding) -> Result<(), HostStoreError> {
        let mut transaction = self.pool.begin().await.map_err(sql_error)?;
        if binding.state != RuntimeBindingState::Pending
            || binding.applied_surface.is_some()
            || binding.driver_binding_id.is_some()
            || binding.source_thread_id.is_some()
            || binding.lease_epoch != 0
        {
            return Err(HostStoreError::Invariant {
                reason: "new Host binding must be an unapplied pending reservation".to_string(),
            });
        }
        let offer = sqlx::query(
            "SELECT o.available,o.service_instance_id,o.instance_revision,o.driver_generation,\
                    o.profile_digest,i.revision,i.desired_state,i.observed_state \
             FROM agent_runtime_offer o \
             JOIN agent_runtime_service_instance i ON i.id=o.service_instance_id \
             WHERE o.id=$1 FOR UPDATE OF o,i",
        )
        .bind(binding.offer_id.as_str())
        .fetch_optional(&mut *transaction)
        .await
        .map_err(sql_error)?
        .ok_or_else(|| HostStoreError::NotFound {
            entity: "agent_runtime_offer",
            id: binding.offer_id.to_string(),
        })?;
        let observed: agentdash_agent_runtime_host::ServiceInstanceObservedState = decode(
            offer.get("observed_state"),
            "agent_runtime_service_instance.observed_state",
        )?;
        if !offer.get::<bool, _>("available")
            || offer.get::<String, _>("service_instance_id") != binding.service_instance_id.as_str()
            || offer.get::<i64, _>("instance_revision") as u64 != binding.instance_revision
            || offer.get::<i64, _>("driver_generation") as u64 != binding.driver_generation.0
            || offer.get::<String, _>("profile_digest") != binding.profile_digest.as_str()
            || offer.get::<i64, _>("revision") as u64 != binding.instance_revision
            || offer.get::<String, _>("desired_state") != "active"
            || observed != agentdash_agent_runtime_host::ServiceInstanceObservedState::Active
        {
            return Err(HostStoreError::Invariant {
                reason: "binding does not match an available current service offer".to_string(),
            });
        }
        sqlx::query(
            "INSERT INTO agent_runtime_binding (id,driver_generation,profile_digest) VALUES ($1,$2,$3)",
        )
        .bind(binding.id.as_str())
        .bind(u64_to_i64(binding.driver_generation.0, "binding generation")?)
        .bind(binding.profile_digest.as_str())
        .execute(&mut *transaction)
        .await
        .map_err(sql_error)?;
        sqlx::query(
            "INSERT INTO agent_runtime_host_binding \
             (binding_id,thread_id,offer_id,service_instance_id,instance_revision,driver_generation,profile_digest,state,lease_epoch,binding) \
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)",
        )
        .bind(binding.id.as_str())
        .bind(binding.thread_id.as_str())
        .bind(binding.offer_id.as_str())
        .bind(binding.service_instance_id.as_str())
        .bind(u64_to_i64(binding.instance_revision, "binding instance revision")?)
        .bind(u64_to_i64(binding.driver_generation.0, "binding generation")?)
        .bind(binding.profile_digest.as_str())
        .bind(binding_state(binding.state))
        .bind(u64_to_i64(binding.lease_epoch, "binding lease epoch")?)
        .bind(encode(&binding, "agent_runtime_host_binding.binding")?)
        .execute(&mut *transaction)
        .await
        .map_err(sql_error)?;
        transaction.commit().await.map_err(sql_error)
    }

    async fn activate_binding(
        &self,
        binding_id: &RuntimeBindingId,
        expected_generation: RuntimeDriverGeneration,
        applied: AppliedSurface,
        driver_binding_id: DriverBindingId,
        source: RuntimeSourceCoordinate,
    ) -> Result<RuntimeBinding, HostStoreError> {
        let mut transaction = self.pool.begin().await.map_err(sql_error)?;
        let mut binding = locked_binding(&mut transaction, binding_id).await?;
        if binding.state != RuntimeBindingState::Pending
            || binding.driver_generation != expected_generation
            || source.binding_id != *binding_id
            || source.generation != expected_generation
            || source.thread_id != binding.thread_id
        {
            return Err(HostStoreError::Invariant {
                reason: "binding activation coordinates or generation are stale".to_string(),
            });
        }
        binding.applied_surface = Some(applied);
        binding.driver_binding_id = Some(driver_binding_id);
        binding.source_thread_id = Some(source.source_thread_id.clone());
        binding.state = RuntimeBindingState::Active;
        sqlx::query(
            "UPDATE agent_runtime_host_binding SET state='active',binding=$2,updated_at=now() WHERE binding_id=$1",
        )
        .bind(binding_id.as_str())
        .bind(encode(&binding, "agent_runtime_host_binding.binding")?)
        .execute(&mut *transaction)
        .await
        .map_err(sql_error)?;
        sqlx::query(
            "INSERT INTO agent_runtime_source_coordinate (binding_id,source_thread_id,thread_id) VALUES ($1,$2,$3)",
        )
        .bind(binding_id.as_str())
        .bind(source.source_thread_id.as_str())
        .bind(source.thread_id.as_str())
        .execute(&mut *transaction)
        .await
        .map_err(sql_error)?;
        transaction.commit().await.map_err(sql_error)?;
        Ok(binding)
    }

    async fn load_binding(
        &self,
        id: &RuntimeBindingId,
    ) -> Result<Option<RuntimeBinding>, HostStoreError> {
        let row = sqlx::query("SELECT binding FROM agent_runtime_host_binding WHERE binding_id=$1")
            .bind(id.as_str())
            .fetch_optional(&self.pool)
            .await
            .map_err(sql_error)?;
        row.map(|row| decode(row.get("binding"), "agent_runtime_host_binding.binding"))
            .transpose()
    }

    async fn pending_bindings(&self) -> Result<Vec<RuntimeBinding>, HostStoreError> {
        sqlx::query(
            "SELECT binding FROM agent_runtime_host_binding WHERE state='pending' ORDER BY created_at",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(sql_error)?
        .into_iter()
        .map(|row| decode(row.get("binding"), "agent_runtime_host_binding.binding"))
        .collect()
    }

    async fn record_apply(
        &self,
        binding_id: &RuntimeBindingId,
        expected_generation: RuntimeDriverGeneration,
        applied: AppliedSurface,
    ) -> Result<RuntimeBinding, HostStoreError> {
        let mut transaction = self.pool.begin().await.map_err(sql_error)?;
        let mut binding = locked_binding(&mut transaction, binding_id).await?;
        if binding.driver_generation != expected_generation
            || binding.state != RuntimeBindingState::Active
        {
            return Err(HostStoreError::Invariant {
                reason: "surface apply receipt targets a stale or inactive binding".to_string(),
            });
        }
        binding.applied_surface = Some(applied);
        sqlx::query(
            "UPDATE agent_runtime_host_binding SET binding=$2,updated_at=now() WHERE binding_id=$1",
        )
        .bind(binding_id.as_str())
        .bind(encode(&binding, "agent_runtime_host_binding.binding")?)
        .execute(&mut *transaction)
        .await
        .map_err(sql_error)?;
        transaction.commit().await.map_err(sql_error)?;
        Ok(binding)
    }

    async fn fail_binding(
        &self,
        binding_id: &RuntimeBindingId,
        expected_generation: RuntimeDriverGeneration,
    ) -> Result<(), HostStoreError> {
        let mut transaction = self.pool.begin().await.map_err(sql_error)?;
        let mut binding = locked_binding(&mut transaction, binding_id).await?;
        if binding.driver_generation != expected_generation {
            return Err(HostStoreError::Invariant {
                reason: "cannot fail a different binding generation".to_string(),
            });
        }
        binding.state = RuntimeBindingState::Failed;
        sqlx::query(
            "UPDATE agent_runtime_host_binding SET state='failed',binding=$2,updated_at=now() WHERE binding_id=$1",
        )
        .bind(binding_id.as_str())
        .bind(encode(&binding, "agent_runtime_host_binding.binding")?)
        .execute(&mut *transaction)
        .await
        .map_err(sql_error)?;
        transaction.commit().await.map_err(sql_error)
    }

    async fn find_binding_by_thread(
        &self,
        thread_id: &RuntimeThreadId,
    ) -> Result<Option<RuntimeBinding>, HostStoreError> {
        let row = sqlx::query("SELECT binding FROM agent_runtime_host_binding WHERE thread_id=$1")
            .bind(thread_id.as_str())
            .fetch_optional(&self.pool)
            .await
            .map_err(sql_error)?;
        row.map(|row| decode(row.get("binding"), "agent_runtime_host_binding.binding"))
            .transpose()
    }

    async fn find_source(
        &self,
        binding_id: &RuntimeBindingId,
        generation: RuntimeDriverGeneration,
    ) -> Result<Option<RuntimeSourceCoordinate>, HostStoreError> {
        let row = sqlx::query(
            "SELECT s.thread_id,s.source_thread_id FROM agent_runtime_source_coordinate s \
             JOIN agent_runtime_binding b ON b.id=s.binding_id \
             WHERE s.binding_id=$1 AND b.driver_generation=$2",
        )
        .bind(binding_id.as_str())
        .bind(u64_to_i64(generation.0, "source generation")?)
        .fetch_optional(&self.pool)
        .await
        .map_err(sql_error)?;
        row.map(|row| {
            Ok(RuntimeSourceCoordinate {
                binding_id: binding_id.clone(),
                generation,
                thread_id: RuntimeThreadId::new(row.get::<String, _>("thread_id")).map_err(
                    |error| HostStoreError::Persistence {
                        reason: error.to_string(),
                    },
                )?,
                source_thread_id: DriverThreadId::new(row.get::<String, _>("source_thread_id"))
                    .map_err(|error| HostStoreError::Persistence {
                        reason: error.to_string(),
                    })?,
            })
        })
        .transpose()
    }

    async fn acquire_lease(
        &self,
        binding_id: &RuntimeBindingId,
        generation: RuntimeDriverGeneration,
        owner: &str,
        now: DateTime<Utc>,
        expires_at: DateTime<Utc>,
    ) -> Result<DriverLease, HostStoreError> {
        let mut transaction = self.pool.begin().await.map_err(sql_error)?;
        let mut binding = locked_binding(&mut transaction, binding_id).await?;
        if binding.driver_generation != generation || binding.state != RuntimeBindingState::Active {
            return Err(HostStoreError::Invariant {
                reason: "lease binding generation is not active".to_string(),
            });
        }
        let database_now: DateTime<Utc> = sqlx::query_scalar("SELECT clock_timestamp()")
            .fetch_one(&mut *transaction)
            .await
            .map_err(sql_error)?;
        let existing = sqlx::query(
            "SELECT owner,epoch,expires_at,driver_generation,lease FROM agent_runtime_driver_lease WHERE binding_id=$1 FOR UPDATE",
        )
        .bind(binding_id.as_str())
        .fetch_optional(&mut *transaction)
        .await
        .map_err(sql_error)?;
        if let Some(row) = &existing
            && row.get::<DateTime<Utc>, _>("expires_at") > database_now
        {
            if row.get::<String, _>("owner") == owner
                && row.get::<i64, _>("driver_generation") as u64 == generation.0
            {
                return decode(row.get("lease"), "agent_runtime_driver_lease.lease");
            }
            return Err(HostStoreError::Conflict {
                entity: "agent_runtime_driver_lease",
                id: binding_id.to_string(),
                expected: None,
                actual: Some(row.get::<i64, _>("epoch") as u64),
            });
        }
        let epoch = existing
            .as_ref()
            .map_or(1, |row| row.get::<i64, _>("epoch") as u64 + 1);
        let duration = expires_at.signed_duration_since(now);
        let lease = DriverLease {
            binding_id: binding_id.clone(),
            generation,
            owner: owner.to_string(),
            token: uuid::Uuid::new_v4().to_string(),
            epoch,
            expires_at: database_now + duration,
        };
        sqlx::query(
            "INSERT INTO agent_runtime_driver_lease \
             (binding_id,driver_generation,owner,token,epoch,expires_at,lease) VALUES ($1,$2,$3,$4,$5,$6,$7) \
             ON CONFLICT (binding_id) DO UPDATE SET driver_generation=EXCLUDED.driver_generation, \
             owner=EXCLUDED.owner,token=EXCLUDED.token,epoch=EXCLUDED.epoch,expires_at=EXCLUDED.expires_at, \
             lease=EXCLUDED.lease,updated_at=now()",
        )
        .bind(binding_id.as_str())
        .bind(u64_to_i64(generation.0, "lease generation")?)
        .bind(owner)
        .bind(&lease.token)
        .bind(u64_to_i64(epoch, "lease epoch")?)
        .bind(lease.expires_at)
        .bind(encode(&lease, "agent_runtime_driver_lease.lease")?)
        .execute(&mut *transaction)
        .await
        .map_err(sql_error)?;
        binding.lease_epoch = epoch;
        sqlx::query(
            "UPDATE agent_runtime_host_binding SET lease_epoch=$2,binding=$3,updated_at=now() WHERE binding_id=$1",
        )
        .bind(binding_id.as_str())
        .bind(u64_to_i64(epoch, "binding lease epoch")?)
        .bind(encode(&binding, "agent_runtime_host_binding.binding")?)
        .execute(&mut *transaction)
        .await
        .map_err(sql_error)?;
        transaction.commit().await.map_err(sql_error)?;
        Ok(lease)
    }

    async fn renew_lease(
        &self,
        binding_id: &RuntimeBindingId,
        generation: RuntimeDriverGeneration,
        owner: &str,
        token: &str,
        now: DateTime<Utc>,
        expires_at: DateTime<Utc>,
    ) -> Result<DriverLease, HostStoreError> {
        let duration = expires_at.signed_duration_since(now);
        let database_now: DateTime<Utc> = sqlx::query_scalar("SELECT clock_timestamp()")
            .fetch_one(&self.pool)
            .await
            .map_err(sql_error)?;
        let database_expires_at = database_now + duration;
        let row = sqlx::query(
            "UPDATE agent_runtime_driver_lease SET expires_at=$5, \
             lease=jsonb_set(lease,'{expires_at}',to_jsonb($5::timestamptz),false),updated_at=now() \
             WHERE binding_id=$1 AND driver_generation=$2 AND owner=$3 AND token=$4 \
             AND expires_at>clock_timestamp() RETURNING lease",
        )
        .bind(binding_id.as_str())
        .bind(u64_to_i64(generation.0, "lease generation")?)
        .bind(owner)
        .bind(token)
        .bind(database_expires_at)
        .fetch_optional(&self.pool)
        .await
        .map_err(sql_error)?
        .ok_or_else(|| HostStoreError::Conflict {
            entity: "agent_runtime_driver_lease",
            id: binding_id.to_string(),
            expected: None,
            actual: None,
        })?;
        decode(row.get("lease"), "agent_runtime_driver_lease.lease")
    }

    async fn release_lease(
        &self,
        binding_id: &RuntimeBindingId,
        generation: RuntimeDriverGeneration,
        owner: &str,
        token: &str,
    ) -> Result<(), HostStoreError> {
        let result = sqlx::query(
            "DELETE FROM agent_runtime_driver_lease WHERE binding_id=$1 \
             AND driver_generation=$2 AND owner=$3 AND token=$4",
        )
        .bind(binding_id.as_str())
        .bind(u64_to_i64(generation.0, "lease generation")?)
        .bind(owner)
        .bind(token)
        .execute(&self.pool)
        .await
        .map_err(sql_error)?;
        if result.rows_affected() != 1 {
            return Err(HostStoreError::Conflict {
                entity: "agent_runtime_driver_lease",
                id: binding_id.to_string(),
                expected: None,
                actual: None,
            });
        }
        Ok(())
    }

    async fn record_driver_coordinate(
        &self,
        binding_id: &RuntimeBindingId,
        generation: RuntimeDriverGeneration,
        coordinate: RuntimeDriverCoordinate,
    ) -> Result<(), HostStoreError> {
        let result = sqlx::query(
            "INSERT INTO agent_runtime_driver_coordinate \
             (binding_id,driver_generation,coordinate_kind,runtime_id,source_id,coordinate) \
             SELECT $1,$2,$3,$4,$5,$6 \
             FROM agent_runtime_host_binding h \
             WHERE h.binding_id=$1 AND h.driver_generation=$2 AND h.state='active' \
             ON CONFLICT (binding_id,driver_generation,coordinate_kind,runtime_id) DO UPDATE \
             SET coordinate=EXCLUDED.coordinate \
             WHERE agent_runtime_driver_coordinate.source_id=EXCLUDED.source_id",
        )
        .bind(binding_id.as_str())
        .bind(u64_to_i64(generation.0, "driver coordinate generation")?)
        .bind(coordinate.kind())
        .bind(coordinate.runtime_id())
        .bind(coordinate.source_id())
        .bind(encode(
            &coordinate,
            "agent_runtime_driver_coordinate.coordinate",
        )?)
        .execute(&self.pool)
        .await
        .map_err(sql_error)?;
        if result.rows_affected() != 1 {
            return Err(HostStoreError::Invariant {
                reason:
                    "driver coordinate targets a stale binding or remaps an existing runtime id"
                        .to_string(),
            });
        }
        Ok(())
    }

    async fn validate_lease(
        &self,
        binding_id: &RuntimeBindingId,
        generation: RuntimeDriverGeneration,
        owner: &str,
        token: &str,
        _now: DateTime<Utc>,
    ) -> Result<DriverLease, HostStoreError> {
        let row = sqlx::query(
            "SELECT lease FROM agent_runtime_driver_lease WHERE binding_id=$1 AND driver_generation=$2 \
             AND owner=$3 AND token=$4 AND expires_at>clock_timestamp()",
        )
        .bind(binding_id.as_str())
        .bind(u64_to_i64(generation.0, "lease generation")?)
        .bind(owner)
        .bind(token)
        .fetch_optional(&self.pool)
        .await
        .map_err(sql_error)?
        .ok_or_else(|| HostStoreError::Invariant {
            reason: "driver lease is stale or expired".to_string(),
        })?;
        decode(row.get("lease"), "agent_runtime_driver_lease.lease")
    }

    async fn mark_binding_lost(
        &self,
        binding_id: &RuntimeBindingId,
        generation: RuntimeDriverGeneration,
    ) -> Result<(), HostStoreError> {
        let mut transaction = self.pool.begin().await.map_err(sql_error)?;
        let mut binding = locked_binding(&mut transaction, binding_id).await?;
        if binding.driver_generation != generation {
            return Err(HostStoreError::Invariant {
                reason: "cannot mark a different binding generation lost".to_string(),
            });
        }
        binding.state = RuntimeBindingState::Lost;
        sqlx::query(
            "UPDATE agent_runtime_host_binding SET state='lost',binding=$2,updated_at=now() WHERE binding_id=$1",
        )
        .bind(binding_id.as_str())
        .bind(encode(&binding, "agent_runtime_host_binding.binding")?)
        .execute(&mut *transaction)
        .await
        .map_err(sql_error)?;
        transaction.commit().await.map_err(sql_error)
    }

    async fn profile_digest_for_binding(
        &self,
        binding_id: &RuntimeBindingId,
    ) -> Result<Option<ProfileDigest>, HostStoreError> {
        let value: Option<String> = sqlx::query_scalar(
            "SELECT profile_digest FROM agent_runtime_host_binding WHERE binding_id=$1",
        )
        .bind(binding_id.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(sql_error)?;
        value
            .map(|value| {
                ProfileDigest::new(value).map_err(|error| HostStoreError::Persistence {
                    reason: error.to_string(),
                })
            })
            .transpose()
    }
}
