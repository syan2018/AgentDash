use std::sync::Arc;

use agentdash_application::session::construction_planner::{
    ResolvedProjectAgentContext, SessionConstructionPlanner,
};
use agentdash_domain::{agent::ProjectAgent, inline_file::InlineFileOwnerKind, project::Project};
use axum::{
    Json,
    extract::{Path, Query, State},
};
use uuid::Uuid;

use agentdash_contracts::core::DeletedFlagResponse;
use agentdash_contracts::project_agent::{
    CreateProjectAgentRequest, OpenProjectAgentSessionResult, ProjectAgent as ProjectAgentResponse,
    ProjectAgentExecutor, ProjectAgentSession, ProjectAgentSummary, ThinkingLevel,
    UpdateProjectAgentRequest,
};

use crate::{
    app_state::AppState,
    auth::{CurrentUser, ProjectPermission, load_project_with_permission},
    dto::OpenSessionQuery,
    rpc::ApiError,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_agent_summary_response_serializes_as_snake_case() {
        let value = serde_json::to_value(ProjectAgentSummary {
            key: "default".to_string(),
            display_name: "项目默认 Agent".to_string(),
            description: "desc".to_string(),
            executor: ProjectAgentExecutor {
                executor: "PI_AGENT".to_string(),
                provider_id: Some("openai".to_string()),
                model_id: Some("test-model".to_string()),
                agent_id: None,
                thinking_level: None,
                permission_policy: Some("AUTO".to_string()),
            },
            preset_name: Some("preset".to_string()),
            source: "project.config.default_agent_type".to_string(),
            session: Some(ProjectAgentSession {
                binding_id: "binding-1".to_string(),
                session_id: "sess-1".to_string(),
                session_title: Some("title".to_string()),
                last_activity: Some(1),
            }),
        })
        .expect("serialize project agent summary");

        assert!(value.get("display_name").is_some());
        assert!(value.get("preset_name").is_some());
        assert!(value.get("displayName").is_none());
        assert!(value.get("presetName").is_none());
    }

    #[test]
    fn parse_project_agent_session_label_requires_expected_prefix() {
        assert_eq!(
            SessionConstructionPlanner::parse_project_agent_session_label("project_agent:agent-1"),
            Some("agent-1")
        );
        assert_eq!(
            SessionConstructionPlanner::parse_project_agent_session_label("agent-1"),
            None
        );
        assert_eq!(
            SessionConstructionPlanner::parse_project_agent_session_label("project_agent:   "),
            None
        );
    }
}

pub fn router() -> axum::Router<std::sync::Arc<crate::app_state::AppState>> {
    axum::Router::new()
        .route(
            "/projects/{id}/agents",
            axum::routing::get(list_project_agent_configs).post(create_project_agent),
        )
        .route(
            "/projects/{id}/agents/summary",
            axum::routing::get(list_project_agents),
        )
        .route(
            "/projects/{id}/agents/{project_agent_id}",
            axum::routing::put(update_project_agent).delete(delete_project_agent),
        )
        .route(
            "/projects/{id}/agents/{project_agent_id}/session",
            axum::routing::post(open_project_agent_session),
        )
        .route(
            "/projects/{id}/agents/{project_agent_id}/sessions",
            axum::routing::get(list_project_agent_sessions),
        )
}

pub async fn list_project_agents(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(project_id): Path<String>,
) -> Result<Json<Vec<ProjectAgentSummary>>, ApiError> {
    let project_id = parse_project_id(&project_id)?;
    let project = load_project_with_permission(
        state.as_ref(),
        &current_user,
        project_id,
        ProjectPermission::View,
    )
    .await?;

    let agents = state
        .repos
        .project_agent_repo
        .list_by_project(project_id)
        .await
        .map_err(ApiError::from)?;

    let mut response = Vec::with_capacity(agents.len());
    for agent in &agents {
        let bridge = SessionConstructionPlanner::build_project_agent_context(&state.repos, agent)
            .await
            .map_err(ApiError::Internal)?;
        let session = find_project_agent_session(&state, project.id, &bridge.key).await?;
        response.push(build_project_agent_summary(&project, &bridge, session));
    }

    response.sort_by(|a, b| a.display_name.cmp(&b.display_name));
    Ok(Json(response))
}

pub async fn open_project_agent_session(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path((project_id, agent_key)): Path<(String, String)>,
    Query(query): Query<OpenSessionQuery>,
) -> Result<Json<OpenProjectAgentSessionResult>, ApiError> {
    let project_id = parse_project_id(&project_id)?;
    let project = load_project_with_permission(
        state.as_ref(),
        &current_user,
        project_id,
        ProjectPermission::Edit,
    )
    .await?;

    let project_agent_id = parse_project_agent_id(&agent_key)?;
    let project_agent = state
        .repos
        .project_agent_repo
        .get_by_project_and_id(project_id, project_agent_id)
        .await
        .map_err(ApiError::from)?
        .ok_or_else(|| ApiError::NotFound(format!("Project Agent `{agent_key}` 不存在")))?;
    let agent =
        SessionConstructionPlanner::build_project_agent_context(&state.repos, &project_agent)
            .await
            .map_err(ApiError::Internal)?;

    let _ = query.force_new;

    let meta = state
        .services
        .session_core
        .create_session("")
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?;
    state
        .services
        .session_core
        .update_session_meta(&meta.id, |meta| {
            meta.project_id = Some(project.id.to_string());
        })
        .await
        .map_err(ApiError::from)?;
    state
        .services
        .session_core
        .mark_owner_bootstrap_pending(&meta.id)
        .await
        .map_err(ApiError::from)?;

    // 自动启动显式 Lifecycle；未配置时归属到 freeform LifecycleRun。
    if let Some(lifecycle_key) = project_agent.default_lifecycle_key.as_deref() {
        if let Err(err) =
            auto_start_lifecycle_run(&state, project.id, &meta.id, lifecycle_key).await
        {
            tracing::warn!(
                project_id = %project.id,
                agent_key = %agent_key,
                lifecycle_key = %lifecycle_key,
                error = %err,
                "自动启动 Lifecycle Run 失败（不阻塞 session 创建）"
            );
        }
    } else if let Err(err) =
        crate::routes::acp_sessions::ensure_freeform_lifecycle_run(&state, project.id, &meta.id)
            .await
    {
        tracing::warn!(
            project_id = %project.id,
            agent_key = %agent_key,
            error = %err,
            "自动启动 freeform LifecycleRun 失败（不阻塞 session 创建）"
        );
    }

    let session = Some(ProjectAgentSession {
        binding_id: meta.id.clone(),
        session_id: meta.id.clone(),
        session_title: Some(meta.title),
        last_activity: Some(meta.updated_at),
    });
    let summary = build_project_agent_summary(&project, &agent, session);

    Ok(Json(OpenProjectAgentSessionResult {
        created: true,
        session_id: meta.id.clone(),
        binding_id: meta.id,
        agent: summary,
    }))
}

fn build_project_agent_summary(
    _project: &Project,
    agent: &ResolvedProjectAgentContext,
    session: Option<ProjectAgentSession>,
) -> ProjectAgentSummary {
    ProjectAgentSummary {
        key: agent.key.clone(),
        display_name: agent.display_name.clone(),
        description: agent.description.clone(),
        executor: ProjectAgentExecutor {
            executor: agent.executor_config.executor.clone(),
            provider_id: agent.executor_config.provider_id.clone(),
            model_id: agent.executor_config.model_id.clone(),
            agent_id: agent.executor_config.agent_id.clone(),
            thinking_level: agent
                .executor_config
                .thinking_level
                .map(thinking_level_response),
            permission_policy: agent.executor_config.permission_policy.clone(),
        },
        preset_name: agent.preset_name.clone(),
        source: agent.source.clone(),
        session,
    }
}

fn thinking_level_response(level: agentdash_spi::ThinkingLevel) -> ThinkingLevel {
    use agentdash_spi::ThinkingLevel as SpiThinkingLevel;

    match level {
        SpiThinkingLevel::Off => ThinkingLevel::Off,
        SpiThinkingLevel::Minimal => ThinkingLevel::Minimal,
        SpiThinkingLevel::Low => ThinkingLevel::Low,
        SpiThinkingLevel::Medium => ThinkingLevel::Medium,
        SpiThinkingLevel::High => ThinkingLevel::High,
        SpiThinkingLevel::Xhigh => ThinkingLevel::Xhigh,
    }
}

async fn find_project_agent_session(
    _state: &Arc<AppState>,
    _project_id: Uuid,
    _agent_key: &str,
) -> Result<Option<ProjectAgentSession>, ApiError> {
    Ok(None)
}

pub async fn list_project_agent_sessions(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path((project_id, _agent_key)): Path<(String, String)>,
) -> Result<Json<Vec<ProjectAgentSession>>, ApiError> {
    let project_id = parse_project_id(&project_id)?;
    let _project = load_project_with_permission(
        state.as_ref(),
        &current_user,
        project_id,
        ProjectPermission::View,
    )
    .await?;

    Ok(Json(vec![]))
}

fn parse_project_id(project_id: &str) -> Result<Uuid, ApiError> {
    Uuid::parse_str(project_id)
        .map_err(|_| ApiError::BadRequest(format!("无效的 project_id: {project_id}")))
}

fn parse_project_agent_id(project_agent_id: &str) -> Result<Uuid, ApiError> {
    Uuid::parse_str(project_agent_id)
        .map_err(|_| ApiError::BadRequest(format!("无效的 project_agent_id: {project_agent_id}")))
}

// ─── Project Agent API ───

fn build_project_agent_response(agent: &ProjectAgent) -> Result<ProjectAgentResponse, ApiError> {
    Ok(ProjectAgentResponse {
        id: agent.id.to_string(),
        project_id: agent.project_id.to_string(),
        name: agent.name.clone(),
        agent_type: agent.agent_type.clone(),
        config: agent.config.clone(),
        default_lifecycle_key: agent.default_lifecycle_key.clone(),
        is_default_for_story: agent.is_default_for_story,
        is_default_for_task: agent.is_default_for_task,
        knowledge_enabled: agent.knowledge_enabled,
        created_at: agent.created_at.to_rfc3339(),
        updated_at: agent.updated_at.to_rfc3339(),
    })
}

/// GET /projects/{id}/agents — 列出项目内所有 Project Agent
pub async fn list_project_agent_configs(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(project_id): Path<String>,
) -> Result<Json<Vec<ProjectAgentResponse>>, ApiError> {
    let project_id = parse_project_id(&project_id)?;
    load_project_with_permission(
        state.as_ref(),
        &current_user,
        project_id,
        ProjectPermission::View,
    )
    .await?;

    let agents = state
        .repos
        .project_agent_repo
        .list_by_project(project_id)
        .await
        .map_err(ApiError::from)?;

    let response = agents
        .iter()
        .map(build_project_agent_response)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Json(response))
}

/// POST /projects/{id}/agents — 创建项目私有 Agent
pub async fn create_project_agent(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(project_id): Path<String>,
    Json(req): Json<CreateProjectAgentRequest>,
) -> Result<Json<ProjectAgentResponse>, ApiError> {
    let project_id = parse_project_id(&project_id)?;
    load_project_with_permission(
        state.as_ref(),
        &current_user,
        project_id,
        ProjectPermission::Edit,
    )
    .await?;

    let name = req.name.trim().to_string();
    if name.is_empty() {
        return Err(ApiError::BadRequest("name 不能为空".into()));
    }
    let agent_type = req.agent_type.trim().to_string();
    if agent_type.is_empty() {
        return Err(ApiError::BadRequest("agent_type 不能为空".into()));
    }
    if state
        .repos
        .project_agent_repo
        .get_by_project_and_name(project_id, &name)
        .await
        .map_err(ApiError::from)?
        .is_some()
    {
        return Err(ApiError::Conflict(format!(
            "Project Agent key 已存在: {name}"
        )));
    }

    let lifecycle_key = resolve_lifecycle_key_for_project_agent(
        &state,
        project_id,
        req.default_lifecycle_key,
        req.default_workflow_key,
    )
    .await?;

    let mut agent = ProjectAgent::new(project_id, name, agent_type);
    if let Some(config) = req.config {
        agent.config = config;
    }
    agent.default_lifecycle_key = lifecycle_key;
    agent.is_default_for_story = req.is_default_for_story;
    agent.is_default_for_task = req.is_default_for_task;

    state
        .repos
        .project_agent_repo
        .create(&agent)
        .await
        .map_err(ApiError::from)?;

    Ok(Json(build_project_agent_response(&agent)?))
}

/// PUT /projects/{id}/agents/{project_agent_id} — 更新 Project Agent
pub async fn update_project_agent(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path((project_id, project_agent_id)): Path<(String, String)>,
    Json(req): Json<UpdateProjectAgentRequest>,
) -> Result<Json<ProjectAgentResponse>, ApiError> {
    let project_id = parse_project_id(&project_id)?;
    load_project_with_permission(
        state.as_ref(),
        &current_user,
        project_id,
        ProjectPermission::Edit,
    )
    .await?;
    let project_agent_id = parse_project_agent_id(&project_agent_id)?;

    let mut agent = state
        .repos
        .project_agent_repo
        .get_by_project_and_id(project_id, project_agent_id)
        .await
        .map_err(ApiError::from)?
        .ok_or_else(|| ApiError::NotFound(format!("Project Agent {project_agent_id} 不存在")))?;

    if let Some(name) = req.name {
        let trimmed = name.trim().to_string();
        if trimmed.is_empty() {
            return Err(ApiError::BadRequest("name 不能为空".into()));
        }
        agent.name = trimmed;
    }
    if let Some(agent_type) = req.agent_type {
        let trimmed = agent_type.trim().to_string();
        if trimmed.is_empty() {
            return Err(ApiError::BadRequest("agent_type 不能为空".into()));
        }
        agent.agent_type = trimmed;
    }
    if let Some(config) = req.config {
        agent.config = config;
    }
    if req.default_lifecycle_key.is_some() || req.default_workflow_key.is_some() {
        agent.default_lifecycle_key = resolve_lifecycle_key_for_project_agent(
            &state,
            project_id,
            req.default_lifecycle_key,
            req.default_workflow_key,
        )
        .await?;
    }
    if let Some(v) = req.is_default_for_story {
        agent.is_default_for_story = v;
    }
    if let Some(v) = req.is_default_for_task {
        agent.is_default_for_task = v;
    }
    if let Some(v) = req.knowledge_enabled {
        agent.knowledge_enabled = v;
    }
    agent.updated_at = chrono::Utc::now();

    state
        .repos
        .project_agent_repo
        .update(&agent)
        .await
        .map_err(ApiError::from)?;

    Ok(Json(build_project_agent_response(&agent)?))
}

/// DELETE /projects/{id}/agents/{project_agent_id} — 删除 Project Agent
pub async fn delete_project_agent(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path((project_id, project_agent_id)): Path<(String, String)>,
) -> Result<Json<DeletedFlagResponse>, ApiError> {
    let project_id = parse_project_id(&project_id)?;
    load_project_with_permission(
        state.as_ref(),
        &current_user,
        project_id,
        ProjectPermission::Edit,
    )
    .await?;
    let project_agent_id = parse_project_agent_id(&project_agent_id)?;

    let routines = state
        .repos
        .routine_repo
        .list_by_project(project_id)
        .await
        .map_err(ApiError::from)?;
    if routines
        .iter()
        .any(|routine| routine.project_agent_id == project_agent_id)
    {
        return Err(ApiError::BadRequest(
            "该 Project Agent 仍被 Routine 使用，需先调整或删除相关 Routine".into(),
        ));
    }

    state
        .repos
        .inline_file_repo
        .delete_by_owner(InlineFileOwnerKind::ProjectAgent, project_agent_id)
        .await
        .map_err(ApiError::from)?;

    state
        .repos
        .project_agent_repo
        .delete(project_id, project_agent_id)
        .await
        .map_err(ApiError::from)?;

    Ok(Json(DeletedFlagResponse { deleted: true }))
}

/// 统一处理 lifecycle_key / workflow_key 的解析
///
/// 如果用户指定了 `default_workflow_key`（单个 workflow），
/// 自动创建一个单步 lifecycle 包装它。
async fn resolve_lifecycle_key_for_project_agent(
    state: &Arc<AppState>,
    project_id: Uuid,
    lifecycle_key: Option<String>,
    workflow_key: Option<String>,
) -> Result<Option<String>, ApiError> {
    if let Some(lk) = lifecycle_key {
        let trimmed = lk.trim().to_string();
        if trimmed.is_empty() {
            return Ok(None);
        }
        state
            .repos
            .activity_lifecycle_definition_repo
            .get_by_project_and_key(project_id, &trimmed)
            .await
            .map_err(ApiError::from)?
            .ok_or_else(|| ApiError::NotFound(format!("Lifecycle `{trimmed}` 不存在")))?;
        return Ok(Some(trimmed));
    }

    if let Some(wk) = workflow_key {
        let wk = wk.trim().to_string();
        if wk.is_empty() {
            return Ok(None);
        }

        let workflow = state
            .repos
            .workflow_definition_repo
            .get_by_project_and_key(project_id, &wk)
            .await
            .map_err(ApiError::from)?
            .ok_or_else(|| ApiError::NotFound(format!("Workflow `{wk}` 不存在")))?;

        let auto_key = format!("auto:{wk}");

        let existing = state
            .repos
            .activity_lifecycle_definition_repo
            .get_by_project_and_key(project_id, &auto_key)
            .await
            .map_err(ApiError::from)?;

        if existing.is_none() {
            use agentdash_domain::workflow::{
                ActivityCompletionPolicy, ActivityDefinition, ActivityExecutorSpec,
                ActivityLifecycleDefinition, AgentActivityExecutorSpec, AgentSessionPolicy,
                WorkflowDefinitionSource,
            };
            let lifecycle = ActivityLifecycleDefinition {
                id: Uuid::new_v4(),
                project_id,
                key: auto_key.clone(),
                name: format!("Auto: {wk}"),
                description: format!("自动创建：包装单个 workflow `{wk}`"),
                binding_kinds: workflow.binding_kinds.clone(),
                source: WorkflowDefinitionSource::UserAuthored,
                installed_source: None,
                version: 1,
                activities: vec![ActivityDefinition {
                    key: "main".to_string(),
                    description: String::new(),
                    executor: ActivityExecutorSpec::Agent(AgentActivityExecutorSpec {
                        workflow_key: wk,
                        session_policy: AgentSessionPolicy::ContinueRoot,
                    }),
                    output_ports: vec![],
                    input_ports: vec![],
                    completion_policy: ActivityCompletionPolicy::OpenEnded,
                    iteration_policy: Default::default(),
                    join_policy: Default::default(),
                }],
                transitions: vec![],
                entry_activity_key: "main".to_string(),
                created_at: chrono::Utc::now(),
                updated_at: chrono::Utc::now(),
            };
            state
                .repos
                .activity_lifecycle_definition_repo
                .create(&lifecycle)
                .await
                .map_err(ApiError::from)?;
        }

        return Ok(Some(auto_key));
    }

    Ok(None)
}

/// 自动启动 lifecycle run（首步含 workflow_key 时同时激活首步）
async fn auto_start_lifecycle_run(
    state: &Arc<AppState>,
    project_id: Uuid,
    session_id: &str,
    lifecycle_key: &str,
) -> Result<(), String> {
    use agentdash_application::workflow::{
        ActivityLifecycleRunService, AgentActivityExecutorLauncher, AgentActivityLaunchContext,
        AgentActivityRuntimePort, StartActivityLifecycleRunCommand,
    };

    let service = ActivityLifecycleRunService::new(
        state.repos.activity_lifecycle_definition_repo.as_ref(),
        state.repos.lifecycle_run_repo.as_ref(),
        state.repos.activity_execution_claim_repo.as_ref(),
    );

    let cmd = StartActivityLifecycleRunCommand {
        project_id,
        lifecycle_id: None,
        lifecycle_key: Some(lifecycle_key.to_string()),
        session_id: session_id.to_string(),
    };

    let run = service
        .start_run(cmd)
        .await
        .map_err(|e| format!("start_run 失败: {e}"))?;

    let launcher = AgentActivityExecutorLauncher::new(
        AgentActivityLaunchContext {
            project_id: run.project_id,
            lifecycle_key: lifecycle_key.to_string(),
            root_session_id: run.session_id.clone().unwrap_or_default(),
        },
        AgentActivityRuntimePort::new(
            state.services.session_core.clone(),
            state.services.session_launch.clone(),
            state.repos.clone(),
            Arc::new(agentdash_infrastructure::DefaultFunctionRunner::new()),
        )
        .with_runtime_context(
            state.services.session_hooks.clone(),
            state.services.session_capability.clone(),
            state.config.platform_config.clone(),
        ),
    );
    service
        .launch_ready_attempts(run.id, &launcher)
        .await
        .map_err(|e| format!("启动 lifecycle activity 失败: {e}"))?;

    Ok(())
}
