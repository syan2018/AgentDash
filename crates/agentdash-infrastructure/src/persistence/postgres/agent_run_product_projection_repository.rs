use agentdash_agent_runtime_contract::{
    ManagedRuntimeSourceBindingEvidence, RuntimeProjectionRevision, RuntimeSourceRef,
    RuntimeThreadId, SurfaceRevision,
};
use agentdash_application_agentrun::agent_run::{
    AgentRunProductRuntimeBinding, AgentRunProductRuntimeBindingRepository,
    AgentRunTerminalAvailability, AgentRunTerminalChange, AgentRunTerminalChangeGap,
    AgentRunTerminalChangeOrigin, AgentRunTerminalChangeSequence, AgentRunTerminalControlRoute,
    AgentRunTerminalControlRoutingRepository, AgentRunTerminalProjection,
    AgentRunTerminalProjectionCommit, AgentRunTerminalProjectionDelta,
    AgentRunTerminalProjectionHead, AgentRunTerminalProjectionRepository,
    AgentRunTerminalProjectionRevision, AgentRunTerminalProjectionStoreError,
    AgentRunTerminalProjectionUnitOfWork, AgentRunTerminalSnapshot,
};
use agentdash_domain::agent_run_target::AgentRunTarget;
use agentdash_workspace_module::workspace_module::presentation_protocol::{
    WorkspaceModulePresentationAckId, WorkspaceModulePresentationAcknowledgePort,
    WorkspaceModulePresentationAcknowledgeRequest, WorkspaceModulePresentationAcknowledgement,
    WorkspaceModulePresentationChange, WorkspaceModulePresentationChangeGap,
    WorkspaceModulePresentationChangeId, WorkspaceModulePresentationChangePage,
    WorkspaceModulePresentationChangeSequence, WorkspaceModulePresentationCommit,
    WorkspaceModulePresentationHead, WorkspaceModulePresentationIntent,
    WorkspaceModulePresentationIntentStatus, WorkspaceModulePresentationOutboxEntry,
    WorkspaceModulePresentationPendingIntent, WorkspaceModulePresentationRepository,
    WorkspaceModulePresentationRevision, WorkspaceModulePresentationSnapshot,
    WorkspaceModulePresentationStoreError, WorkspaceModulePresentationUnitOfWork,
};
use async_trait::async_trait;
use serde::{Serialize, de::DeserializeOwned};
use serde_json::Value;
use sqlx::{PgPool, Postgres, Row, Transaction};

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
    ) -> Result<(), String> {
        let mut tx = self.pool.begin().await.map_err(string_db_error)?;
        let project_id = load_project_id(&mut tx, &binding.target)
            .await
            .map_err(string_db_error)?;
        let evidence = &binding.source_binding;
        let binding_json = serde_json::json!({
            "target": {
                "run_id": binding.target.run_id,
                "agent_id": binding.target.agent_id,
            },
            "runtime_thread_id": binding.runtime_thread_id,
            "source_binding": evidence,
        });
        let result = sqlx::query(
            "INSERT INTO agent_run_product_runtime_binding(
                 target_run_id, target_agent_id, project_id, runtime_thread_id, source_ref,
                 source_committed_revision, source_applied_surface_revision,
                 source_activated_revision, binding
             ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)
             ON CONFLICT (target_run_id,target_agent_id) DO NOTHING",
        )
        .bind(binding.target.run_id.to_string())
        .bind(binding.target.agent_id.to_string())
        .bind(project_id)
        .bind(binding.runtime_thread_id.as_str())
        .bind(evidence.source_ref.as_str())
        .bind(to_i64(evidence.committed_at_revision.0).map_err(|error| error.to_string())?)
        .bind(to_i64(evidence.applied_surface_revision.0).map_err(|error| error.to_string())?)
        .bind(
            evidence
                .activated_at_revision
                .map(|revision| to_i64(revision.0))
                .transpose()
                .map_err(|error| error.to_string())?,
        )
        .bind(&binding_json)
        .execute(&mut *tx)
        .await
        .map_err(string_db_error)?;
        if result.rows_affected() == 0 {
            let existing: Value = sqlx::query_scalar(
                "SELECT binding FROM agent_run_product_runtime_binding
                 WHERE target_run_id=$1 AND target_agent_id=$2",
            )
            .bind(binding.target.run_id.to_string())
            .bind(binding.target.agent_id.to_string())
            .fetch_one(&mut *tx)
            .await
            .map_err(string_db_error)?;
            if existing != binding_json {
                return Err("AgentRun Product runtime binding conflict".to_string());
            }
        }
        tx.commit().await.map_err(string_db_error)
    }
}

#[async_trait]
impl AgentRunProductRuntimeBindingRepository for PostgresAgentRunProductRuntimeBindingRepository {
    async fn load_product_binding(
        &self,
        target: &AgentRunTarget,
    ) -> Result<Option<AgentRunProductRuntimeBinding>, String> {
        let row = sqlx::query(
            "SELECT runtime_thread_id,source_ref,source_committed_revision,
                    source_applied_surface_revision,source_activated_revision
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
        let runtime_thread_id = RuntimeThreadId::new(
            row.try_get::<String, _>("runtime_thread_id")
                .map_err(string_db_error)?,
        )
        .map_err(|error| error.to_string())?;
        let source_ref = RuntimeSourceRef::new(
            row.try_get::<String, _>("source_ref")
                .map_err(string_db_error)?,
        )
        .map_err(|error| error.to_string())?;
        let committed = row
            .try_get::<i64, _>("source_committed_revision")
            .map_err(string_db_error)?;
        let applied = row
            .try_get::<i64, _>("source_applied_surface_revision")
            .map_err(string_db_error)?;
        let activated = row
            .try_get::<Option<i64>, _>("source_activated_revision")
            .map_err(string_db_error)?;
        Ok(Some(AgentRunProductRuntimeBinding {
            target: target.clone(),
            runtime_thread_id,
            source_binding: ManagedRuntimeSourceBindingEvidence {
                source_ref,
                committed_at_revision: RuntimeProjectionRevision(
                    u64::try_from(committed).map_err(|error| error.to_string())?,
                ),
                applied_surface_revision: SurfaceRevision(
                    u64::try_from(applied).map_err(|error| error.to_string())?,
                ),
                activated_at_revision: activated
                    .map(|revision| {
                        u64::try_from(revision)
                            .map(RuntimeProjectionRevision)
                            .map_err(|error| error.to_string())
                    })
                    .transpose()?,
            },
        }))
    }
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
    let binding = &intent.currentness_fence.source_binding;
    sqlx::query(
        "INSERT INTO workspace_module_presentation_intent(
             intent_id,effect_id,target_run_id,target_agent_id,project_id,status,
             presentation_digest,module_id,view_key,renderer_kind,presentation_uri,
             runtime_thread_id,runtime_operation_id,runtime_turn_id,runtime_item_id,
             source_ref,source_committed_revision,source_applied_surface_revision,
             source_activated_revision,currentness_fence,intent,committed_at_ms
         ) VALUES (
             $1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20,$21,$22
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
    .bind(binding.source_ref.as_str())
    .bind(to_i64(binding.committed_at_revision.0)?)
    .bind(to_i64(binding.applied_surface_revision.0)?)
    .bind(
        binding
            .activated_at_revision
            .map(|revision| to_i64(revision.0))
            .transpose()?,
    )
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
