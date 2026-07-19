use agentdash_agent_runtime_contract::ManagedRuntimeCommandEnvelope;
use agentdash_application_agentrun::agent_run::{
    AgentRunAppliedResourceSurface, AgentRunAppliedResourceSurfaceCommitOutcome,
    AgentRunAppliedResourceSurfaceProvenance, AgentRunAppliedResourceSurfaceQueryError,
    AgentRunAppliedResourceSurfaceQueryPort, AgentRunAppliedResourceSurfaceRepository,
    AgentRunAppliedResourceSurfaceSnapshot, PrepareAgentRunAppliedResourceSurface,
    ProductMailboxChange, ProductMailboxChangeGap, ProductMailboxChangeOrigin,
    ProductMailboxChangePage, ProductMailboxCommand, ProductMailboxCommandKind,
    ProductMailboxCommandOutcome, ProductMailboxCommandReceipt, ProductMailboxCommandRepository,
    ProductMailboxCommandRepositoryError, ProductMailboxCommitEvidence,
    ProductMailboxCommittedAtMs, ProductMailboxCursor, ProductMailboxDurableCommand,
    ProductMailboxInvalidMoveReason, ProductMailboxReadError, ProductMailboxReadRepository,
    ProductMailboxSnapshot, ProductMailboxSnapshotDigest, ProductRuntimeCommandClaimError,
    ProductRuntimeCommandClaimRepository, ProductRuntimeCommandClaimRequest,
    canonical_product_mailbox_digest,
};
use agentdash_domain::{
    agent_run_mailbox::{AgentRunMailboxMessage, AgentRunMailboxState},
    agent_run_target::AgentRunTarget,
};
use async_trait::async_trait;
use serde_json::Value;
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

use super::agent_run_mailbox_repository::{AgentRunMailboxMessageRow, AgentRunMailboxStateRow};

const SURFACE_COLUMNS: &str = "run_id,agent_id,snapshot_revision,project_id,workspace_id,\
vfs_mounts,default_mount_id,vfs_grants,agent_surface_revision,agent_surface_digest,vfs_digest,\
task_grants,task_surface_revision,task_surface_digest,task_source_kind,task_source_id,\
task_source_revision,task_projection_revision,task_captured_at_ms,product_binding_digest,\
source_kind,source_id,source_revision,projection_revision,captured_at_ms";

#[derive(Clone)]
pub struct PostgresAgentRunAppliedResourceSurfaceRepository {
    pool: PgPool,
}

impl PostgresAgentRunAppliedResourceSurfaceRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn initialize(&self) -> Result<(), AgentRunAppliedResourceSurfaceWriteError> {
        crate::migration::assert_postgres_tables_ready(
            &self.pool,
            &[
                "agent_run_applied_resource_surface_snapshot",
                "agent_run_applied_resource_surface_current",
            ],
        )
        .await
        .map_err(
            |error| AgentRunAppliedResourceSurfaceWriteError::Repository {
                message: error.to_string(),
            },
        )
    }

    async fn select_current(
        &self,
        target: &AgentRunTarget,
    ) -> Result<Option<AgentRunAppliedResourceSurfaceSnapshot>, String> {
        let row = sqlx::query_as::<_, AppliedResourceSurfaceRow>(&format!(
            "SELECT {} FROM agent_run_applied_resource_surface_current current_surface \
             JOIN agent_run_applied_resource_surface_snapshot snapshot \
               USING (run_id,agent_id,snapshot_revision) \
             WHERE run_id=$1 AND agent_id=$2",
            prefixed_surface_columns("snapshot")
        ))
        .bind(target.run_id)
        .bind(target.agent_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(storage_message)?;
        row.map(TryInto::try_into).transpose()
    }
}

use agentdash_application_agentrun::agent_run::AgentRunAppliedResourceSurfaceWriteError;

#[async_trait]
impl AgentRunAppliedResourceSurfaceRepository for PostgresAgentRunAppliedResourceSurfaceRepository {
    async fn load_current(
        &self,
        target: &AgentRunTarget,
    ) -> Result<
        Option<AgentRunAppliedResourceSurfaceSnapshot>,
        AgentRunAppliedResourceSurfaceWriteError,
    > {
        let snapshot = self
            .select_current(target)
            .await
            .map_err(|message| AgentRunAppliedResourceSurfaceWriteError::Repository { message })?;
        if let Some(snapshot) = &snapshot {
            snapshot.validate_for(target).map_err(|error| {
                AgentRunAppliedResourceSurfaceWriteError::CorruptEvidence {
                    message: error.to_string(),
                }
            })?;
        }
        Ok(snapshot)
    }

    async fn commit(
        &self,
        prepared: PrepareAgentRunAppliedResourceSurface,
    ) -> Result<AgentRunAppliedResourceSurfaceCommitOutcome, AgentRunAppliedResourceSurfaceWriteError>
    {
        prepared.validate()?;
        let target = prepared.next.surface.target.clone();
        let mut tx = self.pool.begin().await.map_err(surface_repository_error)?;
        lock_product_target(&mut tx, &target)
            .await
            .map_err(surface_repository_error)?;

        let next = &prepared.next;
        let surface = &next.surface;
        sqlx::query(
            "INSERT INTO agent_run_applied_resource_surface_snapshot (\
             run_id,agent_id,snapshot_revision,project_id,workspace_id,vfs_mounts,\
             default_mount_id,vfs_grants,agent_surface_revision,agent_surface_digest,vfs_digest,\
             task_grants,task_surface_revision,task_surface_digest,task_source_kind,task_source_id,\
             task_source_revision,task_projection_revision,task_captured_at_ms,\
             product_binding_digest,source_kind,source_id,source_revision,projection_revision,\
             captured_at_ms) \
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,\
                     $20,$21,$22,$23,$24,$25) \
             ON CONFLICT (run_id,agent_id,snapshot_revision) DO NOTHING",
        )
        .bind(target.run_id)
        .bind(target.agent_id)
        .bind(to_i64(next.snapshot_revision, "snapshot_revision")?)
        .bind(surface.project_id)
        .bind(surface.workspace_id)
        .bind(to_json(&surface.vfs_mounts, "vfs_mounts")?)
        .bind(&surface.default_mount_id)
        .bind(to_json(&surface.vfs_grants, "vfs_grants")?)
        .bind(to_i64(
            surface.agent_surface_revision,
            "agent_surface_revision",
        )?)
        .bind(&surface.agent_surface_digest)
        .bind(&surface.vfs_digest)
        .bind(to_json(&surface.task_grants, "task_grants")?)
        .bind(to_i64(
            surface.task_surface_revision,
            "task_surface_revision",
        )?)
        .bind(&surface.task_surface_digest)
        .bind(&surface.task_provenance.source_kind)
        .bind(&surface.task_provenance.source_id)
        .bind(to_i64(
            surface.task_provenance.source_revision,
            "task_source_revision",
        )?)
        .bind(to_i64(
            surface.task_provenance.projection_revision,
            "task_projection_revision",
        )?)
        .bind(to_i64(
            surface.task_provenance.captured_at_ms,
            "task_captured_at_ms",
        )?)
        .bind(&surface.product_binding_digest)
        .bind(&surface.provenance.source_kind)
        .bind(&surface.provenance.source_id)
        .bind(to_i64(
            surface.provenance.source_revision,
            "source_revision",
        )?)
        .bind(to_i64(
            surface.provenance.projection_revision,
            "projection_revision",
        )?)
        .bind(to_i64(surface.provenance.captured_at_ms, "captured_at_ms")?)
        .execute(&mut *tx)
        .await
        .map_err(surface_repository_error)?;

        let stored: AgentRunAppliedResourceSurfaceSnapshot = sqlx::query_as::<
            _,
            AppliedResourceSurfaceRow,
        >(&format!(
            "SELECT {SURFACE_COLUMNS} FROM agent_run_applied_resource_surface_snapshot \
             WHERE run_id=$1 AND agent_id=$2 AND snapshot_revision=$3"
        ))
        .bind(target.run_id)
        .bind(target.agent_id)
        .bind(to_i64(next.snapshot_revision, "snapshot_revision")?)
        .fetch_one(&mut *tx)
        .await
        .map_err(surface_repository_error)?
        .try_into()
        .map_err(|message| AgentRunAppliedResourceSurfaceWriteError::CorruptEvidence { message })?;
        if stored != prepared.next {
            return Err(AgentRunAppliedResourceSurfaceWriteError::Conflict {
                message: "immutable snapshot row differs from the complete prepared evidence"
                    .to_owned(),
            });
        }

        let current: Option<i64> = sqlx::query_scalar(
            "SELECT snapshot_revision \
             FROM agent_run_applied_resource_surface_current \
             WHERE run_id=$1 AND agent_id=$2 FOR UPDATE",
        )
        .bind(target.run_id)
        .bind(target.agent_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(surface_repository_error)?;
        if current == Some(to_i64(next.snapshot_revision, "snapshot_revision")?) {
            tx.commit().await.map_err(surface_repository_error)?;
            return Ok(AgentRunAppliedResourceSurfaceCommitOutcome::AlreadyCurrent);
        }

        match (current, prepared.expected_current_snapshot_revision) {
            (None, None) => {
                sqlx::query(
                    "INSERT INTO agent_run_applied_resource_surface_current \
                     (run_id,agent_id,snapshot_revision) VALUES ($1,$2,$3)",
                )
                .bind(target.run_id)
                .bind(target.agent_id)
                .bind(to_i64(next.snapshot_revision, "snapshot_revision")?)
                .execute(&mut *tx)
                .await
                .map_err(surface_repository_error)?;
            }
            (None, Some(_)) => return Err(AgentRunAppliedResourceSurfaceWriteError::Missing),
            (Some(actual), None) => {
                return Err(AgentRunAppliedResourceSurfaceWriteError::Conflict {
                    message: format!(
                        "current snapshot revision is {}, but the writer expected no current row",
                        actual
                    ),
                });
            }
            (Some(actual), Some(expected)) => {
                let expected = to_i64(expected, "expected_current_snapshot_revision")?;
                if actual != expected {
                    return Err(AgentRunAppliedResourceSurfaceWriteError::Conflict {
                        message: format!(
                            "current snapshot revision is {actual}, expected {expected}"
                        ),
                    });
                }
                let updated = sqlx::query(
                    "UPDATE agent_run_applied_resource_surface_current \
                     SET snapshot_revision=$3 \
                     WHERE run_id=$1 AND agent_id=$2 AND snapshot_revision=$4",
                )
                .bind(target.run_id)
                .bind(target.agent_id)
                .bind(to_i64(next.snapshot_revision, "snapshot_revision")?)
                .bind(expected)
                .execute(&mut *tx)
                .await
                .map_err(surface_repository_error)?;
                if updated.rows_affected() != 1 {
                    return Err(AgentRunAppliedResourceSurfaceWriteError::Conflict {
                        message: "current snapshot compare-and-swap lost its revision fence"
                            .to_owned(),
                    });
                }
            }
        }
        tx.commit().await.map_err(surface_repository_error)?;
        Ok(AgentRunAppliedResourceSurfaceCommitOutcome::Committed)
    }
}

#[async_trait]
impl AgentRunAppliedResourceSurfaceQueryPort for PostgresAgentRunAppliedResourceSurfaceRepository {
    async fn applied_resource_surface(
        &self,
        target: &AgentRunTarget,
        expected_snapshot_revision: Option<u64>,
    ) -> Result<AgentRunAppliedResourceSurfaceSnapshot, AgentRunAppliedResourceSurfaceQueryError>
    {
        let snapshot = self
            .select_current(target)
            .await
            .map_err(|message| AgentRunAppliedResourceSurfaceQueryError::Repository { message })?
            .ok_or(AgentRunAppliedResourceSurfaceQueryError::SurfaceNotApplied)?;
        snapshot.validate_for(target)?;
        if let Some(expected_revision) = expected_snapshot_revision
            && snapshot.snapshot_revision != expected_revision
        {
            return Err(AgentRunAppliedResourceSurfaceQueryError::ProjectionStale {
                expected_revision,
                actual_revision: snapshot.snapshot_revision,
            });
        }
        Ok(snapshot)
    }
}

#[derive(sqlx::FromRow)]
struct AppliedResourceSurfaceRow {
    run_id: Uuid,
    agent_id: Uuid,
    snapshot_revision: i64,
    project_id: Uuid,
    workspace_id: Option<Uuid>,
    vfs_mounts: Value,
    default_mount_id: Option<String>,
    vfs_grants: Value,
    agent_surface_revision: i64,
    agent_surface_digest: String,
    vfs_digest: String,
    task_grants: Value,
    task_surface_revision: i64,
    task_surface_digest: String,
    task_source_kind: String,
    task_source_id: String,
    task_source_revision: i64,
    task_projection_revision: i64,
    task_captured_at_ms: i64,
    product_binding_digest: String,
    source_kind: String,
    source_id: String,
    source_revision: i64,
    projection_revision: i64,
    captured_at_ms: i64,
}

impl TryFrom<AppliedResourceSurfaceRow> for AgentRunAppliedResourceSurfaceSnapshot {
    type Error = String;

    fn try_from(row: AppliedResourceSurfaceRow) -> Result<Self, Self::Error> {
        Ok(Self {
            snapshot_revision: from_i64(row.snapshot_revision, "snapshot_revision")?,
            surface: AgentRunAppliedResourceSurface {
                target: AgentRunTarget {
                    run_id: row.run_id,
                    agent_id: row.agent_id,
                },
                project_id: row.project_id,
                workspace_id: row.workspace_id,
                vfs_mounts: from_json(row.vfs_mounts, "vfs_mounts")?,
                default_mount_id: row.default_mount_id,
                vfs_grants: from_json(row.vfs_grants, "vfs_grants")?,
                agent_surface_revision: from_i64(
                    row.agent_surface_revision,
                    "agent_surface_revision",
                )?,
                agent_surface_digest: row.agent_surface_digest,
                vfs_digest: row.vfs_digest,
                task_grants: from_json(row.task_grants, "task_grants")?,
                task_surface_revision: from_i64(
                    row.task_surface_revision,
                    "task_surface_revision",
                )?,
                task_surface_digest: row.task_surface_digest,
                task_provenance: AgentRunAppliedResourceSurfaceProvenance {
                    source_kind: row.task_source_kind,
                    source_id: row.task_source_id,
                    source_revision: from_i64(row.task_source_revision, "task_source_revision")?,
                    projection_revision: from_i64(
                        row.task_projection_revision,
                        "task_projection_revision",
                    )?,
                    captured_at_ms: from_i64(row.task_captured_at_ms, "task_captured_at_ms")?,
                },
                product_binding_digest: row.product_binding_digest,
                provenance: AgentRunAppliedResourceSurfaceProvenance {
                    source_kind: row.source_kind,
                    source_id: row.source_id,
                    source_revision: from_i64(row.source_revision, "source_revision")?,
                    projection_revision: from_i64(row.projection_revision, "projection_revision")?,
                    captured_at_ms: from_i64(row.captured_at_ms, "captured_at_ms")?,
                },
            },
        })
    }
}

#[derive(Clone)]
pub struct PostgresProductRuntimeCommandClaimRepository {
    pool: PgPool,
}

impl PostgresProductRuntimeCommandClaimRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn initialize(&self) -> Result<(), ProductRuntimeCommandClaimError> {
        crate::migration::assert_postgres_tables_ready(
            &self.pool,
            &["agent_run_product_runtime_command_claim"],
        )
        .await
        .map_err(|error| ProductRuntimeCommandClaimError::Storage {
            message: error.to_string(),
        })
    }

    async fn load_row(
        &self,
        target: &AgentRunTarget,
        client_command_id: &str,
    ) -> Result<Option<RuntimeCommandClaimRow>, ProductRuntimeCommandClaimError> {
        sqlx::query_as(
            "SELECT request_digest,envelope \
             FROM agent_run_product_runtime_command_claim \
             WHERE target_run_id=$1 AND target_agent_id=$2 AND client_command_id=$3",
        )
        .bind(target.run_id)
        .bind(target.agent_id)
        .bind(client_command_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(product_claim_storage)
    }
}

#[async_trait]
impl ProductRuntimeCommandClaimRepository for PostgresProductRuntimeCommandClaimRepository {
    async fn load(
        &self,
        target: &AgentRunTarget,
        client_command_id: &str,
        request_digest: &str,
    ) -> Result<Option<ManagedRuntimeCommandEnvelope>, ProductRuntimeCommandClaimError> {
        let Some(row) = self.load_row(target, client_command_id).await? else {
            return Ok(None);
        };
        if row.request_digest != request_digest {
            return Err(ProductRuntimeCommandClaimError::RequestDigestConflict {
                target: target.clone(),
                client_command_id: client_command_id.to_owned(),
            });
        }
        serde_json::from_value(row.envelope)
            .map(Some)
            .map_err(|error| ProductRuntimeCommandClaimError::Storage {
                message: format!(
                    "agent_run_product_runtime_command_claim.envelope is invalid: {error}"
                ),
            })
    }

    async fn claim(
        &self,
        request: ProductRuntimeCommandClaimRequest,
    ) -> Result<ManagedRuntimeCommandEnvelope, ProductRuntimeCommandClaimError> {
        let envelope = serde_json::to_value(&request.envelope).map_err(|error| {
            ProductRuntimeCommandClaimError::Storage {
                message: format!("failed to encode Managed Runtime command envelope: {error}"),
            }
        })?;
        sqlx::query(
            "INSERT INTO agent_run_product_runtime_command_claim (\
             target_run_id,target_agent_id,client_command_id,request_digest,envelope,created_at_ms) \
             VALUES ($1,$2,$3,$4,$5,\
               floor(extract(epoch FROM clock_timestamp()) * 1000)::NUMERIC(20,0)) \
             ON CONFLICT (target_run_id,target_agent_id,client_command_id) DO NOTHING",
        )
        .bind(request.target.run_id)
        .bind(request.target.agent_id)
        .bind(&request.client_command_id)
        .bind(&request.request_digest)
        .bind(envelope)
        .execute(&self.pool)
        .await
        .map_err(product_claim_storage)?;
        self.load(
            &request.target,
            &request.client_command_id,
            &request.request_digest,
        )
        .await?
        .ok_or_else(|| ProductRuntimeCommandClaimError::Storage {
            message: "command claim disappeared after insert".to_owned(),
        })
    }
}

#[derive(sqlx::FromRow)]
struct RuntimeCommandClaimRow {
    request_digest: String,
    envelope: Value,
}

#[derive(Clone)]
pub struct PostgresProductMailboxRepository {
    pool: PgPool,
    change_retention: Option<u64>,
}

impl PostgresProductMailboxRepository {
    pub fn new(pool: PgPool) -> Self {
        Self {
            pool,
            change_retention: None,
        }
    }

    pub fn with_change_retention(pool: PgPool, change_retention: u64) -> Result<Self, String> {
        if change_retention == 0 {
            return Err("Product mailbox change retention must be positive".to_owned());
        }
        Ok(Self {
            pool,
            change_retention: Some(change_retention),
        })
    }

    pub async fn initialize(&self) -> Result<(), ProductMailboxReadError> {
        crate::migration::assert_postgres_tables_ready(
            &self.pool,
            &[
                "agent_run_mailbox_messages",
                "agent_run_mailbox_states",
                "agent_run_product_mailbox_head",
                "agent_run_product_mailbox_change",
                "agent_run_product_mailbox_command_receipt",
            ],
        )
        .await
        .map_err(|error| ProductMailboxReadError::Storage {
            message: error.to_string(),
        })
    }

    async fn begin_snapshot(&self) -> Result<Transaction<'_, Postgres>, sqlx::Error> {
        self.pool.begin().await
    }

    async fn reconcile(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        target: &AgentRunTarget,
        origin: ProductMailboxChangeOrigin,
    ) -> Result<
        (
            MailboxHead,
            Vec<AgentRunMailboxMessage>,
            Option<AgentRunMailboxState>,
        ),
        String,
    > {
        let (messages, state) = load_canonical_mailbox(tx, target).await?;
        let digest = canonical_product_mailbox_digest(&messages, state.as_ref());
        let current = load_mailbox_head(tx, target, true).await?;
        if let Some(head) = current.as_ref()
            && head.commit.snapshot_digest == digest
        {
            return Ok((head.clone(), messages, state));
        }

        let revision = current.as_ref().map_or(Ok(1), |head| {
            head.cursor
                .revision
                .checked_add(1)
                .ok_or_else(|| "Product mailbox revision exhausted u64".to_owned())
        })?;
        let sequence = current.as_ref().map_or(Ok(1), |head| {
            head.cursor
                .latest_change_sequence
                .checked_add(1)
                .ok_or_else(|| "Product mailbox change sequence exhausted u64".to_owned())
        })?;
        let committed_at_ms = database_time_ms(tx).await?;
        let commit = ProductMailboxCommitEvidence {
            snapshot_digest: digest,
            committed_at_ms: ProductMailboxCommittedAtMs(committed_at_ms),
        };
        sqlx::query(
            "INSERT INTO agent_run_product_mailbox_head (\
             target_run_id,target_agent_id,revision,latest_change_sequence,snapshot_digest,\
             committed_at_ms) \
             VALUES ($1,$2,$3::NUMERIC(20,0),$4::NUMERIC(20,0),$5,$6::NUMERIC(20,0)) \
             ON CONFLICT (target_run_id,target_agent_id) DO UPDATE SET \
               revision=EXCLUDED.revision,\
               latest_change_sequence=EXCLUDED.latest_change_sequence,\
               snapshot_digest=EXCLUDED.snapshot_digest,\
               committed_at_ms=EXCLUDED.committed_at_ms",
        )
        .bind(target.run_id)
        .bind(target.agent_id)
        .bind(decimal(revision))
        .bind(decimal(sequence))
        .bind(commit.snapshot_digest.as_str())
        .bind(decimal(committed_at_ms))
        .execute(&mut **tx)
        .await
        .map_err(storage_message)?;
        let (origin_kind, client_command_id, command_kind) = origin_columns(&origin);
        sqlx::query(
            "INSERT INTO agent_run_product_mailbox_change (\
             target_run_id,target_agent_id,sequence,change_id,revision,snapshot_digest,\
             committed_at_ms,origin_kind,client_command_id,command_kind) \
             VALUES ($1,$2,$3::NUMERIC(20,0),$4,$5::NUMERIC(20,0),$6,\
                     $7::NUMERIC(20,0),$8,$9,$10)",
        )
        .bind(target.run_id)
        .bind(target.agent_id)
        .bind(decimal(sequence))
        .bind(Uuid::new_v4())
        .bind(decimal(revision))
        .bind(commit.snapshot_digest.as_str())
        .bind(decimal(committed_at_ms))
        .bind(origin_kind)
        .bind(client_command_id)
        .bind(command_kind)
        .execute(&mut **tx)
        .await
        .map_err(storage_message)?;
        if let Some(retention) = self.change_retention
            && sequence > retention
        {
            sqlx::query(
                "DELETE FROM agent_run_product_mailbox_change \
                 WHERE target_run_id=$1 AND target_agent_id=$2 \
                   AND sequence <= $3::NUMERIC(20,0)",
            )
            .bind(target.run_id)
            .bind(target.agent_id)
            .bind(decimal(sequence - retention))
            .execute(&mut **tx)
            .await
            .map_err(storage_message)?;
        }
        Ok((
            MailboxHead {
                cursor: ProductMailboxCursor {
                    revision,
                    latest_change_sequence: sequence,
                },
                commit,
            },
            messages,
            state,
        ))
    }
}

#[async_trait]
impl ProductMailboxReadRepository for PostgresProductMailboxRepository {
    async fn snapshot(
        &self,
        target: &AgentRunTarget,
    ) -> Result<ProductMailboxSnapshot, ProductMailboxReadError> {
        let mut tx = self.begin_snapshot().await.map_err(mailbox_read_storage)?;
        lock_product_target(&mut tx, target)
            .await
            .map_err(mailbox_read_storage)?;
        let (head, messages, state) = self
            .reconcile(
                &mut tx,
                target,
                ProductMailboxChangeOrigin::CanonicalReconcile,
            )
            .await
            .map_err(|message| ProductMailboxReadError::Storage { message })?;
        tx.commit().await.map_err(mailbox_read_storage)?;
        Ok(ProductMailboxSnapshot {
            target: target.clone(),
            cursor: head.cursor,
            commit: head.commit,
            messages,
            state,
        })
    }

    async fn changes(
        &self,
        target: &AgentRunTarget,
        after: u64,
        limit: usize,
    ) -> Result<ProductMailboxChangePage, ProductMailboxReadError> {
        if limit == 0 {
            return Err(ProductMailboxReadError::InvalidContinuity {
                message: "Product mailbox change page limit must be positive".to_owned(),
            });
        }
        let limit =
            i64::try_from(limit).map_err(|_| ProductMailboxReadError::InvalidContinuity {
                message: "Product mailbox change page limit exceeds i64".to_owned(),
            })?;
        let mut tx = self.begin_snapshot().await.map_err(mailbox_read_storage)?;
        lock_product_target(&mut tx, target)
            .await
            .map_err(mailbox_read_storage)?;
        let (head, _, _) = self
            .reconcile(
                &mut tx,
                target,
                ProductMailboxChangeOrigin::CanonicalReconcile,
            )
            .await
            .map_err(|message| ProductMailboxReadError::Storage { message })?;
        if after > head.cursor.latest_change_sequence {
            return Err(ProductMailboxReadError::InvalidContinuity {
                message: format!(
                    "requested cursor {after} is ahead of Product mailbox head {}",
                    head.cursor.latest_change_sequence
                ),
            });
        }
        if after == head.cursor.latest_change_sequence {
            tx.commit().await.map_err(mailbox_read_storage)?;
            return Ok(ProductMailboxChangePage {
                target: target.clone(),
                changes: Vec::new(),
                next: after,
                head: head.cursor,
                head_commit: head.commit,
                gap: None,
            });
        }
        let earliest: Option<String> = sqlx::query_scalar(
            "SELECT MIN(sequence)::TEXT FROM agent_run_product_mailbox_change \
             WHERE target_run_id=$1 AND target_agent_id=$2",
        )
        .bind(target.run_id)
        .bind(target.agent_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(mailbox_read_storage)?;
        let earliest = earliest
            .map(|value| parse_decimal(&value, "mailbox earliest sequence"))
            .transpose()
            .map_err(|message| ProductMailboxReadError::Storage { message })?;
        let first_requested =
            after
                .checked_add(1)
                .ok_or_else(|| ProductMailboxReadError::InvalidContinuity {
                    message: "requested cursor overflow".to_owned(),
                })?;
        if let Some(earliest) = earliest
            && first_requested < earliest
        {
            let detected_at_ms = database_time_ms(&mut tx)
                .await
                .map_err(|message| ProductMailboxReadError::Storage { message })?;
            tx.commit().await.map_err(mailbox_read_storage)?;
            return Ok(ProductMailboxChangePage {
                target: target.clone(),
                changes: Vec::new(),
                next: head.cursor.latest_change_sequence,
                head: head.cursor,
                head_commit: head.commit.clone(),
                gap: Some(ProductMailboxChangeGap {
                    requested_after: after,
                    earliest_available: earliest,
                    latest_available: head.cursor.latest_change_sequence,
                    snapshot_revision: head.cursor.revision,
                    snapshot_digest: head.commit.snapshot_digest,
                    detected_at_ms: ProductMailboxCommittedAtMs(detected_at_ms),
                }),
            });
        }
        let rows = sqlx::query_as::<_, MailboxChangeRow>(
            "SELECT sequence::TEXT AS sequence,change_id,revision::TEXT AS revision,\
                    snapshot_digest,committed_at_ms::TEXT AS committed_at_ms,origin_kind,\
                    client_command_id,command_kind \
             FROM agent_run_product_mailbox_change \
             WHERE target_run_id=$1 AND target_agent_id=$2 \
               AND sequence > $3::NUMERIC(20,0) \
             ORDER BY sequence ASC LIMIT $4",
        )
        .bind(target.run_id)
        .bind(target.agent_id)
        .bind(decimal(after))
        .bind(limit)
        .fetch_all(&mut *tx)
        .await
        .map_err(mailbox_read_storage)?;
        let mut changes = Vec::with_capacity(rows.len());
        let mut expected = first_requested;
        let mut previous_revision = if after == 0 {
            None
        } else {
            let value: Option<String> = sqlx::query_scalar(
                "SELECT revision::TEXT FROM agent_run_product_mailbox_change \
                 WHERE target_run_id=$1 AND target_agent_id=$2 \
                   AND sequence=$3::NUMERIC(20,0)",
            )
            .bind(target.run_id)
            .bind(target.agent_id)
            .bind(decimal(after))
            .fetch_optional(&mut *tx)
            .await
            .map_err(mailbox_read_storage)?;
            value
                .map(|value| parse_decimal(&value, "mailbox previous revision"))
                .transpose()
                .map_err(|message| ProductMailboxReadError::Storage { message })?
        };
        for row in rows {
            let change = row
                .into_change(target)
                .map_err(|message| ProductMailboxReadError::Storage { message })?;
            if change.sequence != expected {
                return Err(ProductMailboxReadError::InvalidContinuity {
                    message: format!(
                        "expected retained sequence {expected}, observed {}",
                        change.sequence
                    ),
                });
            }
            if let Some(previous_revision) = previous_revision
                && change.revision < previous_revision
            {
                return Err(ProductMailboxReadError::RevisionRegression {
                    sequence: change.sequence,
                    previous_revision,
                    observed_revision: change.revision,
                });
            }
            previous_revision = Some(change.revision);
            expected = expected.checked_add(1).ok_or_else(|| {
                ProductMailboxReadError::InvalidContinuity {
                    message: "change sequence overflow".to_owned(),
                }
            })?;
            changes.push(change);
        }
        let next = changes.last().map_or(after, |change| change.sequence);
        tx.commit().await.map_err(mailbox_read_storage)?;
        Ok(ProductMailboxChangePage {
            target: target.clone(),
            changes,
            next,
            head: head.cursor,
            head_commit: head.commit,
            gap: None,
        })
    }

    async fn content(
        &self,
        target: &AgentRunTarget,
        message_id: Uuid,
    ) -> Result<Option<Value>, ProductMailboxReadError> {
        let row = sqlx::query_as::<_, (String, String, Option<Value>)>(
            "SELECT run_id,agent_id,payload_json \
             FROM agent_run_mailbox_messages WHERE id=$1",
        )
        .bind(message_id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(mailbox_read_storage)?;
        let Some((run_id, agent_id, payload)) = row else {
            return Ok(None);
        };
        let observed = parse_target(&run_id, &agent_id)
            .map_err(|message| ProductMailboxReadError::Storage { message })?;
        if observed != *target {
            return Err(ProductMailboxReadError::TargetMismatch {
                expected: target.clone(),
                observed,
            });
        }
        Ok(payload)
    }
}

#[async_trait]
impl ProductMailboxCommandRepository for PostgresProductMailboxRepository {
    async fn execute(
        &self,
        command: ProductMailboxDurableCommand,
    ) -> Result<ProductMailboxCommandOutcome, ProductMailboxCommandRepositoryError> {
        let mut tx = self.pool.begin().await.map_err(mailbox_command_storage)?;
        lock_product_target(&mut tx, &command.target)
            .await
            .map_err(mailbox_command_storage)?;

        if let Some(receipt) = load_command_receipt(&mut tx, &command).await? {
            tx.commit().await.map_err(mailbox_command_storage)?;
            return Ok(ProductMailboxCommandOutcome {
                receipt,
                replayed: true,
            });
        }
        validate_command_targets(&mut tx, &command).await?;
        let encoded_command = serde_json::to_value(&command.command).map_err(|error| {
            ProductMailboxCommandRepositoryError::Storage {
                message: format!("failed to encode Product mailbox command: {error}"),
            }
        })?;
        sqlx::query(
            "INSERT INTO agent_run_product_mailbox_command_receipt (\
             target_run_id,target_agent_id,client_command_id,request_digest,command) \
             VALUES ($1,$2,$3,$4,$5)",
        )
        .bind(command.target.run_id)
        .bind(command.target.agent_id)
        .bind(&command.client_command_id)
        .bind(&command.request_digest)
        .bind(encoded_command)
        .execute(&mut *tx)
        .await
        .map_err(mailbox_command_storage)?;

        apply_mailbox_command(&mut tx, &command).await?;
        let (head, _, _) = self
            .reconcile(
                &mut tx,
                &command.target,
                ProductMailboxChangeOrigin::Command {
                    client_command_id: command.client_command_id.clone(),
                    command_kind: command.command.kind(),
                },
            )
            .await
            .map_err(|message| ProductMailboxCommandRepositoryError::Storage { message })?;
        let updated = sqlx::query(
            "UPDATE agent_run_product_mailbox_command_receipt SET \
               terminal=TRUE,revision=$4::NUMERIC(20,0),\
               latest_change_sequence=$5::NUMERIC(20,0),snapshot_digest=$6,\
               committed_at_ms=$7::NUMERIC(20,0) \
             WHERE target_run_id=$1 AND target_agent_id=$2 AND client_command_id=$3 \
               AND terminal=FALSE",
        )
        .bind(command.target.run_id)
        .bind(command.target.agent_id)
        .bind(&command.client_command_id)
        .bind(decimal(head.cursor.revision))
        .bind(decimal(head.cursor.latest_change_sequence))
        .bind(head.commit.snapshot_digest.as_str())
        .bind(decimal(head.commit.committed_at_ms.0))
        .execute(&mut *tx)
        .await
        .map_err(mailbox_command_storage)?;
        if updated.rows_affected() != 1 {
            return Err(ProductMailboxCommandRepositoryError::NonTerminalReceipt {
                client_command_id: command.client_command_id,
            });
        }
        let receipt = ProductMailboxCommandReceipt {
            target: command.target,
            client_command_id: command.client_command_id,
            revision: head.cursor.revision,
            latest_change_sequence: head.cursor.latest_change_sequence,
            commit: head.commit,
        };
        tx.commit().await.map_err(mailbox_command_storage)?;
        Ok(ProductMailboxCommandOutcome {
            receipt,
            replayed: false,
        })
    }
}

#[derive(Debug, Clone)]
struct MailboxHead {
    cursor: ProductMailboxCursor,
    commit: ProductMailboxCommitEvidence,
}

#[derive(sqlx::FromRow)]
struct MailboxHeadRow {
    revision: String,
    latest_change_sequence: String,
    snapshot_digest: String,
    committed_at_ms: String,
}

impl MailboxHeadRow {
    fn into_head(self) -> Result<MailboxHead, String> {
        Ok(MailboxHead {
            cursor: ProductMailboxCursor {
                revision: parse_decimal(&self.revision, "mailbox revision")?,
                latest_change_sequence: parse_decimal(
                    &self.latest_change_sequence,
                    "mailbox latest change sequence",
                )?,
            },
            commit: ProductMailboxCommitEvidence {
                snapshot_digest: ProductMailboxSnapshotDigest::new(self.snapshot_digest)
                    .map_err(|error| error.to_string())?,
                committed_at_ms: ProductMailboxCommittedAtMs(parse_decimal(
                    &self.committed_at_ms,
                    "mailbox committed_at_ms",
                )?),
            },
        })
    }
}

#[derive(sqlx::FromRow)]
struct MailboxChangeRow {
    sequence: String,
    change_id: Uuid,
    revision: String,
    snapshot_digest: String,
    committed_at_ms: String,
    origin_kind: String,
    client_command_id: Option<String>,
    command_kind: Option<String>,
}

impl MailboxChangeRow {
    fn into_change(self, target: &AgentRunTarget) -> Result<ProductMailboxChange, String> {
        let origin = match (
            self.origin_kind.as_str(),
            self.client_command_id,
            self.command_kind,
        ) {
            ("canonical_reconcile", None, None) => ProductMailboxChangeOrigin::CanonicalReconcile,
            ("command", Some(client_command_id), Some(command_kind)) => {
                ProductMailboxChangeOrigin::Command {
                    client_command_id,
                    command_kind: parse_command_kind(&command_kind)?,
                }
            }
            _ => return Err("Product mailbox change origin columns are inconsistent".to_owned()),
        };
        Ok(ProductMailboxChange {
            change_id: self.change_id,
            target: target.clone(),
            sequence: parse_decimal(&self.sequence, "mailbox change sequence")?,
            revision: parse_decimal(&self.revision, "mailbox change revision")?,
            origin,
            commit: ProductMailboxCommitEvidence {
                snapshot_digest: ProductMailboxSnapshotDigest::new(self.snapshot_digest)
                    .map_err(|error| error.to_string())?,
                committed_at_ms: ProductMailboxCommittedAtMs(parse_decimal(
                    &self.committed_at_ms,
                    "mailbox change committed_at_ms",
                )?),
            },
        })
    }
}

#[derive(sqlx::FromRow)]
struct CommandReceiptRow {
    request_digest: String,
    command: Value,
    terminal: bool,
    revision: Option<String>,
    latest_change_sequence: Option<String>,
    snapshot_digest: Option<String>,
    committed_at_ms: Option<String>,
}

async fn load_command_receipt(
    tx: &mut Transaction<'_, Postgres>,
    request: &ProductMailboxDurableCommand,
) -> Result<Option<ProductMailboxCommandReceipt>, ProductMailboxCommandRepositoryError> {
    let row = sqlx::query_as::<_, CommandReceiptRow>(
        "SELECT request_digest,command,terminal,revision::TEXT AS revision,\
                latest_change_sequence::TEXT AS latest_change_sequence,snapshot_digest,\
                committed_at_ms::TEXT AS committed_at_ms \
         FROM agent_run_product_mailbox_command_receipt \
         WHERE target_run_id=$1 AND target_agent_id=$2 AND client_command_id=$3 FOR UPDATE",
    )
    .bind(request.target.run_id)
    .bind(request.target.agent_id)
    .bind(&request.client_command_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(mailbox_command_storage)?;
    let Some(row) = row else {
        return Ok(None);
    };
    if row.request_digest != request.request_digest {
        return Err(
            ProductMailboxCommandRepositoryError::RequestDigestConflict {
                target: request.target.clone(),
                client_command_id: request.client_command_id.clone(),
            },
        );
    }
    let stored_command: ProductMailboxCommand =
        serde_json::from_value(row.command).map_err(|error| {
            ProductMailboxCommandRepositoryError::Storage {
                message: format!(
                    "agent_run_product_mailbox_command_receipt.command is invalid: {error}"
                ),
            }
        })?;
    if stored_command != request.command {
        return Err(ProductMailboxCommandRepositoryError::Storage {
            message: "stored Product mailbox command differs from its request digest".to_owned(),
        });
    }
    if !row.terminal {
        return Err(ProductMailboxCommandRepositoryError::NonTerminalReceipt {
            client_command_id: request.client_command_id.clone(),
        });
    }
    let revision = required_decimal(row.revision, "receipt revision")?;
    let latest_change_sequence =
        required_decimal(row.latest_change_sequence, "receipt latest change sequence")?;
    let snapshot_digest =
        row.snapshot_digest
            .ok_or_else(|| ProductMailboxCommandRepositoryError::Storage {
                message: "terminal Product mailbox receipt is missing snapshot digest".to_owned(),
            })?;
    let committed_at_ms = required_decimal(row.committed_at_ms, "receipt committed_at_ms")?;
    Ok(Some(ProductMailboxCommandReceipt {
        target: request.target.clone(),
        client_command_id: request.client_command_id.clone(),
        revision,
        latest_change_sequence,
        commit: ProductMailboxCommitEvidence {
            snapshot_digest: ProductMailboxSnapshotDigest::new(snapshot_digest).map_err(
                |error| ProductMailboxCommandRepositoryError::Storage {
                    message: error.to_string(),
                },
            )?,
            committed_at_ms: ProductMailboxCommittedAtMs(committed_at_ms),
        },
    }))
}

async fn validate_command_targets(
    tx: &mut Transaction<'_, Postgres>,
    command: &ProductMailboxDurableCommand,
) -> Result<(), ProductMailboxCommandRepositoryError> {
    let mut ids = Vec::new();
    match command.command {
        ProductMailboxCommand::Promote { message_id }
        | ProductMailboxCommand::Delete { message_id } => ids.push(message_id),
        ProductMailboxCommand::Move {
            message_id,
            after_message_id,
        } => {
            ids.push(message_id);
            if let Some(anchor) = after_message_id {
                ids.push(anchor);
            }
        }
        ProductMailboxCommand::Resume => {
            let row: Option<(String, String)> = sqlx::query_as(
                "SELECT run_id,agent_id FROM agent_run_mailbox_states \
                 WHERE run_id=$1 AND agent_id=$2 FOR UPDATE",
            )
            .bind(command.target.run_id.to_string())
            .bind(command.target.agent_id.to_string())
            .fetch_optional(&mut **tx)
            .await
            .map_err(mailbox_command_storage)?;
            if row.is_none() {
                return Err(ProductMailboxCommandRepositoryError::Storage {
                    message: "canonical Product mailbox state is missing".to_owned(),
                });
            }
        }
    }
    for message_id in ids {
        let row: Option<(String, String)> = sqlx::query_as(
            "SELECT run_id,agent_id FROM agent_run_mailbox_messages WHERE id=$1 FOR UPDATE",
        )
        .bind(message_id.to_string())
        .fetch_optional(&mut **tx)
        .await
        .map_err(mailbox_command_storage)?;
        let Some((run_id, agent_id)) = row else {
            return Err(ProductMailboxCommandRepositoryError::MessageNotFound {
                target: command.target.clone(),
                message_id,
            });
        };
        let observed = parse_target(&run_id, &agent_id)
            .map_err(|message| ProductMailboxCommandRepositoryError::Storage { message })?;
        if observed != command.target {
            return Err(ProductMailboxCommandRepositoryError::TargetMismatch {
                expected: command.target.clone(),
                observed,
            });
        }
    }
    if let ProductMailboxCommand::Move {
        message_id,
        after_message_id: Some(anchor_message_id),
    } = command.command
    {
        if message_id == anchor_message_id {
            return Err(ProductMailboxCommandRepositoryError::InvalidMove {
                target: command.target.clone(),
                message_id,
                anchor_message_id,
                reason: ProductMailboxInvalidMoveReason::SelfAnchor,
            });
        }
        let message_priority: i32 =
            sqlx::query_scalar("SELECT priority FROM agent_run_mailbox_messages WHERE id=$1")
                .bind(message_id.to_string())
                .fetch_one(&mut **tx)
                .await
                .map_err(mailbox_command_storage)?;
        let anchor_priority: i32 =
            sqlx::query_scalar("SELECT priority FROM agent_run_mailbox_messages WHERE id=$1")
                .bind(anchor_message_id.to_string())
                .fetch_one(&mut **tx)
                .await
                .map_err(mailbox_command_storage)?;
        if message_priority != anchor_priority {
            return Err(ProductMailboxCommandRepositoryError::InvalidMove {
                target: command.target.clone(),
                message_id,
                anchor_message_id,
                reason: ProductMailboxInvalidMoveReason::CrossPriorityLane,
            });
        }
    }
    Ok(())
}

async fn apply_mailbox_command(
    tx: &mut Transaction<'_, Postgres>,
    command: &ProductMailboxDurableCommand,
) -> Result<(), ProductMailboxCommandRepositoryError> {
    match command.command {
        ProductMailboxCommand::Promote { message_id } => {
            let minimum: Option<i64> = sqlx::query_scalar(
                "SELECT MIN(order_key) FROM agent_run_mailbox_messages \
                 WHERE run_id=$1 AND agent_id=$2 AND priority=100 AND id<>$3",
            )
            .bind(command.target.run_id.to_string())
            .bind(command.target.agent_id.to_string())
            .bind(message_id.to_string())
            .fetch_one(&mut **tx)
            .await
            .map_err(mailbox_command_storage)?;
            let order_key = minimum.unwrap_or(1024).checked_sub(1024).ok_or_else(|| {
                ProductMailboxCommandRepositoryError::Storage {
                    message: "Product mailbox promote order key exhausted i64".to_owned(),
                }
            })?;
            sqlx::query(
                "UPDATE agent_run_mailbox_messages \
                 SET priority=100,order_key=$1,updated_at=clock_timestamp() WHERE id=$2",
            )
            .bind(order_key)
            .bind(message_id.to_string())
            .execute(&mut **tx)
            .await
            .map_err(mailbox_command_storage)?;
        }
        ProductMailboxCommand::Delete { message_id } => {
            sqlx::query(
                "UPDATE agent_run_mailbox_messages SET \
                   status='deleted',payload_json=NULL,deleted_at=clock_timestamp(),\
                   updated_at=clock_timestamp() WHERE id=$1",
            )
            .bind(message_id.to_string())
            .execute(&mut **tx)
            .await
            .map_err(mailbox_command_storage)?;
        }
        ProductMailboxCommand::Move {
            message_id,
            after_message_id,
        } => {
            let priority: i32 =
                sqlx::query_scalar("SELECT priority FROM agent_run_mailbox_messages WHERE id=$1")
                    .bind(message_id.to_string())
                    .fetch_one(&mut **tx)
                    .await
                    .map_err(mailbox_command_storage)?;
            let mut ids: Vec<String> = sqlx::query_scalar(
                "SELECT id FROM agent_run_mailbox_messages \
                 WHERE run_id=$1 AND agent_id=$2 AND priority=$3 \
                 ORDER BY order_key ASC,id ASC FOR UPDATE",
            )
            .bind(command.target.run_id.to_string())
            .bind(command.target.agent_id.to_string())
            .bind(priority)
            .fetch_all(&mut **tx)
            .await
            .map_err(mailbox_command_storage)?;
            let message = message_id.to_string();
            ids.retain(|id| id != &message);
            let destination = match after_message_id {
                Some(anchor) => {
                    let anchor = anchor.to_string();
                    ids.iter().position(|id| id == &anchor).ok_or_else(|| {
                        ProductMailboxCommandRepositoryError::Storage {
                            message: "validated Product mailbox anchor disappeared".to_owned(),
                        }
                    })? + 1
                }
                None => 0,
            };
            ids.insert(destination, message);
            for (index, id) in ids.into_iter().enumerate() {
                let order_key = i64::try_from(index)
                    .ok()
                    .and_then(|value| value.checked_mul(1024))
                    .ok_or_else(|| ProductMailboxCommandRepositoryError::Storage {
                        message: "Product mailbox move order key exhausted i64".to_owned(),
                    })?;
                sqlx::query(
                    "UPDATE agent_run_mailbox_messages \
                     SET order_key=$1,updated_at=clock_timestamp() WHERE id=$2",
                )
                .bind(order_key)
                .bind(id)
                .execute(&mut **tx)
                .await
                .map_err(mailbox_command_storage)?;
            }
        }
        ProductMailboxCommand::Resume => {
            let result = sqlx::query(
                "UPDATE agent_run_mailbox_states SET \
                   paused=FALSE,pause_reason=NULL,pause_message=NULL,\
                   updated_at=clock_timestamp() \
                 WHERE run_id=$1 AND agent_id=$2",
            )
            .bind(command.target.run_id.to_string())
            .bind(command.target.agent_id.to_string())
            .execute(&mut **tx)
            .await
            .map_err(mailbox_command_storage)?;
            if result.rows_affected() != 1 {
                return Err(ProductMailboxCommandRepositoryError::Storage {
                    message: "canonical Product mailbox state disappeared".to_owned(),
                });
            }
        }
    }
    Ok(())
}

async fn load_canonical_mailbox(
    tx: &mut Transaction<'_, Postgres>,
    target: &AgentRunTarget,
) -> Result<(Vec<AgentRunMailboxMessage>, Option<AgentRunMailboxState>), String> {
    let row = sqlx::query_as::<_, CanonicalMailboxRows>(
        "SELECT \
           COALESCE((\
             SELECT jsonb_agg(to_jsonb(message) \
                              ORDER BY priority DESC,order_key ASC,id ASC) \
             FROM agent_run_mailbox_messages message \
             WHERE run_id=$1 AND agent_id=$2\
           ),'[]'::JSONB) AS messages,\
           (\
             SELECT to_jsonb(state) \
             FROM agent_run_mailbox_states state \
             WHERE run_id=$1 AND agent_id=$2\
           ) AS state",
    )
    .bind(target.run_id.to_string())
    .bind(target.agent_id.to_string())
    .fetch_one(&mut **tx)
    .await
    .map_err(storage_message)?;
    let rows: Vec<AgentRunMailboxMessageRow> = serde_json::from_value(row.messages)
        .map_err(|error| format!("canonical Product mailbox messages are invalid: {error}"))?;
    let messages: Vec<AgentRunMailboxMessage> = rows
        .into_iter()
        .map(AgentRunMailboxMessage::try_from)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error: agentdash_domain::common::error::DomainError| error.to_string())?;
    let state: Option<AgentRunMailboxState> = row
        .state
        .map(serde_json::from_value::<AgentRunMailboxStateRow>)
        .transpose()
        .map_err(|error| format!("canonical Product mailbox state is invalid: {error}"))?
        .map(AgentRunMailboxState::try_from)
        .transpose()
        .map_err(|error: agentdash_domain::common::error::DomainError| error.to_string())?;
    for message in &messages {
        let observed = AgentRunTarget {
            run_id: message.run_id,
            agent_id: message.agent_id,
        };
        if observed != *target {
            return Err(format!(
                "canonical mailbox message target mismatch: expected {target:?}, observed {observed:?}"
            ));
        }
    }
    if let Some(state) = &state {
        let observed = AgentRunTarget {
            run_id: state.run_id,
            agent_id: state.agent_id,
        };
        if observed != *target {
            return Err(format!(
                "canonical mailbox state target mismatch: expected {target:?}, observed {observed:?}"
            ));
        }
    }
    Ok((messages, state))
}

#[derive(sqlx::FromRow)]
struct CanonicalMailboxRows {
    messages: Value,
    state: Option<Value>,
}

async fn load_mailbox_head(
    tx: &mut Transaction<'_, Postgres>,
    target: &AgentRunTarget,
    for_update: bool,
) -> Result<Option<MailboxHead>, String> {
    let suffix = if for_update { " FOR UPDATE" } else { "" };
    let row = sqlx::query_as::<_, MailboxHeadRow>(&format!(
        "SELECT revision::TEXT AS revision,\
                latest_change_sequence::TEXT AS latest_change_sequence,\
                snapshot_digest,committed_at_ms::TEXT AS committed_at_ms \
         FROM agent_run_product_mailbox_head \
         WHERE target_run_id=$1 AND target_agent_id=$2{suffix}"
    ))
    .bind(target.run_id)
    .bind(target.agent_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(storage_message)?;
    row.map(MailboxHeadRow::into_head).transpose()
}

async fn lock_product_target(
    tx: &mut Transaction<'_, Postgres>,
    target: &AgentRunTarget,
) -> Result<(), sqlx::Error> {
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
        .bind(format!("{}:{}", target.run_id, target.agent_id))
        .execute(&mut **tx)
        .await?;
    Ok(())
}

async fn database_time_ms(tx: &mut Transaction<'_, Postgres>) -> Result<u64, String> {
    let value: String = sqlx::query_scalar(
        "SELECT floor(extract(epoch FROM clock_timestamp()) * 1000)::NUMERIC(20,0)::TEXT",
    )
    .fetch_one(&mut **tx)
    .await
    .map_err(storage_message)?;
    parse_decimal(&value, "database committed_at_ms")
}

fn origin_columns(
    origin: &ProductMailboxChangeOrigin,
) -> (&'static str, Option<&str>, Option<&'static str>) {
    match origin {
        ProductMailboxChangeOrigin::CanonicalReconcile => ("canonical_reconcile", None, None),
        ProductMailboxChangeOrigin::Command {
            client_command_id,
            command_kind,
        } => (
            "command",
            Some(client_command_id.as_str()),
            Some(command_kind_str(*command_kind)),
        ),
    }
}

fn command_kind_str(kind: ProductMailboxCommandKind) -> &'static str {
    match kind {
        ProductMailboxCommandKind::Promote => "promote",
        ProductMailboxCommandKind::Delete => "delete",
        ProductMailboxCommandKind::Move => "move",
        ProductMailboxCommandKind::Resume => "resume",
    }
}

fn parse_command_kind(value: &str) -> Result<ProductMailboxCommandKind, String> {
    match value {
        "promote" => Ok(ProductMailboxCommandKind::Promote),
        "delete" => Ok(ProductMailboxCommandKind::Delete),
        "move" => Ok(ProductMailboxCommandKind::Move),
        "resume" => Ok(ProductMailboxCommandKind::Resume),
        other => Err(format!("invalid Product mailbox command kind: {other}")),
    }
}

fn parse_target(run_id: &str, agent_id: &str) -> Result<AgentRunTarget, String> {
    Ok(AgentRunTarget {
        run_id: run_id
            .parse()
            .map_err(|_| format!("invalid Product mailbox run id: {run_id}"))?,
        agent_id: agent_id
            .parse()
            .map_err(|_| format!("invalid Product mailbox agent id: {agent_id}"))?,
    })
}

fn required_decimal(
    value: Option<String>,
    field: &'static str,
) -> Result<u64, ProductMailboxCommandRepositoryError> {
    let value = value.ok_or_else(|| ProductMailboxCommandRepositoryError::Storage {
        message: format!("terminal Product mailbox receipt is missing {field}"),
    })?;
    parse_decimal(&value, field)
        .map_err(|message| ProductMailboxCommandRepositoryError::Storage { message })
}

fn decimal(value: u64) -> String {
    value.to_string()
}

fn parse_decimal(value: &str, field: &'static str) -> Result<u64, String> {
    value
        .parse()
        .map_err(|_| format!("{field} is not a canonical u64 decimal: {value}"))
}

fn prefixed_surface_columns(prefix: &str) -> String {
    SURFACE_COLUMNS
        .split(',')
        .map(|column| format!("{prefix}.{column}"))
        .collect::<Vec<_>>()
        .join(",")
}

fn to_i64(
    value: u64,
    field: &'static str,
) -> Result<i64, AgentRunAppliedResourceSurfaceWriteError> {
    i64::try_from(value).map_err(
        |_| AgentRunAppliedResourceSurfaceWriteError::CorruptEvidence {
            message: format!("{field} exceeds the signed PostgreSQL bigint persistence range"),
        },
    )
}

fn from_i64(value: i64, field: &'static str) -> Result<u64, String> {
    u64::try_from(value).map_err(|_| format!("{field} is negative"))
}

fn to_json<T: serde::Serialize>(
    value: &T,
    field: &'static str,
) -> Result<Value, AgentRunAppliedResourceSurfaceWriteError> {
    serde_json::to_value(value).map_err(|error| {
        AgentRunAppliedResourceSurfaceWriteError::CorruptEvidence {
            message: format!("failed to encode {field}: {error}"),
        }
    })
}

fn from_json<T: serde::de::DeserializeOwned>(
    value: Value,
    field: &'static str,
) -> Result<T, String> {
    serde_json::from_value(value)
        .map_err(|error| format!("agent_run_applied_resource_surface_snapshot.{field}: {error}"))
}

fn storage_message(error: sqlx::Error) -> String {
    error.to_string()
}

fn surface_repository_error(error: sqlx::Error) -> AgentRunAppliedResourceSurfaceWriteError {
    AgentRunAppliedResourceSurfaceWriteError::Repository {
        message: error.to_string(),
    }
}

fn product_claim_storage(error: sqlx::Error) -> ProductRuntimeCommandClaimError {
    ProductRuntimeCommandClaimError::Storage {
        message: error.to_string(),
    }
}

fn mailbox_read_storage(error: sqlx::Error) -> ProductMailboxReadError {
    ProductMailboxReadError::Storage {
        message: error.to_string(),
    }
}

fn mailbox_command_storage(error: sqlx::Error) -> ProductMailboxCommandRepositoryError {
    ProductMailboxCommandRepositoryError::Storage {
        message: error.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use agentdash_agent_runtime_contract::{
        ManagedRuntimeCommand, RuntimeIdempotencyKey, RuntimeOperationId,
        RuntimeProjectionRevision, RuntimeThreadId,
    };
    use agentdash_application_agentrun::agent_run::{
        AgentRunAppliedResourceSurfaceQueryPort, AppliedTaskGrant, AppliedTaskOperation,
        AppliedTaskScope, AppliedVfsGrant, AppliedVfsMount, AppliedVfsOperation,
        AppliedVfsPathScope,
    };
    use agentdash_domain::agent_run_mailbox::{
        AgentRunMailboxRepository, ConsumptionBarrier, MailboxDelivery, MailboxDrainMode,
        MailboxMessageOrigin, MailboxSourceIdentity, NewAgentRunMailboxMessage,
    };
    use serde_json::json;

    use super::*;
    use crate::PostgresAgentRunMailboxRepository;

    async fn product_test_pool() -> (PgPool, Option<crate::postgres_runtime::PostgresRuntime>) {
        if crate::persistence::postgres::test_database_url().is_some() {
            return (
                crate::persistence::postgres::test_pg_pool("Product final persistence")
                    .await
                    .expect("configured PostgreSQL test pool"),
                None,
            );
        }
        let data_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/product-persistence-postgres-tests");
        let runtime = crate::postgres_runtime::PostgresRuntime::resolve_embedded_at_data_root(
            "product-persistence-tests",
            61,
            data_root,
        )
        .await
        .expect("start isolated embedded PostgreSQL for Product persistence");
        let database_name = format!("product_persistence_{}", Uuid::new_v4().simple());
        sqlx::query(&format!("CREATE DATABASE {database_name}"))
            .execute(&runtime.pool)
            .await
            .expect("create isolated Product persistence database");
        let options = runtime
            .pool
            .connect_options()
            .as_ref()
            .clone()
            .database(&database_name);
        let pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(8)
            .connect_with(options)
            .await
            .expect("connect isolated Product persistence database");
        crate::migration::run_postgres_migrations(&pool)
            .await
            .expect("run final migrations");
        crate::migration::assert_postgres_schema_ready(&pool)
            .await
            .expect("Product persistence schema ready");
        (pool, Some(runtime))
    }

    fn surface(
        target: AgentRunTarget,
        project_id: Uuid,
        marker: &str,
        evidence_revision: u64,
    ) -> AgentRunAppliedResourceSurface {
        AgentRunAppliedResourceSurface {
            target,
            project_id,
            workspace_id: Some(Uuid::new_v4()),
            vfs_mounts: vec![AppliedVfsMount {
                mount_id: "workspace".to_owned(),
                provider: "project_vfs".to_owned(),
                backend_id: "backend-1".to_owned(),
                root_ref: format!("root-{marker}"),
                capabilities: BTreeSet::from([
                    AppliedVfsOperation::Read,
                    AppliedVfsOperation::Write,
                ]),
                default_write: true,
                display_name: marker.to_owned(),
            }],
            default_mount_id: Some("workspace".to_owned()),
            vfs_grants: vec![AppliedVfsGrant {
                mount_id: "workspace".to_owned(),
                operations: BTreeSet::from([AppliedVfsOperation::Read]),
                path_scopes: vec![AppliedVfsPathScope::All],
            }],
            agent_surface_revision: evidence_revision,
            agent_surface_digest: format!("agent-surface-{marker}"),
            vfs_digest: format!("vfs-{marker}"),
            task_grants: vec![AppliedTaskGrant {
                scope: AppliedTaskScope::Project { project_id },
                operations: BTreeSet::from([AppliedTaskOperation::Read]),
            }],
            task_surface_revision: evidence_revision,
            task_surface_digest: format!("task-{marker}"),
            task_provenance: AgentRunAppliedResourceSurfaceProvenance {
                source_kind: "task_projection".to_owned(),
                source_id: format!("task-source-{marker}"),
                source_revision: evidence_revision,
                projection_revision: evidence_revision,
                captured_at_ms: evidence_revision,
            },
            product_binding_digest: format!("binding-{marker}"),
            provenance: AgentRunAppliedResourceSurfaceProvenance {
                source_kind: "product_binding".to_owned(),
                source_id: format!("binding-source-{marker}"),
                source_revision: evidence_revision,
                projection_revision: evidence_revision,
                captured_at_ms: evidence_revision,
            },
        }
    }

    fn prepared(
        expected: Option<u64>,
        revision: u64,
        surface: AgentRunAppliedResourceSurface,
    ) -> PrepareAgentRunAppliedResourceSurface {
        PrepareAgentRunAppliedResourceSurface {
            expected_current_snapshot_revision: expected,
            next: AgentRunAppliedResourceSurfaceSnapshot {
                snapshot_revision: revision,
                surface,
            },
        }
    }

    async fn insert_agent_run(pool: &PgPool, target: &AgentRunTarget, project_id: Uuid) {
        sqlx::query(
            "INSERT INTO projects (id,name,description,config,created_at,updated_at) \
             VALUES ($1,'Product PG test','','{}',now(),now())",
        )
        .bind(project_id.to_string())
        .execute(pool)
        .await
        .expect("insert project");
        sqlx::query(
            "INSERT INTO lifecycle_runs \
             (id,project_id,topology,orchestrations,status,execution_log,\
              created_at,updated_at,last_activity_at) \
             VALUES ($1,$2,'plain',$3,'ready',$4,now(),now(),now())",
        )
        .bind(target.run_id.to_string())
        .bind(project_id.to_string())
        .bind(json!([]))
        .bind(json!([]))
        .execute(pool)
        .await
        .expect("insert lifecycle run");
        sqlx::query(
            "INSERT INTO lifecycle_agents \
             (id,run_id,project_id,source,status,created_at,updated_at) \
             VALUES ($1,$2,$3,'unknown','idle',now(),now())",
        )
        .bind(target.agent_id.to_string())
        .bind(target.run_id.to_string())
        .bind(project_id.to_string())
        .execute(pool)
        .await
        .expect("insert lifecycle agent");
        sqlx::query(
            "INSERT INTO agent_run_mailbox_states \
             (run_id,agent_id,paused,pause_reason,pause_message,updated_at) \
             VALUES ($1,$2,TRUE,'manual','paused for test',now())",
        )
        .bind(target.run_id.to_string())
        .bind(target.agent_id.to_string())
        .execute(pool)
        .await
        .expect("insert canonical mailbox state");
    }

    fn new_message(
        target: &AgentRunTarget,
        id: Uuid,
        dedup: &str,
        preview: &str,
    ) -> NewAgentRunMailboxMessage {
        NewAgentRunMailboxMessage {
            id: Some(id),
            run_id: target.run_id,
            agent_id: target.agent_id,
            origin: MailboxMessageOrigin::User,
            source: MailboxSourceIdentity::composer(),
            delivery: MailboxDelivery::LaunchOrContinueTurn,
            barrier: ConsumptionBarrier::ImmediateIfIdle,
            drain_mode: MailboxDrainMode::One,
            priority: 0,
            source_dedup_key: Some(dedup.to_owned()),
            delivery_request_digest: format!("sha256:{dedup}"),
            payload_json: Some(json!([{"type":"text","text":preview}])),
            launch_planning_input: Some(json!({"command":"send"})),
            preview: preview.to_owned(),
            has_images: false,
            retain_payload: true,
        }
    }

    fn mailbox_command(
        target: &AgentRunTarget,
        client_command_id: &str,
        request_digest: &str,
        command: ProductMailboxCommand,
    ) -> ProductMailboxDurableCommand {
        ProductMailboxDurableCommand {
            target: target.clone(),
            client_command_id: client_command_id.to_owned(),
            request_digest: request_digest.to_owned(),
            command,
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn final_product_persistence_contract_runs_on_real_postgres() {
        let (pool, _runtime) = product_test_pool().await;

        let surface_repo = PostgresAgentRunAppliedResourceSurfaceRepository::new(pool.clone());
        surface_repo
            .initialize()
            .await
            .expect("surface repository ready");
        let target = AgentRunTarget {
            run_id: Uuid::new_v4(),
            agent_id: Uuid::new_v4(),
        };
        let project_id = Uuid::new_v4();
        let exact = prepared(None, 1, surface(target.clone(), project_id, "exact", 1));
        let (first, replay) = tokio::join!(
            surface_repo.commit(exact.clone()),
            surface_repo.commit(exact.clone())
        );
        assert!(
            matches!(
                (&first, &replay),
                (
                    Ok(AgentRunAppliedResourceSurfaceCommitOutcome::Committed),
                    Ok(AgentRunAppliedResourceSurfaceCommitOutcome::AlreadyCurrent)
                ) | (
                    Ok(AgentRunAppliedResourceSurfaceCommitOutcome::AlreadyCurrent),
                    Ok(AgentRunAppliedResourceSurfaceCommitOutcome::Committed)
                )
            ),
            "concurrent exact first writers must commit once and replay once: {first:?} {replay:?}"
        );

        let conflict_target = AgentRunTarget {
            run_id: Uuid::new_v4(),
            agent_id: Uuid::new_v4(),
        };
        let left = prepared(
            None,
            1,
            surface(conflict_target.clone(), project_id, "left", 1),
        );
        let right = prepared(
            None,
            1,
            surface(conflict_target.clone(), project_id, "right", 1),
        );
        let (left_result, right_result) =
            tokio::join!(surface_repo.commit(left), surface_repo.commit(right));
        assert!(
            matches!(
                (&left_result, &right_result),
                (
                    Ok(AgentRunAppliedResourceSurfaceCommitOutcome::Committed),
                    Err(AgentRunAppliedResourceSurfaceWriteError::Conflict { .. })
                ) | (
                    Err(AgentRunAppliedResourceSurfaceWriteError::Conflict { .. }),
                    Ok(AgentRunAppliedResourceSurfaceCommitOutcome::Committed)
                )
            ),
            "different full rows at the same revision must conflict: {left_result:?} {right_result:?}"
        );

        let next_left = prepared(
            Some(1),
            2,
            surface(target.clone(), project_id, "next-left", 2),
        );
        let next_right = prepared(
            Some(1),
            2,
            surface(target.clone(), project_id, "next-right", 2),
        );
        let (next_left_result, next_right_result) = tokio::join!(
            surface_repo.commit(next_left),
            surface_repo.commit(next_right)
        );
        assert!(
            matches!(
                (&next_left_result, &next_right_result),
                (
                    Ok(AgentRunAppliedResourceSurfaceCommitOutcome::Committed),
                    Err(AgentRunAppliedResourceSurfaceWriteError::Conflict { .. })
                ) | (
                    Err(AgentRunAppliedResourceSurfaceWriteError::Conflict { .. }),
                    Ok(AgentRunAppliedResourceSurfaceCommitOutcome::Committed)
                )
            ),
            "current pointer CAS must choose one complete revision: {next_left_result:?} {next_right_result:?}"
        );
        assert!(matches!(
            surface_repo.commit(exact).await,
            Err(AgentRunAppliedResourceSurfaceWriteError::Conflict { .. })
        ));
        let current = surface_repo
            .applied_resource_surface(&target, Some(2))
            .await
            .expect("query current complete surface");
        assert_eq!(current.snapshot_revision, 2);
        assert!(matches!(
            surface_repo
                .applied_resource_surface(&target, Some(1))
                .await,
            Err(AgentRunAppliedResourceSurfaceQueryError::ProjectionStale {
                expected_revision: 1,
                actual_revision: 2
            })
        ));

        let max_target = AgentRunTarget {
            run_id: Uuid::new_v4(),
            agent_id: Uuid::new_v4(),
        };
        let max_surface = surface(
            max_target.clone(),
            project_id,
            "signed-max",
            i64::MAX as u64,
        );
        assert!(matches!(
            surface_repo
                .commit(prepared(None, 1, max_surface.clone()))
                .await,
            Ok(AgentRunAppliedResourceSurfaceCommitOutcome::Committed)
        ));
        let mut overflow_surface = max_surface;
        overflow_surface.agent_surface_revision = i64::MAX as u64 + 1;
        assert!(matches!(
            surface_repo
                .commit(prepared(Some(1), 2, overflow_surface))
                .await,
            Err(AgentRunAppliedResourceSurfaceWriteError::CorruptEvidence { .. })
        ));

        let claim_repo = PostgresProductRuntimeCommandClaimRepository::new(pool.clone());
        claim_repo
            .initialize()
            .await
            .expect("claim repository ready");
        let claim_target = AgentRunTarget {
            run_id: Uuid::new_v4(),
            agent_id: Uuid::new_v4(),
        };
        let envelope = ManagedRuntimeCommandEnvelope {
            operation_id: RuntimeOperationId::new("operation-1").expect("operation id"),
            idempotency_key: RuntimeIdempotencyKey::new("idempotency-1").expect("idempotency id"),
            thread_id: RuntimeThreadId::new("thread-1").expect("thread id"),
            expected_revision: Some(RuntimeProjectionRevision(u64::MAX)),
            command: ManagedRuntimeCommand::RequestCompaction,
        };
        let claimed = claim_repo
            .claim(ProductRuntimeCommandClaimRequest {
                target: claim_target.clone(),
                client_command_id: "runtime-command-1".to_owned(),
                request_digest: "sha256:request-a".to_owned(),
                envelope: envelope.clone(),
            })
            .await
            .expect("claim resolved envelope");
        assert_eq!(claimed, envelope);
        let restarted_claim_repo = PostgresProductRuntimeCommandClaimRepository::new(pool.clone());
        assert_eq!(
            restarted_claim_repo
                .load(&claim_target, "runtime-command-1", "sha256:request-a")
                .await
                .expect("load after process restart"),
            Some(envelope.clone())
        );
        assert!(matches!(
            restarted_claim_repo
                .claim(ProductRuntimeCommandClaimRequest {
                    target: claim_target.clone(),
                    client_command_id: "runtime-command-1".to_owned(),
                    request_digest: "sha256:request-b".to_owned(),
                    envelope,
                })
                .await,
            Err(ProductRuntimeCommandClaimError::RequestDigestConflict { .. })
        ));

        let mailbox_repo = PostgresProductMailboxRepository::with_change_retention(pool.clone(), 2)
            .expect("positive retention");
        mailbox_repo
            .initialize()
            .await
            .expect("mailbox repository ready");
        let canonical_mailbox = PostgresAgentRunMailboxRepository::new(pool.clone());
        let mailbox_target = AgentRunTarget {
            run_id: Uuid::new_v4(),
            agent_id: Uuid::new_v4(),
        };
        let mailbox_project = Uuid::new_v4();
        insert_agent_run(&pool, &mailbox_target, mailbox_project).await;
        let message_a = Uuid::new_v4();
        let message_b = Uuid::new_v4();
        let message_c = Uuid::new_v4();
        for (id, dedup, preview) in [
            (message_a, "message-a", "A"),
            (message_b, "message-b", "B"),
            (message_c, "message-c", "C"),
        ] {
            canonical_mailbox
                .create_message(new_message(&mailbox_target, id, dedup, preview))
                .await
                .expect("create canonical mailbox message");
        }
        let initial = mailbox_repo
            .snapshot(&mailbox_target)
            .await
            .expect("initial reconcile");
        assert_eq!(initial.cursor.revision, 1);
        assert_eq!(initial.cursor.latest_change_sequence, 1);

        let other_target = AgentRunTarget {
            run_id: Uuid::new_v4(),
            agent_id: Uuid::new_v4(),
        };
        let other_project = Uuid::new_v4();
        insert_agent_run(&pool, &other_target, other_project).await;
        let other_message = Uuid::new_v4();
        canonical_mailbox
            .create_message(new_message(
                &other_target,
                other_message,
                "other-message",
                "other",
            ))
            .await
            .expect("create other target message");
        let before_cross_target = mailbox_repo
            .snapshot(&mailbox_target)
            .await
            .expect("snapshot before cross target commands");
        assert!(matches!(
            mailbox_repo
                .execute(mailbox_command(
                    &mailbox_target,
                    "cross-delete",
                    "sha256:cross-delete",
                    ProductMailboxCommand::Delete {
                        message_id: other_message
                    }
                ))
                .await,
            Err(ProductMailboxCommandRepositoryError::TargetMismatch { .. })
        ));
        assert!(matches!(
            mailbox_repo
                .execute(mailbox_command(
                    &mailbox_target,
                    "cross-move",
                    "sha256:cross-move",
                    ProductMailboxCommand::Move {
                        message_id: message_a,
                        after_message_id: Some(other_message)
                    }
                ))
                .await,
            Err(ProductMailboxCommandRepositoryError::TargetMismatch { .. })
        ));
        let after_cross_target = mailbox_repo
            .snapshot(&mailbox_target)
            .await
            .expect("snapshot after rejected cross target commands");
        assert_eq!(before_cross_target.cursor, after_cross_target.cursor);
        assert_eq!(before_cross_target.commit, after_cross_target.commit);

        sqlx::query(
            "UPDATE agent_run_mailbox_messages \
             SET priority=100,updated_at=clock_timestamp() WHERE id=$1",
        )
        .bind(message_b.to_string())
        .execute(&pool)
        .await
        .expect("place anchor in another priority lane");
        let before_invalid_move = mailbox_repo
            .snapshot(&mailbox_target)
            .await
            .expect("reconcile priority lane fixture");
        let rows_before_invalid_move: Vec<(String, i32, i64, String)> = sqlx::query_as(
            "SELECT id,priority,order_key,status FROM agent_run_mailbox_messages \
             WHERE run_id=$1 AND agent_id=$2 ORDER BY id",
        )
        .bind(mailbox_target.run_id.to_string())
        .bind(mailbox_target.agent_id.to_string())
        .fetch_all(&pool)
        .await
        .expect("load rows before invalid moves");
        let receipts_before_invalid_move: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM agent_run_product_mailbox_command_receipt \
             WHERE target_run_id=$1 AND target_agent_id=$2",
        )
        .bind(mailbox_target.run_id)
        .bind(mailbox_target.agent_id)
        .fetch_one(&pool)
        .await
        .expect("count receipts before invalid moves");
        assert!(matches!(
            mailbox_repo
                .execute(mailbox_command(
                    &mailbox_target,
                    "self-anchor",
                    "sha256:self-anchor",
                    ProductMailboxCommand::Move {
                        message_id: message_a,
                        after_message_id: Some(message_a)
                    }
                ))
                .await,
            Err(ProductMailboxCommandRepositoryError::InvalidMove {
                reason: ProductMailboxInvalidMoveReason::SelfAnchor,
                ..
            })
        ));
        assert!(matches!(
            mailbox_repo
                .execute(mailbox_command(
                    &mailbox_target,
                    "cross-priority",
                    "sha256:cross-priority",
                    ProductMailboxCommand::Move {
                        message_id: message_a,
                        after_message_id: Some(message_b)
                    }
                ))
                .await,
            Err(ProductMailboxCommandRepositoryError::InvalidMove {
                reason: ProductMailboxInvalidMoveReason::CrossPriorityLane,
                ..
            })
        ));
        let rows_after_invalid_move: Vec<(String, i32, i64, String)> = sqlx::query_as(
            "SELECT id,priority,order_key,status FROM agent_run_mailbox_messages \
             WHERE run_id=$1 AND agent_id=$2 ORDER BY id",
        )
        .bind(mailbox_target.run_id.to_string())
        .bind(mailbox_target.agent_id.to_string())
        .fetch_all(&pool)
        .await
        .expect("load rows after invalid moves");
        let receipts_after_invalid_move: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM agent_run_product_mailbox_command_receipt \
             WHERE target_run_id=$1 AND target_agent_id=$2",
        )
        .bind(mailbox_target.run_id)
        .bind(mailbox_target.agent_id)
        .fetch_one(&pool)
        .await
        .expect("count receipts after invalid moves");
        let after_invalid_move = mailbox_repo
            .snapshot(&mailbox_target)
            .await
            .expect("snapshot after invalid moves");
        assert_eq!(rows_after_invalid_move, rows_before_invalid_move);
        assert_eq!(receipts_after_invalid_move, receipts_before_invalid_move);
        assert_eq!(after_invalid_move.cursor, before_invalid_move.cursor);
        assert_eq!(after_invalid_move.commit, before_invalid_move.commit);

        let moved = mailbox_repo
            .execute(mailbox_command(
                &mailbox_target,
                "move",
                "sha256:move",
                ProductMailboxCommand::Move {
                    message_id: message_c,
                    after_message_id: Some(message_a),
                },
            ))
            .await
            .expect("move commits");
        assert!(!moved.replayed);
        let promoted = mailbox_repo
            .execute(mailbox_command(
                &mailbox_target,
                "promote",
                "sha256:promote",
                ProductMailboxCommand::Promote {
                    message_id: message_a,
                },
            ))
            .await
            .expect("promote commits");
        assert!(!promoted.replayed);
        let deleted = mailbox_repo
            .execute(mailbox_command(
                &mailbox_target,
                "delete",
                "sha256:delete",
                ProductMailboxCommand::Delete {
                    message_id: message_b,
                },
            ))
            .await
            .expect("delete commits");
        let resumed = mailbox_repo
            .execute(mailbox_command(
                &mailbox_target,
                "resume",
                "sha256:resume",
                ProductMailboxCommand::Resume,
            ))
            .await
            .expect("resume commits");
        assert!(resumed.receipt.revision >= deleted.receipt.revision);
        let restarted_mailbox =
            PostgresProductMailboxRepository::with_change_retention(pool.clone(), 2)
                .expect("positive retention");
        let replayed = restarted_mailbox
            .execute(mailbox_command(
                &mailbox_target,
                "delete",
                "sha256:delete",
                ProductMailboxCommand::Delete {
                    message_id: message_b,
                },
            ))
            .await
            .expect("restart replays terminal receipt");
        assert!(replayed.replayed);
        assert_eq!(replayed.receipt, deleted.receipt);
        let cursor_before_conflict = restarted_mailbox
            .snapshot(&mailbox_target)
            .await
            .expect("snapshot before digest conflict")
            .cursor;
        assert!(matches!(
            restarted_mailbox
                .execute(mailbox_command(
                    &mailbox_target,
                    "delete",
                    "sha256:different",
                    ProductMailboxCommand::Delete {
                        message_id: message_a
                    }
                ))
                .await,
            Err(ProductMailboxCommandRepositoryError::RequestDigestConflict { .. })
        ));
        assert_eq!(
            restarted_mailbox
                .snapshot(&mailbox_target)
                .await
                .expect("snapshot after digest conflict")
                .cursor,
            cursor_before_conflict
        );

        let rollback_message = Uuid::new_v4();
        canonical_mailbox
            .create_message(new_message(
                &mailbox_target,
                rollback_message,
                "rollback-message",
                "rollback",
            ))
            .await
            .expect("create rollback message");
        restarted_mailbox
            .snapshot(&mailbox_target)
            .await
            .expect("reconcile rollback message");
        sqlx::query(
            "CREATE FUNCTION fail_product_mailbox_head_update() RETURNS trigger \
             LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'injected Product mailbox failure'; END $$",
        )
        .execute(&pool)
        .await
        .expect("create failure function");
        sqlx::query(
            "CREATE TRIGGER fail_product_mailbox_head_update \
             BEFORE UPDATE ON agent_run_product_mailbox_head \
             FOR EACH ROW EXECUTE FUNCTION fail_product_mailbox_head_update()",
        )
        .execute(&pool)
        .await
        .expect("create failure trigger");
        assert!(matches!(
            restarted_mailbox
                .execute(mailbox_command(
                    &mailbox_target,
                    "rollback-delete",
                    "sha256:rollback-delete",
                    ProductMailboxCommand::Delete {
                        message_id: rollback_message
                    }
                ))
                .await,
            Err(ProductMailboxCommandRepositoryError::Storage { .. })
        ));
        sqlx::query(
            "DROP TRIGGER fail_product_mailbox_head_update \
             ON agent_run_product_mailbox_head",
        )
        .execute(&pool)
        .await
        .expect("drop failure trigger");
        sqlx::query("DROP FUNCTION fail_product_mailbox_head_update()")
            .execute(&pool)
            .await
            .expect("drop failure function");
        let rollback_status: String =
            sqlx::query_scalar("SELECT status FROM agent_run_mailbox_messages WHERE id=$1")
                .bind(rollback_message.to_string())
                .fetch_one(&pool)
                .await
                .expect("load rollback message");
        assert_ne!(rollback_status, "deleted");
        let rollback_receipts: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM agent_run_product_mailbox_command_receipt \
             WHERE target_run_id=$1 AND target_agent_id=$2 AND client_command_id=$3",
        )
        .bind(mailbox_target.run_id)
        .bind(mailbox_target.agent_id)
        .bind("rollback-delete")
        .fetch_one(&pool)
        .await
        .expect("count rollback receipts");
        assert_eq!(rollback_receipts, 0);

        let before_external = restarted_mailbox
            .snapshot(&mailbox_target)
            .await
            .expect("snapshot before external mutation");
        sqlx::query(
            "UPDATE agent_run_mailbox_messages \
             SET preview='externally reconciled',updated_at=clock_timestamp() WHERE id=$1",
        )
        .bind(message_c.to_string())
        .execute(&pool)
        .await
        .expect("external canonical mutation");
        let after_external = restarted_mailbox
            .snapshot(&mailbox_target)
            .await
            .expect("reconcile external canonical mutation");
        assert_eq!(
            after_external.cursor.latest_change_sequence,
            before_external.cursor.latest_change_sequence + 1
        );
        assert_eq!(
            restarted_mailbox
                .snapshot(&mailbox_target)
                .await
                .expect("same canonical state does not create another change")
                .cursor,
            after_external.cursor
        );

        sqlx::query(
            "UPDATE agent_run_mailbox_messages \
             SET preview='concurrent reconcile',updated_at=clock_timestamp() WHERE id=$1",
        )
        .bind(message_c.to_string())
        .execute(&pool)
        .await
        .expect("prepare concurrent external reconcile");
        let (reconcile_left, reconcile_right) = tokio::join!(
            restarted_mailbox.snapshot(&mailbox_target),
            restarted_mailbox.snapshot(&mailbox_target)
        );
        let reconcile_left = reconcile_left.expect("left concurrent reconcile");
        let reconcile_right = reconcile_right.expect("right concurrent reconcile");
        assert_eq!(reconcile_left.cursor, reconcile_right.cursor);
        assert_eq!(reconcile_left.commit, reconcile_right.commit);
        assert_eq!(
            reconcile_left.cursor.latest_change_sequence,
            after_external.cursor.latest_change_sequence + 1
        );

        let concurrent_message = Uuid::new_v4();
        canonical_mailbox
            .create_message(new_message(
                &mailbox_target,
                concurrent_message,
                "concurrent-command-message",
                "concurrent command",
            ))
            .await
            .expect("create concurrent command message");
        restarted_mailbox
            .snapshot(&mailbox_target)
            .await
            .expect("reconcile concurrent command fixture");
        let concurrent_command = mailbox_command(
            &mailbox_target,
            "concurrent-promote",
            "sha256:concurrent-promote",
            ProductMailboxCommand::Promote {
                message_id: concurrent_message,
            },
        );
        let (claim_left, claim_right) = tokio::join!(
            restarted_mailbox.execute(concurrent_command.clone()),
            restarted_mailbox.execute(concurrent_command)
        );
        let claim_left = claim_left.expect("left concurrent command claim");
        let claim_right = claim_right.expect("right concurrent command claim");
        assert_eq!(claim_left.receipt, claim_right.receipt);
        assert_ne!(claim_left.replayed, claim_right.replayed);

        let gap = restarted_mailbox
            .changes(&mailbox_target, 0, 1)
            .await
            .expect("retention gap");
        let gap_evidence = gap.gap.expect("old cursor must receive typed gap");
        assert!(gap_evidence.earliest_available > 1);
        assert_eq!(
            gap_evidence.latest_available,
            gap.head.latest_change_sequence
        );
        let first_page = restarted_mailbox
            .changes(&mailbox_target, gap_evidence.earliest_available - 1, 1)
            .await
            .expect("partial retained page");
        assert_eq!(first_page.changes.len(), 1);
        let final_page = restarted_mailbox
            .changes(&mailbox_target, first_page.next, 10)
            .await
            .expect("final retained page");
        assert_eq!(final_page.next, final_page.head.latest_change_sequence);
        assert_eq!(
            final_page.changes.last().expect("final change").commit,
            final_page.head_commit
        );

        if final_page.changes.len() >= 1 {
            let latest = final_page
                .changes
                .last()
                .expect("latest retained change")
                .sequence;
            let previous = latest - 1;
            let previous_revision: String = sqlx::query_scalar(
                "SELECT revision::TEXT FROM agent_run_product_mailbox_change \
                 WHERE target_run_id=$1 AND target_agent_id=$2 \
                   AND sequence=$3::NUMERIC(20,0)",
            )
            .bind(mailbox_target.run_id)
            .bind(mailbox_target.agent_id)
            .bind(decimal(previous))
            .fetch_one(&pool)
            .await
            .expect("load previous retained revision");
            let previous_revision =
                parse_decimal(&previous_revision, "previous revision").expect("u64 revision");
            if previous_revision > 1 {
                sqlx::query(
                    "UPDATE agent_run_product_mailbox_change SET revision=1 \
                     WHERE target_run_id=$1 AND target_agent_id=$2 \
                       AND sequence=$3::NUMERIC(20,0)",
                )
                .bind(mailbox_target.run_id)
                .bind(mailbox_target.agent_id)
                .bind(decimal(latest))
                .execute(&pool)
                .await
                .expect("inject revision regression");
                assert!(matches!(
                    restarted_mailbox
                        .changes(&mailbox_target, previous, 1)
                        .await,
                    Err(ProductMailboxReadError::RevisionRegression { .. })
                ));
                sqlx::query(
                    "UPDATE agent_run_product_mailbox_change \
                     SET revision=$4::NUMERIC(20,0) \
                     WHERE target_run_id=$1 AND target_agent_id=$2 \
                       AND sequence=$3::NUMERIC(20,0)",
                )
                .bind(mailbox_target.run_id)
                .bind(mailbox_target.agent_id)
                .bind(decimal(latest))
                .bind(decimal(final_page.head.revision))
                .execute(&pool)
                .await
                .expect("restore revision");
            }
        }

        let max_cursor_target = AgentRunTarget {
            run_id: Uuid::new_v4(),
            agent_id: Uuid::new_v4(),
        };
        let empty_digest = canonical_product_mailbox_digest(&[], None);
        sqlx::query(
            "INSERT INTO agent_run_product_mailbox_head (\
             target_run_id,target_agent_id,revision,latest_change_sequence,\
             snapshot_digest,committed_at_ms) \
             VALUES ($1,$2,$3::NUMERIC(20,0),$3::NUMERIC(20,0),$4,$3::NUMERIC(20,0))",
        )
        .bind(max_cursor_target.run_id)
        .bind(max_cursor_target.agent_id)
        .bind(decimal(u64::MAX))
        .bind(empty_digest.as_str())
        .execute(&pool)
        .await
        .expect("persist maximum Product cursor");
        sqlx::query(
            "INSERT INTO agent_run_product_mailbox_change (\
             target_run_id,target_agent_id,sequence,change_id,revision,snapshot_digest,\
             committed_at_ms,origin_kind) \
             VALUES ($1,$2,$3::NUMERIC(20,0),$4,$3::NUMERIC(20,0),$5,\
                     $3::NUMERIC(20,0),'canonical_reconcile')",
        )
        .bind(max_cursor_target.run_id)
        .bind(max_cursor_target.agent_id)
        .bind(decimal(u64::MAX))
        .bind(Uuid::new_v4())
        .bind(empty_digest.as_str())
        .execute(&pool)
        .await
        .expect("persist maximum Product change coordinate");
        let maximum_page = PostgresProductMailboxRepository::new(pool.clone())
            .changes(&max_cursor_target, u64::MAX, 1)
            .await
            .expect("maximum canonical u64 coordinates are accepted");
        assert_eq!(maximum_page.head.revision, u64::MAX);
        assert_eq!(maximum_page.next, u64::MAX);
        assert!(maximum_page.changes.is_empty());
    }
}
