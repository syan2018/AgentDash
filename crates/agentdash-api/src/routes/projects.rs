use std::sync::Arc;

use agentdash_application::project::{
    CloneProjectInput, CreateProjectInput, ProjectAuthorizationContext,
    ProjectAuthorizationService, ProjectMutationInput, UpdateProjectInput, clone_project_record,
    create_project_record, delete_project_record, load_project_by_id, load_project_detail_facts,
    update_project_record,
};
use axum::Json;
use axum::extract::{Path, State};
use uuid::Uuid;

use agentdash_contracts::core::{
    DeletedIdResponse, DeletedProjectSubjectGrantResponse,
    ProjectSubjectType as ContractProjectSubjectType, RevokeProjectGrantResponse,
};
use agentdash_domain::project::{Project, ProjectRole, ProjectSubjectGrant, ProjectSubjectType};
use agentdash_plugin_api::AuthIdentity;

use crate::app_state::AppState;
use crate::auth::{CurrentUser, ProjectPermission, require_project_permission};
use crate::dto::{
    CloneProjectRequest, CreateProjectRequest, ProjectAccessSummaryResponse, ProjectDetailResponse,
    ProjectResponse, ProjectSubjectGrantResponse, UpdateProjectRequest, UpsertProjectGrantRequest,
};
use crate::rpc::ApiError;

pub async fn list_projects(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
) -> Result<Json<Vec<ProjectResponse>>, ApiError> {
    let authz = project_authorization_service(state.as_ref());
    let projects = authz
        .list_accessible_projects(&project_authorization_context(&current_user))
        .await?;
    let mut responses = Vec::with_capacity(projects.len());
    for project in projects {
        responses.push(project_response_for_user(state.as_ref(), &current_user, project).await?);
    }
    Ok(Json(responses))
}

pub async fn create_project(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Json(req): Json<CreateProjectRequest>,
) -> Result<Json<ProjectResponse>, ApiError> {
    let project = create_project_record(
        &state.repos,
        CreateProjectInput {
            creator_user_id: current_user.user_id.clone(),
            name: req.name,
            description: req.description,
            mutation: ProjectMutationInput {
                config: req.config,
                visibility: req.visibility,
                is_template: req.is_template,
                cloned_from_project_id: req.cloned_from_project_id,
                context_containers: req.context_containers,
                ..ProjectMutationInput::default()
            },
        },
    )
    .await?;

    Ok(Json(
        project_response_for_user(state.as_ref(), &current_user, project).await?,
    ))
}

pub async fn get_project(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(id): Path<String>,
) -> Result<Json<ProjectDetailResponse>, ApiError> {
    let project_id =
        Uuid::parse_str(&id).map_err(|_| ApiError::BadRequest("无效的 Project ID".into()))?;

    let project = load_project_or_not_found(state.as_ref(), project_id, &id).await?;
    require_project_permission(
        state.as_ref(),
        &current_user,
        &project,
        ProjectPermission::View,
    )
    .await?;

    let detail = load_project_detail_facts(&state.repos, project_id).await?;

    let project_response =
        project_response_for_user(state.as_ref(), &current_user, project).await?;
    Ok(Json(ProjectDetailResponse::from_parts(
        project_response,
        detail.workspaces,
        detail.stories,
    )))
}

pub async fn update_project(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(id): Path<String>,
    Json(req): Json<UpdateProjectRequest>,
) -> Result<Json<ProjectResponse>, ApiError> {
    let project_id =
        Uuid::parse_str(&id).map_err(|_| ApiError::BadRequest("无效的 Project ID".into()))?;

    let project = load_project_or_not_found(state.as_ref(), project_id, &id).await?;

    let requires_owner = req.visibility.is_some()
        || req.is_template.is_some()
        || req.cloned_from_project_id.is_some();
    require_project_permission(
        state.as_ref(),
        &current_user,
        &project,
        if requires_owner {
            ProjectPermission::ManageSharing
        } else {
            ProjectPermission::Edit
        },
    )
    .await?;

    let project = update_project_record(
        &state.repos,
        project,
        UpdateProjectInput {
            updated_by_user_id: current_user.user_id.clone(),
            mutation: ProjectMutationInput {
                name: req.name,
                description: req.description,
                config: req.config,
                visibility: req.visibility,
                is_template: req.is_template,
                cloned_from_project_id: req.cloned_from_project_id,
                context_containers: req.context_containers,
            },
        },
    )
    .await?;

    Ok(Json(
        project_response_for_user(state.as_ref(), &current_user, project).await?,
    ))
}

pub async fn delete_project(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(id): Path<String>,
) -> Result<Json<DeletedIdResponse>, ApiError> {
    let project_id =
        Uuid::parse_str(&id).map_err(|_| ApiError::BadRequest("无效的 Project ID".into()))?;
    let project = load_project_or_not_found(state.as_ref(), project_id, &id).await?;
    require_project_permission(
        state.as_ref(),
        &current_user,
        &project,
        ProjectPermission::ManageSharing,
    )
    .await?;

    delete_project_record(&state.repos, project_id).await?;
    Ok(Json(DeletedIdResponse { deleted: id }))
}

pub async fn clone_project(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(id): Path<String>,
    Json(req): Json<CloneProjectRequest>,
) -> Result<Json<ProjectResponse>, ApiError> {
    let project_id =
        Uuid::parse_str(&id).map_err(|_| ApiError::BadRequest("无效的 Project ID".into()))?;
    let source_project = load_project_or_not_found(state.as_ref(), project_id, &id).await?;
    require_project_permission(
        state.as_ref(),
        &current_user,
        &source_project,
        ProjectPermission::View,
    )
    .await?;

    let cloned_project = clone_project_record(
        &state.repos,
        &source_project,
        CloneProjectInput {
            creator_user_id: current_user.user_id.clone(),
            name: req.name,
            description: req.description,
        },
    )
    .await?;

    Ok(Json(
        project_response_for_user(state.as_ref(), &current_user, cloned_project).await?,
    ))
}

pub async fn list_project_grants(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(id): Path<String>,
) -> Result<Json<Vec<ProjectSubjectGrantResponse>>, ApiError> {
    let project_id =
        Uuid::parse_str(&id).map_err(|_| ApiError::BadRequest("无效的 Project ID".into()))?;
    let project = load_project_or_not_found(state.as_ref(), project_id, &id).await?;
    require_project_permission(
        state.as_ref(),
        &current_user,
        &project,
        ProjectPermission::ManageSharing,
    )
    .await?;

    let grants = state
        .repos
        .project_repo
        .list_subject_grants(project_id)
        .await?;
    Ok(Json(
        grants
            .into_iter()
            .map(ProjectSubjectGrantResponse::from)
            .collect(),
    ))
}

pub async fn grant_project_user(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path((id, user_id)): Path<(String, String)>,
    Json(req): Json<UpsertProjectGrantRequest>,
) -> Result<Json<ProjectSubjectGrantResponse>, ApiError> {
    upsert_project_grant(
        state.as_ref(),
        &current_user,
        &id,
        ProjectSubjectType::User,
        &user_id,
        req.role,
    )
    .await
    .map(Json)
}

pub async fn revoke_project_user(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path((id, user_id)): Path<(String, String)>,
) -> Result<Json<RevokeProjectGrantResponse>, ApiError> {
    revoke_project_grant(
        state.as_ref(),
        &current_user,
        &id,
        ProjectSubjectType::User,
        &user_id,
    )
    .await
    .map(Json)
}

pub async fn grant_project_group(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path((id, group_id)): Path<(String, String)>,
    Json(req): Json<UpsertProjectGrantRequest>,
) -> Result<Json<ProjectSubjectGrantResponse>, ApiError> {
    upsert_project_grant(
        state.as_ref(),
        &current_user,
        &id,
        ProjectSubjectType::Group,
        &group_id,
        req.role,
    )
    .await
    .map(Json)
}

pub async fn revoke_project_group(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path((id, group_id)): Path<(String, String)>,
) -> Result<Json<RevokeProjectGrantResponse>, ApiError> {
    revoke_project_grant(
        state.as_ref(),
        &current_user,
        &id,
        ProjectSubjectType::Group,
        &group_id,
    )
    .await
    .map(Json)
}

fn project_authorization_context(current_user: &AuthIdentity) -> ProjectAuthorizationContext {
    ProjectAuthorizationContext::new(
        current_user.user_id.clone(),
        current_user
            .groups
            .iter()
            .map(|group| group.group_id.clone())
            .collect(),
        current_user.is_admin,
    )
}

fn project_authorization_service<'a>(
    state: &'a AppState,
) -> ProjectAuthorizationService<'a, dyn agentdash_domain::project::ProjectRepository> {
    ProjectAuthorizationService::new(state.repos.project_repo.as_ref())
}

async fn load_project_or_not_found(
    state: &AppState,
    project_id: Uuid,
    raw_id: &str,
) -> Result<Project, ApiError> {
    load_project_by_id(&state.repos, project_id, raw_id)
        .await
        .map_err(ApiError::from)
}

async fn resolve_project_access(
    state: &AppState,
    current_user: &AuthIdentity,
    project: &Project,
) -> Result<agentdash_application::project::ProjectAuthorization, ApiError> {
    let authz = project_authorization_service(state);
    authz
        .resolve_project_access(&project_authorization_context(current_user), project)
        .await
        .map_err(ApiError::from)
}

async fn upsert_project_grant(
    state: &AppState,
    current_user: &AuthIdentity,
    raw_project_id: &str,
    subject_type: ProjectSubjectType,
    subject_id: &str,
    role: ProjectRole,
) -> Result<ProjectSubjectGrantResponse, ApiError> {
    let project_id = parse_project_id(raw_project_id)?;
    let project = load_project_or_not_found(state, project_id, raw_project_id).await?;
    require_project_permission(
        state,
        current_user,
        &project,
        ProjectPermission::ManageSharing,
    )
    .await?;

    let subject_id = normalize_subject_id(subject_type, subject_id)?;
    ensure_project_subject_exists(state, subject_type, &subject_id).await?;

    let authz = project_authorization_service(state);
    if authz
        .would_leave_project_without_owner(project_id, subject_type, &subject_id, Some(role))
        .await?
    {
        return Err(ApiError::Conflict(
            "Project 至少需要保留一个 owner，当前变更会移除最后一个 owner".into(),
        ));
    }

    let grant = ProjectSubjectGrant::new(
        project_id,
        subject_type,
        subject_id.clone(),
        role,
        current_user.user_id.clone(),
    );
    state
        .repos
        .project_repo
        .upsert_subject_grant(&grant)
        .await?;

    find_project_grant(state, project_id, subject_type, &subject_id)
        .await?
        .map(ProjectSubjectGrantResponse::from)
        .ok_or_else(|| ApiError::Internal("Project grant 写入成功但读取结果缺失".into()))
}

async fn revoke_project_grant(
    state: &AppState,
    current_user: &AuthIdentity,
    raw_project_id: &str,
    subject_type: ProjectSubjectType,
    subject_id: &str,
) -> Result<RevokeProjectGrantResponse, ApiError> {
    let project_id = parse_project_id(raw_project_id)?;
    let project = load_project_or_not_found(state, project_id, raw_project_id).await?;
    require_project_permission(
        state,
        current_user,
        &project,
        ProjectPermission::ManageSharing,
    )
    .await?;

    let subject_id = normalize_subject_id(subject_type, subject_id)?;
    let existing = find_project_grant(state, project_id, subject_type, &subject_id)
        .await?
        .ok_or_else(|| {
            ApiError::NotFound(format!(
                "Project {} 不存在该共享记录: {} {}",
                project_id,
                subject_type.as_str(),
                subject_id
            ))
        })?;

    let authz = project_authorization_service(state);
    if authz
        .would_leave_project_without_owner(project_id, subject_type, &subject_id, None)
        .await?
    {
        return Err(ApiError::Conflict(
            "Project 至少需要保留一个 owner，当前撤销会移除最后一个 owner".into(),
        ));
    }

    state
        .repos
        .project_repo
        .delete_subject_grant(project_id, subject_type, &subject_id)
        .await?;

    Ok(RevokeProjectGrantResponse {
        deleted: DeletedProjectSubjectGrantResponse {
            project_id: project_id.to_string(),
            subject_type: ContractProjectSubjectType::from(existing.subject_type),
            subject_id: existing.subject_id,
        },
    })
}

async fn find_project_grant(
    state: &AppState,
    project_id: Uuid,
    subject_type: ProjectSubjectType,
    subject_id: &str,
) -> Result<Option<ProjectSubjectGrant>, ApiError> {
    let grants = state
        .repos
        .project_repo
        .list_subject_grants(project_id)
        .await?;
    Ok(grants
        .into_iter()
        .find(|grant| grant.subject_type == subject_type && grant.subject_id == subject_id))
}

async fn ensure_project_subject_exists(
    state: &AppState,
    subject_type: ProjectSubjectType,
    subject_id: &str,
) -> Result<(), ApiError> {
    match subject_type {
        ProjectSubjectType::User => {
            let user = state
                .repos
                .user_directory_repo
                .get_user_by_id(subject_id)
                .await?;
            if user.is_none() {
                return Err(ApiError::NotFound(format!(
                    "用户 `{subject_id}` 尚未出现在身份目录投影中，暂时无法授权"
                )));
            }
        }
        ProjectSubjectType::Group => {
            let group = state
                .repos
                .user_directory_repo
                .get_group_by_id(subject_id)
                .await?;
            if group.is_none() {
                return Err(ApiError::NotFound(format!(
                    "用户组 `{subject_id}` 尚未出现在 claim 投影目录中，暂时无法授权"
                )));
            }
        }
    }

    Ok(())
}

fn parse_project_id(raw_project_id: &str) -> Result<Uuid, ApiError> {
    Uuid::parse_str(raw_project_id).map_err(|_| ApiError::BadRequest("无效的 Project ID".into()))
}

fn normalize_subject_id(
    subject_type: ProjectSubjectType,
    subject_id: &str,
) -> Result<String, ApiError> {
    let normalized = subject_id.trim();
    if normalized.is_empty() {
        return Err(ApiError::BadRequest(format!(
            "{} 不能为空",
            match subject_type {
                ProjectSubjectType::User => "user_id",
                ProjectSubjectType::Group => "group_id",
            }
        )));
    }

    Ok(normalized.to_string())
}

async fn project_response_for_user(
    state: &AppState,
    current_user: &AuthIdentity,
    project: Project,
) -> Result<ProjectResponse, ApiError> {
    let access = resolve_project_access(state, current_user, &project).await?;
    Ok(ProjectResponse::from_project(
        project,
        project_access_response(access),
    ))
}

fn project_access_response(
    access: agentdash_application::project::ProjectAuthorization,
) -> ProjectAccessSummaryResponse {
    ProjectAccessSummaryResponse {
        role: access
            .role
            .map(agentdash_contracts::core::ProjectRole::from),
        can_view: access.can_view_project(),
        can_edit: access.can_edit_project(),
        can_manage_sharing: access.can_manage_project_sharing(),
        via_admin_bypass: access.via_admin_bypass,
        via_template_visibility: access.via_template_visibility,
    }
}
