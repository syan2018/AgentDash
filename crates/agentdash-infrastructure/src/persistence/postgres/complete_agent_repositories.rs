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
        let snapshot = runtime_snapshot(
            thread_id,
            row.try_get::<String, _>("revision")
                .map_err(runtime_persistence)?,
            row.try_get("facts").map_err(runtime_persistence)?,
        )?;
        verify_runtime_projection(&self.pool, thread_id, &snapshot).await?;
        Ok(snapshot)
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
        replace_runtime_projection(&mut tx, &thread_id, &committed, &facts).await?;
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
        let snapshot = host_snapshot(
            row.try_get::<String, _>("revision")
                .map_err(host_persistence)?,
            row.try_get("facts").map_err(host_persistence)?,
        )?;
        verify_host_projection(&self.pool, &snapshot).await?;
        Ok(snapshot)
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
        replace_host_projection(&mut tx, &committed, &facts).await?;
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
        let snapshot = callback_snapshot(
            row.try_get::<String, _>("revision")
                .map_err(callback_persistence)?,
            row.try_get("facts").map_err(callback_persistence)?,
        )?;
        verify_callback_projection(&self.pool, &snapshot).await?;
        Ok(snapshot)
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
        replace_callback_projection(&mut tx, &committed, &facts).await?;
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

async fn replace_runtime_projection(
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

    for table in [
        "agent_runtime_pending_command",
        "agent_runtime_idempotency",
        "agent_runtime_thread_binding",
        "agent_runtime_source_identity",
        "agent_runtime_source_projection",
        "agent_runtime_projection",
    ] {
        sqlx::query(&format!("DELETE FROM {table} WHERE thread_id = $1"))
            .bind(thread_id)
            .execute(&mut **tx)
            .await
            .map_err(runtime_persistence)?;
    }

    sqlx::query(
        "INSERT INTO agent_runtime_projection(thread_id, projection_revision, change_head, projection)
         SELECT $1, (p->>'revision')::NUMERIC(20,0),
                (p->>'latest_change_sequence')::NUMERIC(20,0), p
         FROM (SELECT $2::JSONB->'projection' AS p) facts
         WHERE p IS NOT NULL AND p <> 'null'::JSONB",
    )
    .bind(thread_id)
    .bind(facts)
    .execute(&mut **tx)
    .await
    .map_err(runtime_persistence)?;

    sqlx::query(
        "INSERT INTO agent_runtime_thread_binding(
             thread_id, source_ref, binding, committed_at_revision, activated_at_revision
         )
         SELECT $1, b->'source_ref', b->'binding',
                (b->>'committed_at_revision')::NUMERIC(20,0),
                NULLIF(b->>'activated_at_revision', '')::NUMERIC(20,0)
         FROM (SELECT $2::JSONB->'binding' AS b) facts
         WHERE b IS NOT NULL AND b <> 'null'::JSONB",
    )
    .bind(thread_id)
    .bind(facts)
    .execute(&mut **tx)
    .await
    .map_err(runtime_persistence)?;

    sqlx::query(
        "INSERT INTO agent_runtime_source_projection(
             thread_id, projection_revision, authority, fidelity, source_revision,
             source_cursor, projection_digest, projection
         )
         SELECT $1, (p->>'platform_revision')::NUMERIC(20,0),
                COALESCE(p#>>'{source_info,authority}', 'source_observed'),
                COALESCE(p#>>'{source_info,fidelity}', 'observed'),
                p#>>'{source_info,revision}', p->>'source_cursor',
                md5(p::TEXT), p
         FROM (SELECT $2::JSONB->'source_projection' AS p) facts
         WHERE p IS NOT NULL AND p <> 'null'::JSONB",
    )
    .bind(thread_id)
    .bind(facts)
    .execute(&mut **tx)
    .await
    .map_err(runtime_persistence)?;

    sqlx::query(
        "WITH identities AS (
             SELECT $1::TEXT AS thread_id, $2::JSONB->'source_identities' AS value
         ), rows AS (
             SELECT thread_id, 'turn' AS kind, entry.key AS source_identity,
                    entry.value #>> '{}' AS runtime_identity
             FROM identities, LATERAL jsonb_each(value->'turns') entry
             UNION ALL
             SELECT thread_id, 'item', entry.key,
                    entry.value->>'runtime_item_id'
             FROM identities, LATERAL jsonb_each(value->'items') entry
             UNION ALL
             SELECT thread_id, 'interaction', entry.key,
                    entry.value->>'runtime_interaction_id'
             FROM identities, LATERAL jsonb_each(value->'interactions') entry
             UNION ALL
             SELECT thread_id, 'surface_revision', entry.key,
                    entry.value #>> '{}'
             FROM identities, LATERAL jsonb_each(value->'surface_revisions') entry
         )
         INSERT INTO agent_runtime_source_identity(
             thread_id, identity_kind, source_identity, runtime_identity
         )
         SELECT thread_id, kind, source_identity, runtime_identity FROM rows",
    )
    .bind(thread_id)
    .bind(facts)
    .execute(&mut **tx)
    .await
    .map_err(runtime_persistence)?;

    sqlx::query(
        "INSERT INTO agent_runtime_source_change(
             thread_id, source_sequence, projection_revision, observation_digest,
             source_revision, source_cursor, changed_sections, change
         )
         SELECT $1, (value->>'sequence')::NUMERIC(20,0),
                (value->>'platform_revision')::NUMERIC(20,0), md5(value::TEXT),
                value#>>'{payload,source_revision}',
                value#>>'{payload,source_cursor}',
                value->'changed_sections', value
         FROM jsonb_array_elements($2::JSONB->'source_changes')
         ON CONFLICT (thread_id, source_sequence) DO NOTHING",
    )
    .bind(thread_id)
    .bind(facts)
    .execute(&mut **tx)
    .await
    .map_err(runtime_persistence)?;

    sqlx::query(
        "INSERT INTO agent_runtime_operation(
             thread_id, operation_id, command_kind, command, receipt, operation
         )
         SELECT $1, entry.key, entry.value#>>'{command,command,kind}',
                entry.value->'command', entry.value->'receipt', entry.value->'operation'
         FROM jsonb_each($2::JSONB->'operations') entry
         ON CONFLICT (thread_id, operation_id) DO NOTHING",
    )
    .bind(thread_id)
    .bind(facts)
    .execute(&mut **tx)
    .await
    .map_err(runtime_persistence)?;
    ensure_runtime_operation_prefix(tx, thread_id, facts).await?;

    sqlx::query(
        "INSERT INTO agent_runtime_idempotency(thread_id, idempotency_key, operation_id)
         SELECT $1, entry.key, entry.value #>> '{}'
         FROM jsonb_each($2::JSONB->'idempotency') entry",
    )
    .bind(thread_id)
    .bind(facts)
    .execute(&mut **tx)
    .await
    .map_err(runtime_persistence)?;
    sqlx::query(
        "INSERT INTO agent_runtime_pending_command(
             thread_id, operation_id, effect_id, state, command, claim_owner, claim_epoch
         )
         SELECT $1, entry.key, entry.value->>'effect_id', entry.value->>'state',
                entry.value->'command', entry.value->>'claim_owner',
                (entry.value->>'claim_epoch')::NUMERIC(20,0)
         FROM jsonb_each($2::JSONB->'pending_commands') entry",
    )
    .bind(thread_id)
    .bind(facts)
    .execute(&mut **tx)
    .await
    .map_err(runtime_persistence)?;

    sqlx::query(
        "INSERT INTO agent_runtime_change(thread_id, sequence, operation_id, change)
         SELECT $1, (value->>'sequence')::NUMERIC(20,0), NULL, value
         FROM jsonb_array_elements($2::JSONB->'changes')
         ON CONFLICT (thread_id, sequence) DO NOTHING",
    )
    .bind(thread_id)
    .bind(facts)
    .execute(&mut **tx)
    .await
    .map_err(runtime_persistence)?;
    sqlx::query(
        "INSERT INTO agent_runtime_outbox(thread_id, sequence, operation_id, change)
         SELECT $1, (value->>'sequence')::NUMERIC(20,0),
                value->>'operation_id', value->'change'
         FROM jsonb_array_elements($2::JSONB->'outbox')
         ON CONFLICT (thread_id, sequence) DO NOTHING",
    )
    .bind(thread_id)
    .bind(facts)
    .execute(&mut **tx)
    .await
    .map_err(runtime_persistence)?;
    sqlx::query(
        "INSERT INTO agent_runtime_surface_snapshot(
             thread_id, surface_revision, surface_digest, surface
         )
         SELECT $1, (surface->>'revision')::NUMERIC(20,0),
                surface->>'digest', surface
         FROM (
             SELECT $2::JSONB#>'{binding,binding,applied_surface}' AS surface
         ) candidate
         WHERE surface IS NOT NULL AND surface <> 'null'::JSONB
         ON CONFLICT (thread_id, surface_revision) DO NOTHING",
    )
    .bind(thread_id)
    .bind(facts)
    .execute(&mut **tx)
    .await
    .map_err(runtime_persistence)?;
    ensure_runtime_ledger_prefix(tx, thread_id, facts).await?;
    Ok(())
}

async fn ensure_runtime_operation_prefix(
    tx: &mut Transaction<'_, Postgres>,
    thread_id: &str,
    facts: &Value,
) -> Result<(), ManagedRuntimeStateStoreError> {
    let drift: bool = sqlx::query_scalar(
        "SELECT EXISTS (
             SELECT 1 FROM jsonb_each($2::JSONB->'operations') entry
             LEFT JOIN agent_runtime_operation stored
               ON stored.thread_id = $1 AND stored.operation_id = entry.key
             WHERE stored.operation_id IS NULL
                OR stored.command <> entry.value->'command'
                OR stored.receipt <> entry.value->'receipt'
                OR stored.operation <> entry.value->'operation'
         )",
    )
    .bind(thread_id)
    .bind(facts)
    .fetch_one(&mut **tx)
    .await
    .map_err(runtime_persistence)?;
    if drift {
        return Err(runtime_invariant(
            "Runtime operation projection drifted from canonical facts",
        ));
    }
    Ok(())
}

async fn ensure_runtime_ledger_prefix(
    tx: &mut Transaction<'_, Postgres>,
    thread_id: &str,
    facts: &Value,
) -> Result<(), ManagedRuntimeStateStoreError> {
    let drift: bool = sqlx::query_scalar(
        "SELECT
           EXISTS (
             SELECT 1 FROM agent_runtime_source_change stored
             LEFT JOIN jsonb_array_elements($2::JSONB->'source_changes') candidate
               ON (candidate->>'sequence')::NUMERIC(20,0) = stored.source_sequence
             WHERE stored.thread_id = $1
               AND (candidate IS NULL OR stored.change <> candidate)
           )
           OR EXISTS (
             SELECT 1 FROM agent_runtime_change stored
             LEFT JOIN jsonb_array_elements($2::JSONB->'changes') candidate
               ON (candidate->>'sequence')::NUMERIC(20,0) = stored.sequence
             WHERE stored.thread_id = $1
               AND (candidate IS NULL OR stored.change <> candidate)
           )
           OR EXISTS (
             SELECT 1 FROM agent_runtime_outbox stored
             LEFT JOIN jsonb_array_elements($2::JSONB->'outbox') candidate
               ON (candidate->>'sequence')::NUMERIC(20,0) = stored.sequence
             WHERE stored.thread_id = $1
               AND (candidate IS NULL OR stored.change <> candidate->'change')
           )",
    )
    .bind(thread_id)
    .bind(facts)
    .fetch_one(&mut **tx)
    .await
    .map_err(runtime_persistence)?;
    if drift {
        return Err(runtime_invariant(
            "Runtime change/outbox ledger is not an exact canonical prefix",
        ));
    }
    Ok(())
}

async fn replace_host_projection(
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

    execute_host_projection(
        tx,
        facts,
        "INSERT INTO agent_runtime_lifecycle_target(
             runtime_thread_id, logical_instance_id, live_attachment_id,
             host_incarnation_id, definition_id, generation, profile_digest,
             bound_surface_digest, target_snapshot, target
         )
         SELECT entry.key, entry.value#>>'{target,logical_instance_id}',
                entry.value#>>'{target,live_attachment_id}',
                entry.value#>>'{target,placement,host_incarnation_id}',
                entry.value#>>'{target,definition_id}',
                (entry.value->>'generation')::NUMERIC(20,0),
                entry.value->>'profile_digest',
                entry.value#>>'{bound_surface,digest}',
                entry.value->'target', entry.value
         FROM jsonb_each($1::JSONB->'runtime_targets') entry
         ON CONFLICT (runtime_thread_id) DO UPDATE SET
             logical_instance_id = EXCLUDED.logical_instance_id,
             live_attachment_id = EXCLUDED.live_attachment_id,
             host_incarnation_id = EXCLUDED.host_incarnation_id,
             definition_id = EXCLUDED.definition_id,
             generation = EXCLUDED.generation,
             profile_digest = EXCLUDED.profile_digest,
             bound_surface_digest = EXCLUDED.bound_surface_digest,
             target_snapshot = EXCLUDED.target_snapshot,
             target = EXCLUDED.target",
    )
    .await?;
    execute_host_projection(
        tx,
        facts,
        "INSERT INTO agent_runtime_binding(
             binding_id, logical_instance_id, live_attachment_id,
             host_incarnation_id, definition_id, generation, source_coordinate,
             profile_digest, bound_surface_digest, target_snapshot, state, binding
         )
         SELECT entry.key, entry.value#>>'{target,logical_instance_id}',
                entry.value#>>'{target,live_attachment_id}',
                entry.value#>>'{target,placement,host_incarnation_id}',
                entry.value#>>'{target,definition_id}',
                (entry.value->>'generation')::NUMERIC(20,0), entry.value->>'source',
                entry.value->>'profile_digest', entry.value#>>'{bound_surface,digest}',
                entry.value->'target', entry.value->>'state', entry.value
         FROM jsonb_each($1::JSONB->'bindings') entry
         ON CONFLICT (binding_id) DO UPDATE SET
             logical_instance_id = EXCLUDED.logical_instance_id,
             live_attachment_id = EXCLUDED.live_attachment_id,
             host_incarnation_id = EXCLUDED.host_incarnation_id,
             definition_id = EXCLUDED.definition_id,
             generation = EXCLUDED.generation,
             source_coordinate = EXCLUDED.source_coordinate,
             profile_digest = EXCLUDED.profile_digest,
             bound_surface_digest = EXCLUDED.bound_surface_digest,
             target_snapshot = EXCLUDED.target_snapshot,
             state = EXCLUDED.state,
             binding = EXCLUDED.binding",
    )
    .await?;
    execute_host_projection(
        tx,
        facts,
        "INSERT INTO agent_runtime_source_coordinate(
             binding_id, generation, source_coordinate
         )
         SELECT entry.key, binding.generation,
                entry.value #>> '{}'
         FROM jsonb_each($1::JSONB->'source_coordinates') entry
         JOIN agent_runtime_binding binding ON binding.binding_id = entry.key
         ON CONFLICT (binding_id) DO UPDATE SET
             generation = EXCLUDED.generation,
             source_coordinate = EXCLUDED.source_coordinate",
    )
    .await?;
    execute_host_projection(
        tx,
        facts,
        "INSERT INTO agent_runtime_callback_route(
             route_id, binding_id, generation, source_coordinate, delivery,
             default_deadline_ms, bound_surface_digest, route
         )
         SELECT entry.key, entry.value->>'binding_id',
                (entry.value->>'generation')::NUMERIC(20,0), entry.value->>'source',
                entry.value->>'delivery',
                (entry.value->>'default_deadline_ms')::NUMERIC(20,0),
                entry.value#>>'{bound_surface,digest}', entry.value
         FROM jsonb_each($1::JSONB->'callback_routes') entry
         ON CONFLICT (route_id) DO UPDATE SET
             binding_id = EXCLUDED.binding_id,
             generation = EXCLUDED.generation,
             source_coordinate = EXCLUDED.source_coordinate,
             delivery = EXCLUDED.delivery,
             default_deadline_ms = EXCLUDED.default_deadline_ms,
             bound_surface_digest = EXCLUDED.bound_surface_digest,
             route = EXCLUDED.route",
    )
    .await?;
    execute_host_projection(
        tx,
        facts,
        "INSERT INTO agent_runtime_callback_route_tombstone(route_id)
         SELECT value #>> '{}' FROM jsonb_array_elements(
             $1::JSONB->'revoked_callback_routes'
         )
         ON CONFLICT (route_id) DO NOTHING",
    )
    .await?;
    execute_host_projection(
        tx,
        facts,
        "INSERT INTO agent_runtime_lifecycle_effect(
             effect_id, runtime_thread_id, child_thread_id, operation_kind,
             live_attachment_id, host_incarnation_id, generation,
             target_snapshot, initial_context_digest, fork_cutoff, outcome, effect
         )
         SELECT entry.key, entry.value->>'runtime_thread_id',
                entry.value->>'child_thread_id', entry.value->>'kind',
                entry.value#>>'{target,live_attachment_id}',
                entry.value#>>'{target,placement,host_incarnation_id}',
                (entry.value->>'generation')::NUMERIC(20,0),
                entry.value->'target',
                entry.value#>>'{initial_context,digest}',
                entry.value->'fork_cutoff', entry.value->'outcome', entry.value
         FROM jsonb_each($1::JSONB->'lifecycle_effects') entry
         ON CONFLICT (effect_id) DO UPDATE SET
             outcome = EXCLUDED.outcome, effect = EXCLUDED.effect",
    )
    .await?;
    execute_host_projection(
        tx,
        facts,
        "INSERT INTO agent_runtime_effect(
             effect_id, command_id, binding_id, generation,
             source_coordinate, payload_digest, delivery_epoch, dispatch_attempt,
             state, effect
         )
         SELECT entry.key, entry.value->>'command_id', entry.value->>'binding_id',
                (entry.value->>'generation')::NUMERIC(20,0), entry.value->>'source',
                entry.value->>'payload_digest',
                (entry.value->>'delivery_epoch')::NUMERIC(20,0),
                (entry.value->>'dispatch_attempt')::NUMERIC(20,0),
                entry.value->>'state', entry.value
         FROM jsonb_each($1::JSONB->'effects') entry
         ON CONFLICT (effect_id) DO UPDATE SET
             delivery_epoch = EXCLUDED.delivery_epoch,
             dispatch_attempt = EXCLUDED.dispatch_attempt,
             state = EXCLUDED.state,
             effect = EXCLUDED.effect",
    )
    .await?;
    execute_host_projection(
        tx,
        facts,
        "INSERT INTO agent_runtime_effect_attempt_history(
             effect_id, dispatch_attempt, delivery_epoch, state, evidence
         )
         SELECT effect.key, (attempt.value->>'dispatch_attempt')::NUMERIC(20,0),
                (attempt.value->>'delivery_epoch')::NUMERIC(20,0),
                attempt.value->>'state', attempt.value
         FROM jsonb_each($1::JSONB->'effects') effect,
              LATERAL jsonb_array_elements(effect.value->'attempt_history') attempt
         ON CONFLICT (effect_id, dispatch_attempt) DO NOTHING",
    )
    .await?;
    ensure_host_attempt_prefix(tx, facts).await?;
    execute_host_projection(
        tx,
        facts,
        "DELETE FROM agent_runtime_lease
         WHERE NOT (($1::JSONB->'leases') ? binding_id)",
    )
    .await?;
    execute_host_projection(
        tx,
        facts,
        "INSERT INTO agent_runtime_lease_epoch(binding_id, epoch)
         SELECT entry.key, generate_series(
                    0::NUMERIC,
                    (entry.value #>> '{}')::NUMERIC(20,0)
                )
         FROM jsonb_each($1::JSONB->'lease_epochs') entry
         ON CONFLICT (binding_id, epoch) DO NOTHING",
    )
    .await?;
    execute_host_projection(
        tx,
        facts,
        "INSERT INTO agent_runtime_lease(
             binding_id, generation, owner, token, epoch, expires_at_ms
         )
         SELECT entry.key, (entry.value->>'generation')::NUMERIC(20,0),
                entry.value->>'owner', entry.value->>'token',
                (entry.value->>'epoch')::NUMERIC(20,0),
                (entry.value->>'expires_at_ms')::NUMERIC(20,0)
         FROM jsonb_each($1::JSONB->'leases') entry
         ON CONFLICT (binding_id) DO UPDATE SET
             generation = EXCLUDED.generation, owner = EXCLUDED.owner,
             token = EXCLUDED.token, epoch = EXCLUDED.epoch,
             expires_at_ms = EXCLUDED.expires_at_ms",
    )
    .await?;
    Ok(())
}

async fn execute_host_projection(
    tx: &mut Transaction<'_, Postgres>,
    facts: &Value,
    sql: &str,
) -> Result<(), CompleteAgentHostStoreError> {
    sqlx::query(sql)
        .bind(facts)
        .execute(&mut **tx)
        .await
        .map_err(host_persistence)?;
    Ok(())
}

async fn ensure_host_attempt_prefix(
    tx: &mut Transaction<'_, Postgres>,
    facts: &Value,
) -> Result<(), CompleteAgentHostStoreError> {
    let drift: bool = sqlx::query_scalar(
        "SELECT EXISTS (
             SELECT 1 FROM agent_runtime_effect_attempt_history stored
             LEFT JOIN jsonb_each($1::JSONB->'effects') effect
               ON effect.key = stored.effect_id
             LEFT JOIN LATERAL jsonb_array_elements(effect.value->'attempt_history') attempt
               ON (attempt->>'dispatch_attempt')::NUMERIC(20,0) = stored.dispatch_attempt
             WHERE attempt IS NULL OR stored.evidence <> attempt
         )",
    )
    .bind(facts)
    .fetch_one(&mut **tx)
    .await
    .map_err(host_persistence)?;
    if drift {
        return Err(host_invariant(
            "Host effect attempt ledger is not an exact canonical prefix",
        ));
    }
    Ok(())
}

async fn replace_callback_projection(
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
    sqlx::query(
        "INSERT INTO agent_runtime_callback_reservation(
             route_id, idempotency_key, callback_kind, request_digest, generation,
             source_coordinate, bound_surface_digest, deadline_at_ms, state, reservation
         )
         SELECT value#>>'{key,route_id}', value#>>'{key,idempotency_key}',
                value->>'kind', value->>'request_digest',
                (value->>'generation')::NUMERIC(20,0), value->>'source',
                value->>'bound_surface_digest',
                (value->>'deadline_at_ms')::NUMERIC(20,0),
                value#>>'{state,state}', value
         FROM jsonb_array_elements($1::JSONB->'callbacks')
         ON CONFLICT (route_id, idempotency_key) DO UPDATE SET
             state = EXCLUDED.state, reservation = EXCLUDED.reservation",
    )
    .bind(facts)
    .execute(&mut **tx)
    .await
    .map_err(callback_persistence)?;
    sqlx::query(
        "INSERT INTO agent_runtime_callback_outcome(route_id, idempotency_key, outcome)
         SELECT value#>>'{key,route_id}', value#>>'{key,idempotency_key}',
                value#>'{state,outcome}'
         FROM jsonb_array_elements($1::JSONB->'callbacks')
         WHERE value#>>'{state,state}' = 'settled'
         ON CONFLICT (route_id, idempotency_key) DO NOTHING",
    )
    .bind(facts)
    .execute(&mut **tx)
    .await
    .map_err(callback_persistence)?;
    ensure_callback_prefix(tx, facts).await
}

async fn ensure_callback_prefix(
    tx: &mut Transaction<'_, Postgres>,
    facts: &Value,
) -> Result<(), CompleteAgentCallbackStoreError> {
    let drift: bool = sqlx::query_scalar(
        "SELECT EXISTS (
             SELECT 1 FROM agent_runtime_callback_reservation stored
             LEFT JOIN jsonb_array_elements($1::JSONB->'callbacks') candidate
               ON candidate#>>'{key,route_id}' = stored.route_id
              AND candidate#>>'{key,idempotency_key}' = stored.idempotency_key
             WHERE candidate IS NULL
                OR stored.request_digest <> candidate->>'request_digest'
                OR stored.generation <> (candidate->>'generation')::NUMERIC(20,0)
                OR stored.source_coordinate <> candidate->>'source'
                OR stored.bound_surface_digest <> candidate->>'bound_surface_digest'
           )
           OR EXISTS (
             SELECT 1 FROM agent_runtime_callback_outcome stored
             LEFT JOIN jsonb_array_elements($1::JSONB->'callbacks') candidate
               ON candidate#>>'{key,route_id}' = stored.route_id
              AND candidate#>>'{key,idempotency_key}' = stored.idempotency_key
             WHERE candidate IS NULL
                OR candidate#>>'{state,state}' <> 'settled'
                OR stored.outcome <> candidate#>'{state,outcome}'
           )",
    )
    .bind(facts)
    .fetch_one(&mut **tx)
    .await
    .map_err(callback_persistence)?;
    if drift {
        return Err(callback_invariant(
            "callback reservation/outcome ledger is not an exact canonical prefix",
        ));
    }
    Ok(())
}

async fn verify_runtime_projection(
    pool: &PgPool,
    thread_id: &RuntimeThreadId,
    snapshot: &ManagedRuntimeStateSnapshot,
) -> Result<(), ManagedRuntimeStateStoreError> {
    let facts = encode_managed_runtime_state_snapshot(snapshot)?
        .get("facts")
        .cloned()
        .ok_or_else(|| runtime_invariant("encoded Runtime snapshot omitted facts"))?;
    let drift: bool = sqlx::query_scalar(
        "SELECT
           EXISTS (
             SELECT 1
             FROM jsonb_array_elements($2::JSONB->'changes') candidate
             LEFT JOIN agent_runtime_change stored
               ON stored.thread_id = $1
              AND stored.sequence = (candidate->>'sequence')::NUMERIC(20,0)
             WHERE stored.change IS DISTINCT FROM candidate
           )
           OR EXISTS (
             SELECT 1
             FROM jsonb_array_elements($2::JSONB->'outbox') candidate
             LEFT JOIN agent_runtime_outbox stored
               ON stored.thread_id = $1
              AND stored.sequence = (candidate->>'sequence')::NUMERIC(20,0)
             WHERE stored.change IS DISTINCT FROM candidate->'change'
           )
           OR EXISTS (
             SELECT 1 FROM agent_runtime_projection stored
             WHERE stored.thread_id = $1
               AND stored.projection IS DISTINCT FROM $2::JSONB->'projection'
           )",
    )
    .bind(thread_id.as_str())
    .bind(&facts)
    .fetch_one(pool)
    .await
    .map_err(runtime_persistence)?;
    if drift {
        return Err(runtime_invariant(
            "Runtime normalized rows drifted from canonical facts",
        ));
    }
    Ok(())
}

async fn verify_host_projection(
    pool: &PgPool,
    snapshot: &CompleteAgentHostSnapshot,
) -> Result<(), CompleteAgentHostStoreError> {
    let facts = encode_complete_agent_host_snapshot(snapshot)?
        .get("facts")
        .cloned()
        .ok_or_else(|| host_invariant("encoded Host snapshot omitted facts"))?;
    let drift: bool = sqlx::query_scalar(
        "SELECT
           EXISTS (
             SELECT 1 FROM jsonb_each($1::JSONB->'runtime_targets') candidate
             LEFT JOIN agent_runtime_lifecycle_target stored
               ON stored.runtime_thread_id = candidate.key
             WHERE stored.target IS DISTINCT FROM candidate.value
           )
           OR EXISTS (
             SELECT 1 FROM jsonb_each($1::JSONB->'bindings') candidate
             LEFT JOIN agent_runtime_binding stored ON stored.binding_id = candidate.key
             WHERE stored.binding IS DISTINCT FROM candidate.value
           )
           OR EXISTS (
             SELECT 1 FROM jsonb_each($1::JSONB->'callback_routes') candidate
             LEFT JOIN agent_runtime_callback_route stored ON stored.route_id = candidate.key
             WHERE stored.route IS DISTINCT FROM candidate.value
           )
           OR EXISTS (
             SELECT 1 FROM jsonb_each($1::JSONB->'lifecycle_effects') candidate
             LEFT JOIN agent_runtime_lifecycle_effect stored
               ON stored.effect_id = candidate.key
             WHERE stored.effect IS DISTINCT FROM candidate.value
           )
           OR EXISTS (
             SELECT 1 FROM jsonb_each($1::JSONB->'effects') candidate
             LEFT JOIN agent_runtime_effect stored ON stored.effect_id = candidate.key
             WHERE stored.effect IS DISTINCT FROM candidate.value
           )
           OR EXISTS (
             SELECT 1 FROM agent_runtime_lease stored
             LEFT JOIN jsonb_each($1::JSONB->'leases') candidate
               ON candidate.key = stored.binding_id
             WHERE candidate IS NULL
                OR stored.generation <> (candidate.value->>'generation')::NUMERIC(20,0)
                OR stored.owner <> candidate.value->>'owner'
                OR stored.token <> candidate.value->>'token'
                OR stored.epoch <> (candidate.value->>'epoch')::NUMERIC(20,0)
                OR stored.expires_at_ms
                   <> (candidate.value->>'expires_at_ms')::NUMERIC(20,0)
           )",
    )
    .bind(&facts)
    .fetch_one(pool)
    .await
    .map_err(host_persistence)?;
    if drift {
        return Err(host_invariant(
            "Host normalized rows drifted from canonical facts",
        ));
    }
    Ok(())
}

async fn verify_callback_projection(
    pool: &PgPool,
    snapshot: &CompleteAgentCallbackSnapshot,
) -> Result<(), CompleteAgentCallbackStoreError> {
    let facts = encode_complete_agent_callback_snapshot(snapshot)?
        .get("facts")
        .cloned()
        .ok_or_else(|| callback_invariant("encoded callback snapshot omitted facts"))?;
    let drift: bool = sqlx::query_scalar(
        "SELECT EXISTS (
             SELECT 1 FROM jsonb_array_elements($1::JSONB->'callbacks') candidate
             LEFT JOIN agent_runtime_callback_reservation stored
               ON stored.route_id = candidate#>>'{key,route_id}'
              AND stored.idempotency_key = candidate#>>'{key,idempotency_key}'
             WHERE stored.reservation IS DISTINCT FROM candidate
         )",
    )
    .bind(&facts)
    .fetch_one(pool)
    .await
    .map_err(callback_persistence)?;
    if drift {
        return Err(callback_invariant(
            "callback normalized rows drifted from canonical facts",
        ));
    }
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
    use std::collections::BTreeMap;

    use super::*;
    use agentdash_agent_runtime::{ManagedRuntimeFacts, ManagedRuntimeStateRevision};
    use agentdash_agent_runtime_contract::{
        ManagedRuntimeAvailabilityEvidence, ManagedRuntimeCommandAvailability,
        ManagedRuntimeCommandKind, ManagedRuntimeLifecycleStatus,
        ManagedRuntimeProjectionAuthority, ManagedRuntimeProjectionFidelity,
        ManagedRuntimeSnapshot, ManagedRuntimeThreadNameSource, RuntimeChangeSequence,
        RuntimePayloadDigest, RuntimeProjectionRevision,
    };
    use agentdash_agent_runtime_host::{
        CompleteAgentBinding, CompleteAgentBindingId, CompleteAgentBindingLease,
        CompleteAgentBindingState, CompleteAgentBindingTarget, CompleteAgentHostFacts,
        CompleteAgentPlacement,
    };
    use agentdash_agent_service_api::{
        AgentBindingGeneration, AgentPayloadDigest, AgentProfileDigest, AgentServiceDefinitionId,
        AgentServiceInstanceId, AgentSourceCoordinate, AgentSurfaceDigest, AgentSurfaceRevision,
        BoundAgentSurface, CompleteAgentLiveAttachmentId,
    };

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
    async fn complete_agent_host_projection_persists_exact_attachment_target_without_inventory() {
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

        let row = sqlx::query(
            "SELECT live_attachment_id, host_incarnation_id, target_snapshot
             FROM agent_runtime_binding WHERE binding_id=$1",
        )
        .bind(binding_id.as_str())
        .fetch_one(&pool)
        .await
        .expect("normalized binding");
        assert_eq!(
            row.try_get::<String, _>("live_attachment_id")
                .expect("attachment"),
            attachment_id.as_str()
        );
        assert_eq!(
            row.try_get::<String, _>("host_incarnation_id")
                .expect("incarnation"),
            "host-incarnation-exact"
        );
        assert_eq!(
            row.try_get::<Value, _>("target_snapshot")
                .expect("target snapshot")["live_attachment_id"],
            attachment_id.as_str()
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
            sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM agent_runtime_lease WHERE binding_id=$1",
            )
            .bind(binding_id.as_str())
            .fetch_one(&pool)
            .await
            .expect("count released normalized lease"),
            0,
            "released leases must disappear from the normalized projection"
        );
    }

    #[tokio::test]
    async fn runtime_thread_name_set_clear_replays_from_canonical_facts_and_rejects_projection_drift()
     {
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

        sqlx::query(
            "UPDATE agent_runtime_projection
             SET projection=jsonb_set(projection,'{thread_name}','\"drift\"'::JSONB,TRUE)
             WHERE thread_id=$1",
        )
        .bind(thread_id.as_str())
        .execute(&pool)
        .await
        .expect("tamper normalized projection");
        assert!(matches!(
            runtime.load(&thread_id).await,
            Err(ManagedRuntimeStateStoreError::Invariant { .. })
        ));
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
