use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use agentdash_agent::dash::{
    AgentHistoryEntry, AgentSessionId, DashAgentRepository, DashAgentRepositoryState,
    DashAgentRepositoryStore, DashServiceError,
};
use agentdash_agent_service_api::{
    AgentEffectIdentity, AgentServiceError, AgentServiceErrorCode, AgentSourceCoordinate,
};
use agentdash_integration_native_agent::{
    DashCompleteAgentStore, DashCompleteAtomicCommit, DashCompleteEffectRecord,
    DashCompleteSourceMetadata, DashCompleteSourceMutation,
};
use async_trait::async_trait;
use serde_json::Value;
use sqlx::{PgPool, Postgres, Row, Transaction};

pub struct PostgresDashCompleteAgentStore {
    pool: PgPool,
    repositories: PostgresDashAgentRepositoryStore,
}

impl PostgresDashCompleteAgentStore {
    pub fn new(pool: PgPool) -> Self {
        Self {
            repositories: PostgresDashAgentRepositoryStore::new(pool.clone()),
            pool,
        }
    }
}

pub struct PostgresDashAgentRepositoryStore {
    pool: PgPool,
}

impl PostgresDashAgentRepositoryStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

struct PostgresDashAgentRepository {
    pool: PgPool,
    source: AgentSessionId,
}

#[async_trait]
impl DashAgentRepositoryStore for PostgresDashAgentRepositoryStore {
    async fn create(
        &self,
        source: &AgentSessionId,
        initial: DashAgentRepositoryState,
    ) -> Result<Arc<dyn DashAgentRepository>, DashServiceError> {
        let mut tx = self.pool.begin().await.map_err(dash_database_error)?;
        insert_repository_state(&mut tx, &source.0, 1, &initial).await?;
        tx.commit().await.map_err(dash_database_error)?;
        Ok(Arc::new(PostgresDashAgentRepository {
            pool: self.pool.clone(),
            source: source.clone(),
        }))
    }

    async fn open(
        &self,
        source: &AgentSessionId,
    ) -> Result<Option<Arc<dyn DashAgentRepository>>, DashServiceError> {
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM dash_agent_session WHERE source_coordinate=$1)",
        )
        .bind(&source.0)
        .fetch_one(&self.pool)
        .await
        .map_err(dash_database_error)?;
        Ok(exists.then(|| {
            Arc::new(PostgresDashAgentRepository {
                pool: self.pool.clone(),
                source: source.clone(),
            }) as Arc<dyn DashAgentRepository>
        }))
    }
}

#[async_trait]
impl DashAgentRepository for PostgresDashAgentRepository {
    async fn initialize(&self, initial: DashAgentRepositoryState) -> Result<(), DashServiceError> {
        let mut tx = self.pool.begin().await.map_err(dash_database_error)?;
        insert_repository_state(&mut tx, &self.source.0, 1, &initial).await?;
        tx.commit().await.map_err(dash_database_error)
    }

    async fn load(&self) -> Result<DashAgentRepositoryState, DashServiceError> {
        let mut tx = self.pool.begin().await.map_err(dash_database_error)?;
        let (_, state) = lock_repository_state(&mut tx, &self.source.0)
            .await?
            .ok_or_else(|| DashServiceError::InvalidState {
                message: format!("Dash source {} was not found", self.source.0),
            })?;
        tx.commit().await.map_err(dash_database_error)?;
        Ok(state)
    }

    async fn compare_and_swap(
        &self,
        expected: DashAgentRepositoryState,
        replacement: DashAgentRepositoryState,
    ) -> Result<(), DashServiceError> {
        let mut tx = self.pool.begin().await.map_err(dash_database_error)?;
        let (revision, current) = lock_repository_state(&mut tx, &self.source.0)
            .await?
            .ok_or_else(|| DashServiceError::InvalidState {
                message: format!("Dash source {} was not found", self.source.0),
            })?;
        if current != expected {
            return Err(DashServiceError::Conflict {
                message: format!("Dash source {} repository state changed", self.source.0),
            });
        }
        replace_repository_state(&mut tx, &self.source.0, revision, &expected, &replacement)
            .await?;
        tx.commit().await.map_err(dash_database_error)
    }
}

#[async_trait]
impl DashCompleteAgentStore for PostgresDashCompleteAgentStore {
    fn repositories(&self) -> &dyn DashAgentRepositoryStore {
        &self.repositories
    }

    async fn load_source(
        &self,
        source: &AgentSourceCoordinate,
    ) -> Result<Option<DashCompleteSourceMetadata>, AgentServiceError> {
        let mut tx = self.pool.begin().await.map_err(agent_database_error)?;
        let repository = lock_repository_state(&mut tx, source.as_str())
            .await
            .map_err(agent_from_dash_error)?;
        let value: Option<Value> = sqlx::query_scalar(
            "SELECT metadata FROM dash_complete_source \
             WHERE source_coordinate=$1 FOR UPDATE",
        )
        .bind(source.as_str())
        .fetch_optional(&mut *tx)
        .await
        .map_err(agent_database_error)?;
        if value.is_some() && repository.is_none() {
            return Err(agent_internal_error(format!(
                "Dash source {} metadata has no repository aggregate",
                source.as_str()
            )));
        }
        let metadata = value
            .map(|value| {
                serde_json::from_value(value).map_err(|error| {
                    agent_internal_error(format!("decode Dash source metadata: {error}"))
                })
            })
            .transpose()?;
        tx.commit().await.map_err(agent_database_error)?;
        Ok(metadata)
    }

    async fn load_effect(
        &self,
        identity: &AgentEffectIdentity,
    ) -> Result<Option<DashCompleteEffectRecord>, AgentServiceError> {
        let row = sqlx::query(
            "SELECT request_fingerprint,receipt,inspection,record \
             FROM dash_complete_effect WHERE effect_id=$1",
        )
        .bind(identity.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(agent_database_error)?;
        let Some(row) = row else {
            return Ok(None);
        };
        decode_complete_effect_row(&row, identity).map(Some)
    }

    async fn commit(&self, commit: DashCompleteAtomicCommit) -> Result<(), AgentServiceError> {
        let mut tx = self.pool.begin().await.map_err(agent_database_error)?;
        lock_complete_identity(&mut tx, 4_401, commit.effect_id.as_str()).await?;
        let current_effect = lock_complete_effect(&mut tx, &commit.effect_id).await?;
        if current_effect.as_ref() == Some(&commit.replacement_effect) {
            tx.rollback().await.map_err(agent_database_error)?;
            return Ok(());
        }
        if current_effect != commit.expected_effect {
            return Err(agent_conflict("Dash Complete Agent effect state changed"));
        }

        let mut mutations = commit.source_mutations;
        mutations.sort_by(|left, right| mutation_source(left).cmp(mutation_source(right)));
        let mut sources = mutations
            .iter()
            .map(|mutation| mutation_source(mutation).as_str())
            .collect::<Vec<_>>();
        sources.dedup();
        for source in sources {
            lock_complete_identity(&mut tx, 4_402, source).await?;
        }
        for mutation in mutations {
            match mutation {
                DashCompleteSourceMutation::Create {
                    source,
                    repository,
                    metadata,
                } => {
                    if lock_repository_state(&mut tx, source.as_str())
                        .await
                        .map_err(agent_from_dash_error)?
                        .is_some()
                    {
                        return Err(agent_conflict(format!(
                            "Dash source {} already exists",
                            source.as_str()
                        )));
                    }
                    insert_repository_state(&mut tx, source.as_str(), 1, &repository)
                        .await
                        .map_err(agent_from_dash_error)?;
                    sqlx::query(
                        "INSERT INTO dash_complete_source \
                         (source_coordinate,repository_revision,metadata) VALUES ($1,1,$2)",
                    )
                    .bind(source.as_str())
                    .bind(to_json(&*metadata)?)
                    .execute(&mut *tx)
                    .await
                    .map_err(agent_database_error)?;
                }
                DashCompleteSourceMutation::CompareAndSwap {
                    source,
                    expected_repository,
                    replacement_repository,
                    expected_metadata,
                    replacement_metadata,
                } => {
                    let (revision, current_repository) =
                        lock_repository_state(&mut tx, source.as_str())
                            .await
                            .map_err(agent_from_dash_error)?
                            .ok_or_else(|| {
                                agent_conflict(format!(
                                    "Dash source {} was not found",
                                    source.as_str()
                                ))
                            })?;
                    let current_metadata: Option<Value> = sqlx::query_scalar(
                        "SELECT metadata FROM dash_complete_source \
                         WHERE source_coordinate=$1 FOR UPDATE",
                    )
                    .bind(source.as_str())
                    .fetch_optional(&mut *tx)
                    .await
                    .map_err(agent_database_error)?;
                    let current_metadata = current_metadata
                        .map(|value| serde_json::from_value(value))
                        .transpose()
                        .map_err(|error| {
                            agent_internal_error(format!(
                                "decode Dash Complete Agent source metadata: {error}"
                            ))
                        })?
                        .ok_or_else(|| {
                            agent_conflict(format!(
                                "Dash source {} metadata was not found",
                                source.as_str()
                            ))
                        })?;
                    if current_repository != *expected_repository
                        || current_metadata != *expected_metadata
                    {
                        return Err(agent_conflict(format!(
                            "Dash source {} state changed",
                            source.as_str()
                        )));
                    }
                    replace_repository_state(
                        &mut tx,
                        source.as_str(),
                        revision,
                        &expected_repository,
                        &replacement_repository,
                    )
                    .await
                    .map_err(agent_from_dash_error)?;
                    sqlx::query(
                        "UPDATE dash_complete_source \
                         SET repository_revision=$2,metadata=$3 WHERE source_coordinate=$1",
                    )
                    .bind(source.as_str())
                    .bind(
                        u64_to_i64(revision.checked_add(1).ok_or_else(|| {
                            agent_internal_error("Dash repository revision is exhausted")
                        })?)
                        .map_err(|error| agent_internal_error(error.to_string()))?,
                    )
                    .bind(to_json(&*replacement_metadata)?)
                    .execute(&mut *tx)
                    .await
                    .map_err(agent_database_error)?;
                }
            }
        }

        let record = to_json(&commit.replacement_effect)?;
        let receipt = to_json(&commit.replacement_effect.receipt)?;
        let inspection = to_json(&commit.replacement_effect.inspection)?;
        match commit.expected_effect {
            None => {
                sqlx::query(
                    "INSERT INTO dash_complete_effect \
                     (effect_id,request_fingerprint,receipt,inspection,record) \
                     VALUES ($1,$2,$3,$4,$5)",
                )
                .bind(commit.effect_id.as_str())
                .bind(&commit.replacement_effect.request_fingerprint)
                .bind(receipt)
                .bind(inspection)
                .bind(record)
                .execute(&mut *tx)
                .await
                .map_err(agent_database_error)?;
            }
            Some(_) => {
                sqlx::query(
                    "UPDATE dash_complete_effect SET \
                     request_fingerprint=$2,receipt=$3,inspection=$4,record=$5 WHERE effect_id=$1",
                )
                .bind(commit.effect_id.as_str())
                .bind(&commit.replacement_effect.request_fingerprint)
                .bind(receipt)
                .bind(inspection)
                .bind(record)
                .execute(&mut *tx)
                .await
                .map_err(agent_database_error)?;
            }
        }
        tx.commit().await.map_err(agent_database_error)
    }
}

async fn lock_complete_identity(
    tx: &mut Transaction<'_, Postgres>,
    namespace: i32,
    identity: &str,
) -> Result<(), AgentServiceError> {
    sqlx::query("SELECT pg_advisory_xact_lock($1, hashtext($2))")
        .bind(namespace)
        .bind(identity)
        .execute(&mut **tx)
        .await
        .map_err(agent_database_error)?;
    Ok(())
}

fn mutation_source(mutation: &DashCompleteSourceMutation) -> &AgentSourceCoordinate {
    match mutation {
        DashCompleteSourceMutation::Create { source, .. }
        | DashCompleteSourceMutation::CompareAndSwap { source, .. } => source,
    }
}

async fn lock_complete_effect(
    tx: &mut Transaction<'_, Postgres>,
    identity: &AgentEffectIdentity,
) -> Result<Option<DashCompleteEffectRecord>, AgentServiceError> {
    let row = sqlx::query(
        "SELECT request_fingerprint,receipt,inspection,record \
         FROM dash_complete_effect WHERE effect_id=$1 FOR UPDATE",
    )
    .bind(identity.as_str())
    .fetch_optional(&mut **tx)
    .await
    .map_err(agent_database_error)?;
    row.as_ref()
        .map(|row| decode_complete_effect_row(row, identity))
        .transpose()
}

fn decode_complete_effect_row(
    row: &sqlx::postgres::PgRow,
    identity: &AgentEffectIdentity,
) -> Result<DashCompleteEffectRecord, AgentServiceError> {
    let record: DashCompleteEffectRecord = serde_json::from_value(
        row.try_get("record").map_err(agent_database_error)?,
    )
    .map_err(|error| agent_internal_error(format!("decode Dash Complete Agent effect: {error}")))?;
    let request_fingerprint: String = row
        .try_get("request_fingerprint")
        .map_err(agent_database_error)?;
    let receipt: Value = row.try_get("receipt").map_err(agent_database_error)?;
    let inspection: Value = row.try_get("inspection").map_err(agent_database_error)?;
    if request_fingerprint != record.request_fingerprint
        || receipt != to_json(&record.receipt)?
        || inspection != to_json(&record.inspection)?
    {
        return Err(agent_internal_error(format!(
            "Dash Complete Agent effect {} scalar projection does not match its record",
            identity.as_str()
        )));
    }
    Ok(record)
}

async fn lock_repository_state(
    tx: &mut Transaction<'_, Postgres>,
    source: &str,
) -> Result<Option<(u64, DashAgentRepositoryState)>, DashServiceError> {
    let row = sqlx::query(
        "SELECT repository_revision,branch_id,head_revision,head_entry_id,history_digest,repository \
         FROM dash_agent_session \
         WHERE source_coordinate=$1 FOR UPDATE",
    )
    .bind(source)
    .fetch_optional(&mut **tx)
    .await
    .map_err(dash_database_error)?;
    let Some(row) = row else {
        return Ok(None);
    };
    let revision = i64_to_u64(
        row.try_get::<i64, _>("repository_revision")
            .map_err(dash_database_error)?,
    )?;
    let value: Value = row.try_get("repository").map_err(dash_database_error)?;
    let state: DashAgentRepositoryState =
        serde_json::from_value(value).map_err(|error| DashServiceError::Internal {
            message: format!("decode Dash repository state: {error}"),
        })?;
    verify_repository_projection(tx, source, revision, &state, &row).await?;
    Ok(Some((revision, state)))
}

async fn verify_repository_projection(
    tx: &mut Transaction<'_, Postgres>,
    source: &str,
    revision: u64,
    state: &DashAgentRepositoryState,
    session: &sqlx::postgres::PgRow,
) -> Result<(), DashServiceError> {
    let history = state.history();
    let head_revision = history.entries().last().map_or(0, |entry| entry.sequence);
    let branch_id: String = session.try_get("branch_id").map_err(dash_database_error)?;
    let stored_head_revision = i64_to_u64(
        session
            .try_get("head_revision")
            .map_err(dash_database_error)?,
    )?;
    let stored_head_entry_id: Option<String> = session
        .try_get("head_entry_id")
        .map_err(dash_database_error)?;
    let stored_history_digest: String = session
        .try_get("history_digest")
        .map_err(dash_database_error)?;
    if branch_id != history.branch_id.0
        || stored_head_revision != head_revision
        || stored_head_entry_id.as_deref() != history.head().map(|head| head.0.as_str())
        || stored_history_digest != history.digest()
    {
        return Err(repository_projection_error(
            source,
            "session head or digest",
        ));
    }

    let branch: Option<Value> = sqlx::query_scalar(
        "SELECT branch FROM dash_agent_branch \
         WHERE source_coordinate=$1 AND branch_id=$2",
    )
    .bind(source)
    .bind(&history.branch_id.0)
    .fetch_optional(&mut **tx)
    .await
    .map_err(dash_database_error)?;
    if branch != Some(serde_json::to_value(history).map_err(dash_json_error)?) {
        return Err(repository_projection_error(source, "branch"));
    }

    let stored_history = sqlx::query_scalar::<_, Value>(
        "SELECT entry FROM dash_agent_history \
         WHERE source_coordinate=$1 AND branch_id=$2 ORDER BY ordinal",
    )
    .bind(source)
    .bind(&history.branch_id.0)
    .fetch_all(&mut **tx)
    .await
    .map_err(dash_database_error)?;
    let expected_history = history
        .entries()
        .iter()
        .map(|entry| serde_json::to_value(entry).map_err(dash_json_error))
        .collect::<Result<Vec<_>, _>>()?;
    if stored_history != expected_history {
        return Err(repository_projection_error(source, "history"));
    }

    let document = dash_state_json(state)?;
    let stored_commands = load_keyed_projection(
        tx,
        source,
        "SELECT command_id,command FROM dash_agent_command \
         WHERE source_coordinate=$1 ORDER BY command_id",
    )
    .await?;
    let expected_commands = lifecycle_map(&document, "commands")
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .collect::<BTreeMap<_, _>>();
    if stored_commands != expected_commands {
        return Err(repository_projection_error(source, "commands"));
    }

    let stored_effects = load_keyed_projection(
        tx,
        source,
        "SELECT effect_id,effect FROM dash_agent_effect \
         WHERE source_coordinate=$1 ORDER BY effect_id",
    )
    .await?;
    if stored_effects != effect_projection(&document) {
        return Err(repository_projection_error(source, "effects"));
    }

    let stored_changes = sqlx::query_scalar::<_, Value>(
        "SELECT change FROM dash_agent_change \
         WHERE source_coordinate=$1 ORDER BY revision,ordinal",
    )
    .bind(source)
    .fetch_all(&mut **tx)
    .await
    .map_err(dash_database_error)?;
    if stored_changes.as_slice() != repository_changes(&document)? {
        return Err(repository_projection_error(source, "changes"));
    }

    let complete_revision: Option<i64> = sqlx::query_scalar(
        "SELECT repository_revision FROM dash_complete_source WHERE source_coordinate=$1",
    )
    .bind(source)
    .fetch_optional(&mut **tx)
    .await
    .map_err(dash_database_error)?;
    if complete_revision
        .map(i64_to_u64)
        .transpose()?
        .is_some_and(|complete_revision| complete_revision != revision)
    {
        return Err(repository_projection_error(
            source,
            "Complete Agent source revision",
        ));
    }
    Ok(())
}

async fn load_keyed_projection(
    tx: &mut Transaction<'_, Postgres>,
    source: &str,
    query: &str,
) -> Result<BTreeMap<String, Value>, DashServiceError> {
    sqlx::query(query)
        .bind(source)
        .fetch_all(&mut **tx)
        .await
        .map_err(dash_database_error)?
        .into_iter()
        .map(|row| {
            Ok((
                row.try_get::<String, _>(0).map_err(dash_database_error)?,
                row.try_get::<Value, _>(1).map_err(dash_database_error)?,
            ))
        })
        .collect()
}

fn repository_projection_error(source: &str, projection: &str) -> DashServiceError {
    DashServiceError::InvalidState {
        message: format!(
            "Dash source {source} normalized {projection} does not match its repository aggregate"
        ),
    }
}

async fn insert_repository_state(
    tx: &mut Transaction<'_, Postgres>,
    source: &str,
    revision: u64,
    state: &DashAgentRepositoryState,
) -> Result<(), DashServiceError> {
    let history = state.history();
    validate_repository_identity(source, state)?;
    let document = dash_state_json(state)?;
    validate_change_sequence(history.entries().len(), repository_changes(&document)?)?;
    let head_revision = history.entries().last().map_or(0, |entry| entry.sequence);
    sqlx::query(
        "INSERT INTO dash_agent_session \
         (source_coordinate,repository_revision,branch_id,head_revision,head_entry_id,history_digest,repository) \
         VALUES ($1,$2,$3,$4,$5,$6,$7)",
    )
    .bind(source)
    .bind(u64_to_i64(revision)?)
    .bind(&history.branch_id.0)
    .bind(u64_to_i64(head_revision)?)
    .bind(history.head().map(|head| head.0.as_str()))
    .bind(history.digest())
    .bind(dash_state_json(state)?)
    .execute(&mut **tx)
    .await
    .map_err(dash_database_error)?;
    insert_repository_children(tx, source, state).await
}

async fn replace_repository_state(
    tx: &mut Transaction<'_, Postgres>,
    source: &str,
    current_revision: u64,
    expected: &DashAgentRepositoryState,
    replacement: &DashAgentRepositoryState,
) -> Result<(), DashServiceError> {
    validate_append_only_replacement(source, expected, replacement)?;
    let history = replacement.history();
    let next_revision =
        current_revision
            .checked_add(1)
            .ok_or_else(|| DashServiceError::Internal {
                message: "Dash repository revision is exhausted".to_owned(),
            })?;
    let head_revision = history.entries().last().map_or(0, |entry| entry.sequence);
    let result = sqlx::query(
        "UPDATE dash_agent_session SET \
         repository_revision=$3,branch_id=$4,head_revision=$5,head_entry_id=$6, \
         history_digest=$7,repository=$8 \
         WHERE source_coordinate=$1 AND repository_revision=$2",
    )
    .bind(source)
    .bind(u64_to_i64(current_revision)?)
    .bind(u64_to_i64(next_revision)?)
    .bind(&history.branch_id.0)
    .bind(u64_to_i64(head_revision)?)
    .bind(history.head().map(|head| head.0.as_str()))
    .bind(history.digest())
    .bind(dash_state_json(replacement)?)
    .execute(&mut **tx)
    .await
    .map_err(dash_database_error)?;
    if result.rows_affected() != 1 {
        return Err(DashServiceError::Conflict {
            message: format!("Dash source {source} repository revision changed"),
        });
    }
    sqlx::query(
        "UPDATE dash_complete_source SET repository_revision=$2 WHERE source_coordinate=$1",
    )
    .bind(source)
    .bind(u64_to_i64(next_revision)?)
    .execute(&mut **tx)
    .await
    .map_err(dash_database_error)?;
    append_repository_children(tx, source, expected, replacement).await
}

async fn insert_repository_children(
    tx: &mut Transaction<'_, Postgres>,
    source: &str,
    state: &DashAgentRepositoryState,
) -> Result<(), DashServiceError> {
    let history = state.history();
    let head_revision = history.entries().last().map_or(0, |entry| entry.sequence);
    sqlx::query(
        "INSERT INTO dash_agent_branch \
         (source_coordinate,branch_id,parent_source_coordinate,parent_branch_id,source_head, \
          source_digest,fork_cutoff,head_revision,head_entry_id,branch) \
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)",
    )
    .bind(source)
    .bind(&history.branch_id.0)
    .bind(
        history
            .lineage
            .as_ref()
            .map(|lineage| lineage.parent_session_id.0.as_str()),
    )
    .bind(
        history
            .lineage
            .as_ref()
            .map(|lineage| lineage.parent_branch_id.0.as_str()),
    )
    .bind(
        history
            .lineage
            .as_ref()
            .and_then(|lineage| lineage.source_head.as_ref())
            .map(|head| head.0.as_str()),
    )
    .bind(
        history
            .lineage
            .as_ref()
            .map(|lineage| lineage.source_digest.as_str()),
    )
    .bind(
        history
            .lineage
            .as_ref()
            .map(|lineage| serde_json::to_value(&lineage.cutoff))
            .transpose()
            .map_err(dash_json_error)?,
    )
    .bind(u64_to_i64(head_revision)?)
    .bind(history.head().map(|head| head.0.as_str()))
    .bind(serde_json::to_value(history).map_err(dash_json_error)?)
    .execute(&mut **tx)
    .await
    .map_err(dash_database_error)?;

    for entry in history.entries() {
        insert_history_entry(tx, source, &history.branch_id.0, entry).await?;
    }

    upsert_command_projection(tx, source, &document).await?;
    upsert_effect_projection(tx, source, &document).await?;
    for change in repository_changes(&document)? {
        insert_change(tx, source, change).await?;
    }
    Ok(())
}

async fn append_repository_children(
    tx: &mut Transaction<'_, Postgres>,
    source: &str,
    expected: &DashAgentRepositoryState,
    replacement: &DashAgentRepositoryState,
) -> Result<(), DashServiceError> {
    let history = replacement.history();
    let expected_history_len = expected.history().entries().len();
    let result = sqlx::query(
        "UPDATE dash_agent_branch SET \
         head_revision=$3,head_entry_id=$4,branch=$5 \
         WHERE source_coordinate=$1 AND branch_id=$2",
    )
    .bind(source)
    .bind(&history.branch_id.0)
    .bind(u64_to_i64(
        history.entries().last().map_or(0, |entry| entry.sequence),
    )?)
    .bind(history.head().map(|head| head.0.as_str()))
    .bind(serde_json::to_value(history).map_err(dash_json_error)?)
    .execute(&mut **tx)
    .await
    .map_err(dash_database_error)?;
    if result.rows_affected() != 1 {
        return Err(DashServiceError::Conflict {
            message: format!("Dash source {source} branch changed"),
        });
    }

    for entry in &history.entries()[expected_history_len..] {
        insert_history_entry(tx, source, &history.branch_id.0, entry).await?;
    }

    let expected_document = dash_state_json(expected)?;
    let replacement_document = dash_state_json(replacement)?;
    upsert_command_projection(tx, source, &replacement_document).await?;
    upsert_effect_projection(tx, source, &replacement_document).await?;
    let expected_changes_len = repository_changes(&expected_document)?.len();
    for change in &repository_changes(&replacement_document)?[expected_changes_len..] {
        insert_change(tx, source, change).await?;
    }
    Ok(())
}

async fn insert_history_entry(
    tx: &mut Transaction<'_, Postgres>,
    source: &str,
    branch_id: &str,
    entry: &AgentHistoryEntry,
) -> Result<(), DashServiceError> {
    sqlx::query(
        "INSERT INTO dash_agent_history \
         (source_coordinate,branch_id,ordinal,entry_id,parent_entry_id,entry) \
         VALUES ($1,$2,$3,$4,$5,$6)",
    )
    .bind(source)
    .bind(branch_id)
    .bind(u64_to_i64(entry.sequence)?)
    .bind(&entry.entry_id.0)
    .bind(
        entry
            .parent_entry_id
            .as_ref()
            .map(|parent| parent.0.as_str()),
    )
    .bind(serde_json::to_value(entry).map_err(dash_json_error)?)
    .execute(&mut **tx)
    .await
    .map_err(dash_database_error)?;
    Ok(())
}

async fn insert_change(
    tx: &mut Transaction<'_, Postgres>,
    source: &str,
    change: &Value,
) -> Result<(), DashServiceError> {
    let revision = change_revision(change)?;
    let ordinal = change_ordinal(change)?;
    sqlx::query(
        "INSERT INTO dash_agent_change(source_coordinate,revision,ordinal,change) \
         VALUES ($1,$2,$3,$4)",
    )
    .bind(source)
    .bind(u64_to_i64(revision)?)
    .bind(u64_to_i64(ordinal)?)
    .bind(change)
    .execute(&mut **tx)
    .await
    .map_err(dash_database_error)?;
    Ok(())
}

async fn upsert_command_projection(
    tx: &mut Transaction<'_, Postgres>,
    source: &str,
    document: &Value,
) -> Result<(), DashServiceError> {
    if let Some(commands) = lifecycle_map(document, "commands") {
        for (command_id, command) in commands {
            sqlx::query(
                "INSERT INTO dash_agent_command(source_coordinate,command_id,command) \
                 VALUES ($1,$2,$3) \
                 ON CONFLICT (source_coordinate,command_id) DO UPDATE SET command=EXCLUDED.command",
            )
            .bind(source)
            .bind(command_id)
            .bind(command)
            .execute(&mut **tx)
            .await
            .map_err(dash_database_error)?;
        }
    }
    Ok(())
}

async fn upsert_effect_projection(
    tx: &mut Transaction<'_, Postgres>,
    source: &str,
    document: &Value,
) -> Result<(), DashServiceError> {
    for (effect_id, effect) in effect_projection(document) {
        sqlx::query(
            "INSERT INTO dash_agent_effect(source_coordinate,effect_id,effect) \
             VALUES ($1,$2,$3) \
             ON CONFLICT (source_coordinate,effect_id) DO UPDATE SET effect=EXCLUDED.effect",
        )
        .bind(source)
        .bind(effect_id)
        .bind(effect)
        .execute(&mut **tx)
        .await
        .map_err(dash_database_error)?;
    }
    Ok(())
}

fn effect_projection(document: &Value) -> BTreeMap<String, Value> {
    let service = document.pointer("/effects").and_then(Value::as_object);
    let lifecycle = lifecycle_map(document, "effects");
    service
        .into_iter()
        .flat_map(|values| values.keys())
        .chain(lifecycle.into_iter().flat_map(|values| values.keys()))
        .cloned()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .map(|effect_id| {
            let effect = serde_json::json!({
                "service": service.and_then(|values| values.get(&effect_id)),
                "lifecycle": lifecycle.and_then(|values| values.get(&effect_id)),
            });
            (effect_id, effect)
        })
        .collect()
}

fn validate_repository_identity(
    source: &str,
    state: &DashAgentRepositoryState,
) -> Result<(), DashServiceError> {
    if state.history().session_id.0 != source {
        return Err(DashServiceError::InvalidState {
            message: format!(
                "Dash source {source} does not match history session {}",
                state.history().session_id.0
            ),
        });
    }
    Ok(())
}

fn validate_append_only_replacement(
    source: &str,
    expected: &DashAgentRepositoryState,
    replacement: &DashAgentRepositoryState,
) -> Result<(), DashServiceError> {
    validate_repository_identity(source, replacement)?;
    let expected_history = expected.history();
    let replacement_history = replacement.history();
    if expected_history.session_id != replacement_history.session_id
        || expected_history.branch_id != replacement_history.branch_id
        || expected_history.lineage != replacement_history.lineage
        || !replacement_history
            .entries()
            .starts_with(expected_history.entries())
    {
        return Err(DashServiceError::InvalidState {
            message: format!("Dash source {source} attempted to rewrite immutable history"),
        });
    }

    let expected_document = dash_state_json(expected)?;
    let replacement_document = dash_state_json(replacement)?;
    let expected_changes = repository_changes(&expected_document)?;
    let replacement_changes = repository_changes(&replacement_document)?;
    if !replacement_changes.starts_with(expected_changes) {
        return Err(DashServiceError::InvalidState {
            message: format!("Dash source {source} attempted to rewrite durable changes"),
        });
    }
    validate_change_sequence(replacement_history.entries().len(), replacement_changes)?;
    validate_projection_keys(
        source,
        &expected_document,
        &replacement_document,
        "commands",
    )?;
    validate_projection_keys(source, &expected_document, &replacement_document, "effects")?;
    validate_service_effect_keys(source, &expected_document, &replacement_document)?;
    Ok(())
}

fn validate_projection_keys(
    source: &str,
    expected: &Value,
    replacement: &Value,
    field: &str,
) -> Result<(), DashServiceError> {
    let expected = lifecycle_map(expected, field);
    let replacement = lifecycle_map(replacement, field);
    if expected.is_some_and(|expected| {
        !expected
            .keys()
            .all(|key| replacement.is_some_and(|replacement| replacement.contains_key(key)))
    }) {
        return Err(DashServiceError::InvalidState {
            message: format!("Dash source {source} attempted to remove lifecycle {field}"),
        });
    }
    Ok(())
}

fn validate_service_effect_keys(
    source: &str,
    expected: &Value,
    replacement: &Value,
) -> Result<(), DashServiceError> {
    let expected = expected.pointer("/effects").and_then(Value::as_object);
    let replacement = replacement.pointer("/effects").and_then(Value::as_object);
    if expected.is_some_and(|expected| {
        !expected
            .keys()
            .all(|key| replacement.is_some_and(|replacement| replacement.contains_key(key)))
    }) {
        return Err(DashServiceError::InvalidState {
            message: format!("Dash source {source} attempted to remove service effects"),
        });
    }
    Ok(())
}

fn validate_change_sequence(history_len: usize, changes: &[Value]) -> Result<(), DashServiceError> {
    let Some(first) = changes.first() else {
        return Ok(());
    };
    let first_revision = change_revision(first)?;
    if first_revision == 0 || change_ordinal(first)? != 0 {
        return Err(DashServiceError::InvalidState {
            message: "Dash changes must start at a positive revision with ordinal zero".to_owned(),
        });
    }
    let mut next = (first_revision, 0_u64);
    for change in changes {
        let actual = (change_revision(change)?, change_ordinal(change)?);
        if actual != next {
            return Err(DashServiceError::InvalidState {
                message: format!(
                    "Dash change sequence is not continuous: expected {}:{}, found {}:{}",
                    next.0, next.1, actual.0, actual.1
                ),
            });
        }
        next = if actual.1 == 0
            && changes.iter().any(|candidate| {
                change_revision(candidate) == Ok(actual.0) && change_ordinal(candidate) == Ok(1)
            }) {
            (actual.0, 1)
        } else {
            (
                actual
                    .0
                    .checked_add(1)
                    .ok_or_else(|| DashServiceError::Internal {
                        message: "Dash change revision is exhausted".to_owned(),
                    })?,
                0,
            )
        };
    }
    if history_len > 0
        && changes.last().map(change_revision).transpose()? != Some(history_len as u64)
    {
        return Err(DashServiceError::InvalidState {
            message: "Dash changes do not cover the history head".to_owned(),
        });
    }
    Ok(())
}

fn lifecycle_map<'a>(
    document: &'a Value,
    field: &str,
) -> Option<&'a serde_json::Map<String, Value>> {
    document
        .pointer(&format!("/store/lifecycle/{field}"))
        .and_then(Value::as_object)
}

fn repository_changes(document: &Value) -> Result<&[Value], DashServiceError> {
    document
        .pointer("/store/changes")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .ok_or_else(|| DashServiceError::Internal {
            message: "Dash repository changes are missing".to_owned(),
        })
}

fn change_revision(change: &Value) -> Result<u64, DashServiceError> {
    change
        .pointer("/cursor/revision")
        .and_then(Value::as_u64)
        .ok_or_else(|| DashServiceError::Internal {
            message: "Dash change revision is missing".to_owned(),
        })
}

fn change_ordinal(change: &Value) -> Result<u64, DashServiceError> {
    change
        .pointer("/cursor/ordinal")
        .and_then(Value::as_u64)
        .ok_or_else(|| DashServiceError::Internal {
            message: "Dash change ordinal is missing".to_owned(),
        })
}

fn dash_state_json(state: &DashAgentRepositoryState) -> Result<Value, DashServiceError> {
    serde_json::to_value(state).map_err(dash_json_error)
}

fn dash_json_error(error: serde_json::Error) -> DashServiceError {
    DashServiceError::Internal {
        message: format!("encode Dash repository state: {error}"),
    }
}

fn dash_database_error(error: sqlx::Error) -> DashServiceError {
    if is_constraint_error(&error) {
        DashServiceError::Conflict {
            message: error.to_string(),
        }
    } else {
        DashServiceError::Unavailable {
            message: error.to_string(),
            retryable: true,
        }
    }
}

fn agent_database_error(error: sqlx::Error) -> AgentServiceError {
    if is_constraint_error(&error) {
        agent_conflict(error.to_string())
    } else {
        AgentServiceError::new(AgentServiceErrorCode::Unavailable, error.to_string(), true)
    }
}

fn agent_from_dash_error(error: DashServiceError) -> AgentServiceError {
    match error {
        DashServiceError::Conflict { message } => agent_conflict(message),
        other => AgentServiceError::new(
            AgentServiceErrorCode::Internal,
            other.to_string(),
            other.retryable(),
        ),
    }
}

fn agent_conflict(message: impl Into<String>) -> AgentServiceError {
    AgentServiceError::new(AgentServiceErrorCode::Conflict, message, false)
}

fn agent_internal_error(message: impl Into<String>) -> AgentServiceError {
    AgentServiceError::new(AgentServiceErrorCode::Internal, message, false)
}

fn to_json<T: serde::Serialize>(value: &T) -> Result<Value, AgentServiceError> {
    serde_json::to_value(value)
        .map_err(|error| agent_internal_error(format!("encode Dash Complete Agent state: {error}")))
}

fn is_constraint_error(error: &sqlx::Error) -> bool {
    matches!(
        error,
        sqlx::Error::Database(database)
            if matches!(
                database.code().as_deref(),
                Some("23505" | "23503" | "23514" | "23P01")
            )
    )
}

fn u64_to_i64(value: u64) -> Result<i64, DashServiceError> {
    i64::try_from(value).map_err(|_| DashServiceError::Internal {
        message: format!("Dash durable revision {value} exceeds PostgreSQL BIGINT"),
    })
}

fn i64_to_u64(value: i64) -> Result<u64, DashServiceError> {
    u64::try_from(value).map_err(|_| DashServiceError::Internal {
        message: format!("Dash durable revision {value} is negative"),
    })
}
