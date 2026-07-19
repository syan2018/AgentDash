use std::sync::Arc;

use agentdash_agent_runtime_contract::{ManagedRuntimeGatewayError, RuntimeChangeSequence};
use agentdash_application_agentrun::agent_run::{
    AgentRunProductCommand, AgentRunProductCommandError, AgentRunProductCommandRequest,
    AgentRunProductDeleteError, AgentRunProductDeleteRequest, AgentRunProductDeleteService,
    AgentRunProductProjectionError, AgentRunProductRuntimeRecoveryError,
    AgentRunProductRuntimeRecoveryRequest, AgentRunTerminalChangeSequence,
};
use agentdash_contracts::agent_run_product_projection as product_projection_contract;
use agentdash_domain::agent_run_target::AgentRunTarget;
use agentdash_workspace_module::workspace_module::presentation_protocol::{
    WorkspaceModulePresentationAcknowledgeRequest, WorkspaceModulePresentationChangeSequence,
    WorkspaceModulePresentationIntentId,
};
use axum::{
    Json,
    extract::{Path, Query, State},
};
use serde::Deserialize;
use uuid::Uuid;

use crate::{
    app_state::AppState,
    auth::{CurrentUser, ProjectPermission, load_project_with_permission},
    rpc::ApiError,
};

const CANONICAL_RUNTIME_CHANGE_PAGE_LIMIT: u32 = 256;
const DEFAULT_PRODUCT_CHANGE_PAGE_LIMIT: usize = 256;
const MAX_PRODUCT_CHANGE_PAGE_LIMIT: usize = 256;

#[derive(Debug, Deserialize)]
pub struct ManagedRuntimeChangesQuery {
    pub after: Option<u64>,
    pub limit: Option<u32>,
}

#[derive(Debug, Deserialize)]
pub struct ProductProjectionChangesQuery {
    pub after: Option<u64>,
    pub limit: Option<usize>,
}

pub fn router() -> axum::Router<Arc<AppState>> {
    axum::Router::new()
        .route(
            "/agent-runs/{run_id}/agents/{agent_id}/runtime/snapshot",
            axum::routing::get(get_managed_runtime_snapshot),
        )
        .route(
            "/agent-runs/{run_id}/agents/{agent_id}/runtime/changes",
            axum::routing::get(get_managed_runtime_changes),
        )
        .route(
            "/agent-runs/{run_id}/agents/{agent_id}/runtime/commands",
            axum::routing::post(execute_managed_runtime_command),
        )
        .route(
            "/projects/{project_id}/agent-runs/{run_id}",
            axum::routing::delete(delete_project_agent_run),
        )
        .route(
            "/agent-runs/{run_id}/agents/{agent_id}/workspace",
            axum::routing::get(get_agent_run_workspace),
        )
        .route(
            "/agent-runs/{run_id}/agents/{agent_id}/workspace-presentations/snapshot",
            axum::routing::get(get_workspace_presentation_snapshot),
        )
        .route(
            "/agent-runs/{run_id}/agents/{agent_id}/workspace-presentations/changes",
            axum::routing::get(get_workspace_presentation_changes),
        )
        .route(
            "/agent-runs/{run_id}/agents/{agent_id}/workspace-presentations/{intent_id}/ack",
            axum::routing::post(acknowledge_workspace_presentation),
        )
        .route(
            "/agent-runs/{run_id}/agents/{agent_id}/runtime/terminals/snapshot",
            axum::routing::get(get_agent_run_terminal_snapshot),
        )
        .route(
            "/agent-runs/{run_id}/agents/{agent_id}/runtime/terminals/changes",
            axum::routing::get(get_agent_run_terminal_changes),
        )
}

async fn delete_project_agent_run(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path((project_id, run_id)): Path<(String, String)>,
) -> Result<Json<agentdash_contracts::workflow::DeleteAgentRunResponse>, ApiError> {
    let project_id = parse_uuid(&project_id, "project_id")?;
    let run_id = parse_uuid(&run_id, "run_id")?;
    load_project_with_permission(
        state.as_ref(),
        &current_user,
        project_id,
        ProjectPermission::Configure,
    )
    .await?;
    let outcome = AgentRunProductDeleteService::new(
        state.repos.lifecycle_run_repo.clone(),
        state.repos.lifecycle_agent_repo.clone(),
        state
            .services
            .agent_run_product_projection_composition
            .gateway
            .clone(),
        state.services.agent_run_product_commands.clone(),
    )
    .delete(AgentRunProductDeleteRequest { project_id, run_id })
    .await
    .map_err(agent_run_product_delete_error)?;
    Ok(Json(
        agentdash_contracts::workflow::DeleteAgentRunResponse {
            deleted: outcome.deleted,
            project_id: outcome.project_id.to_string(),
            run_id: outcome.run_id.to_string(),
        },
    ))
}

fn agent_run_product_delete_error(error: AgentRunProductDeleteError) -> ApiError {
    match error {
        AgentRunProductDeleteError::ProjectMismatch => ApiError::NotFound(error.to_string()),
        AgentRunProductDeleteError::RuntimeNotClosed => ApiError::Conflict(error.to_string()),
        AgentRunProductDeleteError::Repository(_) | AgentRunProductDeleteError::Runtime(_) => {
            ApiError::Internal(error.to_string())
        }
    }
}

async fn get_agent_run_workspace(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path((run_id, agent_id)): Path<(String, String)>,
) -> Result<Json<agentdash_contracts::workflow::AgentRunWorkspaceView>, ApiError> {
    let target = authorize_agent_run_target(
        state.as_ref(),
        &current_user,
        &run_id,
        &agent_id,
        ProjectPermission::Use,
    )
    .await?;
    let run = state
        .repos
        .lifecycle_run_repo
        .get_by_id(target.run_id)
        .await?
        .ok_or_else(|| ApiError::NotFound("AgentRun 不存在".into()))?;
    let agent = state
        .repos
        .lifecycle_agent_repo
        .get(target.agent_id)
        .await?
        .ok_or_else(|| ApiError::NotFound("AgentRun Agent 不存在".into()))?;
    let (parent, children) =
        super::agent_run_workspace::resolve_lineage(state.as_ref(), &run, &agent).await?;
    let mut workspace =
        super::agent_run_workspace::load(state.as_ref(), run, agent, &current_user).await?;
    workspace.parent = parent;
    workspace.children = children;
    Ok(Json(workspace))
}

async fn get_managed_runtime_snapshot(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path((run_id, agent_id)): Path<(String, String)>,
) -> Result<Json<agentdash_agent_runtime_contract::ManagedRuntimeSnapshot>, ApiError> {
    let target = authorize_agent_run_target(
        state.as_ref(),
        &current_user,
        &run_id,
        &agent_id,
        ProjectPermission::Use,
    )
    .await?;
    state
        .services
        .agent_run_product_projection
        .runtime_snapshot(&target)
        .await
        .map(Json)
        .map_err(agent_run_product_projection_error)
}

async fn get_managed_runtime_changes(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path((run_id, agent_id)): Path<(String, String)>,
    Query(query): Query<ManagedRuntimeChangesQuery>,
) -> Result<Json<agentdash_agent_runtime_contract::ManagedRuntimeChangePage>, ApiError> {
    if query.limit.unwrap_or(CANONICAL_RUNTIME_CHANGE_PAGE_LIMIT)
        != CANONICAL_RUNTIME_CHANGE_PAGE_LIMIT
    {
        return Err(ApiError::BadRequest(
            "Managed Runtime change page limit 必须为 canonical 256".to_string(),
        ));
    }
    let target = authorize_agent_run_target(
        state.as_ref(),
        &current_user,
        &run_id,
        &agent_id,
        ProjectPermission::Use,
    )
    .await?;
    state
        .services
        .agent_run_product_projection
        .runtime_changes(&target, query.after.map(RuntimeChangeSequence))
        .await
        .map(Json)
        .map_err(agent_run_product_projection_error)
}

async fn execute_managed_runtime_command(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path((run_id, agent_id)): Path<(String, String)>,
    Json(body): Json<product_projection_contract::AgentRunProductRuntimeCommandRequest>,
) -> Result<Json<agentdash_agent_runtime_contract::ManagedRuntimeOperationReceipt>, ApiError> {
    let target = authorize_agent_run_target(
        state.as_ref(),
        &current_user,
        &run_id,
        &agent_id,
        ProjectPermission::Use,
    )
    .await?;
    if matches!(
        &body.command,
        product_projection_contract::AgentRunProductRuntimeCommand::Rebind
    ) {
        return state
            .services
            .agent_run_product_recovery
            .recover(AgentRunProductRuntimeRecoveryRequest {
                target,
                client_command_id: body.client_command_id,
                expected_revision: body.expected_revision,
            })
            .await
            .map(|outcome| Json(outcome.activate_receipt))
            .map_err(agent_run_product_recovery_error);
    }
    let command = match body.command {
        product_projection_contract::AgentRunProductRuntimeCommand::Resume => {
            AgentRunProductCommand::Resume
        }
        product_projection_contract::AgentRunProductRuntimeCommand::SubmitInput { content } => {
            AgentRunProductCommand::SubmitInput { content }
        }
        product_projection_contract::AgentRunProductRuntimeCommand::Interrupt => {
            AgentRunProductCommand::Interrupt
        }
        product_projection_contract::AgentRunProductRuntimeCommand::RequestCompaction => {
            AgentRunProductCommand::RequestCompaction
        }
        product_projection_contract::AgentRunProductRuntimeCommand::Rebind => unreachable!(),
        product_projection_contract::AgentRunProductRuntimeCommand::ResolveInteraction {
            interaction_id,
            response,
        } => AgentRunProductCommand::ResolveInteraction {
            interaction_id,
            response,
        },
        product_projection_contract::AgentRunProductRuntimeCommand::Close => {
            AgentRunProductCommand::Close
        }
    };
    state
        .services
        .agent_run_product_projection_composition
        .commands
        .execute(AgentRunProductCommandRequest {
            target,
            client_command_id: body.client_command_id,
            expected_revision: body.expected_revision,
            command,
        })
        .await
        .map(Json)
        .map_err(agent_run_product_command_error)
}

fn agent_run_product_recovery_error(error: AgentRunProductRuntimeRecoveryError) -> ApiError {
    match error {
        AgentRunProductRuntimeRecoveryError::InvalidRequest => {
            ApiError::BadRequest(error.to_string())
        }
        AgentRunProductRuntimeRecoveryError::BindingMissing
        | AgentRunProductRuntimeRecoveryError::RuntimeBindingMismatch
        | AgentRunProductRuntimeRecoveryError::Runtime(ManagedRuntimeGatewayError::Conflict {
            ..
        })
        | AgentRunProductRuntimeRecoveryError::Runtime(ManagedRuntimeGatewayError::Unavailable {
            ..
        }) => ApiError::Conflict(error.to_string()),
        AgentRunProductRuntimeRecoveryError::Binding(_)
        | AgentRunProductRuntimeRecoveryError::ResourceSurface(_)
        | AgentRunProductRuntimeRecoveryError::Runtime(ManagedRuntimeGatewayError::NotFound)
        | AgentRunProductRuntimeRecoveryError::Runtime(ManagedRuntimeGatewayError::Persistence {
            ..
        }) => ApiError::Internal(error.to_string()),
        AgentRunProductRuntimeRecoveryError::Runtime(ManagedRuntimeGatewayError::Invalid {
            ..
        }) => ApiError::BadRequest(error.to_string()),
    }
}

async fn get_workspace_presentation_snapshot(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path((run_id, agent_id)): Path<(String, String)>,
) -> Result<Json<product_projection_contract::WorkspaceModulePresentationSnapshot>, ApiError> {
    let target = authorize_agent_run_target(
        state.as_ref(),
        &current_user,
        &run_id,
        &agent_id,
        ProjectPermission::Use,
    )
    .await?;
    state
        .services
        .agent_run_product_projection
        .workspace_presentation_snapshot(&target)
        .await
        .map(product_projection_contract::WorkspaceModulePresentationSnapshot::from)
        .map(Json)
        .map_err(agent_run_product_projection_error)
}

async fn get_workspace_presentation_changes(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path((run_id, agent_id)): Path<(String, String)>,
    Query(query): Query<ProductProjectionChangesQuery>,
) -> Result<Json<product_projection_contract::WorkspaceModulePresentationChangePage>, ApiError> {
    let target = authorize_agent_run_target(
        state.as_ref(),
        &current_user,
        &run_id,
        &agent_id,
        ProjectPermission::Use,
    )
    .await?;
    state
        .services
        .agent_run_product_projection
        .workspace_presentation_changes(
            &target,
            query.after.map(WorkspaceModulePresentationChangeSequence),
            product_projection_limit(query.limit)?,
        )
        .await
        .map(product_projection_contract::WorkspaceModulePresentationChangePage::from)
        .map(Json)
        .map_err(agent_run_product_projection_error)
}

async fn acknowledge_workspace_presentation(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path((run_id, agent_id, intent_id)): Path<(String, String, String)>,
    Json(body): Json<product_projection_contract::WorkspaceModulePresentationAcknowledgeRequest>,
) -> Result<Json<product_projection_contract::WorkspaceModulePresentationChange>, ApiError> {
    let target = authorize_agent_run_target(
        state.as_ref(),
        &current_user,
        &run_id,
        &agent_id,
        ProjectPermission::Use,
    )
    .await?;
    state
        .services
        .agent_run_product_projection
        .acknowledge_workspace_presentation(WorkspaceModulePresentationAcknowledgeRequest {
            target,
            intent_id: WorkspaceModulePresentationIntentId::new(intent_id)
                .map_err(|error| ApiError::BadRequest(error.to_string()))?,
            observed_change_sequence: WorkspaceModulePresentationChangeSequence(
                body.observed_change_sequence,
            ),
        })
        .await
        .map(product_projection_contract::WorkspaceModulePresentationChange::from)
        .map(Json)
        .map_err(agent_run_product_projection_error)
}

async fn get_agent_run_terminal_snapshot(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path((run_id, agent_id)): Path<(String, String)>,
) -> Result<Json<product_projection_contract::AgentRunTerminalSnapshot>, ApiError> {
    let target = authorize_agent_run_target(
        state.as_ref(),
        &current_user,
        &run_id,
        &agent_id,
        ProjectPermission::Use,
    )
    .await?;
    state
        .services
        .agent_run_product_projection
        .terminal_snapshot(&target)
        .await
        .map(product_projection_contract::AgentRunTerminalSnapshot::from)
        .map(Json)
        .map_err(agent_run_product_projection_error)
}

async fn get_agent_run_terminal_changes(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path((run_id, agent_id)): Path<(String, String)>,
    Query(query): Query<ProductProjectionChangesQuery>,
) -> Result<Json<product_projection_contract::AgentRunTerminalChangePage>, ApiError> {
    let target = authorize_agent_run_target(
        state.as_ref(),
        &current_user,
        &run_id,
        &agent_id,
        ProjectPermission::Use,
    )
    .await?;
    state
        .services
        .agent_run_product_projection
        .terminal_changes(
            &target,
            query.after.map(AgentRunTerminalChangeSequence),
            product_projection_limit(query.limit)?,
        )
        .await
        .map(product_projection_contract::AgentRunTerminalChangePage::from)
        .map(Json)
        .map_err(agent_run_product_projection_error)
}

fn product_projection_limit(limit: Option<usize>) -> Result<usize, ApiError> {
    let limit = limit.unwrap_or(DEFAULT_PRODUCT_CHANGE_PAGE_LIMIT);
    if !(1..=MAX_PRODUCT_CHANGE_PAGE_LIMIT).contains(&limit) {
        return Err(ApiError::BadRequest(
            "Product projection change page limit 必须位于 1..=256".to_string(),
        ));
    }
    Ok(limit)
}

async fn authorize_agent_run_target(
    state: &AppState,
    current_user: &agentdash_integration_api::AuthIdentity,
    run_id: &str,
    agent_id: &str,
    permission: ProjectPermission,
) -> Result<AgentRunTarget, ApiError> {
    let run_id = parse_uuid(run_id, "run_id")?;
    let agent_id = parse_uuid(agent_id, "agent_id")?;
    let run = state
        .repos
        .lifecycle_run_repo
        .get_by_id(run_id)
        .await
        .map_err(ApiError::from)?
        .ok_or_else(|| ApiError::NotFound(format!("LifecycleRun 不存在: {run_id}")))?;
    load_project_with_permission(state, current_user, run.project_id, permission).await?;
    let agent = state
        .repos
        .lifecycle_agent_repo
        .get(agent_id)
        .await
        .map_err(ApiError::from)?
        .ok_or_else(|| ApiError::NotFound(format!("LifecycleAgent 不存在: {agent_id}")))?;
    if agent.run_id != run.id || agent.project_id != run.project_id {
        return Err(ApiError::Conflict(format!(
            "LifecycleAgent {agent_id} 不属于 LifecycleRun {run_id}"
        )));
    }
    Ok(AgentRunTarget { run_id, agent_id })
}

fn agent_run_product_projection_error(error: AgentRunProductProjectionError) -> ApiError {
    match error {
        AgentRunProductProjectionError::TargetNotBound => {
            ApiError::Conflict("AgentRun 尚未建立 committed Runtime binding".to_string())
        }
        AgentRunProductProjectionError::Binding(message)
        | AgentRunProductProjectionError::Runtime(message)
        | AgentRunProductProjectionError::Workspace(message)
        | AgentRunProductProjectionError::Terminal(message) => ApiError::Internal(message),
        AgentRunProductProjectionError::RuntimeThreadMismatch
        | AgentRunProductProjectionError::RuntimeSourceBindingMismatch
        | AgentRunProductProjectionError::TargetMismatch => ApiError::Internal(error.to_string()),
    }
}

fn agent_run_product_command_error(error: AgentRunProductCommandError) -> ApiError {
    match error {
        AgentRunProductCommandError::TargetNotBound
        | AgentRunProductCommandError::TargetMismatch
        | AgentRunProductCommandError::RuntimeBindingMismatch
        | AgentRunProductCommandError::ClientCommandConflict
        | AgentRunProductCommandError::CommandUnavailable { .. }
        | AgentRunProductCommandError::StaleAvailabilityEvidence { .. }
        | AgentRunProductCommandError::ActiveTurnMissing
        | AgentRunProductCommandError::Runtime(ManagedRuntimeGatewayError::Conflict { .. })
        | AgentRunProductCommandError::Runtime(ManagedRuntimeGatewayError::Unavailable {
            ..
        }) => ApiError::Conflict(error.to_string()),
        AgentRunProductCommandError::InvalidClientCommandId
        | AgentRunProductCommandError::Runtime(ManagedRuntimeGatewayError::Invalid { .. }) => {
            ApiError::BadRequest(error.to_string())
        }
        AgentRunProductCommandError::Binding(_)
        | AgentRunProductCommandError::ClaimPersistence { .. }
        | AgentRunProductCommandError::Runtime(ManagedRuntimeGatewayError::NotFound)
        | AgentRunProductCommandError::Runtime(ManagedRuntimeGatewayError::Persistence {
            ..
        }) => ApiError::Internal(error.to_string()),
    }
}

fn parse_uuid(raw: &str, field: &str) -> Result<Uuid, ApiError> {
    Uuid::parse_str(raw).map_err(|_| ApiError::BadRequest(format!("无效的 {field}: {raw}")))
}
