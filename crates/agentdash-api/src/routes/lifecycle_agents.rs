use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use agentdash_application::session::{
    AgentRunMailboxControlCommand, AgentRunMailboxService, AgentRunMailboxUserMessageCommand,
};
use agentdash_application::workflow::agent_run_workspace as app_workspace;
use agentdash_contracts::agent_run_mailbox::{
    AgentRunCommandReceipt, AgentRunComposerSubmitRequest, AgentRunMailboxMessageContentView,
    AgentRunMailboxMoveRequest, AgentRunMailboxView, AgentRunMessageCommandResponse,
    MailboxStateView,
};
use agentdash_contracts::workflow::{
    AgentFrameRefDto, AgentFrameRuntimeView, AgentRunCommandOnlyRequest, AgentRunLineageRef,
    AgentRunRefDto, AgentRunWorkspaceControlPlaneStatus, AgentRunWorkspaceControlPlaneView,
    AgentRunWorkspaceListEntry, AgentRunWorkspaceListView, AgentRunWorkspaceShell,
    AgentRunWorkspaceView, ConversationExecutionStatus, LifecycleRunRefDto, RuntimeSessionRefDto,
    RuntimeSessionTraceMeta,
};
use agentdash_domain::workflow::{
    AgentLineage, AgentRunAcceptedRefs, AgentRunCommandClaim, AgentRunCommandKind,
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
            agent_run_message_command_response, mailbox_message_view, mailbox_message_visible,
            mailbox_state_view,
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
        if agents.is_empty() {
            continue;
        }

        // 一次取回该 run 的全部 lineage 边，内存构建控制树 forest。
        // 主从真值源是 AgentLineage，不依赖 agent_role。
        let lineages = state
            .repos
            .agent_lineage_repo
            .list_by_run(run.id)
            .await
            .map_err(ApiError::from)?;
        let (children_map, child_ids) = build_lineage_forest(&lineages);

        // 只对主 Run（控制树 root = 未作为任何 lineage child 出现的 agent）产出 entry，
        // 走轻量列表投影（跳过 vfs / conversation 等重量级解析）。
        for agent in agents.iter().filter(|agent| !child_ids.contains(&agent.id)) {
            let subagent_count = count_descendants(agent.id, &children_map);
            let projection =
                load_agent_run_list_projection(&state, run.clone(), agent.clone()).await?;
            entries.push(list_entry_from_projection(&run, projection, subagent_count));
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
    let mut view =
        agent_run_workspace_view(load_agent_run_workspace_snapshot(&state, &context).await?);
    let (parent, children) =
        resolve_agent_run_lineage(&state, &context.run, &context.agent).await?;
    view.parent = parent;
    view.children = children;
    Ok(Json(view))
}

/// 解析当前 AgentRun 的 lineage 一跳父子，供右侧会话栏展示从属关系与跳转。
///
/// 一次 `list_by_run` 拿全 run 的 lineage 边，内存建 forest：parent 至多一个，
/// children 为直接子节点；每个 ref 附其子树后代数（`subagent_count`）供前端决定是否可下钻。
/// 标题走轻量列表投影，保证与列表/header 一致且不再为取标题做完整详情快照。
async fn resolve_agent_run_lineage(
    state: &AppState,
    run: &LifecycleRun,
    agent: &LifecycleAgent,
) -> Result<(Option<AgentRunLineageRef>, Vec<AgentRunLineageRef>), ApiError> {
    let lineages = state
        .repos
        .agent_lineage_repo
        .list_by_run(run.id)
        .await
        .map_err(ApiError::from)?;
    let (children_map, _) = build_lineage_forest(&lineages);

    let parent = match lineages
        .iter()
        .find(|lineage| lineage.child_agent_id == agent.id)
        .and_then(|lineage| lineage.parent_agent_id.map(|id| (id, lineage.relation_kind.clone())))
    {
        Some((parent_agent_id, relation_kind)) => {
            match state
                .repos
                .lifecycle_agent_repo
                .get(parent_agent_id)
                .await
                .map_err(ApiError::from)?
            {
                Some(parent_agent) => {
                    let subagent_count = count_descendants(parent_agent.id, &children_map);
                    Some(
                        lineage_ref_for_agent(
                            state,
                            run,
                            &parent_agent,
                            relation_kind,
                            subagent_count,
                        )
                        .await?,
                    )
                }
                None => None,
            }
        }
        None => None,
    };

    let mut children = Vec::new();
    for lineage in lineages
        .iter()
        .filter(|lineage| lineage.parent_agent_id == Some(agent.id))
    {
        if let Some(child_agent) = state
            .repos
            .lifecycle_agent_repo
            .get(lineage.child_agent_id)
            .await
            .map_err(ApiError::from)?
        {
            let subagent_count = count_descendants(child_agent.id, &children_map);
            children.push(
                lineage_ref_for_agent(
                    state,
                    run,
                    &child_agent,
                    lineage.relation_kind.clone(),
                    subagent_count,
                )
                .await?,
            );
        }
    }

    Ok((parent, children))
}

/// 为 lineage 上的某 agent 构建一跳引用（标题走轻量投影，避免完整详情快照）。
async fn lineage_ref_for_agent(
    state: &AppState,
    run: &LifecycleRun,
    agent: &LifecycleAgent,
    relation_kind: String,
    subagent_count: u32,
) -> Result<AgentRunLineageRef, ApiError> {
    let projection =
        load_agent_run_list_projection(state, run.clone(), agent.clone()).await?;
    Ok(AgentRunLineageRef {
        run_id: agent.run_id.to_string(),
        agent_id: agent.id.to_string(),
        agent_kind: agent.agent_kind.clone(),
        agent_role: agent.agent_role.clone(),
        relation_kind,
        display_title: projection.shell.display_title,
        subagent_count,
    })
}

pub async fn submit_agent_run_composer_input(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path((run_id, agent_id)): Path<(String, String)>,
    Json(req): Json<AgentRunComposerSubmitRequest>,
) -> Result<Json<AgentRunMessageCommandResponse>, ApiError> {
    tracing::debug!(
        run_id = %run_id,
        agent_id = %agent_id,
        input_blocks = req.input.len(),
        "AgentRun composer submit entered"
    );
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
    tracing::debug!(
        run_id = %context.run.id,
        agent_id = %context.agent.id,
        runtime_session_id = %runtime_session_id,
        "AgentRun composer submit context resolved"
    );
    agent_run_workspace_command_policy(state.as_ref())
        .ensure_composer_submit_allowed(
            command_policy_context(&context, &runtime_session_id),
            &req.command,
        )
        .await
        .map_err(command_policy_error)?;
    tracing::debug!(
        run_id = %context.run.id,
        agent_id = %context.agent.id,
        runtime_session_id = %runtime_session_id,
        "AgentRun composer submit policy accepted"
    );
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
            runtime_session_id: runtime_session_id.clone(),
            source: agentdash_domain::agent_run_mailbox::MailboxMessageSource::Composer,
            schedule_on_submit: true,
            input: req.input,
            client_command_id: req.client_command_id,
            executor_config,
            identity: Some(current_user),
            delivery_intent: req.delivery_intent,
        })
        .await
        .map_err(ApiError::from)?;
    tracing::debug!(
        run_id = %context.run.id,
        agent_id = %context.agent.id,
        runtime_session_id = %runtime_session_id,
        outcome = ?response.outcome,
        "AgentRun composer submit mailbox accepted"
    );
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

/// 轻量列表投影：列表/lineage ref 共用，避免为每个主 Run 走完整详情快照。
async fn load_agent_run_list_projection(
    state: &AppState,
    run: LifecycleRun,
    agent: LifecycleAgent,
) -> Result<app_workspace::AgentRunListProjection, ApiError> {
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
        .resolve_list_projection(app_workspace::AgentRunWorkspaceQueryInput { run, agent })
        .await
        .map_err(ApiError::from)
}

/// 从 run 的全部 lineage 边构建控制树邻接（parent -> [child]）与 child id 集合。
/// root = 未作为任何 lineage child 出现的 agent。
fn build_lineage_forest(
    lineages: &[AgentLineage],
) -> (HashMap<Uuid, Vec<Uuid>>, HashSet<Uuid>) {
    let mut children_map: HashMap<Uuid, Vec<Uuid>> = HashMap::new();
    let mut child_ids: HashSet<Uuid> = HashSet::new();
    for lineage in lineages {
        if let Some(parent_agent_id) = lineage.parent_agent_id {
            children_map
                .entry(parent_agent_id)
                .or_default()
                .push(lineage.child_agent_id);
        }
        child_ids.insert(lineage.child_agent_id);
    }
    (children_map, child_ids)
}

fn shell_model_to_contract(
    shell: app_workspace::AgentRunWorkspaceShellModel,
) -> AgentRunWorkspaceShell {
    AgentRunWorkspaceShell {
        display_title: shell.display_title,
        title_source: shell.title_source,
        workspace_status: shell.workspace_status,
        delivery_status: shell.delivery_status,
        last_turn_id: shell.last_turn_id,
        last_activity_at: shell.last_activity_at,
    }
}

fn list_entry_from_projection(
    run: &LifecycleRun,
    projection: app_workspace::AgentRunListProjection,
    subagent_count: u32,
) -> AgentRunWorkspaceListEntry {
    AgentRunWorkspaceListEntry {
        run_ref: LifecycleRunRefDto {
            run_id: run.id.to_string(),
        },
        agent_ref: AgentRunRefDto {
            run_id: run.id.to_string(),
            agent_id: projection.agent.id.to_string(),
        },
        project_id: run.project_id.to_string(),
        shell: shell_model_to_contract(projection.shell),
        run_status: lifecycle_run_status_to_contract(run.status),
        agent_role: projection.agent_role,
        subagent_count,
        delivery_runtime_ref: projection
            .delivery_runtime_session_id
            .map(|runtime_session_id| RuntimeSessionRefDto { runtime_session_id }),
        delivery_trace_meta: projection
            .delivery_trace_meta
            .map(workspace_trace_meta_to_contract),
        // 列表 UI 不消费 frame_ref，省去 frame runtime 解析。
        frame_ref: None,
        subject_ref: projection.subject_ref,
        subject_label: projection.subject_label,
    }
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
        // lineage 由 get_agent_run_workspace 单独填充（列表路径不需要，保持默认）。
        parent: None,
        children: Vec::new(),
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

/// 统计控制树某 root 子树（传递闭包）下的 subagent 总数。
///
/// lineage 支持任意深度递归且无环检测，因此遍历带 `visited` 防环 + 深度上限兜底，
/// 超限截断并 warn（不静默丢弃）。root 自身不计入。
fn count_descendants(root: Uuid, children_map: &HashMap<Uuid, Vec<Uuid>>) -> u32 {
    const MAX_DEPTH: usize = 64;
    let mut visited: HashSet<Uuid> = HashSet::new();
    let mut stack: Vec<(Uuid, usize)> = vec![(root, 0)];
    let mut count: u32 = 0;
    while let Some((node, depth)) = stack.pop() {
        if depth >= MAX_DEPTH {
            tracing::warn!(
                root = %root,
                node = %node,
                depth,
                "agent lineage 子树超过最大深度，截断后代计数"
            );
            continue;
        }
        let Some(children) = children_map.get(&node) else {
            continue;
        };
        for &child in children {
            if visited.insert(child) {
                count += 1;
                stack.push((child, depth + 1));
            }
        }
    }
    count
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

    #[test]
    fn list_entry_from_projection_carries_role_and_count() {
        let run = LifecycleRun::new_graphless(Uuid::new_v4());
        let agent = LifecycleAgent::new_root(run.id, run.project_id, "PI_AGENT");
        let projection = app_workspace::AgentRunListProjection {
            run: run.clone(),
            agent,
            shell: app_workspace::AgentRunWorkspaceShellModel {
                display_title: "Session meta title".to_string(),
                title_source: "source".to_string(),
                workspace_status: "running".to_string(),
                delivery_status: "idle".to_string(),
                last_turn_id: None,
                last_activity_at: "2026-06-12T00:00:00Z".to_string(),
            },
            agent_role: "primary".to_string(),
            delivery_runtime_session_id: None,
            delivery_trace_meta: None,
            subject_ref: None,
            subject_label: None,
        };

        let entry = list_entry_from_projection(&run, projection, 3);

        assert_eq!(entry.shell.display_title, "Session meta title");
        assert_eq!(entry.shell.title_source, "source");
        assert_eq!(entry.agent_role, "primary");
        assert_eq!(entry.subagent_count, 3);
        assert!(entry.frame_ref.is_none());
    }

    #[test]
    fn build_lineage_forest_identifies_roots() {
        let run_id = Uuid::new_v4();
        let root = Uuid::new_v4();
        let child = Uuid::new_v4();
        let grandchild = Uuid::new_v4();
        let lineages = vec![
            AgentLineage::new(run_id, Some(root), child, "spawn", None, None),
            AgentLineage::new(run_id, Some(child), grandchild, "spawn", None, None),
        ];

        let (children_map, child_ids) = build_lineage_forest(&lineages);

        assert!(!child_ids.contains(&root));
        assert!(child_ids.contains(&child));
        assert!(child_ids.contains(&grandchild));
        assert_eq!(count_descendants(root, &children_map), 2);
    }

    #[test]
    fn count_descendants_counts_full_subtree() {
        // root -> a -> c ; root -> b
        let root = Uuid::new_v4();
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        let c = Uuid::new_v4();
        let mut map: HashMap<Uuid, Vec<Uuid>> = HashMap::new();
        map.insert(root, vec![a, b]);
        map.insert(a, vec![c]);

        assert_eq!(count_descendants(root, &map), 3);
        assert_eq!(count_descendants(a, &map), 1);
        assert_eq!(count_descendants(b, &map), 0);
    }

    #[test]
    fn count_descendants_guards_against_cycle() {
        // 人为构造环 a -> b -> a，遍历不应死循环或重复计数。
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        let mut map: HashMap<Uuid, Vec<Uuid>> = HashMap::new();
        map.insert(a, vec![b]);
        map.insert(b, vec![a]);

        assert_eq!(count_descendants(a, &map), 2);
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
