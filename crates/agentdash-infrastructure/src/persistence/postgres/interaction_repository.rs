use chrono::{DateTime, Utc};
use sqlx::types::Json;
use sqlx::{PgPool, Postgres, Row, Transaction};
use uuid::Uuid;

use agentdash_domain::interaction::{
    DefinitionRevisionCommit, InteractionAttachment, InteractionCommandCommit,
    InteractionCommandTransaction, InteractionCommandTransactionPort, InteractionDefinition,
    InteractionDefinitionRepository, InteractionDefinitionRevision, InteractionError,
    InteractionEvent, InteractionEventRepository, InteractionInstance,
    InteractionInstanceRepository, InteractionOwner, InteractionRuntimeBinding,
    OperationEffectIntent, OperationEffectIntentRepository,
};

#[derive(Clone)]
pub struct PostgresInteractionRepository {
    pool: PgPool,
}

impl PostgresInteractionRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn initialize(&self) -> Result<(), InteractionError> {
        crate::migration::assert_postgres_tables_ready(
            &self.pool,
            &[
                "interaction_definitions",
                "interaction_definition_revisions",
                "interaction_instances",
                "interaction_attachments",
                "interaction_runtime_bindings",
                "interaction_command_receipts",
                "interaction_events",
                "interaction_operation_effect_intents",
            ],
        )
        .await
        .map_err(|error| InteractionError::Persistence {
            operation: "interaction_schema_readiness",
            message: error.to_string(),
        })
    }
}

#[async_trait::async_trait]
impl InteractionDefinitionRepository for PostgresInteractionRepository {
    async fn create(
        &self,
        definition: &InteractionDefinition,
        initial_revision: &InteractionDefinitionRevision,
    ) -> Result<(), InteractionError> {
        initial_revision.validate()?;
        if definition.id != initial_revision.definition_id
            || definition.current_revision_id != initial_revision.revision_id
            || definition.owner != initial_revision.owner
            || definition.kind != initial_revision.kind
        {
            return Err(InteractionError::InvalidField {
                field: "interaction_definition.initial_revision",
                reason: "definition 与 initial revision identity 必须一致",
            });
        }
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(db_error("interaction_create"))?;
        let (owner_kind, owner_id) = owner_parts(&definition.owner);
        sqlx::query("INSERT INTO interaction_definitions (id,owner_kind,owner_id,kind,current_revision_id,status,document,created_at,updated_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)")
            .bind(definition.id.to_string()).bind(owner_kind).bind(owner_id).bind("canvas")
            .bind(definition.current_revision_id.to_string()).bind("active")
            .bind(Json(to_value(definition, "interaction_definition")?))
            .bind(definition.created_at).bind(definition.updated_at)
            .execute(&mut *tx).await.map_err(db_error("interaction_definitions"))?;
        insert_revision(&mut tx, initial_revision).await?;
        tx.commit()
            .await
            .map_err(db_error("interaction_create_commit"))?;
        Ok(())
    }

    async fn get(&self, id: Uuid) -> Result<Option<InteractionDefinition>, InteractionError> {
        fetch_document(
            &self.pool,
            "SELECT document FROM interaction_definitions WHERE id=$1",
            id,
            "interaction_definitions.document",
        )
        .await
    }

    async fn get_revision(
        &self,
        revision_id: Uuid,
    ) -> Result<Option<InteractionDefinitionRevision>, InteractionError> {
        fetch_document(
            &self.pool,
            "SELECT document FROM interaction_definition_revisions WHERE revision_id=$1",
            revision_id,
            "interaction_definition_revisions.document",
        )
        .await
    }

    async fn list_by_owner(
        &self,
        owner: &InteractionOwner,
    ) -> Result<Vec<InteractionDefinition>, InteractionError> {
        owner.validate()?;
        let (owner_kind, owner_id) = owner_parts(owner);
        let rows = sqlx::query("SELECT document FROM interaction_definitions WHERE owner_kind=$1 AND owner_id=$2 ORDER BY created_at,id")
            .bind(owner_kind).bind(owner_id).fetch_all(&self.pool).await.map_err(db_error("interaction_definitions"))?;
        rows.into_iter()
            .map(|row| decode_row(&row, "interaction_definitions.document"))
            .collect()
    }

    async fn commit_revision(
        &self,
        definition_id: Uuid,
        commit: DefinitionRevisionCommit,
    ) -> Result<InteractionDefinition, InteractionError> {
        commit.revision.validate()?;
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(db_error("interaction_revision_begin"))?;
        let row = sqlx::query("SELECT document,current_revision_id FROM interaction_definitions WHERE id=$1 FOR UPDATE")
            .bind(definition_id.to_string()).fetch_optional(&mut *tx).await.map_err(db_error("interaction_definitions"))?
            .ok_or_else(|| InteractionError::NotFound { entity: "interaction_definition", id: definition_id.to_string() })?;
        let mut definition: InteractionDefinition =
            decode_row(&row, "interaction_definitions.document")?;
        let actual = parse_uuid(
            row.try_get::<String, _>("current_revision_id")
                .map_err(db_error("interaction_definitions.current_revision_id"))?,
            "interaction_definitions.current_revision_id",
        )?;
        if actual != commit.expected_current_revision_id {
            return Err(InteractionError::DefinitionRevisionConflict {
                definition_id,
                expected_revision_id: commit.expected_current_revision_id,
                actual_revision_id: actual,
            });
        }
        if commit.revision.definition_id != definition_id
            || commit.revision.owner != definition.owner
            || commit.revision.kind != definition.kind
        {
            return Err(InteractionError::InvalidField {
                field: "definition_revision.identity",
                reason: "revision 必须属于同一 definition/owner/kind",
            });
        }
        insert_revision(&mut tx, &commit.revision).await?;
        definition.current_revision_id = commit.revision.revision_id;
        definition.updated_at = commit.revision.created_at;
        sqlx::query("UPDATE interaction_definitions SET current_revision_id=$2,document=$3,updated_at=$4 WHERE id=$1")
            .bind(definition_id.to_string()).bind(definition.current_revision_id.to_string())
            .bind(Json(to_value(&definition,"interaction_definition")?)).bind(definition.updated_at)
            .execute(&mut *tx).await.map_err(db_error("interaction_definitions"))?;
        tx.commit()
            .await
            .map_err(db_error("interaction_revision_commit"))?;
        Ok(definition)
    }

    async fn archive(
        &self,
        definition_id: Uuid,
    ) -> Result<InteractionDefinition, InteractionError> {
        let mut definition = InteractionDefinitionRepository::get(self, definition_id)
            .await?
            .ok_or_else(|| InteractionError::NotFound {
                entity: "interaction_definition",
                id: definition_id.to_string(),
            })?;
        definition.status = agentdash_domain::interaction::InteractionDefinitionStatus::Archived;
        definition.updated_at = Utc::now();
        sqlx::query("UPDATE interaction_definitions SET status='archived',document=$2,updated_at=$3 WHERE id=$1")
            .bind(definition_id.to_string()).bind(Json(to_value(&definition,"interaction_definition")?)).bind(definition.updated_at)
            .execute(&self.pool).await.map_err(db_error("interaction_definitions"))?;
        Ok(definition)
    }
}

#[async_trait::async_trait]
impl InteractionInstanceRepository for PostgresInteractionRepository {
    async fn create(&self, instance: &InteractionInstance) -> Result<(), InteractionError> {
        let (owner_kind, owner_id) = owner_parts(&instance.owner);
        sqlx::query("INSERT INTO interaction_instances (id,owner_kind,owner_id,definition_id,definition_revision_id,contract_version,state_revision,status,state,document,created_at,updated_at,closed_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13)")
            .bind(instance.id.to_string()).bind(owner_kind).bind(owner_id).bind(instance.definition_id.to_string()).bind(instance.definition_revision_id.to_string())
            .bind(i32::from(instance.interaction_contract_version)).bind(instance.state_revision as i64).bind("open")
            .bind(Json(instance.state.clone())).bind(Json(to_value(instance,"interaction_instance")?)).bind(instance.created_at).bind(instance.updated_at).bind(instance.closed_at)
            .execute(&self.pool).await.map_err(db_error("interaction_instances"))?;
        Ok(())
    }

    async fn get(&self, id: Uuid) -> Result<Option<InteractionInstance>, InteractionError> {
        fetch_document(
            &self.pool,
            "SELECT document FROM interaction_instances WHERE id=$1",
            id,
            "interaction_instances.document",
        )
        .await
    }

    async fn list_by_owner(
        &self,
        owner: &InteractionOwner,
    ) -> Result<Vec<InteractionInstance>, InteractionError> {
        owner.validate()?;
        let (owner_kind, owner_id) = owner_parts(owner);
        let rows=sqlx::query("SELECT document FROM interaction_instances WHERE owner_kind=$1 AND owner_id=$2 ORDER BY created_at,id").bind(owner_kind).bind(owner_id).fetch_all(&self.pool).await.map_err(db_error("interaction_instances"))?;
        rows.into_iter()
            .map(|row| decode_row(&row, "interaction_instances.document"))
            .collect()
    }

    async fn close(
        &self,
        instance_id: Uuid,
        expected_state_revision: u64,
    ) -> Result<InteractionInstance, InteractionError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(db_error("interaction_close_begin"))?;
        let row = sqlx::query(
            "SELECT document,state_revision FROM interaction_instances WHERE id=$1 FOR UPDATE",
        )
        .bind(instance_id.to_string())
        .fetch_optional(&mut *tx)
        .await
        .map_err(db_error("interaction_instances"))?
        .ok_or_else(|| InteractionError::NotFound {
            entity: "interaction_instance",
            id: instance_id.to_string(),
        })?;
        let actual = row
            .try_get::<i64, _>("state_revision")
            .map_err(db_error("interaction_instances.state_revision"))? as u64;
        if actual != expected_state_revision {
            return Err(InteractionError::StateRevisionConflict {
                instance_id,
                expected: expected_state_revision,
                actual,
            });
        }
        let mut instance: InteractionInstance = decode_row(&row, "interaction_instances.document")?;
        instance.close(Utc::now())?;
        sqlx::query("UPDATE interaction_instances SET status='closed',document=$2,updated_at=$3,closed_at=$4 WHERE id=$1").bind(instance_id.to_string()).bind(Json(to_value(&instance,"interaction_instance")?)).bind(instance.updated_at).bind(instance.closed_at).execute(&mut *tx).await.map_err(db_error("interaction_instances"))?;
        tx.commit()
            .await
            .map_err(db_error("interaction_close_commit"))?;
        Ok(instance)
    }

    async fn attach(&self, attachment: &InteractionAttachment) -> Result<(), InteractionError> {
        attachment.validate()?;
        sqlx::query("INSERT INTO interaction_attachments (id,instance_id,document,created_at,detached_at) VALUES ($1,$2,$3,$4,$5)").bind(attachment.id.to_string()).bind(attachment.instance_id.to_string()).bind(Json(to_value(attachment,"interaction_attachment")?)).bind(attachment.created_at).bind(attachment.detached_at).execute(&self.pool).await.map_err(db_error("interaction_attachments"))?;
        Ok(())
    }
    async fn detach(&self, attachment_id: Uuid) -> Result<(), InteractionError> {
        let now = Utc::now();
        let result=sqlx::query("UPDATE interaction_attachments SET detached_at=$2,document=jsonb_set(document,'{detached_at}',to_jsonb($2::timestamptz),true) WHERE id=$1 AND detached_at IS NULL").bind(attachment_id.to_string()).bind(now).execute(&self.pool).await.map_err(db_error("interaction_attachments"))?;
        if result.rows_affected() == 0 {
            return Err(InteractionError::NotFound {
                entity: "interaction_attachment",
                id: attachment_id.to_string(),
            });
        }
        Ok(())
    }
    async fn upsert_runtime_binding(
        &self,
        binding: &InteractionRuntimeBinding,
    ) -> Result<(), InteractionError> {
        binding.validate()?;
        sqlx::query("INSERT INTO interaction_runtime_bindings (id,instance_id,attachment_id,slot_key,document,created_at) VALUES ($1,$2,$3,$4,$5,$6) ON CONFLICT (instance_id,attachment_scope,slot_key) DO UPDATE SET document=EXCLUDED.document") .bind(binding.id.to_string()).bind(binding.instance_id.to_string()).bind(binding.attachment_id.map(|id|id.to_string())).bind(&binding.slot_key).bind(Json(to_value(binding,"interaction_runtime_binding")?)).bind(binding.created_at).execute(&self.pool).await.map_err(db_error("interaction_runtime_bindings"))?;
        Ok(())
    }
    async fn list_runtime_bindings(
        &self,
        instance_id: Uuid,
        attachment_id: Option<Uuid>,
    ) -> Result<Vec<InteractionRuntimeBinding>, InteractionError> {
        let rows=sqlx::query("SELECT document FROM interaction_runtime_bindings WHERE instance_id=$1 AND attachment_scope=$2 ORDER BY slot_key").bind(instance_id.to_string()).bind(attachment_id.map(|id|id.to_string()).unwrap_or_default()).fetch_all(&self.pool).await.map_err(db_error("interaction_runtime_bindings"))?;
        rows.into_iter()
            .map(|row| decode_row(&row, "interaction_runtime_bindings.document"))
            .collect()
    }
}

#[async_trait::async_trait]
impl InteractionCommandTransactionPort for PostgresInteractionRepository {
    async fn commit(
        &self,
        transaction: InteractionCommandTransaction,
    ) -> Result<InteractionCommandCommit, InteractionError> {
        validate_transaction(&transaction)?;
        let request = &transaction.command.request;
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(db_error("interaction_command_begin"))?;
        let row = sqlx::query(
            "SELECT document,state_revision FROM interaction_instances WHERE id=$1 FOR UPDATE",
        )
        .bind(request.instance_id.to_string())
        .fetch_optional(&mut *tx)
        .await
        .map_err(db_error("interaction_instances"))?
        .ok_or_else(|| InteractionError::NotFound {
            entity: "interaction_instance",
            id: request.instance_id.to_string(),
        })?;
        let current: InteractionInstance = decode_row(&row, "interaction_instances.document")?;
        if let Some(receipt)=sqlx::query("SELECT request_digest,event_id,effect_id FROM interaction_command_receipts WHERE instance_id=$1 AND command_id=$2").bind(request.instance_id.to_string()).bind(request.command_id.to_string()).fetch_optional(&mut *tx).await.map_err(db_error("interaction_command_receipts"))? { return duplicate_commit(&mut tx,&current,request.command_id,receipt,&transaction.request_digest).await; }
        let actual = row
            .try_get::<i64, _>("state_revision")
            .map_err(db_error("interaction_instances.state_revision"))? as u64;
        if actual != request.expected_state_revision {
            return Err(InteractionError::StateRevisionConflict {
                instance_id: request.instance_id,
                expected: request.expected_state_revision,
                actual,
            });
        }
        let mut next = current;
        next.state = transaction.next_state.clone();
        next.state_revision = transaction.next_state_revision;
        next.updated_at = transaction.event.created_at;
        sqlx::query("UPDATE interaction_instances SET state=$2,state_revision=$3,document=$4,updated_at=$5 WHERE id=$1").bind(request.instance_id.to_string()).bind(Json(next.state.clone())).bind(next.state_revision as i64).bind(Json(to_value(&next,"interaction_instance")?)).bind(next.updated_at).execute(&mut *tx).await.map_err(db_error("interaction_instances"))?;
        sqlx::query("INSERT INTO interaction_events (id,instance_id,sequence,command_id,document,created_at) VALUES ($1,$2,$3,$4,$5,$6)").bind(transaction.event.id.to_string()).bind(request.instance_id.to_string()).bind(transaction.event.sequence as i64).bind(request.command_id.to_string()).bind(Json(to_value(&transaction.event,"interaction_event")?)).bind(transaction.event.created_at).execute(&mut *tx).await.map_err(db_error("interaction_events"))?;
        let effect_id = if let Some(effect) = &transaction.effect_intent {
            effect.validate()?;
            insert_effect(&mut tx, effect).await?;
            Some(effect.effect_id)
        } else {
            None
        };
        sqlx::query("INSERT INTO interaction_command_receipts (instance_id,command_id,request_digest,event_id,effect_id,created_at) VALUES ($1,$2,$3,$4,$5,$6)").bind(request.instance_id.to_string()).bind(request.command_id.to_string()).bind(&transaction.request_digest).bind(transaction.event.id.to_string()).bind(effect_id.map(|id|id.to_string())).bind(transaction.event.created_at).execute(&mut *tx).await.map_err(db_error("interaction_command_receipts"))?;
        tx.commit()
            .await
            .map_err(db_error("interaction_command_commit"))?;
        Ok(InteractionCommandCommit::Committed {
            instance: next,
            event: transaction.event,
            effect_intent: transaction.effect_intent,
        })
    }
}

#[async_trait::async_trait]
impl InteractionEventRepository for PostgresInteractionRepository {
    async fn list_events(
        &self,
        instance_id: Uuid,
        after_sequence: u64,
    ) -> Result<Vec<InteractionEvent>, InteractionError> {
        let rows = sqlx::query("SELECT document FROM interaction_events WHERE instance_id=$1 AND sequence>$2 ORDER BY sequence")
            .bind(instance_id.to_string()).bind(after_sequence as i64).fetch_all(&self.pool).await.map_err(db_error("interaction_events"))?;
        rows.into_iter()
            .map(|row| decode_row(&row, "interaction_events.document"))
            .collect()
    }
}

#[async_trait::async_trait]
impl OperationEffectIntentRepository for PostgresInteractionRepository {
    async fn claim_due(
        &self,
        limit: usize,
        claimed_at: DateTime<Utc>,
        claim_expires_at: DateTime<Utc>,
    ) -> Result<Vec<OperationEffectIntent>, InteractionError> {
        let limit = i64::try_from(limit).map_err(|_| InteractionError::InvalidField {
            field: "effect_claim.limit",
            reason: "limit 超出 i64",
        })?;
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(db_error("effect_claim_begin"))?;
        let rows=sqlx::query("SELECT document FROM interaction_operation_effect_intents WHERE ((status IN ('pending','retry_scheduled') AND next_attempt_at <= $1) OR (status='claimed' AND claim_expires_at <= $1)) ORDER BY next_attempt_at,effect_id FOR UPDATE SKIP LOCKED LIMIT $2").bind(claimed_at).bind(limit).fetch_all(&mut *tx).await.map_err(db_error("interaction_operation_effect_intents"))?;
        let mut claimed = Vec::with_capacity(rows.len());
        for row in rows {
            let mut effect: OperationEffectIntent =
                decode_row(&row, "interaction_operation_effect_intents.document")?;
            effect.claim(Uuid::new_v4(), claimed_at, claim_expires_at)?;
            update_effect(&mut tx, &effect).await?;
            claimed.push(effect);
        }
        tx.commit().await.map_err(db_error("effect_claim_commit"))?;
        Ok(claimed)
    }
    async fn mark_succeeded(
        &self,
        effect_id: Uuid,
        claim_token: Uuid,
        completed_at: DateTime<Utc>,
    ) -> Result<(), InteractionError> {
        mutate_effect(&self.pool, effect_id, |effect| {
            effect.mark_succeeded(claim_token, completed_at)
        })
        .await
    }
    async fn mark_failed(
        &self,
        effect_id: Uuid,
        claim_token: Uuid,
        next_attempt_at: DateTime<Utc>,
        failure_code: &str,
        terminal: bool,
    ) -> Result<(), InteractionError> {
        let failure_code = failure_code.to_string();
        mutate_effect(&self.pool, effect_id, move |effect| {
            if terminal {
                effect.mark_terminal_failed(claim_token, next_attempt_at, failure_code)
            } else {
                effect.schedule_retry(claim_token, next_attempt_at, failure_code)
            }
        })
        .await
    }
}

fn validate_transaction(
    transaction: &InteractionCommandTransaction,
) -> Result<(), InteractionError> {
    let request = &transaction.command.request;
    if transaction.request_digest.trim().is_empty() {
        return Err(InteractionError::InvalidField {
            field: "interaction_command.request_digest",
            reason: "不能为空",
        });
    }
    if transaction.previous_state_revision != request.expected_state_revision
        || transaction.next_state_revision != transaction.previous_state_revision + 1
        || transaction.event.instance_id != request.instance_id
        || transaction.event.command_id != request.command_id
        || transaction.event.command_key != request.command_key
        || transaction.event.handler != transaction.command.handler
        || transaction.event.resulting_state_revision != transaction.next_state_revision
        || transaction.event.sequence != transaction.next_state_revision
    {
        return Err(InteractionError::InvalidField {
            field: "interaction_command_transaction",
            reason: "request/event/state revision identity 必须一致且单调递增",
        });
    }
    if let Some(effect) = &transaction.effect_intent {
        if effect.instance_id != request.instance_id
            || effect.source_event_id != transaction.event.id
        {
            return Err(InteractionError::InvalidField {
                field: "operation_effect_intent.source",
                reason: "effect 必须关联本事务 instance/event",
            });
        }
    }
    Ok(())
}

async fn insert_revision(
    tx: &mut Transaction<'_, Postgres>,
    revision: &InteractionDefinitionRevision,
) -> Result<(), InteractionError> {
    let (owner_kind, owner_id) = owner_parts(&revision.owner);
    sqlx::query("INSERT INTO interaction_definition_revisions (revision_id,definition_id,revision_number,owner_kind,owner_id,source_bundle_digest,document,created_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8)").bind(revision.revision_id.to_string()).bind(revision.definition_id.to_string()).bind(revision.revision_number as i64).bind(owner_kind).bind(owner_id).bind(&revision.source_bundle.digest).bind(Json(to_value(revision,"interaction_definition_revision")?)).bind(revision.created_at).execute(&mut **tx).await.map_err(db_error("interaction_definition_revisions"))?;
    Ok(())
}
async fn insert_effect(
    tx: &mut Transaction<'_, Postgres>,
    effect: &OperationEffectIntent,
) -> Result<(), InteractionError> {
    sqlx::query("INSERT INTO interaction_operation_effect_intents (effect_id,instance_id,source_event_id,status,next_attempt_at,claim_token,claim_expires_at,document) VALUES ($1,$2,$3,$4,$5,$6,$7,$8)").bind(effect.effect_id.to_string()).bind(effect.instance_id.to_string()).bind(effect.source_event_id.to_string()).bind(effect_status(effect.status)).bind(effect.next_attempt_at).bind(effect.claim_token.map(|id|id.to_string())).bind(effect.claim_expires_at).bind(Json(to_value(effect,"operation_effect_intent")?)).execute(&mut **tx).await.map_err(db_error("interaction_operation_effect_intents"))?;
    Ok(())
}
async fn update_effect(
    tx: &mut Transaction<'_, Postgres>,
    effect: &OperationEffectIntent,
) -> Result<(), InteractionError> {
    sqlx::query("UPDATE interaction_operation_effect_intents SET status=$2,next_attempt_at=$3,claim_token=$4,claim_expires_at=$5,document=$6 WHERE effect_id=$1").bind(effect.effect_id.to_string()).bind(effect_status(effect.status)).bind(effect.next_attempt_at).bind(effect.claim_token.map(|id|id.to_string())).bind(effect.claim_expires_at).bind(Json(to_value(effect,"operation_effect_intent")?)).execute(&mut **tx).await.map_err(db_error("interaction_operation_effect_intents"))?;
    Ok(())
}
async fn mutate_effect<F>(
    pool: &PgPool,
    effect_id: Uuid,
    mutation: F,
) -> Result<(), InteractionError>
where
    F: FnOnce(&mut OperationEffectIntent) -> Result<(), InteractionError>,
{
    let mut tx = pool
        .begin()
        .await
        .map_err(db_error("effect_mutation_begin"))?;
    let row = sqlx::query(
        "SELECT document FROM interaction_operation_effect_intents WHERE effect_id=$1 FOR UPDATE",
    )
    .bind(effect_id.to_string())
    .fetch_optional(&mut *tx)
    .await
    .map_err(db_error("interaction_operation_effect_intents"))?
    .ok_or_else(|| InteractionError::NotFound {
        entity: "operation_effect_intent",
        id: effect_id.to_string(),
    })?;
    let mut effect: OperationEffectIntent =
        decode_row(&row, "interaction_operation_effect_intents.document")?;
    mutation(&mut effect)?;
    update_effect(&mut tx, &effect).await?;
    tx.commit()
        .await
        .map_err(db_error("effect_mutation_commit"))?;
    Ok(())
}

async fn duplicate_commit(
    tx: &mut Transaction<'_, Postgres>,
    instance: &InteractionInstance,
    command_id: Uuid,
    row: sqlx::postgres::PgRow,
    request_digest: &str,
) -> Result<InteractionCommandCommit, InteractionError> {
    let stored: String = row
        .try_get("request_digest")
        .map_err(db_error("interaction_command_receipts.request_digest"))?;
    if stored != request_digest {
        return Err(InteractionError::CommandIdempotencyConflict {
            instance_id: instance.id,
            command_id,
        });
    }
    let event_id = parse_uuid(
        row.try_get::<String, _>("event_id")
            .map_err(db_error("interaction_command_receipts.event_id"))?,
        "event_id",
    )?;
    let event: InteractionEvent = fetch_document_tx(
        tx,
        "SELECT document FROM interaction_events WHERE id=$1",
        event_id,
        "interaction_events.document",
    )
    .await?
    .ok_or_else(|| InteractionError::NotFound {
        entity: "interaction_event",
        id: event_id.to_string(),
    })?;
    let effect_id: Option<String> = row
        .try_get("effect_id")
        .map_err(db_error("interaction_command_receipts.effect_id"))?;
    let effect_intent = if let Some(id) = effect_id {
        let id = parse_uuid(id, "effect_id")?;
        fetch_document_tx(
            tx,
            "SELECT document FROM interaction_operation_effect_intents WHERE effect_id=$1",
            id,
            "interaction_operation_effect_intents.document",
        )
        .await?
    } else {
        None
    };
    Ok(InteractionCommandCommit::Duplicate {
        instance: instance.clone(),
        event,
        effect_intent,
    })
}

async fn fetch_document<T: serde::de::DeserializeOwned>(
    pool: &PgPool,
    sql: &str,
    id: Uuid,
    context: &'static str,
) -> Result<Option<T>, InteractionError> {
    sqlx::query(sql)
        .bind(id.to_string())
        .fetch_optional(pool)
        .await
        .map_err(db_error(context))?
        .map(|row| decode_row(&row, context))
        .transpose()
}
async fn fetch_document_tx<T: serde::de::DeserializeOwned>(
    tx: &mut Transaction<'_, Postgres>,
    sql: &str,
    id: Uuid,
    context: &'static str,
) -> Result<Option<T>, InteractionError> {
    sqlx::query(sql)
        .bind(id.to_string())
        .fetch_optional(&mut **tx)
        .await
        .map_err(db_error(context))?
        .map(|row| decode_row(&row, context))
        .transpose()
}
fn decode_row<T: serde::de::DeserializeOwned>(
    row: &sqlx::postgres::PgRow,
    context: &'static str,
) -> Result<T, InteractionError> {
    let Json(value): Json<serde_json::Value> =
        row.try_get("document").map_err(db_error(context))?;
    serde_json::from_value(value).map_err(|error| InteractionError::Serialization {
        context,
        message: error.to_string(),
    })
}
fn to_value<T: serde::Serialize>(
    value: &T,
    context: &'static str,
) -> Result<serde_json::Value, InteractionError> {
    serde_json::to_value(value).map_err(|error| InteractionError::Serialization {
        context,
        message: error.to_string(),
    })
}
fn owner_parts(owner: &InteractionOwner) -> (&'static str, String) {
    match owner {
        InteractionOwner::User(id) => ("user", id.clone()),
        InteractionOwner::Project(id) => ("project", id.to_string()),
    }
}
fn parse_uuid(value: String, context: &'static str) -> Result<Uuid, InteractionError> {
    Uuid::parse_str(&value).map_err(|error| InteractionError::Serialization {
        context,
        message: error.to_string(),
    })
}
fn effect_status(status: agentdash_domain::interaction::OperationEffectStatus) -> &'static str {
    use agentdash_domain::interaction::OperationEffectStatus::*;
    match status {
        Pending => "pending",
        Claimed => "claimed",
        Succeeded => "succeeded",
        RetryScheduled => "retry_scheduled",
        TerminalFailed => "terminal_failed",
    }
}
fn db_error(operation: &'static str) -> impl Fn(sqlx::Error) -> InteractionError {
    move |error| match error {
        sqlx::Error::Database(database)
            if matches!(
                database.code().as_deref(),
                Some("23505") | Some("23503") | Some("23P01")
            ) =>
        {
            InteractionError::PersistenceConflict {
                entity: operation,
                constraint: database
                    .constraint()
                    .unwrap_or("database_constraint")
                    .to_string(),
            }
        }
        other => InteractionError::Persistence {
            operation,
            message: other.to_string(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentdash_domain::interaction::{
        CommandActorPolicy, InteractionActor, InteractionCommandOrigin, InteractionCommandRequest,
        PlatformCommandHandler, ResolvedInteractionCommand,
    };
    #[test]
    fn transaction_validation_requires_monotonic_event_identity() {
        let instance_id = Uuid::new_v4();
        let command_id = Uuid::new_v4();
        let transaction = InteractionCommandTransaction {
            command: ResolvedInteractionCommand {
                request: InteractionCommandRequest {
                    instance_id,
                    command_id,
                    command_key: "set".into(),
                    payload: serde_json::json!({}),
                    expected_state_revision: 2,
                    actor: InteractionActor::Human {
                        user_id: "u".into(),
                    },
                    origin: InteractionCommandOrigin::UserWorkshop,
                    attachment_id: None,
                },
                handler: PlatformCommandHandler::StatePatchV1,
                actor_policy: CommandActorPolicy::Direct,
            },
            request_digest: "sha256:test".into(),
            previous_state_revision: 2,
            next_state: serde_json::json!({}),
            next_state_revision: 4,
            event: InteractionEvent {
                id: Uuid::new_v4(),
                instance_id,
                sequence: 4,
                command_id,
                command_key: "set".into(),
                handler: PlatformCommandHandler::StatePatchV1,
                actor: InteractionActor::Human {
                    user_id: "u".into(),
                },
                payload: serde_json::json!({}),
                resulting_state_revision: 4,
                created_at: Utc::now(),
            },
            effect_intent: None,
        };
        assert!(matches!(
            validate_transaction(&transaction),
            Err(InteractionError::InvalidField {
                field: "interaction_command_transaction",
                ..
            })
        ));
    }
}
