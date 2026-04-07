use std::sync::Arc;

use agentdash_application::story::{
    AgentBindingInput, StoryMutationInput, TaskMutationInput, apply_story_mutation,
    apply_task_mutation, build_agent_binding, build_story, build_task, delete_story_aggregate,
};
use agentdash_application::task::management::{create_task_aggregate, delete_task_aggregate};
use axum::Json;
use axum::extract::{Path, Query, State};
use serde::Deserialize;
use uuid::Uuid;

use agentdash_domain::context_container::{
    ContextContainerDefinition, validate_context_containers,
    validate_disabled_container_ids,
};
use agentdash_domain::context_source::ContextSourceRef;
use agentdash_domain::project::Project;
use agentdash_domain::session_composition::{SessionComposition, validate_session_composition};
use agentdash_domain::story::{ChangeKind, Story, StoryPriority, StoryStatus, StoryType};
use agentdash_domain::task::TaskStatus;

use crate::app_state::AppState;
use crate::auth::{
    CurrentUser, ProjectPermission, load_project_with_permission,
    load_story_and_project_with_permission, load_task_story_project_with_permission,
};
use crate::dto::{StoryResponse, TaskResponse};
use crate::rpc::ApiError;
use agentdash_domain::story::StateChangeRepository;

#[derive(Deserialize)]
pub struct ListStoriesQuery {
    pub project_id: Option<String>,
}

#[derive(Deserialize)]
pub struct CreateStoryRequest {
    pub project_id: String,
    pub title: String,
    pub description: Option<String>,
    pub priority: Option<StoryPriority>,
    pub story_type: Option<StoryType>,
    pub tags: Option<Vec<String>>,
    pub default_workspace_id: Option<String>,
    pub context_source_refs: Option<Vec<ContextSourceRef>>,
    pub context_containers: Option<Vec<ContextContainerDefinition>>,
    pub disabled_container_ids: Option<Vec<String>>,
    pub session_composition: Option<SessionComposition>,
}

#[derive(Deserialize, Default)]
pub struct UpdateStoryRequest {
    pub title: Option<String>,
    pub description: Option<String>,
    pub default_workspace_id: Option<String>,
    pub status: Option<StoryStatus>,
    pub priority: Option<StoryPriority>,
    pub story_type: Option<StoryType>,
    pub tags: Option<Vec<String>>,
    pub context_source_refs: Option<Vec<ContextSourceRef>>,
    pub context_containers: Option<Vec<ContextContainerDefinition>>,
    pub disabled_container_ids: Option<Vec<String>>,
    pub session_composition: Option<SessionComposition>,
    pub clear_session_composition: Option<bool>,
}

#[derive(Deserialize, Default)]
pub struct CreateTaskAgentBindingRequest {
    pub agent_type: Option<String>,
    pub agent_pid: Option<String>,
    pub preset_name: Option<String>,
    pub prompt_template: Option<String>,
    pub initial_context: Option<String>,
    pub context_sources: Option<Vec<ContextSourceRef>>,
}

#[derive(Deserialize)]
pub struct CreateTaskRequest {
    pub title: String,
    pub description: Option<String>,
    pub workspace_id: Option<String>,
    pub agent_binding: Option<CreateTaskAgentBindingRequest>,
}

#[derive(Deserialize, Default)]
pub struct UpdateTaskRequest {
    pub title: Option<String>,
    pub description: Option<String>,
    pub status: Option<TaskStatus>,
    pub workspace_id: Option<String>,
    pub agent_binding: Option<CreateTaskAgentBindingRequest>,
}

pub async fn list_stories(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Query(query): Query<ListStoriesQuery>,
) -> Result<Json<Vec<StoryResponse>>, ApiError> {
    let stories = if let Some(project_id) = &query.project_id {
        let pid = Uuid::parse_str(project_id)
            .map_err(|_| ApiError::BadRequest("无效的 Project ID".into()))?;
        load_project_with_permission(state.as_ref(), &current_user, pid, ProjectPermission::View)
            .await?;
        state.repos.story_repo.list_by_project(pid).await?
    } else {
        return Err(ApiError::BadRequest("需要 project_id 参数".into()));
    };

    Ok(Json(stories.into_iter().map(StoryResponse::from).collect()))
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
        ProjectPermission::Edit,
    )
    .await?;
    let title = req.title.trim();
    if title.is_empty() {
        return Err(ApiError::BadRequest("Story 标题不能为空".into()));
    }

    let default_workspace_id = req
        .default_workspace_id
        .as_deref()
        .and_then(|s| s.trim().parse::<Uuid>().ok());

    let next_story = build_story(
        project_id,
        title.to_string(),
        req.description.unwrap_or_default(),
        StoryMutationInput {
            default_workspace_id: Some(default_workspace_id),
            priority: req.priority,
            story_type: req.story_type,
            tags: req.tags,
            context_source_refs: req.context_source_refs,
            context_containers: req.context_containers,
            disabled_container_ids: req.disabled_container_ids,
            session_composition: req.session_composition.map(Some),
            ..StoryMutationInput::default()
        },
    );
    validate_story_context(&next_story, &project)?;

    state.repos.story_repo.create(&next_story).await?;
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
        ProjectPermission::View,
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

    let (mut story, project) = load_story_and_project_with_permission(
        state.as_ref(),
        &current_user,
        story_id,
        ProjectPermission::Edit,
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
    apply_story_mutation(
        &mut story,
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
    );

    validate_story_context(&story, &project)?;
    let new_status = story.status.clone();
    state.repos.story_repo.update(&story).await?;

    if status_changed {
        let reconciler = state.services.runtime_reconciler.clone();
        let story_id = story.id;
        tokio::spawn(async move {
            reconciler.on_story_status_changed(story_id, &new_status).await;
        });
    }

    Ok(Json(StoryResponse::from(story)))
}

pub async fn delete_story(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let story_id =
        Uuid::parse_str(&id).map_err(|_| ApiError::BadRequest("无效的 Story ID".into()))?;

    let (story, _) = load_story_and_project_with_permission(
        state.as_ref(),
        &current_user,
        story_id,
        ProjectPermission::Edit,
    )
    .await?;

    delete_story_aggregate(
        state.repos.story_repo.as_ref(),
        state.repos.state_change_repo.as_ref(),
        state.repos.task_repo.as_ref(),
        state.repos.task_command_repo.as_ref(),
        &story,
    )
    .await?;

    Ok(Json(serde_json::json!({ "deleted": id })))
}

pub async fn list_tasks(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(id): Path<String>,
) -> Result<Json<Vec<TaskResponse>>, ApiError> {
    let story_id =
        Uuid::parse_str(&id).map_err(|_| ApiError::BadRequest("无效的 Story ID".into()))?;
    load_story_and_project_with_permission(
        state.as_ref(),
        &current_user,
        story_id,
        ProjectPermission::View,
    )
    .await?;
    let tasks = state.repos.task_repo.list_by_story(story_id).await?;
    Ok(Json(tasks.into_iter().map(TaskResponse::from).collect()))
}

pub async fn create_task(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(id): Path<String>,
    Json(req): Json<CreateTaskRequest>,
) -> Result<Json<TaskResponse>, ApiError> {
    let story_id =
        Uuid::parse_str(&id).map_err(|_| ApiError::BadRequest("无效的 Story ID".into()))?;

    let title = req.title.trim();
    if title.is_empty() {
        return Err(ApiError::BadRequest("Task 标题不能为空".into()));
    }

    let (story, project) = load_story_and_project_with_permission(
        state.as_ref(),
        &current_user,
        story_id,
        ProjectPermission::Edit,
    )
    .await?;

    let workspace_id = match req.workspace_id.as_deref() {
        Some(raw) if !raw.trim().is_empty() => {
            let ws_id = Uuid::parse_str(raw.trim())
                .map_err(|_| ApiError::BadRequest("无效的 Workspace ID".into()))?;

            let workspace = state
                .repos
                .workspace_repo
                .get_by_id(ws_id)
                .await?
                .ok_or_else(|| ApiError::NotFound(format!("Workspace {ws_id} 不存在")))?;

            if workspace.project_id != story.project_id {
                return Err(ApiError::Conflict(
                    "Workspace 与 Story 不属于同一 Project".into(),
                ));
            }

            Some(ws_id)
        }
        _ => None,
    };

    let mut agent_binding = build_agent_binding(req.agent_binding.map(|value| AgentBindingInput {
        agent_type: value.agent_type,
        agent_pid: value.agent_pid,
        preset_name: value.preset_name,
        prompt_template: value.prompt_template,
        initial_context: value.initial_context,
        context_sources: value.context_sources,
    }));

    if let Some(preset_name) = agent_binding.preset_name.clone() {
        let preset = project
            .config
            .agent_presets
            .iter()
            .find(|p| p.name == preset_name)
            .ok_or_else(|| ApiError::BadRequest(format!("Project 中不存在预设: {preset_name}")))?;

        if agent_binding.agent_type.is_none() {
            agent_binding.agent_type = Some(preset.agent_type.clone());
        }
    }

    if agent_binding.agent_type.is_none() {
        agent_binding.agent_type = project.config.default_agent_type.clone();
    }

    if agent_binding.agent_type.is_none() && agent_binding.preset_name.is_none() {
        return Err(ApiError::UnprocessableEntity(
            "请指定 Agent 类型或预设，或在 Project 配置中设置 default_agent_type".into(),
        ));
    }

    let task = build_task(
        story.project_id,
        story_id,
        title.to_string(),
        req.description.unwrap_or_default(),
        workspace_id,
        agent_binding,
    );

    create_task_aggregate(state.repos.task_command_repo.as_ref(), &task).await?;

    Ok(Json(TaskResponse::from(task)))
}

pub async fn get_task(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(id): Path<String>,
) -> Result<Json<TaskResponse>, ApiError> {
    let task_id =
        Uuid::parse_str(&id).map_err(|_| ApiError::BadRequest("无效的 Task ID".into()))?;

    let (task, _, _) = load_task_story_project_with_permission(
        state.as_ref(),
        &current_user,
        task_id,
        ProjectPermission::View,
    )
    .await?;

    Ok(Json(TaskResponse::from(task)))
}

pub async fn update_task(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(id): Path<String>,
    Json(req): Json<UpdateTaskRequest>,
) -> Result<Json<TaskResponse>, ApiError> {
    let task_id =
        Uuid::parse_str(&id).map_err(|_| ApiError::BadRequest("无效的 Task ID".into()))?;

    let (mut task, story, _) = load_task_story_project_with_permission(
        state.as_ref(),
        &current_user,
        task_id,
        ProjectPermission::Edit,
    )
    .await?;

    let old_status = task.status.clone();

    let title = match req.title {
        Some(title) => {
            let trimmed = title.trim();
            if trimmed.is_empty() {
                return Err(ApiError::BadRequest("Task 标题不能为空".into()));
            }
            Some(trimmed.to_string())
        }
        None => None,
    };

    let workspace_id = if let Some(workspace_id_raw) = req.workspace_id {
        let normalized = workspace_id_raw.trim();
        if normalized.is_empty() {
            Some(None)
        } else {
            let ws_id = Uuid::parse_str(normalized)
                .map_err(|_| ApiError::BadRequest("无效的 Workspace ID".into()))?;
            let workspace = state
                .repos
                .workspace_repo
                .get_by_id(ws_id)
                .await?
                .ok_or_else(|| ApiError::NotFound(format!("Workspace {ws_id} 不存在")))?;
            if workspace.project_id != story.project_id {
                return Err(ApiError::Conflict(
                    "Workspace 与 Task 所属 Story 不属于同一 Project".into(),
                ));
            }
            Some(Some(ws_id))
        }
    } else {
        None
    };

    let agent_binding = req.agent_binding.map(|value| {
        build_agent_binding(Some(AgentBindingInput {
            agent_type: value.agent_type,
            agent_pid: value.agent_pid,
            preset_name: value.preset_name,
            prompt_template: value.prompt_template,
            initial_context: value.initial_context,
            context_sources: value.context_sources,
        }))
    });
    apply_task_mutation(
        &mut task,
        TaskMutationInput {
            title,
            description: req.description,
            workspace_id,
            status: req.status,
            agent_binding,
        },
    );

    state.repos.task_repo.update(&task).await?;

    let change_kind = classify_task_change_kind(&old_status, &task.status);
    let payload = serde_json::to_value(&task)
        .map_err(|err| ApiError::Internal(format!("序列化 Task 状态变更失败: {err}")))?;
    append_required_story_change(
        state.repos.state_change_repo.as_ref(),
        task.project_id,
        task.id,
        change_kind,
        payload,
        None,
    )
    .await?;

    // 运行时对账：Task 进入终态时取消关联 session
    if old_status != task.status {
        let reconciler = state.services.runtime_reconciler.clone();
        let task_id = task.id;
        let new_status = task.status.clone();
        tokio::spawn(async move {
            reconciler.on_task_status_changed(task_id, &new_status).await;
        });
    }

    Ok(Json(TaskResponse::from(task)))
}

pub async fn delete_task(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let task_id =
        Uuid::parse_str(&id).map_err(|_| ApiError::BadRequest("无效的 Task ID".into()))?;
    load_task_story_project_with_permission(
        state.as_ref(),
        &current_user,
        task_id,
        ProjectPermission::Edit,
    )
    .await?;

    delete_task_aggregate(state.repos.task_command_repo.as_ref(), task_id).await?;

    state.task_runtime.restart_tracker.clear(task_id);

    Ok(Json(serde_json::json!({ "deleted": id })))
}

fn validate_story_context(story: &Story, project: &Project) -> Result<(), ApiError> {
    validate_context_containers(&story.context.context_containers).map_err(ApiError::BadRequest)?;
    validate_disabled_container_ids(
        &story.context.disabled_container_ids,
        &project.config.context_containers,
    )
    .map_err(ApiError::BadRequest)?;
    if let Some(session_composition) = &story.context.session_composition {
        validate_session_composition(session_composition).map_err(ApiError::BadRequest)?;
    }
    Ok(())
}

fn classify_task_change_kind(old_status: &TaskStatus, new_status: &TaskStatus) -> ChangeKind {
    if new_status != old_status {
        ChangeKind::TaskStatusChanged
    } else {
        ChangeKind::TaskUpdated
    }
}

async fn append_required_story_change(
    repo: &dyn StateChangeRepository,
    project_id: Uuid,
    entity_id: Uuid,
    kind: ChangeKind,
    payload: serde_json::Value,
    backend_id: Option<&str>,
) -> Result<(), ApiError> {
    repo.append_change(project_id, entity_id, kind, payload, backend_id)
        .await
        .map_err(|err| ApiError::Internal(format!("写入 StateChange 失败: {err}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentdash_domain::common::error::DomainError;
    use agentdash_domain::story::StateChange;
    use async_trait::async_trait;
    use std::sync::Mutex;

    struct RecordingStoryRepo {
        append_error: Option<DomainError>,
        recorded: Mutex<Vec<(Uuid, Uuid, ChangeKind)>>,
    }

    #[async_trait]
    impl StateChangeRepository for RecordingStoryRepo {
        async fn get_changes_since(
            &self,
            _since_id: i64,
            _limit: i64,
        ) -> Result<Vec<StateChange>, DomainError> {
            unreachable!("测试未使用");
        }

        async fn get_changes_since_by_project(
            &self,
            _project_id: Uuid,
            _since_id: i64,
            _limit: i64,
        ) -> Result<Vec<StateChange>, DomainError> {
            unreachable!("测试未使用");
        }

        async fn latest_event_id(&self) -> Result<i64, DomainError> {
            unreachable!("测试未使用");
        }

        async fn latest_event_id_by_project(&self, _project_id: Uuid) -> Result<i64, DomainError> {
            unreachable!("测试未使用");
        }

        async fn append_change(
            &self,
            project_id: Uuid,
            entity_id: Uuid,
            kind: ChangeKind,
            _payload: serde_json::Value,
            _backend_id: Option<&str>,
        ) -> Result<(), DomainError> {
            if let Some(err) = &self.append_error {
                return Err(match err {
                    DomainError::NotFound { entity, id } => DomainError::NotFound {
                        entity,
                        id: id.clone(),
                    },
                    DomainError::InvalidTransition { from, to } => DomainError::InvalidTransition {
                        from: from.clone(),
                        to: to.clone(),
                    },
                    DomainError::Serialization(err) => DomainError::InvalidConfig(err.to_string()),
                    DomainError::InvalidConfig(message) => {
                        DomainError::InvalidConfig(message.clone())
                    }
                });
            }
            self.recorded
                .lock()
                .expect("lock recorded")
                .push((project_id, entity_id, kind));
            Ok(())
        }
    }

    #[test]
    fn classify_task_change_kind_returns_status_changed_when_status_differs() {
        let kind = classify_task_change_kind(&TaskStatus::Pending, &TaskStatus::Running);
        assert!(matches!(kind, ChangeKind::TaskStatusChanged));
    }

    #[test]
    fn classify_task_change_kind_returns_updated_when_status_is_same() {
        let kind = classify_task_change_kind(&TaskStatus::Running, &TaskStatus::Running);
        assert!(matches!(kind, ChangeKind::TaskUpdated));
    }

    #[tokio::test]
    async fn append_required_story_change_maps_repo_failure_to_internal_error() {
        let repo = RecordingStoryRepo {
            append_error: Some(DomainError::InvalidConfig("db down".to_string())),
            recorded: Mutex::new(Vec::new()),
        };

        let err = append_required_story_change(
            &repo,
            Uuid::new_v4(),
            Uuid::new_v4(),
            ChangeKind::TaskUpdated,
            serde_json::json!({ "ok": true }),
            None,
        )
        .await
        .expect_err("应返回内部错误");

        match err {
            ApiError::Internal(message) => {
                assert!(message.contains("写入 StateChange 失败"));
                assert!(message.contains("db down"));
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }
}
