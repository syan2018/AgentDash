use std::sync::Arc;
use std::time::Duration;

use axum::{
    Json,
    body::{Body, Bytes},
    extract::{Path, Query, State},
    http::HeaderMap,
    response::IntoResponse,
    response::sse::{Event, KeepAlive, Sse},
};
use futures::stream::Stream;
use serde::Deserialize;
use std::convert::Infallible;
use tokio::time::MissedTickBehavior;

use crate::{app_state::AppState, rpc::ApiError};
use agentdash_domain::{project::Project, session_binding::SessionOwnerType, story::Story, workspace::Workspace};
use agentdash_executor::{PromptSessionRequest, SessionMeta};
use agentdash_injection::{
    ContextComposer, MergeStrategy, ResolveSourcesRequest, resolve_declared_sources,
};
use agentdash_mcp::injection::McpInjectionConfig;
use serde::Serialize;
use serde_json::json;

const ACP_HEARTBEAT_INTERVAL: Duration = Duration::from_secs(20);

#[derive(Debug, Deserialize)]
pub struct NdjsonStreamQuery {
    pub since_id: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub struct ListSessionsQuery {
    pub owner_type: Option<String>,
    pub owner_id: Option<String>,
}

pub async fn list_sessions(
    State(state): State<Arc<AppState>>,
    Query(query): Query<ListSessionsQuery>,
) -> Result<Json<Vec<SessionMeta>>, ApiError> {
    if let (Some(owner_type_str), Some(owner_id_str)) = (&query.owner_type, &query.owner_id) {
        let owner_type = SessionOwnerType::from_str_loose(owner_type_str)
            .ok_or_else(|| ApiError::BadRequest(format!("无效的 owner_type: {owner_type_str}")))?;
        let owner_id: uuid::Uuid = owner_id_str
            .parse()
            .map_err(|_| ApiError::BadRequest(format!("无效的 owner_id: {owner_id_str}")))?;

        let bindings = state
            .session_binding_repo
            .list_by_owner(owner_type, owner_id)
            .await
            .map_err(|e| ApiError::Internal(e.to_string()))?;

        let mut sessions = Vec::with_capacity(bindings.len());
        for binding in &bindings {
            if let Ok(Some(meta)) = state
                .executor_hub
                .get_session_meta(&binding.session_id)
                .await
            {
                sessions.push(meta);
            }
        }
        return Ok(Json(sessions));
    }

    let sessions = state
        .executor_hub
        .list_sessions()
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    Ok(Json(sessions))
}

#[derive(Debug, Deserialize)]
pub struct CreateSessionRequest {
    #[serde(default)]
    pub title: Option<String>,
}

pub async fn create_session(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateSessionRequest>,
) -> Result<Json<SessionMeta>, ApiError> {
    let title = req.title.unwrap_or_else(|| "新会话".to_string());
    let meta = state
        .executor_hub
        .create_session(&title)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    Ok(Json(meta))
}

pub async fn get_session(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
) -> Result<Json<SessionMeta>, ApiError> {
    let meta = state
        .executor_hub
        .get_session_meta(&session_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound(format!("会话 {} 不存在", session_id)))?;
    Ok(Json(meta))
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionBindingOwnerResponse {
    pub id: String,
    pub session_id: String,
    pub owner_type: String,
    pub owner_id: String,
    pub label: String,
    pub created_at: String,
    pub owner_title: Option<String>,
    pub story_id: Option<String>,
    pub task_id: Option<String>,
}

pub async fn get_session_bindings(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
) -> Result<Json<Vec<SessionBindingOwnerResponse>>, ApiError> {
    let bindings = state
        .session_binding_repo
        .list_by_session(&session_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let mut responses = Vec::with_capacity(bindings.len());
    for binding in bindings {
        let mut owner_title = None;
        let mut story_id = None;
        let mut task_id = None;

        match binding.owner_type {
            SessionOwnerType::Story => {
                if let Some(story) = state
                    .story_repo
                    .get_by_id(binding.owner_id)
                    .await
                    .map_err(|e| ApiError::Internal(e.to_string()))?
                {
                    owner_title = Some(story.title);
                    story_id = Some(story.id.to_string());
                }
            }
            SessionOwnerType::Task => {
                if let Some(task) = state
                    .task_repo
                    .get_by_id(binding.owner_id)
                    .await
                    .map_err(|e| ApiError::Internal(e.to_string()))?
                {
                    owner_title = Some(task.title);
                    story_id = Some(task.story_id.to_string());
                    task_id = Some(task.id.to_string());
                }
            }
        }

        responses.push(SessionBindingOwnerResponse {
            id: binding.id.to_string(),
            session_id: binding.session_id,
            owner_type: binding.owner_type.to_string(),
            owner_id: binding.owner_id.to_string(),
            label: binding.label,
            created_at: binding.created_at.to_rfc3339(),
            owner_title,
            story_id,
            task_id,
        });
    }

    Ok(Json(responses))
}

pub async fn delete_session(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    state
        .executor_hub
        .delete_session(&session_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    Ok(Json(
        serde_json::json!({ "deleted": true, "sessionId": session_id }),
    ))
}

pub async fn prompt_session(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
    Json(req): Json<PromptSessionRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let req = augment_prompt_request_for_owner(&state, &session_id, req).await?;
    let turn_id = state
        .executor_hub
        .start_prompt(&session_id, req)
        .await
        .map_err(|e| match &e {
            agentdash_executor::ConnectorError::InvalidConfig(_) => {
                ApiError::BadRequest(e.to_string())
            }
            _ => ApiError::Internal(e.to_string()),
        })?;

    Ok(Json(
        serde_json::json!({ "started": true, "sessionId": session_id, "turnId": turn_id }),
    ))
}

async fn augment_prompt_request_for_owner(
    state: &Arc<AppState>,
    session_id: &str,
    req: PromptSessionRequest,
) -> Result<PromptSessionRequest, ApiError> {
    let bindings = state
        .session_binding_repo
        .list_by_session(session_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let Some(binding) = bindings
        .iter()
        .find(|binding| binding.owner_type == SessionOwnerType::Story)
    else {
        return Ok(req);
    };

    let story = state
        .story_repo
        .get_by_id(binding.owner_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound(format!("Story {} 不存在", binding.owner_id)))?;
    let project = state
        .project_repo
        .get_by_id(story.project_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound(format!("Project {} 不存在", story.project_id)))?;
    let workspace = resolve_story_workspace(state, &project).await?;

    Ok(build_story_owner_prompt_request(state, req, &story, &project, workspace.as_ref()))
}

async fn resolve_story_workspace(
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

fn build_story_owner_prompt_request(
    state: &AppState,
    mut req: PromptSessionRequest,
    story: &Story,
    project: &Project,
    workspace: Option<&Workspace>,
) -> PromptSessionRequest {
    let (context_markdown, source_summary) = build_story_context_markdown(story, project, workspace);

    let mut prefix_blocks = Vec::new();
    if !context_markdown.trim().is_empty() {
        prefix_blocks.push(json!({
            "type": "resource",
            "resource": {
                "uri": format!("agentdash://story-context/{}", story.id),
                "mimeType": "text/markdown",
                "text": context_markdown,
            }
        }));
    }

    let story_instruction = format!(
        "## Instruction\n你是该 Story 的主代理。请围绕 Story 进行分析、补充上下文、拆解并创建 Task。\n\n可优先使用 Story MCP 工具直接更新 Story、维护上下文引用，并在创建 Task 时为 Task Agent 分配合适的上下文引用。\n\n当前来源摘要：{}",
        if source_summary.is_empty() {
            "-".to_string()
        } else {
            source_summary.join(", ")
        }
    );
    prefix_blocks.push(json!({
        "type": "text",
        "text": story_instruction,
    }));

    let user_blocks = match (req.prompt.take(), req.prompt_blocks.take()) {
        (Some(prompt), None) => vec![json!({
            "type": "text",
            "text": prompt,
        })],
        (None, Some(blocks)) => blocks,
        (Some(prompt), Some(mut blocks)) => {
            let mut merged = vec![json!({ "type": "text", "text": prompt })];
            merged.append(&mut blocks);
            merged
        }
        (None, None) => Vec::new(),
    };

    prefix_blocks.extend(user_blocks);
    req.prompt = None;
    req.prompt_blocks = Some(prefix_blocks);

    if req.working_dir.is_none() {
        req.working_dir = workspace.map(|item| item.container_ref.clone());
    }

    let base_url = state
        .mcp_base_url
        .clone()
        .unwrap_or_else(|| "http://127.0.0.1:3001".to_string());
    req.mcp_servers
        .push(McpInjectionConfig::for_story(base_url, project.id, story.id).to_acp_mcp_server());

    req
}

fn build_story_context_markdown(
    story: &Story,
    project: &Project,
    workspace: Option<&Workspace>,
) -> (String, Vec<String>) {
    let mut composer = ContextComposer::default();
    composer.push(
        "story",
        "story_core",
        10,
        MergeStrategy::Append,
        format!(
            "## Story\n- id: {}\n- title: {}\n- description: {}\n- status: {:?}",
            story.id,
            trim_or_dash(&story.title),
            trim_or_dash(&story.description),
            story.status
        ),
    );
    composer.push(
        "project",
        "project_core",
        20,
        MergeStrategy::Append,
        format!(
            "## Project\n- id: {}\n- name: {}\n- backend_id: {}",
            project.id,
            trim_or_dash(&project.name),
            trim_or_dash(&project.backend_id)
        ),
    );
    if let Some(workspace) = workspace {
        composer.push(
            "workspace",
            "workspace_context",
            30,
            MergeStrategy::Append,
            format!(
                "## Workspace\n- id: {}\n- name: {}\n- path: {}",
                workspace.id,
                trim_or_dash(&workspace.name),
                trim_or_dash(&workspace.container_ref)
            ),
        );
    }
    if let Some(prd) = clean_text(story.context.prd_doc.as_deref()) {
        composer.push(
            "story_context",
            "story_prd",
            40,
            MergeStrategy::Append,
            format!("## Story PRD\n{prd}"),
        );
    }
    if !story.context.spec_refs.is_empty() {
        composer.push(
            "story_context",
            "story_spec_refs",
            41,
            MergeStrategy::Append,
            format!(
                "## Spec Refs\n{}",
                story
                    .context
                    .spec_refs
                    .iter()
                    .map(|item| format!("- {item}"))
                    .collect::<Vec<_>>()
                    .join("\n")
            ),
        );
    }
    if !story.context.resource_list.is_empty() {
        composer.push(
            "story_context",
            "story_resources",
            42,
            MergeStrategy::Append,
            format!(
                "## Resources\n{}",
                story
                    .context
                    .resource_list
                    .iter()
                    .map(|item| format!("- [{}] {} ({})", item.resource_type, item.name, item.uri))
                    .collect::<Vec<_>>()
                    .join("\n")
            ),
        );
    }

    let workspace_root = workspace
        .map(|item| std::path::Path::new(item.container_ref.as_str()))
        .or(Some(std::path::Path::new(".")));
    if let Ok(resolved) = resolve_declared_sources(ResolveSourcesRequest {
        sources: &story.context.source_refs,
        workspace_root,
        base_order: 50,
    }) {
        for fragment in resolved.fragments {
            composer.push_fragment(fragment);
        }
        if !resolved.warnings.is_empty() {
            composer.push(
                "story_context",
                "story_context_warnings",
                59,
                MergeStrategy::Append,
                format!(
                    "## Injection Notes\n{}",
                    resolved
                        .warnings
                        .iter()
                        .map(|item| format!("- {item}"))
                        .collect::<Vec<_>>()
                        .join("\n")
                ),
            );
        }
    }

    composer.compose()
}

fn clean_text(input: Option<&str>) -> Option<&str> {
    input.and_then(|text| {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    })
}

fn trim_or_dash(text: &str) -> &str {
    let trimmed = text.trim();
    if trimmed.is_empty() { "-" } else { trimmed }
}

pub async fn cancel_session(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    state
        .executor_hub
        .cancel(&session_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    Ok(Json(
        serde_json::json!({ "cancelled": true, "sessionId": session_id }),
    ))
}

/// ACP 会话流（Streaming HTTP / SSE）
pub async fn acp_session_stream_sse(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
    headers: HeaderMap,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let last_event_id = headers
        .get("last-event-id")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0);

    tracing::info!(
        session_id = %session_id,
        last_event_id = last_event_id,
        "ACP 会话流连接建立（SSE）"
    );

    let (history, mut rx) = state.executor_hub.subscribe_with_history(&session_id).await;
    let start_index = std::cmp::min(last_event_id as usize, history.len());
    let replayed = history.len().saturating_sub(start_index);
    tracing::info!(
        session_id = %session_id,
        replayed_count = replayed,
        history_total = history.len(),
        "ACP 会话流历史补发完成（SSE）"
    );

    let stream = async_stream::stream! {
        for (i, n) in history.iter().enumerate().skip(start_index) {
            let id = (i as u64) + 1;
            if let Ok(json) = serde_json::to_string(n) {
                yield Ok(Event::default().id(id.to_string()).data(json));
            }
        }

        let mut seq = history.len() as u64;
        loop {
            match rx.recv().await {
                Ok(n) => {
                    seq += 1;
                    if let Ok(json) = serde_json::to_string(&n) {
                        yield Ok(Event::default().id(seq.to_string()).data(json));
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!(
                        session_id = %session_id,
                        lagged = n,
                        "ACP 会话流订阅落后，部分消息被跳过（SSE）"
                    );
                    continue;
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                    tracing::info!(
                        session_id = %session_id,
                        last_seq = seq,
                        "ACP 会话流连接关闭：广播通道关闭（SSE）"
                    );
                    break;
                }
            }
        }
    };

    Sse::new(stream).keep_alive(KeepAlive::default())
}

/// ACP 会话流（Fetch Streaming / NDJSON）
pub async fn acp_session_stream_ndjson(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
    headers: HeaderMap,
    Query(query): Query<NdjsonStreamQuery>,
) -> impl IntoResponse {
    let resume_from = parse_ndjson_resume_from(&headers, query.since_id);
    tracing::info!(
        session_id = %session_id,
        resume_from = resume_from,
        "ACP 会话流连接建立（NDJSON）"
    );

    let (history, mut rx) = state.executor_hub.subscribe_with_history(&session_id).await;
    let start_index = std::cmp::min(resume_from as usize, history.len());
    let replayed = history.len().saturating_sub(start_index);
    tracing::info!(
        session_id = %session_id,
        replayed_count = replayed,
        history_total = history.len(),
        "ACP 会话流历史补发完成（NDJSON）"
    );

    let stream = async_stream::stream! {
        let mut seq = history.len() as u64;
        if let Some(line) = to_ndjson_line(&serde_json::json!({
            "type": "connected",
            "last_event_id": seq,
        })) {
            yield Ok::<Bytes, Infallible>(line);
        }

        for (i, n) in history.iter().enumerate().skip(start_index) {
            let id = (i as u64) + 1;
            if let Some(line) = to_ndjson_line(&serde_json::json!({
                "type": "notification",
                "id": id,
                "notification": n,
            })) {
                yield Ok::<Bytes, Infallible>(line);
            }
        }

        let mut heartbeat_tick = tokio::time::interval(ACP_HEARTBEAT_INTERVAL);
        heartbeat_tick.set_missed_tick_behavior(MissedTickBehavior::Delay);

        loop {
            tokio::select! {
                next = rx.recv() => {
                    match next {
                        Ok(n) => {
                            seq += 1;
                            if let Some(line) = to_ndjson_line(&serde_json::json!({
                                "type": "notification",
                                "id": seq,
                                "notification": n,
                            })) {
                                yield Ok::<Bytes, Infallible>(line);
                            }
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                            tracing::warn!(
                                session_id = %session_id,
                                lagged = n,
                                "ACP 会话流订阅落后，部分消息被跳过（NDJSON）"
                            );
                            continue;
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                            tracing::info!(
                                session_id = %session_id,
                                last_seq = seq,
                                "ACP 会话流连接关闭：广播通道关闭（NDJSON）"
                            );
                            break;
                        }
                    }
                }
                _ = heartbeat_tick.tick() => {
                    if let Some(line) = to_ndjson_line(&serde_json::json!({
                        "type": "heartbeat",
                        "timestamp": chrono::Utc::now().timestamp_millis(),
                    })) {
                        yield Ok::<Bytes, Infallible>(line);
                    }
                }
            }
        }
    };

    (
        [
            (
                axum::http::header::CONTENT_TYPE,
                "application/x-ndjson; charset=utf-8",
            ),
            (axum::http::header::CACHE_CONTROL, "no-cache, no-transform"),
            (axum::http::header::CONNECTION, "keep-alive"),
            (axum::http::header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
        ],
        Body::from_stream(stream),
    )
}

fn parse_ndjson_resume_from(headers: &HeaderMap, query_since_id: Option<u64>) -> u64 {
    headers
        .get("x-stream-since-id")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok())
        .or(query_since_id)
        .unwrap_or(0)
}

fn to_ndjson_line(value: &serde_json::Value) -> Option<Bytes> {
    match serde_json::to_vec(value) {
        Ok(mut bytes) => {
            bytes.push(b'\n');
            Some(Bytes::from(bytes))
        }
        Err(err) => {
            tracing::error!(error = %err, "序列化 ACP NDJSON 消息失败");
            None
        }
    }
}
