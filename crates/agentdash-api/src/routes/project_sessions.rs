use std::sync::Arc;

use axum::{
    Json,
    extract::{Path, State},
};
use serde::Serialize;
use uuid::Uuid;

use crate::{
    app_state::AppState,
    routes::project_agents::{
        ProjectAgentMountResponse, build_project_agent_visible_mounts, normalize_optional_string,
        resolve_project_agent_bridge, resolve_project_workspace,
    },
    routes::task_execution::{
        SessionEffectiveContext, SessionProjectDefaults, TaskSessionExecutorSummary,
        build_session_executor_summary,
    },
    rpc::ApiError,
    session_plan::{summarize_runtime_policy, summarize_tool_visibility},
    workflow_runtime::{
        WorkflowRuntimeContext, WorkflowRuntimeSnapshot, resolve_workflow_runtime_injection,
    },
};
use agentdash_domain::session_binding::SessionOwnerType;
use agentdash_mcp::injection::McpInjectionConfig;

#[derive(Debug, Serialize)]
pub struct ProjectSessionContextSnapshot {
    pub agent_key: String,
    pub agent_display_name: String,
    pub executor: TaskSessionExecutorSummary,
    pub project_defaults: SessionProjectDefaults,
    pub effective: SessionEffectiveContext,
    pub shared_context_mounts: Vec<ProjectAgentMountResponse>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workflow_runtime: Option<WorkflowRuntimeSnapshot>,
}

#[derive(Debug, Serialize)]
pub struct ProjectSessionDetailResponse {
    pub binding_id: String,
    pub session_id: String,
    pub label: String,
    pub session_title: Option<String>,
    pub last_activity: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub address_space: Option<agentdash_executor::ExecutionAddressSpace>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_snapshot: Option<ProjectSessionContextSnapshot>,
}

#[derive(Debug)]
struct BuiltProjectSessionContextResponse {
    address_space: Option<agentdash_executor::ExecutionAddressSpace>,
    context_snapshot: Option<ProjectSessionContextSnapshot>,
}

pub async fn get_project_session(
    State(state): State<Arc<AppState>>,
    Path((project_id, binding_id)): Path<(String, String)>,
) -> Result<Json<ProjectSessionDetailResponse>, ApiError> {
    let project_uuid = Uuid::parse_str(&project_id)
        .map_err(|_| ApiError::BadRequest(format!("无效的 project_id: {project_id}")))?;
    let binding_uuid = Uuid::parse_str(&binding_id)
        .map_err(|_| ApiError::BadRequest(format!("无效的 binding_id: {binding_id}")))?;

    let project = state
        .repos
        .project_repo
        .get_by_id(project_uuid)
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?
        .ok_or_else(|| ApiError::NotFound(format!("Project {project_id} 不存在")))?;

    let bindings = state
        .repos
        .session_binding_repo
        .list_by_owner(SessionOwnerType::Project, project_uuid)
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?;
    let binding = bindings
        .into_iter()
        .find(|item| item.id == binding_uuid)
        .ok_or_else(|| {
            ApiError::NotFound(format!("Project Session binding {binding_id} 不存在"))
        })?;

    let meta = state
        .services
        .executor_hub
        .get_session_meta(&binding.session_id)
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?;
    let built_context = build_project_session_context_response(
        &state,
        &project,
        &binding.session_id,
        &binding.label,
    )
    .await;

    Ok(Json(ProjectSessionDetailResponse {
        binding_id,
        session_id: binding.session_id,
        label: binding.label,
        session_title: meta.as_ref().map(|item| item.title.clone()),
        last_activity: meta.as_ref().map(|item| item.updated_at),
        address_space: built_context
            .as_ref()
            .and_then(|context| context.address_space.clone()),
        context_snapshot: built_context.and_then(|context| context.context_snapshot),
    }))
}

async fn build_project_session_context_response(
    state: &Arc<AppState>,
    project: &agentdash_domain::project::Project,
    session_id: &str,
    binding_label: &str,
) -> Option<BuiltProjectSessionContextResponse> {
    let agent_key = binding_label
        .trim()
        .strip_prefix("project_agent:")
        .unwrap_or_default();
    let project_agent = resolve_project_agent_bridge(project, agent_key)?;
    let workspace = resolve_project_workspace(state, project)
        .await
        .ok()
        .flatten();
    let session_meta = state
        .services
        .executor_hub
        .get_session_meta(session_id)
        .await
        .ok()??;
    let resolved_config = session_meta
        .executor_config
        .as_ref()
        .or(Some(&project_agent.executor_config));
    let effective_agent_type =
        resolved_config.and_then(|config| normalize_optional_string(Some(config.executor.clone())));
    let use_address_space = resolved_config.is_some_and(|config| config.is_native_agent());
    let address_space = if use_address_space {
        state
            .services
            .address_space_service
            .build_project_address_space(
                project,
                workspace.as_ref(),
                effective_agent_type.as_deref(),
            )
            .ok()
    } else {
        None
    };
    let mcp_servers = state
        .config
        .mcp_base_url
        .as_ref()
        .map(|base_url| {
            vec![McpInjectionConfig::for_relay(base_url.clone(), project.id).to_acp_mcp_server()]
        })
        .unwrap_or_default();
    let tool_visibility = summarize_tool_visibility(address_space.as_ref(), &mcp_servers);
    let runtime_policy = summarize_runtime_policy(
        workspace.is_some(),
        address_space.as_ref(),
        &mcp_servers,
        &tool_visibility.tool_names,
    );
    let workflow_runtime = resolve_workflow_runtime_injection(
        state,
        WorkflowRuntimeContext {
            target_kind: agentdash_domain::workflow::WorkflowTargetKind::Project,
            target_id: project.id,
            project,
            story: None,
            task: None,
            workspace: workspace.as_ref(),
        },
    )
    .await
    .map(|item| item.snapshot);
    let executor_source = if session_meta.executor_config.is_some() {
        "session.meta.executor_config".to_string()
    } else {
        project_agent.source.clone()
    };

    Some(BuiltProjectSessionContextResponse {
        address_space,
        context_snapshot: Some(ProjectSessionContextSnapshot {
            agent_key: project_agent.key,
            agent_display_name: project_agent.display_name,
            executor: build_session_executor_summary(
                resolved_config,
                project_agent.preset_name,
                executor_source,
                None,
            ),
            project_defaults: SessionProjectDefaults {
                default_agent_type: normalize_optional_string(
                    project.config.default_agent_type.clone(),
                ),
                context_containers: project.config.context_containers.clone(),
                mount_policy: project.config.mount_policy.clone(),
                session_composition: project.config.session_composition.clone(),
            },
            effective: SessionEffectiveContext {
                mount_policy: project.config.mount_policy.clone(),
                session_composition: project.config.session_composition.clone(),
                tool_visibility,
                runtime_policy,
            },
            shared_context_mounts: resolved_config
                .map(|config| build_project_agent_visible_mounts(project, config))
                .unwrap_or_default(),
            workflow_runtime,
        }),
    })
}
