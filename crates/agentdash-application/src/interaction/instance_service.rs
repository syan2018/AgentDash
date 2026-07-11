use std::collections::BTreeSet;
use std::sync::Arc;

use agentdash_domain::interaction::{
    AttachmentCapabilityProjection, AttachmentSubject, InteractionAttachment,
    InteractionAttachmentRole, InteractionDefinitionRepository, InteractionDefinitionStatus,
    InteractionError, InteractionInstance, InteractionInstanceRepository, InteractionOwner,
    InteractionRetention, InteractionRuntimeBinding, PinnedArtifact,
    RuntimeBindingAuthorizationRef, RuntimeBindingTarget,
};
use async_trait::async_trait;
use chrono::Utc;
use uuid::Uuid;

use super::{InteractionApplicationError, InteractionApplicationResult};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InteractionInstanceAccess {
    pub can_view: bool,
    pub can_create: bool,
    pub can_close: bool,
    pub authorization_ref: String,
}

#[async_trait]
pub trait InteractionInstanceAccessResolver: Send + Sync {
    async fn resolve(
        &self,
        owner: &InteractionOwner,
        project_id: Uuid,
        user_id: &str,
    ) -> InteractionApplicationResult<InteractionInstanceAccess>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedComponentArtifact {
    pub artifact_id: Uuid,
    pub archive_digest: String,
}

#[async_trait]
pub trait InteractionComponentArtifactResolver: Send + Sync {
    async fn resolve(
        &self,
        project_id: Uuid,
        component_ref: &str,
    ) -> InteractionApplicationResult<ResolvedComponentArtifact>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateInteractionInstanceInput {
    pub definition_id: Uuid,
    pub definition_revision_id: Uuid,
}

#[derive(Debug, Clone, PartialEq)]
pub struct InteractionInstanceView {
    pub instance: InteractionInstance,
    pub runtime_bindings: Vec<InteractionRuntimeBinding>,
}

#[derive(Clone)]
pub struct InteractionInstanceService {
    definitions: Arc<dyn InteractionDefinitionRepository>,
    instances: Arc<dyn InteractionInstanceRepository>,
    access: Arc<dyn InteractionInstanceAccessResolver>,
    artifacts: Arc<dyn InteractionComponentArtifactResolver>,
}

impl InteractionInstanceService {
    pub fn new(
        definitions: Arc<dyn InteractionDefinitionRepository>,
        instances: Arc<dyn InteractionInstanceRepository>,
        access: Arc<dyn InteractionInstanceAccessResolver>,
        artifacts: Arc<dyn InteractionComponentArtifactResolver>,
    ) -> Self {
        Self {
            definitions,
            instances,
            access,
            artifacts,
        }
    }

    pub async fn create(
        &self,
        input: CreateInteractionInstanceInput,
        user_id: &str,
    ) -> InteractionApplicationResult<InteractionInstanceView> {
        let definition = self
            .definitions
            .get(input.definition_id)
            .await?
            .ok_or_else(|| InteractionError::NotFound {
                entity: "interaction_definition",
                id: input.definition_id.to_string(),
            })?;
        if definition.status != InteractionDefinitionStatus::Active
            || definition.current_revision_id != input.definition_revision_id
        {
            return Err(InteractionApplicationError::ContractUnavailable {
                reason: "只能从 active exact current definition revision 创建 instance".into(),
            });
        }
        let revision = self
            .definitions
            .get_revision(input.definition_revision_id)
            .await?
            .ok_or_else(|| InteractionError::NotFound {
                entity: "interaction_definition_revision",
                id: input.definition_revision_id.to_string(),
            })?;
        if revision.definition_id != definition.id || revision.owner != definition.owner {
            return Err(InteractionApplicationError::ContractUnavailable {
                reason: "definition revision identity 不一致".into(),
            });
        }
        let access = self
            .access
            .resolve(&definition.owner, definition.project_id, user_id)
            .await?;
        require(access.can_create, "当前用户不可创建 Interaction instance")?;

        let mut pinned = Vec::new();
        let mut resolved = Vec::new();
        let mut artifact_keys = BTreeSet::new();
        for component in &revision.component_bindings {
            let artifact = self
                .artifacts
                .resolve(definition.project_id, &component.component_ref)
                .await?;
            let artifact_ref = artifact.artifact_id.to_string();
            if artifact_keys.insert((artifact_ref.clone(), artifact.archive_digest.clone())) {
                pinned.push(PinnedArtifact {
                    artifact_ref: artifact_ref.clone(),
                    digest: artifact.archive_digest.clone(),
                });
            }
            resolved.push((
                component.binding_key.clone(),
                artifact_ref,
                artifact.archive_digest,
            ));
        }
        let mut instance = InteractionInstance::new_v1(
            definition.owner,
            definition.id,
            revision.revision_id,
            revision.initial_state,
            InteractionRetention { retain_until: None },
        )?;
        instance.pinned_artifacts = pinned;
        self.instances.create(&instance).await?;

        let attachment = InteractionAttachment {
            id: Uuid::new_v4(),
            instance_id: instance.id,
            subject: AttachmentSubject::UserWorkshop {
                user_id: user_id.to_owned(),
            },
            role: InteractionAttachmentRole::Editor,
            capabilities: AttachmentCapabilityProjection::for_role(
                InteractionAttachmentRole::Editor,
            ),
            created_at: Utc::now(),
            detached_at: None,
        };
        attachment.validate()?;
        self.instances.attach(&attachment).await?;

        let mut bindings = Vec::new();
        for (binding_key, artifact_ref, digest) in resolved {
            let binding = InteractionRuntimeBinding {
                id: Uuid::new_v4(),
                instance_id: instance.id,
                attachment_id: None,
                slot_key: format!("component:{binding_key}"),
                target: RuntimeBindingTarget::Artifact {
                    artifact_ref,
                    digest,
                },
                authorization: RuntimeBindingAuthorizationRef {
                    grant_ref: access.authorization_ref.clone(),
                    revision: 1,
                },
                created_at: Utc::now(),
            };
            binding.validate()?;
            self.instances.upsert_runtime_binding(&binding).await?;
            bindings.push(binding);
        }
        Ok(InteractionInstanceView {
            instance,
            runtime_bindings: bindings,
        })
    }

    pub async fn get(
        &self,
        instance_id: Uuid,
        user_id: &str,
    ) -> InteractionApplicationResult<InteractionInstanceView> {
        let instance = self.required_instance(instance_id).await?;
        let revision = self
            .definitions
            .get_revision(instance.definition_revision_id)
            .await?
            .ok_or_else(|| InteractionError::NotFound {
                entity: "interaction_definition_revision",
                id: instance.definition_revision_id.to_string(),
            })?;
        let access = self
            .access
            .resolve(&instance.owner, revision.project_id, user_id)
            .await?;
        require(access.can_view, "当前用户不可查看 Interaction instance")?;
        let runtime_bindings = self
            .instances
            .list_runtime_bindings(instance.id, None)
            .await?;
        Ok(InteractionInstanceView {
            instance,
            runtime_bindings,
        })
    }

    pub async fn list_project(
        &self,
        project_id: Uuid,
        user_id: &str,
    ) -> InteractionApplicationResult<Vec<InteractionInstanceView>> {
        let mut instances = self
            .instances
            .list_by_owner(&InteractionOwner::Project(project_id))
            .await?;
        instances.extend(
            self.instances
                .list_by_owner(&InteractionOwner::User(user_id.to_owned()))
                .await?,
        );
        let mut views = Vec::new();
        for instance in instances {
            let Some(revision) = self
                .definitions
                .get_revision(instance.definition_revision_id)
                .await?
            else {
                continue;
            };
            if revision.project_id != project_id {
                continue;
            }
            let access = self
                .access
                .resolve(&instance.owner, project_id, user_id)
                .await?;
            if access.can_view {
                let runtime_bindings = self
                    .instances
                    .list_runtime_bindings(instance.id, None)
                    .await?;
                views.push(InteractionInstanceView {
                    instance,
                    runtime_bindings,
                });
            }
        }
        views.sort_by_key(|view| std::cmp::Reverse(view.instance.updated_at));
        Ok(views)
    }

    async fn required_instance(
        &self,
        instance_id: Uuid,
    ) -> InteractionApplicationResult<InteractionInstance> {
        self.instances.get(instance_id).await?.ok_or_else(|| {
            InteractionError::NotFound {
                entity: "interaction_instance",
                id: instance_id.to_string(),
            }
            .into()
        })
    }
}

fn require(value: bool, reason: &'static str) -> InteractionApplicationResult<()> {
    value
        .then_some(())
        .ok_or_else(|| InteractionApplicationError::AccessDenied {
            reason: reason.to_string(),
        })
}
