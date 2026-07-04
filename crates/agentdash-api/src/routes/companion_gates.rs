use std::sync::Arc;

use axum::{
    Json,
    extract::{Path, State},
};
use uuid::Uuid;

use agentdash_application::{
    companion::{
        AgentRunCompanionMailboxDelivery, CompanionGateControlService, RespondCompanionGateCommand,
    },
    runtime_session_agent_run_bridge::{
        agent_run_session_control, agent_run_session_core, agent_run_session_eventing,
        agent_run_session_launch,
    },
};
use agentdash_contracts::companion::{CompanionGateRespondRequest, CompanionGateRespondResponse};

use crate::{
    app_state::AppState,
    auth::{CurrentUser, ProjectPermission, load_project_with_permission},
    rpc::ApiError,
};

pub fn router() -> axum::Router<Arc<AppState>> {
    axum::Router::new().route(
        "/companion-gates/{gate_id}/respond",
        axum::routing::post(respond_companion_gate),
    )
}

pub async fn respond_companion_gate(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(gate_id): Path<String>,
    Json(req): Json<CompanionGateRespondRequest>,
) -> Result<Json<CompanionGateRespondResponse>, ApiError> {
    let gate_uuid: Uuid = gate_id
        .parse()
        .map_err(|_| ApiError::BadRequest(format!("invalid gate_id: {gate_id}")))?;

    ensure_companion_gate_permission(
        state.as_ref(),
        &current_user,
        gate_uuid,
        ProjectPermission::Use,
    )
    .await?;

    let service = CompanionGateControlService::with_session_eventing(
        state.repos.lifecycle_gate_repo.clone(),
        state.repos.lifecycle_run_repo.clone(),
        state.repos.agent_frame_repo.clone(),
        state.repos.lifecycle_agent_repo.clone(),
        state.repos.execution_anchor_repo.clone(),
        state.repos.agent_run_delivery_binding_repo.clone(),
        state.repos.agent_lineage_repo.clone(),
        state.services.session_eventing.clone(),
    )
    .with_human_response_mailbox_delivery(Arc::new(
        AgentRunCompanionMailboxDelivery::from_runtime_services(
            state.repos.clone(),
            agent_run_session_core(state.services.session_core.clone()),
            agent_run_session_control(state.services.session_control.clone()),
            agent_run_session_eventing(state.services.session_eventing.clone()),
            agent_run_session_launch(state.services.session_launch.clone()),
        ),
    ));
    let result = service
        .respond(RespondCompanionGateCommand {
            gate_id: gate_uuid,
            payload: req.payload,
        })
        .await?;

    Ok(Json(CompanionGateRespondResponse {
        responded: true,
        gate_id: result.gate_id.to_string(),
        request_id: result.request_id,
        delivery_runtime_session_id: result.delivery_runtime_session_id,
        gate_resolved: result.gate_resolved,
    }))
}

async fn ensure_companion_gate_permission(
    state: &AppState,
    user: &agentdash_integration_api::AuthIdentity,
    gate_id: Uuid,
    permission: ProjectPermission,
) -> Result<(), ApiError> {
    let gate = state
        .repos
        .lifecycle_gate_repo
        .get(gate_id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("companion gate 不存在: {gate_id}")))?;

    let agent_id = if let Some(agent_id) = gate.agent_id {
        agent_id
    } else if let Some(frame_id) = gate.frame_id {
        state
            .repos
            .agent_frame_repo
            .get(frame_id)
            .await?
            .ok_or_else(|| ApiError::NotFound(format!("gate frame 不存在: {frame_id}")))?
            .agent_id
    } else {
        return Err(ApiError::Conflict(format!(
            "companion gate 缺少 agent/frame owner: {gate_id}"
        )));
    };

    let agent = state
        .repos
        .lifecycle_agent_repo
        .get(agent_id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("lifecycle_agent 不存在: {agent_id}")))?;
    load_project_with_permission(state, user, agent.project_id, permission).await?;
    Ok(())
}
