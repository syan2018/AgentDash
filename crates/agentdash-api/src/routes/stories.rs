use std::sync::Arc;

use agentdash_application::story::{
    AgentBindingInput, CreateStoryInput, StoryLifecycleLaunchCommand, StoryLifecycleLaunchResult,
    StoryLifecycleLaunchService, StoryMutationInput, TaskMutationInput, apply_task_mutation,
    build_agent_binding, build_task, create_story_record, delete_story_record,
    list_project_stories, update_story_record,
};
use axum::Json;
use axum::extract::{Path, Query, State};
use uuid::Uuid;

use agentdash_contracts::core::DeletedIdResponse;
use agentdash_contracts::workflow::{
    AgentFrameRefDto, LifecycleAgentRefDto, LifecycleRunRefDto, RuntimeSessionRefDto,
    StoryLaunchResult, SubjectRefDto,
};
use agentdash_domain::story::ChangeKind;

use crate::app_state::AppState;
use crate::auth::{
    CurrentUser, ProjectPermission, load_project_with_permission,
    load_story_and_project_with_permission, load_task_story_project_with_permission,
};
use crate::dto::{
    CreateStoryRequest, CreateTaskRequest, ListStoriesQuery, StoryResponse, TaskResponse,
    UpdateStoryRequest, UpdateTaskRequest,
};
use crate::rpc::ApiError;
use agentdash_domain::story::StateChangeRepository;

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
        .route("/stories/{id}/launch", axum::routing::post(launch_story))
        .route(
            "/stories/{id}/tasks",
            axum::routing::get(list_tasks).post(create_task),
        )
        .route(
            "/tasks/{id}",
            axum::routing::get(get_task)
                .put(update_task)
                .delete(delete_task),
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
        ProjectPermission::Edit,
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

    let (story, project) = load_story_and_project_with_permission(
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
        ProjectPermission::Edit,
    )
    .await?;

    delete_story_record(&state.repos, &story).await?;

    Ok(Json(DeletedIdResponse { deleted: id }))
}

pub async fn launch_story(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(id): Path<String>,
) -> Result<Json<StoryLaunchResult>, ApiError> {
    let story_id =
        Uuid::parse_str(&id).map_err(|_| ApiError::BadRequest("无效的 Story ID".into()))?;
    load_story_and_project_with_permission(
        state.as_ref(),
        &current_user,
        story_id,
        ProjectPermission::Edit,
    )
    .await?;

    let service = StoryLifecycleLaunchService {
        repos: state.repos.clone(),
    };
    let result = service
        .launch_story(StoryLifecycleLaunchCommand { story_id })
        .await?;

    Ok(Json(story_launch_result_to_dto(result)))
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
    // M1-b：Story aggregate 已持有 tasks
    let story = state
        .repos
        .story_repo
        .get_by_id(story_id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("Story {story_id} 不存在")))?;
    Ok(Json(
        story.tasks.into_iter().map(TaskResponse::from).collect(),
    ))
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

    // M1-b：task create 走 Story aggregate 命令路径；Postgres 实现在同一事务内
    // 更新 stories.tasks 并追加 TaskCreated / StoryUpdated。
    state
        .repos
        .story_repo
        .add_task_to_story(story_id, &task)
        .await?;

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
            agent_binding,
        },
    );

    let mut story_aggregate = state
        .repos
        .story_repo
        .get_by_id(task.story_id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("Task 所属 Story {} 不存在", task.story_id)))?;
    let updated_spec = story_aggregate.update_task(task.id, |view| {
        *view.title = task.title.clone();
        *view.description = task.description.clone();
        *view.workspace_id = task.workspace_id;
        *view.agent_binding = task.agent_binding.clone();
    });
    if updated_spec.is_none() {
        return Err(ApiError::NotFound(format!(
            "Task {} 不属于 Story {}",
            task.id, task.story_id
        )));
    }
    state.repos.story_repo.update(&story_aggregate).await?;

    let payload = serde_json::to_value(&task)
        .map_err(|err| ApiError::Internal(format!("序列化 Task 更新失败: {err}")))?;
    append_required_story_change(
        state.repos.state_change_repo.as_ref(),
        task.project_id,
        task.id,
        ChangeKind::TaskUpdated,
        payload,
        None,
    )
    .await?;

    Ok(Json(TaskResponse::from(task)))
}

pub async fn delete_task(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(id): Path<String>,
) -> Result<Json<DeletedIdResponse>, ApiError> {
    let task_id =
        Uuid::parse_str(&id).map_err(|_| ApiError::BadRequest("无效的 Task ID".into()))?;
    load_task_story_project_with_permission(
        state.as_ref(),
        &current_user,
        task_id,
        ProjectPermission::Edit,
    )
    .await?;

    // M1-b：task delete 走 Story aggregate 命令路径；Postgres 实现在同一事务内
    // 更新 stories.tasks 并追加 TaskDeleted / StoryUpdated。
    state
        .repos
        .story_repo
        .remove_task_from_story(task_id)
        .await?;

    Ok(Json(DeletedIdResponse { deleted: id }))
}

fn story_launch_result_to_dto(result: StoryLifecycleLaunchResult) -> StoryLaunchResult {
    StoryLaunchResult {
        created: true,
        story_id: result.story_id.to_string(),
        project_agent_id: result.project_agent_id.to_string(),
        run_ref: LifecycleRunRefDto {
            run_id: result.run_ref.to_string(),
        },
        agent_ref: LifecycleAgentRefDto {
            run_id: result.run_ref.to_string(),
            agent_id: result.agent_ref.to_string(),
        },
        frame_ref: AgentFrameRefDto {
            agent_id: result.agent_ref.to_string(),
            frame_id: result.frame_ref.to_string(),
            revision: None,
        },
        runtime_session_ref: result.runtime_session_ref.map(|runtime_session_id| {
            RuntimeSessionRefDto {
                runtime_session_id: runtime_session_id.to_string(),
            }
        }),
        trace_ref: result
            .trace_ref
            .map(|runtime_session_id| RuntimeSessionRefDto {
                runtime_session_id: runtime_session_id.to_string(),
            }),
        subject_ref: SubjectRefDto {
            kind: result.subject_ref.kind,
            id: result.subject_ref.id.to_string(),
        },
    }
}

fn normalize_optional_string(value: Option<String>) -> Option<String> {
    value.and_then(|raw| {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
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
        .map_err(|err| {
            tracing::error!(error = %err, "failed to append required story state change");
            ApiError::Internal(String::from("写入 StateChange 失败"))
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentdash_domain::common::error::DomainError;
    use agentdash_domain::story::StateChange;
    use async_trait::async_trait;
    use std::sync::Mutex;

    struct RecordingStoryRepo {
        append_error: Mutex<Option<DomainError>>,
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
            if let Some(err) = self.append_error.lock().expect("lock append_error").take() {
                return Err(err);
            }
            self.recorded
                .lock()
                .expect("lock recorded")
                .push((project_id, entity_id, kind));
            Ok(())
        }
    }

    #[tokio::test]
    async fn append_required_story_change_maps_repo_failure_to_internal_error() {
        let repo = RecordingStoryRepo {
            append_error: Mutex::new(Some(DomainError::Database {
                operation: "append_state_change",
                message: "db down".to_string(),
            })),
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
                assert!(!message.contains("db down"));
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }
}
