use agentdash_agent_runtime::{
    ManagedRuntimeStateCommit, ManagedRuntimeStateRepository, ManagedRuntimeStateSnapshot,
    ManagedRuntimeStateStoreError, apply_managed_runtime_state_commit,
    decode_managed_runtime_state_snapshot, encode_managed_runtime_state_snapshot,
};
use agentdash_agent_runtime_contract::RuntimeThreadId;
use agentdash_agent_runtime_host::{
    CompleteAgentCallbackCommit, CompleteAgentCallbackRepository, CompleteAgentCallbackSnapshot,
    CompleteAgentCallbackStoreError, CompleteAgentHostCommit, CompleteAgentHostRepository,
    CompleteAgentHostSnapshot, CompleteAgentHostStoreError, apply_complete_agent_callback_commit,
    apply_complete_agent_host_commit, decode_complete_agent_callback_snapshot,
    decode_complete_agent_host_snapshot, encode_complete_agent_callback_snapshot,
    encode_complete_agent_host_snapshot,
};
use async_trait::async_trait;
use serde_json::{Value, json};
use sqlx::{PgPool, Postgres, Row, Transaction};

#[derive(Clone)]
pub struct PostgresManagedRuntimeStateRepository {
    pool: PgPool,
}

impl PostgresManagedRuntimeStateRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[derive(Clone)]
pub struct PostgresCompleteAgentHostRepository {
    pool: PgPool,
}

impl PostgresCompleteAgentHostRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[derive(Clone)]
pub struct PostgresCompleteAgentCallbackRepository {
    pool: PgPool,
}

impl PostgresCompleteAgentCallbackRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ManagedRuntimeStateRepository for PostgresManagedRuntimeStateRepository {
    async fn load(
        &self,
        thread_id: &RuntimeThreadId,
    ) -> Result<ManagedRuntimeStateSnapshot, ManagedRuntimeStateStoreError> {
        let row = sqlx::query(
            "SELECT revision::TEXT AS revision, facts
             FROM agent_runtime_state_revision WHERE thread_id = $1",
        )
        .bind(thread_id.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(runtime_persistence)?;
        let Some(row) = row else {
            return Ok(ManagedRuntimeStateSnapshot::default());
        };
        runtime_snapshot(
            thread_id,
            row.try_get::<String, _>("revision")
                .map_err(runtime_persistence)?,
            row.try_get("facts").map_err(runtime_persistence)?,
        )
    }

    async fn commit(
        &self,
        commit: ManagedRuntimeStateCommit,
    ) -> Result<ManagedRuntimeStateSnapshot, ManagedRuntimeStateStoreError> {
        let mut tx = self.pool.begin().await.map_err(runtime_persistence)?;
        let thread_id = commit.thread_id.clone();
        let row = sqlx::query(
            "SELECT revision::TEXT AS revision, facts FROM agent_runtime_state_revision
             WHERE thread_id = $1 FOR UPDATE",
        )
        .bind(commit.thread_id.as_str())
        .fetch_optional(&mut *tx)
        .await
        .map_err(runtime_persistence)?;
        let mut current = match row {
            Some(row) => runtime_snapshot(
                &commit.thread_id,
                row.try_get::<String, _>("revision")
                    .map_err(runtime_persistence)?,
                row.try_get("facts").map_err(runtime_persistence)?,
            )?,
            None => ManagedRuntimeStateSnapshot::default(),
        };
        let previous = current.clone();
        let committed = apply_managed_runtime_state_commit(&mut current, commit)?;
        if committed == previous {
            tx.commit().await.map_err(runtime_persistence)?;
            return Ok(committed);
        }
        let facts = encode_managed_runtime_state_snapshot(&committed)?
            .get("facts")
            .cloned()
            .ok_or_else(|| runtime_invariant("encoded Runtime snapshot omitted facts"))?;
        persist_runtime_snapshot(&mut tx, &thread_id, &committed, &facts).await?;
        tx.commit().await.map_err(runtime_persistence)?;
        Ok(committed)
    }
}

#[async_trait]
impl CompleteAgentHostRepository for PostgresCompleteAgentHostRepository {
    async fn load(&self) -> Result<CompleteAgentHostSnapshot, CompleteAgentHostStoreError> {
        let row = sqlx::query(
            "SELECT revision::TEXT AS revision, facts
             FROM agent_runtime_host_revision WHERE singleton = TRUE",
        )
        .fetch_one(&self.pool)
        .await
        .map_err(host_persistence)?;
        host_snapshot(
            row.try_get::<String, _>("revision")
                .map_err(host_persistence)?,
            row.try_get("facts").map_err(host_persistence)?,
        )
    }

    async fn commit(
        &self,
        commit: CompleteAgentHostCommit,
    ) -> Result<CompleteAgentHostSnapshot, CompleteAgentHostStoreError> {
        let mut tx = self.pool.begin().await.map_err(host_persistence)?;
        let row = sqlx::query(
            "SELECT revision::TEXT AS revision, facts FROM agent_runtime_host_revision
             WHERE singleton = TRUE FOR UPDATE",
        )
        .fetch_one(&mut *tx)
        .await
        .map_err(host_persistence)?;
        let mut current = host_snapshot(
            row.try_get::<String, _>("revision")
                .map_err(host_persistence)?,
            row.try_get("facts").map_err(host_persistence)?,
        )?;
        let previous = current.clone();
        let committed = apply_complete_agent_host_commit(&mut current, commit)?;
        if committed == previous {
            tx.commit().await.map_err(host_persistence)?;
            return Ok(committed);
        }
        let facts = encode_complete_agent_host_snapshot(&committed)?
            .get("facts")
            .cloned()
            .ok_or_else(|| host_invariant("encoded Host snapshot omitted facts"))?;
        persist_host_snapshot(&mut tx, &committed, &facts).await?;
        tx.commit().await.map_err(host_persistence)?;
        Ok(committed)
    }
}

#[async_trait]
impl CompleteAgentCallbackRepository for PostgresCompleteAgentCallbackRepository {
    async fn load(&self) -> Result<CompleteAgentCallbackSnapshot, CompleteAgentCallbackStoreError> {
        let row = sqlx::query(
            "SELECT revision::TEXT AS revision, facts
             FROM agent_runtime_callback_revision WHERE singleton = TRUE",
        )
        .fetch_one(&self.pool)
        .await
        .map_err(callback_persistence)?;
        callback_snapshot(
            row.try_get::<String, _>("revision")
                .map_err(callback_persistence)?,
            row.try_get("facts").map_err(callback_persistence)?,
        )
    }

    async fn commit(
        &self,
        commit: CompleteAgentCallbackCommit,
    ) -> Result<CompleteAgentCallbackSnapshot, CompleteAgentCallbackStoreError> {
        let mut tx = self.pool.begin().await.map_err(callback_persistence)?;
        let row = sqlx::query(
            "SELECT revision::TEXT AS revision, facts FROM agent_runtime_callback_revision
             WHERE singleton = TRUE FOR UPDATE",
        )
        .fetch_one(&mut *tx)
        .await
        .map_err(callback_persistence)?;
        let mut current = callback_snapshot(
            row.try_get::<String, _>("revision")
                .map_err(callback_persistence)?,
            row.try_get("facts").map_err(callback_persistence)?,
        )?;
        let previous = current.clone();
        let committed = apply_complete_agent_callback_commit(&mut current, commit)?;
        if committed == previous {
            tx.commit().await.map_err(callback_persistence)?;
            return Ok(committed);
        }
        let facts = encode_complete_agent_callback_snapshot(&committed)?
            .get("facts")
            .cloned()
            .ok_or_else(|| callback_invariant("encoded callback snapshot omitted facts"))?;
        persist_callback_snapshot(&mut tx, &committed, &facts).await?;
        tx.commit().await.map_err(callback_persistence)?;
        Ok(committed)
    }
}

fn runtime_snapshot(
    thread_id: &RuntimeThreadId,
    revision: String,
    facts: Value,
) -> Result<ManagedRuntimeStateSnapshot, ManagedRuntimeStateStoreError> {
    let revision = parse_pg_u64(&revision)
        .map_err(|_| runtime_invariant("Runtime revision is outside the canonical u64 domain"))?;
    decode_managed_runtime_state_snapshot(
        thread_id,
        json!({ "revision": revision, "facts": facts }),
    )
}

fn host_snapshot(
    revision: String,
    facts: Value,
) -> Result<CompleteAgentHostSnapshot, CompleteAgentHostStoreError> {
    let revision = parse_pg_u64(&revision)
        .map_err(|_| host_invariant("Host revision is outside the canonical u64 domain"))?;
    decode_complete_agent_host_snapshot(json!({ "revision": revision, "facts": facts }))
}

fn callback_snapshot(
    revision: String,
    facts: Value,
) -> Result<CompleteAgentCallbackSnapshot, CompleteAgentCallbackStoreError> {
    let revision = parse_pg_u64(&revision)
        .map_err(|_| callback_invariant("callback revision is outside the canonical u64 domain"))?;
    decode_complete_agent_callback_snapshot(json!({ "revision": revision, "facts": facts }))
}

async fn persist_runtime_snapshot(
    tx: &mut Transaction<'_, Postgres>,
    runtime_thread_id: &RuntimeThreadId,
    snapshot: &ManagedRuntimeStateSnapshot,
    facts: &Value,
) -> Result<(), ManagedRuntimeStateStoreError> {
    let thread_id = runtime_thread_id.as_str();
    let revision = snapshot.revision.0.to_string();
    sqlx::query(
        "INSERT INTO agent_runtime_state_revision(thread_id, revision, facts)
         VALUES ($1, $2::NUMERIC(20,0), $3)
         ON CONFLICT (thread_id) DO UPDATE
         SET revision = EXCLUDED.revision, facts = EXCLUDED.facts",
    )
    .bind(thread_id)
    .bind(revision)
    .bind(facts)
    .execute(&mut **tx)
    .await
    .map_err(runtime_persistence)?;
    Ok(())
}

async fn persist_host_snapshot(
    tx: &mut Transaction<'_, Postgres>,
    snapshot: &CompleteAgentHostSnapshot,
    facts: &Value,
) -> Result<(), CompleteAgentHostStoreError> {
    let revision = snapshot.revision.0.to_string();
    sqlx::query(
        "UPDATE agent_runtime_host_revision SET revision = $1::NUMERIC(20,0), facts = $2
         WHERE singleton = TRUE",
    )
    .bind(revision)
    .bind(facts)
    .execute(&mut **tx)
    .await
    .map_err(host_persistence)?;
    Ok(())
}

async fn persist_callback_snapshot(
    tx: &mut Transaction<'_, Postgres>,
    snapshot: &CompleteAgentCallbackSnapshot,
    facts: &Value,
) -> Result<(), CompleteAgentCallbackStoreError> {
    let revision = snapshot.revision.0.to_string();
    sqlx::query(
        "UPDATE agent_runtime_callback_revision SET revision = $1::NUMERIC(20,0), facts = $2
         WHERE singleton = TRUE",
    )
    .bind(revision)
    .bind(facts)
    .execute(&mut **tx)
    .await
    .map_err(callback_persistence)?;
    Ok(())
}

fn parse_pg_u64(value: &str) -> Result<u64, std::num::ParseIntError> {
    value.parse()
}

fn runtime_persistence(error: sqlx::Error) -> ManagedRuntimeStateStoreError {
    ManagedRuntimeStateStoreError::Persistence {
        reason: error.to_string(),
    }
}

fn runtime_invariant(reason: &str) -> ManagedRuntimeStateStoreError {
    ManagedRuntimeStateStoreError::Invariant {
        reason: reason.to_owned(),
    }
}

fn host_persistence(error: sqlx::Error) -> CompleteAgentHostStoreError {
    CompleteAgentHostStoreError::Persistence {
        reason: error.to_string(),
    }
}

fn host_invariant(reason: &str) -> CompleteAgentHostStoreError {
    CompleteAgentHostStoreError::Invariant {
        reason: reason.to_owned(),
    }
}

fn callback_persistence(error: sqlx::Error) -> CompleteAgentCallbackStoreError {
    CompleteAgentCallbackStoreError::Persistence {
        reason: error.to_string(),
    }
}

fn callback_invariant(reason: &str) -> CompleteAgentCallbackStoreError {
    CompleteAgentCallbackStoreError::Invariant {
        reason: reason.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeMap, sync::Arc};

    use super::*;
    use agentdash_agent_runtime::{
        ManagedRuntimeCoordinator, ManagedRuntimeFacts, ManagedRuntimeStateRevision,
    };
    use agentdash_agent_runtime_contract::{
        ManagedRuntimeAvailabilityEvidence, ManagedRuntimeCommand,
        ManagedRuntimeCommandAvailability, ManagedRuntimeCommandEnvelope,
        ManagedRuntimeCommandKind, ManagedRuntimeContentBlock, ManagedRuntimeLifecycleStatus,
        ManagedRuntimeOperationStatus, ManagedRuntimeProjectionAuthority,
        ManagedRuntimeProjectionFidelity, ManagedRuntimeSnapshot, ManagedRuntimeThreadNameSource,
        RuntimeChangeSequence, RuntimeIdempotencyKey, RuntimeOperationId, RuntimePayloadDigest,
        RuntimeProjectionRevision,
    };
    use agentdash_agent_runtime_host::{
        CompleteAgentBinding, CompleteAgentBindingId, CompleteAgentBindingLease,
        CompleteAgentBindingState, CompleteAgentBindingTarget, CompleteAgentHostFacts,
        CompleteAgentPlacement,
    };
    use agentdash_agent_service_api::{
        AgentBindingGeneration, AgentEffectIdentity, AgentPayloadDigest, AgentProfileDigest,
        AgentServiceDefinitionId, AgentServiceInstanceId, AgentSourceCoordinate,
        AgentSurfaceDigest, AgentSurfaceRevision, BoundAgentSurface, CompleteAgentLiveAttachmentId,
    };
    use uuid::Uuid;

    fn thread_name_facts(
        thread_id: RuntimeThreadId,
        revision: u64,
        thread_name: Option<&str>,
    ) -> ManagedRuntimeFacts {
        let revision = RuntimeProjectionRevision(revision);
        let evidence = ManagedRuntimeAvailabilityEvidence {
            decided_at_revision: revision,
            blocking_operation_id: None,
            bound_surface_revision: None,
            applied_surface_revision: None,
        };
        ManagedRuntimeFacts {
            projection: Some(ManagedRuntimeSnapshot {
                thread_id,
                revision,
                latest_change_sequence: RuntimeChangeSequence(0),
                captured_at_ms: revision.0,
                lifecycle: ManagedRuntimeLifecycleStatus::Active,
                active_turn_id: None,
                turns: Vec::new(),
                items: Vec::new(),
                interactions: Vec::new(),
                conversation_history: Vec::new(),
                thread_name: thread_name.map(str::to_owned),
                thread_name_source: Some(ManagedRuntimeThreadNameSource {
                    authority: ManagedRuntimeProjectionAuthority::SourceAuthoritative,
                    fidelity: ManagedRuntimeProjectionFidelity::Exact,
                    source_identity_digest: RuntimePayloadDigest::new("sha256:thread-name-source")
                        .expect("source digest"),
                    source_revision_digest: Some(
                        RuntimePayloadDigest::new("sha256:thread-name-revision")
                            .expect("revision digest"),
                    ),
                    observed_at_ms: revision.0,
                }),
                operations: Vec::new(),
                source_binding: None,
                authority: ManagedRuntimeProjectionAuthority::SourceAuthoritative,
                fidelity: ManagedRuntimeProjectionFidelity::Exact,
                command_availability: ManagedRuntimeCommandKind::ALL
                    .into_iter()
                    .map(|command| {
                        (
                            command,
                            ManagedRuntimeCommandAvailability::Available {
                                evidence: evidence.clone(),
                            },
                        )
                    })
                    .collect::<BTreeMap<_, _>>(),
            }),
            ..ManagedRuntimeFacts::default()
        }
    }

    async fn isolated_thread_name_pool()
    -> (PgPool, Option<crate::postgres_runtime::PostgresRuntime>) {
        if crate::persistence::postgres::test_database_url().is_some() {
            return (
                crate::persistence::postgres::test_pg_pool("Runtime thread name persistence")
                    .await
                    .expect("configured PostgreSQL test pool"),
                None,
            );
        }
        let data_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/runtime-thread-name-postgres-tests");
        let runtime = crate::postgres_runtime::PostgresRuntime::resolve_embedded_at_data_root(
            "runtime-thread-name-tests",
            8,
            data_root,
        )
        .await
        .expect("start embedded PostgreSQL for Runtime thread name tests");
        let database_name = format!("runtime_thread_name_{}", uuid::Uuid::new_v4().simple());
        sqlx::query(&format!("CREATE DATABASE {database_name}"))
            .execute(&runtime.pool)
            .await
            .expect("create isolated Runtime thread name database");
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
            .expect("connect isolated Runtime thread name database");
        crate::migration::run_postgres_migrations(&pool)
            .await
            .expect("migrate isolated Runtime thread name database");
        crate::migration::assert_postgres_schema_ready(&pool)
            .await
            .expect("Runtime thread name schema readiness");
        (pool, Some(runtime))
    }

    #[tokio::test]
    async fn final_repositories_replay_exact_facts_without_advancing_revision() {
        let Some(pool) = crate::persistence::postgres::test_pg_pool("final repositories").await
        else {
            return;
        };
        let thread_id = RuntimeThreadId::new(format!("runtime-{}", uuid::Uuid::new_v4()))
            .expect("valid Runtime thread identity");
        let runtime = PostgresManagedRuntimeStateRepository::new(pool.clone());
        assert_eq!(
            runtime
                .load(&thread_id)
                .await
                .expect("load empty Runtime")
                .revision,
            ManagedRuntimeStateRevision(0)
        );
        let first = runtime
            .commit(ManagedRuntimeStateCommit {
                thread_id: thread_id.clone(),
                expected_revision: ManagedRuntimeStateRevision(0),
                facts: ManagedRuntimeFacts::default(),
            })
            .await
            .expect("commit Runtime facts");
        assert_eq!(first.revision, ManagedRuntimeStateRevision(1));
        let replay = runtime
            .commit(ManagedRuntimeStateCommit {
                thread_id,
                expected_revision: ManagedRuntimeStateRevision(0),
                facts: ManagedRuntimeFacts::default(),
            })
            .await
            .expect("replay exact Runtime facts");
        assert_eq!(replay.revision, ManagedRuntimeStateRevision(1));

        let host = PostgresCompleteAgentHostRepository::new(pool.clone());
        let host_before = host.load().await.expect("load Host facts");
        let host_replay = host
            .commit(CompleteAgentHostCommit {
                expected_revision: host_before.revision,
                facts: host_before.facts.clone(),
            })
            .await
            .expect("replay exact Host facts");
        assert_eq!(host_replay, host_before);

        let callback = PostgresCompleteAgentCallbackRepository::new(pool);
        let callback_before = callback.load().await.expect("load callback facts");
        let callback_replay = callback
            .commit(CompleteAgentCallbackCommit {
                expected_revision: callback_before.revision,
                facts: callback_before.facts.clone(),
            })
            .await
            .expect("replay exact callback facts");
        assert_eq!(callback_replay, callback_before);
    }

    #[tokio::test]
    async fn runtime_operation_status_advancement_reloads_from_canonical_document() {
        let (pool, _runtime) = isolated_thread_name_pool().await;
        let thread_id = RuntimeThreadId::new(format!("runtime-operation-{}", Uuid::new_v4()))
            .expect("valid Runtime thread identity");
        let repository = Arc::new(PostgresManagedRuntimeStateRepository::new(pool.clone()));
        repository
            .commit(ManagedRuntimeStateCommit {
                thread_id: thread_id.clone(),
                expected_revision: ManagedRuntimeStateRevision(0),
                facts: thread_name_facts(thread_id.clone(), 1, None),
            })
            .await
            .expect("seed admitted Runtime projection");

        let coordinator = ManagedRuntimeCoordinator::new(repository);
        let operation_id =
            RuntimeOperationId::new("operation-status-advance").expect("operation identity");
        coordinator
            .accept(
                ManagedRuntimeCommandEnvelope {
                    operation_id: operation_id.clone(),
                    idempotency_key: RuntimeIdempotencyKey::new("operation-status-idempotency")
                        .expect("idempotency identity"),
                    thread_id: thread_id.clone(),
                    command: ManagedRuntimeCommand::SubmitInput {
                        content: vec![ManagedRuntimeContentBlock::Text {
                            text: "hello".to_owned(),
                        }],
                    },
                },
                AgentEffectIdentity::new("operation-status-effect").expect("effect identity"),
                2,
            )
            .await
            .expect("accept Runtime operation");

        coordinator
            .mark_running(&thread_id, &operation_id, "test-worker".to_owned(), 3)
            .await
            .expect("advance Runtime operation to running");

        let reloaded = PostgresManagedRuntimeStateRepository::new(pool)
            .load(&thread_id)
            .await
            .expect("reload canonical Runtime document");
        assert_eq!(
            reloaded
                .facts
                .operations
                .get(&operation_id)
                .expect("persisted Runtime operation")
                .operation
                .status,
            ManagedRuntimeOperationStatus::Running
        );
    }

    #[tokio::test]
    async fn complete_agent_host_document_persists_exact_attachment_target() {
        let (pool, _runtime) = isolated_thread_name_pool().await;
        let repository = PostgresCompleteAgentHostRepository::new(pool.clone());
        let profile_digest = AgentProfileDigest::new("sha256:profile").expect("profile");
        let attachment_id =
            CompleteAgentLiveAttachmentId::new("attachment-exact").expect("attachment");
        let binding_id = CompleteAgentBindingId::new("binding-exact").expect("binding");
        let source = AgentSourceCoordinate::new("source-exact").expect("source");
        let binding = CompleteAgentBinding {
            id: binding_id.clone(),
            target: CompleteAgentBindingTarget {
                logical_instance_id: AgentServiceInstanceId::new("logical-service")
                    .expect("instance"),
                live_attachment_id: attachment_id.clone(),
                definition_id: AgentServiceDefinitionId::new("definition").expect("definition"),
                verified_build_digest: AgentPayloadDigest::new("sha256:build").expect("build"),
                verified_profile_digest: profile_digest.clone(),
                offer_profile_digest: profile_digest.clone(),
                placement: CompleteAgentPlacement::InProcess {
                    host_incarnation_id: "host-incarnation-exact".to_owned(),
                },
                remote_binding: None,
            },
            generation: AgentBindingGeneration(1),
            source: source.clone(),
            profile_digest: profile_digest.clone(),
            bound_surface: BoundAgentSurface {
                revision: AgentSurfaceRevision(1),
                digest: AgentSurfaceDigest::new("sha256:surface").expect("surface"),
                offer_profile_digest: profile_digest,
                contributions: Vec::new(),
            },
            applied_surface: None,
            state: CompleteAgentBindingState::PendingSurface,
        };
        let mut facts = CompleteAgentHostFacts::default();
        facts.bindings.insert(binding_id.clone(), binding);
        facts.source_coordinates.insert(binding_id.clone(), source);
        facts.lease_epochs.insert(binding_id.clone(), 1);
        facts.leases.insert(
            binding_id.clone(),
            CompleteAgentBindingLease {
                binding_id: binding_id.clone(),
                generation: AgentBindingGeneration(1),
                owner: "worker-exact".to_owned(),
                token: "lease-token-exact".to_owned(),
                epoch: 1,
                expires_at_ms: u64::MAX,
            },
        );

        let committed = repository
            .commit(CompleteAgentHostCommit {
                expected_revision: Default::default(),
                facts,
            })
            .await
            .expect("commit exact Host target");
        assert_eq!(
            repository.load().await.expect("reload exact Host target"),
            committed
        );

        let reloaded_binding = committed
            .facts
            .bindings
            .get(&binding_id)
            .expect("canonical Host binding");
        assert_eq!(
            reloaded_binding.target.live_attachment_id.as_str(),
            attachment_id.as_str()
        );
        assert_eq!(
            reloaded_binding.target.host_incarnation_id(),
            "host-incarnation-exact"
        );

        let mut released_facts = committed.facts.clone();
        released_facts.leases.remove(&binding_id);
        let released = repository
            .commit(CompleteAgentHostCommit {
                expected_revision: committed.revision,
                facts: released_facts,
            })
            .await
            .expect("release exact Host lease");
        assert!(!released.facts.leases.contains_key(&binding_id));
        assert_eq!(
            repository.load().await.expect("reload released Host lease"),
            released
        );
    }

    #[tokio::test]
    async fn runtime_thread_name_set_clear_replays_from_canonical_document() {
        let (pool, _runtime) = isolated_thread_name_pool().await;
        let thread_id = RuntimeThreadId::new(format!("runtime-name-{}", uuid::Uuid::new_v4()))
            .expect("valid Runtime thread identity");
        let runtime = PostgresManagedRuntimeStateRepository::new(pool.clone());
        let set = runtime
            .commit(ManagedRuntimeStateCommit {
                thread_id: thread_id.clone(),
                expected_revision: ManagedRuntimeStateRevision(0),
                facts: thread_name_facts(thread_id.clone(), 1, Some("Canonical title")),
            })
            .await
            .expect("persist source-authoritative thread name");
        assert_eq!(
            set.facts
                .projection
                .as_ref()
                .and_then(|projection| projection.thread_name.as_deref()),
            Some("Canonical title")
        );
        assert!(
            set.facts
                .projection
                .as_ref()
                .and_then(|projection| projection.thread_name_source.as_ref())
                .is_some()
        );

        let cleared = runtime
            .commit(ManagedRuntimeStateCommit {
                thread_id: thread_id.clone(),
                expected_revision: ManagedRuntimeStateRevision(1),
                facts: thread_name_facts(thread_id.clone(), 2, None),
            })
            .await
            .expect("persist authoritative thread name clear");
        let cleared_projection = cleared.facts.projection.as_ref().expect("clear projection");
        assert_eq!(cleared_projection.thread_name, None);
        assert!(cleared_projection.thread_name_source.is_some());

        assert_eq!(
            PostgresManagedRuntimeStateRepository::new(pool)
                .load(&thread_id)
                .await
                .expect("reload cleared canonical Runtime document"),
            cleared
        );
    }

    #[tokio::test]
    async fn final_postgres_coordinates_cover_exactly_the_canonical_u64_domain() {
        let (pool, _runtime) = isolated_thread_name_pool().await;
        let thread_id = RuntimeThreadId::new(format!("runtime-u64-{}", uuid::Uuid::new_v4()))
            .expect("valid Runtime thread identity");
        let runtime = PostgresManagedRuntimeStateRepository::new(pool.clone());
        runtime
            .commit(ManagedRuntimeStateCommit {
                thread_id: thread_id.clone(),
                expected_revision: ManagedRuntimeStateRevision(0),
                facts: ManagedRuntimeFacts::default(),
            })
            .await
            .expect("seed Runtime revision");
        sqlx::query(
            "UPDATE agent_runtime_state_revision
             SET revision=$2::NUMERIC(20,0) WHERE thread_id=$1",
        )
        .bind(thread_id.as_str())
        .bind((u64::MAX - 1).to_string())
        .execute(&pool)
        .await
        .expect("place Runtime at the last committable revision");

        let committed = runtime
            .commit(ManagedRuntimeStateCommit {
                thread_id: thread_id.clone(),
                expected_revision: ManagedRuntimeStateRevision(u64::MAX - 1),
                facts: thread_name_facts(thread_id.clone(), u64::MAX, Some("u64 max")),
            })
            .await
            .expect("commit canonical u64 max");
        assert_eq!(committed.revision, ManagedRuntimeStateRevision(u64::MAX));
        assert_eq!(
            runtime
                .load(&thread_id)
                .await
                .expect("reload canonical u64 max")
                .revision,
            ManagedRuntimeStateRevision(u64::MAX)
        );

        sqlx::query(
            "UPDATE agent_runtime_host_revision
             SET revision=$1::NUMERIC(20,0) WHERE singleton=TRUE",
        )
        .bind(u64::MAX.to_string())
        .execute(&pool)
        .await
        .expect("store Host u64 max");
        assert_eq!(
            PostgresCompleteAgentHostRepository::new(pool.clone())
                .load()
                .await
                .expect("load Host u64 max")
                .revision
                .0,
            u64::MAX
        );

        sqlx::query(
            "UPDATE agent_runtime_callback_revision
             SET revision=$1::NUMERIC(20,0) WHERE singleton=TRUE",
        )
        .bind(u64::MAX.to_string())
        .execute(&pool)
        .await
        .expect("store callback u64 max");
        assert_eq!(
            PostgresCompleteAgentCallbackRepository::new(pool.clone())
                .load()
                .await
                .expect("load callback u64 max")
                .revision
                .0,
            u64::MAX
        );

        assert!(
            sqlx::query(
                "UPDATE agent_runtime_host_revision
                 SET revision=18446744073709551616 WHERE singleton=TRUE",
            )
            .execute(&pool)
            .await
            .is_err(),
            "PostgreSQL must reject values outside the canonical u64 domain"
        );
    }
}
