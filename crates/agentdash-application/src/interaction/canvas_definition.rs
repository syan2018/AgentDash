use std::sync::Arc;

use agentdash_domain::interaction::{
    ComponentBinding, DefinitionLineage, DefinitionLineageKind, DefinitionRevisionCommit,
    InteractionCommandDefinition, InteractionDefinition, InteractionDefinitionAccess,
    InteractionDefinitionRepository, InteractionDefinitionRevision, InteractionDefinitionStatus,
    InteractionError, InteractionOwner, ResourceSlotDefinition, SourceBundle,
    SourceBundleChangeset,
};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde_json::Value;
use uuid::Uuid;

use super::{InteractionApplicationError, InteractionApplicationResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CanvasProjectAccess {
    pub can_use: bool,
    pub can_configure: bool,
    pub can_manage_sharing: bool,
}

/// Trusted host projection; browser input cannot construct project authority.
#[async_trait]
pub trait CanvasDefinitionAccessResolver: Send + Sync {
    async fn resolve_project_access(
        &self,
        project_id: Uuid,
        user_id: &str,
    ) -> InteractionApplicationResult<CanvasProjectAccess>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CanvasDefinitionListScope {
    All,
    Mine,
    Shared,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CanvasDefinitionView {
    pub definition: InteractionDefinition,
    pub revision: InteractionDefinitionRevision,
    pub access: InteractionDefinitionAccess,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CreateCanvasDefinitionInput {
    pub project_id: Uuid,
    pub title: String,
    pub description: String,
    pub source_bundle: SourceBundle,
    pub initial_state: Value,
    pub state_schema: Value,
    pub command_definitions: Vec<InteractionCommandDefinition>,
    pub component_bindings: Vec<ComponentBinding>,
    pub resource_slots: Vec<ResourceSlotDefinition>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CommitCanvasDefinitionInput {
    pub definition_id: Uuid,
    pub base_revision_id: Uuid,
    pub title: Option<String>,
    pub description: Option<String>,
    pub changeset: SourceBundleChangeset,
    pub command_definitions: Option<Vec<InteractionCommandDefinition>>,
    pub component_bindings: Option<Vec<ComponentBinding>>,
    pub resource_slots: Option<Vec<ResourceSlotDefinition>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublishCanvasDefinitionInput {
    pub source_definition_id: Uuid,
    pub source_revision_id: Uuid,
    pub title: Option<String>,
    pub description: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CopyCanvasDefinitionInput {
    pub source_definition_id: Uuid,
    pub source_revision_id: Uuid,
    pub title: Option<String>,
    pub description: Option<String>,
}

#[derive(Clone)]
pub struct CanvasDefinitionService {
    definitions: Arc<dyn InteractionDefinitionRepository>,
    access: Arc<dyn CanvasDefinitionAccessResolver>,
}

impl CanvasDefinitionService {
    pub fn new(
        definitions: Arc<dyn InteractionDefinitionRepository>,
        access: Arc<dyn CanvasDefinitionAccessResolver>,
    ) -> Self {
        Self {
            definitions,
            access,
        }
    }

    pub async fn create_personal(
        &self,
        input: CreateCanvasDefinitionInput,
        user_id: &str,
    ) -> InteractionApplicationResult<CanvasDefinitionView> {
        require_non_empty("title", &input.title)?;
        self.require_project(input.project_id, user_id, |access| access.can_configure)
            .await?;
        let definition_id = Uuid::new_v4();
        let mut revision = InteractionDefinitionRevision::new_canvas_v1(
            definition_id,
            1,
            input.project_id,
            InteractionOwner::User(user_id.to_owned()),
            input.title,
            input.description,
            input.source_bundle,
            input.initial_state,
            input.state_schema,
            user_id,
        )?;
        revision.command_definitions = input.command_definitions;
        revision.component_bindings = input.component_bindings;
        revision.resource_slots = input.resource_slots;
        revision.validate()?;
        let (definition, revision) = revision.into_initial_definition()?;
        self.definitions.create(&definition, &revision).await?;
        self.view(definition, revision, user_id).await
    }

    pub async fn list(
        &self,
        project_id: Uuid,
        scope: CanvasDefinitionListScope,
        user_id: &str,
    ) -> InteractionApplicationResult<Vec<CanvasDefinitionView>> {
        self.require_project(project_id, user_id, |access| access.can_use)
            .await?;
        let definitions = self.definitions.list_canvas_by_project(project_id).await?;
        let mut views = Vec::new();
        for definition in definitions {
            let include = match (&definition.owner, scope) {
                (InteractionOwner::User(owner), CanvasDefinitionListScope::Mine) => {
                    owner == user_id
                }
                (InteractionOwner::Project(_), CanvasDefinitionListScope::Shared) => true,
                (InteractionOwner::User(owner), CanvasDefinitionListScope::All) => owner == user_id,
                (InteractionOwner::Project(_), CanvasDefinitionListScope::All) => true,
                _ => false,
            };
            if !include || definition.status != InteractionDefinitionStatus::Active {
                continue;
            }
            let revision = self
                .required_revision(definition.current_revision_id)
                .await?;
            let view = self.view(definition, revision, user_id).await?;
            if view.access.can_view {
                views.push(view);
            }
        }
        views.sort_by_key(|view| std::cmp::Reverse(view.definition.updated_at));
        Ok(views)
    }

    pub async fn get(
        &self,
        definition_id: Uuid,
        user_id: &str,
    ) -> InteractionApplicationResult<CanvasDefinitionView> {
        let definition = self.required_definition(definition_id).await?;
        let revision = self
            .required_revision(definition.current_revision_id)
            .await?;
        let view = self.view(definition, revision, user_id).await?;
        require_access(view.access.can_view, "当前用户不可查看 Canvas definition")?;
        Ok(view)
    }

    pub async fn commit_changeset(
        &self,
        input: CommitCanvasDefinitionInput,
        user_id: &str,
        now: DateTime<Utc>,
    ) -> InteractionApplicationResult<CanvasDefinitionView> {
        let definition = self.required_definition(input.definition_id).await?;
        require_access(
            definition.status == InteractionDefinitionStatus::Active,
            "archived definition 不可编辑",
        )?;
        let current = self
            .required_revision(definition.current_revision_id)
            .await?;
        require_access(
            input.base_revision_id == current.revision_id,
            "base revision 不是当前 revision",
        )?;
        let access = self
            .access_for(&definition, Some(&current), user_id)
            .await?;
        require_access(access.can_edit_source, "当前用户不可编辑 Canvas source")?;
        let mut revision = current.clone();
        revision.revision_id = Uuid::new_v4();
        revision.revision_number = revision.revision_number.checked_add(1).ok_or(
            InteractionApplicationError::InvalidCommand {
                field: "definition_revision.revision_number",
                reason: "revision number 已达上限".into(),
            },
        )?;
        revision.title = input.title.unwrap_or(revision.title);
        revision.description = input.description.unwrap_or(revision.description);
        revision.source_bundle = current.source_bundle.apply_changeset(input.changeset)?;
        revision.command_definitions = input
            .command_definitions
            .unwrap_or_else(|| current.command_definitions.clone());
        revision.component_bindings = input
            .component_bindings
            .unwrap_or_else(|| current.component_bindings.clone());
        revision.resource_slots = input
            .resource_slots
            .unwrap_or_else(|| current.resource_slots.clone());
        if revision.source_bundle.digest == current.source_bundle.digest
            && revision.title == current.title
            && revision.description == current.description
            && revision.command_definitions == current.command_definitions
            && revision.component_bindings == current.component_bindings
            && revision.resource_slots == current.resource_slots
        {
            return Err(InteractionApplicationError::InvalidCommand {
                field: "definition_changeset",
                reason: "changeset 未产生新的 definition revision 内容".into(),
            });
        }
        revision.created_by = user_id.to_owned();
        revision.created_at = now;
        revision.validate()?;
        let definition = self
            .definitions
            .commit_revision(
                definition.id,
                DefinitionRevisionCommit {
                    expected_current_revision_id: current.revision_id,
                    revision: revision.clone(),
                },
            )
            .await?;
        self.view(definition, revision, user_id).await
    }

    pub async fn publish(
        &self,
        input: PublishCanvasDefinitionInput,
        user_id: &str,
        now: DateTime<Utc>,
    ) -> InteractionApplicationResult<CanvasDefinitionView> {
        let source_definition = self.required_definition(input.source_definition_id).await?;
        require_access(
            matches!(&source_definition.owner, InteractionOwner::User(owner) if owner == user_id),
            "只有 owner 可以发布 Personal Canvas",
        )?;
        let source = self.required_revision(input.source_revision_id).await?;
        require_revision_membership(&source_definition, &source)?;
        self.require_project(source.project_id, user_id, |access| access.can_configure)
            .await?;

        let existing = self
            .find_active_published_definition(source.project_id, source.definition_id)
            .await?;
        let lineage = DefinitionLineage {
            kind: DefinitionLineageKind::PublishedFrom,
            source_definition_id: source.definition_id,
            source_revision_id: source.revision_id,
            source_bundle_digest: source.source_bundle.digest.clone(),
        };
        let (definition, revision) = if let Some(definition) = existing {
            let current = self
                .required_revision(definition.current_revision_id)
                .await?;
            let revision_number = current.revision_number.checked_add(1).ok_or(
                InteractionApplicationError::InvalidCommand {
                    field: "definition_revision.revision_number",
                    reason: "revision number 已达上限".into(),
                },
            )?;
            let revision = copy_revision(
                &source,
                definition.id,
                revision_number,
                InteractionOwner::Project(source.project_id),
                lineage,
                input.title,
                input.description,
                user_id,
                now,
            )?;
            let definition = self
                .definitions
                .commit_revision(
                    definition.id,
                    DefinitionRevisionCommit {
                        expected_current_revision_id: current.revision_id,
                        revision: revision.clone(),
                    },
                )
                .await?;
            (definition, revision)
        } else {
            let definition_id = Uuid::new_v4();
            let revision = copy_revision(
                &source,
                definition_id,
                1,
                InteractionOwner::Project(source.project_id),
                lineage,
                input.title,
                input.description,
                user_id,
                now,
            )?;
            let (definition, revision) = revision.into_initial_definition()?;
            self.definitions.create(&definition, &revision).await?;
            (definition, revision)
        };
        self.view(definition, revision, user_id).await
    }

    pub async fn copy_to_personal(
        &self,
        input: CopyCanvasDefinitionInput,
        user_id: &str,
        now: DateTime<Utc>,
    ) -> InteractionApplicationResult<CanvasDefinitionView> {
        let source_definition = self.required_definition(input.source_definition_id).await?;
        let source = self.required_revision(input.source_revision_id).await?;
        require_revision_membership(&source_definition, &source)?;
        let source_access = self
            .access_for(&source_definition, Some(&source), user_id)
            .await?;
        require_access(source_access.can_copy, "当前用户不可复制 Canvas")?;
        let definition_id = Uuid::new_v4();
        let revision = copy_revision(
            &source,
            definition_id,
            1,
            InteractionOwner::User(user_id.to_owned()),
            DefinitionLineage {
                kind: DefinitionLineageKind::CopiedFrom,
                source_definition_id: source.definition_id,
                source_revision_id: source.revision_id,
                source_bundle_digest: source.source_bundle.digest.clone(),
            },
            input.title,
            input.description,
            user_id,
            now,
        )?;
        let (definition, revision) = revision.into_initial_definition()?;
        self.definitions.create(&definition, &revision).await?;
        self.view(definition, revision, user_id).await
    }

    pub async fn unpublish(
        &self,
        definition_id: Uuid,
        user_id: &str,
    ) -> InteractionApplicationResult<InteractionDefinition> {
        let definition = self.required_definition(definition_id).await?;
        require_access(
            matches!(definition.owner, InteractionOwner::Project(_)),
            "只有 Project Canvas 可以取消发布",
        )?;
        let revision = self
            .required_revision(definition.current_revision_id)
            .await?;
        let access = self
            .access_for(&definition, Some(&revision), user_id)
            .await?;
        require_access(access.can_manage_shared, "当前用户不可管理 Project Canvas")?;
        Ok(self.definitions.archive(definition.id).await?)
    }

    pub async fn archive(
        &self,
        definition_id: Uuid,
        user_id: &str,
    ) -> InteractionApplicationResult<InteractionDefinition> {
        let definition = self.required_definition(definition_id).await?;
        let revision = self
            .required_revision(definition.current_revision_id)
            .await?;
        let access = self
            .access_for(&definition, Some(&revision), user_id)
            .await?;
        require_access(
            access.can_edit_source || access.can_manage_shared,
            "当前用户不可归档 Canvas definition",
        )?;
        Ok(self.definitions.archive(definition.id).await?)
    }

    async fn view(
        &self,
        definition: InteractionDefinition,
        revision: InteractionDefinitionRevision,
        user_id: &str,
    ) -> InteractionApplicationResult<CanvasDefinitionView> {
        require_revision_membership(&definition, &revision)?;
        let access = self
            .access_for(&definition, Some(&revision), user_id)
            .await?;
        Ok(CanvasDefinitionView {
            definition,
            revision,
            access,
        })
    }

    async fn access_for(
        &self,
        definition: &InteractionDefinition,
        revision: Option<&InteractionDefinitionRevision>,
        user_id: &str,
    ) -> InteractionApplicationResult<InteractionDefinitionAccess> {
        let project = self
            .access
            .resolve_project_access(definition.project_id, user_id)
            .await?;
        Ok(match &definition.owner {
            InteractionOwner::User(owner) if owner == user_id && project.can_use => {
                InteractionDefinitionAccess {
                    can_view: true,
                    can_edit_source: true,
                    can_publish: project.can_configure,
                    can_manage_shared: false,
                    can_copy: true,
                }
            }
            InteractionOwner::Project(_) if project.can_use => InteractionDefinitionAccess {
                can_view: true,
                can_edit_source: false,
                can_publish: false,
                can_manage_shared: project.can_manage_sharing
                    || revision.is_some_and(|revision| revision.created_by == user_id),
                can_copy: true,
            },
            _ => InteractionDefinitionAccess::default(),
        })
    }

    async fn require_project(
        &self,
        project_id: Uuid,
        user_id: &str,
        predicate: impl FnOnce(CanvasProjectAccess) -> bool,
    ) -> InteractionApplicationResult<()> {
        let access = self
            .access
            .resolve_project_access(project_id, user_id)
            .await?;
        require_access(predicate(access), "当前用户无 Project 权限")
    }

    async fn required_definition(
        &self,
        id: Uuid,
    ) -> InteractionApplicationResult<InteractionDefinition> {
        self.definitions.get(id).await?.ok_or_else(|| {
            InteractionError::NotFound {
                entity: "interaction_definition",
                id: id.to_string(),
            }
            .into()
        })
    }

    async fn required_revision(
        &self,
        id: Uuid,
    ) -> InteractionApplicationResult<InteractionDefinitionRevision> {
        self.definitions.get_revision(id).await?.ok_or_else(|| {
            InteractionError::NotFound {
                entity: "interaction_definition_revision",
                id: id.to_string(),
            }
            .into()
        })
    }

    async fn find_active_published_definition(
        &self,
        project_id: Uuid,
        source_definition_id: Uuid,
    ) -> InteractionApplicationResult<Option<InteractionDefinition>> {
        for definition in self.definitions.list_canvas_by_project(project_id).await? {
            if definition.status != InteractionDefinitionStatus::Active
                || !matches!(definition.owner, InteractionOwner::Project(_))
            {
                continue;
            }
            let revision = self
                .required_revision(definition.current_revision_id)
                .await?;
            if revision.lineage.as_ref().is_some_and(|lineage| {
                lineage.kind == DefinitionLineageKind::PublishedFrom
                    && lineage.source_definition_id == source_definition_id
            }) {
                return Ok(Some(definition));
            }
        }
        Ok(None)
    }
}

#[allow(clippy::too_many_arguments)]
fn copy_revision(
    source: &InteractionDefinitionRevision,
    definition_id: Uuid,
    revision_number: u64,
    owner: InteractionOwner,
    lineage: DefinitionLineage,
    title: Option<String>,
    description: Option<String>,
    created_by: &str,
    now: DateTime<Utc>,
) -> InteractionApplicationResult<InteractionDefinitionRevision> {
    let mut revision = source.clone();
    revision.definition_id = definition_id;
    revision.revision_id = Uuid::new_v4();
    revision.revision_number = revision_number;
    revision.owner = owner;
    revision.title = title.unwrap_or(revision.title);
    revision.description = description.unwrap_or(revision.description);
    revision.lineage = Some(lineage);
    revision.created_by = created_by.to_owned();
    revision.created_at = now;
    revision.validate()?;
    Ok(revision)
}

fn require_revision_membership(
    definition: &InteractionDefinition,
    revision: &InteractionDefinitionRevision,
) -> InteractionApplicationResult<()> {
    if revision.definition_id == definition.id
        && revision.project_id == definition.project_id
        && revision.owner == definition.owner
        && revision.kind == definition.kind
    {
        Ok(())
    } else {
        Err(InteractionApplicationError::ContractUnavailable {
            reason: "definition/revision identity 不一致".into(),
        })
    }
}

fn require_access(allowed: bool, reason: &'static str) -> InteractionApplicationResult<()> {
    if allowed {
        Ok(())
    } else {
        Err(InteractionApplicationError::AccessDenied {
            reason: reason.into(),
        })
    }
}

fn require_non_empty(field: &'static str, value: &str) -> InteractionApplicationResult<()> {
    if value.trim().is_empty() {
        Err(InteractionApplicationError::InvalidCommand {
            field,
            reason: "不能为空".into(),
        })
    } else {
        Ok(())
    }
}

#[cfg(test)]
#[path = "canvas_definition_tests.rs"]
mod tests;
