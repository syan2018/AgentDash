use std::sync::Arc;

use agentdash_application_operation_gateway::{
    OperationDescriptor, OperationGateway, OperationInvocationCommand, OperationPrincipal,
    OperationResultValue, OperationTraceContext,
};
use agentdash_application_ports::product_runtime_tool::ProductRuntimeToolOutcome;
use agentdash_contracts::workspace_module::{
    WorkspaceModuleAgentStateProjection, WorkspaceModuleDescriptor, WorkspaceModuleKind,
    WorkspaceModuleStatus, WorkspaceModuleSummary, WorkspaceModuleUiEntry,
};
use agentdash_domain::interaction::{
    AttachmentCapabilityProjection, AttachmentSubject, InteractionAttachment,
    InteractionAttachmentRole, InteractionDefinitionRepository, InteractionDefinitionRevision,
    InteractionDefinitionStatus, InteractionError, InteractionInstance,
    InteractionInstanceRepository, InteractionInstanceStatus, InteractionOwner,
    InteractionRetention,
};
use agentdash_domain::operation::{OperationOriginRef, OperationPrincipalRef, OperationScopeRef};
use async_trait::async_trait;
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

use crate::canvas::operation_provider::canvas_authoring_operation_ref;
use crate::product::{
    WorkspaceModuleActor, WorkspaceModuleOperateOutcome, WorkspaceModuleOperateRequest,
    WorkspaceModulePresentationPreparation, WorkspaceModulePresentationRequest,
    WorkspaceModuleProvider, WorkspaceModuleProviderContext, WorkspaceModuleSurfaceEffect,
    workspace_module_operation_from_descriptor,
};

pub struct CanvasWorkspaceModuleProvider {
    definitions: Arc<dyn InteractionDefinitionRepository>,
    instances: Arc<dyn InteractionInstanceRepository>,
    operation_gateway: Arc<OperationGateway>,
}

impl CanvasWorkspaceModuleProvider {
    pub fn new(
        definitions: Arc<dyn InteractionDefinitionRepository>,
        instances: Arc<dyn InteractionInstanceRepository>,
        operation_gateway: Arc<OperationGateway>,
    ) -> Self {
        Self {
            definitions,
            instances,
            operation_gateway,
        }
    }

    fn authoring_mount_is_applied(
        context: &WorkspaceModuleProviderContext,
        revision: &InteractionDefinitionRevision,
    ) -> bool {
        !matches!(&context.actor, WorkspaceModuleActor::AgentRunAgent { .. })
            || context.vfs_mounts.iter().any(|mount| {
                mount.mount_id == revision.authoring_mount_id
                    && mount.provider == "canvas_fs"
                    && mount.backend_id == revision.definition_id.to_string()
            })
    }

    async fn visible_definitions(
        &self,
        context: &WorkspaceModuleProviderContext,
    ) -> Result<Vec<InteractionDefinitionRevision>, ProductRuntimeToolOutcome> {
        let definitions = self
            .definitions
            .list_canvas_by_project(context.project_id)
            .await
            .map_err(|error| failed("workspace_module_interaction_query_failed", error))?;
        let mut revisions = Vec::new();
        for definition in definitions {
            if definition.status != InteractionDefinitionStatus::Active {
                continue;
            }
            let visible_owner = match &definition.owner {
                InteractionOwner::Project(owner) => *owner == context.project_id,
                InteractionOwner::User(owner) => owner == context.user_id(),
            };
            let module_id = format!("canvas:{}", definition.id);
            if !visible_owner || !context.visibility.allows(&module_id) {
                continue;
            }
            let revision = self
                .definitions
                .get_revision(definition.current_revision_id)
                .await
                .map_err(|error| failed("workspace_module_interaction_query_failed", error))?
                .ok_or_else(|| {
                    failed(
                        "workspace_module_interaction_revision_missing",
                        "InteractionDefinition current revision is missing",
                    )
                })?;
            if !Self::authoring_mount_is_applied(context, &revision) {
                continue;
            }
            revisions.push(revision);
        }
        Ok(revisions)
    }

    async fn runtime_modules(
        &self,
        context: &WorkspaceModuleProviderContext,
    ) -> Result<Vec<WorkspaceModuleDescriptor>, ProductRuntimeToolOutcome> {
        let owner = InteractionOwner::Project(context.project_id);
        let instances = self
            .instances
            .list_by_owner(&owner)
            .await
            .map_err(|error| failed("workspace_module_instance_query_failed", error))?;
        let subject = match &context.actor {
            WorkspaceModuleActor::AgentRunAgent { target, .. } => AttachmentSubject::AgentRun {
                run_id: target.run_id,
                agent_id: target.agent_id,
            },
            WorkspaceModuleActor::User { user_id } => AttachmentSubject::UserWorkshop {
                user_id: user_id.clone(),
            },
        };
        let mut modules = Vec::new();
        for instance in instances
            .into_iter()
            .filter(|instance| instance.status == InteractionInstanceStatus::Open)
        {
            let attached = self
                .instances
                .list_attachments(instance.id)
                .await
                .map_err(|error| failed("workspace_module_attachment_query_failed", error))?
                .into_iter()
                .any(|attachment| {
                    attachment.detached_at.is_none() && attachment.subject == subject
                });
            if !attached {
                continue;
            }
            let revision = self
                .definitions
                .get_revision(instance.definition_revision_id)
                .await
                .map_err(|error| failed("workspace_module_interaction_query_failed", error))?
                .ok_or_else(|| {
                    failed(
                        "workspace_module_interaction_revision_missing",
                        "Interaction instance pinned definition revision is missing",
                    )
                })?;
            if revision.definition_id != instance.definition_id
                || revision.project_id != context.project_id
            {
                return Err(rejected(
                    "workspace_module_interaction_identity_mismatch",
                    "Interaction instance does not belong to the authorized Product surface",
                ));
            }
            if !Self::authoring_mount_is_applied(context, &revision) {
                continue;
            }
            if !context
                .visibility
                .allows(&format!("canvas:{}", revision.definition_id))
            {
                continue;
            }
            modules.push(canvas_runtime_module(
                &instance,
                &revision,
                &context.operations,
            )?);
        }
        Ok(modules)
    }

    async fn attach_presentation(
        &self,
        context: &WorkspaceModuleProviderContext,
        revision: &InteractionDefinitionRevision,
    ) -> Result<PresentationTarget, ProductRuntimeToolOutcome> {
        let definition = self
            .definitions
            .get(revision.definition_id)
            .await
            .map_err(|error| failed("workspace_module_definition_query_failed", error))?
            .ok_or_else(|| {
                rejected(
                    "workspace_module_definition_not_found",
                    "Canvas InteractionDefinition does not exist",
                )
            })?;
        if definition.current_revision_id != revision.revision_id
            || definition.project_id != context.project_id
        {
            return Err(rejected(
                "workspace_module_definition_revision_mismatch",
                "Canvas definition revision does not belong to the authorized Product surface",
            ));
        }
        let owner = InteractionOwner::Project(context.project_id);
        let instance = match self
            .instances
            .list_by_owner(&owner)
            .await
            .map_err(|error| failed("workspace_module_instance_query_failed", error))?
            .into_iter()
            .find(|instance| {
                instance.status == InteractionInstanceStatus::Open
                    && instance.definition_revision_id == revision.revision_id
            }) {
            Some(instance) => instance,
            None => {
                let instance = InteractionInstance::new_v1(
                    owner,
                    revision.definition_id,
                    revision.revision_id,
                    revision.initial_state.clone(),
                    InteractionRetention { retain_until: None },
                )
                .map_err(|error| failed("workspace_module_instance_invalid", error))?;
                self.instances
                    .create(&instance)
                    .await
                    .map_err(|error| failed("workspace_module_instance_create_failed", error))?;
                instance
            }
        };
        let subject = match &context.actor {
            WorkspaceModuleActor::AgentRunAgent { target, .. } => AttachmentSubject::AgentRun {
                run_id: target.run_id,
                agent_id: target.agent_id,
            },
            WorkspaceModuleActor::User { user_id } => AttachmentSubject::UserWorkshop {
                user_id: user_id.clone(),
            },
        };
        let existing = self
            .instances
            .list_attachments(instance.id)
            .await
            .map_err(|error| failed("workspace_module_attachment_query_failed", error))?
            .into_iter()
            .find(|attachment| attachment.detached_at.is_none() && attachment.subject == subject);
        if existing.is_none() {
            let attachment = InteractionAttachment {
                id: uuid::Uuid::new_v4(),
                instance_id: instance.id,
                subject,
                role: InteractionAttachmentRole::Renderer,
                capabilities: AttachmentCapabilityProjection::for_role(
                    InteractionAttachmentRole::Renderer,
                ),
                created_at: chrono::Utc::now(),
                detached_at: None,
            };
            attachment
                .validate()
                .map_err(|error| failed("workspace_module_attachment_invalid", error))?;
            self.instances
                .attach(&attachment)
                .await
                .map_err(|error| match error {
                    InteractionError::PersistenceConflict { .. } => rejected(
                        "workspace_module_attachment_conflict",
                        "AgentRun already has an active attachment for this Interaction",
                    ),
                    _ => failed("workspace_module_attachment_failed", error),
                })?;
        }
        Ok(PresentationTarget {
            instance_id: instance.id,
        })
    }
}

#[async_trait]
impl WorkspaceModuleProvider for CanvasWorkspaceModuleProvider {
    fn provider_key(&self) -> &str {
        "system.canvas"
    }

    async fn modules(
        &self,
        context: &WorkspaceModuleProviderContext,
    ) -> Result<Vec<WorkspaceModuleDescriptor>, ProductRuntimeToolOutcome> {
        let definitions = self.visible_definitions(context).await?;
        let mut modules = definitions
            .iter()
            .map(|revision| canvas_definition_module(revision, &context.operations))
            .collect::<Vec<_>>();
        modules.extend(self.runtime_modules(context).await?);
        Ok(modules)
    }

    fn handles_operate(&self, operation: &str) -> bool {
        matches!(operation, "canvas.create" | "canvas.attach" | "canvas.copy")
    }

    async fn operate(
        &self,
        request: WorkspaceModuleOperateRequest<'_>,
    ) -> Result<WorkspaceModuleOperateOutcome, ProductRuntimeToolOutcome> {
        let operation_key = request.operation.strip_prefix("canvas.").ok_or_else(|| {
            rejected(
                "workspace_module_operation_not_routable",
                "Canvas route must use the canvas.* namespace",
            )
        })?;
        let result = self
            .operation_gateway
            .invoke(
                OperationInvocationCommand {
                    operation_ref: canvas_authoring_operation_ref(
                        request.context.project_id,
                        operation_key,
                    )
                    .map_err(|error| {
                        rejected(
                            "workspace_module_canvas_operation_invalid",
                            error.to_string(),
                        )
                    })?,
                    input: request.input,
                    principal: OperationPrincipal::server_resolved(match &request.context.actor {
                        WorkspaceModuleActor::AgentRunAgent { target, .. } => {
                            OperationPrincipalRef::AgentRunAgent {
                                run_id: target.run_id,
                                agent_id: target.agent_id,
                            }
                        }
                        WorkspaceModuleActor::User { user_id } => OperationPrincipalRef::User {
                            user_id: user_id.clone(),
                        },
                    }),
                    scope_ref: OperationScopeRef::Project {
                        project_id: request.context.project_id,
                    },
                    origin: match &request.context.actor {
                        WorkspaceModuleActor::AgentRunAgent { .. } => OperationOriginRef::AgentTool,
                        WorkspaceModuleActor::User { .. } => OperationOriginRef::UserWorkshop,
                    },
                    trace: OperationTraceContext::root(),
                    deadline: chrono::Utc::now() + chrono::Duration::seconds(30),
                    idempotency_key: Some(request.context.invocation_id.clone()),
                    attachment_ref: None,
                },
                CancellationToken::new(),
            )
            .await
            .map_err(|error| failed("workspace_module_canvas_operation_failed", error))?;
        let value = match result.value {
            OperationResultValue::Inline { value } => value,
            OperationResultValue::Ref { .. } => {
                return Err(failed(
                    "workspace_module_canvas_result_not_inline",
                    "Canvas authoring Operation must return an inline result",
                ));
            }
        };
        let definition_id = value
            .get("canvas_id")
            .and_then(Value::as_str)
            .and_then(|value| uuid::Uuid::parse_str(value).ok())
            .ok_or_else(|| {
                failed(
                    "workspace_module_canvas_result_invalid",
                    "Canvas authoring Operation result is missing canvas_id",
                )
            })?;
        let definition = self
            .definitions
            .get(definition_id)
            .await
            .map_err(|error| failed("workspace_module_definition_query_failed", error))?
            .ok_or_else(|| {
                failed(
                    "workspace_module_canvas_result_missing",
                    "Canvas authoring Operation completed without a definition",
                )
            })?;
        let revision = self
            .definitions
            .get_revision(definition.current_revision_id)
            .await
            .map_err(|error| failed("workspace_module_definition_query_failed", error))?
            .ok_or_else(|| {
                failed(
                    "workspace_module_canvas_result_missing",
                    "Canvas authoring Operation completed without a current definition revision",
                )
            })?;
        let module_id = format!("canvas:{}", revision.definition_id);
        Ok(WorkspaceModuleOperateOutcome {
            output: json!({
                "content": [{
                    "type": "text",
                    "text": format!(
                        "operated workspace module\noperation={}\nmodule_id={module_id}\ncanvas_id={}\ncanvas_mount_id={}\nvfs_mount={}://\nskill_path=lifecycle://skills/canvas-system/SKILL.md",
                        request.operation,
                        revision.definition_id,
                        revision.authoring_mount_id,
                        revision.authoring_mount_id,
                    )
                }],
                "is_error": false,
                "details": {
                    "operation": request.operation,
                    "module_id": module_id,
                    "canvas": {
                        "action": value.get("action").cloned().unwrap_or(Value::Null),
                        "canvas_id": revision.definition_id,
                        "canvas_mount_id": revision.authoring_mount_id,
                        "vfs_mount_id": revision.authoring_mount_id,
                        "module_id": module_id,
                        "presentation_uri": format!("canvas://{}", revision.definition_id),
                        "title": revision.title,
                        "entry_file": revision.source_bundle.entry_file,
                        "skill_name": "canvas-system",
                        "skill_path": "lifecycle://skills/canvas-system/SKILL.md"
                    },
                },
            }),
            surface_effect: WorkspaceModuleSurfaceEffect::RefreshRequired { module_id },
        })
    }

    fn owns_module(&self, module: &WorkspaceModuleDescriptor) -> bool {
        module.summary.module_id.starts_with("canvas:")
            || module.summary.module_id.starts_with("interaction:")
    }

    async fn prepare_presentation(
        &self,
        request: WorkspaceModulePresentationRequest<'_>,
    ) -> Result<WorkspaceModulePresentationPreparation, ProductRuntimeToolOutcome> {
        let Some(definition_id) = request
            .module
            .summary
            .module_id
            .strip_prefix("canvas:")
            .and_then(|value| uuid::Uuid::parse_str(value).ok())
        else {
            return Ok(WorkspaceModulePresentationPreparation::default());
        };
        let revision = self
            .visible_definitions(request.context)
            .await?
            .into_iter()
            .find(|revision| revision.definition_id == definition_id)
            .ok_or_else(|| {
                rejected(
                    "workspace_module_definition_not_visible",
                    "Canvas definition revision is not visible in the current actor surface",
                )
            })?;
        let target = self.attach_presentation(request.context, &revision).await?;
        Ok(WorkspaceModulePresentationPreparation::Redirected {
            module_id: format!("interaction:{}", target.instance_id),
            view_key: "runtime".to_owned(),
            diagnostics: Some(json!({
                "definition_uri": format!("canvas://{definition_id}"),
                "instance_id": target.instance_id
            })),
        })
    }
}

struct PresentationTarget {
    instance_id: uuid::Uuid,
}

fn canvas_definition_module(
    revision: &InteractionDefinitionRevision,
    operation_catalog: &[OperationDescriptor],
) -> WorkspaceModuleDescriptor {
    let definition_id = revision.definition_id.to_string();
    let operations = operation_catalog
        .iter()
        .filter(|operation| {
            operation.operation_ref.provider.namespace == "interaction"
                && operation
                    .operation_ref
                    .provider
                    .provider_key
                    .starts_with(&format!("{definition_id}."))
        })
        .map(workspace_module_operation_from_descriptor)
        .collect::<Vec<_>>();
    WorkspaceModuleDescriptor {
        summary: WorkspaceModuleSummary {
            module_id: format!("canvas:{definition_id}"),
            kind: WorkspaceModuleKind::new("canvas"),
            title: revision.title.clone(),
            description: revision.description.clone(),
            source: definition_id.clone(),
            ui_summary: Some("1 view".to_owned()),
            operation_summary: operations
                .iter()
                .map(|operation| operation.operation_key.clone())
                .collect(),
            permission_summary: Vec::new(),
            status: WorkspaceModuleStatus::ready(),
        },
        ui_entries: vec![WorkspaceModuleUiEntry {
            view_key: "preview".to_owned(),
            renderer_kind: "canvas".to_owned(),
            presentation_uri: Some(format!("canvas://{definition_id}")),
            uri_scheme: None,
            title: revision.title.clone(),
        }],
        operations,
        runtime_backing: Some(format!("interaction_definition:{}", revision.revision_id)),
        agent_state_projection: None,
    }
}

fn canvas_runtime_module(
    instance: &InteractionInstance,
    revision: &InteractionDefinitionRevision,
    operation_catalog: &[OperationDescriptor],
) -> Result<WorkspaceModuleDescriptor, ProductRuntimeToolOutcome> {
    let definition_provider_key = format!("{}.{}", revision.definition_id, revision.revision_id);
    let instance_provider_key = instance.id.to_string();
    let operations = operation_catalog
        .iter()
        .filter(|operation| {
            (operation.operation_ref.provider.namespace == "interaction"
                && operation.operation_ref.provider.provider_key == definition_provider_key)
                || (operation.operation_ref.provider.namespace == "canvas_runtime"
                    && operation.operation_ref.provider.provider_key == instance_provider_key)
        })
        .map(workspace_module_operation_from_descriptor)
        .collect::<Vec<_>>();
    let projection = revision
        .agent_projection
        .project(&instance.state)
        .map_err(|error| failed("workspace_module_agent_projection_failed", error))?;
    Ok(WorkspaceModuleDescriptor {
        summary: WorkspaceModuleSummary {
            module_id: format!("interaction:{}", instance.id),
            kind: WorkspaceModuleKind::new("interaction"),
            title: revision.title.clone(),
            description: revision.description.clone(),
            source: instance.id.to_string(),
            ui_summary: Some("1 runtime view".to_owned()),
            operation_summary: operations
                .iter()
                .map(|operation| operation.operation_key.clone())
                .collect(),
            permission_summary: Vec::new(),
            status: WorkspaceModuleStatus::ready(),
        },
        ui_entries: vec![WorkspaceModuleUiEntry {
            view_key: "runtime".to_owned(),
            renderer_kind: "canvas".to_owned(),
            presentation_uri: Some(format!("interaction://{}", instance.id)),
            uri_scheme: None,
            title: revision.title.clone(),
        }],
        operations,
        runtime_backing: Some(format!("interaction_instance:{}", instance.id)),
        agent_state_projection: Some(WorkspaceModuleAgentStateProjection {
            instance_id: instance.id.to_string(),
            definition_id: instance.definition_id.to_string(),
            definition_revision_id: instance.definition_revision_id.to_string(),
            state_revision: instance.state_revision,
            values: projection,
        }),
    })
}

fn rejected(code: impl Into<String>, message: impl Into<String>) -> ProductRuntimeToolOutcome {
    ProductRuntimeToolOutcome::Rejected {
        code: code.into(),
        message: message.into(),
    }
}

fn failed(code: impl Into<String>, error: impl std::fmt::Display) -> ProductRuntimeToolOutcome {
    ProductRuntimeToolOutcome::Failed {
        code: code.into(),
        message: error.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    use super::*;
    use agentdash_application_operation_gateway::{
        EphemeralOperationResultStore, OperationAuthorityGrant, OperationAuthorityResolver,
        OperationAuthorizationScope, OperationExecutionError, OperationOriginRef,
        TracingOperationAuditSink,
    };
    use agentdash_domain::DomainError;
    use agentdash_domain::interaction::{
        DefinitionRevisionCommit, InteractionAgentProjection, InteractionDefinition,
        InteractionDefinitionKind, InteractionError, InteractionRuntimeBinding, SourceBundle,
        SourceFile, SourceSandboxConfig,
    };
    use agentdash_domain::workflow::{AgentSource, LifecycleAgent, LifecycleAgentRepository};
    use tokio::sync::RwLock;

    use crate::canvas::mount_surface::{
        CanvasMountConvergence, CanvasMountMaterializationError, CanvasMountMaterializationPort,
        CanvasMountMaterializationRequest,
    };
    use crate::canvas::operation_provider::CanvasAuthoringOperationProvider;

    #[derive(Default)]
    struct FixtureDefinitions {
        definitions: RwLock<BTreeMap<uuid::Uuid, InteractionDefinition>>,
        revisions: RwLock<BTreeMap<uuid::Uuid, InteractionDefinitionRevision>>,
    }

    #[async_trait]
    impl InteractionDefinitionRepository for FixtureDefinitions {
        async fn create(
            &self,
            definition: &InteractionDefinition,
            initial_revision: &InteractionDefinitionRevision,
        ) -> Result<(), InteractionError> {
            self.definitions
                .write()
                .await
                .insert(definition.id, definition.clone());
            self.revisions
                .write()
                .await
                .insert(initial_revision.revision_id, initial_revision.clone());
            Ok(())
        }

        async fn get(
            &self,
            id: uuid::Uuid,
        ) -> Result<Option<InteractionDefinition>, InteractionError> {
            Ok(self.definitions.read().await.get(&id).cloned())
        }

        async fn get_revision(
            &self,
            revision_id: uuid::Uuid,
        ) -> Result<Option<InteractionDefinitionRevision>, InteractionError> {
            Ok(self.revisions.read().await.get(&revision_id).cloned())
        }

        async fn list_by_owner(
            &self,
            owner: &InteractionOwner,
        ) -> Result<Vec<InteractionDefinition>, InteractionError> {
            Ok(self
                .definitions
                .read()
                .await
                .values()
                .filter(|definition| &definition.owner == owner)
                .cloned()
                .collect())
        }

        async fn list_canvas_by_project(
            &self,
            project_id: uuid::Uuid,
        ) -> Result<Vec<InteractionDefinition>, InteractionError> {
            Ok(self
                .definitions
                .read()
                .await
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
            _definition_id: uuid::Uuid,
            _commit: DefinitionRevisionCommit,
        ) -> Result<InteractionDefinition, InteractionError> {
            Err(fixture_persistence_error("commit_revision"))
        }

        async fn archive(
            &self,
            _definition_id: uuid::Uuid,
        ) -> Result<InteractionDefinition, InteractionError> {
            Err(fixture_persistence_error("archive"))
        }
    }

    #[derive(Default)]
    struct FixtureInstances;

    #[async_trait]
    impl InteractionInstanceRepository for FixtureInstances {
        async fn create(&self, _instance: &InteractionInstance) -> Result<(), InteractionError> {
            Ok(())
        }

        async fn get(
            &self,
            _id: uuid::Uuid,
        ) -> Result<Option<InteractionInstance>, InteractionError> {
            Ok(None)
        }

        async fn list_by_owner(
            &self,
            _owner: &InteractionOwner,
        ) -> Result<Vec<InteractionInstance>, InteractionError> {
            Ok(Vec::new())
        }

        async fn close(
            &self,
            _instance_id: uuid::Uuid,
            _expected_state_revision: u64,
        ) -> Result<InteractionInstance, InteractionError> {
            Err(fixture_persistence_error("close"))
        }

        async fn attach(
            &self,
            _attachment: &InteractionAttachment,
        ) -> Result<(), InteractionError> {
            Ok(())
        }

        async fn detach(&self, _attachment_id: uuid::Uuid) -> Result<(), InteractionError> {
            Ok(())
        }

        async fn list_attachments(
            &self,
            _instance_id: uuid::Uuid,
        ) -> Result<Vec<InteractionAttachment>, InteractionError> {
            Ok(Vec::new())
        }

        async fn upsert_runtime_binding(
            &self,
            _binding: &InteractionRuntimeBinding,
        ) -> Result<(), InteractionError> {
            Ok(())
        }

        async fn list_runtime_bindings(
            &self,
            _instance_id: uuid::Uuid,
            _attachment_id: Option<uuid::Uuid>,
        ) -> Result<Vec<InteractionRuntimeBinding>, InteractionError> {
            Ok(Vec::new())
        }
    }

    struct FixtureLifecycleAgents {
        agent: LifecycleAgent,
    }

    #[async_trait]
    impl LifecycleAgentRepository for FixtureLifecycleAgents {
        async fn create(&self, _agent: &LifecycleAgent) -> Result<(), DomainError> {
            Ok(())
        }

        async fn get(&self, id: uuid::Uuid) -> Result<Option<LifecycleAgent>, DomainError> {
            Ok((self.agent.id == id).then(|| self.agent.clone()))
        }

        async fn list_by_run(
            &self,
            run_id: uuid::Uuid,
        ) -> Result<Vec<LifecycleAgent>, DomainError> {
            Ok((self.agent.run_id == run_id)
                .then(|| self.agent.clone())
                .into_iter()
                .collect())
        }

        async fn update(&self, _agent: &LifecycleAgent) -> Result<(), DomainError> {
            Ok(())
        }
    }

    struct AllowOperationAuthority;

    #[async_trait]
    impl OperationAuthorityResolver for AllowOperationAuthority {
        async fn resolve(
            &self,
            _principal: &OperationPrincipal,
            _scope: &OperationAuthorizationScope,
            _origin: &OperationOriginRef,
            _cancel: CancellationToken,
        ) -> Result<OperationAuthorityGrant, OperationExecutionError> {
            Ok(OperationAuthorityGrant {
                authority_revision: "fixture-revision".to_owned(),
                capabilities: BTreeSet::from(["operation.invoke".to_owned()]),
            })
        }
    }

    struct SuccessfulMountMaterializer;

    #[async_trait]
    impl CanvasMountMaterializationPort for SuccessfulMountMaterializer {
        async fn materialize(
            &self,
            request: CanvasMountMaterializationRequest,
        ) -> Result<CanvasMountConvergence, CanvasMountMaterializationError> {
            Ok(CanvasMountConvergence {
                frame_id: uuid::Uuid::new_v4(),
                frame_revision: 2,
                wrote_frame_revision: true,
                applied_generation: Some(2),
                module_ref: format!("canvas:{}", request.definition_id),
                authoring_mount_id: "created-canvas".to_owned(),
            })
        }
    }

    fn fixture_persistence_error(operation: &'static str) -> InteractionError {
        InteractionError::Persistence {
            operation,
            message: "fixture method is not used".to_owned(),
        }
    }

    #[tokio::test]
    async fn create_does_not_query_the_materialized_canvas_through_the_stale_input_surface() {
        let project_id = uuid::Uuid::new_v4();
        let run_id = uuid::Uuid::new_v4();
        let lifecycle_agent = LifecycleAgent::new_root_for_user(
            run_id,
            project_id,
            AgentSource::ProjectAgent,
            "user-1",
        );
        let target = agentdash_domain::agent_run_target::AgentRunTarget {
            run_id,
            agent_id: lifecycle_agent.id,
        };
        let definitions = Arc::new(FixtureDefinitions::default());
        let instances = Arc::new(FixtureInstances);
        let lifecycle_agents = Arc::new(FixtureLifecycleAgents {
            agent: lifecycle_agent,
        });
        let mount_materializer = Arc::new(SuccessfulMountMaterializer);
        let operation_provider = Arc::new(CanvasAuthoringOperationProvider::new(
            definitions.clone(),
            lifecycle_agents,
            mount_materializer,
        ));
        let operation_gateway = Arc::new(
            OperationGateway::try_new(
                Arc::new(AllowOperationAuthority),
                [],
                [operation_provider
                    as Arc<dyn agentdash_application_operation_gateway::DynamicOperationProvider>],
                Arc::new(EphemeralOperationResultStore::default()),
                Arc::new(TracingOperationAuditSink),
            )
            .expect("operation gateway"),
        );
        let provider =
            CanvasWorkspaceModuleProvider::new(definitions, instances, operation_gateway);
        let context = WorkspaceModuleProviderContext {
            project_id,
            actor: WorkspaceModuleActor::AgentRunAgent {
                user_id: "user-1".to_owned(),
                target,
            },
            invocation_id: "create-canvas".to_owned(),
            visibility: agentdash_platform_spi::WorkspaceModuleDimension::all(),
            vfs_mounts: Vec::new(),
            operations: Vec::new(),
        };

        let outcome = provider
            .operate(WorkspaceModuleOperateRequest {
                context: &context,
                operation: "canvas.create",
                input: json!({ "title": "Created Canvas" }),
            })
            .await
            .expect("successful materialization must not query through the stale input surface");
        let module_id = outcome.output["details"]["module_id"]
            .as_str()
            .expect("module id")
            .to_owned();

        assert_eq!(
            outcome.surface_effect,
            WorkspaceModuleSurfaceEffect::RefreshRequired { module_id }
        );
        assert!(outcome.output["details"].get("descriptor").is_none());
    }

    #[test]
    fn runtime_projection_stays_owned_by_canvas_provider_and_is_allowlisted() {
        let project_id = uuid::Uuid::new_v4();
        let mut revision = InteractionDefinitionRevision::new_canvas_v1(
            uuid::Uuid::new_v4(),
            1,
            project_id,
            InteractionOwner::Project(project_id),
            "Runtime",
            "",
            SourceBundle::new(
                "index.html",
                vec![SourceFile::new("index.html", "<main />", None).expect("source file")],
                SourceSandboxConfig::default(),
            )
            .expect("source bundle"),
            json!({"public": {"value": 1}, "secret": "hidden"}),
            json!({"type": "object"}),
            "user-1",
        )
        .expect("canvas revision");
        revision.agent_projection = InteractionAgentProjection {
            version: 1,
            allowed_state_paths: vec!["/public/value".to_owned()],
        };
        let instance = InteractionInstance::new_v1(
            InteractionOwner::Project(project_id),
            revision.definition_id,
            revision.revision_id,
            revision.initial_state.clone(),
            InteractionRetention { retain_until: None },
        )
        .expect("interaction instance");

        let module =
            canvas_runtime_module(&instance, &revision, &[]).expect("runtime module projection");
        let projection = module.agent_state_projection.expect("agent projection");

        assert_eq!(
            module.summary.module_id,
            format!("interaction:{}", instance.id)
        );
        assert_eq!(module.summary.kind, WorkspaceModuleKind::new("interaction"));
        assert_eq!(projection.values["/public/value"], json!(1));
        assert_eq!(projection.values.len(), 1);
    }
}
