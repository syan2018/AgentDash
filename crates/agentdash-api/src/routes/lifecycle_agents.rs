use agentdash_diagnostics::{DiagnosticErrorContext, Subsystem, diag, diag_error};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use agentdash_agent::MessageRef;
use agentdash_application::runtime_session_agent_run_bridge::{
    agent_run_session_cancel_runtime, agent_run_session_control, agent_run_session_core,
    agent_run_session_eventing, agent_run_session_launch,
};
use agentdash_application_agentrun::AgentRunRepositorySet;
use agentdash_application_agentrun::agent_run::{
    self as app_agent_run, workspace as app_workspace,
};
use agentdash_application_agentrun::agent_run::{
    AgentConversationContentPartModel, AgentConversationFeedInput,
    AgentConversationFeedMessageModel, AgentConversationFeedModel, AgentConversationFeedProjector,
    AgentConversationMessageRoleModel,
};
use agentdash_application_agentrun::agent_run::{
    AgentRunCancelCommand, AgentRunCancelCommandService, AgentRunCommandReceiptView,
    AgentRunDeleteCommand, AgentRunDeleteCommandService, AgentRunDeleteRepos, AgentRunForkCommand,
    AgentRunForkCommandResult, AgentRunForkService, AgentRunForkSubmitCommand,
    AgentRunMailboxControlCommand, AgentRunMailboxService, AgentRunMailboxUserMessageCommand,
    AgentRunTerminalLaunchTarget, DeliveryRuntimeSelectionError, DeliveryRuntimeSelectionService,
    ProjectAgentRunStartRepos,
};
use agentdash_application_lifecycle::AgentRunLifecycleSurfaceProjector;
use agentdash_application_runtime_session::session::terminal_cache::TerminalState;
use agentdash_contracts::agent_run_mailbox::{
    AgentRunCommandReceipt, AgentRunComposerSubmitRequest, AgentRunForkLineageView,
    AgentRunForkOutcomeView, AgentRunForkRequest, AgentRunForkResponse, AgentRunForkSubmitRequest,
    AgentRunMailboxMessageContentView, AgentRunMailboxMoveRequest, AgentRunMailboxView,
    AgentRunMessageCommandResponse, AgentRunToolCallApprovalResponse,
    AgentRunToolCallRejectionResponse, MailboxMessageView, MailboxStateView,
};
use agentdash_contracts::session::SessionMessageRefDto;
use agentdash_contracts::workflow::{
    AgentConversationContentPartView, AgentConversationFeedMessage, AgentConversationFeedSnapshot,
    AgentConversationIdentity, AgentConversationLifecycleContext, AgentConversationMessageRefView,
    AgentConversationMessageRole, AgentConversationSnapshot, AgentConversationSourceRangeView,
    AgentConversationToolCallView, AgentConversationToolResultView, AgentFrameRefDto,
    AgentFrameRuntimeView, AgentRunCommandOnlyRequest, AgentRunCommandPreconditionView,
    AgentRunLineageRef, AgentRunListChild, AgentRunOwnershipView, AgentRunRefDto,
    AgentRunResourceSurfaceCoordinateView, AgentRunResourceSurfaceSourceAnchorView, AgentRunView,
    AgentRunWorkspaceControlPlaneStatus, AgentRunWorkspaceControlPlaneView,
    AgentRunWorkspaceListEntry, AgentRunWorkspaceListView, AgentRunWorkspaceShell,
    AgentRunWorkspaceView, ConversationCommandKind, ConversationCommandPlacement,
    ConversationCommandSetView, ConversationCommandStaleGuardView, ConversationCommandView,
    ConversationDiagnosticView, ConversationEffectiveExecutorConfigView,
    ConversationExecutionStatus, ConversationExecutionView, ConversationKeyboardMapView,
    ConversationMailboxSnapshotView, ConversationModelConfigSource, ConversationModelConfigStatus,
    ConversationModelConfigView, ConversationWaitingItemView, DeleteAgentRunResponse,
    LifecycleRunRefDto, LifecycleSubjectAssociationDto, RuntimeSessionRefDto,
    RuntimeSessionTraceMeta, SessionRuntimeControlView, SubjectRefDto, ValidationSeverity,
};
use agentdash_domain::workflow::{AgentLineage, LifecycleAgent, LifecycleRun};
use agentdash_spi::AgentConfig;
use axum::{
    Json,
    extract::{Path, Query, State},
    http::HeaderMap,
    response::IntoResponse,
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::dto::{
    ContextAuditQuery, NdjsonStreamQuery, RejectToolApprovalRequest, SessionEventsQuery,
    SpawnTerminalBody,
};
use crate::{
    agent_run_runtime_surface::resolve_terminal_launch_target_for_runtime_session,
    app_state::AppState,
    auth::{CurrentUser, ProjectPermission, load_project_with_permission},
    routes::{
        agent_run_mailbox_contracts::{
            agent_run_message_accepted_refs, agent_run_message_command_response,
            backend_selection_input, command_receipt_view, mailbox_command_outcome_view,
            mailbox_message_view, mailbox_message_visible, mailbox_state_view,
        },
        sessions, terminals,
        vfs_surfaces::dto as vfs_surface_dto,
    },
    rpc::{ApiError, ApiErrorWithCode},
    vfs_surface_runtime::ApiVfsSurfaceRuntimeProjection,
};

struct AgentRunContext {
    run: LifecycleRun,
    agent: LifecycleAgent,
    delivery_runtime_session_id: Option<String>,
    delivery_frame_id: Option<Uuid>,
}

struct AgentRunDeliveryRuntimeContext {
    runtime_session_id: String,
    frame_id: Uuid,
}

pub fn router() -> axum::Router<Arc<AppState>> {
    axum::Router::new()
        .route(
            "/projects/{project_id}/agent-runs",
            axum::routing::get(get_project_agent_runs),
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
            "/agent-runs/{run_id}/agents/{agent_id}/conversation/feed",
            axum::routing::get(get_agent_run_conversation_feed),
        )
        .route(
            "/agent-runs/{run_id}/agents/{agent_id}/composer-submit",
            axum::routing::post(submit_agent_run_composer_input),
        )
        .route(
            "/agent-runs/{run_id}/agents/{agent_id}/fork",
            axum::routing::post(fork_agent_run),
        )
        .route(
            "/agent-runs/{run_id}/agents/{agent_id}/fork-submit",
            axum::routing::post(fork_submit_agent_run),
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
        .route(
            "/agent-runs/{run_id}/agents/{agent_id}/runtime/control",
            axum::routing::get(get_agent_run_runtime_control),
        )
        .route(
            "/agent-runs/{run_id}/agents/{agent_id}/runtime/events",
            axum::routing::get(list_agent_run_runtime_events),
        )
        .route(
            "/agent-runs/{run_id}/agents/{agent_id}/runtime/terminals",
            axum::routing::get(list_agent_run_runtime_terminals)
                .post(spawn_agent_run_runtime_terminal),
        )
        .route(
            "/agent-runs/{run_id}/agents/{agent_id}/runtime/context/projection",
            axum::routing::get(get_agent_run_runtime_context_projection),
        )
        .route(
            "/agent-runs/{run_id}/agents/{agent_id}/runtime/context/audit",
            axum::routing::get(get_agent_run_runtime_context_audit),
        )
        .route(
            "/agent-runs/{run_id}/agents/{agent_id}/runtime/stream/ndjson",
            axum::routing::get(agent_run_runtime_stream_ndjson),
        )
        .route(
            "/agent-runs/{run_id}/agents/{agent_id}/runtime/tool-approvals/{tool_call_id}/approve",
            axum::routing::post(approve_agent_run_tool_call),
        )
        .route(
            "/agent-runs/{run_id}/agents/{agent_id}/runtime/tool-approvals/{tool_call_id}/reject",
            axum::routing::post(reject_agent_run_tool_call),
        )
}

pub async fn get_project_agent_runs(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(project_id): Path<String>,
    Query(query): Query<AgentRunListQuery>,
) -> Result<Json<AgentRunWorkspaceListView>, ApiError> {
    let project_id = parse_uuid(&project_id, "project_id")?;
    load_project_with_permission(
        state.as_ref(),
        &current_user,
        project_id,
        ProjectPermission::Use,
    )
    .await?;

    let limit = query
        .limit
        .map(|l| l as usize)
        .unwrap_or(DEFAULT_PAGE_LIMIT)
        .clamp(1, MAX_PAGE_LIMIT);
    let cursor = query.cursor.as_deref().and_then(decode_cursor);

    // keyset 分页：按 run 级 last_activity_at desc（run_id desc tiebreak）稳定排序，
    // 仅对**页内** run 跑投影——成本随页大小而非项目历史 Run 总量增长。
    let mut runs = state
        .repos
        .lifecycle_run_repo
        .list_by_project(project_id)
        .await
        .map_err(ApiError::from)?;
    // 排序键与游标 keyset 同为毫秒粒度，避免亚毫秒同刻在分页边界错位。
    runs.sort_by(|a, b| {
        b.last_activity_at
            .timestamp_millis()
            .cmp(&a.last_activity_at.timestamp_millis())
            .then_with(|| b.id.cmp(&a.id))
    });
    if let Some((cur_millis, cur_id)) = cursor {
        // 严格排在游标项之后（desc 序）的 run。
        runs.retain(|run| (run.last_activity_at.timestamp_millis(), run.id) < (cur_millis, cur_id));
    }

    let total = runs.len();
    let mut entries = Vec::new();
    let mut next_cursor = None;
    for (idx, run) in runs.iter().enumerate() {
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
        // 主从真值源是 AgentLineage 控制树。
        let lineages = state
            .repos
            .agent_lineage_repo
            .list_by_run(run.id)
            .await
            .map_err(ApiError::from)?;
        let (children_map, child_ids) = build_lineage_forest(&lineages);

        // 只对主 Run（控制树 root = 未作为任何 lineage child 出现的 agent）产出 entry，
        // 并内联其直接子 Agent（一跳），均走轻量列表投影。
        for agent in agents.iter().filter(|agent| !child_ids.contains(&agent.id)) {
            let subagent_count = count_descendants(agent.id, &children_map);
            let projection =
                load_agent_run_list_projection(&state, run.clone(), agent.clone()).await?;
            let children =
                build_inline_children(&state, run, agent.id, &agents, &children_map, 0).await?;
            entries.push(list_entry_from_projection(
                run,
                projection,
                subagent_count,
                children,
            ));
        }

        // 按 run 分页：本 run 全部 entry 产出后若已达页大小，游标指向本 run，下一页从其后开始。
        if entries.len() >= limit {
            if idx + 1 < total {
                next_cursor = Some(encode_cursor(run.last_activity_at, run.id));
            }
            break;
        }
    }

    Ok(Json(AgentRunWorkspaceListView {
        project_id: project_id.to_string(),
        agent_runs: entries,
        next_cursor,
    }))
}

pub async fn delete_project_agent_run(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path((project_id, run_id)): Path<(String, String)>,
) -> Result<Json<DeleteAgentRunResponse>, ApiError> {
    let project_id = parse_uuid(&project_id, "project_id")?;
    let run_id = parse_uuid(&run_id, "run_id")?;
    load_project_with_permission(
        state.as_ref(),
        &current_user,
        project_id,
        ProjectPermission::Use,
    )
    .await?;

    let agent_run_repos = state.repos.to_agent_run_repository_set();
    let service = AgentRunDeleteCommandService::new(
        AgentRunDeleteRepos::from_repository_set(&agent_run_repos),
        agent_run_session_core(state.services.session_core.clone()),
    );
    let outcome = service
        .delete(AgentRunDeleteCommand { project_id, run_id })
        .await
        .map_err(ApiError::from)?;

    Ok(Json(DeleteAgentRunResponse {
        deleted: outcome.deleted,
        project_id: outcome.project_id.to_string(),
        run_id: outcome.run_id.to_string(),
    }))
}

/// AgentRun 列表分页查询参数。
#[derive(serde::Deserialize)]
pub struct AgentRunListQuery {
    pub limit: Option<u32>,
    pub cursor: Option<String>,
}

const DEFAULT_PAGE_LIMIT: usize = 30;
const MAX_PAGE_LIMIT: usize = 100;

/// keyset 游标：编码页尾 run 的 (last_activity_at_millis, run_id)，base64 不透明串。
fn encode_cursor(last_activity_at: DateTime<Utc>, run_id: Uuid) -> String {
    let raw = format!("{}:{}", last_activity_at.timestamp_millis(), run_id);
    URL_SAFE_NO_PAD.encode(raw.as_bytes())
}

fn decode_cursor(cursor: &str) -> Option<(i64, Uuid)> {
    let bytes = URL_SAFE_NO_PAD.decode(cursor.as_bytes()).ok()?;
    let raw = String::from_utf8(bytes).ok()?;
    let (millis, run_id) = raw.split_once(':')?;
    Some((millis.parse().ok()?, Uuid::parse_str(run_id).ok()?))
}

/// 递归内联某节点的直接子 Agent 子树，每个节点携带真实 shell 状态。
///
/// forest（`children_map`）已在 list 循环内建好，取子节点零额外 repo 查询；仅投影是新增异步调用。
/// 深度上限保护 lineage 环 / 异常深树（与 `count_descendants` 同语义）。async 递归经 `Box::pin`。
fn build_inline_children<'a>(
    state: &'a AppState,
    run: &'a LifecycleRun,
    parent_id: Uuid,
    agents: &'a [LifecycleAgent],
    children_map: &'a HashMap<Uuid, Vec<Uuid>>,
    depth: usize,
) -> std::pin::Pin<
    Box<dyn std::future::Future<Output = Result<Vec<AgentRunListChild>, ApiError>> + Send + 'a>,
> {
    Box::pin(async move {
        const MAX_DEPTH: usize = 16;
        let mut children = Vec::new();
        if depth >= MAX_DEPTH {
            diag!(Warn, Subsystem::Api,
        run_id = %run.id, parent = %parent_id, depth, "inline children 触达深度上限，截断");
            return Ok(children);
        }
        let Some(direct) = children_map.get(&parent_id) else {
            return Ok(children);
        };
        for child_id in direct {
            let Some(child_agent) = agents.iter().find(|a| a.id == *child_id) else {
                continue;
            };
            let subagent_count = count_descendants(child_agent.id, children_map);
            let projection =
                load_agent_run_list_projection(state, run.clone(), child_agent.clone()).await?;
            let nested =
                build_inline_children(state, run, child_agent.id, agents, children_map, depth + 1)
                    .await?;
            let mut node = list_child_from_projection(run, projection, subagent_count);
            node.children = nested;
            children.push(node);
        }
        children.sort_by(|a, b| b.shell.last_activity_at.cmp(&a.shell.last_activity_at));
        Ok(children)
    })
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
        ProjectPermission::Use,
    )
    .await?;
    let mut view = agent_run_workspace_view(
        load_agent_run_workspace_snapshot(&state, &context, &current_user.user_id).await?,
    );
    let (parent, children) =
        resolve_agent_run_lineage(&state, &context.run, &context.agent).await?;
    view.parent = parent;
    view.children = children;
    Ok(Json(view))
}

pub async fn get_agent_run_conversation_feed(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path((run_id, agent_id)): Path<(String, String)>,
) -> Result<Json<AgentConversationFeedSnapshot>, ApiError> {
    let context = resolve_agent_run_context(
        &state,
        &current_user,
        &run_id,
        &agent_id,
        ProjectPermission::Use,
    )
    .await?;
    let runtime_session_id = context
        .delivery_runtime_session_id
        .as_deref()
        .ok_or_else(|| {
            ApiError::NotFound("AgentRun 当前没有可读取的 delivery RuntimeSession".to_string())
        })?;
    let envelope = state
        .services
        .session_eventing
        .build_agent_context_envelope(runtime_session_id)
        .await
        .map_err(ApiError::from)?;
    let model = AgentConversationFeedProjector::derive(AgentConversationFeedInput {
        run: context.run,
        agent: context.agent,
        runtime_session_id: runtime_session_id.to_string(),
        envelope,
    });

    Ok(Json(agent_conversation_feed_snapshot(model)))
}

fn agent_conversation_feed_snapshot(
    model: AgentConversationFeedModel,
) -> AgentConversationFeedSnapshot {
    let messages = model
        .messages
        .into_iter()
        .map(agent_conversation_feed_message)
        .collect::<Vec<_>>();
    AgentConversationFeedSnapshot {
        run_ref: LifecycleRunRefDto {
            run_id: model.run_id.clone(),
        },
        agent_ref: AgentRunRefDto {
            run_id: model.run_id,
            agent_id: model.agent_id,
        },
        runtime_session_ref: Some(RuntimeSessionRefDto {
            runtime_session_id: model.runtime_session_id,
        }),
        projection_kind: model.projection_kind,
        projection_version: model.projection_version,
        head_event_seq: model.head_event_seq,
        runtime_replay_start_seq: model.runtime_replay_start_seq,
        active_compaction_id: model.active_compaction_id,
        message_count: messages.len() as u64,
        messages,
    }
}

fn agent_conversation_feed_message(
    message: AgentConversationFeedMessageModel,
) -> AgentConversationFeedMessage {
    AgentConversationFeedMessage {
        message_ref: AgentConversationMessageRefView {
            turn_id: message.message_ref.turn_id,
            entry_index: message.message_ref.entry_index,
        },
        role: agent_conversation_message_role(message.role),
        text: message.text,
        content_parts: message
            .content_parts
            .into_iter()
            .map(agent_conversation_content_part)
            .collect(),
        tool_calls: message
            .tool_calls
            .into_iter()
            .map(|tool_call| AgentConversationToolCallView {
                id: tool_call.id,
                call_id: tool_call.call_id,
                name: tool_call.name,
                arguments: tool_call.arguments,
            })
            .collect(),
        tool_result: message
            .tool_result
            .map(|tool_result| AgentConversationToolResultView {
                tool_call_id: tool_result.tool_call_id,
                call_id: tool_result.call_id,
                tool_name: tool_result.tool_name,
                details: tool_result.details,
                is_error: tool_result.is_error,
            }),
        origin: message.origin,
        synthetic: message.synthetic,
        projection_kind: message.projection_kind,
        source_event_seq: message.source_event_seq,
        source_range: message
            .source_range
            .map(|range| AgentConversationSourceRangeView {
                start_event_seq: range.start_event_seq,
                end_event_seq: range.end_event_seq,
            }),
        projection_segment_id: message.projection_segment_id,
        timestamp_ms: message.timestamp_ms,
    }
}

fn agent_conversation_message_role(
    role: AgentConversationMessageRoleModel,
) -> AgentConversationMessageRole {
    match role {
        AgentConversationMessageRoleModel::User => AgentConversationMessageRole::User,
        AgentConversationMessageRoleModel::Assistant => AgentConversationMessageRole::Assistant,
        AgentConversationMessageRoleModel::ToolResult => AgentConversationMessageRole::ToolResult,
        AgentConversationMessageRoleModel::CompactionSummary => {
            AgentConversationMessageRole::CompactionSummary
        }
    }
}

fn agent_conversation_content_part(
    part: AgentConversationContentPartModel,
) -> AgentConversationContentPartView {
    match part {
        AgentConversationContentPartModel::Text { text } => {
            AgentConversationContentPartView::Text { text }
        }
        AgentConversationContentPartModel::Image { mime_type, data } => {
            AgentConversationContentPartView::Image { mime_type, data }
        }
        AgentConversationContentPartModel::Reasoning {
            text,
            id,
            signature,
        } => AgentConversationContentPartView::Reasoning {
            text,
            id,
            signature,
        },
    }
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

    let same_run_parent = match lineages
        .iter()
        .find(|lineage| lineage.child_agent_id == agent.id)
        .and_then(|lineage| {
            lineage
                .parent_agent_id
                .map(|id| (id, lineage.relation_kind.clone()))
        }) {
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
    let parent = same_run_parent;

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
    let projection = load_agent_run_list_projection(state, run.clone(), agent.clone()).await?;
    Ok(AgentRunLineageRef {
        run_id: agent.run_id.to_string(),
        agent_id: agent.id.to_string(),
        source: agent.source.as_str().to_string(),
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
    diag!(Debug, Subsystem::Api,

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
        ProjectPermission::Use,
    )
    .await?;
    let runtime_session_id = context.delivery_runtime_session_id.clone().ok_or_else(|| {
        ApiError::Conflict(format!(
            "AgentRun {} / {} 缺少 delivery runtime",
            context.run.id, context.agent.id
        ))
    })?;
    let frame_id = delivery_frame_id_from_agent_run_context(&context)?;
    diag!(Debug, Subsystem::Api,

        run_id = %context.run.id,
        agent_id = %context.agent.id,
        runtime_session_id = %runtime_session_id,
        "AgentRun composer submit context resolved"
    );
    let agent_run_repos = state.repos.to_agent_run_repository_set();
    agent_run_workspace_command_policy(state.as_ref(), &agent_run_repos)
        .ensure_composer_submit_allowed(
            command_policy_context(&context, &runtime_session_id),
            &command_precondition_to_application(req.command),
        )
        .await
        .map_err(command_policy_error)?;
    diag!(Debug, Subsystem::Api,

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
    if context.run.created_by_user_id != current_user.user_id {
        let service = agent_run_fork_service(state.as_ref(), &agent_run_repos);
        let current_user_id = current_user.user_id.clone();
        let client_command_id = req.client_command_id.clone();
        let response = service
            .fork_submit(AgentRunForkSubmitCommand {
                parent_run_id: context.run.id,
                parent_agent_id: context.agent.id,
                current_user_id: current_user_id.clone(),
                title: None,
                fork_point_ref: None,
                metadata_json: None,
                input: req.input,
                client_command_id: req.client_command_id,
                executor_config,
                backend_selection: backend_selection_input(req.backend_selection),
                identity: Some(current_user),
            })
            .await
            .map_err(|error| {
                log_agent_run_fork_route_error(
                    "composer-submit:auto-fork",
                    context.run.id,
                    context.agent.id,
                    &current_user_id,
                    &client_command_id,
                    None,
                    &error,
                );
                ApiError::from(error)
            })?;
        return Ok(Json(agent_run_fork_submit_message_response(response)));
    }
    let service = agent_run_mailbox_service(state.as_ref(), &agent_run_repos);
    let response = service
        .accept_user_message(AgentRunMailboxUserMessageCommand {
            run_id: context.run.id,
            agent_id: context.agent.id,
            frame_id,
            source: agentdash_domain::agent_run_mailbox::MailboxSourceIdentity::composer(),
            schedule_on_submit: true,
            input: req.input,
            client_command_id: req.client_command_id,
            executor_config,
            backend_selection: backend_selection_input(req.backend_selection),
            identity: Some(current_user),
            delivery_intent: req.delivery_intent,
        })
        .await
        .map_err(ApiError::from)?;
    diag!(Debug, Subsystem::Api,

        run_id = %context.run.id,
        agent_id = %context.agent.id,
        runtime_session_id = %runtime_session_id,
        outcome = ?response.outcome,
        "AgentRun composer submit mailbox accepted"
    );
    Ok(Json(agent_run_message_command_response(response)))
}

async fn fork_agent_run(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path((run_id, agent_id)): Path<(String, String)>,
    Json(req): Json<AgentRunForkRequest>,
) -> Result<Json<AgentRunForkResponse>, ApiError> {
    if req.client_command_id.trim().is_empty() {
        return Err(ApiError::BadRequest(
            "client_command_id 不能为空".to_string(),
        ));
    }
    let context = resolve_agent_run_context(
        &state,
        &current_user,
        &run_id,
        &agent_id,
        ProjectPermission::Use,
    )
    .await?;
    let agent_run_repos = state.repos.to_agent_run_repository_set();
    let service = agent_run_fork_service(state.as_ref(), &agent_run_repos);
    let current_user_id = current_user.user_id.clone();
    let client_command_id = req.client_command_id.clone();
    let fork_point_ref = req.fork_point_ref.map(message_ref_from_contract);
    let result = service
        .explicit_fork(AgentRunForkCommand {
            parent_run_id: context.run.id,
            parent_agent_id: context.agent.id,
            current_user_id: current_user_id.clone(),
            title: req.title,
            fork_point_ref: fork_point_ref.clone(),
            metadata_json: req.metadata_json,
            client_command_id: req.client_command_id,
        })
        .await
        .map_err(|error| {
            log_agent_run_fork_route_error(
                "fork-agent-run",
                context.run.id,
                context.agent.id,
                &current_user_id,
                &client_command_id,
                fork_point_ref.as_ref(),
                &error,
            );
            ApiError::from(error)
        })?;
    Ok(Json(agent_run_fork_response(result)))
}

async fn fork_submit_agent_run(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path((run_id, agent_id)): Path<(String, String)>,
    Json(req): Json<AgentRunForkSubmitRequest>,
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
        ProjectPermission::Use,
    )
    .await?;
    let executor_config = req
        .executor_config
        .map(serde_json::from_value::<AgentConfig>)
        .transpose()
        .map_err(|e| ApiError::BadRequest(format!("executor_config 格式错误: {e}")))?;
    let agent_run_repos = state.repos.to_agent_run_repository_set();
    let service = agent_run_fork_service(state.as_ref(), &agent_run_repos);
    let current_user_id = current_user.user_id.clone();
    let client_command_id = req.client_command_id.clone();
    let fork_point_ref = req.fork_point_ref.map(message_ref_from_contract);
    let result = service
        .fork_submit(AgentRunForkSubmitCommand {
            parent_run_id: context.run.id,
            parent_agent_id: context.agent.id,
            current_user_id: current_user_id.clone(),
            title: req.title,
            fork_point_ref: fork_point_ref.clone(),
            metadata_json: req.metadata_json,
            input: req.input,
            client_command_id: req.client_command_id,
            executor_config,
            backend_selection: backend_selection_input(req.backend_selection),
            identity: Some(current_user),
        })
        .await
        .map_err(|error| {
            log_agent_run_fork_route_error(
                "fork-submit-agent-run",
                context.run.id,
                context.agent.id,
                &current_user_id,
                &client_command_id,
                fork_point_ref.as_ref(),
                &error,
            );
            ApiError::from(error)
        })?;
    Ok(Json(agent_run_fork_submit_message_response(result)))
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
        ProjectPermission::Use,
    )
    .await?;
    Ok(Json(
        build_agent_run_mailbox_view(state.as_ref(), &context, &current_user.user_id).await?,
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
        ProjectPermission::Use,
    )
    .await?;
    let runtime_session_id = context.delivery_runtime_session_id.clone().ok_or_else(|| {
        ApiError::Conflict(format!(
            "AgentRun {} / {} 缺少 delivery runtime",
            context.run.id, context.agent.id
        ))
    })?;
    let frame_id = delivery_frame_id_from_agent_run_context(&context)?;
    let agent_run_repos = state.repos.to_agent_run_repository_set();
    agent_run_workspace_command_policy(state.as_ref(), &agent_run_repos)
        .ensure_command_allowed(
            command_policy_context(&context, &runtime_session_id),
            app_workspace::AgentRunWorkspaceCommandPrecondition::DeleteMailboxMessage {
                command: command_precondition_to_application(body.command.clone()),
            },
        )
        .await
        .map_err(command_policy_error)?;
    let message_id = parse_uuid(&message_id, "message_id")?;
    let response = agent_run_mailbox_service(state.as_ref(), &agent_run_repos)
        .delete_message(AgentRunMailboxControlCommand {
            run_id: context.run.id,
            agent_id: context.agent.id,
            frame_id,
            message_id: Some(message_id),
            after_message_id: None,
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
        ProjectPermission::Use,
    )
    .await?;
    let runtime_session_id = context.delivery_runtime_session_id.clone().ok_or_else(|| {
        ApiError::Conflict(format!(
            "AgentRun {} / {} 缺少 delivery runtime",
            context.run.id, context.agent.id
        ))
    })?;
    let frame_id = delivery_frame_id_from_agent_run_context(&context)?;
    let agent_run_repos = state.repos.to_agent_run_repository_set();
    agent_run_workspace_command_policy(state.as_ref(), &agent_run_repos)
        .ensure_command_allowed(
            command_policy_context(&context, &runtime_session_id),
            app_workspace::AgentRunWorkspaceCommandPrecondition::ResumeMailbox {
                command: command_precondition_to_application(body.command.clone()),
            },
        )
        .await
        .map_err(command_policy_error)?;
    let response = agent_run_mailbox_service(state.as_ref(), &agent_run_repos)
        .resume_mailbox(
            AgentRunMailboxControlCommand {
                run_id: context.run.id,
                agent_id: context.agent.id,
                frame_id,
                message_id: None,
                after_message_id: None,
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
        ProjectPermission::Use,
    )
    .await?;
    let runtime_session_id = context.delivery_runtime_session_id.clone().ok_or_else(|| {
        ApiError::Conflict(format!(
            "AgentRun {} / {} 缺少 delivery runtime",
            context.run.id, context.agent.id
        ))
    })?;
    let frame_id = delivery_frame_id_from_agent_run_context(&context)?;
    let agent_run_repos = state.repos.to_agent_run_repository_set();
    agent_run_workspace_command_policy(state.as_ref(), &agent_run_repos)
        .ensure_command_allowed(
            command_policy_context(&context, &runtime_session_id),
            app_workspace::AgentRunWorkspaceCommandPrecondition::PromoteMailboxMessage {
                command: command_precondition_to_application(body.command.clone()),
            },
        )
        .await
        .map_err(command_policy_error)?;
    let message_id = parse_uuid(&message_id, "message_id")?;
    let response = agent_run_mailbox_service(state.as_ref(), &agent_run_repos)
        .promote_message(
            AgentRunMailboxControlCommand {
                run_id: context.run.id,
                agent_id: context.agent.id,
                frame_id,
                message_id: Some(message_id),
                after_message_id: None,
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
        ProjectPermission::Use,
    )
    .await?;
    let runtime_session_id = context.delivery_runtime_session_id.clone().ok_or_else(|| {
        ApiError::Conflict(format!(
            "AgentRun {} / {} 缺少 delivery runtime",
            context.run.id, context.agent.id
        ))
    })?;
    let frame_id = delivery_frame_id_from_agent_run_context(&context)?;
    let agent_run_repos = state.repos.to_agent_run_repository_set();
    agent_run_workspace_command_policy(state.as_ref(), &agent_run_repos)
        .ensure_command_allowed(
            command_policy_context(&context, &runtime_session_id),
            app_workspace::AgentRunWorkspaceCommandPrecondition::MoveMailboxMessage {
                command: command_precondition_to_application(body.command.clone()),
            },
        )
        .await
        .map_err(command_policy_error)?;
    let message_id = parse_uuid(&message_id, "message_id")?;
    let after_message_id = body
        .after_message_id
        .as_deref()
        .map(|id| parse_uuid(id, "after_message_id"))
        .transpose()?;
    let result = agent_run_mailbox_service(state.as_ref(), &agent_run_repos)
        .move_message(AgentRunMailboxControlCommand {
            run_id: context.run.id,
            agent_id: context.agent.id,
            frame_id,
            message_id: Some(message_id),
            after_message_id,
            client_command_id: body.client_command_id,
        })
        .await
        .map_err(ApiError::from)?;
    let order_key = result.order_key;
    Ok(Json(
        serde_json::json!({ "ok": true, "order_key": order_key }),
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
        ProjectPermission::Use,
    )
    .await?;
    let message_id = parse_uuid(&message_id, "message_id")?;
    let agent_run_repos = state.repos.to_agent_run_repository_set();
    let input = agent_run_mailbox_service(state.as_ref(), &agent_run_repos)
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
        ProjectPermission::Use,
    )
    .await?;
    let runtime_session_id = context.delivery_runtime_session_id.clone().ok_or_else(|| {
        ApiError::Conflict(format!(
            "AgentRun {} / {} 缺少 delivery runtime",
            context.run.id, context.agent.id
        ))
    })?;
    let agent_run_repos = state.repos.to_agent_run_repository_set();
    agent_run_workspace_command_policy(state.as_ref(), &agent_run_repos)
        .ensure_command_allowed(
            command_policy_context(&context, &runtime_session_id),
            app_workspace::AgentRunWorkspaceCommandPrecondition::Cancel {
                command: command_precondition_to_application(body.command.clone()),
            },
        )
        .await
        .map_err(command_policy_error)?;
    let delivery_selection = DeliveryRuntimeSelectionService::from_repository_set(&agent_run_repos)
        .select_current_delivery(context.run.id, context.agent.id)
        .await
        .map_err(delivery_runtime_selection_error)?;
    let cancel_runtime = agent_run_session_cancel_runtime(state.services.session_runtime.clone());
    let receipt = AgentRunCancelCommandService::new(
        state.repos.agent_run_command_receipt_repo.as_ref(),
        &cancel_runtime,
    )
    .cancel(AgentRunCancelCommand {
        run_id: context.run.id,
        agent_id: context.agent.id,
        frame_id: Some(delivery_selection.launch_frame_id),
        runtime_session_id,
        client_command_id: body.client_command_id,
        reason: None,
    })
    .await
    .map_err(ApiError::from)?;
    Ok(Json(agent_run_command_receipt_contract(receipt)))
}

async fn get_agent_run_runtime_control(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path((run_id, agent_id)): Path<(String, String)>,
) -> Result<Json<SessionRuntimeControlView>, ApiError> {
    let context = resolve_agent_run_context(
        &state,
        &current_user,
        &run_id,
        &agent_id,
        ProjectPermission::Use,
    )
    .await?;
    let runtime_session_id = context
        .delivery_runtime_session_id
        .as_deref()
        .ok_or_else(|| {
            ApiError::NotFound("AgentRun 当前没有可读取的 delivery RuntimeSession".to_string())
        })?;
    Ok(Json(
        sessions::load_session_runtime_control_view(
            state.as_ref(),
            &current_user,
            runtime_session_id,
        )
        .await?,
    ))
}

async fn list_agent_run_runtime_events(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path((run_id, agent_id)): Path<(String, String)>,
    Query(query): Query<SessionEventsQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let runtime_session_id =
        resolve_agent_run_delivery_runtime(&state, &current_user, &run_id, &agent_id).await?;
    Ok(Json(
        sessions::load_runtime_session_events_page(state.as_ref(), &runtime_session_id, query)
            .await?,
    ))
}

async fn list_agent_run_runtime_terminals(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path((run_id, agent_id)): Path<(String, String)>,
) -> Result<Json<Vec<TerminalState>>, ApiError> {
    let (runtime_session_id, _) =
        resolve_agent_run_terminal_launch_target(&state, &current_user, &run_id, &agent_id).await?;
    Ok(Json(
        state
            .services
            .terminal_cache
            .list_terminals(&runtime_session_id),
    ))
}

async fn spawn_agent_run_runtime_terminal(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path((run_id, agent_id)): Path<(String, String)>,
    Json(body): Json<SpawnTerminalBody>,
) -> Result<impl IntoResponse, ApiError> {
    let (runtime_session_id, launch_target) =
        resolve_agent_run_terminal_launch_target(&state, &current_user, &run_id, &agent_id).await?;
    terminals::spawn_terminal_for_runtime_session(&state, &runtime_session_id, launch_target, body)
        .await
}

async fn resolve_agent_run_terminal_launch_target(
    state: &Arc<AppState>,
    current_user: &agentdash_integration_api::AuthIdentity,
    run_id: &str,
    agent_id: &str,
) -> Result<(String, AgentRunTerminalLaunchTarget), ApiError> {
    let context = resolve_agent_run_context(
        state.as_ref(),
        current_user,
        run_id,
        agent_id,
        ProjectPermission::Use,
    )
    .await?;
    let runtime_session_id = delivery_runtime_session_from_agent_run_context(&context)?;
    let launch_target =
        resolve_terminal_launch_target_for_runtime_session(&state, &runtime_session_id).await?;
    if launch_target.project_id != context.run.project_id {
        return Err(ApiError::Conflict(format!(
            "AgentRun {} / {} 与 terminal runtime surface Project 不一致",
            context.run.id, context.agent.id
        )));
    }
    Ok((runtime_session_id, launch_target.target))
}

async fn get_agent_run_runtime_context_projection(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path((run_id, agent_id)): Path<(String, String)>,
) -> Result<impl IntoResponse, ApiError> {
    let runtime_session_id =
        resolve_agent_run_delivery_runtime(&state, &current_user, &run_id, &agent_id).await?;
    Ok(Json(
        sessions::load_runtime_session_context_projection(state.as_ref(), &runtime_session_id)
            .await?,
    ))
}

async fn get_agent_run_runtime_context_audit(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path((run_id, agent_id)): Path<(String, String)>,
    Query(query): Query<ContextAuditQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let runtime_session_id =
        resolve_agent_run_delivery_runtime(&state, &current_user, &run_id, &agent_id).await?;
    Ok(Json(
        sessions::load_runtime_session_context_audit(state.as_ref(), &runtime_session_id, query)
            .await?,
    ))
}

async fn agent_run_runtime_stream_ndjson(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path((run_id, agent_id)): Path<(String, String)>,
    headers: HeaderMap,
    Query(query): Query<NdjsonStreamQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let runtime_session_id =
        resolve_agent_run_delivery_runtime(&state, &current_user, &run_id, &agent_id).await?;
    sessions::runtime_session_stream_ndjson(state.as_ref(), runtime_session_id, headers, query)
        .await
}

async fn approve_agent_run_tool_call(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path((run_id, agent_id, tool_call_id)): Path<(String, String, String)>,
) -> Result<impl IntoResponse, ApiError> {
    let context = resolve_agent_run_context(
        state.as_ref(),
        &current_user,
        &run_id,
        &agent_id,
        ProjectPermission::Use,
    )
    .await?;
    approve_tool_call_for_agent_run_delivery(state.as_ref(), &context, tool_call_id).await
}

async fn reject_agent_run_tool_call(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path((run_id, agent_id, tool_call_id)): Path<(String, String, String)>,
    Json(req): Json<RejectToolApprovalRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let context = resolve_agent_run_context(
        state.as_ref(),
        &current_user,
        &run_id,
        &agent_id,
        ProjectPermission::Use,
    )
    .await?;
    reject_tool_call_for_agent_run_delivery(state.as_ref(), &context, tool_call_id, req.reason)
        .await
}

async fn approve_tool_call_for_agent_run_delivery(
    state: &AppState,
    context: &AgentRunContext,
    tool_call_id: String,
) -> Result<Json<AgentRunToolCallApprovalResponse>, ApiError> {
    let session_id = delivery_runtime_session_from_agent_run_context(context)?;
    state
        .services
        .session_control
        .approve_tool_call(&session_id, &tool_call_id)
        .await
        .map_err(ApiError::from)?;

    Ok(Json(AgentRunToolCallApprovalResponse {
        approved: true,
        run_ref: LifecycleRunRefDto {
            run_id: context.run.id.to_string(),
        },
        agent_ref: AgentRunRefDto {
            run_id: context.run.id.to_string(),
            agent_id: context.agent.id.to_string(),
        },
        tool_call_id,
    }))
}

async fn reject_tool_call_for_agent_run_delivery(
    state: &AppState,
    context: &AgentRunContext,
    tool_call_id: String,
    reason: Option<String>,
) -> Result<Json<AgentRunToolCallRejectionResponse>, ApiError> {
    let session_id = delivery_runtime_session_from_agent_run_context(context)?;
    state
        .services
        .session_control
        .reject_tool_call(&session_id, &tool_call_id, reason)
        .await
        .map_err(ApiError::from)?;

    Ok(Json(AgentRunToolCallRejectionResponse {
        rejected: true,
        run_ref: LifecycleRunRefDto {
            run_id: context.run.id.to_string(),
        },
        agent_ref: AgentRunRefDto {
            run_id: context.run.id.to_string(),
            agent_id: context.agent.id.to_string(),
        },
        tool_call_id,
    }))
}

async fn resolve_agent_run_delivery_runtime(
    state: &Arc<AppState>,
    current_user: &agentdash_integration_api::AuthIdentity,
    run_id: &str,
    agent_id: &str,
) -> Result<String, ApiError> {
    let context = resolve_agent_run_context(
        state,
        current_user,
        run_id,
        agent_id,
        ProjectPermission::Use,
    )
    .await?;
    delivery_runtime_session_from_agent_run_context(&context)
}

fn delivery_runtime_session_from_agent_run_context(
    context: &AgentRunContext,
) -> Result<String, ApiError> {
    context.delivery_runtime_session_id.clone().ok_or_else(|| {
        ApiError::Conflict(format!(
            "AgentRun {} / {} 缺少 delivery runtime",
            context.run.id, context.agent.id
        ))
    })
}

fn delivery_frame_id_from_agent_run_context(context: &AgentRunContext) -> Result<Uuid, ApiError> {
    context.delivery_frame_id.ok_or_else(|| {
        ApiError::Conflict(format!(
            "AgentRun {} / {} 缺少 current delivery frame",
            context.run.id, context.agent.id
        ))
    })
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
    let delivery_runtime = delivery_runtime_session_for_agent_run(state, run.id, agent.id).await?;
    Ok(AgentRunContext {
        run,
        agent,
        delivery_runtime_session_id: delivery_runtime
            .as_ref()
            .map(|delivery| delivery.runtime_session_id.clone()),
        delivery_frame_id: delivery_runtime.map(|delivery| delivery.frame_id),
    })
}

async fn delivery_runtime_session_for_agent_run(
    state: &AppState,
    run_id: Uuid,
    agent_id: Uuid,
) -> Result<Option<AgentRunDeliveryRuntimeContext>, ApiError> {
    let agent_run_repos = state.repos.to_agent_run_repository_set();
    delivery_runtime_session_for_agent_run_from_repos(&agent_run_repos, run_id, agent_id).await
}

async fn delivery_runtime_session_for_agent_run_from_repos(
    repos: &AgentRunRepositorySet,
    run_id: Uuid,
    agent_id: Uuid,
) -> Result<Option<AgentRunDeliveryRuntimeContext>, ApiError> {
    delivery_runtime_session_for_agent_run_from_selection(
        DeliveryRuntimeSelectionService::from_repository_set(repos),
        run_id,
        agent_id,
    )
    .await
}

async fn delivery_runtime_session_for_agent_run_from_selection(
    selection_service: DeliveryRuntimeSelectionService<'_>,
    run_id: Uuid,
    agent_id: Uuid,
) -> Result<Option<AgentRunDeliveryRuntimeContext>, ApiError> {
    match selection_service
        .select_current_delivery(run_id, agent_id)
        .await
    {
        Ok(selection) => Ok(Some(AgentRunDeliveryRuntimeContext {
            runtime_session_id: selection.runtime_session_id,
            frame_id: selection.current_frame_id,
        })),
        Err(DeliveryRuntimeSelectionError::CurrentDeliveryMissing { .. }) => Ok(None),
        Err(error) => Err(delivery_runtime_selection_error(error)),
    }
}

fn delivery_runtime_selection_error(error: DeliveryRuntimeSelectionError) -> ApiError {
    match error {
        DeliveryRuntimeSelectionError::RunNotFound { .. }
        | DeliveryRuntimeSelectionError::AgentNotFound { .. }
        | DeliveryRuntimeSelectionError::CurrentFrameNotFound { .. }
        | DeliveryRuntimeSelectionError::LaunchFrameNotFound { .. }
        | DeliveryRuntimeSelectionError::SubjectNotFound { .. } => {
            ApiError::NotFound(error.to_string())
        }
        DeliveryRuntimeSelectionError::Repository(source) => ApiError::from(source),
        other => ApiError::Conflict(other.to_string()),
    }
}

async fn load_agent_run_workspace_snapshot(
    state: &AppState,
    context: &AgentRunContext,
    viewer_user_id: &str,
) -> Result<app_workspace::AgentRunWorkspaceSnapshot, ApiError> {
    let vfs_runtime = ApiVfsSurfaceRuntimeProjection::new(
        state.services.backend_registry.clone(),
        state.services.mount_provider_registry.clone(),
    );
    let agent_run_repos = state.repos.to_agent_run_repository_set();
    let lifecycle_repos = state.repos.to_lifecycle_repository_set();
    let lifecycle_surface_projection = AgentRunLifecycleSurfaceProjector::new(&lifecycle_repos);
    let service = app_workspace::AgentRunWorkspaceQueryService::new(
        &agent_run_repos,
        agent_run_session_core(state.services.session_core.clone()),
        agent_run_session_control(state.services.session_control.clone()),
        &vfs_runtime,
        &lifecycle_surface_projection,
        state.services.lifecycle_read_model_query.as_ref(),
    );
    let mut snapshot = service
        .resolve(app_workspace::AgentRunWorkspaceQueryInput {
            run: context.run.clone(),
            agent: context.agent.clone(),
            viewer_user_id: Some(viewer_user_id.to_string()),
        })
        .await
        .map_err(ApiError::from)?;
    append_exec_terminal_waiting_items(&mut snapshot, state);
    Ok(snapshot)
}

fn append_exec_terminal_waiting_items(
    snapshot: &mut app_workspace::AgentRunWorkspaceSnapshot,
    state: &AppState,
) {
    let Some(runtime_session_id) = snapshot.delivery_runtime_session_id.as_deref() else {
        return;
    };
    let existing_exec_refs = snapshot
        .conversation
        .mailbox
        .waiting_items
        .iter()
        .filter(|item| item.kind == "exec")
        .map(|item| {
            item.source_ref
                .clone()
                .unwrap_or_else(|| item.wait_id.clone())
        })
        .collect::<HashSet<_>>();

    let terminal_items = state
        .services
        .terminal_cache
        .list_terminals(runtime_session_id)
        .into_iter()
        .filter(|terminal| exec_terminal_is_waiting(&terminal.state))
        .filter(|terminal| !existing_exec_refs.contains(&terminal.terminal_id))
        .map(exec_terminal_waiting_item);
    snapshot
        .conversation
        .mailbox
        .waiting_items
        .extend(terminal_items);
}

fn exec_terminal_is_waiting(state: &str) -> bool {
    matches!(state, "starting" | "running")
}

fn exec_terminal_waiting_item(
    terminal: agentdash_application_runtime_session::session::terminal_cache::TerminalState,
) -> app_agent_run::ConversationWaitingItemModel {
    app_agent_run::ConversationWaitingItemModel {
        wait_id: terminal.terminal_id.clone(),
        gate_id: terminal.terminal_id.clone(),
        kind: "exec".to_string(),
        source_ref: Some(terminal.terminal_id),
        correlation_ref: None,
        status: terminal.state,
        source_label: Some("Terminal".to_string()),
        preview: terminal.cwd,
        created_at: timestamp_millis_to_rfc3339(terminal.created_at),
        resolved_at: terminal.exited_at.map(timestamp_millis_to_rfc3339),
    }
}

fn timestamp_millis_to_rfc3339(timestamp_millis: i64) -> String {
    DateTime::<Utc>::from_timestamp_millis(timestamp_millis)
        .unwrap_or_else(Utc::now)
        .to_rfc3339()
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
    let agent_run_repos = state.repos.to_agent_run_repository_set();
    let lifecycle_repos = state.repos.to_lifecycle_repository_set();
    let lifecycle_surface_projection = AgentRunLifecycleSurfaceProjector::new(&lifecycle_repos);
    let service = app_workspace::AgentRunWorkspaceQueryService::new(
        &agent_run_repos,
        agent_run_session_core(state.services.session_core.clone()),
        agent_run_session_control(state.services.session_control.clone()),
        &vfs_runtime,
        &lifecycle_surface_projection,
        state.services.lifecycle_read_model_query.as_ref(),
    );
    service
        .resolve_list_projection(app_workspace::AgentRunWorkspaceQueryInput {
            run,
            agent,
            viewer_user_id: None,
        })
        .await
        .map_err(ApiError::from)
}

/// 从 run 的全部 lineage 边构建控制树邻接（parent -> [child]）与 child id 集合。
/// root = 未作为任何 lineage child 出现的 agent。
fn build_lineage_forest(lineages: &[AgentLineage]) -> (HashMap<Uuid, Vec<Uuid>>, HashSet<Uuid>) {
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

/// Root list entry 的 activity 展示时间必须与服务端 keyset 排序 / cursor 同源。
fn root_list_shell_model_to_contract(
    run: &LifecycleRun,
    shell: app_workspace::AgentRunWorkspaceShellModel,
) -> AgentRunWorkspaceShell {
    let mut shell = shell_model_to_contract(shell);
    shell.last_activity_at = run.last_activity_at.to_rfc3339();
    shell
}

/// 内联子 Agent 节点：复用列表投影的 shell（含真实 delivery_status / last_activity_at）。
fn list_child_from_projection(
    run: &LifecycleRun,
    projection: app_workspace::AgentRunListProjection,
    subagent_count: u32,
) -> AgentRunListChild {
    AgentRunListChild {
        run_ref: LifecycleRunRefDto {
            run_id: run.id.to_string(),
        },
        agent_ref: AgentRunRefDto {
            run_id: run.id.to_string(),
            agent_id: projection.agent.id.to_string(),
        },
        project_agent_label: projection.project_agent_label,
        source: projection.agent.source.as_str().to_string(),
        shell: shell_model_to_contract(projection.shell),
        subagent_count,
        children: Vec::new(),
        delivery_runtime_ref: projection
            .delivery_runtime_session_id
            .map(|runtime_session_id| RuntimeSessionRefDto { runtime_session_id }),
    }
}

fn list_entry_from_projection(
    run: &LifecycleRun,
    projection: app_workspace::AgentRunListProjection,
    subagent_count: u32,
    children: Vec<AgentRunListChild>,
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
        project_agent_label: projection.project_agent_label.clone(),
        source: projection.agent.source.as_str().to_string(),
        shell: root_list_shell_model_to_contract(run, projection.shell),
        run_status: lifecycle_run_status_to_contract(run.status),
        subagent_count,
        children,
        delivery_runtime_ref: projection
            .delivery_runtime_session_id
            .map(|runtime_session_id| RuntimeSessionRefDto { runtime_session_id }),
        delivery_trace_meta: projection
            .delivery_trace_meta
            .map(workspace_trace_meta_to_contract),
        // 列表 UI 不消费 frame_ref，省去 frame runtime 解析。
        frame_ref: None,
        subject_ref: projection.subject_ref.map(subject_ref_to_contract),
        subject_label: projection.subject_label,
    }
}

fn agent_run_workspace_view(
    snapshot: app_workspace::AgentRunWorkspaceSnapshot,
) -> AgentRunWorkspaceView {
    let resource_surface = snapshot
        .resource_surface
        .map(vfs_surface_dto::surface_from_application);
    let resource_surface_coordinate = snapshot
        .resource_surface_coordinate
        .map(resource_surface_coordinate_to_contract);
    let mailbox = workspace_mailbox_to_contract(snapshot.mailbox);
    let mailbox_messages = snapshot
        .mailbox_messages
        .into_iter()
        .map(mailbox_message_view)
        .collect();
    let conversation =
        conversation_to_contract(snapshot.conversation, Some(mailbox), mailbox_messages);
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
        agent: snapshot.agent_view.map(|agent| AgentRunView {
            agent_ref: AgentRunRefDto {
                run_id: agent.agent_ref.run_id,
                agent_id: agent.agent_ref.agent_id,
            },
            project_id: agent.project_id,
            source: agent.source,
            project_agent_id: agent.project_agent_id,
            status: agent.status,
            delivery_runtime_ref: agent.delivery_runtime_ref.map(|runtime_ref| {
                RuntimeSessionRefDto {
                    runtime_session_id: runtime_ref.runtime_session_id,
                }
            }),
            last_delivery_status: agent.last_delivery_status,
            created_at: agent.created_at,
            updated_at: agent.updated_at,
        }),
        frame_runtime: snapshot.frame_runtime.map(frame_runtime_to_contract),
        subject_associations: snapshot
            .subject_associations
            .into_iter()
            .map(|association| LifecycleSubjectAssociationDto {
                id: association.id,
                anchor_run_id: association.anchor_run_id,
                anchor_agent_id: association.anchor_agent_id,
                subject_ref: SubjectRefDto {
                    kind: association.subject_ref.kind,
                    id: association.subject_ref.id,
                },
                role: association.role,
                metadata: association.metadata,
                created_at: association.created_at,
            })
            .collect(),
        resource_surface,
        resource_surface_coordinate,
        conversation: Some(conversation),
        // lineage 由 get_agent_run_workspace 单独填充（列表路径不需要，保持默认）。
        parent: None,
        children: Vec::new(),
    }
}

fn resource_surface_coordinate_to_contract(
    coordinate: app_workspace::AgentRunResourceSurfaceCoordinateModel,
) -> AgentRunResourceSurfaceCoordinateView {
    AgentRunResourceSurfaceCoordinateView {
        surface_frame_ref: AgentFrameRefDto {
            agent_id: coordinate.surface_frame_ref.agent_id,
            frame_id: coordinate.surface_frame_ref.frame_id,
            revision: coordinate.surface_frame_ref.revision,
        },
        source_anchor: coordinate.source_anchor.map(|anchor| {
            AgentRunResourceSurfaceSourceAnchorView {
                runtime_session_ref: RuntimeSessionRefDto {
                    runtime_session_id: anchor.runtime_session_id,
                },
                launch_frame_id: anchor.launch_frame_id,
                orchestration_id: anchor.orchestration_id,
                node_path: anchor.node_path,
                node_attempt: anchor.node_attempt,
                delivery_status: anchor.delivery_status,
                observed_at: anchor.observed_at,
            }
        }),
    }
}

fn conversation_to_contract(
    conversation: app_agent_run::AgentConversationSnapshotModel,
    mailbox_state: Option<MailboxStateView>,
    mailbox_messages: Vec<MailboxMessageView>,
) -> AgentConversationSnapshot {
    AgentConversationSnapshot {
        snapshot_id: conversation.snapshot_id,
        identity: AgentConversationIdentity {
            run_ref: LifecycleRunRefDto {
                run_id: conversation.identity.run_id.clone(),
            },
            agent_ref: AgentRunRefDto {
                run_id: conversation.identity.run_id,
                agent_id: conversation.identity.agent_id,
            },
            project_id: conversation.identity.project_id,
        },
        lifecycle_context: AgentConversationLifecycleContext {
            frame_ref: conversation
                .lifecycle_context
                .frame_ref
                .map(|frame| AgentFrameRefDto {
                    agent_id: frame.agent_id,
                    frame_id: frame.frame_id,
                    revision: frame.revision,
                }),
            delivery_runtime_ref: conversation
                .lifecycle_context
                .delivery_runtime_session_id
                .map(|runtime_session_id| RuntimeSessionRefDto { runtime_session_id }),
            subject_associations: conversation
                .lifecycle_context
                .subject_associations
                .into_iter()
                .map(|association| LifecycleSubjectAssociationDto {
                    id: association.id,
                    anchor_run_id: association.anchor_run_id,
                    anchor_agent_id: association.anchor_agent_id,
                    subject_ref: SubjectRefDto {
                        kind: association.subject_ref.kind,
                        id: association.subject_ref.id,
                    },
                    role: association.role,
                    metadata: association.metadata,
                    created_at: association.created_at,
                })
                .collect(),
        },
        execution: conversation_execution_to_contract(conversation.execution),
        model_config: conversation_model_config_to_contract(conversation.model_config),
        commands: conversation_command_set_to_contract(conversation.commands),
        mailbox: ConversationMailboxSnapshotView {
            visible_message_count: conversation.mailbox.visible_message_count,
            paused: conversation.mailbox.paused,
            user_attention: conversation.mailbox.user_attention,
            resume_command: conversation
                .mailbox
                .resume_command
                .map(conversation_command_to_contract),
            state: mailbox_state,
            messages: mailbox_messages,
            waiting_items: conversation
                .mailbox
                .waiting_items
                .into_iter()
                .map(conversation_waiting_item_to_contract)
                .collect(),
        },
        resource_surface: conversation
            .resource_surface
            .map(vfs_surface_dto::surface_from_application),
        resource_surface_coordinate: conversation
            .resource_surface_coordinate
            .map(resource_surface_coordinate_to_contract),
        diagnostics: conversation
            .diagnostics
            .into_iter()
            .map(conversation_diagnostic_to_contract)
            .collect(),
    }
}

fn conversation_waiting_item_to_contract(
    item: app_agent_run::ConversationWaitingItemModel,
) -> ConversationWaitingItemView {
    ConversationWaitingItemView {
        wait_id: item.wait_id,
        gate_id: item.gate_id,
        kind: item.kind,
        source_ref: item.source_ref,
        correlation_ref: item.correlation_ref,
        status: item.status,
        source_label: item.source_label,
        preview: item.preview,
        created_at: item.created_at,
        resolved_at: item.resolved_at,
    }
}

fn conversation_execution_to_contract(
    execution: app_agent_run::ConversationExecutionModel,
) -> ConversationExecutionView {
    ConversationExecutionView {
        status: conversation_execution_status_to_contract(execution.status),
        runtime_session_ref: execution
            .runtime_session_id
            .map(|runtime_session_id| RuntimeSessionRefDto { runtime_session_id }),
        active_turn_id: execution.active_turn_id,
        reason: execution.reason,
    }
}

fn conversation_execution_status_to_contract(
    status: app_agent_run::ConversationExecutionStatusModel,
) -> ConversationExecutionStatus {
    match status {
        app_agent_run::ConversationExecutionStatusModel::Draft => {
            ConversationExecutionStatus::Draft
        }
        app_agent_run::ConversationExecutionStatusModel::ModelRequired => {
            ConversationExecutionStatus::ModelRequired
        }
        app_agent_run::ConversationExecutionStatusModel::Ready => {
            ConversationExecutionStatus::Ready
        }
        app_agent_run::ConversationExecutionStatusModel::StartingClaimed => {
            ConversationExecutionStatus::StartingClaimed
        }
        app_agent_run::ConversationExecutionStatusModel::RunningActive => {
            ConversationExecutionStatus::RunningActive
        }
        app_agent_run::ConversationExecutionStatusModel::Cancelling => {
            ConversationExecutionStatus::Cancelling
        }
        app_agent_run::ConversationExecutionStatusModel::Terminal => {
            ConversationExecutionStatus::Terminal
        }
        app_agent_run::ConversationExecutionStatusModel::FrameMissing => {
            ConversationExecutionStatus::FrameMissing
        }
        app_agent_run::ConversationExecutionStatusModel::DeliveryMissing => {
            ConversationExecutionStatus::DeliveryMissing
        }
    }
}

fn conversation_model_config_to_contract(
    config: app_agent_run::ConversationModelConfigModel,
) -> ConversationModelConfigView {
    ConversationModelConfigView {
        status: match config.status {
            app_agent_run::ConversationModelConfigStatusModel::Resolved => {
                ConversationModelConfigStatus::Resolved
            }
            app_agent_run::ConversationModelConfigStatusModel::ModelRequired => {
                ConversationModelConfigStatus::ModelRequired
            }
        },
        effective_executor_config: config
            .effective_executor_config
            .map(conversation_effective_executor_config_to_contract),
        missing_fields: config.missing_fields,
        message: config.message,
    }
}

fn conversation_effective_executor_config_to_contract(
    config: app_agent_run::ConversationEffectiveExecutorConfigModel,
) -> ConversationEffectiveExecutorConfigView {
    ConversationEffectiveExecutorConfigView {
        executor: config.executor,
        provider_id: config.provider_id,
        model_id: config.model_id,
        agent_id: config.agent_id,
        thinking_level: config.thinking_level,
        permission_policy: config.permission_policy,
        source: match config.source {
            app_agent_run::ConversationModelConfigSourceModel::ProjectAgentPreset => {
                ConversationModelConfigSource::ProjectAgentPreset
            }
            app_agent_run::ConversationModelConfigSourceModel::FrameExecutionProfile => {
                ConversationModelConfigSource::FrameExecutionProfile
            }
            app_agent_run::ConversationModelConfigSourceModel::UserOverride => {
                ConversationModelConfigSource::UserOverride
            }
            app_agent_run::ConversationModelConfigSourceModel::ExecutorDiscoveryDefault => {
                ConversationModelConfigSource::ExecutorDiscoveryDefault
            }
            app_agent_run::ConversationModelConfigSourceModel::Unspecified => {
                ConversationModelConfigSource::Unspecified
            }
        },
    }
}

fn conversation_command_set_to_contract(
    commands: app_agent_run::ConversationCommandSetModel,
) -> ConversationCommandSetView {
    ConversationCommandSetView {
        ownership: ownership_to_contract(commands.ownership),
        commands: commands
            .commands
            .into_iter()
            .map(conversation_command_to_contract)
            .collect(),
        keyboard: ConversationKeyboardMapView {
            enter: commands.keyboard.enter,
            ctrl_enter: commands.keyboard.ctrl_enter,
        },
    }
}

fn ownership_to_contract(
    ownership: app_agent_run::AgentRunOwnershipModel,
) -> AgentRunOwnershipView {
    AgentRunOwnershipView {
        run_created_by_user_id: ownership.run_created_by_user_id,
        agent_created_by_user_id: ownership.agent_created_by_user_id,
        current_user_controls_run: ownership.current_user_controls_run,
    }
}

fn conversation_command_to_contract(
    command: app_agent_run::ConversationCommandModel,
) -> ConversationCommandView {
    ConversationCommandView {
        kind: conversation_command_kind_to_contract(command.kind),
        command_id: command.command_id,
        enabled: command.enabled,
        unavailable_reason: command.unavailable_reason,
        disabled_code: command.disabled_code,
        shortcut: command.shortcut,
        requires_input: command.requires_input,
        executor_config_policy: command.executor_config_policy,
        placement: command
            .placement
            .into_iter()
            .map(conversation_command_placement_to_contract)
            .collect(),
        stale_guard: conversation_stale_guard_to_contract(command.stale_guard),
    }
}

fn conversation_command_kind_to_contract(
    kind: app_agent_run::ConversationCommandKindModel,
) -> ConversationCommandKind {
    match kind {
        app_agent_run::ConversationCommandKindModel::SubmitMessage => {
            ConversationCommandKind::SubmitMessage
        }
        app_agent_run::ConversationCommandKindModel::PromoteMailboxMessage => {
            ConversationCommandKind::PromoteMailboxMessage
        }
        app_agent_run::ConversationCommandKindModel::DeleteMailboxMessage => {
            ConversationCommandKind::DeleteMailboxMessage
        }
        app_agent_run::ConversationCommandKindModel::MoveMailboxMessage => {
            ConversationCommandKind::MoveMailboxMessage
        }
        app_agent_run::ConversationCommandKindModel::ResumeMailbox => {
            ConversationCommandKind::ResumeMailbox
        }
        app_agent_run::ConversationCommandKindModel::Cancel => ConversationCommandKind::Cancel,
    }
}

fn conversation_command_kind_to_application(
    kind: ConversationCommandKind,
) -> app_agent_run::ConversationCommandKindModel {
    match kind {
        ConversationCommandKind::SubmitMessage => {
            app_agent_run::ConversationCommandKindModel::SubmitMessage
        }
        ConversationCommandKind::PromoteMailboxMessage => {
            app_agent_run::ConversationCommandKindModel::PromoteMailboxMessage
        }
        ConversationCommandKind::DeleteMailboxMessage => {
            app_agent_run::ConversationCommandKindModel::DeleteMailboxMessage
        }
        ConversationCommandKind::MoveMailboxMessage => {
            app_agent_run::ConversationCommandKindModel::MoveMailboxMessage
        }
        ConversationCommandKind::ResumeMailbox => {
            app_agent_run::ConversationCommandKindModel::ResumeMailbox
        }
        ConversationCommandKind::Cancel => app_agent_run::ConversationCommandKindModel::Cancel,
    }
}

fn conversation_command_placement_to_contract(
    placement: app_agent_run::ConversationCommandPlacementModel,
) -> ConversationCommandPlacement {
    match placement {
        app_agent_run::ConversationCommandPlacementModel::ComposerPrimary => {
            ConversationCommandPlacement::ComposerPrimary
        }
        app_agent_run::ConversationCommandPlacementModel::ComposerSecondary => {
            ConversationCommandPlacement::ComposerSecondary
        }
        app_agent_run::ConversationCommandPlacementModel::MailboxRow => {
            ConversationCommandPlacement::MailboxRow
        }
        app_agent_run::ConversationCommandPlacementModel::MailboxBanner => {
            ConversationCommandPlacement::MailboxBanner
        }
        app_agent_run::ConversationCommandPlacementModel::Header => {
            ConversationCommandPlacement::Header
        }
    }
}

fn conversation_stale_guard_to_contract(
    guard: app_agent_run::ConversationCommandStaleGuardModel,
) -> ConversationCommandStaleGuardView {
    ConversationCommandStaleGuardView {
        snapshot_id: guard.snapshot_id,
        run_id: guard.run_id,
        agent_id: guard.agent_id,
        frame_id: guard.frame_id,
        active_turn_id: guard.active_turn_id,
    }
}

fn command_precondition_to_application(
    command: AgentRunCommandPreconditionView,
) -> app_agent_run::AgentRunCommandPreconditionModel {
    app_agent_run::AgentRunCommandPreconditionModel {
        command_id: command.command_id,
        command_kind: conversation_command_kind_to_application(command.command_kind),
        stale_guard: app_agent_run::ConversationCommandStaleGuardModel {
            snapshot_id: command.stale_guard.snapshot_id,
            run_id: command.stale_guard.run_id,
            agent_id: command.stale_guard.agent_id,
            frame_id: command.stale_guard.frame_id,
            active_turn_id: command.stale_guard.active_turn_id,
        },
    }
}

fn conversation_diagnostic_to_contract(
    diagnostic: app_agent_run::ConversationDiagnosticModel,
) -> ConversationDiagnosticView {
    ConversationDiagnosticView {
        code: diagnostic.code,
        severity: match diagnostic.severity {
            app_agent_run::ValidationSeverityModel::Warning => ValidationSeverity::Warning,
            app_agent_run::ValidationSeverityModel::Error => ValidationSeverity::Error,
        },
        message: diagnostic.message,
        detail: diagnostic.detail,
    }
}

fn subject_ref_to_contract(subject_ref: app_workspace::SubjectRefModel) -> SubjectRefDto {
    SubjectRefDto {
        kind: subject_ref.kind,
        id: subject_ref.id,
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
        ownership: conversation.commands.ownership.clone(),
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
        effective_executor_config: frame
            .effective_executor_config
            .map(conversation_effective_executor_config_to_contract),
    }
}

/// 统计控制树某 root 子树（传递闭包）下的 subagent 总数。
///
/// lineage 支持任意深度递归且无环检测，因此遍历带 `visited` 防环 + 深度上限保护，
/// 超限截断并 warn（不静默丢弃）。root 自身不计入。
fn count_descendants(root: Uuid, children_map: &HashMap<Uuid, Vec<Uuid>>) -> u32 {
    const MAX_DEPTH: usize = 64;
    let mut visited: HashSet<Uuid> = HashSet::new();
    let mut stack: Vec<(Uuid, usize)> = vec![(root, 0)];
    let mut count: u32 = 0;
    while let Some((node, depth)) = stack.pop() {
        if depth >= MAX_DEPTH {
            diag!(Warn, Subsystem::Api,

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
    viewer_user_id: &str,
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
            app_workspace::load_hide_system_steer_messages_setting(
                state.repos.settings_repo.as_ref(),
                Some(viewer_user_id),
            )
            .await?,
        ),
        messages: messages
            .into_iter()
            .filter(mailbox_message_visible)
            .map(mailbox_message_view)
            .collect(),
    })
}

fn agent_run_mailbox_service<'a>(
    state: &AppState,
    agent_run_repos: &'a AgentRunRepositorySet,
) -> AgentRunMailboxService<'a> {
    ProjectAgentRunStartRepos::from_repository_set(agent_run_repos).mailbox_service(
        agent_run_session_core(state.services.session_core.clone()),
        agent_run_session_control(state.services.session_control.clone()),
        agent_run_session_eventing(state.services.session_eventing.clone()),
        agent_run_session_launch(state.services.session_launch.clone()),
    )
}

fn agent_run_fork_service<'a>(
    state: &AppState,
    agent_run_repos: &'a AgentRunRepositorySet,
) -> AgentRunForkService<'a> {
    AgentRunForkService::new(
        agent_run_repos,
        state.services.session_branching.clone(),
        agent_run_session_core(state.services.session_core.clone()),
        agent_run_mailbox_service(state, agent_run_repos),
    )
}

fn agent_run_fork_submit_message_response(
    result: AgentRunForkCommandResult,
) -> AgentRunMessageCommandResponse {
    let fork = agent_run_fork_outcome_view(&result);
    AgentRunMessageCommandResponse {
        command_receipt: command_receipt_view(result.command_receipt),
        outcome: mailbox_command_outcome_view(
            result
                .mailbox_outcome
                .unwrap_or(app_agent_run::AgentRunMailboxCommandOutcome::Queued),
        ),
        mailbox_message: result.mailbox_message.map(mailbox_message_view),
        accepted_refs: Some(agent_run_message_accepted_refs(result.child_refs.clone())),
        fork: Some(fork),
    }
}

fn agent_run_fork_response(result: AgentRunForkCommandResult) -> AgentRunForkResponse {
    let fork = agent_run_fork_outcome_view(&result);
    AgentRunForkResponse {
        command_receipt: command_receipt_view(result.command_receipt),
        outcome: fork.outcome,
        parent_refs: fork.parent_refs,
        child_refs: fork.child_refs,
        lineage: fork.lineage,
        redirect: fork.redirect,
    }
}

fn agent_run_fork_outcome_view(result: &AgentRunForkCommandResult) -> AgentRunForkOutcomeView {
    let parent_refs = agent_run_message_accepted_refs(result.parent_refs.clone());
    let child_refs = agent_run_message_accepted_refs(result.child_refs.clone());
    AgentRunForkOutcomeView {
        outcome: "forked".to_string(),
        parent_refs: parent_refs.clone(),
        child_refs: child_refs.clone(),
        lineage: AgentRunForkLineageView {
            id: result.lineage.id.to_string(),
            parent: parent_refs,
            child: child_refs.clone(),
            relation_kind: result.lineage.relation_kind.clone(),
            fork_point_event_seq: result.lineage.fork_point_event_seq,
            fork_point_ref: result
                .lineage
                .fork_point_ref_json
                .clone()
                .and_then(|value| serde_json::from_value::<SessionMessageRefDto>(value).ok()),
            forked_by_user_id: result.lineage.forked_by_user_id.clone(),
            created_at: result.lineage.created_at.to_rfc3339(),
        },
        redirect: child_refs.agent_ref,
    }
}

fn message_ref_from_contract(value: SessionMessageRefDto) -> MessageRef {
    MessageRef {
        turn_id: value.turn_id,
        entry_index: value.entry_index,
    }
}

fn log_agent_run_fork_route_error(
    route: &'static str,
    run_id: Uuid,
    agent_id: Uuid,
    current_user_id: &str,
    client_command_id: &str,
    fork_point_ref: Option<&MessageRef>,
    error: &(impl std::fmt::Debug + std::fmt::Display),
) {
    let fork_point = message_ref_log_label(fork_point_ref);
    let error_context = DiagnosticErrorContext::new("agent_run.fork", "route");
    diag_error!(Error, Subsystem::Api,
        context = &error_context,
        error = &error,
        route = route,
        run_id = %run_id,
        agent_id = %agent_id,
        current_user_id = %current_user_id,
        client_command_id = %client_command_id,
        fork_point = %fork_point,
        "AgentRun fork route failed"
    );
}

fn message_ref_log_label(value: Option<&MessageRef>) -> String {
    value
        .map(|message_ref| format!("{}:{}", message_ref.turn_id, message_ref.entry_index))
        .unwrap_or_else(|| "head".to_string())
}

fn agent_run_workspace_command_policy<'a>(
    state: &AppState,
    agent_run_repos: &'a AgentRunRepositorySet,
) -> app_workspace::AgentRunWorkspaceCommandPolicyService<'a> {
    app_workspace::AgentRunWorkspaceCommandPolicyService::new(
        agent_run_repos,
        agent_run_session_core(state.services.session_core.clone()),
        agent_run_session_control(state.services.session_control.clone()),
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

fn agent_run_command_receipt_contract(
    receipt: AgentRunCommandReceiptView,
) -> AgentRunCommandReceipt {
    AgentRunCommandReceipt {
        client_command_id: receipt.client_command_id,
        status: receipt.status,
        duplicate: receipt.duplicate,
        message: receipt.message,
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
    use agentdash_application_agentrun::agent_run::DeliveryRuntimeSelectionRepositories;
    use agentdash_domain::DomainError;
    use agentdash_domain::workflow::{
        AgentFrame, AgentFrameRepository, AgentRunAcceptedRefs, AgentRunDeliveryBinding,
        AgentRunDeliveryBindingRepository, AgentRunLineage, AgentSource, DeliveryBindingStatus,
        LifecycleAgent, LifecycleAgentRepository, LifecycleRun, LifecycleRunRepository,
        RuntimeSessionExecutionAnchor, RuntimeSessionExecutionAnchorRepository,
    };
    use tokio::sync::Mutex;

    use super::*;

    #[test]
    fn exec_terminal_waiting_item_uses_terminal_id_as_exec_ref() {
        let terminal =
            agentdash_application_runtime_session::session::terminal_cache::TerminalState {
                terminal_id: "term-1".to_string(),
                session_id: "runtime-1".to_string(),
                backend_id: "backend-1".to_string(),
                mount_id: Some("main".to_string()),
                cwd: Some("D:/repo".to_string()),
                capability: Some("interactive".to_string()),
                state: "running".to_string(),
                exit_code: None,
                process_id: None,
                created_at: 0,
                exited_at: None,
            };

        let item = exec_terminal_waiting_item(terminal);

        assert_eq!(item.wait_id, "term-1");
        assert_eq!(item.gate_id, "term-1");
        assert_eq!(item.kind, "exec");
        assert_eq!(item.source_ref.as_deref(), Some("term-1"));
        assert_eq!(item.status, "running");
        assert_eq!(item.preview.as_deref(), Some("D:/repo"));
    }

    #[test]
    fn exec_terminal_waiting_projection_only_keeps_live_states() {
        assert!(exec_terminal_is_waiting("starting"));
        assert!(exec_terminal_is_waiting("running"));
        assert!(!exec_terminal_is_waiting("exited"));
        assert!(!exec_terminal_is_waiting("killed"));
        assert!(!exec_terminal_is_waiting("lost"));
    }

    #[test]
    fn agent_run_fork_response_preserves_redirect_and_lineage() {
        let parent_run_id = Uuid::new_v4();
        let parent_agent_id = Uuid::new_v4();
        let child_run_id = Uuid::new_v4();
        let child_agent_id = Uuid::new_v4();
        let child_frame_id = Uuid::new_v4();
        let fork_point_ref = SessionMessageRefDto {
            turn_id: "turn-1".to_string(),
            entry_index: 2,
        };
        let lineage = AgentRunLineage::new_fork(
            parent_run_id,
            parent_agent_id,
            child_run_id,
            child_agent_id,
            Some(12),
            Some(serde_json::to_value(&fork_point_ref).expect("message ref should serialize")),
            "parent-runtime",
            "child-runtime",
            "user-a",
            Some(serde_json::json!({ "source": "api-test" })),
        );

        let response = agent_run_fork_response(AgentRunForkCommandResult {
            command_receipt: AgentRunCommandReceiptView {
                client_command_id: "cmd-fork".to_string(),
                status: "accepted".to_string(),
                duplicate: false,
                message: None,
            },
            parent_refs: AgentRunAcceptedRefs {
                run_id: parent_run_id,
                agent_id: parent_agent_id,
                frame_id: None,
                frame_revision: None,
                runtime_session_id: Some("parent-runtime".to_string()),
                agent_run_turn_id: None,
                protocol_turn_id: None,
            },
            child_refs: AgentRunAcceptedRefs {
                run_id: child_run_id,
                agent_id: child_agent_id,
                frame_id: Some(child_frame_id),
                frame_revision: Some(1),
                runtime_session_id: Some("child-runtime".to_string()),
                agent_run_turn_id: None,
                protocol_turn_id: None,
            },
            lineage,
            mailbox_outcome: Some(app_agent_run::AgentRunMailboxCommandOutcome::Queued),
            mailbox_message: None,
        });

        assert_eq!(response.outcome, "forked");
        assert_eq!(response.redirect.run_id, child_run_id.to_string());
        assert_eq!(response.redirect.agent_id, child_agent_id.to_string());
        assert_eq!(
            response.lineage.parent.agent_ref.run_id,
            parent_run_id.to_string()
        );
        assert_eq!(
            response.lineage.child.agent_ref.run_id,
            child_run_id.to_string()
        );
        assert_eq!(response.lineage.fork_point_event_seq, Some(12));
        let response_fork_point_ref = response
            .lineage
            .fork_point_ref
            .expect("fork point ref should be mapped");
        assert_eq!(response_fork_point_ref.turn_id, fork_point_ref.turn_id);
        assert_eq!(
            response_fork_point_ref.entry_index,
            fork_point_ref.entry_index
        );
        assert_eq!(response.lineage.forked_by_user_id, "user-a");
    }

    #[derive(Default)]
    struct MemoryLifecycleRunRepository {
        runs: Mutex<Vec<LifecycleRun>>,
    }

    #[async_trait::async_trait]
    impl LifecycleRunRepository for MemoryLifecycleRunRepository {
        async fn create(&self, run: &LifecycleRun) -> Result<(), DomainError> {
            self.runs.lock().await.push(run.clone());
            Ok(())
        }

        async fn get_by_id(&self, id: Uuid) -> Result<Option<LifecycleRun>, DomainError> {
            Ok(self
                .runs
                .lock()
                .await
                .iter()
                .find(|run| run.id == id)
                .cloned())
        }

        async fn list_by_ids(&self, ids: &[Uuid]) -> Result<Vec<LifecycleRun>, DomainError> {
            Ok(self
                .runs
                .lock()
                .await
                .iter()
                .filter(|run| ids.contains(&run.id))
                .cloned()
                .collect())
        }

        async fn list_by_project(
            &self,
            project_id: Uuid,
        ) -> Result<Vec<LifecycleRun>, DomainError> {
            Ok(self
                .runs
                .lock()
                .await
                .iter()
                .filter(|run| run.project_id == project_id)
                .cloned()
                .collect())
        }

        async fn update(&self, run: &LifecycleRun) -> Result<(), DomainError> {
            let mut runs = self.runs.lock().await;
            if let Some(existing) = runs.iter_mut().find(|item| item.id == run.id) {
                *existing = run.clone();
            }
            Ok(())
        }

        async fn delete(&self, id: Uuid) -> Result<(), DomainError> {
            self.runs.lock().await.retain(|run| run.id != id);
            Ok(())
        }
    }

    #[derive(Default)]
    struct MemoryLifecycleAgentRepository {
        agents: Mutex<Vec<LifecycleAgent>>,
    }

    #[async_trait::async_trait]
    impl LifecycleAgentRepository for MemoryLifecycleAgentRepository {
        async fn create(&self, agent: &LifecycleAgent) -> Result<(), DomainError> {
            self.agents.lock().await.push(agent.clone());
            Ok(())
        }

        async fn get(&self, id: Uuid) -> Result<Option<LifecycleAgent>, DomainError> {
            Ok(self
                .agents
                .lock()
                .await
                .iter()
                .find(|agent| agent.id == id)
                .cloned())
        }

        async fn list_by_run(&self, run_id: Uuid) -> Result<Vec<LifecycleAgent>, DomainError> {
            Ok(self
                .agents
                .lock()
                .await
                .iter()
                .filter(|agent| agent.run_id == run_id)
                .cloned()
                .collect())
        }

        async fn update(&self, agent: &LifecycleAgent) -> Result<(), DomainError> {
            let mut agents = self.agents.lock().await;
            if let Some(existing) = agents.iter_mut().find(|item| item.id == agent.id) {
                *existing = agent.clone();
            }
            Ok(())
        }
    }

    #[derive(Default)]
    struct MemoryAgentFrameRepository {
        frames: Mutex<Vec<AgentFrame>>,
    }

    #[async_trait::async_trait]
    impl AgentFrameRepository for MemoryAgentFrameRepository {
        async fn create(&self, frame: &AgentFrame) -> Result<(), DomainError> {
            self.frames.lock().await.push(frame.clone());
            Ok(())
        }

        async fn get(&self, frame_id: Uuid) -> Result<Option<AgentFrame>, DomainError> {
            Ok(self
                .frames
                .lock()
                .await
                .iter()
                .find(|frame| frame.id == frame_id)
                .cloned())
        }

        async fn get_current(&self, agent_id: Uuid) -> Result<Option<AgentFrame>, DomainError> {
            Ok(self
                .frames
                .lock()
                .await
                .iter()
                .filter(|frame| frame.agent_id == agent_id)
                .max_by_key(|frame| frame.revision)
                .cloned())
        }

        async fn list_by_agent(&self, agent_id: Uuid) -> Result<Vec<AgentFrame>, DomainError> {
            Ok(self
                .frames
                .lock()
                .await
                .iter()
                .filter(|frame| frame.agent_id == agent_id)
                .cloned()
                .collect())
        }
    }

    #[derive(Default)]
    struct MemoryRuntimeSessionExecutionAnchorRepository {
        anchors: Mutex<Vec<RuntimeSessionExecutionAnchor>>,
    }

    #[async_trait::async_trait]
    impl RuntimeSessionExecutionAnchorRepository for MemoryRuntimeSessionExecutionAnchorRepository {
        async fn create_once(
            &self,
            anchor: &RuntimeSessionExecutionAnchor,
        ) -> Result<(), DomainError> {
            let mut anchors = self.anchors.lock().await;
            if let Some(existing) = anchors
                .iter()
                .find(|item| item.runtime_session_id == anchor.runtime_session_id)
            {
                if existing.has_same_launch_coordinates_as(anchor) {
                    return Ok(());
                }
                return Err(existing.immutable_conflict(anchor));
            }
            anchors.push(anchor.clone());
            Ok(())
        }

        async fn delete_by_session(&self, runtime_session_id: &str) -> Result<(), DomainError> {
            self.anchors
                .lock()
                .await
                .retain(|anchor| anchor.runtime_session_id != runtime_session_id);
            Ok(())
        }

        async fn find_by_session(
            &self,
            runtime_session_id: &str,
        ) -> Result<Option<RuntimeSessionExecutionAnchor>, DomainError> {
            Ok(self
                .anchors
                .lock()
                .await
                .iter()
                .find(|anchor| anchor.runtime_session_id == runtime_session_id)
                .cloned())
        }

        async fn list_by_run(
            &self,
            run_id: Uuid,
        ) -> Result<Vec<RuntimeSessionExecutionAnchor>, DomainError> {
            Ok(self
                .anchors
                .lock()
                .await
                .iter()
                .filter(|anchor| anchor.run_id == run_id)
                .cloned()
                .collect())
        }

        async fn list_by_agent(
            &self,
            agent_id: Uuid,
        ) -> Result<Vec<RuntimeSessionExecutionAnchor>, DomainError> {
            Ok(self
                .anchors
                .lock()
                .await
                .iter()
                .filter(|anchor| anchor.agent_id == agent_id)
                .cloned()
                .collect())
        }

        async fn list_by_project_session_ids(
            &self,
            runtime_session_ids: &[String],
        ) -> Result<Vec<RuntimeSessionExecutionAnchor>, DomainError> {
            Ok(self
                .anchors
                .lock()
                .await
                .iter()
                .filter(|anchor| runtime_session_ids.contains(&anchor.runtime_session_id))
                .cloned()
                .collect())
        }
    }

    #[derive(Default)]
    struct MemoryAgentRunDeliveryBindingRepository {
        bindings: Mutex<Vec<AgentRunDeliveryBinding>>,
    }

    #[async_trait::async_trait]
    impl AgentRunDeliveryBindingRepository for MemoryAgentRunDeliveryBindingRepository {
        async fn upsert(&self, binding: &AgentRunDeliveryBinding) -> Result<(), DomainError> {
            let mut bindings = self.bindings.lock().await;
            if let Some(existing) = bindings
                .iter_mut()
                .find(|item| item.run_id == binding.run_id && item.agent_id == binding.agent_id)
            {
                *existing = binding.clone();
            } else {
                bindings.push(binding.clone());
            }
            Ok(())
        }

        async fn get_current(
            &self,
            run_id: Uuid,
            agent_id: Uuid,
        ) -> Result<Option<AgentRunDeliveryBinding>, DomainError> {
            Ok(self
                .bindings
                .lock()
                .await
                .iter()
                .find(|item| item.run_id == run_id && item.agent_id == agent_id)
                .cloned())
        }

        async fn list_by_run(
            &self,
            run_id: Uuid,
        ) -> Result<Vec<AgentRunDeliveryBinding>, DomainError> {
            Ok(self
                .bindings
                .lock()
                .await
                .iter()
                .filter(|item| item.run_id == run_id)
                .cloned()
                .collect())
        }

        async fn delete_by_session(&self, runtime_session_id: &str) -> Result<(), DomainError> {
            self.bindings
                .lock()
                .await
                .retain(|item| item.runtime_session_id != runtime_session_id);
            Ok(())
        }
    }

    #[derive(Default)]
    struct DeliverySelectionFixture {
        runs: MemoryLifecycleRunRepository,
        agents: MemoryLifecycleAgentRepository,
        frames: MemoryAgentFrameRepository,
        anchors: MemoryRuntimeSessionExecutionAnchorRepository,
        delivery_bindings: MemoryAgentRunDeliveryBindingRepository,
    }

    impl DeliverySelectionFixture {
        fn service(&self) -> DeliveryRuntimeSelectionService<'_> {
            DeliveryRuntimeSelectionService::new(DeliveryRuntimeSelectionRepositories {
                lifecycle_runs: &self.runs,
                lifecycle_agents: &self.agents,
                agent_frames: &self.frames,
                execution_anchors: &self.anchors,
                delivery_bindings: &self.delivery_bindings,
            })
        }
    }

    #[tokio::test]
    async fn delivery_runtime_session_context_ignores_anchor_without_binding() {
        let fixture = DeliverySelectionFixture::default();
        let run = LifecycleRun::new_plain(Uuid::new_v4());
        let agent = LifecycleAgent::new_root(run.id, run.project_id, AgentSource::ProjectAgent);
        let launch_frame = AgentFrame::new_initial(agent.id);
        let current_frame = AgentFrame::new_revision(agent.id, 2, "test");
        let old_anchor = RuntimeSessionExecutionAnchor::new_dispatch(
            "runtime-old",
            run.id,
            launch_frame.id,
            agent.id,
        );

        fixture.runs.create(&run).await.expect("run");
        fixture
            .frames
            .create(&launch_frame)
            .await
            .expect("launch frame");
        fixture
            .frames
            .create(&current_frame)
            .await
            .expect("current frame");
        fixture.agents.create(&agent).await.expect("agent");
        fixture
            .anchors
            .create_once(&old_anchor)
            .await
            .expect("old anchor");

        let delivery_runtime = delivery_runtime_session_for_agent_run_from_selection(
            fixture.service(),
            run.id,
            agent.id,
        )
        .await
        .expect("selection result");

        assert!(delivery_runtime.is_none());
    }

    #[tokio::test]
    async fn delivery_runtime_session_context_uses_binding_not_latest_anchor() {
        let fixture = DeliverySelectionFixture::default();
        let run = LifecycleRun::new_plain(Uuid::new_v4());
        let agent = LifecycleAgent::new_root(run.id, run.project_id, AgentSource::ProjectAgent);
        let current_launch_frame = AgentFrame::new_initial(agent.id);
        let current_frame = AgentFrame::new_revision(agent.id, 2, "test");

        let mut current_anchor = RuntimeSessionExecutionAnchor::new_dispatch(
            "runtime-current",
            run.id,
            current_launch_frame.id,
            agent.id,
        );
        current_anchor.updated_at = Utc::now() - chrono::Duration::seconds(30);
        let mut latest_anchor = RuntimeSessionExecutionAnchor::new_dispatch(
            "runtime-latest-raw-evidence",
            run.id,
            current_launch_frame.id,
            agent.id,
        );
        latest_anchor.updated_at = Utc::now();
        let current_binding = AgentRunDeliveryBinding::from_anchor(
            &current_anchor,
            DeliveryBindingStatus::Running,
            current_anchor.updated_at,
        );

        fixture.runs.create(&run).await.expect("run");
        fixture
            .frames
            .create(&current_launch_frame)
            .await
            .expect("launch frame");
        fixture
            .frames
            .create(&current_frame)
            .await
            .expect("current frame");
        fixture.agents.create(&agent).await.expect("agent");
        fixture
            .anchors
            .create_once(&latest_anchor)
            .await
            .expect("latest anchor");
        fixture
            .anchors
            .create_once(&current_anchor)
            .await
            .expect("current anchor");
        fixture
            .delivery_bindings
            .upsert(&current_binding)
            .await
            .expect("current binding");

        let delivery_runtime = delivery_runtime_session_for_agent_run_from_selection(
            fixture.service(),
            run.id,
            agent.id,
        )
        .await
        .expect("selection result");

        let delivery_runtime = delivery_runtime.expect("current delivery");
        assert_eq!(delivery_runtime.runtime_session_id, "runtime-current");
        assert_eq!(delivery_runtime.frame_id, current_frame.id);
    }

    #[test]
    fn list_entry_from_projection_carries_source_and_count() {
        let run = LifecycleRun::new_plain(Uuid::new_v4());
        let agent = LifecycleAgent::new_root(run.id, run.project_id, AgentSource::ProjectAgent);
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
            project_agent_label: Some("Code Reviewer".to_string()),
            delivery_runtime_session_id: None,
            delivery_trace_meta: None,
            subject_ref: None,
            subject_label: None,
        };

        let entry = list_entry_from_projection(&run, projection, 3, Vec::new());

        assert_eq!(entry.shell.display_title, "Session meta title");
        assert_eq!(entry.shell.title_source, "source");
        assert_eq!(entry.source, "project_agent");
        assert_eq!(entry.project_agent_label.as_deref(), Some("Code Reviewer"));
        assert_eq!(entry.subagent_count, 3);
        assert!(entry.frame_ref.is_none());
        assert!(entry.children.is_empty());
    }

    #[test]
    fn list_entry_from_projection_uses_run_activity_for_root_shell() {
        let mut run = LifecycleRun::new_plain(Uuid::new_v4());
        run.last_activity_at = DateTime::parse_from_rfc3339("2026-06-18T08:30:00Z")
            .expect("run activity")
            .with_timezone(&Utc);
        let agent = LifecycleAgent::new_root(run.id, run.project_id, AgentSource::ProjectAgent);
        let projection = app_workspace::AgentRunListProjection {
            run: run.clone(),
            agent,
            shell: app_workspace::AgentRunWorkspaceShellModel {
                display_title: "Root AgentRun".to_string(),
                title_source: "source".to_string(),
                workspace_status: "running".to_string(),
                delivery_status: "idle".to_string(),
                last_turn_id: None,
                last_activity_at: "2026-06-12T00:00:00Z".to_string(),
            },
            project_agent_label: None,
            delivery_runtime_session_id: None,
            delivery_trace_meta: None,
            subject_ref: None,
            subject_label: None,
        };

        let entry = list_entry_from_projection(&run, projection, 0, Vec::new());
        let (cursor_millis, cursor_run_id) =
            decode_cursor(&encode_cursor(run.last_activity_at, run.id)).expect("cursor");
        let shell_activity = DateTime::parse_from_rfc3339(&entry.shell.last_activity_at)
            .expect("shell activity")
            .with_timezone(&Utc);

        assert_eq!(
            entry.shell.last_activity_at,
            run.last_activity_at.to_rfc3339()
        );
        assert_eq!(cursor_millis, shell_activity.timestamp_millis());
        assert_eq!(cursor_run_id, run.id);
    }

    #[test]
    fn list_child_from_projection_preserves_agent_scoped_activity() {
        let mut run = LifecycleRun::new_plain(Uuid::new_v4());
        run.last_activity_at = DateTime::parse_from_rfc3339("2026-06-18T08:30:00Z")
            .expect("run activity")
            .with_timezone(&Utc);
        let child_activity = "2026-06-12T00:00:00Z";
        let agent = LifecycleAgent::new_root(run.id, run.project_id, AgentSource::ProjectAgent);
        let projection = app_workspace::AgentRunListProjection {
            run: run.clone(),
            agent,
            shell: app_workspace::AgentRunWorkspaceShellModel {
                display_title: "Child AgentRun".to_string(),
                title_source: "source".to_string(),
                workspace_status: "running".to_string(),
                delivery_status: "idle".to_string(),
                last_turn_id: None,
                last_activity_at: child_activity.to_string(),
            },
            project_agent_label: None,
            delivery_runtime_session_id: None,
            delivery_trace_meta: None,
            subject_ref: None,
            subject_label: None,
        };

        let child = list_child_from_projection(&run, projection, 0);

        assert_eq!(child.shell.last_activity_at, child_activity);
    }

    #[test]
    fn cursor_round_trips_keyset() {
        let run_id = Uuid::new_v4();
        let at = DateTime::parse_from_rfc3339("2026-06-16T08:30:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let encoded = encode_cursor(at, run_id);
        let (millis, decoded_id) = decode_cursor(&encoded).expect("decode");
        assert_eq!(millis, at.timestamp_millis());
        assert_eq!(decoded_id, run_id);
        assert!(decode_cursor("not-base64!!").is_none());
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
            runtime_session_id: Some("runtime-1".to_string()),
            paused: true,
            pause_reason: Some("turn_interrupted".to_string()),
            pause_message: Some("上一轮已中断，mailbox 已暂停。".to_string()),
            backend_selection_preference: None,
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
            runtime_session_id: Some("runtime-1".to_string()),
            paused: true,
            pause_reason: Some("turn_interrupted".to_string()),
            pause_message: Some("上一轮已中断，mailbox 已暂停。".to_string()),
            backend_selection_preference: None,
            updated_at: chrono::Utc::now(),
        };
        let view = mailbox_state_view(Some(&state), true, 0, false);

        assert!(!view.paused);
        assert!(!view.can_resume);
        assert_eq!(view.pause_reason.as_deref(), Some("turn_interrupted"));
    }
}
