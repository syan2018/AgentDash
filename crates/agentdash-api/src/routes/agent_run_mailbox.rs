use std::sync::Arc;

use agentdash_agent_runtime_contract::{AgentRuntimeOperationReceipt, AgentTurnId};
use agentdash_application_agentrun::agent_run::mailbox::{
    AgentRunMailboxDeliveryIntent, AgentRunMailboxError, AgentRunMailboxIntakeCommand,
    AgentRunMailboxIntakeOutcome,
};
use agentdash_contracts::agent_run_mailbox::{
    AgentRunCommandReceipt, AgentRunComposerDeliveryIntent, AgentRunComposerSubmitRequest,
    AgentRunMailboxCommandRequest, AgentRunMailboxMessageContentView, AgentRunMailboxMoveRequest,
    AgentRunMailboxMoveResponse, AgentRunMailboxPromoteRequest, AgentRunMailboxView,
    AgentRunMessageCommandOutcome, AgentRunMessageCommandResponse,
};
use agentdash_domain::agent_run_mailbox::{
    MailboxMessageOrigin, MailboxMessageStatus, MailboxSourceIdentity,
};
use axum::{
    Json, Router,
    extract::{Path, State},
    routing::{delete, get, post, put},
};
use uuid::Uuid;

use crate::{
    app_state::AppState,
    auth::{CurrentUser, ProjectPermission},
    rpc::ApiError,
};

use super::{
    agent_run_mailbox_contracts::{mailbox_message_view, mailbox_state_view},
    lifecycle_agents::authorize_agent_run_target,
};

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route(
            "/agent-runs/{run_id}/agents/{agent_id}/mailbox",
            get(get_mailbox),
        )
        .route(
            "/agent-runs/{run_id}/agents/{agent_id}/composer-submit",
            post(submit_composer),
        )
        .route(
            "/agent-runs/{run_id}/agents/{agent_id}/mailbox/resume",
            post(resume_mailbox),
        )
        .route(
            "/agent-runs/{run_id}/agents/{agent_id}/mailbox/messages/{message_id}",
            delete(delete_message),
        )
        .route(
            "/agent-runs/{run_id}/agents/{agent_id}/mailbox/messages/{message_id}/content",
            get(get_message_content),
        )
        .route(
            "/agent-runs/{run_id}/agents/{agent_id}/mailbox/messages/{message_id}/move",
            put(move_message),
        )
        .route(
            "/agent-runs/{run_id}/agents/{agent_id}/mailbox/messages/{message_id}/promote",
            post(promote_message),
        )
}

async fn get_mailbox(
    State(state): State<Arc<AppState>>,
    CurrentUser(user): CurrentUser,
    Path((run_id, agent_id)): Path<(String, String)>,
) -> Result<Json<AgentRunMailboxView>, ApiError> {
    let target = authorize_agent_run_target(
        state.as_ref(),
        &user,
        &run_id,
        &agent_id,
        ProjectPermission::Use,
    )
    .await?;
    let messages = state
        .services
        .agent_run_mailbox
        .list(&target)
        .await
        .map_err(mailbox_error)?;
    let state_view = state
        .services
        .agent_run_mailbox
        .state(&target)
        .await
        .map_err(mailbox_error)?;
    let messages = messages
        .into_iter()
        .filter(|message| {
            !matches!(
                message.status,
                MailboxMessageStatus::Dispatched
                    | MailboxMessageStatus::Steered
                    | MailboxMessageStatus::Deleted
            )
        })
        .collect::<Vec<_>>();
    let visible = messages.len();
    Ok(Json(AgentRunMailboxView {
        state: mailbox_state_view(state_view.as_ref(), visible),
        messages: messages.into_iter().map(mailbox_message_view).collect(),
    }))
}

async fn submit_composer(
    State(state): State<Arc<AppState>>,
    CurrentUser(user): CurrentUser,
    Path((run_id, agent_id)): Path<(String, String)>,
    Json(request): Json<AgentRunComposerSubmitRequest>,
) -> Result<Json<AgentRunMessageCommandResponse>, ApiError> {
    let client_command_id = request.client_command_id.clone();
    let target = authorize_agent_run_target(
        state.as_ref(),
        &user,
        &run_id,
        &agent_id,
        ProjectPermission::Use,
    )
    .await?;
    let delivery_intent =
        match request.delivery_intent {
            AgentRunComposerDeliveryIntent::Queue => AgentRunMailboxDeliveryIntent::Queue,
            AgentRunComposerDeliveryIntent::Steer => AgentRunMailboxDeliveryIntent::Steer {
                expected_turn_id: AgentTurnId::new(request.expected_turn_id.ok_or_else(|| {
                    ApiError::BadRequest("Steer 缺少 expected_turn_id".to_owned())
                })?)
                .map_err(|error| ApiError::BadRequest(error.to_string()))?,
            },
        };
    let outcome = state
        .services
        .agent_run_mailbox
        .accept(AgentRunMailboxIntakeCommand {
            target,
            content: request.input,
            source: MailboxSourceIdentity::composer(),
            origin: MailboxMessageOrigin::User,
            client_command_id: request.client_command_id,
            delivery_intent,
            retain_payload: false,
        })
        .await
        .map_err(mailbox_error)?;
    Ok(Json(command_response(outcome, client_command_id)))
}

async fn delete_message(
    State(state): State<Arc<AppState>>,
    CurrentUser(user): CurrentUser,
    Path((run_id, agent_id, message_id)): Path<(String, String, String)>,
    Json(request): Json<AgentRunMailboxCommandRequest>,
) -> Result<Json<AgentRunMessageCommandResponse>, ApiError> {
    validate_client_command_id(&request.client_command_id)?;
    let target = authorize_agent_run_target(
        state.as_ref(),
        &user,
        &run_id,
        &agent_id,
        ProjectPermission::Use,
    )
    .await?;
    let message_id = parse_message_id(&message_id)?;
    let message = state
        .services
        .agent_run_mailbox
        .delete(&target, message_id)
        .await
        .map_err(mailbox_error)?
        .ok_or_else(|| ApiError::NotFound("Mailbox message 不存在".to_owned()))?;
    Ok(Json(command_response(
        AgentRunMailboxIntakeOutcome {
            message,
            operation_receipt: None,
            duplicate: false,
        },
        request.client_command_id,
    )))
}

async fn promote_message(
    State(state): State<Arc<AppState>>,
    CurrentUser(user): CurrentUser,
    Path((run_id, agent_id, message_id)): Path<(String, String, String)>,
    Json(request): Json<AgentRunMailboxPromoteRequest>,
) -> Result<Json<AgentRunMessageCommandResponse>, ApiError> {
    validate_client_command_id(&request.client_command_id)?;
    let target = authorize_agent_run_target(
        state.as_ref(),
        &user,
        &run_id,
        &agent_id,
        ProjectPermission::Use,
    )
    .await?;
    let outcome = state
        .services
        .agent_run_mailbox
        .promote(
            &target,
            parse_message_id(&message_id)?,
            AgentTurnId::new(request.expected_turn_id)
                .map_err(|error| ApiError::BadRequest(error.to_string()))?,
        )
        .await
        .map_err(mailbox_error)?;
    Ok(Json(command_response(outcome, request.client_command_id)))
}

async fn resume_mailbox(
    State(state): State<Arc<AppState>>,
    CurrentUser(user): CurrentUser,
    Path((run_id, agent_id)): Path<(String, String)>,
    Json(request): Json<AgentRunMailboxCommandRequest>,
) -> Result<Json<AgentRunCommandReceipt>, ApiError> {
    validate_client_command_id(&request.client_command_id)?;
    let target = authorize_agent_run_target(
        state.as_ref(),
        &user,
        &run_id,
        &agent_id,
        ProjectPermission::Use,
    )
    .await?;
    let receipt = state
        .services
        .agent_run_mailbox
        .resume(&target)
        .await
        .map_err(mailbox_error)?
        .map(|(_, receipt)| receipt);
    Ok(Json(receipt_view(
        request.client_command_id,
        receipt.as_ref(),
        false,
    )))
}

async fn move_message(
    State(state): State<Arc<AppState>>,
    CurrentUser(user): CurrentUser,
    Path((run_id, agent_id, message_id)): Path<(String, String, String)>,
    Json(request): Json<AgentRunMailboxMoveRequest>,
) -> Result<Json<AgentRunMailboxMoveResponse>, ApiError> {
    validate_client_command_id(&request.client_command_id)?;
    let target = authorize_agent_run_target(
        state.as_ref(),
        &user,
        &run_id,
        &agent_id,
        ProjectPermission::Use,
    )
    .await?;
    let moved = state
        .services
        .agent_run_mailbox
        .move_after(
            &target,
            parse_message_id(&message_id)?,
            request
                .after_message_id
                .as_deref()
                .map(parse_message_id)
                .transpose()?,
        )
        .await
        .map_err(mailbox_error)?;
    Ok(Json(AgentRunMailboxMoveResponse {
        ok: true,
        order_key: moved.order_key,
    }))
}

async fn get_message_content(
    State(state): State<Arc<AppState>>,
    CurrentUser(user): CurrentUser,
    Path((run_id, agent_id, message_id)): Path<(String, String, String)>,
) -> Result<Json<AgentRunMailboxMessageContentView>, ApiError> {
    let target = authorize_agent_run_target(
        state.as_ref(),
        &user,
        &run_id,
        &agent_id,
        ProjectPermission::Use,
    )
    .await?;
    let message_id = parse_message_id(&message_id)?;
    let input = state
        .services
        .agent_run_mailbox
        .content(&target, message_id)
        .await
        .map_err(mailbox_error)?
        .ok_or_else(|| ApiError::NotFound("Mailbox payload 已清理".to_owned()))?;
    Ok(Json(AgentRunMailboxMessageContentView {
        id: message_id.to_string(),
        input,
    }))
}

fn command_response(
    outcome: AgentRunMailboxIntakeOutcome,
    client_command_id: String,
) -> AgentRunMessageCommandResponse {
    let status = outcome.message.status;
    AgentRunMessageCommandResponse {
        command_receipt: receipt_view(
            client_command_id,
            outcome.operation_receipt.as_ref(),
            outcome.duplicate,
        ),
        outcome: match status {
            MailboxMessageStatus::Dispatched => AgentRunMessageCommandOutcome::Launched,
            MailboxMessageStatus::Steered => AgentRunMessageCommandOutcome::Steered,
            MailboxMessageStatus::Deleted => AgentRunMessageCommandOutcome::Deleted,
            MailboxMessageStatus::Blocked => AgentRunMessageCommandOutcome::Blocked,
            MailboxMessageStatus::Failed => AgentRunMessageCommandOutcome::Failed,
            _ => AgentRunMessageCommandOutcome::Queued,
        },
        mailbox_message: Some(mailbox_message_view(outcome.message)),
        accepted_refs: None,
        fork: None,
    }
}

fn receipt_view(
    client_command_id: String,
    receipt: Option<&AgentRuntimeOperationReceipt>,
    intake_duplicate: bool,
) -> AgentRunCommandReceipt {
    AgentRunCommandReceipt {
        client_command_id,
        status: receipt
            .map(|receipt| receipt.status.as_str())
            .unwrap_or("accepted")
            .to_owned(),
        duplicate: intake_duplicate || receipt.is_some_and(|receipt| receipt.duplicate),
        message: None,
    }
}

fn validate_client_command_id(value: &str) -> Result<(), ApiError> {
    if value.trim().is_empty() || value.len() > 256 {
        return Err(ApiError::BadRequest("client_command_id 无效".to_owned()));
    }
    Ok(())
}

fn parse_message_id(value: &str) -> Result<Uuid, ApiError> {
    value
        .parse()
        .map_err(|_| ApiError::BadRequest("Mailbox message id 无效".to_owned()))
}

fn mailbox_error(error: AgentRunMailboxError) -> ApiError {
    match error {
        AgentRunMailboxError::EmptyInput | AgentRunMailboxError::InvalidClientCommandId => {
            ApiError::BadRequest(error.to_string())
        }
        AgentRunMailboxError::StaleExpectedTurn | AgentRunMailboxError::DuplicateConflict => {
            ApiError::Conflict(error.to_string())
        }
        AgentRunMailboxError::Projection(_) | AgentRunMailboxError::Delivery(_) => {
            ApiError::ServiceUnavailable(error.to_string())
        }
        AgentRunMailboxError::Repository(_) => ApiError::Internal(error.to_string()),
    }
}
