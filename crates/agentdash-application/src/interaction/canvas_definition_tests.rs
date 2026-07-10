use std::collections::HashMap;
use std::sync::Mutex;

use agentdash_domain::interaction::{
    InteractionDefinitionKind, SourceFile, SourceFileChange, SourceSandboxConfig,
};

use super::*;

#[derive(Default)]
struct MemoryDefinitions {
    definitions: Mutex<HashMap<Uuid, InteractionDefinition>>,
    revisions: Mutex<HashMap<Uuid, InteractionDefinitionRevision>>,
}

#[async_trait]
impl InteractionDefinitionRepository for MemoryDefinitions {
    async fn create(
        &self,
        definition: &InteractionDefinition,
        revision: &InteractionDefinitionRevision,
    ) -> Result<(), InteractionError> {
        let mut definitions = self.definitions.lock().expect("definitions");
        if definitions
            .insert(definition.id, definition.clone())
            .is_some()
        {
            return Err(InteractionError::PersistenceConflict {
                entity: "interaction_definition",
                constraint: "primary_key".into(),
            });
        }
        self.revisions
            .lock()
            .expect("revisions")
            .insert(revision.revision_id, revision.clone());
        Ok(())
    }

    async fn get(&self, id: Uuid) -> Result<Option<InteractionDefinition>, InteractionError> {
        Ok(self
            .definitions
            .lock()
            .expect("definitions")
            .get(&id)
            .cloned())
    }

    async fn get_revision(
        &self,
        id: Uuid,
    ) -> Result<Option<InteractionDefinitionRevision>, InteractionError> {
        Ok(self.revisions.lock().expect("revisions").get(&id).cloned())
    }

    async fn list_by_owner(
        &self,
        owner: &InteractionOwner,
    ) -> Result<Vec<InteractionDefinition>, InteractionError> {
        Ok(self
            .definitions
            .lock()
            .expect("definitions")
            .values()
            .filter(|definition| &definition.owner == owner)
            .cloned()
            .collect())
    }

    async fn list_canvas_by_project(
        &self,
        project_id: Uuid,
    ) -> Result<Vec<InteractionDefinition>, InteractionError> {
        Ok(self
            .definitions
            .lock()
            .expect("definitions")
            .values()
            .filter(|definition| {
                definition.project_id == project_id
                    && definition.kind == InteractionDefinitionKind::Canvas
            })
            .cloned()
            .collect())
    }

    async fn commit_revision(
        &self,
        definition_id: Uuid,
        commit: DefinitionRevisionCommit,
    ) -> Result<InteractionDefinition, InteractionError> {
        let mut definitions = self.definitions.lock().expect("definitions");
        let definition =
            definitions
                .get_mut(&definition_id)
                .ok_or_else(|| InteractionError::NotFound {
                    entity: "interaction_definition",
                    id: definition_id.to_string(),
                })?;
        if definition.current_revision_id != commit.expected_current_revision_id {
            return Err(InteractionError::DefinitionRevisionConflict {
                definition_id,
                expected_revision_id: commit.expected_current_revision_id,
                actual_revision_id: definition.current_revision_id,
            });
        }
        definition.current_revision_id = commit.revision.revision_id;
        definition.updated_at = commit.revision.created_at;
        self.revisions
            .lock()
            .expect("revisions")
            .insert(commit.revision.revision_id, commit.revision);
        Ok(definition.clone())
    }

    async fn archive(
        &self,
        definition_id: Uuid,
    ) -> Result<InteractionDefinition, InteractionError> {
        let mut definitions = self.definitions.lock().expect("definitions");
        let definition =
            definitions
                .get_mut(&definition_id)
                .ok_or_else(|| InteractionError::NotFound {
                    entity: "interaction_definition",
                    id: definition_id.to_string(),
                })?;
        definition.status = InteractionDefinitionStatus::Archived;
        Ok(definition.clone())
    }
}

struct FullAccess;

#[async_trait]
impl CanvasDefinitionAccessResolver for FullAccess {
    async fn resolve_project_access(
        &self,
        _: Uuid,
        _: &str,
    ) -> InteractionApplicationResult<CanvasProjectAccess> {
        Ok(CanvasProjectAccess {
            can_use: true,
            can_configure: true,
            can_manage_sharing: true,
        })
    }
}

fn source(content: &str) -> SourceBundle {
    SourceBundle::new(
        "main.tsx",
        vec![SourceFile::new("main.tsx", content, None).expect("source")],
        SourceSandboxConfig::default(),
    )
    .expect("bundle")
}

fn create_input(project_id: Uuid) -> CreateCanvasDefinitionInput {
    CreateCanvasDefinitionInput {
        project_id,
        title: "Dashboard".into(),
        description: String::new(),
        source_bundle: source("v1"),
        initial_state: serde_json::json!({}),
        state_schema: serde_json::json!({"type":"object"}),
        command_definitions: vec![],
        component_bindings: vec![],
        resource_slots: vec![],
    }
}

fn service() -> (CanvasDefinitionService, Arc<MemoryDefinitions>) {
    let definitions = Arc::new(MemoryDefinitions::default());
    (
        CanvasDefinitionService::new(definitions.clone(), Arc::new(FullAccess)),
        definitions,
    )
}

#[tokio::test]
async fn changeset_uses_current_revision_cas_and_rejects_noop() {
    let (service, _) = service();
    let created = service
        .create_personal(create_input(Uuid::new_v4()), "u")
        .await
        .expect("create");
    let changed = service
        .commit_changeset(
            CommitCanvasDefinitionInput {
                definition_id: created.definition.id,
                base_revision_id: created.revision.revision_id,
                title: None,
                description: None,
                changeset: SourceBundleChangeset {
                    file_changes: vec![SourceFileChange::Upsert(
                        SourceFile::new("main.tsx", "v2", None).expect("source"),
                    )],
                    ..SourceBundleChangeset::default()
                },
                command_definitions: None,
                component_bindings: None,
                resource_slots: None,
            },
            "u",
            Utc::now(),
        )
        .await
        .expect("commit");
    assert_eq!(changed.revision.revision_number, 2);
    let no_op = service
        .commit_changeset(
            CommitCanvasDefinitionInput {
                definition_id: changed.definition.id,
                base_revision_id: changed.revision.revision_id,
                title: None,
                description: None,
                changeset: SourceBundleChangeset::default(),
                command_definitions: None,
                component_bindings: None,
                resource_slots: None,
            },
            "u",
            Utc::now(),
        )
        .await;
    assert!(matches!(
        no_op,
        Err(InteractionApplicationError::InvalidCommand {
            field: "definition_changeset",
            ..
        })
    ));
}

#[tokio::test]
async fn publish_copy_and_unpublish_pin_exact_lineage() {
    let (service, _) = service();
    let personal = service
        .create_personal(create_input(Uuid::new_v4()), "u")
        .await
        .expect("personal");
    let shared = service
        .publish(
            PublishCanvasDefinitionInput {
                source_definition_id: personal.definition.id,
                source_revision_id: personal.revision.revision_id,
                title: None,
                description: None,
            },
            "u",
            Utc::now(),
        )
        .await
        .expect("publish");
    assert!(matches!(
        shared.definition.owner,
        InteractionOwner::Project(_)
    ));
    assert_eq!(
        shared
            .revision
            .lineage
            .as_ref()
            .map(|lineage| lineage.source_revision_id),
        Some(personal.revision.revision_id)
    );
    let copied = service
        .copy_to_personal(
            CopyCanvasDefinitionInput {
                source_definition_id: shared.definition.id,
                source_revision_id: shared.revision.revision_id,
                title: Some("My Copy".into()),
                description: None,
            },
            "u",
            Utc::now(),
        )
        .await
        .expect("copy");
    assert!(matches!(copied.definition.owner, InteractionOwner::User(_)));
    assert_eq!(
        copied.revision.lineage.as_ref().map(|lineage| lineage.kind),
        Some(DefinitionLineageKind::CopiedFrom)
    );
    let archived = service
        .unpublish(shared.definition.id, "u")
        .await
        .expect("unpublish");
    assert_eq!(archived.status, InteractionDefinitionStatus::Archived);
}
