use std::sync::Arc;

use agentdash_application::interaction::{
    CanvasDefinitionAccessResolver, CanvasDefinitionListScope, CanvasDefinitionService,
    CanvasDefinitionView, CanvasProjectAccess, CommitCanvasDefinitionInput,
    CopyCanvasDefinitionInput, CreateCanvasDefinitionInput, CreateInteractionInstanceInput,
    InteractionApplicationError, InteractionApplicationResult, InteractionCloseInput,
    InteractionCommandAdmission, InteractionCommandAdmissionPort, InteractionCommandCallerContext,
    InteractionCommandInput, InteractionCommandService, InteractionComponentArtifactResolver,
    InteractionEffectDescriptorAdmissionPort, InteractionInstanceAccess,
    InteractionInstanceAccessResolver, InteractionInstanceService, InteractionInstanceView,
    PublishCanvasDefinitionInput, ResolvedComponentArtifact,
};
use agentdash_application_runtime_gateway::{OperationPrincipal, OperationReadiness};
use agentdash_contracts::interaction::{
    ArchiveInteractionDefinitionResponse, CanvasDefinitionDto, CanvasDefinitionListScopeDto,
    CloseInteractionInstanceRequestDto, CommitCanvasDefinitionRequest,
    CreateCanvasDefinitionRequest, CreateInteractionInstanceRequestDto,
    DistributeCanvasDefinitionRequest, InteractionCommandActorPolicyDto,
    InteractionCommandDefinitionDto, InteractionCommandRequestDto, InteractionCommandResponseDto,
    InteractionComponentBindingDto, InteractionComponentEventBindingDto,
    InteractionDefinitionAccessDto, InteractionDefinitionLineageDto,
    InteractionDefinitionLineageKindDto, InteractionDefinitionStatusDto, InteractionInstanceDto,
    InteractionInstanceViewDto, InteractionOperationRefDto, InteractionOwnerDto,
    InteractionPinnedArtifactDto, InteractionResourceSlotDto, InteractionResourceSlotKindDto,
    InteractionRuntimeBindingDto, InteractionRuntimeBindingTargetDto, InteractionSourceBundleDto,
    InteractionSourceChangesetDto, InteractionSourceFileChangeDto, InteractionSourceFileDto,
    InteractionSourceSandboxDto, InteractionStatePatchV1ContractDto, ListCanvasDefinitionsQuery,
};
use agentdash_domain::interaction::{
    CommandActorPolicy, ComponentBinding, ComponentEventCommandBinding, DefinitionLineageKind,
    InteractionActor, InteractionCommandCommit, InteractionCommandDefinition,
    InteractionCommandOrigin, InteractionDefinition, InteractionDefinitionStatus,
    InteractionOperationEffectDefinition, InteractionOwner, OperationEffectSafety,
    PlatformCommandHandler, ResourceSlotDefinition, ResourceSlotKind, RuntimeBindingTarget,
    SourceBundle, SourceBundleChangeset, SourceFile, SourceFileChange, SourceSandboxConfig,
    StatePatchV1Contract,
};
use agentdash_domain::operation::{OperationOriginRef, OperationReplayPolicy, OperationScopeRef};
use agentdash_domain::project::{ProjectAuthorizationContext, ProjectAuthorizationService};
use async_trait::async_trait;
use axum::Json;
use axum::extract::{Path, Query, State};
use chrono::Utc;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::app_state::AppState;
use crate::auth::{CurrentUser, project_authorization_context};
use crate::dto::{InteractionDefinitionPath, ProjectInteractionDefinitionsPath};
use crate::rpc::ApiError;

pub fn router() -> axum::Router<Arc<AppState>> {
    axum::Router::new()
        .route(
            "/projects/{project_id}/interaction-definitions/canvas",
            axum::routing::get(list_canvas_definitions).post(create_canvas_definition),
        )
        .route(
            "/interaction-definitions/{definition_id}",
            axum::routing::get(get_canvas_definition),
        )
        .route(
            "/interaction-definitions/{definition_id}/revisions",
            axum::routing::post(commit_canvas_definition),
        )
        .route(
            "/interaction-definitions/{definition_id}/publish",
            axum::routing::post(publish_canvas_definition),
        )
        .route(
            "/interaction-definitions/{definition_id}/copy",
            axum::routing::post(copy_canvas_definition),
        )
        .route(
            "/interaction-definitions/{definition_id}/unpublish",
            axum::routing::post(unpublish_canvas_definition),
        )
        .route(
            "/interaction-definitions/{definition_id}/archive",
            axum::routing::post(archive_canvas_definition),
        )
        .route(
            "/interaction-definitions/{definition_id}/instances",
            axum::routing::post(create_interaction_instance),
        )
        .route(
            "/projects/{project_id}/interaction-instances",
            axum::routing::get(list_interaction_instances),
        )
        .route(
            "/interaction-instances/{instance_id}",
            axum::routing::get(get_interaction_instance),
        )
        .route(
            "/interaction-instances/{instance_id}/commands",
            axum::routing::post(execute_interaction_command),
        )
        .route(
            "/interaction-instances/{instance_id}/close",
            axum::routing::post(close_interaction_instance),
        )
}

#[derive(Debug, serde::Deserialize)]
struct InteractionInstancePath {
    instance_id: String,
}

async fn list_canvas_definitions(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(path): Path<ProjectInteractionDefinitionsPath>,
    Query(query): Query<ListCanvasDefinitionsQuery>,
) -> Result<Json<Vec<CanvasDefinitionDto>>, ApiError> {
    let project_id = parse_uuid(&path.project_id, "project_id")?;
    let service = definition_service(&state, &current_user);
    let scope = match query.scope.unwrap_or(CanvasDefinitionListScopeDto::All) {
        CanvasDefinitionListScopeDto::All => CanvasDefinitionListScope::All,
        CanvasDefinitionListScopeDto::Mine => CanvasDefinitionListScope::Mine,
        CanvasDefinitionListScopeDto::Shared => CanvasDefinitionListScope::Shared,
    };
    let views = service
        .list(project_id, scope, &current_user.user_id)
        .await?;
    Ok(Json(views.into_iter().map(view_to_dto).collect()))
}

async fn create_canvas_definition(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(path): Path<ProjectInteractionDefinitionsPath>,
    Json(request): Json<CreateCanvasDefinitionRequest>,
) -> Result<Json<CanvasDefinitionDto>, ApiError> {
    let service = definition_service(&state, &current_user);
    let view = service
        .create_personal(
            CreateCanvasDefinitionInput {
                project_id: parse_uuid(&path.project_id, "project_id")?,
                title: request.title,
                description: request.description,
                source_bundle: source_bundle_from_dto(request.source_bundle)?,
                initial_state: request.initial_state,
                state_schema: request.state_schema,
                command_definitions: request
                    .command_definitions
                    .into_iter()
                    .map(command_definition_from_dto)
                    .collect::<Result<Vec<_>, _>>()?,
                component_bindings: request
                    .component_bindings
                    .into_iter()
                    .map(component_binding_from_dto)
                    .collect(),
                resource_slots: request
                    .resource_slots
                    .into_iter()
                    .map(resource_slot_from_dto)
                    .collect(),
            },
            &current_user.user_id,
        )
        .await?;
    Ok(Json(view_to_dto(view)))
}

async fn get_canvas_definition(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(path): Path<InteractionDefinitionPath>,
) -> Result<Json<CanvasDefinitionDto>, ApiError> {
    let view = definition_service(&state, &current_user)
        .get(
            parse_uuid(&path.definition_id, "definition_id")?,
            &current_user.user_id,
        )
        .await?;
    Ok(Json(view_to_dto(view)))
}

async fn commit_canvas_definition(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(path): Path<InteractionDefinitionPath>,
    Json(request): Json<CommitCanvasDefinitionRequest>,
) -> Result<Json<CanvasDefinitionDto>, ApiError> {
    let view = definition_service(&state, &current_user)
        .commit_changeset(
            CommitCanvasDefinitionInput {
                definition_id: parse_uuid(&path.definition_id, "definition_id")?,
                base_revision_id: parse_uuid(&request.base_revision_id, "base_revision_id")?,
                title: request.title,
                description: request.description,
                changeset: changeset_from_dto(request.changeset)?,
                command_definitions: request
                    .command_definitions
                    .map(|definitions| {
                        definitions
                            .into_iter()
                            .map(command_definition_from_dto)
                            .collect::<Result<Vec<_>, _>>()
                    })
                    .transpose()?,
                component_bindings: request.component_bindings.map(|bindings| {
                    bindings
                        .into_iter()
                        .map(component_binding_from_dto)
                        .collect()
                }),
                resource_slots: request
                    .resource_slots
                    .map(|slots| slots.into_iter().map(resource_slot_from_dto).collect()),
            },
            &current_user.user_id,
            Utc::now(),
        )
        .await?;
    Ok(Json(view_to_dto(view)))
}

async fn publish_canvas_definition(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(path): Path<InteractionDefinitionPath>,
    Json(request): Json<DistributeCanvasDefinitionRequest>,
) -> Result<Json<CanvasDefinitionDto>, ApiError> {
    let view = definition_service(&state, &current_user)
        .publish(
            PublishCanvasDefinitionInput {
                source_definition_id: parse_uuid(&path.definition_id, "definition_id")?,
                source_revision_id: parse_uuid(&request.source_revision_id, "source_revision_id")?,
                title: request.title,
                description: request.description,
            },
            &current_user.user_id,
            Utc::now(),
        )
        .await?;
    Ok(Json(view_to_dto(view)))
}

async fn copy_canvas_definition(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(path): Path<InteractionDefinitionPath>,
    Json(request): Json<DistributeCanvasDefinitionRequest>,
) -> Result<Json<CanvasDefinitionDto>, ApiError> {
    let view = definition_service(&state, &current_user)
        .copy_to_personal(
            CopyCanvasDefinitionInput {
                source_definition_id: parse_uuid(&path.definition_id, "definition_id")?,
                source_revision_id: parse_uuid(&request.source_revision_id, "source_revision_id")?,
                title: request.title,
                description: request.description,
            },
            &current_user.user_id,
            Utc::now(),
        )
        .await?;
    Ok(Json(view_to_dto(view)))
}

async fn unpublish_canvas_definition(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(path): Path<InteractionDefinitionPath>,
) -> Result<Json<ArchiveInteractionDefinitionResponse>, ApiError> {
    let definition = definition_service(&state, &current_user)
        .unpublish(
            parse_uuid(&path.definition_id, "definition_id")?,
            &current_user.user_id,
        )
        .await?;
    Ok(Json(archive_response(definition)))
}

async fn archive_canvas_definition(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(path): Path<InteractionDefinitionPath>,
) -> Result<Json<ArchiveInteractionDefinitionResponse>, ApiError> {
    let definition = definition_service(&state, &current_user)
        .archive(
            parse_uuid(&path.definition_id, "definition_id")?,
            &current_user.user_id,
        )
        .await?;
    Ok(Json(archive_response(definition)))
}

async fn create_interaction_instance(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(path): Path<InteractionDefinitionPath>,
    Json(request): Json<CreateInteractionInstanceRequestDto>,
) -> Result<Json<InteractionInstanceViewDto>, ApiError> {
    let view = instance_service(&state, &current_user)
        .create(
            CreateInteractionInstanceInput {
                definition_id: parse_uuid(&path.definition_id, "definition_id")?,
                definition_revision_id: parse_uuid(
                    &request.definition_revision_id,
                    "definition_revision_id",
                )?,
            },
            &current_user.user_id,
        )
        .await?;
    Ok(Json(instance_view_to_dto(view)))
}

async fn list_interaction_instances(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(path): Path<ProjectInteractionDefinitionsPath>,
) -> Result<Json<Vec<InteractionInstanceViewDto>>, ApiError> {
    let views = instance_service(&state, &current_user)
        .list_project(
            parse_uuid(&path.project_id, "project_id")?,
            &current_user.user_id,
        )
        .await?;
    Ok(Json(views.into_iter().map(instance_view_to_dto).collect()))
}

async fn get_interaction_instance(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(path): Path<InteractionInstancePath>,
) -> Result<Json<InteractionInstanceViewDto>, ApiError> {
    let view = instance_service(&state, &current_user)
        .get(
            parse_uuid(&path.instance_id, "instance_id")?,
            &current_user.user_id,
        )
        .await?;
    Ok(Json(instance_view_to_dto(view)))
}

async fn execute_interaction_command(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(path): Path<InteractionInstancePath>,
    Json(request): Json<InteractionCommandRequestDto>,
) -> Result<Json<InteractionCommandResponseDto>, ApiError> {
    let instance_id = parse_uuid(&path.instance_id, "instance_id")?;
    let view = instance_service(&state, &current_user)
        .get(instance_id, &current_user.user_id)
        .await?;
    let revision = state
        .repos
        .interaction_definition_repo
        .get_revision(view.instance.definition_revision_id)
        .await?
        .ok_or_else(|| ApiError::NotFound("Interaction definition revision 不存在".into()))?;
    let commit = command_service(&state, &current_user, revision.project_id)
        .execute(
            InteractionCommandInput {
                instance_id,
                command_id: parse_uuid(&request.command_id, "command_id")?,
                command_key: request.command_key,
                payload: request.payload,
                expected_state_revision: request.expected_state_revision,
            },
            InteractionCommandCallerContext::AuthenticatedUser {
                user_id: current_user.user_id.clone(),
            },
            Utc::now(),
        )
        .await?;
    let (instance, event, duplicate) = match commit {
        InteractionCommandCommit::Committed {
            instance, event, ..
        } => (instance, event, false),
        InteractionCommandCommit::Duplicate {
            instance, event, ..
        } => (instance, event, true),
    };
    Ok(Json(InteractionCommandResponseDto {
        instance: interaction_instance_to_dto(instance),
        event_id: event.id.to_string(),
        event_sequence: event.sequence,
        duplicate,
    }))
}

async fn close_interaction_instance(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(path): Path<InteractionInstancePath>,
    Json(request): Json<CloseInteractionInstanceRequestDto>,
) -> Result<Json<InteractionInstanceDto>, ApiError> {
    let instance_id = parse_uuid(&path.instance_id, "instance_id")?;
    let view = instance_service(&state, &current_user)
        .get(instance_id, &current_user.user_id)
        .await?;
    let revision = state
        .repos
        .interaction_definition_repo
        .get_revision(view.instance.definition_revision_id)
        .await?
        .ok_or_else(|| ApiError::NotFound("Interaction definition revision 不存在".into()))?;
    let instance = command_service(&state, &current_user, revision.project_id)
        .close(
            InteractionCloseInput {
                instance_id,
                expected_state_revision: request.expected_state_revision,
            },
            InteractionCommandCallerContext::AuthenticatedUser {
                user_id: current_user.user_id.clone(),
            },
        )
        .await?;
    Ok(Json(interaction_instance_to_dto(instance)))
}

fn definition_service(
    state: &Arc<AppState>,
    current_user: &agentdash_integration_api::AuthIdentity,
) -> CanvasDefinitionService {
    CanvasDefinitionService::new(
        state.repos.interaction_definition_repo.clone(),
        Arc::new(ApiCanvasDefinitionAccessResolver {
            project_repo: state.repos.project_repo.clone(),
            context: project_authorization_context(current_user),
        }),
    )
}

fn instance_service(
    state: &Arc<AppState>,
    current_user: &agentdash_integration_api::AuthIdentity,
) -> InteractionInstanceService {
    let access = Arc::new(ApiInteractionInstanceAccessResolver {
        project_repo: state.repos.project_repo.clone(),
        context: project_authorization_context(current_user),
    });
    InteractionInstanceService::new(
        state.repos.interaction_definition_repo.clone(),
        state.repos.interaction_instance_repo.clone(),
        access,
        Arc::new(ApiInteractionComponentArtifactResolver {
            installations: state.repos.project_extension_installation_repo.clone(),
        }),
    )
}

fn command_service(
    state: &Arc<AppState>,
    current_user: &agentdash_integration_api::AuthIdentity,
    project_id: Uuid,
) -> InteractionCommandService {
    InteractionCommandService::new(
        state.repos.interaction_definition_repo.clone(),
        state.repos.interaction_instance_repo.clone(),
        state.repos.interaction_command_transaction.clone(),
        state.repos.interaction_event_repo.clone(),
        Arc::new(ApiInteractionCommandAdmission {
            definitions: state.repos.interaction_definition_repo.clone(),
            access: ApiInteractionInstanceAccessResolver {
                project_repo: state.repos.project_repo.clone(),
                context: project_authorization_context(current_user),
            },
        }),
        Arc::new(ApiInteractionEffectAdmission {
            gateway: state.services.operation_gateway.clone(),
            identity: current_user.clone(),
            project_id,
        }),
    )
}

struct ApiInteractionCommandAdmission {
    definitions: Arc<dyn agentdash_domain::interaction::InteractionDefinitionRepository>,
    access: ApiInteractionInstanceAccessResolver,
}

#[async_trait]
impl InteractionCommandAdmissionPort for ApiInteractionCommandAdmission {
    async fn admit(
        &self,
        instance: &agentdash_domain::interaction::InteractionInstance,
        _input: &InteractionCommandInput,
        caller: &InteractionCommandCallerContext,
    ) -> InteractionApplicationResult<InteractionCommandAdmission> {
        let InteractionCommandCallerContext::AuthenticatedUser { user_id } = caller else {
            return Err(InteractionApplicationError::AccessDenied {
                reason: "HTTP Interaction command 只接受 authenticated user".into(),
            });
        };
        let revision = self
            .definitions
            .get_revision(instance.definition_revision_id)
            .await?
            .ok_or_else(
                || agentdash_domain::interaction::InteractionError::NotFound {
                    entity: "interaction_definition_revision",
                    id: instance.definition_revision_id.to_string(),
                },
            )?;
        let access = self
            .access
            .resolve(&instance.owner, revision.project_id, user_id)
            .await?;
        if !access.can_view {
            return Err(InteractionApplicationError::AccessDenied {
                reason: "当前用户不可提交 Interaction command".into(),
            });
        }
        Ok(InteractionCommandAdmission {
            actor: InteractionActor::Human {
                user_id: user_id.clone(),
            },
            origin: InteractionCommandOrigin::UserWorkshop,
            attachment_id: None,
            capability_revision_ref: access.authorization_ref,
        })
    }

    async fn admit_close(
        &self,
        instance: &agentdash_domain::interaction::InteractionInstance,
        _input: &InteractionCloseInput,
        caller: &InteractionCommandCallerContext,
    ) -> InteractionApplicationResult<()> {
        let InteractionCommandCallerContext::AuthenticatedUser { user_id } = caller else {
            return Err(InteractionApplicationError::AccessDenied {
                reason: "HTTP Interaction close 只接受 authenticated user".into(),
            });
        };
        let revision = self
            .definitions
            .get_revision(instance.definition_revision_id)
            .await?
            .ok_or_else(
                || agentdash_domain::interaction::InteractionError::NotFound {
                    entity: "interaction_definition_revision",
                    id: instance.definition_revision_id.to_string(),
                },
            )?;
        let access = self
            .access
            .resolve(&instance.owner, revision.project_id, user_id)
            .await?;
        if access.can_close {
            Ok(())
        } else {
            Err(InteractionApplicationError::AccessDenied {
                reason: "当前用户不可关闭 Interaction instance".into(),
            })
        }
    }
}

struct ApiInteractionEffectAdmission {
    gateway: Arc<agentdash_application_runtime_gateway::OperationGateway>,
    identity: agentdash_integration_api::AuthIdentity,
    project_id: Uuid,
}

#[async_trait]
impl InteractionEffectDescriptorAdmissionPort for ApiInteractionEffectAdmission {
    async fn admit_replay_safe(
        &self,
        operation_ref: &agentdash_domain::operation::OperationRef,
    ) -> InteractionApplicationResult<OperationEffectSafety> {
        let surface = self
            .gateway
            .surface_current(
                &OperationPrincipal::authenticated_user(self.identity.clone()),
                &OperationScopeRef::Project {
                    project_id: self.project_id,
                },
                &OperationOriginRef::UserWorkshop,
                CancellationToken::new(),
            )
            .await
            .map_err(|error| InteractionApplicationError::ContractUnavailable {
                reason: error.to_string(),
            })?;
        let descriptor = surface.catalog.get(operation_ref).ok_or_else(|| {
            InteractionApplicationError::ContractUnavailable {
                reason: format!("Operation 不在当前 user surface: {operation_ref:?}"),
            }
        })?;
        if !matches!(descriptor.readiness, OperationReadiness::Ready) {
            return Err(InteractionApplicationError::ContractUnavailable {
                reason: "Operation 当前不可执行".into(),
            });
        }
        match descriptor.replay_policy {
            OperationReplayPolicy::ReplaySafe => Ok(OperationEffectSafety::ReplaySafe),
            OperationReplayPolicy::Idempotent => Ok(OperationEffectSafety::Idempotent),
            OperationReplayPolicy::NonReplayable => {
                Err(agentdash_domain::interaction::InteractionError::EffectNotReplaySafe.into())
            }
        }
    }
}

struct ApiInteractionInstanceAccessResolver {
    project_repo: Arc<dyn agentdash_domain::project::ProjectRepository>,
    context: ProjectAuthorizationContext,
}

#[async_trait]
impl InteractionInstanceAccessResolver for ApiInteractionInstanceAccessResolver {
    async fn resolve(
        &self,
        owner: &InteractionOwner,
        project_id: Uuid,
        user_id: &str,
    ) -> InteractionApplicationResult<InteractionInstanceAccess> {
        if user_id != self.context.user_id {
            return Err(InteractionApplicationError::AccessDenied {
                reason: "authenticated user 与 instance access subject 不一致".into(),
            });
        }
        if matches!(owner, InteractionOwner::User(owner_id) if owner_id != user_id) {
            return Ok(InteractionInstanceAccess {
                can_view: false,
                can_create: false,
                can_close: false,
                authorization_ref: String::new(),
            });
        }
        let project = self
            .project_repo
            .get_by_id(project_id)
            .await
            .map_err(|error| InteractionApplicationError::ContractUnavailable {
                reason: error.to_string(),
            })?
            .ok_or_else(|| InteractionApplicationError::ContractUnavailable {
                reason: format!("Project 不存在: {project_id}"),
            })?;
        let project_access = ProjectAuthorizationService::new(self.project_repo.as_ref())
            .resolve_project_access(&self.context, &project)
            .await
            .map_err(|error| InteractionApplicationError::ContractUnavailable {
                reason: error.to_string(),
            })?;
        let allowed = project_access.can_use_project();
        Ok(InteractionInstanceAccess {
            can_view: allowed,
            can_create: allowed,
            can_close: allowed,
            authorization_ref: format!("project:{project_id}:use:{user_id}"),
        })
    }
}

struct ApiInteractionComponentArtifactResolver {
    installations:
        Arc<dyn agentdash_domain::shared_library::ProjectExtensionInstallationRepository>,
}

#[async_trait]
impl InteractionComponentArtifactResolver for ApiInteractionComponentArtifactResolver {
    async fn resolve(
        &self,
        project_id: Uuid,
        component_ref: &str,
    ) -> InteractionApplicationResult<ResolvedComponentArtifact> {
        let installations = self
            .installations
            .list_enabled_by_project(project_id)
            .await
            .map_err(|error| InteractionApplicationError::ContractUnavailable {
                reason: error.to_string(),
            })?;
        let mut matches = installations.into_iter().filter(|installation| {
            installation
                .manifest
                .ui_components
                .iter()
                .any(|component| component.component_key == component_ref)
        });
        let installation =
            matches
                .next()
                .ok_or_else(|| InteractionApplicationError::ContractUnavailable {
                    reason: format!("UI component 不可用: {component_ref}"),
                })?;
        if matches.next().is_some() {
            return Err(InteractionApplicationError::ContractUnavailable {
                reason: format!("UI component identity 冲突: {component_ref}"),
            });
        }
        let artifact = installation.package_artifact.ok_or_else(|| {
            InteractionApplicationError::ContractUnavailable {
                reason: format!("UI component 缺少 packaged artifact: {component_ref}"),
            }
        })?;
        Ok(ResolvedComponentArtifact {
            artifact_id: artifact.artifact_id,
            archive_digest: artifact.archive_digest,
        })
    }
}

struct ApiCanvasDefinitionAccessResolver {
    project_repo: Arc<dyn agentdash_domain::project::ProjectRepository>,
    context: ProjectAuthorizationContext,
}

#[async_trait]
impl CanvasDefinitionAccessResolver for ApiCanvasDefinitionAccessResolver {
    async fn resolve_project_access(
        &self,
        project_id: Uuid,
        user_id: &str,
    ) -> InteractionApplicationResult<CanvasProjectAccess> {
        if user_id != self.context.user_id {
            return Err(InteractionApplicationError::AccessDenied {
                reason: "authenticated user 与 access subject 不一致".into(),
            });
        }
        let project = self
            .project_repo
            .get_by_id(project_id)
            .await
            .map_err(|error| InteractionApplicationError::ContractUnavailable {
                reason: error.to_string(),
            })?
            .ok_or_else(|| InteractionApplicationError::ContractUnavailable {
                reason: format!("Project 不存在: {project_id}"),
            })?;
        let access = ProjectAuthorizationService::new(self.project_repo.as_ref())
            .resolve_project_access(&self.context, &project)
            .await
            .map_err(|error| InteractionApplicationError::ContractUnavailable {
                reason: error.to_string(),
            })?;
        Ok(CanvasProjectAccess {
            can_use: access.can_use_project(),
            can_configure: access.can_configure_project(),
            can_manage_sharing: access.can_manage_project_sharing(),
        })
    }
}

fn source_bundle_from_dto(dto: InteractionSourceBundleDto) -> Result<SourceBundle, ApiError> {
    let expected_format = dto.format_version;
    let expected_digest = dto.digest;
    let bundle = SourceBundle::new(
        dto.entry_file,
        dto.files
            .into_iter()
            .map(|file| SourceFile::new(file.path, file.content, file.media_type))
            .collect::<Result<Vec<_>, _>>()?,
        SourceSandboxConfig {
            libraries: dto.sandbox.libraries,
            import_map: dto.sandbox.import_map,
        },
    )?;
    if expected_format != bundle.format_version || expected_digest != bundle.digest {
        return Err(ApiError::BadRequest(
            "source_bundle format_version/digest 与 canonical 内容不一致".into(),
        ));
    }
    Ok(bundle)
}

fn command_definition_from_dto(
    dto: InteractionCommandDefinitionDto,
) -> Result<InteractionCommandDefinition, ApiError> {
    let max_operations = usize::try_from(dto.state_patch_v1.max_operations)
        .map_err(|_| ApiError::BadRequest("max_operations 超出范围".into()))?;
    let max_state_bytes = usize::try_from(dto.state_patch_v1.max_state_bytes)
        .map_err(|_| ApiError::BadRequest("max_state_bytes 超出范围".into()))?;
    Ok(InteractionCommandDefinition {
        command_key: dto.command_key,
        handler: PlatformCommandHandler::StatePatchV1,
        actor_policy: match dto.actor_policy {
            InteractionCommandActorPolicyDto::Direct => CommandActorPolicy::Direct,
            InteractionCommandActorPolicyDto::HumanOnly => CommandActorPolicy::HumanOnly,
        },
        payload_schema: dto.payload_schema,
        state_patch_v1: Some(StatePatchV1Contract::new(
            dto.state_patch_v1.allowed_paths,
            max_operations,
            max_state_bytes,
        )?),
        operation_effect: dto
            .operation_effect
            .map(|operation| {
                agentdash_domain::operation::OperationRef::new(
                    operation.namespace,
                    operation.provider_key,
                    operation.operation_key,
                    operation.contract_version,
                )
                .map(|operation_ref| InteractionOperationEffectDefinition { operation_ref })
            })
            .transpose()
            .map_err(|error| ApiError::BadRequest(error.to_string()))?,
    })
}

fn component_binding_from_dto(dto: InteractionComponentBindingDto) -> ComponentBinding {
    ComponentBinding {
        binding_key: dto.binding_key,
        component_ref: dto.component_ref,
        component_abi_version: dto.component_abi_version,
        props: dto.props,
        event_commands: dto
            .event_commands
            .into_iter()
            .map(|event| ComponentEventCommandBinding {
                event_type: event.event_type,
                payload_schema: event.payload_schema,
                command_key: event.command_key,
            })
            .collect(),
    }
}

fn resource_slot_from_dto(dto: InteractionResourceSlotDto) -> ResourceSlotDefinition {
    ResourceSlotDefinition {
        slot_key: dto.slot_key,
        kind: match dto.kind {
            InteractionResourceSlotKindDto::Resource => ResourceSlotKind::Resource,
            InteractionResourceSlotKindDto::Artifact => ResourceSlotKind::Artifact,
            InteractionResourceSlotKindDto::Provider => ResourceSlotKind::Provider,
        },
        required: dto.required,
        contract: dto.contract,
    }
}

fn changeset_from_dto(
    dto: InteractionSourceChangesetDto,
) -> Result<SourceBundleChangeset, ApiError> {
    let file_changes = dto
        .file_changes
        .into_iter()
        .map(|change| match change {
            InteractionSourceFileChangeDto::Upsert { file } => {
                SourceFile::new(file.path, file.content, file.media_type)
                    .map(SourceFileChange::Upsert)
            }
            InteractionSourceFileChangeDto::Delete { path } => {
                Ok(SourceFileChange::Delete { path })
            }
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(SourceBundleChangeset {
        entry_file: dto.entry_file,
        sandbox: dto.sandbox.map(|sandbox| SourceSandboxConfig {
            libraries: sandbox.libraries,
            import_map: sandbox.import_map,
        }),
        file_changes,
    })
}

fn view_to_dto(view: CanvasDefinitionView) -> CanvasDefinitionDto {
    let definition = view.definition;
    let revision = view.revision;
    CanvasDefinitionDto {
        definition_id: definition.id.to_string(),
        project_id: definition.project_id.to_string(),
        owner: owner_to_dto(definition.owner),
        status: status_to_dto(definition.status),
        current_revision_id: definition.current_revision_id.to_string(),
        revision_number: revision.revision_number,
        definition_format_version: revision.definition_format_version,
        interaction_contract_version: revision.interaction_contract_version,
        title: revision.title,
        description: revision.description,
        source_bundle: InteractionSourceBundleDto {
            format_version: revision.source_bundle.format_version,
            entry_file: revision.source_bundle.entry_file,
            files: revision
                .source_bundle
                .files
                .into_iter()
                .map(|file| InteractionSourceFileDto {
                    path: file.path,
                    content: file.content,
                    media_type: file.media_type,
                })
                .collect(),
            sandbox: InteractionSourceSandboxDto {
                libraries: revision.source_bundle.sandbox.libraries,
                import_map: revision.source_bundle.sandbox.import_map,
            },
            digest: revision.source_bundle.digest,
        },
        initial_state: revision.initial_state,
        state_schema: revision.state_schema,
        command_definitions: revision
            .command_definitions
            .into_iter()
            .map(command_definition_to_dto)
            .collect(),
        component_bindings: revision
            .component_bindings
            .into_iter()
            .map(|binding| InteractionComponentBindingDto {
                binding_key: binding.binding_key,
                component_ref: binding.component_ref,
                component_abi_version: binding.component_abi_version,
                props: binding.props,
                event_commands: binding
                    .event_commands
                    .into_iter()
                    .map(|event| InteractionComponentEventBindingDto {
                        event_type: event.event_type,
                        payload_schema: event.payload_schema,
                        command_key: event.command_key,
                    })
                    .collect(),
            })
            .collect(),
        resource_slots: revision
            .resource_slots
            .into_iter()
            .map(|slot| InteractionResourceSlotDto {
                slot_key: slot.slot_key,
                kind: match slot.kind {
                    ResourceSlotKind::Resource => InteractionResourceSlotKindDto::Resource,
                    ResourceSlotKind::Artifact => InteractionResourceSlotKindDto::Artifact,
                    ResourceSlotKind::Provider => InteractionResourceSlotKindDto::Provider,
                },
                required: slot.required,
                contract: slot.contract,
            })
            .collect(),
        lineage: revision
            .lineage
            .map(|lineage| InteractionDefinitionLineageDto {
                kind: match lineage.kind {
                    DefinitionLineageKind::PublishedFrom => {
                        InteractionDefinitionLineageKindDto::PublishedFrom
                    }
                    DefinitionLineageKind::CopiedFrom => {
                        InteractionDefinitionLineageKindDto::CopiedFrom
                    }
                },
                source_definition_id: lineage.source_definition_id.to_string(),
                source_revision_id: lineage.source_revision_id.to_string(),
                source_bundle_digest: lineage.source_bundle_digest,
            }),
        access: InteractionDefinitionAccessDto {
            can_view: view.access.can_view,
            can_edit_source: view.access.can_edit_source,
            can_publish: view.access.can_publish,
            can_manage_shared: view.access.can_manage_shared,
            can_copy: view.access.can_copy,
        },
        created_at: definition.created_at.to_rfc3339(),
        updated_at: definition.updated_at.to_rfc3339(),
    }
}

fn command_definition_to_dto(
    definition: InteractionCommandDefinition,
) -> InteractionCommandDefinitionDto {
    let contract = definition
        .state_patch_v1
        .expect("validated state_patch_v1 command has contract");
    InteractionCommandDefinitionDto {
        command_key: definition.command_key,
        actor_policy: match definition.actor_policy {
            CommandActorPolicy::Direct => InteractionCommandActorPolicyDto::Direct,
            CommandActorPolicy::HumanOnly => InteractionCommandActorPolicyDto::HumanOnly,
        },
        payload_schema: definition.payload_schema,
        state_patch_v1: InteractionStatePatchV1ContractDto {
            allowed_paths: contract.allowed_paths,
            max_operations: contract.max_operations as u64,
            max_state_bytes: contract.max_state_bytes as u64,
        },
        operation_effect: definition
            .operation_effect
            .map(|effect| InteractionOperationRefDto {
                namespace: effect.operation_ref.provider.namespace,
                provider_key: effect.operation_ref.provider.provider_key,
                operation_key: effect.operation_ref.operation_key,
                contract_version: effect.operation_ref.contract_version,
            }),
    }
}

fn instance_view_to_dto(view: InteractionInstanceView) -> InteractionInstanceViewDto {
    InteractionInstanceViewDto {
        instance: interaction_instance_to_dto(view.instance),
        runtime_bindings: view
            .runtime_bindings
            .into_iter()
            .map(|binding| InteractionRuntimeBindingDto {
                binding_id: binding.id.to_string(),
                slot_key: binding.slot_key,
                target: match binding.target {
                    RuntimeBindingTarget::Resource {
                        resource_ref,
                        version_ref,
                    } => InteractionRuntimeBindingTargetDto::Resource {
                        resource_ref,
                        version_ref,
                    },
                    RuntimeBindingTarget::Artifact {
                        artifact_ref,
                        digest,
                    } => InteractionRuntimeBindingTargetDto::Artifact {
                        artifact_ref,
                        digest,
                    },
                    RuntimeBindingTarget::Provider {
                        provider_ref,
                        contract_version,
                    } => InteractionRuntimeBindingTargetDto::Provider {
                        provider_ref,
                        contract_version,
                    },
                },
            })
            .collect(),
    }
}

fn interaction_instance_to_dto(
    instance: agentdash_domain::interaction::InteractionInstance,
) -> InteractionInstanceDto {
    InteractionInstanceDto {
        instance_id: instance.id.to_string(),
        owner: owner_to_dto(instance.owner),
        definition_id: instance.definition_id.to_string(),
        definition_revision_id: instance.definition_revision_id.to_string(),
        interaction_contract_version: instance.interaction_contract_version,
        state: instance.state,
        state_revision: instance.state_revision,
        status: instance.status.as_str().to_string(),
        pinned_artifacts: instance
            .pinned_artifacts
            .into_iter()
            .map(|artifact| InteractionPinnedArtifactDto {
                artifact_ref: artifact.artifact_ref,
                digest: artifact.digest,
            })
            .collect(),
        created_at: instance.created_at.to_rfc3339(),
        updated_at: instance.updated_at.to_rfc3339(),
        closed_at: instance.closed_at.map(|value| value.to_rfc3339()),
    }
}

fn archive_response(definition: InteractionDefinition) -> ArchiveInteractionDefinitionResponse {
    ArchiveInteractionDefinitionResponse {
        definition_id: definition.id.to_string(),
        status: status_to_dto(definition.status),
    }
}

fn owner_to_dto(owner: InteractionOwner) -> InteractionOwnerDto {
    match owner {
        InteractionOwner::User(user_id) => InteractionOwnerDto::User(user_id),
        InteractionOwner::Project(project_id) => {
            InteractionOwnerDto::Project(project_id.to_string())
        }
    }
}

fn status_to_dto(status: InteractionDefinitionStatus) -> InteractionDefinitionStatusDto {
    match status {
        InteractionDefinitionStatus::Active => InteractionDefinitionStatusDto::Active,
        InteractionDefinitionStatus::Archived => InteractionDefinitionStatusDto::Archived,
    }
}

fn parse_uuid(value: &str, field: &'static str) -> Result<Uuid, ApiError> {
    Uuid::parse_str(value).map_err(|_| ApiError::BadRequest(format!("{field} 必须是 UUID")))
}
