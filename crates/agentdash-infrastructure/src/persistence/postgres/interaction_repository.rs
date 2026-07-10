use chrono::{DateTime, Utc};
use sqlx::types::Json;
use sqlx::{PgPool, Postgres, Row, Transaction};
use uuid::Uuid;

use agentdash_domain::interaction::{
    AttachmentSubject, ComponentBinding, DefinitionLineage, DefinitionLineageKind,
    DefinitionRevisionCommit, InteractionAttachment, InteractionAttachmentRole,
    InteractionCommandCommit, InteractionCommandDefinition, InteractionCommandTransaction,
    InteractionCommandTransactionPort, InteractionDefinition, InteractionDefinitionKind,
    InteractionDefinitionRepository, InteractionDefinitionRevision, InteractionError,
    InteractionEvent, InteractionEventRepository, InteractionInstance,
    InteractionInstanceRepository, InteractionOwner, InteractionPresentationRepository,
    InteractionPresentationState, InteractionRendererLease, InteractionRuntimeBinding,
    OperationEffectIntent, OperationEffectIntentRepository, ResourceSlotDefinition, SourceBundle,
    SourceFile, SourceSandboxConfig,
};

#[derive(serde::Serialize, serde::Deserialize)]
struct PersistedDefinitionRevision {
    definition_id: Uuid,
    revision_id: Uuid,
    revision_number: u64,
    project_id: Uuid,
    owner: InteractionOwner,
    kind: InteractionDefinitionKind,
    definition_format_version: u16,
    interaction_contract_version: u16,
    title: String,
    description: String,
    source_bundle_digest: String,
    initial_state: serde_json::Value,
    state_schema: serde_json::Value,
    command_definitions: Vec<InteractionCommandDefinition>,
    component_bindings: Vec<ComponentBinding>,
    resource_slots: Vec<ResourceSlotDefinition>,
    created_by: String,
    created_at: DateTime<Utc>,
}

impl From<&InteractionDefinitionRevision> for PersistedDefinitionRevision {
    fn from(revision: &InteractionDefinitionRevision) -> Self {
        Self {
            definition_id: revision.definition_id,
            revision_id: revision.revision_id,
            revision_number: revision.revision_number,
            project_id: revision.project_id,
            owner: revision.owner.clone(),
            kind: revision.kind,
            definition_format_version: revision.definition_format_version,
            interaction_contract_version: revision.interaction_contract_version,
            title: revision.title.clone(),
            description: revision.description.clone(),
            source_bundle_digest: revision.source_bundle.digest.clone(),
            initial_state: revision.initial_state.clone(),
            state_schema: revision.state_schema.clone(),
            command_definitions: revision.command_definitions.clone(),
            component_bindings: revision.component_bindings.clone(),
            resource_slots: revision.resource_slots.clone(),
            created_by: revision.created_by.clone(),
            created_at: revision.created_at,
        }
    }
}

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
                "interaction_source_bundles",
                "interaction_source_files",
                "interaction_definition_lineage",
                "interaction_instances",
                "interaction_state_revisions",
                "interaction_attachments",
                "interaction_runtime_bindings",
                "interaction_presentation_states",
                "interaction_renderer_leases",
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
        definition.validate()?;
        if definition.id != initial_revision.definition_id
            || definition.current_revision_id != initial_revision.revision_id
            || definition.project_id != initial_revision.project_id
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
        sqlx::query("INSERT INTO interaction_definitions (id,project_id,owner_kind,owner_id,kind,current_revision_id,status,document,created_at,updated_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)")
            .bind(definition.id).bind(definition.project_id).bind(owner_kind).bind(owner_id).bind("canvas")
            .bind(definition.current_revision_id).bind("active")
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
        fetch_revision(&self.pool, revision_id).await
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

    async fn list_canvas_by_project(
        &self,
        project_id: Uuid,
    ) -> Result<Vec<InteractionDefinition>, InteractionError> {
        let rows = sqlx::query("SELECT document FROM interaction_definitions WHERE project_id=$1 AND kind='canvas' ORDER BY created_at,id")
            .bind(project_id)
            .fetch_all(&self.pool)
            .await
            .map_err(db_error("interaction_definitions"))?;
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
            .bind(definition_id).fetch_optional(&mut *tx).await.map_err(db_error("interaction_definitions"))?
            .ok_or_else(|| InteractionError::NotFound { entity: "interaction_definition", id: definition_id.to_string() })?;
        let mut definition: InteractionDefinition =
            decode_row(&row, "interaction_definitions.document")?;
        definition.validate()?;
        let actual: Uuid = row
            .try_get("current_revision_id")
            .map_err(db_error("interaction_definitions.current_revision_id"))?;
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
            || commit.revision.project_id != definition.project_id
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
            .bind(definition_id).bind(definition.current_revision_id)
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
            .bind(definition_id).bind(Json(to_value(&definition,"interaction_definition")?)).bind(definition.updated_at)
            .execute(&self.pool).await.map_err(db_error("interaction_definitions"))?;
        Ok(definition)
    }
}

#[async_trait::async_trait]
impl InteractionInstanceRepository for PostgresInteractionRepository {
    async fn create(&self, instance: &InteractionInstance) -> Result<(), InteractionError> {
        let (owner_kind, owner_id) = owner_parts(&instance.owner);
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(db_error("interaction_instance_create"))?;
        sqlx::query("INSERT INTO interaction_instances (id,owner_kind,owner_id,definition_id,definition_revision_id,contract_version,state_revision,status,state,document,created_at,updated_at,closed_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13)")
            .bind(instance.id).bind(owner_kind).bind(owner_id).bind(instance.definition_id).bind(instance.definition_revision_id)
            .bind(i16::try_from(instance.interaction_contract_version).map_err(|_| invalid_numeric("interaction_instance.contract_version"))?).bind(to_i64(instance.state_revision,"interaction_instance.state_revision")?).bind("open")
            .bind(Json(instance.state.clone())).bind(Json(to_value(instance,"interaction_instance")?)).bind(instance.created_at).bind(instance.updated_at).bind(instance.closed_at)
            .execute(&mut *tx).await.map_err(db_error("interaction_instances"))?;
        sqlx::query("INSERT INTO interaction_state_revisions (instance_id,state_revision,source_event_id,state,created_at) VALUES ($1,$2,NULL,$3,$4)")
            .bind(instance.id).bind(to_i64(instance.state_revision,"interaction_state_revision")?).bind(Json(instance.state.clone())).bind(instance.created_at)
            .execute(&mut *tx).await.map_err(db_error("interaction_state_revisions"))?;
        tx.commit()
            .await
            .map_err(db_error("interaction_instance_create_commit"))?;
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
        .bind(instance_id)
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
        sqlx::query("UPDATE interaction_instances SET status='closed',document=$2,updated_at=$3,closed_at=$4 WHERE id=$1").bind(instance_id).bind(Json(to_value(&instance,"interaction_instance")?)).bind(instance.updated_at).bind(instance.closed_at).execute(&mut *tx).await.map_err(db_error("interaction_instances"))?;
        tx.commit()
            .await
            .map_err(db_error("interaction_close_commit"))?;
        Ok(instance)
    }

    async fn attach(&self, attachment: &InteractionAttachment) -> Result<(), InteractionError> {
        attachment.validate()?;
        attachment.validate()?;
        let (subject_kind, subject_id) = attachment_subject_parts(attachment);
        sqlx::query("INSERT INTO interaction_attachments (id,instance_id,subject_kind,subject_id,role,document,created_at,detached_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8)").bind(attachment.id).bind(attachment.instance_id).bind(subject_kind).bind(subject_id).bind(attachment_role(attachment.role)).bind(Json(to_value(attachment,"interaction_attachment")?)).bind(attachment.created_at).bind(attachment.detached_at).execute(&self.pool).await.map_err(db_error("interaction_attachments"))?;
        Ok(())
    }
    async fn detach(&self, attachment_id: Uuid) -> Result<(), InteractionError> {
        let now = Utc::now();
        let result=sqlx::query("UPDATE interaction_attachments SET detached_at=$2,document=jsonb_set(document,'{detached_at}',to_jsonb($2::timestamptz),true) WHERE id=$1 AND detached_at IS NULL").bind(attachment_id).bind(now).execute(&self.pool).await.map_err(db_error("interaction_attachments"))?;
        if result.rows_affected() == 0 {
            return Err(InteractionError::NotFound {
                entity: "interaction_attachment",
                id: attachment_id.to_string(),
            });
        }
        Ok(())
    }

    async fn list_attachments(
        &self,
        instance_id: Uuid,
    ) -> Result<Vec<InteractionAttachment>, InteractionError> {
        let rows = sqlx::query(
            "SELECT document FROM interaction_attachments WHERE instance_id=$1 AND detached_at IS NULL ORDER BY created_at,id",
        )
        .bind(instance_id)
        .fetch_all(&self.pool)
        .await
        .map_err(db_error("interaction_attachments"))?;
        rows.into_iter()
            .map(|row| decode_row(&row, "interaction_attachments.document"))
            .collect()
    }
    async fn upsert_runtime_binding(
        &self,
        binding: &InteractionRuntimeBinding,
    ) -> Result<(), InteractionError> {
        binding.validate()?;
        binding.validate()?;
        sqlx::query("INSERT INTO interaction_runtime_bindings (id,instance_id,attachment_id,slot_key,document,created_at) VALUES ($1,$2,$3,$4,$5,$6) ON CONFLICT (instance_id,attachment_scope,slot_key) DO UPDATE SET id=EXCLUDED.id,attachment_id=EXCLUDED.attachment_id,document=EXCLUDED.document,created_at=EXCLUDED.created_at") .bind(binding.id).bind(binding.instance_id).bind(binding.attachment_id).bind(&binding.slot_key).bind(Json(to_value(binding,"interaction_runtime_binding")?)).bind(binding.created_at).execute(&self.pool).await.map_err(db_error("interaction_runtime_bindings"))?;
        Ok(())
    }
    async fn list_runtime_bindings(
        &self,
        instance_id: Uuid,
        attachment_id: Option<Uuid>,
    ) -> Result<Vec<InteractionRuntimeBinding>, InteractionError> {
        let rows=sqlx::query("SELECT document FROM interaction_runtime_bindings WHERE instance_id=$1 AND attachment_scope=$2 ORDER BY slot_key").bind(instance_id).bind(attachment_id.map(|id|id.to_string()).unwrap_or_default()).fetch_all(&self.pool).await.map_err(db_error("interaction_runtime_bindings"))?;
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
        .bind(request.instance_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(db_error("interaction_instances"))?
        .ok_or_else(|| InteractionError::NotFound {
            entity: "interaction_instance",
            id: request.instance_id.to_string(),
        })?;
        let current: InteractionInstance = decode_row(&row, "interaction_instances.document")?;
        if let Some(receipt)=sqlx::query("SELECT request_digest,event_id,effect_id FROM interaction_command_receipts WHERE instance_id=$1 AND command_id=$2").bind(request.instance_id).bind(request.command_id).fetch_optional(&mut *tx).await.map_err(db_error("interaction_command_receipts"))? { return duplicate_commit(&mut tx,&current,request.command_id,receipt,&transaction.request_digest).await; }
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
        sqlx::query("UPDATE interaction_instances SET state=$2,state_revision=$3,document=$4,updated_at=$5 WHERE id=$1").bind(request.instance_id).bind(Json(next.state.clone())).bind(to_i64(next.state_revision,"interaction_instance.state_revision")?).bind(Json(to_value(&next,"interaction_instance")?)).bind(next.updated_at).execute(&mut *tx).await.map_err(db_error("interaction_instances"))?;
        sqlx::query("INSERT INTO interaction_events (id,instance_id,sequence,command_id,document,created_at) VALUES ($1,$2,$3,$4,$5,$6)").bind(transaction.event.id).bind(request.instance_id).bind(to_i64(transaction.event.sequence,"interaction_event.sequence")?).bind(request.command_id).bind(Json(to_value(&transaction.event,"interaction_event")?)).bind(transaction.event.created_at).execute(&mut *tx).await.map_err(db_error("interaction_events"))?;
        sqlx::query("INSERT INTO interaction_state_revisions (instance_id,state_revision,source_event_id,state,created_at) VALUES ($1,$2,$3,$4,$5)").bind(request.instance_id).bind(to_i64(next.state_revision,"interaction_state_revision")?).bind(transaction.event.id).bind(Json(next.state.clone())).bind(transaction.event.created_at).execute(&mut *tx).await.map_err(db_error("interaction_state_revisions"))?;
        let effect_id = if let Some(effect) = &transaction.effect_intent {
            effect.validate()?;
            insert_effect(&mut tx, effect).await?;
            Some(effect.effect_id)
        } else {
            None
        };
        sqlx::query("INSERT INTO interaction_command_receipts (instance_id,command_id,request_digest,event_id,effect_id,created_at) VALUES ($1,$2,$3,$4,$5,$6)").bind(request.instance_id).bind(request.command_id).bind(&transaction.request_digest).bind(transaction.event.id).bind(effect_id).bind(transaction.event.created_at).execute(&mut *tx).await.map_err(db_error("interaction_command_receipts"))?;
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
            .bind(instance_id).bind(to_i64(after_sequence,"interaction_event.after_sequence")?).fetch_all(&self.pool).await.map_err(db_error("interaction_events"))?;
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

#[async_trait::async_trait]
impl InteractionPresentationRepository for PostgresInteractionRepository {
    async fn get_presentation_state(
        &self,
        instance_id: Uuid,
        user_id: &str,
        presentation_key: &str,
    ) -> Result<Option<InteractionPresentationState>, InteractionError> {
        sqlx::query("SELECT id,instance_id,user_id,presentation_key,revision,value,updated_at FROM interaction_presentation_states WHERE instance_id=$1 AND user_id=$2 AND presentation_key=$3")
            .bind(instance_id).bind(user_id).bind(presentation_key)
            .fetch_optional(&self.pool).await.map_err(db_error("interaction_presentation_states"))?
            .map(|row| {
                let Json(value): Json<serde_json::Value> = row.try_get("value").map_err(db_error("interaction_presentation_states.value"))?;
                Ok(InteractionPresentationState {
                    id: row.try_get("id").map_err(db_error("interaction_presentation_states.id"))?,
                    instance_id: row.try_get("instance_id").map_err(db_error("interaction_presentation_states.instance_id"))?,
                    user_id: row.try_get("user_id").map_err(db_error("interaction_presentation_states.user_id"))?,
                    presentation_key: row.try_get("presentation_key").map_err(db_error("interaction_presentation_states.presentation_key"))?,
                    revision: row.try_get::<i64,_>("revision").map_err(db_error("interaction_presentation_states.revision"))? as u64,
                    value,
                    updated_at: row.try_get("updated_at").map_err(db_error("interaction_presentation_states.updated_at"))?,
                })
            }).transpose()
    }

    async fn upsert_presentation_state(
        &self,
        state: &InteractionPresentationState,
        expected_revision: Option<u64>,
    ) -> Result<(), InteractionError> {
        state.validate()?;
        let result = match expected_revision {
            None => sqlx::query("INSERT INTO interaction_presentation_states (id,instance_id,user_id,presentation_key,revision,value,updated_at) VALUES ($1,$2,$3,$4,$5,$6,$7) ON CONFLICT (instance_id,user_id,presentation_key) DO NOTHING")
                .bind(state.id).bind(state.instance_id).bind(&state.user_id).bind(&state.presentation_key).bind(to_i64(state.revision,"interaction_presentation_state.revision")?).bind(Json(state.value.clone())).bind(state.updated_at)
                .execute(&self.pool).await,
            Some(expected) => sqlx::query("UPDATE interaction_presentation_states SET id=$1,revision=$2,value=$3,updated_at=$4 WHERE instance_id=$5 AND user_id=$6 AND presentation_key=$7 AND revision=$8")
                .bind(state.id).bind(to_i64(state.revision,"interaction_presentation_state.revision")?).bind(Json(state.value.clone())).bind(state.updated_at).bind(state.instance_id).bind(&state.user_id).bind(&state.presentation_key).bind(to_i64(expected,"interaction_presentation_state.expected_revision")?)
                .execute(&self.pool).await,
        }.map_err(db_error("interaction_presentation_states"))?;
        if result.rows_affected() != 1 {
            return Err(InteractionError::StateRevisionConflict {
                instance_id: state.instance_id,
                expected: expected_revision.unwrap_or(0),
                actual: state.revision,
            });
        }
        Ok(())
    }

    async fn upsert_renderer_lease(
        &self,
        lease: &InteractionRendererLease,
        expected_revision: Option<u64>,
    ) -> Result<(), InteractionError> {
        lease.validate(Utc::now())?;
        let result = match expected_revision {
            None => sqlx::query("INSERT INTO interaction_renderer_leases (id,instance_id,renderer_key,user_id,revision,acquired_at,renewed_at,expires_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT (instance_id,renderer_key) DO UPDATE SET id=EXCLUDED.id,user_id=EXCLUDED.user_id,revision=EXCLUDED.revision,acquired_at=EXCLUDED.acquired_at,renewed_at=EXCLUDED.renewed_at,expires_at=EXCLUDED.expires_at WHERE interaction_renderer_leases.expires_at <= EXCLUDED.renewed_at")
                .bind(lease.id).bind(lease.instance_id).bind(&lease.renderer_key).bind(&lease.user_id).bind(to_i64(lease.revision,"interaction_renderer_lease.revision")?).bind(lease.acquired_at).bind(lease.renewed_at).bind(lease.expires_at)
                .execute(&self.pool).await,
            Some(expected) => sqlx::query("UPDATE interaction_renderer_leases SET revision=$1,renewed_at=$2,expires_at=$3 WHERE id=$4 AND instance_id=$5 AND renderer_key=$6 AND user_id=$7 AND revision=$8 AND expires_at>$2")
                .bind(to_i64(lease.revision,"interaction_renderer_lease.revision")?).bind(lease.renewed_at).bind(lease.expires_at).bind(lease.id).bind(lease.instance_id).bind(&lease.renderer_key).bind(&lease.user_id).bind(to_i64(expected,"interaction_renderer_lease.expected_revision")?)
                .execute(&self.pool).await,
        }.map_err(db_error("interaction_renderer_leases"))?;
        if result.rows_affected() != 1 {
            return Err(InteractionError::StateRevisionConflict {
                instance_id: lease.instance_id,
                expected: expected_revision.unwrap_or(0),
                actual: lease.revision,
            });
        }
        Ok(())
    }

    async fn release_renderer_lease(
        &self,
        lease_id: Uuid,
        expected_revision: u64,
    ) -> Result<(), InteractionError> {
        let result =
            sqlx::query("DELETE FROM interaction_renderer_leases WHERE id=$1 AND revision=$2")
                .bind(lease_id)
                .bind(to_i64(
                    expected_revision,
                    "interaction_renderer_lease.expected_revision",
                )?)
                .execute(&self.pool)
                .await
                .map_err(db_error("interaction_renderer_leases"))?;
        if result.rows_affected() == 0 {
            return Err(InteractionError::StateRevisionConflict {
                instance_id: lease_id,
                expected: expected_revision,
                actual: 0,
            });
        }
        Ok(())
    }

    async fn list_active_renderer_leases(
        &self,
        instance_id: Uuid,
        now: DateTime<Utc>,
    ) -> Result<Vec<InteractionRendererLease>, InteractionError> {
        let rows = sqlx::query("SELECT id,instance_id,renderer_key,user_id,revision,acquired_at,renewed_at,expires_at FROM interaction_renderer_leases WHERE instance_id=$1 AND expires_at>$2 ORDER BY expires_at,renderer_key")
            .bind(instance_id).bind(now).fetch_all(&self.pool).await.map_err(db_error("interaction_renderer_leases"))?;
        rows.into_iter()
            .map(|row| {
                Ok(InteractionRendererLease {
                    id: row
                        .try_get("id")
                        .map_err(db_error("interaction_renderer_leases.id"))?,
                    instance_id: row
                        .try_get("instance_id")
                        .map_err(db_error("interaction_renderer_leases.instance_id"))?,
                    renderer_key: row
                        .try_get("renderer_key")
                        .map_err(db_error("interaction_renderer_leases.renderer_key"))?,
                    user_id: row
                        .try_get("user_id")
                        .map_err(db_error("interaction_renderer_leases.user_id"))?,
                    revision: row
                        .try_get::<i64, _>("revision")
                        .map_err(db_error("interaction_renderer_leases.revision"))?
                        as u64,
                    acquired_at: row
                        .try_get("acquired_at")
                        .map_err(db_error("interaction_renderer_leases.acquired_at"))?,
                    renewed_at: row
                        .try_get("renewed_at")
                        .map_err(db_error("interaction_renderer_leases.renewed_at"))?,
                    expires_at: row
                        .try_get("expires_at")
                        .map_err(db_error("interaction_renderer_leases.expires_at"))?,
                })
            })
            .collect()
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
    if let Some(effect) = &transaction.effect_intent
        && (effect.instance_id != request.instance_id
            || effect.source_event_id != transaction.event.id)
    {
        return Err(InteractionError::InvalidField {
            field: "operation_effect_intent.source",
            reason: "effect 必须关联本事务 instance/event",
        });
    }
    Ok(())
}

async fn insert_revision(
    tx: &mut Transaction<'_, Postgres>,
    revision: &InteractionDefinitionRevision,
) -> Result<(), InteractionError> {
    let (owner_kind, owner_id) = owner_parts(&revision.owner);
    insert_source_bundle(tx, &revision.source_bundle, revision.created_at).await?;
    let document = PersistedDefinitionRevision::from(revision);
    sqlx::query("INSERT INTO interaction_definition_revisions (revision_id,definition_id,revision_number,project_id,owner_kind,owner_id,source_bundle_digest,document,created_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)").bind(revision.revision_id).bind(revision.definition_id).bind(to_i64(revision.revision_number,"interaction_definition_revision.revision_number")?).bind(revision.project_id).bind(owner_kind).bind(owner_id).bind(&revision.source_bundle.digest).bind(Json(to_value(&document,"interaction_definition_revision")?)).bind(revision.created_at).execute(&mut **tx).await.map_err(db_error("interaction_definition_revisions"))?;
    if let Some(lineage) = &revision.lineage {
        sqlx::query("INSERT INTO interaction_definition_lineage (definition_revision_id,lineage_kind,source_definition_id,source_revision_id,source_bundle_digest) VALUES ($1,$2,$3,$4,$5)")
            .bind(revision.revision_id).bind(lineage_kind(lineage.kind)).bind(lineage.source_definition_id).bind(lineage.source_revision_id).bind(&lineage.source_bundle_digest)
            .execute(&mut **tx).await.map_err(db_error("interaction_definition_lineage"))?;
    }
    Ok(())
}

async fn insert_source_bundle(
    tx: &mut Transaction<'_, Postgres>,
    bundle: &SourceBundle,
    created_at: DateTime<Utc>,
) -> Result<(), InteractionError> {
    bundle.verify_digest()?;
    sqlx::query("INSERT INTO interaction_source_bundles (digest,format_version,entry_file,sandbox,created_at) VALUES ($1,$2,$3,$4,$5) ON CONFLICT (digest) DO NOTHING")
        .bind(&bundle.digest).bind(i16::try_from(bundle.format_version).map_err(|_| invalid_numeric("interaction_source_bundle.format_version"))?).bind(&bundle.entry_file).bind(Json(to_value(&bundle.sandbox,"interaction_source_bundle.sandbox")?)).bind(created_at)
        .execute(&mut **tx).await.map_err(db_error("interaction_source_bundles"))?;
    for file in &bundle.files {
        sqlx::query("INSERT INTO interaction_source_files (source_bundle_digest,path,content,media_type) VALUES ($1,$2,$3,$4) ON CONFLICT (source_bundle_digest,path) DO NOTHING")
            .bind(&bundle.digest).bind(&file.path).bind(&file.content).bind(&file.media_type)
            .execute(&mut **tx).await.map_err(db_error("interaction_source_files"))?;
    }
    let stored = fetch_source_bundle_tx(tx, &bundle.digest).await?;
    validate_stored_source_bundle(bundle, &stored)?;
    Ok(())
}

fn validate_stored_source_bundle(
    incoming: &SourceBundle,
    stored: &SourceBundle,
) -> Result<(), InteractionError> {
    stored
        .verify_digest()
        .map_err(|_| InteractionError::PersistenceConflict {
            entity: "interaction_source_bundle",
            constraint: "digest_content_integrity".into(),
        })?;
    if stored != incoming {
        return Err(InteractionError::PersistenceConflict {
            entity: "interaction_source_bundle",
            constraint: "digest_content_integrity".into(),
        });
    }
    Ok(())
}

async fn fetch_source_bundle_tx(
    tx: &mut Transaction<'_, Postgres>,
    digest: &str,
) -> Result<SourceBundle, InteractionError> {
    let row = sqlx::query(
        "SELECT format_version,entry_file,sandbox FROM interaction_source_bundles WHERE digest=$1",
    )
    .bind(digest)
    .fetch_one(&mut **tx)
    .await
    .map_err(db_error("interaction_source_bundles"))?;
    let Json(sandbox): Json<SourceSandboxConfig> = row
        .try_get("sandbox")
        .map_err(db_error("interaction_source_bundles.sandbox"))?;
    let file_rows = sqlx::query("SELECT path,content,media_type FROM interaction_source_files WHERE source_bundle_digest=$1 ORDER BY path")
        .bind(digest).fetch_all(&mut **tx).await.map_err(db_error("interaction_source_files"))?;
    source_bundle_from_rows(digest, row, file_rows, sandbox)
}

async fn fetch_revision(
    pool: &PgPool,
    revision_id: Uuid,
) -> Result<Option<InteractionDefinitionRevision>, InteractionError> {
    let Some(row) = sqlx::query("SELECT document,source_bundle_digest FROM interaction_definition_revisions WHERE revision_id=$1")
        .bind(revision_id).fetch_optional(pool).await.map_err(db_error("interaction_definition_revisions"))?
    else { return Ok(None); };
    let persisted: PersistedDefinitionRevision =
        decode_row(&row, "interaction_definition_revisions.document")?;
    let digest: String = row.try_get("source_bundle_digest").map_err(db_error(
        "interaction_definition_revisions.source_bundle_digest",
    ))?;
    if persisted.source_bundle_digest != digest {
        return Err(InteractionError::Serialization {
            context: "interaction_definition_revisions.source_bundle_digest",
            message: "document ref 与 scalar digest 不一致".into(),
        });
    }
    let source_bundle = fetch_source_bundle(pool, &digest).await?;
    let lineage = fetch_lineage(pool, revision_id).await?;
    let revision = InteractionDefinitionRevision {
        definition_id: persisted.definition_id,
        revision_id: persisted.revision_id,
        revision_number: persisted.revision_number,
        project_id: persisted.project_id,
        owner: persisted.owner,
        kind: persisted.kind,
        definition_format_version: persisted.definition_format_version,
        interaction_contract_version: persisted.interaction_contract_version,
        title: persisted.title,
        description: persisted.description,
        source_bundle,
        initial_state: persisted.initial_state,
        state_schema: persisted.state_schema,
        command_definitions: persisted.command_definitions,
        component_bindings: persisted.component_bindings,
        resource_slots: persisted.resource_slots,
        lineage,
        created_by: persisted.created_by,
        created_at: persisted.created_at,
    };
    revision.validate()?;
    Ok(Some(revision))
}

async fn fetch_source_bundle(
    pool: &PgPool,
    digest: &str,
) -> Result<SourceBundle, InteractionError> {
    let row = sqlx::query(
        "SELECT format_version,entry_file,sandbox FROM interaction_source_bundles WHERE digest=$1",
    )
    .bind(digest)
    .fetch_optional(pool)
    .await
    .map_err(db_error("interaction_source_bundles"))?
    .ok_or_else(|| InteractionError::NotFound {
        entity: "interaction_source_bundle",
        id: digest.to_owned(),
    })?;
    let Json(sandbox): Json<SourceSandboxConfig> = row
        .try_get("sandbox")
        .map_err(db_error("interaction_source_bundles.sandbox"))?;
    let file_rows = sqlx::query("SELECT path,content,media_type FROM interaction_source_files WHERE source_bundle_digest=$1 ORDER BY path")
        .bind(digest).fetch_all(pool).await.map_err(db_error("interaction_source_files"))?;
    source_bundle_from_rows(digest, row, file_rows, sandbox)
}

fn source_bundle_from_rows(
    digest: &str,
    row: sqlx::postgres::PgRow,
    file_rows: Vec<sqlx::postgres::PgRow>,
    sandbox: SourceSandboxConfig,
) -> Result<SourceBundle, InteractionError> {
    let files = file_rows
        .into_iter()
        .map(|row| {
            Ok(SourceFile {
                path: row
                    .try_get("path")
                    .map_err(db_error("interaction_source_files.path"))?,
                content: row
                    .try_get("content")
                    .map_err(db_error("interaction_source_files.content"))?,
                media_type: row
                    .try_get("media_type")
                    .map_err(db_error("interaction_source_files.media_type"))?,
            })
        })
        .collect::<Result<Vec<_>, InteractionError>>()?;
    let bundle = SourceBundle {
        format_version: row
            .try_get::<i16, _>("format_version")
            .map_err(db_error("interaction_source_bundles.format_version"))?
            as u16,
        entry_file: row
            .try_get("entry_file")
            .map_err(db_error("interaction_source_bundles.entry_file"))?,
        files,
        sandbox,
        digest: digest.to_owned(),
    };
    bundle.verify_digest()?;
    Ok(bundle)
}

async fn fetch_lineage(
    pool: &PgPool,
    revision_id: Uuid,
) -> Result<Option<DefinitionLineage>, InteractionError> {
    sqlx::query("SELECT lineage_kind,source_definition_id,source_revision_id,source_bundle_digest FROM interaction_definition_lineage WHERE definition_revision_id=$1")
        .bind(revision_id).fetch_optional(pool).await.map_err(db_error("interaction_definition_lineage"))?
        .map(|row| Ok(DefinitionLineage {
            kind: parse_lineage_kind(row.try_get::<String,_>("lineage_kind").map_err(db_error("interaction_definition_lineage.lineage_kind"))?.as_str())?,
            source_definition_id: row.try_get("source_definition_id").map_err(db_error("interaction_definition_lineage.source_definition_id"))?,
            source_revision_id: row.try_get("source_revision_id").map_err(db_error("interaction_definition_lineage.source_revision_id"))?,
            source_bundle_digest: row.try_get("source_bundle_digest").map_err(db_error("interaction_definition_lineage.source_bundle_digest"))?,
        })).transpose()
}
async fn insert_effect(
    tx: &mut Transaction<'_, Postgres>,
    effect: &OperationEffectIntent,
) -> Result<(), InteractionError> {
    sqlx::query("INSERT INTO interaction_operation_effect_intents (effect_id,instance_id,source_event_id,status,next_attempt_at,claim_token,claim_expires_at,document) VALUES ($1,$2,$3,$4,$5,$6,$7,$8)").bind(effect.effect_id).bind(effect.instance_id).bind(effect.source_event_id).bind(effect_status(effect.status)).bind(effect.next_attempt_at).bind(effect.claim_token).bind(effect.claim_expires_at).bind(Json(to_value(effect,"operation_effect_intent")?)).execute(&mut **tx).await.map_err(db_error("interaction_operation_effect_intents"))?;
    Ok(())
}
async fn update_effect(
    tx: &mut Transaction<'_, Postgres>,
    effect: &OperationEffectIntent,
) -> Result<(), InteractionError> {
    sqlx::query("UPDATE interaction_operation_effect_intents SET status=$2,next_attempt_at=$3,claim_token=$4,claim_expires_at=$5,document=$6 WHERE effect_id=$1").bind(effect.effect_id).bind(effect_status(effect.status)).bind(effect.next_attempt_at).bind(effect.claim_token).bind(effect.claim_expires_at).bind(Json(to_value(effect,"operation_effect_intent")?)).execute(&mut **tx).await.map_err(db_error("interaction_operation_effect_intents"))?;
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
    .bind(effect_id)
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
    let event_id: Uuid = row
        .try_get("event_id")
        .map_err(db_error("interaction_command_receipts.event_id"))?;
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
    let effect_id: Option<Uuid> = row
        .try_get("effect_id")
        .map_err(db_error("interaction_command_receipts.effect_id"))?;
    let effect_intent = if let Some(id) = effect_id {
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
        .bind(id)
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
        .bind(id)
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
fn to_i64(value: u64, field: &'static str) -> Result<i64, InteractionError> {
    i64::try_from(value).map_err(|_| invalid_numeric(field))
}
fn invalid_numeric(field: &'static str) -> InteractionError {
    InteractionError::InvalidField {
        field,
        reason: "数值超出 PostgreSQL BIGINT/SMALLINT 范围",
    }
}
fn owner_parts(owner: &InteractionOwner) -> (&'static str, String) {
    match owner {
        InteractionOwner::User(id) => ("user", id.clone()),
        InteractionOwner::Project(id) => ("project", id.to_string()),
    }
}
fn attachment_subject_parts(attachment: &InteractionAttachment) -> (&'static str, String) {
    match &attachment.subject {
        AttachmentSubject::AgentRun { run_id } => ("agent_run", run_id.to_string()),
        AttachmentSubject::UserWorkshop { user_id } => ("user_workshop", user_id.clone()),
        AttachmentSubject::WorkflowRun { run_id } => ("workflow_run", run_id.to_string()),
    }
}
fn attachment_role(role: InteractionAttachmentRole) -> &'static str {
    match role {
        InteractionAttachmentRole::Editor => "editor",
        InteractionAttachmentRole::Observer => "observer",
        InteractionAttachmentRole::Renderer => "renderer",
        InteractionAttachmentRole::Automation => "automation",
    }
}
fn lineage_kind(kind: DefinitionLineageKind) -> &'static str {
    match kind {
        DefinitionLineageKind::PublishedFrom => "published_from",
        DefinitionLineageKind::CopiedFrom => "copied_from",
    }
}
fn parse_lineage_kind(value: &str) -> Result<DefinitionLineageKind, InteractionError> {
    match value {
        "published_from" => Ok(DefinitionLineageKind::PublishedFrom),
        "copied_from" => Ok(DefinitionLineageKind::CopiedFrom),
        _ => Err(InteractionError::Serialization {
            context: "interaction_definition_lineage.lineage_kind",
            message: format!("unknown lineage kind: {value}"),
        }),
    }
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

    #[test]
    fn migration_and_repository_required_columns_stay_in_sync() {
        let migration = include_str!("../../../migrations/0062_interaction_canvas_replacement.sql");
        for required in [
            "CREATE TABLE interaction_definitions",
            "project_id uuid NOT NULL",
            "CREATE TABLE interaction_source_bundles",
            "CREATE TABLE interaction_source_files",
            "CREATE TABLE interaction_definition_lineage",
            "CREATE TABLE interaction_state_revisions",
            "CREATE TABLE interaction_presentation_states",
            "CREATE TABLE interaction_renderer_leases",
            "revision bigint NOT NULL",
            "renewed_at timestamptz NOT NULL",
            "DROP TABLE IF EXISTS canvases",
            "DROP COLUMN IF EXISTS visible_canvas_mount_ids_json",
        ] {
            assert!(migration.contains(required), "0062 missing {required}");
        }
        let repository = include_str!("interaction_repository.rs");
        for required in [
            "interaction_source_bundles",
            "interaction_source_files",
            "interaction_definition_lineage",
            "interaction_state_revisions",
            "interaction_presentation_states",
            "interaction_renderer_leases",
        ] {
            assert!(
                repository.contains(required),
                "repository missing {required}"
            );
        }
    }

    #[test]
    fn source_bundle_integrity_rejects_corrupt_or_missing_files() {
        let incoming = SourceBundle::new(
            "main.tsx",
            vec![SourceFile::new("main.tsx", "content", None).expect("source")],
            SourceSandboxConfig::default(),
        )
        .expect("bundle");
        let mut corrupt = incoming.clone();
        corrupt.files[0].content = "corrupt".into();
        assert!(matches!(
            validate_stored_source_bundle(&incoming, &corrupt),
            Err(InteractionError::PersistenceConflict { .. })
        ));
        let mut missing = incoming.clone();
        missing.files.clear();
        assert!(matches!(
            validate_stored_source_bundle(&incoming, &missing),
            Err(InteractionError::PersistenceConflict { .. })
        ));
    }
}
