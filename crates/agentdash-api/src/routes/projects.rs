use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, State};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use agentdash_domain::context_container::{
    ContextContainerDefinition, MountDerivationPolicy, validate_context_containers,
};
use agentdash_domain::project::{Project, ProjectConfig};
use agentdash_domain::session_composition::{SessionComposition, validate_session_composition};
use agentdash_domain::story::Story;
use agentdash_domain::workspace::Workspace;

use crate::app_state::AppState;
use crate::rpc::ApiError;

#[derive(Deserialize)]
pub struct CreateProjectRequest {
    pub name: String,
    pub description: Option<String>,
    pub backend_id: String,
    pub config: Option<ProjectConfig>,
    pub context_containers: Option<Vec<ContextContainerDefinition>>,
    pub mount_policy: Option<MountDerivationPolicy>,
    pub session_composition: Option<SessionComposition>,
}

#[derive(Deserialize)]
pub struct UpdateProjectRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub backend_id: Option<String>,
    pub config: Option<ProjectConfig>,
    pub context_containers: Option<Vec<ContextContainerDefinition>>,
    pub mount_policy: Option<MountDerivationPolicy>,
    pub session_composition: Option<SessionComposition>,
}

#[derive(Serialize)]
pub struct ProjectDetailResponse {
    #[serde(flatten)]
    pub project: Project,
    pub workspaces: Vec<Workspace>,
    pub stories: Vec<Story>,
}

pub async fn list_projects(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<Project>>, ApiError> {
    let projects = state.project_repo.list_all().await?;
    Ok(Json(projects))
}

pub async fn create_project(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateProjectRequest>,
) -> Result<Json<Project>, ApiError> {
    let mut project = Project::new(
        req.name,
        req.description.unwrap_or_default(),
        req.backend_id,
    );
    if let Some(config) = req.config {
        project.config = config;
    }
    if let Some(context_containers) = req.context_containers {
        project.config.context_containers = context_containers;
    }
    if let Some(mount_policy) = req.mount_policy {
        project.config.mount_policy = mount_policy;
    }
    if let Some(session_composition) = req.session_composition {
        project.config.session_composition = session_composition;
    }
    validate_project_config(&project.config)?;
    state.project_repo.create(&project).await?;
    Ok(Json(project))
}

pub async fn get_project(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ProjectDetailResponse>, ApiError> {
    let project_id =
        Uuid::parse_str(&id).map_err(|_| ApiError::BadRequest("无效的 Project ID".into()))?;

    let project = state
        .project_repo
        .get_by_id(project_id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("Project {id} 不存在")))?;

    let workspaces = state.workspace_repo.list_by_project(project_id).await?;
    let stories = state.story_repo.list_by_project(project_id).await?;

    Ok(Json(ProjectDetailResponse {
        project,
        workspaces,
        stories,
    }))
}

pub async fn update_project(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<UpdateProjectRequest>,
) -> Result<Json<Project>, ApiError> {
    let project_id =
        Uuid::parse_str(&id).map_err(|_| ApiError::BadRequest("无效的 Project ID".into()))?;

    let mut project = state
        .project_repo
        .get_by_id(project_id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("Project {id} 不存在")))?;

    if let Some(name) = req.name {
        project.name = name;
    }
    if let Some(description) = req.description {
        project.description = description;
    }
    if let Some(backend_id) = req.backend_id {
        project.backend_id = backend_id;
    }
    if let Some(config) = req.config {
        project.config = config;
    }
    if let Some(context_containers) = req.context_containers {
        project.config.context_containers = context_containers;
    }
    if let Some(mount_policy) = req.mount_policy {
        project.config.mount_policy = mount_policy;
    }
    if let Some(session_composition) = req.session_composition {
        project.config.session_composition = session_composition;
    }

    validate_project_config(&project.config)?;

    state.project_repo.update(&project).await?;
    Ok(Json(project))
}

pub async fn delete_project(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let project_id =
        Uuid::parse_str(&id).map_err(|_| ApiError::BadRequest("无效的 Project ID".into()))?;

    // 先删除 Project 下的 Task/Story/Workspace，再删除 Project 本身，避免外键约束失败
    let stories = state.story_repo.list_by_project(project_id).await?;
    for story in stories {
        let tasks = state.task_repo.list_by_story(story.id).await?;
        for task in tasks {
            state.task_repo.delete(task.id).await?;
        }
        state.story_repo.delete(story.id).await?;
    }

    let workspaces = state.workspace_repo.list_by_project(project_id).await?;
    for workspace in workspaces {
        state.workspace_repo.delete(workspace.id).await?;
    }

    state.project_repo.delete(project_id).await?;
    Ok(Json(serde_json::json!({ "deleted": id })))
}

fn validate_project_config(config: &ProjectConfig) -> Result<(), ApiError> {
    validate_context_containers(&config.context_containers).map_err(ApiError::BadRequest)?;
    validate_session_composition(&config.session_composition).map_err(ApiError::BadRequest)?;
    Ok(())
}
