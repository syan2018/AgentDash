use std::sync::Arc;

use agentdash_application::story::{
    CreateStoryInput, StoryMutationInput, create_story_record, delete_story_record,
    list_project_stories, update_story_record,
};
use agentdash_application::task::plan::{
    StoryTaskProjectionSourceKind as AppProjectionSourceKind, build_story_task_projection,
};
use axum::Json;
use axum::extract::{Path, Query, State};
use uuid::Uuid;

use agentdash_contracts::common_response::DeletedIdResponse;
use agentdash_contracts::story::{
    StoryTaskProjectionItem, StoryTaskProjectionResponse, StoryTaskProjectionSource,
    StoryTaskProjectionSourceKind,
};
use agentdash_contracts::task::TaskResponse as ContractTaskResponse;
use agentdash_contracts::workflow::SubjectRefDto;

use crate::app_state::AppState;
use crate::auth::{
    CurrentUser, ProjectPermission, load_project_with_permission,
    load_story_and_project_with_permission,
};
use crate::dto::{CreateStoryRequest, ListStoriesQuery, StoryResponse, UpdateStoryRequest};
use crate::rpc::ApiError;

pub async fn list_stories(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Query(query): Query<ListStoriesQuery>,
) -> Result<Json<Vec<StoryResponse>>, ApiError> {
    let stories = if let Some(project_id) = &query.project_id {
        let pid = Uuid::parse_str(project_id)
            .map_err(|_| ApiError::BadRequest("无效的 Project ID".into()))?;
        load_project_with_permission(state.as_ref(), &current_user, pid, ProjectPermission::Use)
            .await?;
        list_project_stories(&state.repos, pid).await?
    } else {
        return Err(ApiError::BadRequest("需要 project_id 参数".into()));
    };

    Ok(Json(stories.into_iter().map(StoryResponse::from).collect()))
}

pub fn router() -> axum::Router<std::sync::Arc<crate::app_state::AppState>> {
    axum::Router::new()
        .route(
            "/stories",
            axum::routing::get(list_stories).post(create_story),
        )
        .route(
            "/stories/{id}",
            axum::routing::get(get_story)
                .put(update_story)
                .delete(delete_story),
        )
        .route(
            "/stories/{id}/task-projection",
            axum::routing::get(get_story_task_projection),
        )
}

pub async fn create_story(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Json(req): Json<CreateStoryRequest>,
) -> Result<Json<StoryResponse>, ApiError> {
    let project_id = Uuid::parse_str(&req.project_id)
        .map_err(|_| ApiError::BadRequest("无效的 Project ID".into()))?;
    let project = load_project_with_permission(
        state.as_ref(),
        &current_user,
        project_id,
        ProjectPermission::Configure,
    )
    .await?;
    let default_workspace_id = req
        .default_workspace_id
        .as_deref()
        .and_then(|s| s.trim().parse::<Uuid>().ok());

    let next_story = create_story_record(
        &state.repos,
        &project,
        CreateStoryInput {
            project_id,
            title: req.title,
            description: req.description,
            mutation: StoryMutationInput {
                default_workspace_id: Some(default_workspace_id),
                status: req.status,
                priority: req.priority,
                story_type: req.story_type,
                tags: req.tags,
                context_source_refs: req.context_source_refs,
                context_containers: req.context_containers,
                disabled_container_ids: req.disabled_container_ids,
                session_composition: req.session_composition.map(Some),
                ..StoryMutationInput::default()
            },
        },
    )
    .await?;

    Ok(Json(StoryResponse::from(next_story)))
}

pub async fn get_story(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(id): Path<String>,
) -> Result<Json<StoryResponse>, ApiError> {
    let story_id =
        Uuid::parse_str(&id).map_err(|_| ApiError::BadRequest("无效的 Story ID".into()))?;
    let (story, _) = load_story_and_project_with_permission(
        state.as_ref(),
        &current_user,
        story_id,
        ProjectPermission::Use,
    )
    .await?;

    Ok(Json(StoryResponse::from(story)))
}

pub async fn update_story(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(id): Path<String>,
    Json(req): Json<UpdateStoryRequest>,
) -> Result<Json<StoryResponse>, ApiError> {
    let story_id =
        Uuid::parse_str(&id).map_err(|_| ApiError::BadRequest("无效的 Story ID".into()))?;

    let (story, project) = load_story_and_project_with_permission(
        state.as_ref(),
        &current_user,
        story_id,
        ProjectPermission::Configure,
    )
    .await?;

    let title = match req.title {
        Some(title) => {
            let trimmed = title.trim();
            if trimmed.is_empty() {
                return Err(ApiError::BadRequest("Story 标题不能为空".into()));
            }
            Some(trimmed.to_string())
        }
        None => None,
    };
    let default_workspace_id = match req.default_workspace_id {
        Some(raw) => {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                Some(None)
            } else {
                Some(Some(trimmed.parse::<Uuid>().map_err(|_| {
                    ApiError::BadRequest("无效的 default_workspace_id".into())
                })?))
            }
        }
        None => None,
    };
    let session_composition = if req.clear_session_composition.unwrap_or(false) {
        Some(None)
    } else {
        req.session_composition.map(Some)
    };
    let status_changed = req.status.is_some();
    let story = update_story_record(
        &state.repos,
        story,
        &project,
        StoryMutationInput {
            title,
            description: req.description,
            default_workspace_id,
            status: req.status,
            priority: req.priority,
            story_type: req.story_type,
            tags: req.tags,
            context_source_refs: req.context_source_refs,
            context_containers: req.context_containers,
            disabled_container_ids: req.disabled_container_ids,
            session_composition,
        },
    )
    .await?;
    let new_status = story.status.clone();

    if status_changed {
        let coordinator = state.services.terminal_cancel_coordinator.clone();
        let story_id = story.id;
        tokio::spawn(async move {
            coordinator
                .on_story_status_changed(story_id, &new_status)
                .await;
        });
    }

    Ok(Json(StoryResponse::from(story)))
}

pub async fn delete_story(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(id): Path<String>,
) -> Result<Json<DeletedIdResponse>, ApiError> {
    let story_id =
        Uuid::parse_str(&id).map_err(|_| ApiError::BadRequest("无效的 Story ID".into()))?;

    let (story, _) = load_story_and_project_with_permission(
        state.as_ref(),
        &current_user,
        story_id,
        ProjectPermission::Configure,
    )
    .await?;

    delete_story_record(&state.repos, &story).await?;

    Ok(Json(DeletedIdResponse { deleted: id }))
}

pub async fn get_story_task_projection(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(id): Path<String>,
) -> Result<Json<StoryTaskProjectionResponse>, ApiError> {
    let story_id =
        Uuid::parse_str(&id).map_err(|_| ApiError::BadRequest("无效的 Story ID".into()))?;
    let (story, project) = load_story_and_project_with_permission(
        state.as_ref(),
        &current_user,
        story_id,
        ProjectPermission::Use,
    )
    .await?;
    let view = build_story_task_projection(
        state.repos.lifecycle_run_repo.as_ref(),
        state.repos.lifecycle_subject_association_repo.as_ref(),
        project.id,
        story.id,
    )
    .await?;

    Ok(Json(StoryTaskProjectionResponse {
        story_id: view.story_id.to_string(),
        tasks: view
            .tasks
            .into_iter()
            .map(|item| StoryTaskProjectionItem {
                task: ContractTaskResponse::from_plan_item(
                    item.project_id.to_string(),
                    item.owning_run_id.to_string(),
                    item.task,
                ),
                sources: item
                    .sources
                    .into_iter()
                    .map(|source| StoryTaskProjectionSource {
                        kind: match source.kind {
                            AppProjectionSourceKind::OwningRun => {
                                StoryTaskProjectionSourceKind::OwningRun
                            }
                            AppProjectionSourceKind::LinkedRun => {
                                StoryTaskProjectionSourceKind::LinkedRun
                            }
                            AppProjectionSourceKind::StoryRef => {
                                StoryTaskProjectionSourceKind::StoryRef
                            }
                        },
                        run_id: source.run_id.to_string(),
                        agent_id: source.agent_id.map(|id| id.to_string()),
                        story_ref: source.story_ref.map(|subject| SubjectRefDto {
                            kind: subject.kind,
                            id: subject.id.to_string(),
                        }),
                        reason: source.reason,
                    })
                    .collect(),
            })
            .collect(),
    }))
}
