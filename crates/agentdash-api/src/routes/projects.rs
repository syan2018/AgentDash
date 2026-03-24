use std::sync::Arc;

use agentdash_application::project::{ProjectAuthorizationContext, ProjectAuthorizationService};
use axum::Json;
use axum::extract::{Path, State};
use serde::Deserialize;
use uuid::Uuid;

use agentdash_domain::context_container::{
    ContextContainerDefinition, MountDerivationPolicy, validate_context_containers,
};
use agentdash_domain::project::{
    Project, ProjectConfig, ProjectRole, ProjectSubjectGrant, ProjectSubjectType, ProjectVisibility,
};
use agentdash_domain::workflow::WorkflowAssignment;
use agentdash_plugin_api::AuthIdentity;

use crate::app_state::AppState;
use crate::auth::{CurrentUser, ProjectPermission, require_project_permission};
use crate::dto::{ProjectDetailResponse, ProjectResponse, ProjectSubjectGrantResponse};
use crate::rpc::ApiError;

#[derive(Deserialize)]
pub struct CreateProjectRequest {
    pub name: String,
    pub description: Option<String>,
    pub config: Option<ProjectConfig>,
    pub visibility: Option<ProjectVisibility>,
    pub is_template: Option<bool>,
    pub cloned_from_project_id: Option<Uuid>,
    pub context_containers: Option<Vec<ContextContainerDefinition>>,
    pub mount_policy: Option<MountDerivationPolicy>,
}

#[derive(Deserialize)]
pub struct UpdateProjectRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub config: Option<ProjectConfig>,
    pub visibility: Option<ProjectVisibility>,
    pub is_template: Option<bool>,
    pub cloned_from_project_id: Option<Uuid>,
    pub context_containers: Option<Vec<ContextContainerDefinition>>,
    pub mount_policy: Option<MountDerivationPolicy>,
}

#[derive(Deserialize)]
pub struct UpsertProjectGrantRequest {
    pub role: ProjectRole,
}

#[derive(Deserialize, Default)]
pub struct CloneProjectRequest {
    pub name: Option<String>,
    pub description: Option<String>,
}

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
    let mut project = Project::new_with_creator(
        req.name,
        req.description.unwrap_or_default(),
        current_user.user_id.clone(),
    );
    if let Some(config) = req.config {
        project.config = config;
    }
    if let Some(visibility) = req.visibility {
        project.visibility = visibility;
    }
    if let Some(is_template) = req.is_template {
        project.is_template = is_template;
    }
    if let Some(cloned_from_project_id) = req.cloned_from_project_id {
        project.cloned_from_project_id = Some(cloned_from_project_id);
    }
    if let Some(context_containers) = req.context_containers {
        project.config.context_containers = context_containers;
    }
    if let Some(mount_policy) = req.mount_policy {
        project.config.mount_policy = mount_policy;
    }
    validate_project_config(&project.config)?;
    validate_project_contract(&project)?;
    state.repos.project_repo.create(&project).await?;
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

    let workspaces = state
        .repos
        .workspace_repo
        .list_by_project(project_id)
        .await?;
    let stories = state.repos.story_repo.list_by_project(project_id).await?;

    Ok(Json(ProjectDetailResponse::new(
        project.clone(),
        resolve_project_access(state.as_ref(), &current_user, &project).await?,
        workspaces,
        stories,
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

    let mut project = state
        .repos
        .project_repo
        .get_by_id(project_id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("Project {id} 不存在")))?;

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

    if let Some(name) = req.name {
        project.name = name;
    }
    if let Some(description) = req.description {
        project.description = description;
    }
    if let Some(config) = req.config {
        project.config = config;
    }
    if let Some(visibility) = req.visibility {
        project.visibility = visibility;
    }
    if let Some(is_template) = req.is_template {
        project.is_template = is_template;
    }
    if let Some(cloned_from_project_id) = req.cloned_from_project_id {
        project.cloned_from_project_id = Some(cloned_from_project_id);
    }
    if let Some(context_containers) = req.context_containers {
        project.config.context_containers = context_containers;
    }
    if let Some(mount_policy) = req.mount_policy {
        project.config.mount_policy = mount_policy;
    }
    validate_project_config(&project.config)?;
    validate_project_contract(&project)?;
    project.touch_updated_by(current_user.user_id.clone());

    state.repos.project_repo.update(&project).await?;
    Ok(Json(
        project_response_for_user(state.as_ref(), &current_user, project).await?,
    ))
}

pub async fn delete_project(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
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

    // 先删除 Project 下的 Task/Story/Workspace，再删除 Project 本身，避免外键约束失败
    let stories = state.repos.story_repo.list_by_project(project_id).await?;
    for story in stories {
        let tasks = state.repos.task_repo.list_by_story(story.id).await?;
        for task in tasks {
            state.repos.task_repo.delete(task.id).await?;
        }
        state.repos.story_repo.delete(story.id).await?;
    }

    let workspaces = state
        .repos
        .workspace_repo
        .list_by_project(project_id)
        .await?;
    for workspace in workspaces {
        state.repos.workspace_repo.delete(workspace.id).await?;
    }

    state.repos.project_repo.delete(project_id).await?;
    Ok(Json(serde_json::json!({ "deleted": id })))
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

    if !source_project.is_template {
        return Err(ApiError::BadRequest(
            "仅模板 Project 支持 clone；请先将源 Project 标记为模板".into(),
        ));
    }

    let mut cloned_project = Project::new_with_creator(
        normalize_clone_name(req.name, &source_project.name)?,
        req.description
            .unwrap_or_else(|| source_project.description.clone()),
        current_user.user_id.clone(),
    );
    cloned_project.config = source_project.config.clone();
    cloned_project.config.default_workspace_id = None;
    cloned_project.visibility = ProjectVisibility::Private;
    cloned_project.is_template = false;
    cloned_project.cloned_from_project_id = Some(source_project.id);
    validate_project_config(&cloned_project.config)?;
    validate_project_contract(&cloned_project)?;

    state.repos.project_repo.create(&cloned_project).await?;
    clone_workflow_assignments(state.as_ref(), source_project.id, cloned_project.id).await?;

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
) -> Result<Json<serde_json::Value>, ApiError> {
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
) -> Result<Json<serde_json::Value>, ApiError> {
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

fn validate_project_config(config: &ProjectConfig) -> Result<(), ApiError> {
    validate_context_containers(&config.context_containers).map_err(ApiError::BadRequest)?;
    Ok(())
}

fn validate_project_contract(project: &Project) -> Result<(), ApiError> {
    if matches!(project.visibility, ProjectVisibility::TemplateVisible) && !project.is_template {
        return Err(ApiError::BadRequest(
            "template_visible 仅适用于模板 Project；请同时设置 is_template=true".into(),
        ));
    }

    Ok(())
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
    state
        .repos
        .project_repo
        .get_by_id(project_id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("Project {raw_id} 不存在")))
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
) -> Result<serde_json::Value, ApiError> {
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

    Ok(serde_json::json!({
        "deleted": {
            "project_id": project_id,
            "subject_type": existing.subject_type,
            "subject_id": existing.subject_id,
        }
    }))
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
    Ok(ProjectResponse::new(project, access))
}

fn normalize_clone_name(raw_name: Option<String>, source_name: &str) -> Result<String, ApiError> {
    match raw_name {
        Some(name) => {
            let trimmed = name.trim();
            if trimmed.is_empty() {
                return Err(ApiError::BadRequest(
                    "clone 后的 Project 名称不能为空".into(),
                ));
            }
            Ok(trimmed.to_string())
        }
        None => Ok(format!("{source_name}（副本）")),
    }
}

async fn clone_workflow_assignments(
    state: &AppState,
    source_project_id: Uuid,
    target_project_id: Uuid,
) -> Result<(), ApiError> {
    let assignments = state
        .repos
        .workflow_assignment_repo
        .list_by_project(source_project_id)
        .await?;

    for assignment in assignments {
        let mut cloned_assignment =
            WorkflowAssignment::new(target_project_id, assignment.workflow_id, assignment.role);
        cloned_assignment.enabled = assignment.enabled;
        cloned_assignment.is_default = assignment.is_default;
        state
            .repos
            .workflow_assignment_repo
            .create(&cloned_assignment)
            .await?;
    }

    Ok(())
}
