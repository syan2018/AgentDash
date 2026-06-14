use std::sync::Arc;

use agentdash_application::session::{
    AgentRunMailboxCommandOutcome as AppMailboxCommandOutcome, AgentRunMailboxCommandResult,
    AgentRunMailboxControlCommand, AgentRunMailboxService, AgentRunMailboxUserMessageCommand,
};
use agentdash_application::workflow::agent_run_workspace as app_workspace;
use agentdash_contracts::agent_run_mailbox::{
    AgentRunCommandReceipt, AgentRunComposerSubmitRequest, AgentRunMailboxMessageContentView,
    AgentRunMailboxMoveRequest, AgentRunMailboxView, AgentRunMessageAcceptedRefs,
    AgentRunMessageCommandOutcome, AgentRunMessageCommandResponse, MailboxStateView,
    RuntimeSessionCommandStateDto,
};
use agentdash_contracts::workflow::{
    AgentFrameRefDto, AgentFrameRuntimeView, AgentRunCommandOnlyRequest, AgentRunRefDto,
    AgentRunWorkspaceControlPlaneStatus, AgentRunWorkspaceControlPlaneView,
    AgentRunWorkspaceListEntry, AgentRunWorkspaceListView, AgentRunWorkspaceShell,
    AgentRunWorkspaceView, ConversationExecutionStatus, LifecycleRunRefDto,
    LifecycleSubjectAssociationDto, RuntimeSessionRefDto, RuntimeSessionTraceMeta,
};
use agentdash_domain::workflow::{
    AgentRunAcceptedRefs, AgentRunCommandClaim, AgentRunCommandKind,
    AgentRunCommandReceipt as DomainAgentRunCommandReceipt, LifecycleAgent, LifecycleRun,
    NewAgentRunCommandReceipt,
};
use agentdash_spi::AgentConfig;
use axum::{
    Json,
    extract::{Path, State},
};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::{
    app_state::AppState,
    auth::{CurrentUser, ProjectPermission, load_project_with_permission},
    routes::{
        agent_run_mailbox_contracts::{
            mailbox_message_view, mailbox_message_visible, mailbox_state_view,
        },
        lifecycle_contracts::{agent_run_to_contract, subject_association_to_contract},
        vfs_surfaces::dto as vfs_surface_dto,
    },
    rpc::{ApiError, ApiErrorWithCode},
    vfs_surface_runtime::ApiVfsSurfaceRuntimeProjection,
};

struct AgentRunContext {
    run: LifecycleRun,
    agent: LifecycleAgent,
    delivery_runtime_session_id: Option<String>,
}

pub fn router() -> axum::Router<Arc<AppState>> {
    axum::Router::new()
        .route(
            "/projects/{project_id}/agent-runs",
            axum::routing::get(get_project_agent_runs),
        )
        .route(
            "/agent-runs/{run_id}/agents/{agent_id}/workspace",
            axum::routing::get(get_agent_run_workspace),
        )
        .route(
            "/agent-runs/{run_id}/agents/{agent_id}/composer-submit",
            axum::routing::post(submit_agent_run_composer_input),
        )
        .route(
            "/agent-runs/{run_id}/agents/{agent_id}/mailbox",
            axum::routing::get(get_agent_run_mailbox),
        )
        .route(
            "/agent-runs/{run_id}/agents/{agent_id}/mailbox/resume",
            axum::routing::post(resume_agent_run_mailbox),
        )
        .route(
            "/agent-runs/{run_id}/agents/{agent_id}/mailbox/messages/{message_id}",
            axum::routing::delete(delete_agent_run_mailbox_message),
        )
        .route(
            "/agent-runs/{run_id}/agents/{agent_id}/mailbox/messages/{message_id}/promote",
            axum::routing::post(promote_agent_run_mailbox_message),
        )
        .route(
            "/agent-runs/{run_id}/agents/{agent_id}/mailbox/messages/{message_id}/move",
            axum::routing::put(move_agent_run_mailbox_message),
        )
        .route(
            "/agent-runs/{run_id}/agents/{agent_id}/mailbox/messages/{message_id}/content",
            axum::routing::get(get_agent_run_mailbox_message_content),
        )
        .route(
            "/agent-runs/{run_id}/agents/{agent_id}/cancel",
            axum::routing::post(cancel_agent_run),
        )
}

pub async fn get_project_agent_runs(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(project_id): Path<String>,
) -> Result<Json<AgentRunWorkspaceListView>, ApiError> {
    let project_id = parse_uuid(&project_id, "project_id")?;
    load_project_with_permission(
        state.as_ref(),
        &current_user,
        project_id,
        ProjectPermission::View,
    )
    .await?;

    let runs = state
        .repos
        .lifecycle_run_repo
        .list_by_project(project_id)
        .await
        .map_err(ApiError::from)?;
    let mut entries = Vec::new();
    for run in runs {
        let agents = state
            .repos
            .lifecycle_agent_repo
            .list_by_run(run.id)
            .await
            .map_err(ApiError::from)?;
        for agent in agents {
            let delivery_runtime_session_id = state
                .repos
                .execution_anchor_repo
                .latest_for_agent(agent.id)
                .await
                .map_err(ApiError::from)?
                .map(|anchor| anchor.runtime_session_id);
            let context = AgentRunContext {
                run: run.clone(),
                agent,
                delivery_runtime_session_id,
            };
            let workspace = agent_run_workspace_view(
                load_agent_run_workspace_snapshot(&state, &context).await?,
            );
            entries.push(agent_run_workspace_list_entry(&context.run, workspace));
        }
    }
    entries.sort_by(|a, b| b.shell.last_activity_at.cmp(&a.shell.last_activity_at));

    Ok(Json(AgentRunWorkspaceListView {
        project_id: project_id.to_string(),
        agent_runs: entries,
    }))
}

pub async fn get_agent_run_workspace(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path((run_id, agent_id)): Path<(String, String)>,
) -> Result<Json<AgentRunWorkspaceView>, ApiError> {
    let context = resolve_agent_run_context(
        &state,
        &current_user,
        &run_id,
        &agent_id,
        ProjectPermission::View,
    )
    .await?;
    Ok(Json(agent_run_workspace_view(
        load_agent_run_workspace_snapshot(&state, &context).await?,
    )))
}

pub async fn submit_agent_run_composer_input(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path((run_id, agent_id)): Path<(String, String)>,
    Json(req): Json<AgentRunComposerSubmitRequest>,
) -> Result<Json<AgentRunMessageCommandResponse>, ApiError> {
    if req.client_command_id.trim().is_empty() {
        return Err(ApiError::BadRequest(
            "client_command_id 不能为空".to_string(),
        ));
    }
    if req.input.is_empty() {
        return Err(ApiError::BadRequest("input 不能为空".to_string()));
    }

    let context = resolve_agent_run_context(
        &state,
        &current_user,
        &run_id,
        &agent_id,
        ProjectPermission::Edit,
    )
    .await?;
    let runtime_session_id = context.delivery_runtime_session_id.clone().ok_or_else(|| {
        ApiError::Conflict(format!(
            "AgentRun {} / {} 缺少 delivery runtime",
            context.run.id, context.agent.id
        ))
    })?;
    agent_run_workspace_command_policy(state.as_ref())
        .ensure_composer_submit_allowed(
            command_policy_context(&context, &runtime_session_id),
            &req.command,
        )
        .await
        .map_err(command_policy_error)?;
    let executor_config = req
        .executor_config
        .map(serde_json::from_value::<AgentConfig>)
        .transpose()
        .map_err(|e| ApiError::BadRequest(format!("executor_config 格式错误: {e}")))?;
    let service = agent_run_mailbox_service(state.as_ref());
    let response = service
        .accept_user_message(AgentRunMailboxUserMessageCommand {
            run_id: context.run.id,
            agent_id: context.agent.id,
            runtime_session_id,
            input: req.input,
            client_command_id: req.client_command_id,
            executor_config,
            identity: Some(current_user),
            delivery_intent: req.delivery_intent,
        })
        .await
        .map_err(ApiError::from)?;
    Ok(Json(agent_run_message_command_response(response)))
}

async fn get_agent_run_mailbox(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path((run_id, agent_id)): Path<(String, String)>,
) -> Result<Json<AgentRunMailboxView>, ApiError> {
    let context = resolve_agent_run_context(
        &state,
        &current_user,
        &run_id,
        &agent_id,
        ProjectPermission::View,
    )
    .await?;
    Ok(Json(
        build_agent_run_mailbox_view(state.as_ref(), &context).await?,
    ))
}

async fn delete_agent_run_mailbox_message(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path((run_id, agent_id, message_id)): Path<(String, String, String)>,
    Json(body): Json<AgentRunCommandOnlyRequest>,
) -> Result<Json<AgentRunMessageCommandResponse>, ApiError> {
    let context = resolve_agent_run_context(
        &state,
        &current_user,
        &run_id,
        &agent_id,
        ProjectPermission::Edit,
    )
    .await?;
    let runtime_session_id = context.delivery_runtime_session_id.clone().ok_or_else(|| {
        ApiError::Conflict(format!(
            "AgentRun {} / {} 缺少 delivery runtime",
            context.run.id, context.agent.id
        ))
    })?;
    agent_run_workspace_command_policy(state.as_ref())
        .ensure_command_allowed(
            command_policy_context(&context, &runtime_session_id),
            app_workspace::AgentRunWorkspaceCommandPrecondition::DeleteMailboxMessage {
                command: body.command.clone(),
            },
        )
        .await
        .map_err(command_policy_error)?;
    let message_id = parse_uuid(&message_id, "message_id")?;
    let response = agent_run_mailbox_service(state.as_ref())
        .delete_message(AgentRunMailboxControlCommand {
            run_id: context.run.id,
            agent_id: context.agent.id,
            runtime_session_id,
            message_id: Some(message_id),
            client_command_id: body.client_command_id,
        })
        .await
        .map_err(ApiError::from)?;
    Ok(Json(agent_run_message_command_response(response)))
}

async fn resume_agent_run_mailbox(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path((run_id, agent_id)): Path<(String, String)>,
    Json(body): Json<AgentRunCommandOnlyRequest>,
) -> Result<Json<AgentRunMessageCommandResponse>, ApiError> {
    let context = resolve_agent_run_context(
        &state,
        &current_user,
        &run_id,
        &agent_id,
        ProjectPermission::Edit,
    )
    .await?;
    let runtime_session_id = context.delivery_runtime_session_id.clone().ok_or_else(|| {
        ApiError::Conflict(format!(
            "AgentRun {} / {} 缺少 delivery runtime",
            context.run.id, context.agent.id
        ))
    })?;
    agent_run_workspace_command_policy(state.as_ref())
        .ensure_command_allowed(
            command_policy_context(&context, &runtime_session_id),
            app_workspace::AgentRunWorkspaceCommandPrecondition::ResumeMailbox {
                command: body.command.clone(),
            },
        )
        .await
        .map_err(command_policy_error)?;
    let response = agent_run_mailbox_service(state.as_ref())
        .resume_mailbox(
            AgentRunMailboxControlCommand {
                run_id: context.run.id,
                agent_id: context.agent.id,
                runtime_session_id,
                message_id: None,
                client_command_id: body.client_command_id,
            },
            Some(current_user),
        )
        .await
        .map_err(ApiError::from)?;
    Ok(Json(agent_run_message_command_response(response)))
}

async fn promote_agent_run_mailbox_message(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path((run_id, agent_id, message_id)): Path<(String, String, String)>,
    Json(body): Json<AgentRunCommandOnlyRequest>,
) -> Result<Json<AgentRunMessageCommandResponse>, ApiError> {
    let context = resolve_agent_run_context(
        &state,
        &current_user,
        &run_id,
        &agent_id,
        ProjectPermission::Edit,
    )
    .await?;
    let runtime_session_id = context.delivery_runtime_session_id.clone().ok_or_else(|| {
        ApiError::Conflict(format!(
            "AgentRun {} / {} 缺少 delivery runtime",
            context.run.id, context.agent.id
        ))
    })?;
    agent_run_workspace_command_policy(state.as_ref())
        .ensure_command_allowed(
            command_policy_context(&context, &runtime_session_id),
            app_workspace::AgentRunWorkspaceCommandPrecondition::PromoteMailboxMessage {
                command: body.command.clone(),
            },
        )
        .await
        .map_err(command_policy_error)?;
    let message_id = parse_uuid(&message_id, "message_id")?;
    let response = agent_run_mailbox_service(state.as_ref())
        .promote_message(
            AgentRunMailboxControlCommand {
                run_id: context.run.id,
                agent_id: context.agent.id,
                runtime_session_id,
                message_id: Some(message_id),
                client_command_id: body.client_command_id,
            },
            Some(current_user),
        )
        .await
        .map_err(ApiError::from)?;
    Ok(Json(agent_run_message_command_response(response)))
}

async fn move_agent_run_mailbox_message(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path((run_id, agent_id, message_id)): Path<(String, String, String)>,
    Json(body): Json<AgentRunMailboxMoveRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let context = resolve_agent_run_context(
        &state,
        &current_user,
        &run_id,
        &agent_id,
        ProjectPermission::Edit,
    )
    .await?;
    let message_id = parse_uuid(&message_id, "message_id")?;
    let after_message_id = body
        .after_message_id
        .as_deref()
        .map(|id| parse_uuid(id, "after_message_id"))
        .transpose()?;
    let updated = agent_run_mailbox_service(state.as_ref())
        .move_message(
            context.run.id,
            context.agent.id,
            message_id,
            after_message_id,
        )
        .await
        .map_err(ApiError::from)?;
    Ok(Json(
        serde_json::json!({ "ok": true, "order_key": updated.order_key }),
    ))
}

async fn get_agent_run_mailbox_message_content(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path((run_id, agent_id, message_id)): Path<(String, String, String)>,
) -> Result<Json<AgentRunMailboxMessageContentView>, ApiError> {
    let context = resolve_agent_run_context(
        &state,
        &current_user,
        &run_id,
        &agent_id,
        ProjectPermission::View,
    )
    .await?;
    let message_id = parse_uuid(&message_id, "message_id")?;
    let input = agent_run_mailbox_service(state.as_ref())
        .get_message_content(context.run.id, context.agent.id, message_id)
        .await
        .map_err(ApiError::from)?;
    Ok(Json(AgentRunMailboxMessageContentView {
        id: message_id.to_string(),
        input,
    }))
}

async fn cancel_agent_run(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path((run_id, agent_id)): Path<(String, String)>,
    Json(body): Json<AgentRunCommandOnlyRequest>,
) -> Result<Json<AgentRunCommandReceipt>, ApiError> {
    if body.client_command_id.trim().is_empty() {
        return Err(ApiError::BadRequest(
            "client_command_id 不能为空".to_string(),
        ));
    }
    let context = resolve_agent_run_context(
        &state,
        &current_user,
        &run_id,
        &agent_id,
        ProjectPermission::Edit,
    )
    .await?;
    let runtime_session_id = context.delivery_runtime_session_id.clone().ok_or_else(|| {
        ApiError::Conflict(format!(
            "AgentRun {} / {} 缺少 delivery runtime",
            context.run.id, context.agent.id
        ))
    })?;
    agent_run_workspace_command_policy(state.as_ref())
        .ensure_command_allowed(
            command_policy_context(&context, &runtime_session_id),
            app_workspace::AgentRunWorkspaceCommandPrecondition::Cancel {
                command: body.command.clone(),
            },
        )
        .await
        .map_err(command_policy_error)?;
    let request_digest =
        digest_cancel_command_request(context.run.id, context.agent.id, &runtime_session_id)?;
    let claim = state
        .repos
        .agent_run_command_receipt_repo
        .claim(NewAgentRunCommandReceipt {
            scope_kind: "agent_run_mailbox".to_string(),
            scope_key: format!("{}:{}", context.run.id, context.agent.id),
            command_kind: AgentRunCommandKind::Cancel,
            client_command_id: body.client_command_id,
            request_digest,
        })
        .await
        .map_err(ApiError::from)?;
    let receipt = match claim {
        AgentRunCommandClaim::Duplicate(receipt) => {
            return Ok(Json(domain_command_receipt_view(&receipt, true)));
        }
        AgentRunCommandClaim::Created(receipt) => receipt,
    };
    if let Err(error) = state
        .services
        .session_runtime
        .cancel(&runtime_session_id)
        .await
    {
        if let Err(mark_error) = state
            .repos
            .agent_run_command_receipt_repo
            .mark_terminal_failed(receipt.id, error.to_string())
            .await
        {
            tracing::warn!(
                receipt_id = %receipt.id,
                error = %mark_error,
                "写入 AgentRun cancel terminal_failed receipt 失败"
            );
        }
        return Err(ApiError::from(error));
    }
    let accepted = state
        .repos
        .agent_run_command_receipt_repo
        .mark_accepted(
            receipt.id,
            AgentRunAcceptedRefs {
                run_id: context.run.id,
                agent_id: context.agent.id,
                frame_id: None,
                frame_revision: None,
                runtime_session_id: Some(runtime_session_id),
                agent_run_turn_id: None,
                protocol_turn_id: None,
            },
        )
        .await
        .map_err(ApiError::from)?;
    let stored = state
        .repos
        .agent_run_command_receipt_repo
        .store_result_json(receipt.id, serde_json::json!({ "cancelled": true }))
        .await
        .map_err(ApiError::from)?;
    let final_receipt = if stored.updated_at >= accepted.updated_at {
        stored
    } else {
        accepted
    };
    Ok(Json(domain_command_receipt_view(&final_receipt, false)))
}

async fn resolve_agent_run_context(
    state: &AppState,
    current_user: &agentdash_integration_api::AuthIdentity,
    run_id: &str,
    agent_id: &str,
    permission: ProjectPermission,
) -> Result<AgentRunContext, ApiError> {
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
    let delivery_runtime_session_id =
        delivery_runtime_session_for_agent_run(state, run.id, agent.id).await?;
    Ok(AgentRunContext {
        run,
        agent,
        delivery_runtime_session_id,
    })
}

async fn delivery_runtime_session_for_agent_run(
    state: &AppState,
    run_id: Uuid,
    agent_id: Uuid,
) -> Result<Option<String>, ApiError> {
    let anchors = state
        .repos
        .execution_anchor_repo
        .list_by_run(run_id)
        .await
        .map_err(ApiError::from)?;
    Ok(anchors
        .into_iter()
        .filter(|anchor| anchor.agent_id == agent_id)
        .max_by_key(|anchor| anchor.updated_at)
        .map(|anchor| anchor.runtime_session_id))
}

async fn load_agent_run_workspace_snapshot(
    state: &AppState,
    context: &AgentRunContext,
) -> Result<app_workspace::AgentRunWorkspaceSnapshot, ApiError> {
    let vfs_runtime = ApiVfsSurfaceRuntimeProjection::new(
        state.services.backend_registry.clone(),
        state.services.mount_provider_registry.clone(),
    );
    let service = app_workspace::AgentRunWorkspaceQueryService::new(
        &state.repos,
        state.services.session_core.clone(),
        state.services.session_control.clone(),
        &vfs_runtime,
    );
    service
        .resolve(app_workspace::AgentRunWorkspaceQueryInput {
            run: context.run.clone(),
            agent: context.agent.clone(),
        })
        .await
        .map_err(ApiError::from)
}

fn agent_run_workspace_view(
    snapshot: app_workspace::AgentRunWorkspaceSnapshot,
) -> AgentRunWorkspaceView {
    let resource_surface = snapshot
        .resource_surface
        .map(vfs_surface_dto::surface_from_application);
    let mailbox = workspace_mailbox_to_contract(snapshot.mailbox);
    let mailbox_messages = snapshot
        .mailbox_messages
        .into_iter()
        .map(mailbox_message_view)
        .collect();
    let mut conversation = snapshot.conversation;
    conversation.mailbox.state = Some(mailbox);
    conversation.mailbox.messages = mailbox_messages;
    let control_plane = workspace_control_plane_from_conversation(&conversation);

    AgentRunWorkspaceView {
        run_ref: LifecycleRunRefDto {
            run_id: snapshot.run.id.to_string(),
        },
        agent_ref: AgentRunRefDto {
            run_id: snapshot.run.id.to_string(),
            agent_id: snapshot.agent.id.to_string(),
        },
        project_id: snapshot.run.project_id.to_string(),
        shell: AgentRunWorkspaceShell {
            display_title: snapshot.shell.display_title,
            title_source: snapshot.shell.title_source,
            workspace_status: snapshot.shell.workspace_status,
            delivery_status: snapshot.shell.delivery_status,
            last_turn_id: snapshot.shell.last_turn_id,
            last_activity_at: snapshot.shell.last_activity_at,
        },
        delivery_runtime_ref: snapshot
            .delivery_runtime_session_id
            .map(|runtime_session_id| RuntimeSessionRefDto { runtime_session_id }),
        delivery_trace_meta: snapshot
            .delivery_trace_meta
            .map(workspace_trace_meta_to_contract),
        control_plane,
        agent: snapshot.agent_view.map(agent_run_to_contract),
        frame_runtime: snapshot.frame_runtime.map(frame_runtime_to_contract),
        subject_associations: snapshot
            .subject_associations
            .into_iter()
            .map(subject_association_to_contract)
            .collect(),
        resource_surface,
        conversation: Some(conversation),
    }
}

fn workspace_trace_meta_to_contract(
    meta: app_workspace::AgentRunWorkspaceTraceMetaModel,
) -> RuntimeSessionTraceMeta {
    RuntimeSessionTraceMeta {
        runtime_session_ref: RuntimeSessionRefDto {
            runtime_session_id: meta.runtime_session_id,
        },
        last_event_seq: meta.last_event_seq,
        executor_session_id: meta.executor_session_id,
        trace_title: meta.trace_title,
        trace_title_source: meta.trace_title_source,
        delivery_status: meta.delivery_status,
        last_turn_id: meta.last_turn_id,
        terminal_summary: meta.terminal_summary,
        updated_at: meta.updated_at,
    }
}

fn workspace_control_plane_from_conversation(
    conversation: &agentdash_contracts::workflow::AgentConversationSnapshot,
) -> AgentRunWorkspaceControlPlaneView {
    let status = match conversation.execution.status {
        ConversationExecutionStatus::Ready
        | ConversationExecutionStatus::Draft
        | ConversationExecutionStatus::ModelRequired => AgentRunWorkspaceControlPlaneStatus::Ready,
        ConversationExecutionStatus::StartingClaimed
        | ConversationExecutionStatus::RunningActive => {
            AgentRunWorkspaceControlPlaneStatus::Running
        }
        ConversationExecutionStatus::Cancelling => AgentRunWorkspaceControlPlaneStatus::Cancelling,
        ConversationExecutionStatus::Terminal => AgentRunWorkspaceControlPlaneStatus::Terminal,
        ConversationExecutionStatus::FrameMissing => {
            AgentRunWorkspaceControlPlaneStatus::FrameMissing
        }
        ConversationExecutionStatus::DeliveryMissing => {
            AgentRunWorkspaceControlPlaneStatus::DeliveryMissing
        }
    };
    AgentRunWorkspaceControlPlaneView {
        status,
        reason: conversation.execution.reason.clone(),
    }
}

fn workspace_mailbox_to_contract(
    mailbox: app_workspace::AgentRunWorkspaceMailboxStateModel,
) -> MailboxStateView {
    MailboxStateView {
        paused: mailbox.paused,
        pause_reason: mailbox.pause_reason,
        message: mailbox.message,
        can_resume: mailbox.can_resume,
        hide_system_steer_messages: mailbox.hide_system_steer_messages,
    }
}

fn frame_runtime_to_contract(
    frame: app_workspace::AgentRunWorkspaceFrameRuntimeModel,
) -> AgentFrameRuntimeView {
    AgentFrameRuntimeView {
        frame_ref: AgentFrameRefDto {
            agent_id: frame.frame_ref.agent_id,
            frame_id: frame.frame_ref.frame_id,
            revision: frame.frame_ref.revision,
        },
        capability_surface: frame.capability_surface,
        context_slice: frame.context_slice,
        vfs_surface: frame.vfs_surface,
        mcp_surface: frame.mcp_surface,
        runtime_session_refs: frame
            .runtime_session_refs
            .into_iter()
            .map(|runtime_ref| RuntimeSessionRefDto {
                runtime_session_id: runtime_ref.runtime_session_id,
            })
            .collect(),
        execution_profile: frame.execution_profile,
        effective_executor_config: frame.effective_executor_config,
    }
}

fn agent_run_workspace_list_entry(
    run: &LifecycleRun,
    workspace: AgentRunWorkspaceView,
) -> AgentRunWorkspaceListEntry {
    let subject_association = workspace.subject_associations.first();
    AgentRunWorkspaceListEntry {
        run_ref: workspace.run_ref,
        agent_ref: workspace.agent_ref,
        project_id: workspace.project_id,
        shell: workspace.shell,
        run_status: lifecycle_run_status_to_contract(run.status),
        delivery_runtime_ref: workspace.delivery_runtime_ref,
        delivery_trace_meta: workspace.delivery_trace_meta,
        frame_ref: workspace.frame_runtime.map(|frame| frame.frame_ref),
        subject_ref: subject_association.map(|association| association.subject_ref.clone()),
        subject_label: subject_association.and_then(subject_label_from_metadata),
    }
}

fn subject_label_from_metadata(association: &LifecycleSubjectAssociationDto) -> Option<String> {
    let metadata = association.metadata.as_ref()?;
    ["label", "title", "name"]
        .iter()
        .find_map(|key| metadata.get(key).and_then(|value| value.as_str()))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

async fn build_agent_run_mailbox_view(
    state: &AppState,
    context: &AgentRunContext,
) -> Result<AgentRunMailboxView, ApiError> {
    let messages = state
        .repos
        .agent_run_mailbox_repo
        .list_messages(context.run.id, context.agent.id)
        .await
        .map_err(ApiError::from)?;
    let visible_message_count = messages
        .iter()
        .filter(|message| mailbox_message_visible(message))
        .count();
    let mailbox_state = state
        .repos
        .agent_run_mailbox_repo
        .get_state(context.run.id, context.agent.id)
        .await
        .map_err(ApiError::from)?;
    Ok(AgentRunMailboxView {
        state: mailbox_state_view(
            mailbox_state.as_ref(),
            context.delivery_runtime_session_id.is_some()
                && !app_workspace::is_terminal_agent_status(&context.agent.status),
            visible_message_count,
            state
                .repos
                .backend_repo
                .get_preferences()
                .await
                .unwrap_or_default()
                .hide_system_steer_messages,
        ),
        messages: messages
            .into_iter()
            .filter(|msg| mailbox_message_visible(msg))
            .map(mailbox_message_view)
            .collect(),
    })
}

fn agent_run_mailbox_service(state: &AppState) -> AgentRunMailboxService<'_> {
    AgentRunMailboxService::new(
        state.repos.lifecycle_run_repo.as_ref(),
        state.repos.lifecycle_agent_repo.as_ref(),
        state.repos.agent_frame_repo.as_ref(),
        state.repos.execution_anchor_repo.as_ref(),
        state.repos.agent_run_command_receipt_repo.as_ref(),
        state.repos.agent_run_mailbox_repo.as_ref(),
        state.services.session_core.clone(),
        state.services.session_control.clone(),
        state.services.session_eventing.clone(),
        state.services.session_launch.clone(),
    )
}

fn agent_run_workspace_command_policy(
    state: &AppState,
) -> app_workspace::AgentRunWorkspaceCommandPolicyService<'_> {
    app_workspace::AgentRunWorkspaceCommandPolicyService::new(
        &state.repos,
        state.services.session_core.clone(),
        state.services.session_control.clone(),
    )
}

fn command_policy_context<'a>(
    context: &'a AgentRunContext,
    runtime_session_id: &'a str,
) -> app_workspace::AgentRunWorkspaceCommandPolicyContext<'a> {
    app_workspace::AgentRunWorkspaceCommandPolicyContext {
        run: &context.run,
        agent: &context.agent,
        runtime_session_id,
    }
}

fn agent_run_message_command_response(
    result: AgentRunMailboxCommandResult,
) -> AgentRunMessageCommandResponse {
    AgentRunMessageCommandResponse {
        command_receipt: command_receipt_view(result.command_receipt),
        outcome: mailbox_command_outcome_view(result.outcome),
        mailbox_message: result.mailbox_message.map(mailbox_message_view),
        accepted_refs: result.accepted_refs.map(agent_run_message_accepted_refs),
        runtime_state: result.runtime_state.map(|state| {
            runtime_command_state_dto(
                app_workspace::AgentRunWorkspaceProjection::runtime_command_state(&state),
            )
        }),
    }
}

fn mailbox_command_outcome_view(
    outcome: AppMailboxCommandOutcome,
) -> AgentRunMessageCommandOutcome {
    match outcome {
        AppMailboxCommandOutcome::Launched => AgentRunMessageCommandOutcome::Launched,
        AppMailboxCommandOutcome::Queued => AgentRunMessageCommandOutcome::Queued,
        AppMailboxCommandOutcome::Steered => AgentRunMessageCommandOutcome::Steered,
        AppMailboxCommandOutcome::Deleted => AgentRunMessageCommandOutcome::Deleted,
        AppMailboxCommandOutcome::Resumed => AgentRunMessageCommandOutcome::Resumed,
        AppMailboxCommandOutcome::Blocked => AgentRunMessageCommandOutcome::Blocked,
        AppMailboxCommandOutcome::Failed => AgentRunMessageCommandOutcome::Failed,
    }
}

fn agent_run_message_accepted_refs(
    refs: agentdash_domain::workflow::AgentRunAcceptedRefs,
) -> AgentRunMessageAcceptedRefs {
    AgentRunMessageAcceptedRefs {
        run_ref: LifecycleRunRefDto {
            run_id: refs.run_id.to_string(),
        },
        agent_ref: AgentRunRefDto {
            run_id: refs.run_id.to_string(),
            agent_id: refs.agent_id.to_string(),
        },
        frame_ref: refs.frame_id.map(|frame_id| AgentFrameRefDto {
            agent_id: refs.agent_id.to_string(),
            frame_id: frame_id.to_string(),
            revision: refs.frame_revision,
        }),
        runtime_session_ref: refs
            .runtime_session_id
            .map(|runtime_session_id| RuntimeSessionRefDto { runtime_session_id }),
        agent_run_turn_id: refs.agent_run_turn_id,
        protocol_turn_id: refs.protocol_turn_id,
    }
}

fn lifecycle_run_status_to_contract(
    status: agentdash_domain::workflow::LifecycleRunStatus,
) -> agentdash_contracts::workflow::LifecycleRunStatus {
    match status {
        agentdash_domain::workflow::LifecycleRunStatus::Draft => {
            agentdash_contracts::workflow::LifecycleRunStatus::Draft
        }
        agentdash_domain::workflow::LifecycleRunStatus::Ready => {
            agentdash_contracts::workflow::LifecycleRunStatus::Ready
        }
        agentdash_domain::workflow::LifecycleRunStatus::Running => {
            agentdash_contracts::workflow::LifecycleRunStatus::Running
        }
        agentdash_domain::workflow::LifecycleRunStatus::Blocked => {
            agentdash_contracts::workflow::LifecycleRunStatus::Blocked
        }
        agentdash_domain::workflow::LifecycleRunStatus::Completed => {
            agentdash_contracts::workflow::LifecycleRunStatus::Completed
        }
        agentdash_domain::workflow::LifecycleRunStatus::Failed => {
            agentdash_contracts::workflow::LifecycleRunStatus::Failed
        }
        agentdash_domain::workflow::LifecycleRunStatus::Cancelled => {
            agentdash_contracts::workflow::LifecycleRunStatus::Cancelled
        }
    }
}

fn command_receipt_view(
    receipt: agentdash_application::workflow::AgentRunCommandReceiptView,
) -> AgentRunCommandReceipt {
    AgentRunCommandReceipt {
        client_command_id: receipt.client_command_id,
        status: receipt.status,
        duplicate: receipt.duplicate,
        message: receipt.message,
    }
}

fn domain_command_receipt_view(
    receipt: &DomainAgentRunCommandReceipt,
    duplicate: bool,
) -> AgentRunCommandReceipt {
    AgentRunCommandReceipt {
        client_command_id: receipt.client_command_id.clone(),
        status: receipt.status.as_str().to_string(),
        duplicate,
        message: receipt.error_message.clone(),
    }
}

fn digest_cancel_command_request(
    run_id: Uuid,
    agent_id: Uuid,
    runtime_session_id: &str,
) -> Result<String, ApiError> {
    let value = serde_json::json!({
        "kind": "agent_run_cancel",
        "run_id": run_id,
        "agent_id": agent_id,
        "runtime_session_id": runtime_session_id,
    });
    let bytes = serde_json::to_vec(&value).map_err(|error| {
        ApiError::BadRequest(format!("cancel command digest 无法序列化: {error}"))
    })?;
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    Ok(format!("sha256:{:x}", hasher.finalize()))
}

fn runtime_command_state_dto(
    state: app_workspace::AgentRunWorkspaceRuntimeCommandStateModel,
) -> RuntimeSessionCommandStateDto {
    RuntimeSessionCommandStateDto {
        status: state.status.as_str().to_string(),
        turn_id: state.turn_id,
        message: state.message,
    }
}

fn parse_uuid(raw: &str, field: &str) -> Result<Uuid, ApiError> {
    Uuid::parse_str(raw).map_err(|_| ApiError::BadRequest(format!("无效的 {field}: {raw}")))
}

fn command_policy_error(error: app_workspace::AgentRunWorkspaceCommandPolicyError) -> ApiError {
    match error {
        app_workspace::AgentRunWorkspaceCommandPolicyError::Application(error) => {
            ApiError::from(error)
        }
        app_workspace::AgentRunWorkspaceCommandPolicyError::Conflict(conflict) => {
            ApiError::ConflictWithCode(Box::new(ApiErrorWithCode {
                message: conflict.message,
                error_code: conflict.error_code,
                replacement_command: conflict.replacement_command,
                detail: conflict.detail,
            }))
        }
    }
}

#[cfg(test)]
mod tests {
    use agentdash_domain::workflow::LifecycleRun;

    use super::*;

    fn test_shell(display_title: &str, title_source: &str) -> AgentRunWorkspaceShell {
        AgentRunWorkspaceShell {
            display_title: display_title.to_string(),
            title_source: title_source.to_string(),
            workspace_status: "running".to_string(),
            delivery_status: "idle".to_string(),
            last_turn_id: None,
            last_activity_at: "2026-06-12T00:00:00Z".to_string(),
        }
    }

    #[test]
    fn list_entry_inherits_workspace_shell_title() {
        let run = LifecycleRun::new_graphless(Uuid::new_v4());
        let run_id = run.id.to_string();
        let agent_id = Uuid::new_v4().to_string();
        let project_id = run.project_id.to_string();
        let workspace = AgentRunWorkspaceView {
            run_ref: LifecycleRunRefDto {
                run_id: run_id.clone(),
            },
            agent_ref: AgentRunRefDto { run_id, agent_id },
            project_id,
            shell: test_shell("Session meta title", "source"),
            delivery_runtime_ref: None,
            delivery_trace_meta: None,
            control_plane: AgentRunWorkspaceControlPlaneView {
                status: AgentRunWorkspaceControlPlaneStatus::Ready,
                reason: None,
            },
            agent: None,
            frame_runtime: None,
            subject_associations: Vec::new(),
            resource_surface: None,
            conversation: None,
        };

        let entry = agent_run_workspace_list_entry(&run, workspace);

        assert_eq!(entry.shell.display_title, "Session meta title");
        assert_eq!(entry.shell.title_source, "source");
    }

    #[test]
    fn mailbox_state_view_exposes_pause_reason_and_resume() {
        let state = agentdash_domain::agent_run_mailbox::AgentRunMailboxState {
            run_id: Uuid::new_v4(),
            agent_id: Uuid::new_v4(),
            runtime_session_id: "runtime-1".to_string(),
            paused: true,
            pause_reason: Some("turn_interrupted".to_string()),
            pause_message: Some("上一轮已中断，mailbox 已暂停。".to_string()),
            updated_at: chrono::Utc::now(),
        };
        let view = mailbox_state_view(Some(&state), true, 1, false);

        assert!(view.paused);
        assert_eq!(view.pause_reason.as_deref(), Some("turn_interrupted"));
        assert_eq!(
            view.message.as_deref(),
            Some("上一轮已中断，mailbox 已暂停。")
        );
        assert!(view.can_resume);
    }

    #[test]
    fn mailbox_state_view_hides_empty_paused_prompt() {
        let state = agentdash_domain::agent_run_mailbox::AgentRunMailboxState {
            run_id: Uuid::new_v4(),
            agent_id: Uuid::new_v4(),
            runtime_session_id: "runtime-1".to_string(),
            paused: true,
            pause_reason: Some("turn_interrupted".to_string()),
            pause_message: Some("上一轮已中断，mailbox 已暂停。".to_string()),
            updated_at: chrono::Utc::now(),
        };
        let view = mailbox_state_view(Some(&state), true, 0, false);

        assert!(!view.paused);
        assert!(!view.can_resume);
        assert_eq!(view.pause_reason.as_deref(), Some("turn_interrupted"));
    }
}
