use agentdash_agent_runtime_contract::RuntimeThreadId;
use agentdash_application_agentrun::agent_run::{
    AgentRunCommittedProductRuntimeBinding, AgentRunProductRuntimeBinding,
    AgentRunProductRuntimeBindingRepository, AgentRunProductRuntimeBindingStore,
    AgentRunTerminalAvailability, AgentRunTerminalChange, AgentRunTerminalChangeGap,
    AgentRunTerminalChangeOrigin, AgentRunTerminalChangeSequence, AgentRunTerminalControlRoute,
    AgentRunTerminalControlRoutingRepository, AgentRunTerminalProjection,
    AgentRunTerminalProjectionCommit, AgentRunTerminalProjectionDelta,
    AgentRunTerminalProjectionHead, AgentRunTerminalProjectionRepository,
    AgentRunTerminalProjectionRevision, AgentRunTerminalProjectionStoreError,
    AgentRunTerminalProjectionUnitOfWork, AgentRunTerminalSnapshot,
    AgentRunTerminalSourceProjectionLookup,
};
use agentdash_domain::agent_run_target::AgentRunTarget;
use async_trait::async_trait;
use serde::{Serialize, de::DeserializeOwned};
use serde_json::Value;
use sqlx::{PgPool, Postgres, Row, Transaction};
use uuid::Uuid;

#[derive(Clone)]
pub struct PostgresAgentRunProductRuntimeBindingRepository {
    pool: PgPool,
}

impl PostgresAgentRunProductRuntimeBindingRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn commit_product_binding(
        &self,
        binding: &AgentRunProductRuntimeBinding,
    ) -> Result<AgentRunCommittedProductRuntimeBinding, String> {
        let receipt = binding.committed_receipt()?;
        let binding_json = product_binding_json(binding)?;
        let result = sqlx::query(
            "UPDATE lifecycle_agents
             SET runtime_binding=$3
             WHERE id=$1 AND run_id=$2
               AND (runtime_binding IS NULL OR runtime_binding=$3)",
        )
        .bind(binding.target.agent_id.to_string())
        .bind(binding.target.run_id.to_string())
        .bind(&binding_json)
        .execute(&self.pool)
        .await
        .map_err(string_db_error)?;
        if result.rows_affected() == 0 {
            return Err(
                "LifecycleAgent does not exist or already owns a different Agent association"
                    .to_string(),
            );
        }
        Ok(receipt)
    }

    pub async fn replace_product_binding(
        &self,
        expected_previous_binding_digest: &str,
        binding: &AgentRunProductRuntimeBinding,
    ) -> Result<AgentRunCommittedProductRuntimeBinding, String> {
        if expected_previous_binding_digest.trim().is_empty() {
            return Err("Product binding replacement requires a previous digest".to_string());
        }
        let receipt = binding.committed_receipt()?;
        let binding_json = product_binding_json(binding)?;
        let mut tx = self.pool.begin().await.map_err(string_db_error)?;
        let current = sqlx::query_scalar::<_, Value>(
            "SELECT runtime_binding
             FROM lifecycle_agents
             WHERE id=$1 AND run_id=$2 AND runtime_binding IS NOT NULL
             FOR UPDATE",
        )
        .bind(binding.target.agent_id.to_string())
        .bind(binding.target.run_id.to_string())
        .fetch_optional(&mut *tx)
        .await
        .map_err(string_db_error)?
        .ok_or_else(|| "LifecycleAgent has no Product Agent association".to_string())?;
        let current = map_product_binding_document(binding.target.clone(), current)?;
        let current_digest = current.committed_receipt()?.binding_digest;
        if current_digest != expected_previous_binding_digest {
            return Err("AgentRun Product binding replacement CAS conflict".to_string());
        }
        let result = sqlx::query(
            "UPDATE lifecycle_agents
             SET runtime_binding=$3
             WHERE id=$1 AND run_id=$2 AND runtime_binding=$4",
        )
        .bind(binding.target.agent_id.to_string())
        .bind(binding.target.run_id.to_string())
        .bind(&binding_json)
        .bind(product_binding_json(&current)?)
        .execute(&mut *tx)
        .await
        .map_err(string_db_error)?;
        if result.rows_affected() != 1 {
            return Err("AgentRun Product binding replacement CAS conflict".to_string());
        }
        tx.commit().await.map_err(string_db_error)?;
        Ok(receipt)
    }

    pub async fn load_committed_tool_binding(
        &self,
        runtime_thread_id: &RuntimeThreadId,
    ) -> Result<Option<crate::CommittedRuntimeToolProductBinding>, String> {
        let row = sqlx::query(
            "SELECT id,run_id,runtime_binding
             FROM lifecycle_agents
             WHERE runtime_binding ->> 'runtime_thread_id'=$1",
        )
        .bind(runtime_thread_id.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(string_db_error)?;
        let Some(row) = row else {
            return Ok(None);
        };
        let target = AgentRunTarget {
            run_id: Uuid::parse_str(
                &row.try_get::<String, _>("run_id")
                    .map_err(string_db_error)?,
            )
            .map_err(|error| error.to_string())?,
            agent_id: Uuid::parse_str(&row.try_get::<String, _>("id").map_err(string_db_error)?)
                .map_err(|error| error.to_string())?,
        };
        let binding = map_product_binding_document(
            target,
            row.try_get("runtime_binding").map_err(string_db_error)?,
        )?;
        let binding_digest = binding.committed_receipt()?.binding_digest;
        Ok(Some(crate::CommittedRuntimeToolProductBinding {
            binding,
            binding_digest,
        }))
    }

    pub async fn load_product_binding_by_runtime_thread(
        &self,
        runtime_thread_id: &RuntimeThreadId,
    ) -> Result<Option<AgentRunProductRuntimeBinding>, String> {
        let row = sqlx::query(
            "SELECT id,run_id,runtime_binding
             FROM lifecycle_agents
             WHERE runtime_binding ->> 'runtime_thread_id'=$1",
        )
        .bind(runtime_thread_id.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(string_db_error)?;
        let Some(row) = row else {
            return Ok(None);
        };
        let run_id = Uuid::parse_str(
            &row.try_get::<String, _>("run_id")
                .map_err(string_db_error)?,
        )
        .map_err(|error| error.to_string())?;
        let agent_id = Uuid::parse_str(&row.try_get::<String, _>("id").map_err(string_db_error)?)
            .map_err(|error| error.to_string())?;
        map_product_binding_document(
            AgentRunTarget { run_id, agent_id },
            row.try_get("runtime_binding").map_err(string_db_error)?,
        )
        .map(Some)
    }
}

pub fn product_runtime_binding_digest(
    binding: &AgentRunProductRuntimeBinding,
) -> Result<String, String> {
    binding.calculated_digest()
}

fn product_binding_json(binding: &AgentRunProductRuntimeBinding) -> Result<Value, String> {
    serde_json::to_value(binding).map_err(|error| error.to_string())
}

#[async_trait]
impl AgentRunProductRuntimeBindingRepository for PostgresAgentRunProductRuntimeBindingRepository {
    async fn load_product_binding(
        &self,
        target: &AgentRunTarget,
    ) -> Result<Option<AgentRunProductRuntimeBinding>, String> {
        let value = sqlx::query_scalar::<_, Value>(
            "SELECT runtime_binding
             FROM lifecycle_agents
             WHERE run_id=$1 AND id=$2 AND runtime_binding IS NOT NULL",
        )
        .bind(target.run_id.to_string())
        .bind(target.agent_id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(string_db_error)?;
        let Some(value) = value else {
            return Ok(None);
        };
        map_product_binding_document(target.clone(), value).map(Some)
    }

    async fn load_product_binding_by_runtime_thread(
        &self,
        runtime_thread_id: &RuntimeThreadId,
    ) -> Result<Option<AgentRunProductRuntimeBinding>, String> {
        PostgresAgentRunProductRuntimeBindingRepository::load_product_binding_by_runtime_thread(
            self,
            runtime_thread_id,
        )
        .await
    }
}

#[async_trait]
impl AgentRunProductRuntimeBindingStore for PostgresAgentRunProductRuntimeBindingRepository {
    async fn commit_product_binding(
        &self,
        binding: &AgentRunProductRuntimeBinding,
    ) -> Result<AgentRunCommittedProductRuntimeBinding, String> {
        PostgresAgentRunProductRuntimeBindingRepository::commit_product_binding(self, binding).await
    }

    async fn replace_product_binding(
        &self,
        expected_previous_binding_digest: &str,
        binding: &AgentRunProductRuntimeBinding,
    ) -> Result<AgentRunCommittedProductRuntimeBinding, String> {
        PostgresAgentRunProductRuntimeBindingRepository::replace_product_binding(
            self,
            expected_previous_binding_digest,
            binding,
        )
        .await
    }
}

fn map_product_binding_document(
    target: AgentRunTarget,
    value: Value,
) -> Result<AgentRunProductRuntimeBinding, String> {
    let binding = serde_json::from_value::<AgentRunProductRuntimeBinding>(value)
        .map_err(|error| format!("lifecycle_agents.runtime_binding is invalid: {error}"))?;
    if binding.target != target {
        return Err("LifecycleAgent runtime binding belongs to a different owner".to_string());
    }
    binding.calculated_digest()?;
    Ok(binding)
}

#[derive(Clone)]
pub struct PostgresAgentRunTerminalProjectionStore {
    pool: PgPool,
}

impl PostgresAgentRunTerminalProjectionStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl AgentRunTerminalProjectionRepository for PostgresAgentRunTerminalProjectionStore {
    async fn load_head(
        &self,
        target: &AgentRunTarget,
    ) -> Result<AgentRunTerminalProjectionHead, AgentRunTerminalProjectionStoreError> {
        let row = sqlx::query(
            "SELECT revision,latest_change_sequence
             FROM agent_run_terminal_projection_head
             WHERE target_run_id=$1 AND target_agent_id=$2",
        )
        .bind(target.run_id.to_string())
        .bind(target.agent_id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(terminal_db_error)?;
        terminal_head(target, row)
    }

    async fn load_snapshot(
        &self,
        target: &AgentRunTarget,
    ) -> Result<AgentRunTerminalSnapshot, AgentRunTerminalProjectionStoreError> {
        let head = self.load_head(target).await?;
        let values = sqlx::query_scalar::<_, Value>(
            "SELECT projection FROM agent_run_terminal_projection
             WHERE target_run_id=$1 AND target_agent_id=$2 ORDER BY terminal_id",
        )
        .bind(target.run_id.to_string())
        .bind(target.agent_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(terminal_db_error)?;
        Ok(AgentRunTerminalSnapshot {
            target: target.clone(),
            revision: head.revision,
            latest_change_sequence: head.latest_change_sequence,
            captured_at_ms: now_ms(),
            terminals: decode_all(values).map_err(terminal_serde_error)?,
        })
    }

    async fn load_changes(
        &self,
        target: &AgentRunTarget,
        after: Option<AgentRunTerminalChangeSequence>,
        limit: usize,
    ) -> Result<
        agentdash_application_agentrun::agent_run::AgentRunTerminalChangePage,
        AgentRunTerminalProjectionStoreError,
    > {
        let head = self.load_head(target).await?;
        let bounds = sqlx::query(
            "SELECT MIN(change_sequence) AS earliest,MAX(change_sequence) AS latest
             FROM agent_run_terminal_projection_change
             WHERE target_run_id=$1 AND target_agent_id=$2",
        )
        .bind(target.run_id.to_string())
        .bind(target.agent_id.to_string())
        .fetch_one(&self.pool)
        .await
        .map_err(terminal_db_error)?;
        let earliest = bounds
            .try_get::<Option<i64>, _>("earliest")
            .map_err(terminal_db_error)?;
        let latest = bounds
            .try_get::<Option<i64>, _>("latest")
            .map_err(terminal_db_error)?;
        if let (Some(earliest), Some(latest), Some(after)) = (earliest, latest, after)
            && after.0.saturating_add(1) < terminal_u64(earliest)?
        {
            return Ok(
                agentdash_application_agentrun::agent_run::AgentRunTerminalChangePage {
                    target: target.clone(),
                    changes: Vec::new(),
                    next: AgentRunTerminalChangeSequence(terminal_u64(latest)?),
                    gap: Some(AgentRunTerminalChangeGap {
                        requested_after: Some(after),
                        earliest_available: AgentRunTerminalChangeSequence(terminal_u64(earliest)?),
                        latest_available: AgentRunTerminalChangeSequence(terminal_u64(latest)?),
                        snapshot_revision: head.revision,
                    }),
                },
            );
        }
        let after_value = after.map_or(0, |sequence| sequence.0);
        let values = sqlx::query_scalar::<_, Value>(
            "SELECT change FROM agent_run_terminal_projection_change
             WHERE target_run_id=$1 AND target_agent_id=$2 AND change_sequence>$3
             ORDER BY change_sequence LIMIT $4",
        )
        .bind(target.run_id.to_string())
        .bind(target.agent_id.to_string())
        .bind(terminal_i64(after_value)?)
        .bind(i64::try_from(limit.max(1)).unwrap_or(i64::MAX))
        .fetch_all(&self.pool)
        .await
        .map_err(terminal_db_error)?;
        let changes: Vec<AgentRunTerminalChange> =
            decode_all(values).map_err(terminal_serde_error)?;
        let next = changes
            .last()
            .map_or(AgentRunTerminalChangeSequence(after_value), |change| {
                change.sequence
            });
        Ok(
            agentdash_application_agentrun::agent_run::AgentRunTerminalChangePage {
                target: target.clone(),
                changes,
                next,
                gap: None,
            },
        )
    }
}

#[async_trait]
impl AgentRunTerminalSourceProjectionLookup for PostgresAgentRunTerminalProjectionStore {
    async fn load_source_projection(
        &self,
        terminal_id: &agentdash_application_agentrun::agent_run::AgentRunTerminalId,
        terminal_owner_epoch_id: &agentdash_application_agentrun::agent_run::AgentRunTerminalOwnerEpochId,
        backend_id: &str,
    ) -> Result<Option<AgentRunTerminalProjection>, AgentRunTerminalProjectionStoreError> {
        let value = sqlx::query_scalar::<_, Value>(
            "SELECT projection FROM agent_run_terminal_projection
             WHERE terminal_id=$1
               AND projection#>>'{owner,terminal_owner_epoch_id}'=$2
               AND projection#>>'{owner,backend_id}'=$3",
        )
        .bind(terminal_id.as_str())
        .bind(terminal_owner_epoch_id.as_str())
        .bind(backend_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(terminal_db_error)?;
        value
            .map(|value| decode(value).map_err(terminal_serde_error))
            .transpose()
    }

    async fn list_backend_source_projections(
        &self,
        backend_id: &str,
    ) -> Result<Vec<AgentRunTerminalProjection>, AgentRunTerminalProjectionStoreError> {
        let values = sqlx::query_scalar::<_, Value>(
            "SELECT projection FROM agent_run_terminal_projection
             WHERE projection#>>'{owner,backend_id}'=$1
             ORDER BY terminal_id",
        )
        .bind(backend_id)
        .fetch_all(&self.pool)
        .await
        .map_err(terminal_db_error)?;
        decode_all(values).map_err(terminal_serde_error)
    }
}

#[async_trait]
impl AgentRunTerminalControlRoutingRepository for PostgresAgentRunTerminalProjectionStore {
    async fn resolve_control_route(
        &self,
        target: &AgentRunTarget,
        terminal_id: &agentdash_application_agentrun::agent_run::AgentRunTerminalId,
    ) -> Result<Option<AgentRunTerminalControlRoute>, AgentRunTerminalProjectionStoreError> {
        let value = sqlx::query_scalar::<_, Value>(
            "SELECT projection FROM agent_run_terminal_projection
             WHERE target_run_id=$1 AND target_agent_id=$2 AND terminal_id=$3",
        )
        .bind(target.run_id.to_string())
        .bind(target.agent_id.to_string())
        .bind(terminal_id.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(terminal_db_error)?;
        value
            .map(|value| {
                let projection: AgentRunTerminalProjection =
                    decode(value).map_err(terminal_serde_error)?;
                Ok(AgentRunTerminalControlRoute {
                    terminal_id: projection.terminal_id,
                    owner: projection.owner,
                    availability: projection.availability,
                })
            })
            .transpose()
    }
}

#[async_trait]
impl AgentRunTerminalProjectionUnitOfWork for PostgresAgentRunTerminalProjectionStore {
    async fn commit(
        &self,
        commit: AgentRunTerminalProjectionCommit,
    ) -> Result<(), AgentRunTerminalProjectionStoreError> {
        commit.validate().map_err(|error| {
            AgentRunTerminalProjectionStoreError::Persistence(error.to_string())
        })?;
        let mut tx = self.pool.begin().await.map_err(terminal_db_error)?;
        let target = &commit.change.target;
        let project_id = load_project_id(&mut tx, target)
            .await
            .map_err(terminal_db_error)?;
        ensure_terminal_head(&mut tx, target, &project_id).await?;
        let head = lock_terminal_head(&mut tx, target).await?;
        if head.revision != commit.expected_revision
            || head.latest_change_sequence.0 != commit.expected_revision.0
        {
            return Err(AgentRunTerminalProjectionStoreError::Conflict);
        }
        apply_terminal_delta(&mut tx, &commit, &project_id).await?;
        insert_terminal_change(&mut tx, &commit, &project_id).await?;
        advance_terminal_head(
            &mut tx,
            target,
            commit.expected_revision,
            commit.change.revision,
            commit.change.sequence,
        )
        .await?;
        tx.commit().await.map_err(terminal_db_error)
    }
}

async fn apply_terminal_delta(
    tx: &mut Transaction<'_, Postgres>,
    commit: &AgentRunTerminalProjectionCommit,
    project_id: &str,
) -> Result<(), AgentRunTerminalProjectionStoreError> {
    let target = &commit.change.target;
    match &commit.change.delta {
        AgentRunTerminalProjectionDelta::Registered { terminal } => {
            insert_terminal_projection(tx, terminal, project_id).await
        }
        AgentRunTerminalProjectionDelta::Removed { terminal_id, .. } => {
            let result = sqlx::query(
                "DELETE FROM agent_run_terminal_projection
                 WHERE terminal_id=$1 AND target_run_id=$2 AND target_agent_id=$3",
            )
            .bind(terminal_id.as_str())
            .bind(target.run_id.to_string())
            .bind(target.agent_id.to_string())
            .execute(&mut **tx)
            .await
            .map_err(terminal_db_error)?;
            if result.rows_affected() != 1 {
                return Err(AgentRunTerminalProjectionStoreError::Conflict);
            }
            Ok(())
        }
        delta => {
            let terminal_id = terminal_delta_id(delta);
            let value = sqlx::query_scalar::<_, Value>(
                "SELECT projection FROM agent_run_terminal_projection
                 WHERE terminal_id=$1 AND target_run_id=$2 AND target_agent_id=$3 FOR UPDATE",
            )
            .bind(terminal_id.as_str())
            .bind(target.run_id.to_string())
            .bind(target.agent_id.to_string())
            .fetch_optional(&mut **tx)
            .await
            .map_err(terminal_db_error)?
            .ok_or(AgentRunTerminalProjectionStoreError::Conflict)?;
            let mut projection: AgentRunTerminalProjection =
                decode(value).map_err(terminal_serde_error)?;
            if &projection.owner != terminal_delta_owner(delta) {
                return Err(AgentRunTerminalProjectionStoreError::Conflict);
            }
            mutate_terminal_projection(&mut projection, &commit.change.origin, delta)?;
            update_terminal_projection(tx, &projection).await
        }
    }
}

async fn insert_terminal_projection(
    tx: &mut Transaction<'_, Postgres>,
    projection: &AgentRunTerminalProjection,
    project_id: &str,
) -> Result<(), AgentRunTerminalProjectionStoreError> {
    let binding = &projection.owner.source_binding;
    sqlx::query(
        "INSERT INTO agent_run_terminal_projection(
             terminal_id,target_run_id,target_agent_id,project_id,terminal_owner_epoch_id,
             runtime_thread_id,source_ref,source_committed_revision,
             source_applied_surface_revision,source_activated_revision,backend_id,
             process_state,availability,latest_source_sequence,next_output_sequence,
             max_output_bytes,projection
         ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17)",
    )
    .bind(projection.terminal_id.as_str())
    .bind(projection.owner.target.run_id.to_string())
    .bind(projection.owner.target.agent_id.to_string())
    .bind(project_id)
    .bind(
        transparent_string(&projection.owner.terminal_owner_epoch_id)
            .map_err(terminal_serde_error)?,
    )
    .bind(projection.owner.runtime_thread_id.as_str())
    .bind(binding.source_ref.as_str())
    .bind(terminal_i64(binding.committed_at_revision.0)?)
    .bind(terminal_i64(binding.applied_surface_revision.0)?)
    .bind(
        binding
            .activated_at_revision
            .map(|revision| terminal_i64(revision.0))
            .transpose()?,
    )
    .bind(&projection.owner.backend_id)
    .bind(terminal_state_name(projection.state))
    .bind(terminal_availability_name(projection.availability))
    .bind(terminal_i64(projection.latest_source_sequence.0)?)
    .bind(terminal_i64(projection.output.next_sequence.0)?)
    .bind(terminal_i64(projection.max_output_bytes)?)
    .bind(encode(projection).map_err(terminal_serde_error)?)
    .execute(&mut **tx)
    .await
    .map_err(terminal_conflict_or_persistence)?;
    Ok(())
}

async fn update_terminal_projection(
    tx: &mut Transaction<'_, Postgres>,
    projection: &AgentRunTerminalProjection,
) -> Result<(), AgentRunTerminalProjectionStoreError> {
    let result = sqlx::query(
        "UPDATE agent_run_terminal_projection SET
             process_state=$2,availability=$3,latest_source_sequence=$4,
             next_output_sequence=$5,projection=$6
         WHERE terminal_id=$1",
    )
    .bind(projection.terminal_id.as_str())
    .bind(terminal_state_name(projection.state))
    .bind(terminal_availability_name(projection.availability))
    .bind(terminal_i64(projection.latest_source_sequence.0)?)
    .bind(terminal_i64(projection.output.next_sequence.0)?)
    .bind(encode(projection).map_err(terminal_serde_error)?)
    .execute(&mut **tx)
    .await
    .map_err(terminal_db_error)?;
    if result.rows_affected() != 1 {
        return Err(AgentRunTerminalProjectionStoreError::Conflict);
    }
    Ok(())
}

fn mutate_terminal_projection(
    projection: &mut AgentRunTerminalProjection,
    origin: &AgentRunTerminalChangeOrigin,
    delta: &AgentRunTerminalProjectionDelta,
) -> Result<(), AgentRunTerminalProjectionStoreError> {
    if let AgentRunTerminalChangeOrigin::SourceFact {
        source_sequence, ..
    } = origin
    {
        projection.latest_source_sequence = *source_sequence;
    }
    match delta {
        AgentRunTerminalProjectionDelta::OutputAppended {
            output_sequence,
            data,
            ..
        } => {
            if projection.output.next_sequence != *output_sequence {
                return Err(AgentRunTerminalProjectionStoreError::Conflict);
            }
            let max = usize::try_from(projection.max_output_bytes).unwrap_or(usize::MAX);
            if projection
                .output
                .retained_output
                .len()
                .saturating_add(data.len())
                > max
            {
                return Err(AgentRunTerminalProjectionStoreError::Conflict);
            }
            projection.output.retained_output.push_str(data);
            projection.output.next_sequence =
                agentdash_application_agentrun::agent_run::AgentRunTerminalOutputSequence(
                    output_sequence.0.saturating_add(1),
                );
        }
        AgentRunTerminalProjectionDelta::OutputOmitted {
            output_sequence,
            omitted_bytes,
            retained_output,
            ..
        } => {
            if projection.output.next_sequence != *output_sequence {
                return Err(AgentRunTerminalProjectionStoreError::Conflict);
            }
            let max = usize::try_from(projection.max_output_bytes).unwrap_or(usize::MAX);
            let remaining = max.saturating_sub(projection.output.retained_output.len());
            let retained = utf8_prefix(retained_output, remaining);
            projection.output.retained_output.push_str(retained);
            projection.output.truncated = true;
            projection.output.omitted_bytes = projection
                .output
                .omitted_bytes
                .saturating_add(*omitted_bytes)
                .saturating_add(
                    u64::try_from(retained_output.len().saturating_sub(retained.len()))
                        .unwrap_or(u64::MAX),
                );
            projection.output.next_sequence =
                agentdash_application_agentrun::agent_run::AgentRunTerminalOutputSequence(
                    output_sequence.0.saturating_add(1),
                );
        }
        AgentRunTerminalProjectionDelta::StateChanged {
            state,
            exit_code,
            changed_at_ms,
            ..
        } => {
            projection.state = *state;
            projection.exit_code = *exit_code;
            if matches!(
                state,
                agentdash_application_agentrun::agent_run::AgentRunTerminalLifecycleState::Exited
                    | agentdash_application_agentrun::agent_run::AgentRunTerminalLifecycleState::Killed
                    | agentdash_application_agentrun::agent_run::AgentRunTerminalLifecycleState::Lost
            ) {
                projection.exited_at_ms = Some(*changed_at_ms);
            }
        }
        AgentRunTerminalProjectionDelta::AvailabilityChanged { availability, .. } => {
            projection.availability = *availability;
        }
        AgentRunTerminalProjectionDelta::ControlCorrelated { .. } => {}
        AgentRunTerminalProjectionDelta::Registered { .. }
        | AgentRunTerminalProjectionDelta::Removed { .. } => {
            return Err(AgentRunTerminalProjectionStoreError::Conflict);
        }
    }
    Ok(())
}

async fn insert_terminal_change(
    tx: &mut Transaction<'_, Postgres>,
    commit: &AgentRunTerminalProjectionCommit,
    project_id: &str,
) -> Result<(), AgentRunTerminalProjectionStoreError> {
    let change = &commit.change;
    let terminal_id = terminal_delta_id(&change.delta);
    let owner = terminal_delta_owner(&change.delta);
    let change_id = transparent_string(&change.change_id).map_err(terminal_serde_error)?;
    let owner_epoch =
        transparent_string(&owner.terminal_owner_epoch_id).map_err(terminal_serde_error)?;
    let source_sequence = match &change.origin {
        AgentRunTerminalChangeOrigin::SourceFact {
            source_sequence, ..
        } => Some(terminal_i64(source_sequence.0)?),
        AgentRunTerminalChangeOrigin::ProductFact { .. } => None,
    };
    let output_sequence = match &change.delta {
        AgentRunTerminalProjectionDelta::OutputAppended {
            output_sequence, ..
        }
        | AgentRunTerminalProjectionDelta::OutputOmitted {
            output_sequence, ..
        } => Some(terminal_i64(output_sequence.0)?),
        _ => None,
    };
    sqlx::query(
        "INSERT INTO agent_run_terminal_projection_change(
             target_run_id,target_agent_id,project_id,revision,change_sequence,change_id,
             terminal_id,terminal_owner_epoch_id,source_sequence,output_sequence,
             payload_digest,delta_kind,change
         ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13)",
    )
    .bind(change.target.run_id.to_string())
    .bind(change.target.agent_id.to_string())
    .bind(project_id)
    .bind(terminal_i64(change.revision.0)?)
    .bind(terminal_i64(change.sequence.0)?)
    .bind(&change_id)
    .bind(terminal_id.as_str())
    .bind(&owner_epoch)
    .bind(source_sequence)
    .bind(output_sequence)
    .bind(change.payload_digest.as_str())
    .bind(terminal_delta_kind(&change.delta))
    .bind(encode(change).map_err(terminal_serde_error)?)
    .execute(&mut **tx)
    .await
    .map_err(terminal_conflict_or_persistence)?;
    if let AgentRunTerminalProjectionDelta::ControlCorrelated {
        correlation_id,
        control,
        status,
        ..
    } = &change.delta
    {
        sqlx::query(
            "INSERT INTO agent_run_terminal_control_correlation(
                 correlation_id,terminal_id,terminal_owner_epoch_id,change_id,
                 control_kind,control_status,correlation
             ) VALUES ($1,$2,$3,$4,$5,$6,$7)",
        )
        .bind(transparent_string(correlation_id).map_err(terminal_serde_error)?)
        .bind(terminal_id.as_str())
        .bind(&owner_epoch)
        .bind(&change_id)
        .bind(terminal_control_name(*control))
        .bind(terminal_control_status_name(*status))
        .bind(encode(&change.delta).map_err(terminal_serde_error)?)
        .execute(&mut **tx)
        .await
        .map_err(terminal_conflict_or_persistence)?;
    }
    sqlx::query(
        "INSERT INTO agent_run_terminal_projection_outbox(
             target_run_id,target_agent_id,change_sequence,change_id,entry
         ) VALUES ($1,$2,$3,$4,$5)",
    )
    .bind(commit.outbox.target.run_id.to_string())
    .bind(commit.outbox.target.agent_id.to_string())
    .bind(terminal_i64(commit.outbox.sequence.0)?)
    .bind(change_id)
    .bind(encode(&commit.outbox).map_err(terminal_serde_error)?)
    .execute(&mut **tx)
    .await
    .map_err(terminal_conflict_or_persistence)?;
    Ok(())
}

fn terminal_delta_id(
    delta: &AgentRunTerminalProjectionDelta,
) -> &agentdash_application_agentrun::agent_run::AgentRunTerminalId {
    match delta {
        AgentRunTerminalProjectionDelta::Registered { terminal } => &terminal.terminal_id,
        AgentRunTerminalProjectionDelta::OutputAppended { terminal_id, .. }
        | AgentRunTerminalProjectionDelta::OutputOmitted { terminal_id, .. }
        | AgentRunTerminalProjectionDelta::StateChanged { terminal_id, .. }
        | AgentRunTerminalProjectionDelta::AvailabilityChanged { terminal_id, .. }
        | AgentRunTerminalProjectionDelta::ControlCorrelated { terminal_id, .. }
        | AgentRunTerminalProjectionDelta::Removed { terminal_id, .. } => terminal_id,
    }
}

fn terminal_delta_owner(
    delta: &AgentRunTerminalProjectionDelta,
) -> &agentdash_application_agentrun::agent_run::AgentRunTerminalOwnerFence {
    match delta {
        AgentRunTerminalProjectionDelta::Registered { terminal } => &terminal.owner,
        AgentRunTerminalProjectionDelta::OutputAppended { owner, .. }
        | AgentRunTerminalProjectionDelta::OutputOmitted { owner, .. }
        | AgentRunTerminalProjectionDelta::StateChanged { owner, .. }
        | AgentRunTerminalProjectionDelta::AvailabilityChanged { owner, .. }
        | AgentRunTerminalProjectionDelta::ControlCorrelated { owner, .. }
        | AgentRunTerminalProjectionDelta::Removed { owner, .. } => owner,
    }
}

fn terminal_delta_kind(delta: &AgentRunTerminalProjectionDelta) -> &'static str {
    match delta {
        AgentRunTerminalProjectionDelta::Registered { .. } => "registered",
        AgentRunTerminalProjectionDelta::OutputAppended { .. } => "output_appended",
        AgentRunTerminalProjectionDelta::OutputOmitted { .. } => "output_omitted",
        AgentRunTerminalProjectionDelta::StateChanged { .. } => "state_changed",
        AgentRunTerminalProjectionDelta::AvailabilityChanged { .. } => "availability_changed",
        AgentRunTerminalProjectionDelta::ControlCorrelated { .. } => "control_correlated",
        AgentRunTerminalProjectionDelta::Removed { .. } => "removed",
    }
}

async fn load_project_id(
    tx: &mut Transaction<'_, Postgres>,
    target: &AgentRunTarget,
) -> Result<String, sqlx::Error> {
    sqlx::query_scalar("SELECT project_id FROM lifecycle_agents WHERE id=$1 AND run_id=$2")
        .bind(target.agent_id.to_string())
        .bind(target.run_id.to_string())
        .fetch_one(&mut **tx)
        .await
}

async fn ensure_terminal_head(
    tx: &mut Transaction<'_, Postgres>,
    target: &AgentRunTarget,
    project_id: &str,
) -> Result<(), AgentRunTerminalProjectionStoreError> {
    sqlx::query(
        "INSERT INTO agent_run_terminal_projection_head(
             target_run_id,target_agent_id,project_id,revision,latest_change_sequence
         ) VALUES ($1,$2,$3,0,0) ON CONFLICT (target_run_id,target_agent_id) DO NOTHING",
    )
    .bind(target.run_id.to_string())
    .bind(target.agent_id.to_string())
    .bind(project_id)
    .execute(&mut **tx)
    .await
    .map_err(terminal_db_error)?;
    Ok(())
}

async fn lock_terminal_head(
    tx: &mut Transaction<'_, Postgres>,
    target: &AgentRunTarget,
) -> Result<AgentRunTerminalProjectionHead, AgentRunTerminalProjectionStoreError> {
    let row = sqlx::query(
        "SELECT revision,latest_change_sequence
         FROM agent_run_terminal_projection_head
         WHERE target_run_id=$1 AND target_agent_id=$2 FOR UPDATE",
    )
    .bind(target.run_id.to_string())
    .bind(target.agent_id.to_string())
    .fetch_one(&mut **tx)
    .await
    .map_err(terminal_db_error)?;
    terminal_head(target, Some(row))
}

async fn advance_terminal_head(
    tx: &mut Transaction<'_, Postgres>,
    target: &AgentRunTarget,
    expected: AgentRunTerminalProjectionRevision,
    revision: AgentRunTerminalProjectionRevision,
    sequence: AgentRunTerminalChangeSequence,
) -> Result<(), AgentRunTerminalProjectionStoreError> {
    let result = sqlx::query(
        "UPDATE agent_run_terminal_projection_head
         SET revision=$3,latest_change_sequence=$4
         WHERE target_run_id=$1 AND target_agent_id=$2 AND revision=$5",
    )
    .bind(target.run_id.to_string())
    .bind(target.agent_id.to_string())
    .bind(terminal_i64(revision.0)?)
    .bind(terminal_i64(sequence.0)?)
    .bind(terminal_i64(expected.0)?)
    .execute(&mut **tx)
    .await
    .map_err(terminal_db_error)?;
    if result.rows_affected() != 1 {
        return Err(AgentRunTerminalProjectionStoreError::Conflict);
    }
    Ok(())
}

fn terminal_head(
    target: &AgentRunTarget,
    row: Option<sqlx::postgres::PgRow>,
) -> Result<AgentRunTerminalProjectionHead, AgentRunTerminalProjectionStoreError> {
    let (revision, latest) = match row {
        Some(row) => (
            row.try_get::<i64, _>("revision")
                .map_err(terminal_db_error)?,
            row.try_get::<i64, _>("latest_change_sequence")
                .map_err(terminal_db_error)?,
        ),
        None => (0, 0),
    };
    Ok(AgentRunTerminalProjectionHead {
        target: target.clone(),
        revision: AgentRunTerminalProjectionRevision(terminal_u64(revision)?),
        latest_change_sequence: AgentRunTerminalChangeSequence(terminal_u64(latest)?),
    })
}

fn terminal_state_name(
    state: agentdash_application_agentrun::agent_run::AgentRunTerminalLifecycleState,
) -> &'static str {
    use agentdash_application_agentrun::agent_run::AgentRunTerminalLifecycleState as State;
    match state {
        State::Starting => "starting",
        State::Running => "running",
        State::Exited => "exited",
        State::Killed => "killed",
        State::Lost => "lost",
    }
}

fn terminal_availability_name(state: AgentRunTerminalAvailability) -> &'static str {
    match state {
        AgentRunTerminalAvailability::Online => "online",
        AgentRunTerminalAvailability::Offline => "offline",
        AgentRunTerminalAvailability::Reconciling => "reconciling",
    }
}

fn terminal_control_name(
    control: agentdash_application_agentrun::agent_run::AgentRunTerminalControlKind,
) -> &'static str {
    use agentdash_application_agentrun::agent_run::AgentRunTerminalControlKind as Control;
    match control {
        Control::Input => "input",
        Control::Resize => "resize",
        Control::Terminate => "terminate",
        Control::Read => "read",
        Control::Status => "status",
    }
}

fn terminal_control_status_name(
    status: agentdash_application_agentrun::agent_run::AgentRunTerminalControlStatus,
) -> &'static str {
    use agentdash_application_agentrun::agent_run::AgentRunTerminalControlStatus as Status;
    match status {
        Status::Accepted => "accepted",
        Status::Completed => "completed",
        Status::Failed => "failed",
    }
}

fn encode<T: Serialize>(value: &T) -> Result<Value, serde_json::Error> {
    serde_json::to_value(value)
}

fn decode<T: DeserializeOwned>(value: Value) -> Result<T, serde_json::Error> {
    serde_json::from_value(value)
}

fn decode_all<T: DeserializeOwned>(values: Vec<Value>) -> Result<Vec<T>, serde_json::Error> {
    values.into_iter().map(decode).collect()
}

fn transparent_string<T: Serialize>(value: &T) -> Result<String, serde_json::Error> {
    let value = encode(value)?;
    value
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| serde_json::Error::io(std::io::Error::other("expected transparent string")))
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(u128::from(u64::MAX)) as u64)
        .unwrap_or(0)
}

#[cfg(test)]
#[allow(clippy::items_after_test_module)]
mod product_binding_persistence_tests {
    use super::*;

    #[tokio::test]
    async fn postgres_persists_canonical_binding_across_repository_restart() {
        let (pool, _runtime) = activation_test_pool().await;
        let project_id = Uuid::new_v4();
        let target = AgentRunTarget {
            run_id: Uuid::new_v4(),
            agent_id: Uuid::new_v4(),
        };
        let thread_id = format!("thread-{}", Uuid::new_v4());
        let launch_frame_id = Uuid::new_v4();
        let mut execution_profile =
            agentdash_application_agentrun::agent_run::ProductExecutionProfileRef {
                profile_key: "codex".to_owned(),
                profile_revision: 1,
                profile_digest: String::new(),
                configuration: serde_json::json!({
                    "z_option": true,
                    "a_option": false,
                    "executor": "codex",
                }),
                credential_scope: None,
            };
        execution_profile.refresh_digest();
        let product_binding = AgentRunProductRuntimeBinding {
            target: target.clone(),
            runtime_thread_id: RuntimeThreadId::new(thread_id.clone()).unwrap(),
            agent: agentdash_application_agentrun::agent_run::AgentRunCompleteAgentAssociation {
                service_instance_id: agentdash_agent_service_api::AgentServiceInstanceId::new(
                    "fixture-agent",
                )
                .unwrap(),
                source: agentdash_agent_service_api::AgentSourceCoordinate::new("fixture-source")
                    .unwrap(),
            },
            launch_frame: agentdash_application_agentrun::agent_run::ProductAgentFrameRef {
                frame_id: launch_frame_id,
                agent_id: target.agent_id,
                revision: 1,
            },
            execution_profile_digest: execution_profile.profile_digest.clone(),
            execution_profile,
        };
        let binding_digest = product_runtime_binding_digest(&product_binding).unwrap();

        sqlx::query(
            "INSERT INTO projects(id,name,created_at,updated_at) VALUES ($1,$2,NOW(),NOW())",
        )
        .bind(project_id.to_string())
        .bind("runtime activation test")
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO lifecycle_runs(
                 id,project_id,topology,status,created_at,updated_at,last_activity_at
             ) VALUES ($1,$2,'single','active',NOW(),NOW(),NOW())",
        )
        .bind(target.run_id.to_string())
        .bind(project_id.to_string())
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO lifecycle_agents(
                 id,run_id,project_id,source,status,created_at,updated_at
             ) VALUES ($1,$2,$3,'unknown','idle',NOW(),NOW())",
        )
        .bind(target.agent_id.to_string())
        .bind(target.run_id.to_string())
        .bind(project_id.to_string())
        .execute(&pool)
        .await
        .unwrap();
        let repository = PostgresAgentRunProductRuntimeBindingRepository::new(pool.clone());
        let committed_receipt = repository
            .commit_product_binding(&product_binding)
            .await
            .expect("commit canonical Product binding");
        assert_eq!(committed_receipt.binding_digest, binding_digest);
        let replayed_receipt = repository
            .commit_product_binding(&product_binding)
            .await
            .expect("Product binding replay");
        assert_eq!(replayed_receipt, committed_receipt);
        let restarted = PostgresAgentRunProductRuntimeBindingRepository::new(pool);
        let committed = restarted
            .load_committed_tool_binding(&product_binding.runtime_thread_id)
            .await
            .expect("query after restart")
            .expect("committed binding");
        assert_eq!(committed.binding_digest, binding_digest);
        assert_eq!(
            committed.binding.calculated_digest().unwrap(),
            binding_digest,
            "Product binding must remain canonical"
        );
        assert_eq!(committed.binding, product_binding);
    }

    async fn activation_test_pool() -> (PgPool, Option<crate::postgres_runtime::PostgresRuntime>) {
        if crate::persistence::postgres::test_database_url().is_some() {
            return (
                crate::persistence::postgres::test_pg_pool("Product activation pins")
                    .await
                    .expect("configured PostgreSQL test pool"),
                None,
            );
        }
        let data_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/product-activation-postgres-tests");
        let runtime = crate::postgres_runtime::PostgresRuntime::resolve_embedded_at_data_root(
            "product-activation-tests",
            8,
            data_root,
        )
        .await
        .expect("start isolated embedded PostgreSQL for Product activation");
        let database_name = format!("product_activation_{}", Uuid::new_v4().simple());
        sqlx::query(&format!("CREATE DATABASE {database_name}"))
            .execute(&runtime.pool)
            .await
            .expect("create isolated Product activation database");
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
            .expect("connect isolated Product activation database");
        crate::migration::run_postgres_migrations(&pool)
            .await
            .expect("migrate isolated Product activation database");
        crate::migration::assert_postgres_schema_ready(&pool)
            .await
            .expect("Product activation schema readiness");
        (pool, Some(runtime))
    }
}

fn utf8_prefix(value: &str, max_bytes: usize) -> &str {
    if value.len() <= max_bytes {
        return value;
    }
    let end = value
        .char_indices()
        .map(|(index, character)| index + character.len_utf8())
        .take_while(|end| *end <= max_bytes)
        .last()
        .unwrap_or(0);
    &value[..end]
}

fn terminal_i64(value: u64) -> Result<i64, AgentRunTerminalProjectionStoreError> {
    i64::try_from(value).map_err(|_| {
        AgentRunTerminalProjectionStoreError::Persistence(
            "terminal projection integer exceeds PostgreSQL BIGINT".to_string(),
        )
    })
}

fn terminal_u64(value: i64) -> Result<u64, AgentRunTerminalProjectionStoreError> {
    u64::try_from(value).map_err(|_| {
        AgentRunTerminalProjectionStoreError::Persistence(
            "terminal projection integer is negative".to_string(),
        )
    })
}

fn terminal_serde_error(error: serde_json::Error) -> AgentRunTerminalProjectionStoreError {
    AgentRunTerminalProjectionStoreError::Persistence(error.to_string())
}

fn terminal_db_error(error: sqlx::Error) -> AgentRunTerminalProjectionStoreError {
    AgentRunTerminalProjectionStoreError::Persistence(error.to_string())
}

fn terminal_conflict_or_persistence(error: sqlx::Error) -> AgentRunTerminalProjectionStoreError {
    if is_conflict(&error) {
        AgentRunTerminalProjectionStoreError::Conflict
    } else {
        terminal_db_error(error)
    }
}

fn is_conflict(error: &sqlx::Error) -> bool {
    error
        .as_database_error()
        .and_then(|error| error.code())
        .is_some_and(|code| matches!(code.as_ref(), "23505" | "40001"))
}

fn string_db_error(error: sqlx::Error) -> String {
    error.to_string()
}
