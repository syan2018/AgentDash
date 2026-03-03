use std::{collections::HashMap, sync::Arc};

use agent_client_protocol::{
    Meta, SessionId, SessionInfoUpdate, SessionNotification, SessionUpdate, ToolCall,
    ToolCallStatus,
};
use agentdash_acp_meta::{
    AgentDashEventV1, AgentDashMetaV1, AgentDashSourceV1, AgentDashTraceV1, merge_agentdash_meta,
    parse_agentdash_meta,
};
use agentdash_domain::{
    project::Project,
    story::{ChangeKind, Story},
    task::{Artifact, ArtifactType, Task, TaskStatus},
    workspace::Workspace,
};
use agentdash_executor::{ConnectorError, PromptSessionRequest, SessionMeta};
use axum::{
    Json,
    extract::{Path, State},
};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use uuid::Uuid;

use crate::{
    app_state::AppState,
    rpc::ApiError,
    task_agent_context::{TaskAgentBuildInput, TaskExecutionPhase, build_task_agent_context},
};

#[derive(Debug, Deserialize, Default)]
pub struct StartTaskRequest {
    #[serde(default)]
    pub override_prompt: Option<String>,
    #[serde(default)]
    pub executor_config: Option<executors::profile::ExecutorConfig>,
}

#[derive(Debug, Serialize)]
pub struct StartTaskResponse {
    pub task_id: Uuid,
    pub session_id: String,
    pub executor_session_id: Option<String>,
    pub turn_id: String,
    pub status: TaskStatus,
    pub context_sources: Vec<String>,
}

#[derive(Debug, Deserialize, Default)]
pub struct ContinueTaskRequest {
    #[serde(default)]
    pub additional_prompt: Option<String>,
    #[serde(default)]
    pub executor_config: Option<executors::profile::ExecutorConfig>,
}

#[derive(Debug, Serialize)]
pub struct ContinueTaskResponse {
    pub task_id: Uuid,
    pub session_id: String,
    pub executor_session_id: Option<String>,
    pub turn_id: String,
    pub status: TaskStatus,
    pub context_sources: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct TaskSessionResponse {
    pub task_id: Uuid,
    pub session_id: Option<String>,
    pub executor_session_id: Option<String>,
    pub task_status: TaskStatus,
    pub agent_binding: agentdash_domain::task::AgentBinding,
    pub session_title: Option<String>,
    pub last_activity: Option<i64>,
}

pub async fn start_task(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<StartTaskRequest>,
) -> Result<Json<StartTaskResponse>, ApiError> {
    let task_id = parse_task_id(&id)?;
    let mut task = get_task(&state, task_id).await?;

    if task.session_id.is_some() {
        return Err(ApiError::Conflict(
            "Task 已绑定 Session，请使用 continue 接口继续执行".into(),
        ));
    }
    if task.status == TaskStatus::Running {
        return Err(ApiError::Conflict("该任务已有执行进行中".into()));
    }

    let (story, project, workspace) = load_related_context(&state, &task).await?;
    let built = build_task_agent_context(TaskAgentBuildInput {
        task: &task,
        story: &story,
        project: &project,
        workspace: workspace.as_ref(),
        phase: TaskExecutionPhase::Start,
        override_prompt: req.override_prompt.as_deref(),
        additional_prompt: None,
    })
    .map_err(ApiError::UnprocessableEntity)?;

    let session = create_task_session(&state, &task).await?;
    let previous_status = task.status.clone();

    task.session_id = Some(session.id.clone());
    task.executor_session_id = None;
    task.status = TaskStatus::Running;
    state.task_repo.update(&task).await?;

    append_task_change(
        &state,
        task.id,
        &story.backend_id,
        ChangeKind::TaskUpdated,
        json!({
            "reason": "task_session_bound",
            "task_id": task.id,
            "story_id": task.story_id,
            "session_id": task.session_id,
            "executor_session_id": task.executor_session_id,
        }),
    )
    .await?;

    if previous_status != task.status {
        append_task_change(
            &state,
            task.id,
            &story.backend_id,
            ChangeKind::TaskStatusChanged,
            json!({
                "reason": "task_start_accepted",
                "task_id": task.id,
                "story_id": task.story_id,
                "session_id": task.session_id,
                "executor_session_id": task.executor_session_id,
                "from": previous_status.clone(),
                "to": task.status.clone(),
            }),
        )
        .await?;
    }

    let prompt_req = PromptSessionRequest {
        prompt: None,
        prompt_blocks: Some(built.prompt_blocks),
        working_dir: built.working_dir,
        env: HashMap::new(),
        executor_config: resolve_task_executor_config(req.executor_config, &task, &project)?,
    };

    let turn_id = match state
        .executor_hub
        .start_prompt(&session.id, prompt_req)
        .await
    {
        Ok(turn_id) => turn_id,
        Err(e) => {
            let mut fail_task = task.clone();
            fail_task.status = TaskStatus::Failed;
            let _ = state.task_repo.update(&fail_task).await;
            let _ = append_task_change(
                &state,
                fail_task.id,
                &story.backend_id,
                ChangeKind::TaskStatusChanged,
                json!({
                    "reason": "task_start_failed",
                    "task_id": fail_task.id,
                    "story_id": fail_task.story_id,
                    "session_id": fail_task.session_id,
                    "executor_session_id": fail_task.executor_session_id,
                    "from": TaskStatus::Running,
                    "to": TaskStatus::Failed,
                    "error": e.to_string(),
                }),
            )
            .await;
            return Err(map_connector_error(e));
        }
    };

    bridge_task_status_event_to_session(
        &state,
        &session.id,
        &turn_id,
        "task_start_accepted",
        "Task 已开始执行",
        json!({
            "task_id": task.id,
            "story_id": task.story_id,
            "session_id": session.id,
            "executor_session_id": task.executor_session_id,
            "from": previous_status,
            "to": task.status,
        }),
    )
    .await;

    spawn_task_turn_monitor(
        state.clone(),
        task.id,
        session.id.clone(),
        turn_id.clone(),
        story.backend_id.clone(),
    );

    Ok(Json(StartTaskResponse {
        task_id: task.id,
        session_id: session.id,
        executor_session_id: task.executor_session_id.clone(),
        turn_id,
        status: task.status,
        context_sources: built.source_summary,
    }))
}

pub async fn continue_task(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<ContinueTaskRequest>,
) -> Result<Json<ContinueTaskResponse>, ApiError> {
    let task_id = parse_task_id(&id)?;
    let mut task = get_task(&state, task_id).await?;

    let session_id = task
        .session_id
        .clone()
        .ok_or_else(|| ApiError::UnprocessableEntity("Task 尚未启动，请先执行 start".into()))?;

    if task.status == TaskStatus::Running {
        return Err(ApiError::Conflict("该任务已有执行进行中".into()));
    }

    let (story, project, workspace) = load_related_context(&state, &task).await?;
    let built = build_task_agent_context(TaskAgentBuildInput {
        task: &task,
        story: &story,
        project: &project,
        workspace: workspace.as_ref(),
        phase: TaskExecutionPhase::Continue,
        override_prompt: None,
        additional_prompt: req.additional_prompt.as_deref(),
    })
    .map_err(ApiError::UnprocessableEntity)?;

    let prompt_req = PromptSessionRequest {
        prompt: None,
        prompt_blocks: Some(built.prompt_blocks),
        working_dir: built.working_dir,
        env: HashMap::new(),
        executor_config: resolve_task_executor_config(req.executor_config, &task, &project)?,
    };

    let turn_id = state
        .executor_hub
        .start_prompt(&session_id, prompt_req)
        .await
        .map_err(map_connector_error)?;

    let previous_status = task.status.clone();
    task.status = TaskStatus::Running;
    state.task_repo.update(&task).await?;

    if previous_status != task.status {
        append_task_change(
            &state,
            task.id,
            &story.backend_id,
            ChangeKind::TaskStatusChanged,
            json!({
                "reason": "task_continue_accepted",
                "task_id": task.id,
                "story_id": task.story_id,
                "session_id": task.session_id,
                "executor_session_id": task.executor_session_id,
                "from": previous_status.clone(),
                "to": task.status.clone(),
            }),
        )
        .await?;
    }

    bridge_task_status_event_to_session(
        &state,
        &session_id,
        &turn_id,
        "task_continue_accepted",
        "Task 已继续执行",
        json!({
            "task_id": task.id,
            "story_id": task.story_id,
            "session_id": session_id,
            "executor_session_id": task.executor_session_id,
            "from": previous_status,
            "to": task.status,
        }),
    )
    .await;

    spawn_task_turn_monitor(
        state.clone(),
        task.id,
        session_id.clone(),
        turn_id.clone(),
        story.backend_id.clone(),
    );

    Ok(Json(ContinueTaskResponse {
        task_id: task.id,
        session_id,
        executor_session_id: task.executor_session_id.clone(),
        turn_id,
        status: task.status,
        context_sources: built.source_summary,
    }))
}

pub async fn cancel_task(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<Task>, ApiError> {
    let task_id = parse_task_id(&id)?;
    let mut task = get_task(&state, task_id).await?;

    let session_id = task
        .session_id
        .clone()
        .ok_or_else(|| ApiError::UnprocessableEntity("Task 尚未启动，无法取消执行".into()))?;

    state
        .executor_hub
        .cancel(&session_id)
        .await
        .map_err(map_connector_error)?;

    if task.status == TaskStatus::Running {
        let previous_status = task.status.clone();
        task.status = TaskStatus::Failed;
        state.task_repo.update(&task).await?;

        let backend_id = state
            .story_repo
            .get_by_id(task.story_id)
            .await?
            .map(|story| story.backend_id)
            .unwrap_or_else(|| "unknown".to_string());

        append_task_change(
            &state,
            task.id,
            &backend_id,
            ChangeKind::TaskStatusChanged,
            json!({
                "reason": "task_cancel_requested",
                "task_id": task.id,
                "story_id": task.story_id,
                "session_id": task.session_id,
                "executor_session_id": task.executor_session_id,
                "from": previous_status,
                "to": task.status,
            }),
        )
        .await?;
    }

    Ok(Json(task))
}

pub async fn get_task_session(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<TaskSessionResponse>, ApiError> {
    let task_id = parse_task_id(&id)?;
    let task = get_task(&state, task_id).await?;
    let session_id = task.session_id.clone();

    let (session_title, last_activity) = if let Some(session_id) = session_id.as_deref() {
        match state.executor_hub.get_session_meta(session_id).await {
            Ok(Some(meta)) => (Some(meta.title), Some(meta.updated_at)),
            Ok(None) => (None, None),
            Err(e) => return Err(ApiError::Internal(e.to_string())),
        }
    } else {
        (None, None)
    };

    Ok(Json(TaskSessionResponse {
        task_id: task.id,
        session_id,
        executor_session_id: task.executor_session_id,
        task_status: task.status,
        agent_binding: task.agent_binding,
        session_title,
        last_activity,
    }))
}

fn spawn_task_turn_monitor(
    state: Arc<AppState>,
    task_id: Uuid,
    session_id: String,
    turn_id: String,
    backend_id: String,
) {
    tokio::spawn(async move {
        let (history, mut rx) = state.executor_hub.subscribe_with_history(&session_id).await;

        for notification in history {
            match handle_turn_notification(
                &state,
                task_id,
                &session_id,
                &turn_id,
                &backend_id,
                &notification,
            )
            .await
            {
                Ok(true) => return,
                Ok(false) => {}
                Err(err) => {
                    tracing::error!(
                        task_id = %task_id,
                        session_id = %session_id,
                        turn_id = %turn_id,
                        "处理历史会话事件失败: {}",
                        err
                    );
                }
            }
        }

        loop {
            match rx.recv().await {
                Ok(notification) => match handle_turn_notification(
                    &state,
                    task_id,
                    &session_id,
                    &turn_id,
                    &backend_id,
                    &notification,
                )
                .await
                {
                    Ok(true) => return,
                    Ok(false) => {}
                    Err(err) => {
                        tracing::error!(
                            task_id = %task_id,
                            session_id = %session_id,
                            turn_id = %turn_id,
                            "处理实时会话事件失败: {}",
                            err
                        );
                    }
                },
                Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                    tracing::warn!(
                        task_id = %task_id,
                        session_id = %session_id,
                        turn_id = %turn_id,
                        skipped = skipped,
                        "Task turn 监听落后，部分消息被跳过"
                    );
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                    tracing::warn!(
                        task_id = %task_id,
                        session_id = %session_id,
                        turn_id = %turn_id,
                        "Task turn 监听通道关闭，未收到终态事件"
                    );
                    let _ = update_task_status(
                        &state,
                        task_id,
                        &backend_id,
                        TaskStatus::Failed,
                        "turn_monitor_closed",
                        json!({
                            "session_id": session_id,
                            "turn_id": turn_id,
                        }),
                    )
                    .await;
                    return;
                }
            }
        }
    });
}

async fn handle_turn_notification(
    state: &Arc<AppState>,
    task_id: Uuid,
    session_id: &str,
    turn_id: &str,
    backend_id: &str,
    notification: &SessionNotification,
) -> Result<bool, agentdash_domain::DomainError> {
    match &notification.update {
        SessionUpdate::ToolCall(tool_call) => {
            if turn_matches(tool_call.meta.as_ref(), turn_id) {
                persist_tool_call_artifact(
                    state,
                    task_id,
                    session_id,
                    turn_id,
                    &tool_call.tool_call_id.to_string(),
                    build_tool_call_patch(tool_call),
                    backend_id,
                    "tool_call",
                )
                .await?;
            }
        }
        SessionUpdate::ToolCallUpdate(update) => {
            if turn_matches(update.meta.as_ref(), turn_id) {
                persist_tool_call_artifact(
                    state,
                    task_id,
                    session_id,
                    turn_id,
                    &update.tool_call_id.to_string(),
                    build_tool_call_update_patch(update),
                    backend_id,
                    "tool_call_update",
                )
                .await?;
            }
        }
        SessionUpdate::SessionInfoUpdate(info) => {
            sync_task_executor_session_binding_from_hub(
                state, task_id, backend_id, session_id, turn_id,
            )
            .await?;

            if let Some((event_type, message)) = parse_turn_event(info.meta.as_ref(), turn_id) {
                match event_type.as_str() {
                    "turn_completed" => {
                        let _ = update_task_status(
                            state,
                            task_id,
                            backend_id,
                            TaskStatus::AwaitingVerification,
                            "turn_completed",
                            json!({
                                "session_id": session_id,
                                "turn_id": turn_id,
                            }),
                        )
                        .await?;
                        return Ok(true);
                    }
                    "turn_failed" => {
                        if let Some(err_msg) = message.as_deref() {
                            persist_turn_failure_artifact(
                                state, task_id, backend_id, session_id, turn_id, err_msg,
                            )
                            .await?;
                        }
                        let _ = update_task_status(
                            state,
                            task_id,
                            backend_id,
                            TaskStatus::Failed,
                            "turn_failed",
                            json!({
                                "session_id": session_id,
                                "turn_id": turn_id,
                                "error": message.clone(),
                            }),
                        )
                        .await?;
                        return Ok(true);
                    }
                    _ => {}
                }
            }
        }
        _ => {}
    }

    Ok(false)
}

async fn persist_tool_call_artifact(
    state: &Arc<AppState>,
    task_id: Uuid,
    session_id: &str,
    turn_id: &str,
    tool_call_id: &str,
    patch: Map<String, Value>,
    backend_id: &str,
    reason: &str,
) -> Result<(), agentdash_domain::DomainError> {
    let mut task = match state.task_repo.get_by_id(task_id).await? {
        Some(task) => task,
        None => return Ok(()),
    };

    let changed =
        upsert_tool_execution_artifact(&mut task, session_id, turn_id, tool_call_id, patch);
    if !changed {
        return Ok(());
    }

    state.task_repo.update(&task).await?;
    append_task_change(
        state,
        task.id,
        backend_id,
        ChangeKind::TaskArtifactAdded,
        json!({
            "reason": reason,
            "task_id": task.id,
            "story_id": task.story_id,
            "session_id": session_id,
            "turn_id": turn_id,
            "tool_call_id": tool_call_id,
            "artifact_type": "tool_execution",
        }),
    )
    .await?;

    Ok(())
}

fn upsert_tool_execution_artifact(
    task: &mut Task,
    session_id: &str,
    turn_id: &str,
    tool_call_id: &str,
    mut patch: Map<String, Value>,
) -> bool {
    let now = chrono::Utc::now();
    let now_str = now.to_rfc3339();

    patch.insert("session_id".to_string(), json!(session_id));
    patch.insert("turn_id".to_string(), json!(turn_id));
    patch.insert("tool_call_id".to_string(), json!(tool_call_id));
    patch.insert("updated_at".to_string(), json!(now_str));

    if let Some(index) = task
        .artifacts
        .iter()
        .position(|item| is_same_tool_execution_artifact(item, turn_id, tool_call_id))
    {
        let artifact = &mut task.artifacts[index];
        let before = artifact.content.clone();
        let mut content = artifact.content.as_object().cloned().unwrap_or_default();
        for (key, value) in patch {
            if key == "started_at" && content.contains_key("started_at") {
                continue;
            }
            content.insert(key, value);
        }
        if !content.contains_key("started_at") {
            content.insert("started_at".to_string(), json!(now_str));
        }
        let next = Value::Object(content);
        if before == next {
            return false;
        }
        artifact.content = next;
        return true;
    }

    if !patch.contains_key("started_at") {
        patch.insert("started_at".to_string(), json!(now_str));
    }

    task.artifacts.push(Artifact {
        id: Uuid::new_v4(),
        artifact_type: ArtifactType::ToolExecution,
        content: Value::Object(patch),
        created_at: now,
    });
    true
}

fn is_same_tool_execution_artifact(artifact: &Artifact, turn_id: &str, tool_call_id: &str) -> bool {
    artifact.artifact_type == ArtifactType::ToolExecution
        && artifact.content.get("turn_id").and_then(Value::as_str) == Some(turn_id)
        && artifact.content.get("tool_call_id").and_then(Value::as_str) == Some(tool_call_id)
}

async fn persist_turn_failure_artifact(
    state: &Arc<AppState>,
    task_id: Uuid,
    backend_id: &str,
    session_id: &str,
    turn_id: &str,
    error_message: &str,
) -> Result<(), agentdash_domain::DomainError> {
    let mut task = match state.task_repo.get_by_id(task_id).await? {
        Some(task) => task,
        None => return Ok(()),
    };

    task.artifacts.push(Artifact {
        id: Uuid::new_v4(),
        artifact_type: ArtifactType::LogOutput,
        content: json!({
            "kind": "turn_error",
            "session_id": session_id,
            "turn_id": turn_id,
            "message": error_message,
            "created_at": chrono::Utc::now().to_rfc3339(),
        }),
        created_at: chrono::Utc::now(),
    });

    state.task_repo.update(&task).await?;
    append_task_change(
        state,
        task.id,
        backend_id,
        ChangeKind::TaskArtifactAdded,
        json!({
            "reason": "turn_failed_error_summary",
            "task_id": task.id,
            "story_id": task.story_id,
            "session_id": session_id,
            "turn_id": turn_id,
            "artifact_type": "log_output",
        }),
    )
    .await?;

    Ok(())
}

async fn bind_executor_session_id(
    state: &Arc<AppState>,
    task_id: Uuid,
    backend_id: &str,
    session_id: &str,
    turn_id: &str,
    executor_session_id: &str,
) -> Result<(), agentdash_domain::DomainError> {
    let Some(mut task) = state.task_repo.get_by_id(task_id).await? else {
        return Ok(());
    };

    if task.executor_session_id.as_deref() == Some(executor_session_id) {
        return Ok(());
    }

    task.executor_session_id = Some(executor_session_id.to_string());
    state.task_repo.update(&task).await?;

    append_task_change(
        state,
        task.id,
        backend_id,
        ChangeKind::TaskUpdated,
        json!({
            "reason": "executor_session_bound",
            "task_id": task.id,
            "story_id": task.story_id,
            "session_id": session_id,
            "turn_id": turn_id,
            "executor_session_id": executor_session_id,
        }),
    )
    .await?;

    Ok(())
}

async fn sync_task_executor_session_binding_from_hub(
    state: &Arc<AppState>,
    task_id: Uuid,
    backend_id: &str,
    session_id: &str,
    turn_id: &str,
) -> Result<(), agentdash_domain::DomainError> {
    let meta = match state.executor_hub.get_session_meta(session_id).await {
        Ok(Some(meta)) => meta,
        Ok(None) => return Ok(()),
        Err(err) => {
            return Err(agentdash_domain::DomainError::InvalidConfig(
                err.to_string(),
            ));
        }
    };

    let Some(executor_session_id) = meta
        .executor_session_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Ok(());
    };

    bind_executor_session_id(
        state,
        task_id,
        backend_id,
        session_id,
        turn_id,
        executor_session_id,
    )
    .await
}

async fn update_task_status(
    state: &Arc<AppState>,
    task_id: Uuid,
    backend_id: &str,
    next_status: TaskStatus,
    reason: &str,
    context: Value,
) -> Result<bool, agentdash_domain::DomainError> {
    let mut task = match state.task_repo.get_by_id(task_id).await? {
        Some(task) => task,
        None => return Ok(false),
    };

    if task.status == next_status {
        return Ok(false);
    }

    let previous_status = task.status.clone();
    task.status = next_status.clone();
    state.task_repo.update(&task).await?;

    append_task_change(
        state,
        task.id,
        backend_id,
        ChangeKind::TaskStatusChanged,
        json!({
            "reason": reason,
            "task_id": task.id,
            "story_id": task.story_id,
            "session_id": task.session_id,
            "executor_session_id": task.executor_session_id,
            "from": previous_status,
            "to": next_status,
            "context": context,
        }),
    )
    .await?;

    Ok(true)
}

async fn append_task_change(
    state: &Arc<AppState>,
    task_id: Uuid,
    backend_id: &str,
    kind: ChangeKind,
    payload: Value,
) -> Result<(), agentdash_domain::DomainError> {
    state
        .story_repo
        .append_change(task_id, kind, payload, backend_id)
        .await
}

/// 将 Task 侧生命周期事件桥接到对应 ACP session 流，便于后续 inbox 等场景复用。
/// 当前常规会话 UI 保持静默渲染（不直接展示这类系统事件卡片）。
async fn bridge_task_status_event_to_session(
    state: &Arc<AppState>,
    session_id: &str,
    turn_id: &str,
    event_type: &str,
    message: &str,
    data: Value,
) {
    let mut trace = AgentDashTraceV1::new();
    trace.turn_id = Some(turn_id.to_string());

    let mut event = AgentDashEventV1::new(event_type);
    event.severity = Some("info".to_string());
    event.message = Some(message.to_string());
    event.data = Some(data);

    let source = AgentDashSourceV1::new("agentdash-task-execution", "api_task_route");
    let agentdash = AgentDashMetaV1::new()
        .source(Some(source))
        .trace(Some(trace))
        .event(Some(event));
    let meta = merge_agentdash_meta(None, &agentdash);

    let notification = SessionNotification::new(
        SessionId::new(session_id.to_string()),
        SessionUpdate::SessionInfoUpdate(SessionInfoUpdate::new().meta(meta)),
    );

    if let Err(err) = state
        .executor_hub
        .inject_notification(session_id, notification)
        .await
    {
        tracing::warn!(
            session_id = %session_id,
            turn_id = %turn_id,
            event_type = %event_type,
            error = %err,
            "桥接 Task 生命周期事件到 session 流失败"
        );
    }
}

fn turn_matches(meta: Option<&Meta>, expected_turn_id: &str) -> bool {
    let Some(meta) = meta else {
        return false;
    };
    parse_agentdash_meta(meta)
        .and_then(|m| m.trace.and_then(|trace| trace.turn_id))
        .as_deref()
        == Some(expected_turn_id)
}

fn parse_turn_event(
    meta: Option<&Meta>,
    expected_turn_id: &str,
) -> Option<(String, Option<String>)> {
    let parsed = parse_agentdash_meta(meta?)?;
    let trace = parsed.trace?;
    let turn_id = trace.turn_id?;
    if turn_id != expected_turn_id {
        return None;
    }
    let event = parsed.event?;
    Some((event.r#type, event.message))
}

fn build_tool_call_patch(tool_call: &ToolCall) -> Map<String, Value> {
    let mut patch = Map::new();
    patch.insert("title".to_string(), json!(tool_call.title));
    patch.insert("kind".to_string(), json!(enum_to_string(&tool_call.kind)));
    patch.insert(
        "status".to_string(),
        json!(tool_status_to_string(tool_call.status)),
    );

    if !tool_call.content.is_empty() {
        let content = serde_json::to_value(&tool_call.content).unwrap_or_else(|_| json!([]));
        patch.insert("content".to_string(), content.clone());
        patch.insert("output_preview".to_string(), json!(preview_value(&content)));
    }
    if !tool_call.locations.is_empty() {
        patch.insert(
            "locations".to_string(),
            serde_json::to_value(&tool_call.locations).unwrap_or_else(|_| json!([])),
        );
    }
    if let Some(raw_input) = tool_call.raw_input.clone() {
        patch.insert("raw_input".to_string(), raw_input.clone());
        patch.insert(
            "input_preview".to_string(),
            json!(preview_value(&raw_input)),
        );
    }
    if let Some(raw_output) = tool_call.raw_output.clone() {
        patch.insert("raw_output".to_string(), raw_output.clone());
        patch.insert(
            "output_preview".to_string(),
            json!(preview_value(&raw_output)),
        );
    }

    patch
}

fn build_tool_call_update_patch(
    update: &agent_client_protocol::ToolCallUpdate,
) -> Map<String, Value> {
    let mut patch = Map::new();
    if let Some(title) = update.fields.title.clone() {
        patch.insert("title".to_string(), json!(title));
    }
    if let Some(kind) = update.fields.kind {
        patch.insert("kind".to_string(), json!(enum_to_string(&kind)));
    }
    if let Some(status) = update.fields.status {
        patch.insert("status".to_string(), json!(tool_status_to_string(status)));
    }
    if let Some(content) = update.fields.content.clone() {
        let content_value = serde_json::to_value(content).unwrap_or_else(|_| json!([]));
        patch.insert("content".to_string(), content_value.clone());
        patch.insert(
            "output_preview".to_string(),
            json!(preview_value(&content_value)),
        );
    }
    if let Some(locations) = update.fields.locations.clone() {
        patch.insert(
            "locations".to_string(),
            serde_json::to_value(locations).unwrap_or_else(|_| json!([])),
        );
    }
    if let Some(raw_input) = update.fields.raw_input.clone() {
        patch.insert("raw_input".to_string(), raw_input.clone());
        patch.insert(
            "input_preview".to_string(),
            json!(preview_value(&raw_input)),
        );
    }
    if let Some(raw_output) = update.fields.raw_output.clone() {
        patch.insert("raw_output".to_string(), raw_output.clone());
        patch.insert(
            "output_preview".to_string(),
            json!(preview_value(&raw_output)),
        );
    }
    patch
}

fn preview_value(value: &Value) -> String {
    let raw = value.to_string();
    const MAX_LEN: usize = 240;
    if raw.len() <= MAX_LEN {
        raw
    } else {
        let shortened: String = raw.chars().take(MAX_LEN).collect();
        format!("{shortened}...")
    }
}

fn tool_status_to_string(status: ToolCallStatus) -> &'static str {
    match status {
        ToolCallStatus::Pending => "pending",
        ToolCallStatus::InProgress => "in_progress",
        ToolCallStatus::Completed => "completed",
        ToolCallStatus::Failed => "failed",
        _ => "pending",
    }
}

fn enum_to_string<T: Serialize>(value: &T) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|raw| raw.as_str().map(ToOwned::to_owned))
        .unwrap_or_else(|| "other".to_string())
}

fn parse_task_id(id: &str) -> Result<Uuid, ApiError> {
    Uuid::parse_str(id).map_err(|_| ApiError::BadRequest("无效的 Task ID".into()))
}

async fn get_task(state: &Arc<AppState>, task_id: Uuid) -> Result<Task, ApiError> {
    state
        .task_repo
        .get_by_id(task_id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("Task {task_id} 不存在")))
}

async fn load_related_context(
    state: &Arc<AppState>,
    task: &Task,
) -> Result<(Story, Project, Option<Workspace>), ApiError> {
    let story = state
        .story_repo
        .get_by_id(task.story_id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("Task 所属 Story {} 不存在", task.story_id)))?;

    let project = state
        .project_repo
        .get_by_id(story.project_id)
        .await?
        .ok_or_else(|| {
            ApiError::NotFound(format!("Story 所属 Project {} 不存在", story.project_id))
        })?;

    let workspace = if let Some(ws_id) = task.workspace_id {
        Some(
            state
                .workspace_repo
                .get_by_id(ws_id)
                .await?
                .ok_or_else(|| ApiError::NotFound(format!("Task 关联 Workspace {ws_id} 不存在")))?,
        )
    } else {
        None
    };

    Ok((story, project, workspace))
}

async fn create_task_session(state: &Arc<AppState>, task: &Task) -> Result<SessionMeta, ApiError> {
    let title = format!("Task: {}", task.title.trim());
    state
        .executor_hub
        .create_session(title.trim())
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))
}

fn map_connector_error(err: ConnectorError) -> ApiError {
    match err {
        ConnectorError::InvalidConfig(message) => ApiError::BadRequest(message),
        ConnectorError::Runtime(message) => ApiError::Conflict(message),
        other => ApiError::Internal(other.to_string()),
    }
}

fn resolve_task_executor_config(
    explicit: Option<executors::profile::ExecutorConfig>,
    task: &Task,
    project: &Project,
) -> Result<Option<executors::profile::ExecutorConfig>, ApiError> {
    if explicit.is_some() {
        return Ok(explicit);
    }

    let Some(agent_type) = resolve_task_agent_type(task, project)? else {
        return Ok(None);
    };

    let executor = parse_base_agent(&agent_type)?;
    Ok(Some(executors::profile::ExecutorConfig::new(executor)))
}

fn resolve_task_agent_type(task: &Task, project: &Project) -> Result<Option<String>, ApiError> {
    if let Some(agent_type) = normalize_option_string(task.agent_binding.agent_type.clone()) {
        return Ok(Some(agent_type));
    }

    if let Some(preset_name) = normalize_option_string(task.agent_binding.preset_name.clone()) {
        let preset = project
            .config
            .agent_presets
            .iter()
            .find(|item| item.name == preset_name)
            .ok_or_else(|| ApiError::BadRequest(format!("Project 中不存在预设: {preset_name}")))?;
        return Ok(normalize_option_string(Some(preset.agent_type.clone())));
    }

    Ok(normalize_option_string(
        project.config.default_agent_type.clone(),
    ))
}

fn parse_base_agent(raw: &str) -> Result<executors::executors::BaseCodingAgent, ApiError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(ApiError::BadRequest("Agent 类型不能为空".into()));
    }

    if let Ok(executor) = trimmed.parse::<executors::executors::BaseCodingAgent>() {
        return Ok(executor);
    }

    let normalized = trimmed.replace('-', "_").to_ascii_uppercase();
    normalized
        .parse::<executors::executors::BaseCodingAgent>()
        .map_err(|_| ApiError::BadRequest(format!("无效的 Agent 类型: {trimmed}")))
}

fn normalize_option_string(value: Option<String>) -> Option<String> {
    value.and_then(|raw| {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}
