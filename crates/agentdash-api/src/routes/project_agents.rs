use std::sync::Arc;

use agentdash_application::session::construction_planner::{
    ResolvedProjectAgentContext, build_project_agent_context,
};
use agentdash_application::workflow::{
    AgentRunMessageLaunchDeliveryPort, ProjectAgentRunStartCommand, ProjectAgentRunStartRepos,
    ProjectAgentRunStartService,
};
use agentdash_domain::{
    agent::ProjectAgent, inline_file::InlineFileOwnerKind, project::Project, workflow::SubjectRef,
};
use agentdash_spi::AgentConfig;
use axum::{
    Json,
    extract::{Path, State},
};
use uuid::Uuid;

use agentdash_contracts::core::DeletedFlagResponse;
use agentdash_contracts::project_agent::{
    CreateProjectAgentRequest, CreateProjectAgentRunRequest, ProjectAgent as ProjectAgentResponse,
    ProjectAgentExecutor, ProjectAgentLaunchResult, ProjectAgentRunStartResult,
    ProjectAgentSummary, ThinkingLevel, UpdateProjectAgentRequest,
};
use agentdash_contracts::workflow::{
    AgentFrameRefDto, AgentRunAcceptedRefs, AgentRunCommandReceipt, AgentRunRefDto,
    LifecycleRunRefDto, RuntimeSessionRefDto, SubjectRefDto,
};

use crate::{
    app_state::AppState,
    auth::{CurrentUser, ProjectPermission, load_project_with_permission},
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
        })
        .expect("serialize project agent summary");

        assert!(value.get("display_name").is_some());
        assert!(value.get("preset_name").is_some());
        assert!(value.get("displayName").is_none());
        assert!(value.get("presetName").is_none());
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
            "/projects/{id}/agents/{project_agent_id}/launch",
            axum::routing::post(launch_project_agent),
        )
        .route(
            "/projects/{id}/agents/{project_agent_id}/agent-runs",
            axum::routing::post(create_project_agent_run),
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
        let bridge = build_project_agent_context(&state.repos, agent)
            .await
            .map_err(ApiError::Internal)?;
        response.push(build_project_agent_summary(&project, &bridge));
    }

    response.sort_by(|a, b| a.display_name.cmp(&b.display_name));
    Ok(Json(response))
}

pub async fn launch_project_agent(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path((project_id, agent_key)): Path<(String, String)>,
) -> Result<Json<ProjectAgentLaunchResult>, ApiError> {
    use agentdash_application::workflow::LifecycleDispatchService;
    use agentdash_domain::workflow::{
        AgentLaunchIntent, AgentPolicy, CapabilityPolicy, ContextPolicy, ExecutionSource,
        RunPolicy, RuntimePolicy, SubjectRef, WorkflowGraphRef,
    };

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
    let agent_context = build_project_agent_context(&state.repos, &project_agent)
        .await
        .map_err(ApiError::Internal)?;

    let workflow_graph_ref = project_agent
        .default_lifecycle_key
        .as_deref()
        .map(str::trim)
        .filter(|key| !key.is_empty())
        .map(|key| WorkflowGraphRef::ByKey {
            project_id: project.id,
            key: key.to_string(),
        });

    // 构造 AgentLaunchIntent 并通过 dispatch service 创建 lifecycle 实体
    let intent = AgentLaunchIntent {
        project_id: project.id,
        source: ExecutionSource::ProjectAgent,
        subject_ref: Some(SubjectRef::new("project", project.id)),
        parent_run_id: None,
        parent_agent_id: None,
        workflow_graph_ref,
        run_policy: RunPolicy::CreateLinkedRun,
        agent_policy: AgentPolicy::Create,
        context_policy: ContextPolicy::Isolated,
        capability_policy: CapabilityPolicy::Baseline,
        runtime_policy: RuntimePolicy::CreateRuntimeSession,
    };

    let dispatch_service = LifecycleDispatchService::new(
        state.repos.lifecycle_run_repo.as_ref(),
        state.repos.workflow_graph_repo.as_ref(),
        state.repos.lifecycle_agent_repo.as_ref(),
        state.repos.agent_frame_repo.as_ref(),
        state.repos.lifecycle_subject_association_repo.as_ref(),
        state.repos.lifecycle_gate_repo.as_ref(),
        state.repos.agent_lineage_repo.as_ref(),
    )
    .with_anchor_repo(state.repos.execution_anchor_repo.as_ref())
    .with_runtime_session_creator(state.repos.runtime_session_creator.as_ref());

    let dispatch_result = dispatch_service
        .launch_agent(&intent)
        .await
        .map_err(|err| ApiError::Internal(format!("Lifecycle dispatch 失败: {err}")))?;
    if let Some(mut lifecycle_agent) = state
        .repos
        .lifecycle_agent_repo
        .get(dispatch_result.runtime_refs.agent_ref)
        .await
        .map_err(ApiError::from)?
    {
        lifecycle_agent.project_agent_id = Some(project_agent.id);
        state
            .repos
            .lifecycle_agent_repo
            .update(&lifecycle_agent)
            .await
            .map_err(ApiError::from)?;
    }

    let summary = build_project_agent_summary(&project, &agent_context);

    Ok(Json(ProjectAgentLaunchResult {
        created: true,
        agent: summary,
        run_ref: LifecycleRunRefDto {
            run_id: dispatch_result.runtime_refs.run_ref.to_string(),
        },
        agent_ref: AgentRunRefDto {
            run_id: dispatch_result.runtime_refs.run_ref.to_string(),
            agent_id: dispatch_result.runtime_refs.agent_ref.to_string(),
        },
        frame_ref: AgentFrameRefDto {
            agent_id: dispatch_result.runtime_refs.agent_ref.to_string(),
            frame_id: dispatch_result.runtime_refs.frame_ref.to_string(),
            revision: None,
        },
        delivery_runtime_ref: dispatch_result
            .delivery_runtime_ref
            .map(|runtime_session_id| RuntimeSessionRefDto {
                runtime_session_id: runtime_session_id.to_string(),
            }),
        subject_ref: intent.subject_ref.as_ref().map(|subject| SubjectRefDto {
            kind: subject.kind.clone(),
            id: subject.id.to_string(),
        }),
    }))
}

pub async fn create_project_agent_run(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path((project_id, agent_key)): Path<(String, String)>,
    Json(req): Json<CreateProjectAgentRunRequest>,
) -> Result<Json<ProjectAgentRunStartResult>, ApiError> {
    if req.client_command_id.trim().is_empty() {
        return Err(ApiError::BadRequest(
            "client_command_id 不能为空".to_string(),
        ));
    }
    let project_id = parse_project_id(&project_id)?;
    let project = load_project_with_permission(
        state.as_ref(),
        &current_user,
        project_id,
        ProjectPermission::Edit,
    )
    .await?;
    let project_agent_id = parse_project_agent_id(&agent_key)?;
    let executor_config = req
        .executor_config
        .map(serde_json::from_value::<AgentConfig>)
        .transpose()
        .map_err(|error| ApiError::BadRequest(format!("executor_config 非法: {error}")))?;

    let service = ProjectAgentRunStartService::new(
        ProjectAgentRunStartRepos::from_repository_set(&state.repos),
        &state.services.session_core,
    );
    let delivery = AgentRunMessageLaunchDeliveryPort::new(state.services.session_launch.clone());
    let dispatch = service
        .start_run(
            ProjectAgentRunStartCommand {
                project_id,
                project_agent_id,
                input: req.input,
                client_command_id: req.client_command_id,
                executor_config,
                subject_ref: parse_subject_ref(req.subject_ref)?,
                identity: Some(current_user.clone()),
            },
            delivery,
        )
        .await
        .map_err(ApiError::from)?;

    let agent_context = build_project_agent_context(&state.repos, &dispatch.project_agent)
        .await
        .map_err(ApiError::Internal)?;
    let summary = build_project_agent_summary(&project, &agent_context);

    Ok(Json(ProjectAgentRunStartResult {
        command_receipt: AgentRunCommandReceipt {
            client_command_id: dispatch.command_receipt.client_command_id,
            status: dispatch.command_receipt.status,
            duplicate: dispatch.command_receipt.duplicate,
            message: dispatch.command_receipt.message,
        },
        accepted_refs: AgentRunAcceptedRefs {
            run_ref: LifecycleRunRefDto {
                run_id: dispatch.run_id.to_string(),
            },
            agent_ref: AgentRunRefDto {
                run_id: dispatch.run_id.to_string(),
                agent_id: dispatch.agent_id.to_string(),
            },
            frame_ref: Some(AgentFrameRefDto {
                agent_id: dispatch.agent_id.to_string(),
                frame_id: dispatch.frame_id.to_string(),
                revision: Some(dispatch.frame_revision),
            }),
            runtime_session_ref: Some(RuntimeSessionRefDto {
                runtime_session_id: dispatch.runtime_session_id.clone(),
            }),
            turn_id: Some(dispatch.turn_id.clone()),
        },
        runtime_session_id: dispatch.runtime_session_id,
        turn_id: dispatch.turn_id,
        agent: summary,
        run_ref: LifecycleRunRefDto {
            run_id: dispatch.run_id.to_string(),
        },
        agent_ref: AgentRunRefDto {
            run_id: dispatch.run_id.to_string(),
            agent_id: dispatch.agent_id.to_string(),
        },
        frame_ref: AgentFrameRefDto {
            agent_id: dispatch.agent_id.to_string(),
            frame_id: dispatch.frame_id.to_string(),
            revision: Some(dispatch.frame_revision),
        },
        subject_ref: dispatch.subject_ref.as_ref().map(|subject| SubjectRefDto {
            kind: subject.kind.clone(),
            id: subject.id.to_string(),
        }),
    }))
}

fn build_project_agent_summary(
    _project: &Project,
    agent: &ResolvedProjectAgentContext,
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

fn parse_project_id(project_id: &str) -> Result<Uuid, ApiError> {
    Uuid::parse_str(project_id)
        .map_err(|_| ApiError::BadRequest(format!("无效的 project_id: {project_id}")))
}

fn parse_project_agent_id(project_agent_id: &str) -> Result<Uuid, ApiError> {
    Uuid::parse_str(project_agent_id)
        .map_err(|_| ApiError::BadRequest(format!("无效的 project_agent_id: {project_agent_id}")))
}

fn parse_subject_ref(subject_ref: Option<SubjectRefDto>) -> Result<Option<SubjectRef>, ApiError> {
    subject_ref
        .map(|subject| {
            let id = Uuid::parse_str(&subject.id).map_err(|_| {
                ApiError::BadRequest(format!("无效的 subject_ref.id: {}", subject.id))
            })?;
            Ok(SubjectRef::new(subject.kind, id))
        })
        .transpose()
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

    let lifecycle_key =
        resolve_lifecycle_key_for_project_agent(&state, project_id, req.default_lifecycle_key)
            .await?;

    let mut agent = ProjectAgent::new(project_id, name, agent_type);
    if let Some(config) = req.config {
        agent.config = config;
    }
    agent.default_lifecycle_key = lifecycle_key;

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
    if let Some(default_lifecycle_key) = req.default_lifecycle_key {
        agent.default_lifecycle_key = resolve_lifecycle_key_for_project_agent(
            &state,
            project_id,
            Some(default_lifecycle_key),
        )
        .await?;
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

async fn resolve_lifecycle_key_for_project_agent(
    state: &Arc<AppState>,
    project_id: Uuid,
    lifecycle_key: Option<String>,
) -> Result<Option<String>, ApiError> {
    if let Some(lk) = lifecycle_key {
        let trimmed = lk.trim().to_string();
        if trimmed.is_empty() {
            return Ok(None);
        }
        state
            .repos
            .workflow_graph_repo
            .get_by_project_and_key(project_id, &trimmed)
            .await
            .map_err(ApiError::from)?
            .ok_or_else(|| ApiError::NotFound(format!("Lifecycle `{trimmed}` 不存在")))?;
        return Ok(Some(trimmed));
    }

    Ok(None)
}
