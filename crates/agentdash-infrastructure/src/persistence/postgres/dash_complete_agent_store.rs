use std::sync::Arc;

use agentdash_agent::dash::{
    AgentSessionId, DashAgentChange, DashAgentRepository, DashAgentRepositoryState,
    DashAgentRepositoryStore, DashServiceError,
};
use agentdash_agent_service_api::{
    AgentEffectIdentity, AgentObservation, AgentServiceError, AgentServiceErrorCode,
    AgentSourceCoordinate,
};
use agentdash_integration_native_agent::{
    DashCompleteAgentStore, DashCompleteAtomicCommit, DashCompleteEffectRecord,
    DashCompleteSourceMetadata, DashCompleteSourceMutation, dash_complete_agent_observation,
};
use async_trait::async_trait;
use serde_json::Value;
use sqlx::{PgPool, Postgres, Row, Transaction};

#[derive(Debug, Clone, PartialEq)]
struct DashCompleteSourceDocument {
    repository: DashAgentRepositoryState,
    metadata: DashCompleteSourceMetadata,
}

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
        insert_source_document(
            &mut tx,
            &source.0,
            &DashCompleteSourceDocument {
                repository: initial,
                metadata: empty_source_metadata(),
            },
        )
        .await?;
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
            "SELECT EXISTS(SELECT 1 FROM dash_complete_source WHERE source_coordinate=$1)",
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
        insert_source_document(
            &mut tx,
            &self.source.0,
            &DashCompleteSourceDocument {
                repository: initial,
                metadata: empty_source_metadata(),
            },
        )
        .await?;
        tx.commit().await.map_err(dash_database_error)
    }

    async fn load(&self) -> Result<DashAgentRepositoryState, DashServiceError> {
        load_source_repository(&self.pool, &self.source.0)
            .await?
            .ok_or_else(|| DashServiceError::InvalidState {
                message: format!("Dash source {} was not found", self.source.0),
            })
    }

    async fn compare_and_swap(
        &self,
        expected: DashAgentRepositoryState,
        replacement: DashAgentRepositoryState,
    ) -> Result<(), DashServiceError> {
        let mut tx = self.pool.begin().await.map_err(dash_database_error)?;
        let current = lock_source_document(&mut tx, &self.source.0)
            .await?
            .ok_or_else(|| DashServiceError::InvalidState {
                message: format!("Dash source {} was not found", self.source.0),
            })?;
        if current.repository != expected {
            return Err(DashServiceError::Conflict {
                message: format!("Dash source {} repository state changed", self.source.0),
            });
        }
        validate_append_only_replacement(&self.source.0, &expected, &replacement)?;
        replace_source_document(
            &mut tx,
            &self.source.0,
            &DashCompleteSourceDocument {
                repository: replacement,
                metadata: current.metadata,
            },
        )
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
        load_source_metadata(&self.pool, source.as_str())
            .await
            .map_err(agent_from_dash_error)
    }

    async fn load_observation(
        &self,
        source: &AgentSourceCoordinate,
    ) -> Result<Option<AgentObservation>, AgentServiceError> {
        let row =
            sqlx::query("SELECT observation FROM dash_complete_source WHERE source_coordinate=$1")
                .bind(source.as_str())
                .fetch_optional(&self.pool)
                .await
                .map_err(agent_database_error)?;
        let observation = row
            .as_ref()
            .map(|row| {
                serde_json::from_value::<AgentObservation>(
                    row.try_get("observation").map_err(agent_database_error)?,
                )
                .map_err(|error| {
                    agent_internal_error(format!("decode Dash source observation: {error}"))
                })
            })
            .transpose()?;
        if observation
            .as_ref()
            .is_some_and(|observation| observation.source != *source)
        {
            return Err(agent_internal_error(format!(
                "Dash source {} observation has a different identity",
                source.as_str()
            )));
        }
        Ok(observation)
    }

    async fn load_effect(
        &self,
        identity: &AgentEffectIdentity,
    ) -> Result<Option<DashCompleteEffectRecord>, AgentServiceError> {
        let row = sqlx::query("SELECT record FROM dash_complete_effect WHERE effect_id=$1")
            .bind(identity.as_str())
            .fetch_optional(&self.pool)
            .await
            .map_err(agent_database_error)?;
        row.as_ref()
            .map(|row| decode_complete_effect_row(row, identity))
            .transpose()
    }

    async fn commit(&self, commit: DashCompleteAtomicCommit) -> Result<(), AgentServiceError> {
        let mut tx = self.pool.begin().await.map_err(agent_database_error)?;
        lock_identity(&mut tx, 4_401, commit.effect_id.as_str()).await?;
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
            lock_identity(&mut tx, 4_402, source).await?;
        }

        for mutation in mutations {
            match mutation {
                DashCompleteSourceMutation::Create {
                    source,
                    repository,
                    metadata,
                } => {
                    if lock_source_document(&mut tx, source.as_str())
                        .await
                        .map_err(agent_from_dash_error)?
                        .is_some()
                    {
                        return Err(agent_conflict(format!(
                            "Dash source {} already exists",
                            source.as_str()
                        )));
                    }
                    insert_source_document(
                        &mut tx,
                        source.as_str(),
                        &DashCompleteSourceDocument {
                            repository: *repository,
                            metadata: *metadata,
                        },
                    )
                    .await
                    .map_err(agent_from_dash_error)?;
                }
                DashCompleteSourceMutation::CompareAndSwap {
                    source,
                    expected_repository,
                    replacement_repository,
                    expected_metadata,
                    replacement_metadata,
                } => {
                    let current = lock_source_document(&mut tx, source.as_str())
                        .await
                        .map_err(agent_from_dash_error)?
                        .ok_or_else(|| {
                            agent_conflict(format!("Dash source {} was not found", source.as_str()))
                        })?;
                    if current.repository != *expected_repository
                        || current.metadata != *expected_metadata
                    {
                        return Err(agent_conflict(format!(
                            "Dash source {} state changed",
                            source.as_str()
                        )));
                    }
                    validate_append_only_replacement(
                        source.as_str(),
                        &expected_repository,
                        &replacement_repository,
                    )
                    .map_err(agent_from_dash_error)?;
                    replace_source_document(
                        &mut tx,
                        source.as_str(),
                        &DashCompleteSourceDocument {
                            repository: *replacement_repository,
                            metadata: *replacement_metadata,
                        },
                    )
                    .await
                    .map_err(agent_from_dash_error)?;
                }
            }
        }

        let record = to_json(&commit.replacement_effect)?;
        match commit.expected_effect {
            None => {
                sqlx::query("INSERT INTO dash_complete_effect(effect_id,record) VALUES ($1,$2)")
                    .bind(commit.effect_id.as_str())
                    .bind(record)
                    .execute(&mut *tx)
                    .await
                    .map_err(agent_database_error)?;
            }
            Some(_) => {
                sqlx::query("UPDATE dash_complete_effect SET record=$2 WHERE effect_id=$1")
                    .bind(commit.effect_id.as_str())
                    .bind(record)
                    .execute(&mut *tx)
                    .await
                    .map_err(agent_database_error)?;
            }
        }
        tx.commit().await.map_err(agent_database_error)
    }
}

async fn lock_identity(
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
    let row = sqlx::query("SELECT record FROM dash_complete_effect WHERE effect_id=$1 FOR UPDATE")
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
    if record.inspection.effect_id != *identity {
        return Err(agent_internal_error(format!(
            "Dash Complete Agent effect {} record has a different identity",
            identity.as_str()
        )));
    }
    Ok(record)
}

async fn lock_source_document(
    tx: &mut Transaction<'_, Postgres>,
    source: &str,
) -> Result<Option<DashCompleteSourceDocument>, DashServiceError> {
    let row = sqlx::query(
        "SELECT repository, metadata FROM dash_complete_source \
         WHERE source_coordinate=$1 FOR UPDATE",
    )
    .bind(source)
    .fetch_optional(&mut **tx)
    .await
    .map_err(dash_database_error)?;
    let Some(row) = row else {
        return Ok(None);
    };
    let document = decode_source_document(&row)?;
    validate_repository_identity(source, &document.repository)?;
    validate_repository_document(&document.repository)?;
    Ok(Some(document))
}

async fn load_source_repository(
    pool: &PgPool,
    source: &str,
) -> Result<Option<DashAgentRepositoryState>, DashServiceError> {
    let row = sqlx::query("SELECT repository FROM dash_complete_source WHERE source_coordinate=$1")
        .bind(source)
        .fetch_optional(pool)
        .await
        .map_err(dash_database_error)?;
    let Some(row) = row else {
        return Ok(None);
    };
    let repository = decode_json_column(&row, "repository", "Dash repository")?;
    validate_repository_identity(source, &repository)?;
    validate_repository_document(&repository)?;
    Ok(Some(repository))
}

async fn load_source_metadata(
    pool: &PgPool,
    source: &str,
) -> Result<Option<DashCompleteSourceMetadata>, DashServiceError> {
    let row = sqlx::query("SELECT metadata FROM dash_complete_source WHERE source_coordinate=$1")
        .bind(source)
        .fetch_optional(pool)
        .await
        .map_err(dash_database_error)?;
    row.as_ref()
        .map(|row| decode_json_column(row, "metadata", "Dash source metadata"))
        .transpose()
}

fn decode_source_document(
    row: &sqlx::postgres::PgRow,
) -> Result<DashCompleteSourceDocument, DashServiceError> {
    Ok(DashCompleteSourceDocument {
        repository: decode_json_column(row, "repository", "Dash repository")?,
        metadata: decode_json_column(row, "metadata", "Dash source metadata")?,
    })
}

fn decode_json_column<T: serde::de::DeserializeOwned>(
    row: &sqlx::postgres::PgRow,
    column: &str,
    label: &str,
) -> Result<T, DashServiceError> {
    serde_json::from_value(row.try_get(column).map_err(dash_database_error)?).map_err(|error| {
        DashServiceError::Internal {
            message: format!("decode {label}: {error}"),
        }
    })
}

async fn insert_source_document(
    tx: &mut Transaction<'_, Postgres>,
    source: &str,
    document: &DashCompleteSourceDocument,
) -> Result<(), DashServiceError> {
    validate_repository_identity(source, &document.repository)?;
    validate_repository_document(&document.repository)?;
    sqlx::query(
        "INSERT INTO dash_complete_source(source_coordinate,repository,metadata,observation) \
         VALUES ($1,$2,$3,$4)",
    )
    .bind(source)
    .bind(dash_json(&document.repository, "Dash repository")?)
    .bind(dash_json(&document.metadata, "Dash source metadata")?)
    .bind(source_observation_json(source, &document.repository)?)
    .execute(&mut **tx)
    .await
    .map_err(dash_database_error)?;
    Ok(())
}

async fn replace_source_document(
    tx: &mut Transaction<'_, Postgres>,
    source: &str,
    replacement: &DashCompleteSourceDocument,
) -> Result<(), DashServiceError> {
    validate_repository_identity(source, &replacement.repository)?;
    validate_repository_document(&replacement.repository)?;
    let result = sqlx::query(
        "UPDATE dash_complete_source SET repository=$2, metadata=$3, observation=$4 \
         WHERE source_coordinate=$1",
    )
    .bind(source)
    .bind(dash_json(&replacement.repository, "Dash repository")?)
    .bind(dash_json(&replacement.metadata, "Dash source metadata")?)
    .bind(source_observation_json(source, &replacement.repository)?)
    .execute(&mut **tx)
    .await
    .map_err(dash_database_error)?;
    if result.rows_affected() != 1 {
        return Err(DashServiceError::Conflict {
            message: format!("Dash source {source} was not found"),
        });
    }
    Ok(())
}

fn empty_source_metadata() -> DashCompleteSourceMetadata {
    DashCompleteSourceMetadata {
        applied_surface: None,
        initial_context: None,
        callback_surface: None,
        callback_binding: None,
    }
}

fn dash_json<T: serde::Serialize>(
    value: &T,
    label: &str,
) -> Result<serde_json::Value, DashServiceError> {
    serde_json::to_value(value).map_err(|error| DashServiceError::Internal {
        message: format!("encode {label}: {error}"),
    })
}

fn source_observation_json(
    source: &str,
    repository: &DashAgentRepositoryState,
) -> Result<Value, DashServiceError> {
    let source =
        AgentSourceCoordinate::new(source).map_err(|error| DashServiceError::InvalidState {
            message: format!("invalid Dash source coordinate: {error}"),
        })?;
    let observation = dash_complete_agent_observation(&source, repository).map_err(|error| {
        DashServiceError::InvalidState {
            message: format!("derive Dash source observation: {error}"),
        }
    })?;
    dash_json(&observation, "Dash source observation")
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

fn validate_repository_document(state: &DashAgentRepositoryState) -> Result<(), DashServiceError> {
    validate_change_sequence(state.history().entries().len(), state.store().changes())
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

    let expected_changes = expected.store().changes();
    let replacement_changes = replacement.store().changes();
    if !replacement_changes.starts_with(expected_changes) {
        return Err(DashServiceError::InvalidState {
            message: format!("Dash source {source} attempted to rewrite durable changes"),
        });
    }
    validate_change_sequence(replacement_history.entries().len(), replacement_changes)?;
    validate_key_retention(
        source,
        expected.store().lifecycle().command_ids(),
        replacement.store().lifecycle().command_ids(),
        "lifecycle commands",
    )?;
    validate_key_retention(
        source,
        expected.store().lifecycle().effect_ids(),
        replacement.store().lifecycle().effect_ids(),
        "lifecycle effects",
    )?;
    validate_key_retention(
        source,
        expected.service_effect_ids(),
        replacement.service_effect_ids(),
        "service effects",
    )
}

fn validate_key_retention<'a, T: 'a + Eq>(
    source: &str,
    mut expected: impl Iterator<Item = &'a T>,
    replacement: impl Iterator<Item = &'a T>,
    label: &str,
) -> Result<(), DashServiceError> {
    let replacement = replacement.collect::<Vec<_>>();
    if expected.any(|key| !replacement.contains(&key)) {
        return Err(DashServiceError::InvalidState {
            message: format!("Dash source {source} attempted to remove {label}"),
        });
    }
    Ok(())
}

fn validate_change_sequence(
    history_len: usize,
    changes: &[DashAgentChange],
) -> Result<(), DashServiceError> {
    let Some(first) = changes.first() else {
        return Ok(());
    };
    if first.cursor.revision == 0 || first.cursor.ordinal != 0 {
        return Err(DashServiceError::InvalidState {
            message: "Dash changes must start at a positive revision with ordinal zero".to_owned(),
        });
    }
    for pair in changes.windows(2) {
        let current = &pair[0].cursor;
        let next = &pair[1].cursor;
        let next_revision = current.revision.checked_add(1);
        let continuous = match current.ordinal {
            0 => {
                (next.revision == current.revision && next.ordinal == 1)
                    || (Some(next.revision) == next_revision && next.ordinal == 0)
            }
            1 => Some(next.revision) == next_revision && next.ordinal == 0,
            _ => false,
        };
        if !continuous {
            return Err(DashServiceError::InvalidState {
                message: format!(
                    "Dash change sequence is not continuous between {}:{} and {}:{}",
                    current.revision, current.ordinal, next.revision, next.ordinal
                ),
            });
        }
    }
    if history_len > 0
        && changes.last().map(|change| change.cursor.revision) != Some(history_len as u64)
    {
        return Err(DashServiceError::InvalidState {
            message: "Dash changes do not cover the history head".to_owned(),
        });
    }
    Ok(())
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
