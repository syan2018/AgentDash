use std::sync::Arc;

use agentdash_application::interaction::{
    CanvasDefinitionAccessResolver, CanvasDefinitionListScope, CanvasDefinitionService,
    CanvasDefinitionView, CanvasProjectAccess, CommitCanvasDefinitionInput,
    CopyCanvasDefinitionInput, CreateCanvasDefinitionInput, InteractionApplicationError,
    InteractionApplicationResult, PublishCanvasDefinitionInput,
};
use agentdash_contracts::interaction::{
    ArchiveInteractionDefinitionResponse, CanvasDefinitionDto, CanvasDefinitionListScopeDto,
    CommitCanvasDefinitionRequest, CreateCanvasDefinitionRequest,
    DistributeCanvasDefinitionRequest, InteractionDefinitionAccessDto,
    InteractionDefinitionLineageDto, InteractionDefinitionLineageKindDto,
    InteractionDefinitionStatusDto, InteractionOwnerDto, InteractionSourceBundleDto,
    InteractionSourceChangesetDto, InteractionSourceFileChangeDto, InteractionSourceFileDto,
    InteractionSourceSandboxDto, ListCanvasDefinitionsQuery,
};
use agentdash_domain::interaction::{
    DefinitionLineageKind, InteractionDefinition, InteractionDefinitionStatus, InteractionOwner,
    SourceBundle, SourceBundleChangeset, SourceFile, SourceFileChange, SourceSandboxConfig,
};
use agentdash_domain::project::{ProjectAuthorizationContext, ProjectAuthorizationService};
use async_trait::async_trait;
use axum::Json;
use axum::extract::{Path, Query, State};
use chrono::Utc;
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
