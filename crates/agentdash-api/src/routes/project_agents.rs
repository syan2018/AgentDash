#![allow(clippy::items_after_test_module)]

use std::sync::Arc;

use agentdash_agent_runtime_contract::RuntimeActor;
use agentdash_application_agentrun::agent_run::{
    ConversationEffectiveExecutorConfigModel, ConversationModelConfigResolver,
    ConversationModelConfigSourceModel, DeliverAgentRunProductInput, ResolvedProjectAgentContext,
    build_project_agent_context,
};
use agentdash_domain::{
    agent::ProjectAgent, common::AgentPresetConfig, inline_file::InlineFileOwnerKind,
    project::Project,
};
use axum::{
    Json,
    extract::{Path, State},
};
use uuid::Uuid;

use agentdash_contracts::agent_run_mailbox::{
    AgentRunAcceptedRefs, AgentRunCommandReceipt, AgentRunMessageAcceptedRefs,
    AgentRunMessageCommandOutcome, AgentRunMessageCommandResponse,
};
use agentdash_contracts::common_response::DeletedFlagResponse;
use agentdash_contracts::project_agent::{
    CreateProjectAgentRequest, CreateProjectAgentRunRequest, ProjectAgent as ProjectAgentResponse,
    ProjectAgentExecutor, ProjectAgentRunStartResult, ProjectAgentSummary, ThinkingLevel,
    UpdateProjectAgentRequest,
};
use agentdash_contracts::workflow::{
    AgentFrameRefDto, AgentRunRefDto, ConversationEffectiveExecutorConfigView,
    ConversationModelConfigSource, LifecycleRunRefDto, SubjectRefDto,
};
use agentdash_domain::workflow::{
    AgentLaunchIntent, AgentPolicy, CapabilityPolicy, ContextPolicy, ExecutionSource, RunPolicy,
    RuntimePolicy, SubjectRef,
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
            effective_executor_config: None,
            preset_name: Some("preset".to_string()),
            source: "project.config.default_agent_type".to_string(),
        })
        .expect("serialize project agent summary");

        assert!(value.get("display_name").is_some());
        assert!(value.get("preset_name").is_some());
        assert!(value.get("displayName").is_none());
        assert!(value.get("presetName").is_none());
    }

    #[test]
    fn normalize_project_agent_config_converts_legacy_mcp_preset_keys() {
        let value = normalize_project_agent_config(serde_json::json!({
            "mcp_preset_keys": ["abc-config"],
            "capability_directives": [
                { "remove": "mcp:abc-config::ABCConfigAnalyzer_get_file_content" }
            ]
        }))
        .expect("normalize config");

        assert!(value.get("mcp_preset_keys").is_none());
        assert_eq!(
            value["capability_directives"],
            serde_json::json!([
                { "add": "mcp:abc-config" },
                { "remove": "mcp:abc-config::ABCConfigAnalyzer_get_file_content" }
            ])
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
            "/projects/{id}/agents/{project_agent_id}/agent-runs",
            axum::routing::post(create_project_agent_run),
        )
}

pub async fn create_project_agent_run(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path((project_id, project_agent_id)): Path<(String, String)>,
    Json(req): Json<CreateProjectAgentRunRequest>,
) -> Result<Json<ProjectAgentRunStartResult>, ApiError> {
    let project_id = parse_project_id(&project_id)?;
    let project = load_project_with_permission(
        state.as_ref(),
        &current_user,
        project_id,
        ProjectPermission::Use,
    )
    .await?;
    let project_agent_id = parse_project_agent_id(&project_agent_id)?;
    if req.client_command_id.trim().is_empty() {
        return Err(ApiError::BadRequest(
            "client_command_id 不能为空".to_string(),
        ));
    }
    if req.executor_config.is_some() || req.backend_selection.is_some() {
        return Err(ApiError::BadRequest(
            "当前 Runtime surface 不接受单次启动 executor/backend override；请更新 Project Agent 配置"
                .to_string(),
        ));
    }
    let project_agent = state
        .repos
        .project_agent_repo
        .get_by_project_and_id(project_id, project_agent_id)
        .await
        .map_err(ApiError::from)?
        .ok_or_else(|| ApiError::NotFound(format!("Project Agent {project_agent_id} 不存在")))?;
    let subject_ref = req
        .subject_ref
        .map(|subject| {
            let id = Uuid::parse_str(&subject.id)
                .map_err(|_| ApiError::BadRequest("subject_ref.id 无效".to_string()))?;
            Ok::<SubjectRef, ApiError>(SubjectRef::new(subject.kind, id))
        })
        .transpose()?;
    let dispatch = state
        .repos
        .project_agent_lifecycle_launch
        .launch_project_agent(&AgentLaunchIntent {
            project_id,
            source: ExecutionSource::ProjectAgent,
            created_by_user_id: Some(current_user.user_id.clone()),
            subject_ref: subject_ref.clone(),
            parent_run_id: None,
            parent_agent_id: None,
            project_agent_id: Some(project_agent_id),
            workflow_graph_ref: None,
            run_policy: RunPolicy::CreateLinkedRun,
            agent_policy: AgentPolicy::Create,
            context_policy: ContextPolicy::Isolated,
            capability_policy: CapabilityPolicy::Baseline,
            runtime_policy: RuntimePolicy::ProvisionRuntimeThread,
        })
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?;
    let runtime_refs = dispatch.runtime_refs;
    let input = super::lifecycle_agents::runtime_input_from_codex(req.input)?;
    let delivery = state
        .services
        .agent_run_product_delivery
        .deliver(DeliverAgentRunProductInput {
            run_id: runtime_refs.run_ref,
            agent_id: runtime_refs.agent_ref,
            input,
            actor: RuntimeActor::User {
                subject: current_user.user_id.clone(),
            },
            client_command_id: req.client_command_id.clone(),
        })
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?;
    let frame = state
        .repos
        .agent_frame_repo
        .get(runtime_refs.frame_ref)
        .await
        .map_err(ApiError::from)?
        .ok_or_else(|| ApiError::Internal("Lifecycle launch 未产出 AgentFrame".to_string()))?;
    let binding = state
        .repos
        .agent_run_runtime_binding_repo
        .load(
            &agentdash_application_ports::agent_run_runtime::AgentRunRuntimeTarget {
                run_id: runtime_refs.run_ref,
                agent_id: runtime_refs.agent_ref,
            },
        )
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?;
    let operation_id = delivery
        .operation_receipt
        .as_ref()
        .map(|receipt| receipt.operation_id.to_string());
    let runtime_thread_id = binding
        .as_ref()
        .map(|binding| binding.thread_id.to_string());
    let receipt = AgentRunCommandReceipt {
        client_command_id: req.client_command_id,
        status: if delivery.queued {
            "queued"
        } else {
            "accepted"
        }
        .to_string(),
        duplicate: delivery
            .operation_receipt
            .as_ref()
            .is_some_and(|receipt| receipt.duplicate),
        accepted_runtime_operation_id: operation_id.clone(),
        message: None,
    };
    let run_ref = LifecycleRunRefDto {
        run_id: runtime_refs.run_ref.to_string(),
    };
    let agent_ref = AgentRunRefDto {
        run_id: runtime_refs.run_ref.to_string(),
        agent_id: runtime_refs.agent_ref.to_string(),
    };
    let frame_ref = AgentFrameRefDto {
        agent_id: runtime_refs.agent_ref.to_string(),
        frame_id: runtime_refs.frame_ref.to_string(),
        revision: Some(frame.revision),
    };
    let accepted_refs = AgentRunAcceptedRefs {
        run_ref: run_ref.clone(),
        agent_ref: agent_ref.clone(),
        frame_ref: Some(frame_ref.clone()),
        runtime_thread_id: runtime_thread_id.clone(),
        runtime_operation_id: operation_id.clone(),
    };
    let initial_message = AgentRunMessageCommandResponse {
        command_receipt: receipt.clone(),
        outcome: if delivery.queued {
            AgentRunMessageCommandOutcome::Queued
        } else {
            AgentRunMessageCommandOutcome::Dispatched
        },
        mailbox_message: None,
        accepted_refs: Some(AgentRunMessageAcceptedRefs {
            run_ref: run_ref.clone(),
            agent_ref: agent_ref.clone(),
            frame_ref: Some(frame_ref.clone()),
            runtime_thread_id,
            runtime_operation_id: operation_id,
        }),
    };
    let context = build_project_agent_context(&project_agent)
        .await
        .map_err(ApiError::Internal)?;
    Ok(Json(ProjectAgentRunStartResult {
        command_receipt: receipt,
        accepted_refs,
        initial_message,
        effective_executor_config: Some(conversation_effective_executor_config_to_contract(
            ConversationModelConfigResolver::view_for_config(
                &context.executor_config,
                ConversationModelConfigSourceModel::ProjectAgentPreset,
            ),
        )),
        agent: build_project_agent_summary(&project, &context),
        run_ref,
        agent_ref,
        frame_ref,
        subject_ref: subject_ref.map(|subject| SubjectRefDto {
            kind: subject.kind,
            id: subject.id.to_string(),
        }),
    }))
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
        ProjectPermission::Use,
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
        let bridge = build_project_agent_context(agent)
            .await
            .map_err(ApiError::Internal)?;
        response.push(build_project_agent_summary(&project, &bridge));
    }

    response.sort_by(|a, b| a.display_name.cmp(&b.display_name));
    Ok(Json(response))
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
        effective_executor_config: Some(conversation_effective_executor_config_to_contract(
            ConversationModelConfigResolver::view_for_config(
                &agent.executor_config,
                ConversationModelConfigSourceModel::ProjectAgentPreset,
            ),
        )),
        preset_name: agent.preset_name.clone(),
        source: agent.source.clone(),
    }
}

fn conversation_effective_executor_config_to_contract(
    config: ConversationEffectiveExecutorConfigModel,
) -> ConversationEffectiveExecutorConfigView {
    ConversationEffectiveExecutorConfigView {
        executor: config.executor,
        provider_id: config.provider_id,
        model_id: config.model_id,
        agent_id: config.agent_id,
        thinking_level: config.thinking_level,
        permission_policy: config.permission_policy,
        source: match config.source {
            ConversationModelConfigSourceModel::ProjectAgentPreset => {
                ConversationModelConfigSource::ProjectAgentPreset
            }
            ConversationModelConfigSourceModel::FrameExecutionProfile => {
                ConversationModelConfigSource::FrameExecutionProfile
            }
            ConversationModelConfigSourceModel::UserOverride => {
                ConversationModelConfigSource::UserOverride
            }
            ConversationModelConfigSourceModel::ExecutorDiscoveryDefault => {
                ConversationModelConfigSource::ExecutorDiscoveryDefault
            }
            ConversationModelConfigSourceModel::Unspecified => {
                ConversationModelConfigSource::Unspecified
            }
        },
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

// ─── Project Agent API ───

fn build_project_agent_response(agent: &ProjectAgent) -> Result<ProjectAgentResponse, ApiError> {
    let config = AgentPresetConfig::normalize_json_value(&agent.config).map_err(ApiError::from)?;
    Ok(ProjectAgentResponse {
        id: agent.id.to_string(),
        project_id: agent.project_id.to_string(),
        name: agent.name.clone(),
        agent_type: agent.agent_type.clone(),
        config,
        default_lifecycle_key: agent.default_lifecycle_key.clone(),
        knowledge_enabled: agent.knowledge_enabled,
        created_at: agent.created_at.to_rfc3339(),
        updated_at: agent.updated_at.to_rfc3339(),
    })
}

fn normalize_project_agent_config(
    config: serde_json::Value,
) -> Result<serde_json::Value, ApiError> {
    AgentPresetConfig::normalize_json_value(&config).map_err(ApiError::from)
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
        ProjectPermission::Use,
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
        ProjectPermission::Configure,
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
    if !crate::routes::execution_profiles::is_known_execution_profile(&state, &agent_type) {
        return Err(ApiError::BadRequest(format!(
            "未知 execution profile: {agent_type}"
        )));
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
        agent.config = normalize_project_agent_config(config)?;
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
        ProjectPermission::Configure,
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
        if !crate::routes::execution_profiles::is_known_execution_profile(&state, &trimmed) {
            return Err(ApiError::BadRequest(format!(
                "未知 execution profile: {trimmed}"
            )));
        }
        agent.agent_type = trimmed;
    }
    if let Some(config) = req.config {
        agent.config = normalize_project_agent_config(config)?;
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
        ProjectPermission::Configure,
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
