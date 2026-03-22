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
        build_project_agent_visible_mounts, resolve_project_agent_bridge, resolve_project_workspace,
    },
    rpc::ApiError,
    session_context::{
        self, ExecutorSummaryInput, SessionContextInput, SessionContextSnapshot,
        SessionOwnerVariant, SharedContextMount,
    },
};
use agentdash_domain::session_binding::SessionOwnerType;
use agentdash_mcp::injection::McpInjectionConfig;

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
    pub context_snapshot: Option<SessionContextSnapshot>,
}

#[derive(Debug)]
struct BuiltProjectSessionContextResponse {
    address_space: Option<agentdash_executor::ExecutionAddressSpace>,
    context_snapshot: Option<SessionContextSnapshot>,
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
    let workspace = resolve_project_workspace(state, project).await.ok().flatten();
    let session_meta = state.services.executor_hub.get_session_meta(session_id).await.ok()??;

    let resolved_config = session_meta
        .executor_config
        .as_ref()
        .or(Some(&project_agent.executor_config));
    let use_address_space = resolved_config.is_some_and(|c| c.is_native_agent());
    let address_space = if use_address_space {
        state.services.address_space_service
            .build_project_address_space(project, workspace.as_ref(), resolved_config.map(|c| c.executor.as_str()))
            .ok()
    } else {
        None
    };
    let mcp_servers = state.config.mcp_base_url.as_ref()
        .map(|base_url| vec![McpInjectionConfig::for_relay(base_url.clone(), project.id).to_acp_mcp_server()])
        .unwrap_or_default();

    let executor_source = if session_meta.executor_config.is_some() {
        "session.meta.executor_config".to_string()
    } else {
        project_agent.source.clone()
    };

    let shared_mounts: Vec<SharedContextMount> = resolved_config
        .map(|config| {
            build_project_agent_visible_mounts(project, config)
                .into_iter()
                .map(|m| SharedContextMount {
                    container_id: m.container_id,
                    mount_id: m.mount_id,
                    display_name: m.display_name,
                    writable: m.writable,
                })
                .collect()
        })
        .unwrap_or_default();

    let snapshot = session_context::build_session_context(SessionContextInput {
        project,
        story: None,
        workspace_attached: workspace.is_some(),
        resolved_config,
        address_space: address_space.as_ref(),
        mcp_servers: &mcp_servers,
        executor_summary: ExecutorSummaryInput {
            preset_name: project_agent.preset_name,
            source: executor_source,
            resolution_error: None,
        },
        owner_variant: SessionOwnerVariant::Project {
            agent_key: project_agent.key,
            agent_display_name: project_agent.display_name,
            shared_context_mounts: shared_mounts,
        },
    });

    Some(BuiltProjectSessionContextResponse {
        address_space,
        context_snapshot: Some(snapshot),
    })
}
