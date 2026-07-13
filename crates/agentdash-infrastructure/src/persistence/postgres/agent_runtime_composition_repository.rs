use agentdash_application_ports::agent_run_runtime::{
    AgentRunRuntimeBinding, AgentRunRuntimeBindingError, AgentRunRuntimeBindingRepository,
    AgentRunRuntimeRecoveryIntent, AgentRunRuntimeRecoveryState, AgentRunRuntimeTarget,
};
use agentdash_integration_api::{
    AgentRuntimeSurfaceBroker, DriverSurfaceError, DriverSurfaceRequest, DriverToolSurface,
    MaterializedDriverSurface,
};
use async_trait::async_trait;
use sqlx::{PgPool, Row};

#[derive(Clone)]
pub struct PostgresAgentRuntimeCompositionRepository {
    pool: PgPool,
}

impl PostgresAgentRuntimeCompositionRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn load_by_runtime_binding(
        &self,
        binding_id: &agentdash_agent_runtime_contract::RuntimeBindingId,
    ) -> Result<Option<AgentRunRuntimeBinding>, AgentRunRuntimeBindingError> {
        let row = sqlx::query(
            "SELECT binding FROM agent_run_runtime_binding_lineage WHERE runtime_binding_id=$1",
        )
        .bind(binding_id.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(binding_sql_error)?;
        row.map(|row| {
            serde_json::from_value(row.get("binding")).map_err(|error| {
                AgentRunRuntimeBindingError::Persistence {
                    reason: format!("agent_run_runtime_binding.binding: {error}"),
                }
            })
        })
        .transpose()
    }

    pub async fn put_surface(
        &self,
        binding_id: &agentdash_agent_runtime_contract::RuntimeBindingId,
        surface: &MaterializedDriverSurface,
    ) -> Result<(), DriverSurfaceError> {
        let materialized = serde_json::to_value(surface).map_err(|error| {
            DriverSurfaceError::InvalidMaterialization {
                reason: error.to_string(),
            }
        })?;
        let result = sqlx::query(
            "INSERT INTO agent_runtime_surface_snapshot \
             (binding_id,surface_revision,surface_digest,tool_set_revision,tool_set_digest,hook_plan_revision,hook_plan_digest,materialized) \
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT (binding_id,surface_revision,surface_digest) DO NOTHING",
        )
        .bind(binding_id.as_str())
        .bind(i64::try_from(surface.revision.0).map_err(|_| invalid_surface("surface revision"))?)
        .bind(surface.digest.as_str())
        .bind(i64::try_from(surface.tools.revision.0).map_err(|_| invalid_surface("tool revision"))?)
        .bind(&surface.tools.digest)
        .bind(i64::try_from(surface.hooks.revision.0).map_err(|_| invalid_surface("hook revision"))?)
        .bind(surface.hooks.digest.as_str())
        .bind(materialized.clone())
        .execute(&self.pool)
        .await
        .map_err(surface_sql_error)?;
        if result.rows_affected() == 0 {
            let existing: serde_json::Value = sqlx::query_scalar(
                "SELECT materialized FROM agent_runtime_surface_snapshot WHERE binding_id=$1 AND surface_revision=$2 AND surface_digest=$3",
            )
            .bind(binding_id.as_str())
            .bind(i64::try_from(surface.revision.0).map_err(|_| invalid_surface("surface revision"))?)
            .bind(surface.digest.as_str())
            .fetch_one(&self.pool)
            .await
            .map_err(surface_sql_error)?;
            if existing != materialized {
                return Err(DriverSurfaceError::InvalidMaterialization {
                    reason: "binding surface identity was reused with different content"
                        .to_string(),
                });
            }
        }
        Ok(())
    }

    pub async fn load_bound_surface(
        &self,
        binding_id: &agentdash_agent_runtime_contract::RuntimeBindingId,
    ) -> Result<Option<MaterializedDriverSurface>, DriverSurfaceError> {
        let row = sqlx::query(
            "SELECT materialized FROM agent_runtime_surface_snapshot WHERE binding_id=$1 ORDER BY surface_revision DESC LIMIT 1",
        )
        .bind(binding_id.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(surface_sql_error)?;
        row.map(|row| {
            serde_json::from_value(row.get("materialized")).map_err(|error| {
                DriverSurfaceError::InvalidMaterialization {
                    reason: format!("agent_runtime_surface_snapshot.materialized: {error}"),
                }
            })
        })
        .transpose()
    }
}

#[async_trait]
impl AgentRuntimeSurfaceBroker for PostgresAgentRuntimeCompositionRepository {
    async fn materialize(
        &self,
        request: DriverSurfaceRequest,
    ) -> Result<MaterializedDriverSurface, DriverSurfaceError> {
        let row = sqlx::query(
            "SELECT materialized FROM agent_runtime_surface_snapshot WHERE binding_id=$1 AND surface_revision=$2 AND surface_digest=$3",
        )
        .bind(request.binding_id.as_str())
        .bind(i64::try_from(request.surface_revision.0).map_err(|_| invalid_surface("surface revision"))?)
        .bind(request.surface_digest.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(surface_sql_error)?;
        let surface: MaterializedDriverSurface = row
            .map(|row| serde_json::from_value(row.get("materialized")))
            .transpose()
            .map_err(|error| DriverSurfaceError::InvalidMaterialization {
                reason: error.to_string(),
            })?
            .ok_or_else(|| DriverSurfaceError::Unavailable {
                reason: "bound surface does not exist".to_string(),
                retryable: false,
            })?;
        if surface.revision != request.surface_revision || surface.digest != request.surface_digest
        {
            return Err(DriverSurfaceError::Stale);
        }
        Ok(surface)
    }

    async fn materialize_tool_set(
        &self,
        binding_id: agentdash_agent_runtime_contract::RuntimeBindingId,
        revision: agentdash_agent_runtime_contract::ToolSetRevision,
        digest: &str,
    ) -> Result<DriverToolSurface, DriverSurfaceError> {
        let row = sqlx::query(
            "SELECT materialized FROM agent_runtime_surface_snapshot WHERE binding_id=$1 AND tool_set_revision=$2 AND tool_set_digest=$3 ORDER BY surface_revision DESC LIMIT 1",
        )
        .bind(binding_id.as_str())
        .bind(i64::try_from(revision.0).map_err(|_| invalid_surface("tool revision"))?)
        .bind(digest)
        .fetch_optional(&self.pool)
        .await
        .map_err(surface_sql_error)?;
        let surface: MaterializedDriverSurface = row
            .map(|row| serde_json::from_value(row.get("materialized")))
            .transpose()
            .map_err(|error| DriverSurfaceError::InvalidMaterialization {
                reason: error.to_string(),
            })?
            .ok_or(DriverSurfaceError::Stale)?;
        Ok(surface.tools)
    }
}

#[async_trait]
impl agentdash_agent_runtime::RuntimeSurfaceReferenceValidator
    for PostgresAgentRuntimeCompositionRepository
{
    async fn validate_surface_reference(
        &self,
        binding_id: &agentdash_agent_runtime_contract::RuntimeBindingId,
        runtime_thread_id: &agentdash_agent_runtime_contract::RuntimeThreadId,
        target: &agentdash_agent_runtime_contract::RuntimeSurfaceDescriptor,
    ) -> Result<(), String> {
        let surface = AgentRuntimeSurfaceBroker::materialize(
            self,
            DriverSurfaceRequest {
                binding_id: binding_id.clone(),
                surface_revision: target.surface_revision,
                surface_digest: target.surface_digest.clone(),
            },
        )
        .await
        .map_err(|error| error.to_string())?;
        if &surface.runtime_thread_id != runtime_thread_id
            || surface.workspace.digest != target.vfs_digest
            || surface.context.recipe.revision != target.context_recipe_revision
            || surface.context.recipe.provenance.settings_revision != target.settings_revision
            || surface.context.digest != target.context_digest
            || surface.tools.revision != target.tool_set_revision
            || surface.tools.digest != target.tool_set_digest
            || surface.hooks.revision != target.hook_plan.revision
            || surface.hooks.digest != target.hook_plan.digest
        {
            return Err(
                "materialized Runtime surface components do not match the adoption descriptor"
                    .to_string(),
            );
        }
        Ok(())
    }
}

#[async_trait]
impl AgentRunRuntimeBindingRepository for PostgresAgentRuntimeCompositionRepository {
    async fn load(
        &self,
        target: &AgentRunRuntimeTarget,
    ) -> Result<Option<AgentRunRuntimeBinding>, AgentRunRuntimeBindingError> {
        let row = sqlx::query(CURRENT_BINDING_BY_TARGET)
            .bind(target.run_id.to_string())
            .bind(target.agent_id.to_string())
            .fetch_optional(&self.pool)
            .await
            .map_err(binding_sql_error)?;
        row.map(|row| {
            serde_json::from_value(row.get("binding")).map_err(|error| {
                AgentRunRuntimeBindingError::Persistence {
                    reason: format!("agent_run_runtime_binding.binding: {error}"),
                }
            })
        })
        .transpose()
    }

    async fn load_by_thread_id(
        &self,
        thread_id: &agentdash_agent_runtime_contract::RuntimeThreadId,
    ) -> Result<Option<AgentRunRuntimeBinding>, AgentRunRuntimeBindingError> {
        let row = sqlx::query(CURRENT_BINDING_BY_THREAD)
            .bind(thread_id.as_str())
            .fetch_optional(&self.pool)
            .await
            .map_err(binding_sql_error)?;
        row.map(|row| {
            serde_json::from_value(row.get("binding")).map_err(|error| {
                AgentRunRuntimeBindingError::Persistence {
                    reason: format!("agent_run_runtime_binding.binding: {error}"),
                }
            })
        })
        .transpose()
    }

    async fn list_by_run(
        &self,
        run_id: uuid::Uuid,
    ) -> Result<Vec<AgentRunRuntimeBinding>, AgentRunRuntimeBindingError> {
        load_runtime_bindings(
            sqlx::query(&format!("{CURRENT_BINDINGS} WHERE a.run_id=$1"))
                .bind(run_id.to_string())
                .fetch_all(&self.pool)
                .await
                .map_err(binding_sql_error)?,
        )
    }

    async fn list_by_agent(
        &self,
        agent_id: uuid::Uuid,
    ) -> Result<Vec<AgentRunRuntimeBinding>, AgentRunRuntimeBindingError> {
        load_runtime_bindings(
            sqlx::query(&format!("{CURRENT_BINDINGS} WHERE a.agent_id=$1"))
                .bind(agent_id.to_string())
                .fetch_all(&self.pool)
                .await
                .map_err(binding_sql_error)?,
        )
    }

    async fn insert(
        &self,
        binding: AgentRunRuntimeBinding,
    ) -> Result<AgentRunRuntimeBinding, AgentRunRuntimeBindingError> {
        let document = serde_json::to_value(&binding).map_err(|error| {
            AgentRunRuntimeBindingError::Persistence {
                reason: error.to_string(),
            }
        })?;
        if binding.binding_epoch.0 != 1 {
            return Err(AgentRunRuntimeBindingError::Conflict);
        }
        let mut tx = self.pool.begin().await.map_err(binding_sql_error)?;
        let result = sqlx::query(
            "INSERT INTO agent_run_runtime_thread_anchor \
             (run_id,agent_id,runtime_thread_id,bootstrap_runtime_binding_id) \
             VALUES ($1,$2,$3,$4) ON CONFLICT (run_id,agent_id) DO NOTHING",
        )
        .bind(binding.target.run_id.to_string())
        .bind(binding.target.agent_id.to_string())
        .bind(binding.thread_id.as_str())
        .bind(binding.binding_id.as_str())
        .execute(&mut *tx)
        .await
        .map_err(binding_sql_error)?;
        if result.rows_affected() == 0 {
            tx.rollback().await.map_err(binding_sql_error)?;
            let existing = self
                .load(&binding.target)
                .await?
                .ok_or(AgentRunRuntimeBindingError::Conflict)?;
            if existing != binding {
                return Err(AgentRunRuntimeBindingError::Conflict);
            }
            return Ok(existing);
        }
        sqlx::query(
            "INSERT INTO agent_run_runtime_binding_lineage \
             (run_id,agent_id,binding_epoch,runtime_binding_id,binding) VALUES ($1,$2,1,$3,$4)",
        )
        .bind(binding.target.run_id.to_string())
        .bind(binding.target.agent_id.to_string())
        .bind(binding.binding_id.as_str())
        .bind(document)
        .execute(&mut *tx)
        .await
        .map_err(binding_sql_error)?;
        tx.commit().await.map_err(binding_sql_error)?;
        Ok(binding)
    }

    async fn lineage(
        &self,
        target: &AgentRunRuntimeTarget,
    ) -> Result<Vec<AgentRunRuntimeBinding>, AgentRunRuntimeBindingError> {
        load_runtime_bindings(sqlx::query("SELECT binding FROM agent_run_runtime_binding_lineage WHERE run_id=$1 AND agent_id=$2 ORDER BY binding_epoch")
            .bind(target.run_id.to_string()).bind(target.agent_id.to_string()).fetch_all(&self.pool).await.map_err(binding_sql_error)?)
    }

    async fn append_lineage(
        &self,
        expected: &AgentRunRuntimeBinding,
        binding: AgentRunRuntimeBinding,
        recovery_intent_id: &str,
    ) -> Result<AgentRunRuntimeBinding, AgentRunRuntimeBindingError> {
        if binding.target != expected.target
            || binding.thread_id != expected.thread_id
            || binding.binding_epoch <= expected.binding_epoch
        {
            return Err(AgentRunRuntimeBindingError::Conflict);
        }
        let document = serde_json::to_value(&binding).map_err(|e| {
            AgentRunRuntimeBindingError::Persistence {
                reason: e.to_string(),
            }
        })?;
        let result = sqlx::query("INSERT INTO agent_run_runtime_binding_lineage (run_id,agent_id,binding_epoch,runtime_binding_id,binding,recovery_intent_id) SELECT $1,$2,$3,$4,$5,$6 WHERE EXISTS (SELECT 1 FROM agent_runtime_thread WHERE id=$7 AND binding_id=$8 AND driver_generation=$9) ON CONFLICT (run_id,agent_id,binding_epoch) DO NOTHING")
            .bind(binding.target.run_id.to_string()).bind(binding.target.agent_id.to_string())
            .bind(i64::try_from(binding.binding_epoch.0).map_err(|_| AgentRunRuntimeBindingError::Conflict)?)
            .bind(binding.binding_id.as_str()).bind(document).bind(recovery_intent_id)
            .bind(binding.thread_id.as_str()).bind(expected.binding_id.as_str())
            .bind(i64::try_from(expected.driver_generation.0).map_err(|_| AgentRunRuntimeBindingError::Conflict)?)
            .execute(&self.pool).await.map_err(binding_sql_error)?;
        if result.rows_affected() == 0 {
            let existing = self
                .lineage(&binding.target)
                .await?
                .into_iter()
                .find(|item| item.binding_epoch == binding.binding_epoch)
                .ok_or(AgentRunRuntimeBindingError::Conflict)?;
            if existing != binding {
                return Err(AgentRunRuntimeBindingError::Conflict);
            }
            return Ok(existing);
        }
        Ok(binding)
    }

    async fn load_active_recovery(
        &self,
        target: &AgentRunRuntimeTarget,
    ) -> Result<Option<AgentRunRuntimeRecoveryIntent>, AgentRunRuntimeBindingError> {
        load_recovery(sqlx::query("SELECT intent FROM agent_run_runtime_recovery_intent WHERE run_id=$1 AND agent_id=$2 AND state IN ('prepared','host_bound')")
            .bind(target.run_id.to_string()).bind(target.agent_id.to_string()).fetch_optional(&self.pool).await.map_err(binding_sql_error)?)
    }

    async fn load_latest_recovery(
        &self,
        target: &AgentRunRuntimeTarget,
    ) -> Result<Option<AgentRunRuntimeRecoveryIntent>, AgentRunRuntimeBindingError> {
        load_recovery(sqlx::query("SELECT intent FROM agent_run_runtime_recovery_intent WHERE run_id=$1 AND agent_id=$2 ORDER BY created_at DESC, id DESC LIMIT 1")
            .bind(target.run_id.to_string()).bind(target.agent_id.to_string()).fetch_optional(&self.pool).await.map_err(binding_sql_error)?)
    }

    async fn prepare_recovery(
        &self,
        mut intent: AgentRunRuntimeRecoveryIntent,
    ) -> Result<AgentRunRuntimeRecoveryIntent, AgentRunRuntimeBindingError> {
        let mut tx = self.pool.begin().await.map_err(binding_sql_error)?;
        sqlx::query("SELECT 1 FROM agent_run_runtime_thread_anchor WHERE run_id=$1 AND agent_id=$2 FOR UPDATE")
            .bind(intent.target.run_id.to_string())
            .bind(intent.target.agent_id.to_string())
            .fetch_one(&mut *tx).await.map_err(binding_sql_error)?;
        if let Some(row) = sqlx::query("SELECT intent FROM agent_run_runtime_recovery_intent WHERE run_id=$1 AND agent_id=$2 AND state IN ('prepared','host_bound')")
            .bind(intent.target.run_id.to_string()).bind(intent.target.agent_id.to_string())
            .fetch_optional(&mut *tx).await.map_err(binding_sql_error)? {
            let active = load_recovery(Some(row))?.ok_or(AgentRunRuntimeBindingError::Conflict)?;
            tx.commit().await.map_err(binding_sql_error)?;
            return Ok(active);
        }
        let max_epoch: i64 = sqlx::query_scalar(
            "SELECT GREATEST( \
                COALESCE((SELECT MAX(binding_epoch) FROM agent_run_runtime_binding_lineage WHERE run_id=$1 AND agent_id=$2),0), \
                COALESCE((SELECT MAX(binding_epoch) FROM agent_run_runtime_recovery_intent WHERE run_id=$1 AND agent_id=$2),0))",
        )
        .bind(intent.target.run_id.to_string()).bind(intent.target.agent_id.to_string())
        .fetch_one(&mut *tx).await.map_err(binding_sql_error)?;
        let (epoch, proposed, id) = recovery_attempt_identity(
            &intent,
            u64::try_from(max_epoch).map_err(|_| AgentRunRuntimeBindingError::Conflict)?,
        )?;
        intent.binding_epoch = epoch;
        intent.proposed_binding_id = proposed;
        intent.id = id;
        let document = serde_json::to_value(&intent).map_err(|e| {
            AgentRunRuntimeBindingError::Persistence {
                reason: e.to_string(),
            }
        })?;
        let result = sqlx::query("INSERT INTO agent_run_runtime_recovery_intent (id,run_id,agent_id,runtime_thread_id,expected_old_binding_id,expected_old_generation,expected_runtime_revision,binding_epoch,proposed_binding_id,selected_offer_id,source_thread_id,state,failure_reason,intent) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,'prepared',NULL,$12) ON CONFLICT DO NOTHING")
            .bind(&intent.id).bind(intent.target.run_id.to_string()).bind(intent.target.agent_id.to_string()).bind(intent.thread_id.as_str()).bind(intent.expected_old_binding_id.as_str())
            .bind(i64::try_from(intent.expected_old_generation.0).map_err(|_| AgentRunRuntimeBindingError::Conflict)?).bind(i64::try_from(intent.expected_runtime_revision.0).map_err(|_| AgentRunRuntimeBindingError::Conflict)?)
            .bind(i64::try_from(intent.binding_epoch.0).map_err(|_| AgentRunRuntimeBindingError::Conflict)?).bind(intent.proposed_binding_id.as_str()).bind(&intent.selected_offer_id).bind(intent.source_thread_id.as_str()).bind(document)
            .execute(&mut *tx).await.map_err(binding_sql_error)?;
        if result.rows_affected() == 1 {
            tx.commit().await.map_err(binding_sql_error)?;
            return Ok(intent);
        }
        tx.rollback().await.map_err(binding_sql_error)?;
        Err(AgentRunRuntimeBindingError::Conflict)
    }

    async fn advance_recovery(
        &self,
        intent_id: &str,
        expected: AgentRunRuntimeRecoveryState,
        next: AgentRunRuntimeRecoveryState,
        failure_reason: Option<String>,
    ) -> Result<AgentRunRuntimeRecoveryIntent, AgentRunRuntimeBindingError> {
        let row = sqlx::query("SELECT intent FROM agent_run_runtime_recovery_intent WHERE id=$1")
            .bind(intent_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(binding_sql_error)?;
        let mut intent = load_recovery(row)?.ok_or(AgentRunRuntimeBindingError::NotFound)?;
        if intent.state == next {
            return Ok(intent);
        }
        if intent.state != expected {
            return Err(AgentRunRuntimeBindingError::Conflict);
        }
        intent.state = next;
        intent.failure_reason = failure_reason.clone();
        let document = serde_json::to_value(&intent).map_err(|e| {
            AgentRunRuntimeBindingError::Persistence {
                reason: e.to_string(),
            }
        })?;
        let result = sqlx::query("UPDATE agent_run_runtime_recovery_intent SET state=$2,failure_reason=$3,intent=$4,updated_at=now() WHERE id=$1 AND state=$5")
            .bind(intent_id).bind(recovery_state(next)).bind(failure_reason).bind(document).bind(recovery_state(expected)).execute(&self.pool).await.map_err(binding_sql_error)?;
        if result.rows_affected() != 1 {
            return Err(AgentRunRuntimeBindingError::Conflict);
        }
        Ok(intent)
    }
}

const CURRENT_BINDINGS: &str = "SELECT l.binding FROM agent_run_runtime_thread_anchor a LEFT JOIN agent_runtime_thread t ON t.id=a.runtime_thread_id JOIN agent_run_runtime_binding_lineage l ON l.runtime_binding_id=COALESCE(t.binding_id,a.bootstrap_runtime_binding_id)";
const CURRENT_BINDING_BY_TARGET: &str = "SELECT l.binding FROM agent_run_runtime_thread_anchor a LEFT JOIN agent_runtime_thread t ON t.id=a.runtime_thread_id JOIN agent_run_runtime_binding_lineage l ON l.runtime_binding_id=COALESCE(t.binding_id,a.bootstrap_runtime_binding_id) WHERE a.run_id=$1 AND a.agent_id=$2";
const CURRENT_BINDING_BY_THREAD: &str = "SELECT l.binding FROM agent_run_runtime_thread_anchor a LEFT JOIN agent_runtime_thread t ON t.id=a.runtime_thread_id JOIN agent_run_runtime_binding_lineage l ON l.runtime_binding_id=COALESCE(t.binding_id,a.bootstrap_runtime_binding_id) WHERE a.runtime_thread_id=$1";

fn load_recovery(
    row: Option<sqlx::postgres::PgRow>,
) -> Result<Option<AgentRunRuntimeRecoveryIntent>, AgentRunRuntimeBindingError> {
    row.map(|row| {
        serde_json::from_value(row.get("intent")).map_err(|e| {
            AgentRunRuntimeBindingError::Persistence {
                reason: format!("agent_run_runtime_recovery_intent.intent: {e}"),
            }
        })
    })
    .transpose()
}

fn recovery_state(state: AgentRunRuntimeRecoveryState) -> &'static str {
    match state {
        AgentRunRuntimeRecoveryState::Prepared => "prepared",
        AgentRunRuntimeRecoveryState::HostBound => "host_bound",
        AgentRunRuntimeRecoveryState::Committed => "committed",
        AgentRunRuntimeRecoveryState::Failed => "failed",
    }
}

fn recovery_attempt_identity(
    intent: &AgentRunRuntimeRecoveryIntent,
    max_historical_epoch: u64,
) -> Result<
    (
        agentdash_agent_runtime_contract::BindingEpoch,
        agentdash_agent_runtime_contract::RuntimeBindingId,
        String,
    ),
    AgentRunRuntimeBindingError,
> {
    let next_epoch = max_historical_epoch
        .checked_add(1)
        .ok_or(AgentRunRuntimeBindingError::Conflict)?;
    let binding_id = agentdash_agent_runtime_contract::RuntimeBindingId::new(format!(
        "{}-epoch-{next_epoch}",
        intent.expected_old_binding_id.as_str()
    ))
    .map_err(|error| AgentRunRuntimeBindingError::Persistence {
        reason: error.to_string(),
    })?;
    Ok((
        agentdash_agent_runtime_contract::BindingEpoch(next_epoch),
        binding_id,
        format!(
            "recovery-{}-{}-{}-{}",
            intent.target.run_id, intent.target.agent_id, next_epoch, intent.selected_offer_id
        ),
    ))
}

#[cfg(test)]
mod recovery_tests {
    use super::*;

    #[test]
    fn failed_epoch_two_allocates_distinct_epoch_three_attempt() {
        let intent = AgentRunRuntimeRecoveryIntent {
            id: String::new(),
            target: AgentRunRuntimeTarget {
                run_id: uuid::Uuid::nil(),
                agent_id: uuid::Uuid::from_u128(1),
            },
            thread_id: agentdash_agent_runtime_contract::RuntimeThreadId::new("thread-1").unwrap(),
            expected_old_binding_id: agentdash_agent_runtime_contract::RuntimeBindingId::new(
                "binding-1",
            )
            .unwrap(),
            expected_old_generation: agentdash_agent_runtime_contract::RuntimeDriverGeneration(1),
            expected_runtime_revision: agentdash_agent_runtime_contract::RuntimeRevision(4),
            binding_epoch: agentdash_agent_runtime_contract::BindingEpoch(2),
            proposed_binding_id: agentdash_agent_runtime_contract::RuntimeBindingId::new("unused")
                .unwrap(),
            selected_offer_id: "offer-new".to_string(),
            source_thread_id: agentdash_agent_runtime_contract::DriverThreadId::new("source-1")
                .unwrap(),
            state: AgentRunRuntimeRecoveryState::Prepared,
            failure_reason: None,
        };
        let (epoch, binding_id, id) = recovery_attempt_identity(&intent, 2).unwrap();
        assert_eq!(epoch, agentdash_agent_runtime_contract::BindingEpoch(3));
        assert_eq!(binding_id.as_str(), "binding-1-epoch-3");
        assert!(id.contains("-3-offer-new"));
    }
}

fn load_runtime_bindings(
    rows: Vec<sqlx::postgres::PgRow>,
) -> Result<Vec<AgentRunRuntimeBinding>, AgentRunRuntimeBindingError> {
    rows.into_iter()
        .map(|row| {
            serde_json::from_value(row.get("binding")).map_err(|error| {
                AgentRunRuntimeBindingError::Persistence {
                    reason: format!("agent_run_runtime_binding.binding: {error}"),
                }
            })
        })
        .collect()
}

fn invalid_surface(field: &'static str) -> DriverSurfaceError {
    DriverSurfaceError::InvalidMaterialization {
        reason: format!("{field} exceeds PostgreSQL bigint"),
    }
}

fn surface_sql_error(error: sqlx::Error) -> DriverSurfaceError {
    DriverSurfaceError::Unavailable {
        reason: error.to_string(),
        retryable: true,
    }
}

fn binding_sql_error(error: sqlx::Error) -> AgentRunRuntimeBindingError {
    if let sqlx::Error::Database(database) = &error
        && matches!(database.code().as_deref(), Some("23505" | "23503"))
    {
        return AgentRunRuntimeBindingError::Conflict;
    }
    AgentRunRuntimeBindingError::Persistence {
        reason: error.to_string(),
    }
}
