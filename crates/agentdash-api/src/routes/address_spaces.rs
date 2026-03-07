use std::path::PathBuf;
use std::sync::Arc;

use agentdash_domain::{project::Project, story::Story, task::Task, workspace::Workspace};
use axum::{
    Json,
    extract::{Path, Query, State},
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::app_state::AppState;
use crate::rpc::ApiError;
use crate::routes::workspace_files::{
    FileEntry, MAX_LIST_RESULTS, normalize_path_display, walk_files,
};

const WORKSPACE_FILE_SPACE_ID: &str = "workspace_file";

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddressSpaceQuery {
    pub project_id: Option<String>,
    pub story_id: Option<String>,
    pub task_id: Option<String>,
    pub workspace_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddressEntriesQuery {
    pub project_id: Option<String>,
    pub story_id: Option<String>,
    pub task_id: Option<String>,
    pub workspace_id: Option<String>,
    pub query: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AddressSpaceSelectorResponse {
    pub trigger: String,
    pub placeholder: String,
    pub result_item_type: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AddressSpaceDescriptorResponse {
    pub id: String,
    pub label: String,
    pub kind: String,
    pub provider: String,
    pub supports: Vec<String>,
    pub root: Option<String>,
    pub workspace_id: Option<String>,
    pub selector: Option<AddressSpaceSelectorResponse>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ListAddressSpacesResponse {
    pub spaces: Vec<AddressSpaceDescriptorResponse>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ListAddressEntriesResponse {
    pub space_id: String,
    pub root: String,
    pub workspace_id: String,
    pub entries: Vec<FileEntry>,
}

pub async fn list_address_spaces(
    State(state): State<Arc<AppState>>,
    Query(query): Query<AddressSpaceQuery>,
) -> Result<Json<ListAddressSpacesResponse>, ApiError> {
    let workspace = resolve_workspace_from_query(&state, &query).await?;

    let spaces = workspace
        .as_ref()
        .map(|workspace| {
            vec![AddressSpaceDescriptorResponse {
                id: WORKSPACE_FILE_SPACE_ID.to_string(),
                label: "工作空间文件".to_string(),
                kind: "file".to_string(),
                provider: "workspace".to_string(),
                supports: vec!["search".to_string(), "browse".to_string(), "read".to_string()],
                root: Some(normalize_path_display(PathBuf::from(&workspace.container_ref).as_path())),
                workspace_id: Some(workspace.id.to_string()),
                selector: Some(AddressSpaceSelectorResponse {
                    trigger: "@".to_string(),
                    placeholder: "搜索工作空间文件".to_string(),
                    result_item_type: "file".to_string(),
                }),
            }]
        })
        .unwrap_or_default();

    Ok(Json(ListAddressSpacesResponse { spaces }))
}

pub async fn list_address_entries(
    State(state): State<Arc<AppState>>,
    Path(space_id): Path<String>,
    Query(query): Query<AddressEntriesQuery>,
) -> Result<Json<ListAddressEntriesResponse>, ApiError> {
    if space_id != WORKSPACE_FILE_SPACE_ID {
        return Err(ApiError::NotFound(format!("不支持的寻址空间: {space_id}")));
    }

    let workspace = resolve_workspace_from_entry_query(&state, &query)
        .await?
        .ok_or_else(|| ApiError::BadRequest("当前环境没有可用的工作空间文件寻址能力".into()))?;

    let root = PathBuf::from(&workspace.container_ref);
    let pattern = query.query.unwrap_or_default().to_lowercase();
    let root_for_walk = root.clone();
    let entries = tokio::task::spawn_blocking(move || walk_files(&root_for_walk, &pattern, MAX_LIST_RESULTS))
        .await
        .map_err(|e| ApiError::Internal(format!("文件列表任务异常: {e}")))?;

    Ok(Json(ListAddressEntriesResponse {
        space_id,
        root: normalize_path_display(root.as_path()),
        workspace_id: workspace.id.to_string(),
        entries,
    }))
}

async fn resolve_workspace_from_entry_query(
    state: &Arc<AppState>,
    query: &AddressEntriesQuery,
) -> Result<Option<Workspace>, ApiError> {
    resolve_workspace(
        state,
        query.project_id.as_deref(),
        query.story_id.as_deref(),
        query.task_id.as_deref(),
        query.workspace_id.as_deref(),
    )
    .await
}

async fn resolve_workspace_from_query(
    state: &Arc<AppState>,
    query: &AddressSpaceQuery,
) -> Result<Option<Workspace>, ApiError> {
    resolve_workspace(
        state,
        query.project_id.as_deref(),
        query.story_id.as_deref(),
        query.task_id.as_deref(),
        query.workspace_id.as_deref(),
    )
    .await
}

async fn resolve_workspace(
    state: &Arc<AppState>,
    project_id: Option<&str>,
    story_id: Option<&str>,
    task_id: Option<&str>,
    workspace_id: Option<&str>,
) -> Result<Option<Workspace>, ApiError> {
    if let Some(workspace_id) = workspace_id {
        let workspace_id = parse_uuid(workspace_id, "workspace_id")?;
        return state
            .workspace_repo
            .get_by_id(workspace_id)
            .await
            .map_err(|e| ApiError::Internal(e.to_string()));
    }

    if let Some(task_id) = task_id {
        let task_id = parse_uuid(task_id, "task_id")?;
        let task = state
            .task_repo
            .get_by_id(task_id)
            .await
            .map_err(|e| ApiError::Internal(e.to_string()))?
            .ok_or_else(|| ApiError::NotFound(format!("Task {task_id} 不存在")))?;
        return resolve_workspace_for_task(state, &task).await;
    }

    if let Some(story_id) = story_id {
        let story_id = parse_uuid(story_id, "story_id")?;
        let story = state
            .story_repo
            .get_by_id(story_id)
            .await
            .map_err(|e| ApiError::Internal(e.to_string()))?
            .ok_or_else(|| ApiError::NotFound(format!("Story {story_id} 不存在")))?;
        return resolve_workspace_for_story(state, &story).await;
    }

    if let Some(project_id) = project_id {
        let project_id = parse_uuid(project_id, "project_id")?;
        let project = state
            .project_repo
            .get_by_id(project_id)
            .await
            .map_err(|e| ApiError::Internal(e.to_string()))?
            .ok_or_else(|| ApiError::NotFound(format!("Project {project_id} 不存在")))?;
        return resolve_workspace_for_project(state, &project).await;
    }

    Ok(None)
}

async fn resolve_workspace_for_task(
    state: &Arc<AppState>,
    task: &Task,
) -> Result<Option<Workspace>, ApiError> {
    if let Some(workspace_id) = task.workspace_id {
        return state
            .workspace_repo
            .get_by_id(workspace_id)
            .await
            .map_err(|e| ApiError::Internal(e.to_string()));
    }

    let story = state
        .story_repo
        .get_by_id(task.story_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound(format!("Story {} 不存在", task.story_id)))?;

    resolve_workspace_for_story(state, &story).await
}

async fn resolve_workspace_for_story(
    state: &Arc<AppState>,
    story: &Story,
) -> Result<Option<Workspace>, ApiError> {
    let project = state
        .project_repo
        .get_by_id(story.project_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound(format!("Project {} 不存在", story.project_id)))?;

    resolve_workspace_for_project(state, &project).await
}

async fn resolve_workspace_for_project(
    state: &Arc<AppState>,
    project: &Project,
) -> Result<Option<Workspace>, ApiError> {
    if let Some(workspace_id) = project.config.default_workspace_id {
        return state
            .workspace_repo
            .get_by_id(workspace_id)
            .await
            .map_err(|e| ApiError::Internal(e.to_string()));
    }

    let workspaces = state
        .workspace_repo
        .list_by_project(project.id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    Ok(workspaces.into_iter().next())
}

fn parse_uuid(raw: &str, field: &str) -> Result<Uuid, ApiError> {
    Uuid::parse_str(raw.trim()).map_err(|_| ApiError::BadRequest(format!("无效的 {field}")))
}
