#![allow(clippy::items_after_test_module)]

use std::sync::Arc;

use agentdash_application_agentrun::agent_run::{
    AgentRunAcceptedProductResultKind, AgentRunMessageProductResultProjector,
    ConversationEffectiveExecutorConfigModel, ConversationModelConfigResolver,
    ConversationModelConfigSourceModel, FnAgentRunMessageProductResultProjector,
    ResolvedProjectAgentContext, build_project_agent_context,
};
use agentdash_application_ports::launch::{BackendSelectionInput, BackendSelectionInputMode};
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
    AgentRunAcceptedRefs, AgentRunCommandReceipt, AgentRunMessageCommandOutcome,
    AgentRunMessageCommandResponse, BackendSelectionModeDto, BackendSelectionRequestDto,
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
use agentdash_domain::workflow::SubjectRef;

use crate::{
    app_state::AppState,
    auth::{CurrentUser, ProjectPermission, load_project_with_permission},
    rpc::ApiError,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_run_contract_accepts_executor_config() {
        let result = serde_json::from_value::<CreateProjectAgentRunRequest>(serde_json::json!({
            "input": [],
            "client_command_id": "cmd-1",
            "executor_config": {
                "executor": "CODEX"
            }
        }));

        let request = result.expect("executor_config remains accepted");
        assert_eq!(
            request.executor_config,
            Some(serde_json::json!({ "executor": "CODEX" }))
        );
    }

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
    let executor_config_override = req
        .executor_config
        .clone()
        .map(serde_json::from_value::<agentdash_spi::AgentConfig>)
        .transpose()
        .map_err(|error| ApiError::BadRequest(format!("executor_config 非法: {error}")))?;
    let backend_selection = req
        .backend_selection
        .as_ref()
        .map(backend_selection_input)
        .transpose()?;
    let subject_ref = req
        .subject_ref
        .map(|subject| {
            let id = Uuid::parse_str(&subject.id)
                .map_err(|_| ApiError::BadRequest("subject_ref.id 无效".to_string()))?;
            Ok::<SubjectRef, ApiError>(SubjectRef::new(subject.kind, id))
        })
        .transpose()?;
    let runtime_mailbox = super::lifecycle_agents::runtime_agent_run_mailbox(state.as_ref());
    let profile_state = state.clone();
    let execution_profiles: Arc<
        dyn agentdash_application_agentrun::agent_run::ProjectAgentExecutionProfilePolicy,
    > = Arc::new(move |profile_id: &str| {
        crate::routes::execution_profiles::is_known_execution_profile(
            profile_state.as_ref(),
            profile_id,
        )
    });
    let projection_project = project.clone();
    let projection: Arc<
        dyn agentdash_application_agentrun::agent_run::ProjectAgentRunStartProductProjectionPort,
    > = Arc::new(
        move |context: agentdash_application_agentrun::agent_run::ProjectAgentRunStartProjectionContext| {
            Ok(project_agent_run_start_result_projector(
                project_agent_run_start_base_result(&projection_project, context),
            ))
        },
    );
    let service = agentdash_application_agentrun::agent_run::ProjectAgentRunStartService::new(
        agentdash_application_agentrun::agent_run::ProjectAgentRunStartDeps {
            project_agents: state.repos.project_agent_repo.clone(),
            lifecycle_runs: state.repos.lifecycle_run_repo.clone(),
            frames: state.repos.agent_frame_repo.clone(),
            lifecycle_launch: state.repos.project_agent_lifecycle_launch.clone(),
            receipts: Arc::new(
                agentdash_application_agentrun::agent_run::AgentRunMessageReservationService::new(
                    state.repos.agent_run_message_submission_store.clone(),
                ),
            ),
            initial_submission: Arc::new(
                agentdash_application_agentrun::agent_run::ProjectAgentInitialMessageSubmissionService::new(
                    state.repos.agent_run_message_submission_store.clone(),
                    runtime_mailbox,
                ),
            ),
            execution_profiles,
            projection,
        },
    );
    let submission = service
        .start_run(
            agentdash_application_agentrun::agent_run::ProjectAgentRunStartCommand {
                project_id,
                project_agent_id,
                input: req.input,
                client_command_id: req.client_command_id,
                executor_config: executor_config_override,
                backend_selection,
                subject_ref,
                identity: Some(current_user),
            },
        )
        .await?;
    let mut response: ProjectAgentRunStartResult =
        serde_json::from_value(submission.result_json)
            .map_err(|error| ApiError::Internal(error.to_string()))?;
    response.command_receipt.duplicate = submission.duplicate;
    response.initial_message.command_receipt.duplicate = submission.duplicate;
    response.command_receipt.message = submission.error_message.clone();
    response.initial_message.command_receipt.message = submission.error_message;
    Ok(Json(response))
}

pub(crate) fn backend_selection_input(
    selection: &BackendSelectionRequestDto,
) -> Result<BackendSelectionInput, ApiError> {
    let mode = match selection.mode {
        BackendSelectionModeDto::Explicit => BackendSelectionInputMode::Explicit,
        BackendSelectionModeDto::AutoIdle => BackendSelectionInputMode::AutoIdle,
        BackendSelectionModeDto::WorkspaceBinding => BackendSelectionInputMode::WorkspaceBinding,
    };
    let backend_id = selection
        .backend_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    if mode == BackendSelectionInputMode::Explicit && backend_id.is_none() {
        return Err(ApiError::BadRequest(
            "explicit backend selection requires backend_id".to_string(),
        ));
    }
    Ok(BackendSelectionInput { mode, backend_id })
}

fn project_agent_run_start_base_result(
    project: &Project,
    context: agentdash_application_agentrun::agent_run::ProjectAgentRunStartProjectionContext,
) -> ProjectAgentRunStartResult {
    let runtime_refs = &context.dispatch.runtime_refs;
    let receipt = AgentRunCommandReceipt {
        client_command_id: context.client_command_id,
        status: "accepted".to_string(),
        duplicate: false,
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
        revision: Some(context.frame_revision),
    };
    ProjectAgentRunStartResult {
        command_receipt: receipt.clone(),
        accepted_refs: AgentRunAcceptedRefs {
            run_ref: run_ref.clone(),
            agent_ref: agent_ref.clone(),
            frame_ref: Some(frame_ref.clone()),
            turn_id: None,
        },
        initial_message: AgentRunMessageCommandResponse {
            command_receipt: receipt,
            outcome: AgentRunMessageCommandOutcome::Launched,
            mailbox_message: None,
            accepted_refs: None,
            fork: None,
        },
        effective_executor_config: Some(conversation_effective_executor_config_to_contract(
            context.effective_executor_config,
        )),
        agent: build_project_agent_summary(project, &context.project_agent_context),
        run_ref,
        agent_ref,
        frame_ref,
        subject_ref: Some(SubjectRefDto {
            kind: context.subject_ref.kind,
            id: context.subject_ref.id.to_string(),
        }),
    }
}

fn project_agent_run_start_result_projector(
    base: ProjectAgentRunStartResult,
) -> Arc<dyn AgentRunMessageProductResultProjector> {
    let accepted_base = base.clone();
    let accepted = Arc::new(move |kind: AgentRunAcceptedProductResultKind| {
        let mut result = accepted_base.clone();
        result.command_receipt.status = "accepted".to_string();
        result.initial_message.command_receipt.status = "accepted".to_string();
        result.initial_message.outcome = match kind {
            AgentRunAcceptedProductResultKind::Started => AgentRunMessageCommandOutcome::Launched,
            AgentRunAcceptedProductResultKind::Steered => AgentRunMessageCommandOutcome::Steered,
        };
        result.initial_message.mailbox_message = None;
        serde_json::to_value(result).map_err(|error| {
            agentdash_application_agentrun::WorkflowApplicationError::Internal(error.to_string())
        })
    });

    let queued_base = base.clone();
    let queued = Arc::new(
        move |message: &agentdash_domain::agent_run_mailbox::AgentRunMailboxMessage| {
            let mut result = queued_base.clone();
            result.command_receipt.status = "queued".to_string();
            result.initial_message.command_receipt.status = "queued".to_string();
            result.initial_message.outcome = AgentRunMessageCommandOutcome::Queued;
            result.initial_message.mailbox_message = Some(
                super::lifecycle_agents::mailbox_message_contract(message.clone()),
            );
            serde_json::to_value(result).map_err(|error| {
                agentdash_application_agentrun::WorkflowApplicationError::Internal(
                    error.to_string(),
                )
            })
        },
    );

    let failed = Arc::new(move || {
        let mut result = base.clone();
        result.command_receipt.status = "failed".to_string();
        result.initial_message.command_receipt.status = "failed".to_string();
        result.initial_message.outcome = AgentRunMessageCommandOutcome::Failed;
        result.initial_message.mailbox_message = None;
        serde_json::to_value(result).map_err(|error| {
            agentdash_application_agentrun::WorkflowApplicationError::Internal(error.to_string())
        })
    });

    Arc::new(FnAgentRunMessageProductResultProjector::new(
        accepted, queued, failed,
    ))
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
