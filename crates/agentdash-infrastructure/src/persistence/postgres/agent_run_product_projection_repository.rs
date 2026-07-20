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
use agentdash_workspace_module::workspace_module::presentation_protocol::{
    WorkspaceModulePresentationAckId, WorkspaceModulePresentationAcknowledgePort,
    WorkspaceModulePresentationAcknowledgeRequest, WorkspaceModulePresentationAcknowledgement,
    WorkspaceModulePresentationChange, WorkspaceModulePresentationChangeGap,
    WorkspaceModulePresentationChangeId, WorkspaceModulePresentationChangePage,
    WorkspaceModulePresentationChangeSequence, WorkspaceModulePresentationCommit,
    WorkspaceModulePresentationEffectId, WorkspaceModulePresentationHead,
    WorkspaceModulePresentationIntent, WorkspaceModulePresentationIntentStatus,
    WorkspaceModulePresentationOutboxEntry, WorkspaceModulePresentationPendingIntent,
    WorkspaceModulePresentationRepository, WorkspaceModulePresentationRevision,
    WorkspaceModulePresentationSnapshot, WorkspaceModulePresentationStoreError,
    WorkspaceModulePresentationUnitOfWork,
};
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
        let mut tx = self.pool.begin().await.map_err(string_db_error)?;
        let project_id = load_project_id(&mut tx, &binding.target)
            .await
            .map_err(string_db_error)?;
        let result = sqlx::query(
            "INSERT INTO agent_run_product_runtime_binding(
                 target_run_id, target_agent_id, project_id, runtime_thread_id,
                 launch_frame_id, binding_digest, binding
             ) VALUES ($1,$2,$3,$4,$5,$6,$7)
             ON CONFLICT (target_run_id,target_agent_id) DO NOTHING",
        )
        .bind(binding.target.run_id.to_string())
        .bind(binding.target.agent_id.to_string())
        .bind(project_id)
        .bind(binding.runtime_thread_id.as_str())
        .bind(binding.launch_frame.frame_id.to_string())
        .bind(&receipt.binding_digest)
        .bind(&binding_json)
        .execute(&mut *tx)
        .await
        .map_err(string_db_error)?;
        if result.rows_affected() == 0 {
            let row = sqlx::query(
                "SELECT target_run_id,target_agent_id,runtime_thread_id,launch_frame_id,
                        binding_digest,binding
                 FROM agent_run_product_runtime_binding
                 WHERE target_run_id=$1 AND target_agent_id=$2
                 FOR UPDATE",
            )
            .bind(binding.target.run_id.to_string())
            .bind(binding.target.agent_id.to_string())
            .fetch_one(&mut *tx)
            .await
            .map_err(string_db_error)?;
            let existing = map_product_binding_row(binding.target.clone(), row)?;
            if existing.committed_receipt()? != receipt {
                return Err("AgentRun Product runtime binding conflict".to_string());
            }
        }
        tx.commit().await.map_err(string_db_error)?;
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
        let result = sqlx::query(
            "UPDATE agent_run_product_runtime_binding
             SET launch_frame_id=$5,
                 binding_digest=$6,
                 binding=$7
             WHERE target_run_id=$1 AND target_agent_id=$2
               AND runtime_thread_id=$3
               AND (
                   binding_digest=$4
                   OR (binding_digest=$6 AND binding=$7)
               )",
        )
        .bind(binding.target.run_id.to_string())
        .bind(binding.target.agent_id.to_string())
        .bind(binding.runtime_thread_id.as_str())
        .bind(expected_previous_binding_digest)
        .bind(binding.launch_frame.frame_id.to_string())
        .bind(&receipt.binding_digest)
        .bind(&binding_json)
        .execute(&self.pool)
        .await
        .map_err(string_db_error)?;
        if result.rows_affected() != 1 {
            return Err("AgentRun Product binding replacement CAS conflict".to_string());
        }
        Ok(receipt)
    }

    pub async fn load_committed_tool_binding(
        &self,
        runtime_thread_id: &RuntimeThreadId,
    ) -> Result<Option<crate::CommittedRuntimeToolProductBinding>, String> {
        let row = sqlx::query(
            "SELECT target_run_id,target_agent_id,runtime_thread_id,
                    launch_frame_id,binding_digest,binding
             FROM agent_run_product_runtime_binding
             WHERE runtime_thread_id=$1",
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
                &row.try_get::<String, _>("target_run_id")
                    .map_err(string_db_error)?,
            )
            .map_err(|error| error.to_string())?,
            agent_id: Uuid::parse_str(
                &row.try_get::<String, _>("target_agent_id")
                    .map_err(string_db_error)?,
            )
            .map_err(|error| error.to_string())?,
        };
        let binding_digest = row.try_get("binding_digest").map_err(string_db_error)?;
        Ok(Some(crate::CommittedRuntimeToolProductBinding {
            binding: map_product_binding_row(target, row)?,
            binding_digest,
        }))
    }

    pub async fn load_product_binding_by_runtime_thread(
        &self,
        runtime_thread_id: &RuntimeThreadId,
    ) -> Result<Option<AgentRunProductRuntimeBinding>, String> {
        let row = sqlx::query(
            "SELECT target_run_id,target_agent_id,runtime_thread_id,
                    launch_frame_id,binding_digest,binding
             FROM agent_run_product_runtime_binding
             WHERE runtime_thread_id=$1",
        )
        .bind(runtime_thread_id.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(string_db_error)?;
        let Some(row) = row else {
            return Ok(None);
        };
        let run_id = Uuid::parse_str(
            &row.try_get::<String, _>("target_run_id")
                .map_err(string_db_error)?,
        )
        .map_err(|error| error.to_string())?;
        let agent_id = Uuid::parse_str(
            &row.try_get::<String, _>("target_agent_id")
                .map_err(string_db_error)?,
        )
        .map_err(|error| error.to_string())?;
        map_product_binding_row(AgentRunTarget { run_id, agent_id }, row).map(Some)
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
        let row = sqlx::query(
            "SELECT target_run_id,target_agent_id,runtime_thread_id,launch_frame_id,
                    binding_digest,binding
             FROM agent_run_product_runtime_binding
             WHERE target_run_id=$1 AND target_agent_id=$2",
        )
        .bind(target.run_id.to_string())
        .bind(target.agent_id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(string_db_error)?;
        let Some(row) = row else {
            return Ok(None);
        };
        map_product_binding_row(target.clone(), row).map(Some)
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

fn map_product_binding_row(
    target: AgentRunTarget,
    row: sqlx::postgres::PgRow,
) -> Result<AgentRunProductRuntimeBinding, String> {
    let binding = serde_json::from_value::<AgentRunProductRuntimeBinding>(
        row.try_get::<Value, _>("binding")
            .map_err(string_db_error)?,
    )
    .map_err(|error| format!("agent_run_product_runtime_binding.binding is invalid: {error}"))?;
    let runtime_thread_id = row
        .try_get::<String, _>("runtime_thread_id")
        .map_err(string_db_error)?;
    let launch_frame_id = row
        .try_get::<String, _>("launch_frame_id")
        .map_err(string_db_error)?;
    let stored_digest = row
        .try_get::<String, _>("binding_digest")
        .map_err(string_db_error)?;
    let calculated_digest = binding.calculated_digest()?;
    if binding.target != target
        || binding.runtime_thread_id.as_str() != runtime_thread_id
        || binding.launch_frame.frame_id.to_string() != launch_frame_id
        || stored_digest != calculated_digest
    {
        return Err(
            "agent_run_product_runtime_binding canonical document conflicts with its index evidence"
                .to_string(),
        );
    }
    Ok(binding)
}

#[derive(Clone)]
pub struct PostgresWorkspaceModulePresentationStore {
    pool: PgPool,
}

impl PostgresWorkspaceModulePresentationStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl WorkspaceModulePresentationRepository for PostgresWorkspaceModulePresentationStore {
    async fn load_change_by_effect(
        &self,
        effect_id: &WorkspaceModulePresentationEffectId,
    ) -> Result<Option<WorkspaceModulePresentationChange>, WorkspaceModulePresentationStoreError>
    {
        let value = sqlx::query_scalar::<_, Value>(
            "SELECT change FROM workspace_module_presentation_change
             WHERE intent_id=(
                 SELECT intent_id FROM workspace_module_presentation_intent WHERE effect_id=$1
             )
             ORDER BY change_sequence DESC LIMIT 1",
        )
        .bind(effect_id.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(workspace_db_error)?;
        value
            .map(|value| decode(value).map_err(workspace_serde_error))
            .transpose()
    }

    async fn load_head(
        &self,
        target: &AgentRunTarget,
    ) -> Result<WorkspaceModulePresentationHead, WorkspaceModulePresentationStoreError> {
        let row = sqlx::query(
            "SELECT revision,latest_change_sequence
             FROM workspace_module_presentation_head
             WHERE target_run_id=$1 AND target_agent_id=$2",
        )
        .bind(target.run_id.to_string())
        .bind(target.agent_id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(workspace_db_error)?;
        workspace_head(target, row)
    }

    async fn load_snapshot(
        &self,
        target: &AgentRunTarget,
    ) -> Result<WorkspaceModulePresentationSnapshot, WorkspaceModulePresentationStoreError> {
        let head = self.load_head(target).await?;
        let values = sqlx::query_scalar::<_, Value>(
            "SELECT intent FROM workspace_module_presentation_intent
             WHERE target_run_id=$1 AND target_agent_id=$2 AND status='pending'
             ORDER BY committed_at_ms,intent_id",
        )
        .bind(target.run_id.to_string())
        .bind(target.agent_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(workspace_db_error)?;
        let mut pending_intents = Vec::with_capacity(values.len());
        for value in values {
            let intent: WorkspaceModulePresentationIntent =
                decode(value).map_err(workspace_serde_error)?;
            let sequence = sqlx::query_scalar::<_, Option<i64>>(
                "SELECT MIN(change_sequence)
                 FROM workspace_module_presentation_change
                 WHERE target_run_id=$1 AND target_agent_id=$2
                   AND intent_id=$3 AND status='pending'",
            )
            .bind(target.run_id.to_string())
            .bind(target.agent_id.to_string())
            .bind(transparent_string(&intent.intent_id).map_err(workspace_serde_error)?)
            .fetch_one(&self.pool)
            .await
            .map_err(workspace_db_error)?
            .ok_or_else(|| {
                WorkspaceModulePresentationStoreError::Persistence(
                    "pending workspace intent has no pending change".to_string(),
                )
            })?;
            pending_intents.push(WorkspaceModulePresentationPendingIntent {
                change_sequence: WorkspaceModulePresentationChangeSequence(from_i64(sequence)?),
                intent,
            });
        }
        Ok(WorkspaceModulePresentationSnapshot {
            target: target.clone(),
            revision: head.revision,
            latest_change_sequence: head.latest_change_sequence,
            captured_at_ms: now_ms(),
            pending_intents,
        })
    }

    async fn load_changes(
        &self,
        target: &AgentRunTarget,
        after: Option<WorkspaceModulePresentationChangeSequence>,
        limit: usize,
    ) -> Result<WorkspaceModulePresentationChangePage, WorkspaceModulePresentationStoreError> {
        let head = self.load_head(target).await?;
        let bounds = sqlx::query(
            "SELECT MIN(change_sequence) AS earliest,MAX(change_sequence) AS latest
             FROM workspace_module_presentation_change
             WHERE target_run_id=$1 AND target_agent_id=$2",
        )
        .bind(target.run_id.to_string())
        .bind(target.agent_id.to_string())
        .fetch_one(&self.pool)
        .await
        .map_err(workspace_db_error)?;
        let earliest = bounds
            .try_get::<Option<i64>, _>("earliest")
            .map_err(workspace_db_error)?;
        let latest = bounds
            .try_get::<Option<i64>, _>("latest")
            .map_err(workspace_db_error)?;
        if let (Some(earliest), Some(latest), Some(after)) = (earliest, latest, after)
            && after.0.saturating_add(1) < from_i64(earliest)?
        {
            return Ok(WorkspaceModulePresentationChangePage {
                target: target.clone(),
                changes: Vec::new(),
                next: WorkspaceModulePresentationChangeSequence(from_i64(latest)?),
                gap: Some(WorkspaceModulePresentationChangeGap {
                    requested_after: Some(after),
                    earliest_available: WorkspaceModulePresentationChangeSequence(from_i64(
                        earliest,
                    )?),
                    latest_available: WorkspaceModulePresentationChangeSequence(from_i64(latest)?),
                    snapshot_revision: head.revision,
                }),
            });
        }
        let after_value = after.map_or(0, |sequence| sequence.0);
        let values = sqlx::query_scalar::<_, Value>(
            "SELECT change FROM workspace_module_presentation_change
             WHERE target_run_id=$1 AND target_agent_id=$2 AND change_sequence>$3
             ORDER BY change_sequence LIMIT $4",
        )
        .bind(target.run_id.to_string())
        .bind(target.agent_id.to_string())
        .bind(to_i64(after_value)?)
        .bind(i64::try_from(limit.max(1)).unwrap_or(i64::MAX))
        .fetch_all(&self.pool)
        .await
        .map_err(workspace_db_error)?;
        let changes: Vec<WorkspaceModulePresentationChange> =
            decode_all(values).map_err(workspace_serde_error)?;
        let next = changes.last().map_or(
            WorkspaceModulePresentationChangeSequence(after_value),
            |change| change.sequence,
        );
        Ok(WorkspaceModulePresentationChangePage {
            target: target.clone(),
            changes,
            next,
            gap: None,
        })
    }
}

#[async_trait]
impl WorkspaceModulePresentationUnitOfWork for PostgresWorkspaceModulePresentationStore {
    async fn commit(
        &self,
        commit: WorkspaceModulePresentationCommit,
    ) -> Result<(), WorkspaceModulePresentationStoreError> {
        commit.validate().map_err(|error| {
            WorkspaceModulePresentationStoreError::Persistence(error.to_string())
        })?;
        let mut tx = self.pool.begin().await.map_err(workspace_db_error)?;
        commit_workspace(&mut tx, &commit).await?;
        tx.commit().await.map_err(workspace_db_error)
    }
}

#[async_trait]
impl WorkspaceModulePresentationAcknowledgePort for PostgresWorkspaceModulePresentationStore {
    async fn acknowledge(
        &self,
        request: WorkspaceModulePresentationAcknowledgeRequest,
    ) -> Result<WorkspaceModulePresentationChange, WorkspaceModulePresentationStoreError> {
        let mut tx = self.pool.begin().await.map_err(workspace_db_error)?;
        let intent_id = transparent_string(&request.intent_id).map_err(workspace_serde_error)?;
        let row = sqlx::query(
            "SELECT status,intent FROM workspace_module_presentation_intent
             WHERE intent_id=$1 AND target_run_id=$2 AND target_agent_id=$3 FOR UPDATE",
        )
        .bind(&intent_id)
        .bind(request.target.run_id.to_string())
        .bind(request.target.agent_id.to_string())
        .fetch_optional(&mut *tx)
        .await
        .map_err(workspace_db_error)?
        .ok_or(WorkspaceModulePresentationStoreError::Conflict)?;
        let status: String = row.try_get("status").map_err(workspace_db_error)?;
        if status == "fulfilled" {
            let value = sqlx::query_scalar::<_, Value>(
                "SELECT change FROM workspace_module_presentation_change
                 WHERE target_run_id=$1 AND target_agent_id=$2
                   AND intent_id=$3 AND status='fulfilled'
                 ORDER BY change_sequence DESC LIMIT 1",
            )
            .bind(request.target.run_id.to_string())
            .bind(request.target.agent_id.to_string())
            .bind(intent_id)
            .fetch_one(&mut *tx)
            .await
            .map_err(workspace_db_error)?;
            tx.commit().await.map_err(workspace_db_error)?;
            return decode(value).map_err(workspace_serde_error);
        }
        if status != "pending" {
            return Err(WorkspaceModulePresentationStoreError::Conflict);
        }
        let intent: WorkspaceModulePresentationIntent =
            decode(row.try_get("intent").map_err(workspace_db_error)?)
                .map_err(workspace_serde_error)?;
        let pending_sequence = sqlx::query_scalar::<_, Option<i64>>(
            "SELECT MIN(change_sequence)
             FROM workspace_module_presentation_change
             WHERE target_run_id=$1 AND target_agent_id=$2
               AND intent_id=$3 AND status='pending'",
        )
        .bind(request.target.run_id.to_string())
        .bind(request.target.agent_id.to_string())
        .bind(&intent_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(workspace_db_error)?
        .ok_or(WorkspaceModulePresentationStoreError::Conflict)?;
        if request.observed_change_sequence.0 < from_i64(pending_sequence)? {
            return Err(WorkspaceModulePresentationStoreError::Conflict);
        }
        ensure_workspace_head(&mut tx, &request.target).await?;
        let head = lock_workspace_head(&mut tx, &request.target).await?;
        if request.observed_change_sequence > head.latest_change_sequence {
            return Err(WorkspaceModulePresentationStoreError::Conflict);
        }
        let sequence = WorkspaceModulePresentationChangeSequence(head.latest_change_sequence.0 + 1);
        let revision = WorkspaceModulePresentationRevision(head.revision.0 + 1);
        let acknowledgement = WorkspaceModulePresentationAcknowledgement {
            ack_id: WorkspaceModulePresentationAckId::new(format!(
                "workspace-presentation-ack:{intent_id}"
            ))
            .map_err(|error| {
                WorkspaceModulePresentationStoreError::Persistence(error.to_string())
            })?,
            target: request.target.clone(),
            intent_id: request.intent_id.clone(),
            effect_id: intent.effect_id.clone(),
            acknowledged_change_sequence: request.observed_change_sequence,
            fulfilled_at_ms: now_ms(),
        };
        let change = WorkspaceModulePresentationChange {
            change_id: WorkspaceModulePresentationChangeId::new(format!(
                "workspace-presentation-fulfilled:{intent_id}"
            ))
            .map_err(|error| {
                WorkspaceModulePresentationStoreError::Persistence(error.to_string())
            })?,
            target: request.target.clone(),
            sequence,
            revision,
            status: WorkspaceModulePresentationIntentStatus::Fulfilled,
            intent,
            acknowledgement: Some(acknowledgement.clone()),
        };
        let change_json = encode(&change).map_err(workspace_serde_error)?;
        let acknowledgement_json = encode(&acknowledgement).map_err(workspace_serde_error)?;
        let change_id = transparent_string(&change.change_id).map_err(workspace_serde_error)?;
        let ack_id = transparent_string(&acknowledgement.ack_id).map_err(workspace_serde_error)?;
        sqlx::query(
            "UPDATE workspace_module_presentation_intent SET status='fulfilled'
             WHERE intent_id=$1 AND status='pending'",
        )
        .bind(&intent_id)
        .execute(&mut *tx)
        .await
        .map_err(workspace_db_error)?;
        sqlx::query(
            "INSERT INTO workspace_module_presentation_ack(
                 ack_id,intent_id,effect_id,target_run_id,target_agent_id,
                 acknowledged_change_sequence,fulfilled_at_ms,acknowledgement
             ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8)",
        )
        .bind(ack_id)
        .bind(&intent_id)
        .bind(change.intent.effect_id.as_str())
        .bind(request.target.run_id.to_string())
        .bind(request.target.agent_id.to_string())
        .bind(to_i64(request.observed_change_sequence.0)?)
        .bind(to_i64(acknowledgement.fulfilled_at_ms)?)
        .bind(acknowledgement_json)
        .execute(&mut *tx)
        .await
        .map_err(workspace_db_error)?;
        sqlx::query(
            "INSERT INTO workspace_module_presentation_change(
                 target_run_id,target_agent_id,revision,change_sequence,change_id,
                 intent_id,status,change
             ) VALUES ($1,$2,$3,$4,$5,$6,'fulfilled',$7)",
        )
        .bind(request.target.run_id.to_string())
        .bind(request.target.agent_id.to_string())
        .bind(to_i64(revision.0)?)
        .bind(to_i64(sequence.0)?)
        .bind(&change_id)
        .bind(&intent_id)
        .bind(change_json)
        .execute(&mut *tx)
        .await
        .map_err(workspace_db_error)?;
        let outbox = WorkspaceModulePresentationOutboxEntry {
            effect_id: change.intent.effect_id.clone(),
            change_id: change.change_id.clone(),
            target: request.target.clone(),
            sequence,
        };
        sqlx::query(
            "INSERT INTO workspace_module_presentation_outbox(
                 target_run_id,target_agent_id,change_sequence,effect_id,change_id,entry
             ) VALUES ($1,$2,$3,$4,$5,$6)",
        )
        .bind(request.target.run_id.to_string())
        .bind(request.target.agent_id.to_string())
        .bind(to_i64(sequence.0)?)
        .bind(outbox.effect_id.as_str())
        .bind(change_id)
        .bind(encode(&outbox).map_err(workspace_serde_error)?)
        .execute(&mut *tx)
        .await
        .map_err(workspace_db_error)?;
        advance_workspace_head(&mut tx, &request.target, head.revision, revision, sequence).await?;
        tx.commit().await.map_err(workspace_db_error)?;
        Ok(change)
    }
}

async fn commit_workspace(
    tx: &mut Transaction<'_, Postgres>,
    commit: &WorkspaceModulePresentationCommit,
) -> Result<(), WorkspaceModulePresentationStoreError> {
    let target = &commit.change.target;
    let project_id = load_project_id(tx, target)
        .await
        .map_err(workspace_db_error)?;
    ensure_workspace_head_with_project(tx, target, &project_id).await?;
    let head = lock_workspace_head(tx, target).await?;
    if head.revision != commit.expected_revision
        || head.latest_change_sequence.0 != commit.expected_revision.0
    {
        return Err(WorkspaceModulePresentationStoreError::Conflict);
    }
    let intent = &commit.change.intent;
    let intent_id = transparent_string(&intent.intent_id).map_err(workspace_serde_error)?;
    let change_id = transparent_string(&commit.change.change_id).map_err(workspace_serde_error)?;
    let status = workspace_status(commit.change.status);
    sqlx::query(
        "INSERT INTO workspace_module_presentation_intent(
             intent_id,effect_id,target_run_id,target_agent_id,project_id,status,
             presentation_digest,module_id,view_key,renderer_kind,presentation_uri,
             runtime_thread_id,runtime_operation_id,runtime_turn_id,runtime_item_id,
             currentness_fence,intent,committed_at_ms
         ) VALUES (
             $1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18
         )",
    )
    .bind(&intent_id)
    .bind(intent.effect_id.as_str())
    .bind(target.run_id.to_string())
    .bind(target.agent_id.to_string())
    .bind(project_id)
    .bind(status)
    .bind(intent.presentation_digest.as_str())
    .bind(&intent.presentation.module_id)
    .bind(&intent.presentation.view_key)
    .bind(&intent.presentation.renderer_kind)
    .bind(&intent.presentation.presentation_uri)
    .bind(intent.currentness_fence.runtime_thread_id.as_str())
    .bind(
        intent
            .cause
            .runtime_operation_id
            .as_ref()
            .map(|identity| identity.as_str()),
    )
    .bind(intent.cause.runtime_turn_id.as_str())
    .bind(intent.cause.runtime_item_id.as_str())
    .bind(encode(&intent.currentness_fence).map_err(workspace_serde_error)?)
    .bind(encode(intent).map_err(workspace_serde_error)?)
    .bind(to_i64(intent.committed_at_ms)?)
    .execute(&mut **tx)
    .await
    .map_err(workspace_conflict_or_persistence)?;
    sqlx::query(
        "INSERT INTO workspace_module_presentation_change(
             target_run_id,target_agent_id,revision,change_sequence,change_id,
             intent_id,status,change
         ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8)",
    )
    .bind(target.run_id.to_string())
    .bind(target.agent_id.to_string())
    .bind(to_i64(commit.change.revision.0)?)
    .bind(to_i64(commit.change.sequence.0)?)
    .bind(&change_id)
    .bind(intent_id)
    .bind(status)
    .bind(encode(&commit.change).map_err(workspace_serde_error)?)
    .execute(&mut **tx)
    .await
    .map_err(workspace_conflict_or_persistence)?;
    sqlx::query(
        "INSERT INTO workspace_module_presentation_outbox(
             target_run_id,target_agent_id,change_sequence,effect_id,change_id,entry
         ) VALUES ($1,$2,$3,$4,$5,$6)",
    )
    .bind(target.run_id.to_string())
    .bind(target.agent_id.to_string())
    .bind(to_i64(commit.outbox.sequence.0)?)
    .bind(commit.outbox.effect_id.as_str())
    .bind(change_id)
    .bind(encode(&commit.outbox).map_err(workspace_serde_error)?)
    .execute(&mut **tx)
    .await
    .map_err(workspace_conflict_or_persistence)?;
    advance_workspace_head(
        tx,
        target,
        commit.expected_revision,
        commit.change.revision,
        commit.change.sequence,
    )
    .await
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

async fn ensure_workspace_head(
    tx: &mut Transaction<'_, Postgres>,
    target: &AgentRunTarget,
) -> Result<(), WorkspaceModulePresentationStoreError> {
    let project_id = load_project_id(tx, target)
        .await
        .map_err(workspace_db_error)?;
    ensure_workspace_head_with_project(tx, target, &project_id).await
}

async fn ensure_workspace_head_with_project(
    tx: &mut Transaction<'_, Postgres>,
    target: &AgentRunTarget,
    project_id: &str,
) -> Result<(), WorkspaceModulePresentationStoreError> {
    sqlx::query(
        "INSERT INTO workspace_module_presentation_head(
             target_run_id,target_agent_id,project_id,revision,latest_change_sequence
         ) VALUES ($1,$2,$3,0,0) ON CONFLICT (target_run_id,target_agent_id) DO NOTHING",
    )
    .bind(target.run_id.to_string())
    .bind(target.agent_id.to_string())
    .bind(project_id)
    .execute(&mut **tx)
    .await
    .map_err(workspace_db_error)?;
    Ok(())
}

async fn lock_workspace_head(
    tx: &mut Transaction<'_, Postgres>,
    target: &AgentRunTarget,
) -> Result<WorkspaceModulePresentationHead, WorkspaceModulePresentationStoreError> {
    let row = sqlx::query(
        "SELECT revision,latest_change_sequence
         FROM workspace_module_presentation_head
         WHERE target_run_id=$1 AND target_agent_id=$2 FOR UPDATE",
    )
    .bind(target.run_id.to_string())
    .bind(target.agent_id.to_string())
    .fetch_one(&mut **tx)
    .await
    .map_err(workspace_db_error)?;
    workspace_head(target, Some(row))
}

async fn advance_workspace_head(
    tx: &mut Transaction<'_, Postgres>,
    target: &AgentRunTarget,
    expected: WorkspaceModulePresentationRevision,
    revision: WorkspaceModulePresentationRevision,
    sequence: WorkspaceModulePresentationChangeSequence,
) -> Result<(), WorkspaceModulePresentationStoreError> {
    let result = sqlx::query(
        "UPDATE workspace_module_presentation_head
         SET revision=$3,latest_change_sequence=$4
         WHERE target_run_id=$1 AND target_agent_id=$2 AND revision=$5",
    )
    .bind(target.run_id.to_string())
    .bind(target.agent_id.to_string())
    .bind(to_i64(revision.0)?)
    .bind(to_i64(sequence.0)?)
    .bind(to_i64(expected.0)?)
    .execute(&mut **tx)
    .await
    .map_err(workspace_db_error)?;
    if result.rows_affected() != 1 {
        return Err(WorkspaceModulePresentationStoreError::Conflict);
    }
    Ok(())
}

fn workspace_head(
    target: &AgentRunTarget,
    row: Option<sqlx::postgres::PgRow>,
) -> Result<WorkspaceModulePresentationHead, WorkspaceModulePresentationStoreError> {
    let (revision, latest) = match row {
        Some(row) => (
            row.try_get::<i64, _>("revision")
                .map_err(workspace_db_error)?,
            row.try_get::<i64, _>("latest_change_sequence")
                .map_err(workspace_db_error)?,
        ),
        None => (0, 0),
    };
    Ok(WorkspaceModulePresentationHead {
        target: target.clone(),
        revision: WorkspaceModulePresentationRevision(from_i64(revision)?),
        latest_change_sequence: WorkspaceModulePresentationChangeSequence(from_i64(latest)?),
    })
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

fn workspace_status(status: WorkspaceModulePresentationIntentStatus) -> &'static str {
    match status {
        WorkspaceModulePresentationIntentStatus::Pending => "pending",
        WorkspaceModulePresentationIntentStatus::Fulfilled => "fulfilled",
    }
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
        sqlx::query(
            "INSERT INTO agent_frames(
                 id,agent_id,revision,surface,created_by_kind,created_at
             ) VALUES ($1,$2,1,'{}'::JSONB,'test',NOW())",
        )
        .bind(launch_frame_id.to_string())
        .bind(target.agent_id.to_string())
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

fn to_i64(value: u64) -> Result<i64, WorkspaceModulePresentationStoreError> {
    i64::try_from(value).map_err(|_| {
        WorkspaceModulePresentationStoreError::Persistence(
            "workspace projection integer exceeds PostgreSQL BIGINT".to_string(),
        )
    })
}

fn from_i64(value: i64) -> Result<u64, WorkspaceModulePresentationStoreError> {
    u64::try_from(value).map_err(|_| {
        WorkspaceModulePresentationStoreError::Persistence(
            "workspace projection integer is negative".to_string(),
        )
    })
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

fn workspace_serde_error(error: serde_json::Error) -> WorkspaceModulePresentationStoreError {
    WorkspaceModulePresentationStoreError::Persistence(error.to_string())
}

fn terminal_serde_error(error: serde_json::Error) -> AgentRunTerminalProjectionStoreError {
    AgentRunTerminalProjectionStoreError::Persistence(error.to_string())
}

fn workspace_db_error(error: sqlx::Error) -> WorkspaceModulePresentationStoreError {
    WorkspaceModulePresentationStoreError::Persistence(error.to_string())
}

fn workspace_conflict_or_persistence(error: sqlx::Error) -> WorkspaceModulePresentationStoreError {
    if is_conflict(&error) {
        WorkspaceModulePresentationStoreError::Conflict
    } else {
        workspace_db_error(error)
    }
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
