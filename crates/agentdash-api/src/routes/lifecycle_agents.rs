use agentdash_diagnostics::{DiagnosticErrorContext, Subsystem, diag, diag_error};
use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;

use crate::routes::runtime_traces;
use agentdash_agent_protocol::codex_app_server_protocol as codex;
use agentdash_agent_runtime_contract::{
    EventSequence, InteractionResponse, OperationReceipt, PresentationThreadId, RuntimeActor,
    RuntimeContextView, RuntimeEventEnvelope, RuntimeInput, RuntimeInteractionId, RuntimeSnapshot,
};
use agentdash_application::agent_run_list::{
    AgentRunListChildModel, AgentRunListEntryModel, AgentRunListRuntimeSummaryModel,
    ProjectAgentRunListInput, ProjectAgentRunListPage,
};
use agentdash_application_agentrun::agent_run::terminal_registry::TerminalState;
use agentdash_application_agentrun::agent_run::{
    AgentRunCommandGuard, AgentRunDeleteCommand, AgentRunDeleteCommandService,
    AgentRunForkCommandService, AgentRunForkGraph, AgentRunForkRuntimePort, AgentRunJournalEvent,
    AgentRunJournalLiveEvent, AgentRunJournalQuery, AgentRunPresentationDraft,
    AgentRunProductCommandService, AgentRunRuntimeError, AgentRunRuntimeView,
    EnqueueRuntimeMailboxMessage, ForkAgentRunRuntime, GuardedAgentRunCommand, ReadAgentRunEvents,
    ResolveAgentRunInteraction, RuntimeAgentRunMailbox, RuntimeMailboxError,
    RuntimeMailboxSubmitOutcome,
};
use agentdash_application_ports::agent_run_runtime::{
    AgentRunRuntimeBinding, AgentRunRuntimeBindingError, AgentRunRuntimeTarget,
};
use agentdash_application_ports::agent_run_surface::AgentRunTerminalLaunchTarget;
use agentdash_contracts::agent_run_mailbox::{
    AgentRunCommandOnlyRequest, AgentRunCommandReceipt, AgentRunComposerSubmitRequest,
    AgentRunContextCompactionCommandOutcome, AgentRunContextCompactionCommandResponse,
    AgentRunForkLineageView, AgentRunForkOutcomeView, AgentRunForkRequest, AgentRunForkResponse,
    AgentRunForkSubmitRequest, AgentRunMailboxMessageContentView, AgentRunMailboxMoveRequest,
    AgentRunMailboxView, AgentRunMessageAcceptedRefs, AgentRunMessageCommandOutcome,
    AgentRunMessageCommandResponse, AgentRunToolCallApprovalResponse,
    AgentRunToolCallRejectionResponse, ConsumptionBarrier, MailboxDelivery, MailboxDrainMode,
    MailboxMessageOrigin, MailboxMessageStatus, MailboxMessageView, MailboxSourceIdentity,
    MailboxStateView, SteeringStopEffect,
};
use agentdash_contracts::session::{
    SessionEventResponse, SessionEventsPageResponse, SessionNdjsonEnvelope,
};
use agentdash_contracts::workflow::{
    AgentFrameRefDto, AgentRunCommandPreconditionView, AgentRunListChildView,
    AgentRunListEntryView, AgentRunListRuntimeSummaryView, AgentRunListRuntimeThreadStatus,
    AgentRunRefDto, AgentRunWorkspaceView, ConversationCommandKind, DeleteAgentRunResponse,
    LifecycleRunRefDto, ProjectAgentRunListView, SubjectRefDto,
};
use agentdash_domain::workflow::{
    AgentRunAcceptedRefs as DomainAgentRunAcceptedRefs, AgentRunCommandKind, AgentRunCommandStatus,
    AgentRunLineage, LifecycleAgent, LifecycleRun,
};
use async_trait::async_trait;
use axum::{
    Json,
    body::Body,
    body::Bytes,
    extract::{Path, Query, State},
    http::HeaderMap,
    response::{IntoResponse, Response},
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::time::MissedTickBehavior;
use uuid::Uuid;

use crate::dto::{
    AgentRunJournalEventsQuery, AgentRunJournalStreamQuery, ContextAuditQuery, SpawnTerminalBody,
};
use crate::{
    agent_run_runtime_surface::resolve_terminal_launch_target_for_runtime_session,
    app_state::AppState,
    auth::{CurrentUser, ProjectPermission, load_project_with_permission},
    routes::terminals,
    rpc::ApiError,
};

struct AgentRunContext {
    run: LifecycleRun,
    agent: LifecycleAgent,
    presentation_thread_id: Option<PresentationThreadId>,
}

struct AgentRunDeliveryRuntimeContext {
    presentation_thread_id: PresentationThreadId,
}

struct ApiAgentRunForkRuntimePort<'a> {
    runtime: &'a dyn agentdash_application_agentrun::agent_run::AgentRunRuntime,
}

#[async_trait]
impl AgentRunForkRuntimePort for ApiAgentRunForkRuntimePort<'_> {
    async fn fork_runtime(
        &self,
        command: ForkAgentRunRuntime,
    ) -> Result<AgentRunRuntimeBinding, agentdash_application_agentrun::WorkflowApplicationError>
    {
        self.runtime.fork_runtime(command).await.map_err(|error| {
            agentdash_application_agentrun::WorkflowApplicationError::Internal(error.to_string())
        })
    }
}

const AGENT_RUN_JOURNAL_STREAM_HEARTBEAT_INTERVAL: Duration = Duration::from_secs(15);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AgentRunJournalLiveReceiveAction {
    ContinueAfterLag(u64),
    BreakAfterClose,
}

#[derive(serde::Deserialize)]
pub struct AgentRunListQuery {
    pub limit: Option<u32>,
    pub cursor: Option<String>,
}

fn agent_run_journal_live_receive_action(
    error: tokio::sync::broadcast::error::RecvError,
) -> AgentRunJournalLiveReceiveAction {
    match error {
        tokio::sync::broadcast::error::RecvError::Lagged(lagged) => {
            AgentRunJournalLiveReceiveAction::ContinueAfterLag(lagged)
        }
        tokio::sync::broadcast::error::RecvError::Closed => {
            AgentRunJournalLiveReceiveAction::BreakAfterClose
        }
    }
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
            "/agent-runs/{run_id}/agents/{agent_id}/composer-submit",
            axum::routing::post(submit_agent_run_composer_input),
        )
        .route(
            "/agent-runs/{run_id}/agents/{agent_id}/cancel",
            axum::routing::post(cancel_agent_run),
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
            "/agent-runs/{run_id}/agents/{agent_id}/mailbox/resume",
            axum::routing::post(resume_agent_run_mailbox),
        )
        .route(
            "/agent-runs/{run_id}/agents/{agent_id}/mailbox",
            axum::routing::get(get_agent_run_mailbox),
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
            "/agent-runs/{run_id}/agents/{agent_id}/runtime",
            axum::routing::get(inspect_agent_run_runtime),
        )
        .route(
            "/agent-runs/{run_id}/agents/{agent_id}/runtime/control",
            axum::routing::get(get_agent_run_runtime_control),
        )
        .route(
            "/agent-runs/{run_id}/agents/{agent_id}/workspace",
            axum::routing::get(get_agent_run_workspace),
        )
        .route(
            "/agent-runs/{run_id}/agents/{agent_id}/runtime/context",
            axum::routing::get(read_agent_run_runtime_context),
        )
        .route(
            "/agent-runs/{run_id}/agents/{agent_id}/runtime/context/projection",
            axum::routing::get(get_agent_run_runtime_context_projection),
        )
        .route(
            "/agent-runs/{run_id}/agents/{agent_id}/runtime/events/stream/ndjson",
            axum::routing::get(stream_agent_run_runtime_events),
        )
        .route(
            "/agent-runs/{run_id}/agents/{agent_id}/journal/events",
            axum::routing::get(list_agent_run_journal_events),
        )
        .route(
            "/agent-runs/{run_id}/agents/{agent_id}/journal/stream/ndjson",
            axum::routing::get(agent_run_journal_stream_route),
        )
        .route(
            "/agent-runs/{run_id}/agents/{agent_id}/runtime/terminals",
            axum::routing::get(list_agent_run_runtime_terminals)
                .post(spawn_agent_run_runtime_terminal),
        )
        .route(
            "/agent-runs/{run_id}/agents/{agent_id}/runtime/context/compact",
            axum::routing::post(compact_agent_run_context),
        )
        .route(
            "/agent-runs/{run_id}/agents/{agent_id}/runtime/context/audit",
            axum::routing::get(get_agent_run_runtime_context_audit),
        )
        .route(
            "/agent-runs/{run_id}/agents/{agent_id}/runtime/interactions/{interaction_id}/respond",
            axum::routing::post(respond_agent_run_interaction),
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

async fn get_project_agent_runs(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(project_id): Path<String>,
    Query(query): Query<AgentRunListQuery>,
) -> Result<Json<ProjectAgentRunListView>, ApiError> {
    let project_id = parse_uuid(&project_id, "project_id")?;
    load_project_with_permission(
        state.as_ref(),
        &current_user,
        project_id,
        ProjectPermission::Use,
    )
    .await?;
    let page = state
        .services
        .project_agent_run_list_query
        .list(ProjectAgentRunListInput {
            project_id,
            limit: query.limit.map(|limit| limit as usize),
            cursor: query.cursor.as_deref(),
        })
        .await?;
    Ok(Json(project_agent_run_list_to_contract(page)))
}

async fn delete_project_agent_run(
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
    let outcome = AgentRunDeleteCommandService::new(state.repos.agent_run_delete_store.as_ref())
        .delete(AgentRunDeleteCommand { project_id, run_id })
        .await?;
    Ok(Json(DeleteAgentRunResponse {
        deleted: true,
        project_id: outcome.project_id.to_string(),
        run_id: outcome.run_id.to_string(),
    }))
}

fn project_agent_run_list_to_contract(page: ProjectAgentRunListPage) -> ProjectAgentRunListView {
    ProjectAgentRunListView {
        project_id: page.project_id.to_string(),
        agent_runs: page
            .entries
            .into_iter()
            .map(agent_run_list_entry_to_contract)
            .collect(),
        next_cursor: page.next_cursor,
    }
}

fn agent_run_list_entry_to_contract(model: AgentRunListEntryModel) -> AgentRunListEntryView {
    AgentRunListEntryView {
        run_ref: LifecycleRunRefDto {
            run_id: model.run_id.to_string(),
        },
        agent_ref: AgentRunRefDto {
            run_id: model.run_id.to_string(),
            agent_id: model.agent_id.to_string(),
        },
        title: model.title,
        lifecycle_status: model.lifecycle_status,
        last_activity_at: model.last_activity_at,
        project_agent_label: model.project_agent_label,
        source: model.source,
        runtime: model.runtime.map(agent_run_list_runtime_to_contract),
        subagent_count: model.subagent_count,
        children: model
            .children
            .into_iter()
            .map(agent_run_list_child_to_contract)
            .collect(),
        subject_ref: model.subject.as_ref().map(|subject| SubjectRefDto {
            kind: subject.kind.clone(),
            id: subject.id.to_string(),
        }),
        subject_label: model.subject.and_then(|subject| subject.label),
    }
}

fn agent_run_list_child_to_contract(model: AgentRunListChildModel) -> AgentRunListChildView {
    AgentRunListChildView {
        run_ref: LifecycleRunRefDto {
            run_id: model.run_id.to_string(),
        },
        agent_ref: AgentRunRefDto {
            run_id: model.run_id.to_string(),
            agent_id: model.agent_id.to_string(),
        },
        title: model.title,
        lifecycle_status: model.lifecycle_status,
        last_activity_at: model.last_activity_at,
        project_agent_label: model.project_agent_label,
        source: model.source,
        runtime: model.runtime.map(agent_run_list_runtime_to_contract),
        children: model
            .children
            .into_iter()
            .map(agent_run_list_child_to_contract)
            .collect(),
    }
}

fn agent_run_list_runtime_to_contract(
    model: AgentRunListRuntimeSummaryModel,
) -> AgentRunListRuntimeSummaryView {
    agent_run_runtime_summary_to_contract(model.thread_status, model.active_turn_id)
}

fn agent_run_runtime_summary_to_contract(
    thread_status: agentdash_agent_runtime_contract::RuntimeThreadStatus,
    active_turn_id: Option<String>,
) -> AgentRunListRuntimeSummaryView {
    AgentRunListRuntimeSummaryView {
        thread_status: match thread_status {
            agentdash_agent_runtime_contract::RuntimeThreadStatus::Active => {
                AgentRunListRuntimeThreadStatus::Active
            }
            agentdash_agent_runtime_contract::RuntimeThreadStatus::Suspended => {
                AgentRunListRuntimeThreadStatus::Suspended
            }
            agentdash_agent_runtime_contract::RuntimeThreadStatus::Desynchronized => {
                AgentRunListRuntimeThreadStatus::Desynchronized
            }
            agentdash_agent_runtime_contract::RuntimeThreadStatus::Closed => {
                AgentRunListRuntimeThreadStatus::Closed
            }
            agentdash_agent_runtime_contract::RuntimeThreadStatus::Lost => {
                AgentRunListRuntimeThreadStatus::Lost
            }
        },
        active_turn_id,
    }
}

async fn get_agent_run_workspace(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path((run_id, agent_id)): Path<(String, String)>,
) -> Result<Json<AgentRunWorkspaceView>, ApiError> {
    let context = resolve_agent_run_context(
        state.as_ref(),
        &current_user,
        &run_id,
        &agent_id,
        ProjectPermission::Use,
    )
    .await?;
    let mut view = super::agent_run_workspace::load(
        state.as_ref(),
        context.run.clone(),
        context.agent.clone(),
        &current_user.user_id,
    )
    .await?;
    let (parent, children) =
        super::agent_run_workspace::resolve_lineage(state.as_ref(), &context.run, &context.agent)
            .await?;
    view.parent = parent;
    view.children = children;
    Ok(Json(view))
}

async fn get_agent_run_runtime_control(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path((run_id, agent_id)): Path<(String, String)>,
) -> Result<Json<AgentRunWorkspaceView>, ApiError> {
    let context = resolve_agent_run_context(
        state.as_ref(),
        &current_user,
        &run_id,
        &agent_id,
        ProjectPermission::Use,
    )
    .await?;
    let mut view = super::agent_run_workspace::load(
        state.as_ref(),
        context.run.clone(),
        context.agent.clone(),
        &current_user.user_id,
    )
    .await?;
    let (parent, children) =
        super::agent_run_workspace::resolve_lineage(state.as_ref(), &context.run, &context.agent)
            .await?;
    view.parent = parent;
    view.children = children;
    Ok(Json(view))
}

pub(crate) fn mailbox_message_visible(
    message: &agentdash_domain::agent_run_mailbox::AgentRunMailboxMessage,
) -> bool {
    !matches!(
        message.status,
        agentdash_domain::agent_run_mailbox::MailboxMessageStatus::Dispatched
            | agentdash_domain::agent_run_mailbox::MailboxMessageStatus::Steered
            | agentdash_domain::agent_run_mailbox::MailboxMessageStatus::Deleted
    )
}

pub(crate) fn mailbox_message_contract(
    message: agentdash_domain::agent_run_mailbox::AgentRunMailboxMessage,
) -> MailboxMessageView {
    use agentdash_domain::agent_run_mailbox as domain;
    let can_delete = matches!(
        message.status,
        domain::MailboxMessageStatus::Accepted
            | domain::MailboxMessageStatus::Queued
            | domain::MailboxMessageStatus::ReadyToConsume
            | domain::MailboxMessageStatus::Paused
            | domain::MailboxMessageStatus::Blocked
    );
    let can_promote = can_delete
        && message.delivery == domain::MailboxDelivery::LaunchOrContinueTurn
        && message.last_error.as_deref() != Some(domain::MAILBOX_DELIVERY_RESULT_UNKNOWN);
    let can_reorder = can_delete
        && message.origin == domain::MailboxMessageOrigin::User
        && message.delivery == domain::MailboxDelivery::LaunchOrContinueTurn;
    let can_recall = can_delete
        && message.origin == domain::MailboxMessageOrigin::User
        && message.payload_json.is_some();
    MailboxMessageView {
        id: message.id.to_string(),
        origin: match message.origin {
            domain::MailboxMessageOrigin::User => MailboxMessageOrigin::User,
            domain::MailboxMessageOrigin::System => MailboxMessageOrigin::System,
            domain::MailboxMessageOrigin::Hook => MailboxMessageOrigin::Hook,
            domain::MailboxMessageOrigin::Companion => MailboxMessageOrigin::Companion,
            domain::MailboxMessageOrigin::Workflow => MailboxMessageOrigin::Workflow,
        },
        source: MailboxSourceIdentity {
            namespace: message.source.namespace.clone(),
            kind: message.source.kind.clone(),
            source_ref: message.source.source_ref.clone(),
            correlation_ref: message.source.correlation_ref.clone(),
            actor: message.source.actor.clone(),
            route: message.source.route.clone(),
            display_label_key: message.source.display_label_key.clone(),
            metadata: message.source.metadata.clone(),
        },
        delivery: match &message.delivery {
            domain::MailboxDelivery::LaunchOrContinueTurn => MailboxDelivery::LaunchOrContinueTurn,
            domain::MailboxDelivery::SteerActiveTurn { stop_effect } => {
                MailboxDelivery::SteerActiveTurn {
                    stop_effect: match stop_effect {
                        domain::SteeringStopEffect::None => SteeringStopEffect::None,
                        domain::SteeringStopEffect::ContinueOnStop => {
                            SteeringStopEffect::ContinueOnStop
                        }
                    },
                }
            }
            domain::MailboxDelivery::ResumeLaunchSource { launch_source } => {
                MailboxDelivery::ResumeLaunchSource {
                    launch_source: launch_source.clone(),
                }
            }
        },
        barrier: match message.barrier {
            domain::ConsumptionBarrier::ImmediateIfIdle => ConsumptionBarrier::ImmediateIfIdle,
            domain::ConsumptionBarrier::AgentLoopTurnBoundary => {
                ConsumptionBarrier::AgentLoopTurnBoundary
            }
            domain::ConsumptionBarrier::AgentRunTurnBoundary => {
                ConsumptionBarrier::AgentRunTurnBoundary
            }
            domain::ConsumptionBarrier::ManualResume => ConsumptionBarrier::ManualResume,
        },
        drain_mode: match message.drain_mode {
            domain::MailboxDrainMode::One => MailboxDrainMode::One,
            domain::MailboxDrainMode::All => MailboxDrainMode::All,
        },
        status: match message.status {
            domain::MailboxMessageStatus::Accepted => MailboxMessageStatus::Accepted,
            domain::MailboxMessageStatus::Queued => MailboxMessageStatus::Queued,
            domain::MailboxMessageStatus::ReadyToConsume => MailboxMessageStatus::ReadyToConsume,
            domain::MailboxMessageStatus::Consuming => MailboxMessageStatus::Consuming,
            domain::MailboxMessageStatus::Dispatched => MailboxMessageStatus::Dispatched,
            domain::MailboxMessageStatus::Steered => MailboxMessageStatus::Steered,
            domain::MailboxMessageStatus::Paused => MailboxMessageStatus::Paused,
            domain::MailboxMessageStatus::Blocked => MailboxMessageStatus::Blocked,
            domain::MailboxMessageStatus::Failed => MailboxMessageStatus::Failed,
            domain::MailboxMessageStatus::Deleted => MailboxMessageStatus::Deleted,
        },
        preview: message.preview.clone(),
        has_images: message.has_images,
        attempt_count: message.attempt_count,
        accepted_refs: if message.accepted_agent_run_turn_id.is_some()
            || message.accepted_protocol_turn_id.is_some()
        {
            Some(AgentRunMessageAcceptedRefs {
                run_ref: LifecycleRunRefDto {
                    run_id: message.run_id.to_string(),
                },
                agent_ref: AgentRunRefDto {
                    run_id: message.run_id.to_string(),
                    agent_id: message.agent_id.to_string(),
                },
                frame_ref: None,
                agent_run_turn_id: message.accepted_agent_run_turn_id.clone(),
                protocol_turn_id: message.accepted_protocol_turn_id.clone(),
            })
        } else {
            None
        },
        last_error: message.last_error.clone(),
        created_at: message.created_at.to_rfc3339(),
        updated_at: message.updated_at.to_rfc3339(),
        can_promote,
        can_delete,
        can_reorder,
        can_recall,
    }
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
    diag!(Debug, Subsystem::Api,

        run_id = %context.run.id,
        agent_id = %context.agent.id,
        "AgentRun composer submit context resolved"
    );
    if context.run.created_by_user_id != current_user.user_id
        || context.agent.created_by_user_id != current_user.user_id
    {
        return fork_submit_agent_run(
            State(state),
            CurrentUser(current_user),
            Path((run_id, agent_id)),
            Json(AgentRunForkSubmitRequest {
                input: req.input,
                client_command_id: req.client_command_id,
                executor_config: req.executor_config,
                title: None,
                fork_point_ref: None,
                metadata_json: None,
                backend_selection: req.backend_selection,
            }),
        )
        .await;
    }
    validate_agent_run_product_command(
        state.as_ref(),
        &context,
        &current_user,
        &req.command,
        ConversationCommandKind::SubmitMessage,
    )
    .await?;
    let target = agent_run_runtime_target(&context);
    let presentation = AgentRunPresentationDraft {
        content: req.input.clone(),
        source: agentdash_agent_protocol::UserInputSource::core_composer(),
        launch_source: agentdash_application_agentrun::agent_run::LaunchPresentationSource::LifecycleAgentUserMessage,
        submission_kind: agentdash_agent_protocol::UserInputSubmissionKind::Prompt,
        started_at_seconds: Utc::now().timestamp(),
    };
    let runtime_input = runtime_input_from_codex(req.input)?;
    let outcome = runtime_agent_run_mailbox(state.as_ref())
        .submit(EnqueueRuntimeMailboxMessage {
            target: target.clone(),
            presentation_thread_id: context.presentation_thread_id.clone().ok_or_else(|| {
                ApiError::Conflict(format!(
                    "AgentRun {} / {} 缺少 delivery runtime",
                    context.run.id, context.agent.id
                ))
            })?,
            presentation,
            client_command_id: req.client_command_id.clone(),
            input: runtime_input,
            actor: runtime_actor(&current_user),
            identity: Some(current_user.clone()),
            origin: agentdash_domain::agent_run_mailbox::MailboxMessageOrigin::User,
            source: agentdash_domain::agent_run_mailbox::MailboxSourceIdentity::composer(),
            delivery_intent: req.delivery_intent,
            executor_config: req.executor_config,
            backend_selection: req
                .backend_selection
                .as_ref()
                .map(super::project_agents::backend_selection_input)
                .transpose()?,
        })
        .await
        .map_err(runtime_mailbox_error)?;
    let response = match outcome {
        RuntimeMailboxSubmitOutcome::Queued { message } => {
            spawn_runtime_mailbox_watcher(state.clone(), target);
            agent_run_message_command_response(
                req.client_command_id,
                AgentRunMessageCommandOutcome::Queued,
                None,
                Some(message),
            )
        }
        RuntimeMailboxSubmitOutcome::Dispatched {
            receipt, steered, ..
        } => agent_run_message_command_response(
            req.client_command_id,
            if steered {
                AgentRunMessageCommandOutcome::Steered
            } else {
                AgentRunMessageCommandOutcome::Launched
            },
            Some(receipt),
            None,
        ),
    };
    diag!(Debug, Subsystem::Api,

        run_id = %context.run.id,
        agent_id = %context.agent.id,
        outcome = ?response,
        "AgentRun composer submit accepted by managed runtime"
    );
    Ok(Json(response))
}

fn agent_run_message_command_response(
    client_command_id: String,
    outcome: AgentRunMessageCommandOutcome,
    receipt: Option<OperationReceipt>,
    mailbox_message: Option<agentdash_domain::agent_run_mailbox::AgentRunMailboxMessage>,
) -> AgentRunMessageCommandResponse {
    let mailbox_message = mailbox_message.map(mailbox_message_contract);
    AgentRunMessageCommandResponse {
        command_receipt: AgentRunCommandReceipt {
            client_command_id,
            status: match outcome {
                AgentRunMessageCommandOutcome::Queued => "queued",
                AgentRunMessageCommandOutcome::Launched
                | AgentRunMessageCommandOutcome::Steered
                | AgentRunMessageCommandOutcome::Deleted
                | AgentRunMessageCommandOutcome::Moved
                | AgentRunMessageCommandOutcome::Resumed => "accepted",
                AgentRunMessageCommandOutcome::Blocked => "blocked",
                AgentRunMessageCommandOutcome::Failed => "failed",
            }
            .to_string(),
            duplicate: receipt.as_ref().is_some_and(|receipt| receipt.duplicate),
            message: None,
        },
        outcome,
        mailbox_message,
        accepted_refs: None,
        fork: None,
    }
}

fn mailbox_control_response(
    client_command_id: String,
    outcome: AgentRunMessageCommandOutcome,
    message: Option<agentdash_domain::agent_run_mailbox::AgentRunMailboxMessage>,
) -> AgentRunMessageCommandResponse {
    let mailbox_message = message.map(mailbox_message_contract);
    AgentRunMessageCommandResponse {
        command_receipt: AgentRunCommandReceipt {
            client_command_id,
            status: "accepted".to_string(),
            duplicate: false,
            message: None,
        },
        outcome,
        mailbox_message,
        accepted_refs: None,
        fork: None,
    }
}

fn domain_agent_run_refs(context: &AgentRunContext) -> DomainAgentRunAcceptedRefs {
    DomainAgentRunAcceptedRefs {
        run_id: context.run.id,
        agent_id: context.agent.id,
        frame_id: None,
        frame_revision: None,
        runtime_session_id: context
            .presentation_thread_id
            .as_ref()
            .map(ToString::to_string),
        agent_run_turn_id: None,
        protocol_turn_id: None,
    }
}

fn contract_agent_run_refs(context: &AgentRunContext) -> AgentRunMessageAcceptedRefs {
    AgentRunMessageAcceptedRefs {
        run_ref: LifecycleRunRefDto {
            run_id: context.run.id.to_string(),
        },
        agent_ref: AgentRunRefDto {
            run_id: context.run.id.to_string(),
            agent_id: context.agent.id.to_string(),
        },
        frame_ref: None,
        agent_run_turn_id: None,
        protocol_turn_id: None,
    }
}

fn replay_mailbox_response(
    claim: &agentdash_application_agentrun::agent_run::AgentRunProductCommandClaim,
) -> Result<AgentRunMessageCommandResponse, ApiError> {
    let result_json = replay_product_command_json(claim, "mailbox")?;
    let mut response: AgentRunMessageCommandResponse = serde_json::from_value(result_json)
        .map_err(|error| ApiError::Internal(error.to_string()))?;
    response.command_receipt.duplicate = true;
    Ok(response)
}

fn replay_product_command_json(
    claim: &agentdash_application_agentrun::agent_run::AgentRunProductCommandClaim,
    label: &str,
) -> Result<serde_json::Value, ApiError> {
    match claim.status {
        AgentRunCommandStatus::Accepted => claim
            .result_json
            .clone()
            .ok_or_else(|| ApiError::Conflict(format!("{label} 命令结果尚未持久化"))),
        AgentRunCommandStatus::Pending => Err(ApiError::Conflict(format!("{label} 命令正在执行"))),
        AgentRunCommandStatus::TerminalFailed => claim.result_json.clone().ok_or_else(|| {
            ApiError::Conflict(
                claim
                    .error_message
                    .clone()
                    .unwrap_or_else(|| format!("{label} 命令执行失败")),
            )
        }),
    }
}

async fn product_command_step<T>(
    service: &AgentRunProductCommandService<'_>,
    receipt_id: Uuid,
    result: Result<T, ApiError>,
) -> Result<T, ApiError> {
    match result {
        Ok(value) => Ok(value),
        Err(error) => {
            let _ = service.fail(receipt_id, error.to_string()).await;
            Err(error)
        }
    }
}

fn ensure_mailbox_message_target(
    context: &AgentRunContext,
    message: &agentdash_domain::agent_run_mailbox::AgentRunMailboxMessage,
) -> Result<(), ApiError> {
    if message.run_id != context.run.id || message.agent_id != context.agent.id {
        return Err(ApiError::NotFound("mailbox message 不存在".to_string()));
    }
    Ok(())
}

async fn fork_agent_run(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path((run_id, agent_id)): Path<(String, String)>,
    Json(body): Json<AgentRunForkRequest>,
) -> Result<Json<AgentRunForkResponse>, ApiError> {
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
    let parent_frame = state
        .repos
        .agent_frame_repo
        .get_current(context.agent.id)
        .await?
        .ok_or_else(|| ApiError::Conflict("current AgentFrame 尚未就绪".to_string()))?;
    if context.presentation_thread_id.is_none() {
        return Err(ApiError::Conflict(
            "AgentRun 尚未建立 runtime binding".to_string(),
        ));
    }
    let fork_point_event_seq = if let Some(point) = body.fork_point_ref.as_ref() {
        let page = state
            .services
            .agent_run_journal
            .load_visible_journal_page(
                AgentRunJournalQuery {
                    run_id: context.run.id,
                    agent_id: context.agent.id,
                },
                0,
                u32::MAX,
            )
            .await?;
        Some(
            page.events
                .into_iter()
                .find(|event| {
                    let coordinate = &event.record.carrier().coordinate;
                    coordinate.source_turn_id.as_deref() == Some(point.turn_id.as_str())
                        && coordinate.source_entry_index == Some(point.entry_index)
                })
                .map(|event| event.journal_seq)
                .ok_or_else(|| ApiError::NotFound("fork point 不存在于可见会话日志".to_string()))?,
        )
    } else {
        None
    };
    let child_run =
        LifecycleRun::new_plain_for_user(context.run.project_id, current_user.user_id.clone());
    let mut child_agent = LifecycleAgent::new_root_for_user(
        child_run.id,
        context.run.project_id,
        context.agent.source,
        current_user.user_id.clone(),
    );
    child_agent.project_agent_id = context.agent.project_agent_id;
    child_agent.bootstrap_status = context.agent.bootstrap_status.clone();
    child_agent.workspace_title = body
        .title
        .clone()
        .or_else(|| context.agent.workspace_title.clone());
    child_agent.workspace_title_source = child_agent.workspace_title.as_ref().map(|_| {
        if body.title.is_some() {
            "user"
        } else {
            "source"
        }
        .to_string()
    });
    let mut child_frame = parent_frame.clone();
    child_frame.id = Uuid::new_v4();
    child_frame.agent_id = child_agent.id;
    child_frame.revision = 1;
    child_frame.created_by_kind = "agent_run_fork_materialization".to_string();
    child_frame.created_by_id = Some(current_user.user_id.clone());
    child_frame.created_at = Utc::now();

    let child_target = AgentRunRuntimeTarget {
        run_id: child_run.id,
        agent_id: child_agent.id,
    };
    let child_presentation_thread_id =
        PresentationThreadId::new(format!("agentrun-{}-{}", child_run.id, child_agent.id))
            .map_err(|error| ApiError::Internal(error.to_string()))?;
    let through_source_turn_id = body
        .fork_point_ref
        .as_ref()
        .map(|point| agentdash_agent_runtime_contract::DriverTurnId::new(point.turn_id.clone()))
        .transpose()
        .map_err(|error| ApiError::BadRequest(error.to_string()))?;
    let fork_point_ref_json = body
        .fork_point_ref
        .as_ref()
        .map(serde_json::to_value)
        .transpose()
        .map_err(|error| ApiError::Internal(error.to_string()))?;
    let lineage = AgentRunLineage::new_fork(
        context.run.id,
        context.agent.id,
        child_run.id,
        child_agent.id,
        fork_point_event_seq,
        fork_point_ref_json,
        current_user.user_id.clone(),
        body.metadata_json.clone(),
    )
    .with_frame_baseline(
        parent_frame.id,
        parent_frame.revision,
        child_frame.id,
        child_frame.revision,
    );
    let command_service =
        AgentRunProductCommandService::new(state.repos.agent_run_command_receipt_repo.as_ref());
    let claim = command_service
        .claim(
            context.run.id,
            context.agent.id,
            AgentRunCommandKind::AgentRunFork,
            body.client_command_id.clone(),
            &body,
        )
        .await?;
    if claim.duplicate {
        if claim.status == AgentRunCommandStatus::TerminalFailed {
            return Err(ApiError::Conflict(
                claim
                    .error_message
                    .clone()
                    .unwrap_or_else(|| "fork 命令执行失败".to_string()),
            ));
        }
        let result_json = if claim.status == AgentRunCommandStatus::Pending {
            claim
                .result_json
                .clone()
                .ok_or_else(|| ApiError::Conflict("fork 命令正在执行".to_string()))?
        } else {
            replay_product_command_json(&claim, "fork")?
        };
        let mut response: AgentRunForkResponse = serde_json::from_value(result_json)
            .map_err(|error| ApiError::Internal(error.to_string()))?;
        if claim.status == AgentRunCommandStatus::Pending {
            let child_run_id = parse_uuid(&response.child_refs.run_ref.run_id, "child run_id")?;
            let child_agent_id =
                parse_uuid(&response.child_refs.agent_ref.agent_id, "child agent_id")?;
            let runtime = state
                .services
                .agent_run_runtime
                .inspect(AgentRunRuntimeTarget {
                    run_id: child_run_id,
                    agent_id: child_agent_id,
                })
                .await
                .map_err(agent_run_runtime_error)?;
            let binding = runtime.binding.ok_or_else(|| {
                ApiError::Conflict("fork 结果已持久化但 runtime binding 尚未就绪".to_string())
            })?;
            command_service
                .accept_refs(
                    claim.receipt_id,
                    domain_fork_child_refs(&response, Some(binding.thread_id.to_string()))?,
                )
                .await?;
        }
        response.command_receipt.duplicate = true;
        return Ok(Json(response));
    }
    let response = fork_response_view(
        &context,
        &parent_frame,
        &child_run,
        &child_agent,
        &child_frame,
        &lineage,
        &body,
    );
    command_service
        .store_result(claim.receipt_id, &response)
        .await?;
    let graph = AgentRunForkGraph {
        child_run: child_run.clone(),
        child_agent: child_agent.clone(),
        child_frame: child_frame.clone(),
        lineage: lineage.clone(),
    };
    let runtime_port = ApiAgentRunForkRuntimePort {
        runtime: state.services.agent_run_runtime.as_ref(),
    };
    let fork_service = AgentRunForkCommandService::new(
        state.repos.agent_run_fork_graph_store.as_ref(),
        &runtime_port,
    );
    let binding = fork_service
        .materialize(
            &graph,
            ForkAgentRunRuntime {
                source_target: agent_run_runtime_target(&context),
                child_target: child_target.clone(),
                child_presentation_thread_id,
                through_source_turn_id,
                identity: Some(current_user.clone()),
                backend_selection: None,
            },
        )
        .await
        .map_err(|error| {
            let error = ApiError::from(error);
            error
        });
    let binding = match binding {
        Ok(binding) => binding,
        Err(error) => {
            let _ = command_service
                .fail(claim.receipt_id, error.to_string())
                .await;
            return Err(error);
        }
    };

    command_service
        .accept_refs(
            claim.receipt_id,
            domain_fork_child_refs(&response, Some(binding.thread_id.to_string()))?,
        )
        .await?;
    Ok(Json(response))
}

fn domain_fork_child_refs(
    response: &AgentRunForkResponse,
    runtime_session_id: Option<String>,
) -> Result<DomainAgentRunAcceptedRefs, ApiError> {
    let frame = response.child_refs.frame_ref.as_ref();
    Ok(DomainAgentRunAcceptedRefs {
        run_id: parse_uuid(&response.child_refs.run_ref.run_id, "child run_id")?,
        agent_id: parse_uuid(&response.child_refs.agent_ref.agent_id, "child agent_id")?,
        frame_id: frame
            .map(|frame| parse_uuid(&frame.frame_id, "child frame_id"))
            .transpose()?,
        frame_revision: frame.and_then(|frame| frame.revision),
        runtime_session_id,
        agent_run_turn_id: response.child_refs.agent_run_turn_id.clone(),
        protocol_turn_id: response.child_refs.protocol_turn_id.clone(),
    })
}

fn fork_response_view(
    context: &AgentRunContext,
    parent_frame: &agentdash_domain::workflow::AgentFrame,
    child_run: &LifecycleRun,
    child_agent: &LifecycleAgent,
    child_frame: &agentdash_domain::workflow::AgentFrame,
    lineage: &AgentRunLineage,
    body: &AgentRunForkRequest,
) -> AgentRunForkResponse {
    let parent_refs = AgentRunMessageAcceptedRefs {
        run_ref: LifecycleRunRefDto {
            run_id: context.run.id.to_string(),
        },
        agent_ref: AgentRunRefDto {
            run_id: context.run.id.to_string(),
            agent_id: context.agent.id.to_string(),
        },
        frame_ref: Some(AgentFrameRefDto {
            agent_id: context.agent.id.to_string(),
            frame_id: parent_frame.id.to_string(),
            revision: Some(parent_frame.revision),
        }),
        agent_run_turn_id: None,
        protocol_turn_id: body
            .fork_point_ref
            .as_ref()
            .map(|point| point.turn_id.clone()),
    };
    let child_refs = AgentRunMessageAcceptedRefs {
        run_ref: LifecycleRunRefDto {
            run_id: child_run.id.to_string(),
        },
        agent_ref: AgentRunRefDto {
            run_id: child_run.id.to_string(),
            agent_id: child_agent.id.to_string(),
        },
        frame_ref: Some(AgentFrameRefDto {
            agent_id: child_agent.id.to_string(),
            frame_id: child_frame.id.to_string(),
            revision: Some(child_frame.revision),
        }),
        agent_run_turn_id: None,
        protocol_turn_id: None,
    };
    let lineage_view = AgentRunForkLineageView {
        id: lineage.id.to_string(),
        parent: parent_refs.clone(),
        child: child_refs.clone(),
        relation_kind: lineage.relation_kind.clone(),
        fork_point_event_seq: lineage.fork_point_event_seq,
        fork_point_ref: body.fork_point_ref.clone(),
        forked_by_user_id: lineage.forked_by_user_id.clone(),
        created_at: lineage.created_at.to_rfc3339(),
    };
    let redirect = AgentRunRefDto {
        run_id: child_run.id.to_string(),
        agent_id: child_agent.id.to_string(),
    };
    AgentRunForkResponse {
        command_receipt: AgentRunCommandReceipt {
            client_command_id: body.client_command_id.clone(),
            status: "accepted".to_string(),
            duplicate: false,
            message: None,
        },
        outcome: "forked".to_string(),
        parent_refs,
        child_refs,
        lineage: lineage_view,
        redirect,
    }
}

async fn fork_submit_agent_run(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path((run_id, agent_id)): Path<(String, String)>,
    Json(body): Json<AgentRunForkSubmitRequest>,
) -> Result<Json<AgentRunMessageCommandResponse>, ApiError> {
    if body.input.is_empty() {
        return Err(ApiError::BadRequest("input 不能为空".to_string()));
    }
    let fork = fork_agent_run(
        State(state.clone()),
        CurrentUser(current_user.clone()),
        Path((run_id, agent_id)),
        Json(AgentRunForkRequest {
            client_command_id: format!("{}:fork", body.client_command_id),
            title: body.title,
            fork_point_ref: body.fork_point_ref,
            metadata_json: body.metadata_json,
        }),
    )
    .await?
    .0;
    let child = resolve_agent_run_context(
        &state,
        &current_user,
        &fork.redirect.run_id,
        &fork.redirect.agent_id,
        ProjectPermission::Use,
    )
    .await?;
    let target = agent_run_runtime_target(&child);
    let presentation = AgentRunPresentationDraft {
        content: body.input.clone(),
        source: agentdash_agent_protocol::UserInputSource::core_composer(),
        launch_source: agentdash_application_agentrun::agent_run::LaunchPresentationSource::LifecycleAgentUserMessage,
        submission_kind: agentdash_agent_protocol::UserInputSubmissionKind::Prompt,
        started_at_seconds: Utc::now().timestamp(),
    };
    let runtime_input = runtime_input_from_codex(body.input)?;
    let outcome = runtime_agent_run_mailbox(state.as_ref())
        .submit(EnqueueRuntimeMailboxMessage {
            target: target.clone(),
            presentation_thread_id: child.presentation_thread_id.clone().ok_or_else(|| {
                ApiError::Conflict("forked AgentRun 缺少 delivery runtime".to_string())
            })?,
            presentation,
            client_command_id: body.client_command_id.clone(),
            input: runtime_input,
            actor: runtime_actor(&current_user),
            identity: Some(current_user),
            origin: agentdash_domain::agent_run_mailbox::MailboxMessageOrigin::User,
            source: agentdash_domain::agent_run_mailbox::MailboxSourceIdentity::composer(),
            delivery_intent: None,
            executor_config: body.executor_config,
            backend_selection: body
                .backend_selection
                .as_ref()
                .map(super::project_agents::backend_selection_input)
                .transpose()?,
        })
        .await
        .map_err(runtime_mailbox_error)?;
    let fork_outcome = AgentRunForkOutcomeView {
        outcome: fork.outcome,
        parent_refs: fork.parent_refs,
        child_refs: fork.child_refs.clone(),
        lineage: fork.lineage,
        redirect: fork.redirect,
    };
    let mut response = match outcome {
        RuntimeMailboxSubmitOutcome::Queued { message } => {
            spawn_runtime_mailbox_watcher(state, target);
            agent_run_message_command_response(
                body.client_command_id,
                AgentRunMessageCommandOutcome::Queued,
                None,
                Some(message),
            )
        }
        RuntimeMailboxSubmitOutcome::Dispatched { receipt, .. } => {
            agent_run_message_command_response(
                body.client_command_id,
                AgentRunMessageCommandOutcome::Launched,
                Some(receipt),
                None,
            )
        }
    };
    response.accepted_refs = Some(fork.child_refs);
    response.fork = Some(fork_outcome);
    Ok(Json(response))
}

async fn get_agent_run_mailbox(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path((run_id, agent_id)): Path<(String, String)>,
) -> Result<Json<AgentRunMailboxView>, ApiError> {
    let context = resolve_agent_run_context(
        state.as_ref(),
        &current_user,
        &run_id,
        &agent_id,
        ProjectPermission::Use,
    )
    .await?;
    let messages = state
        .repos
        .agent_run_mailbox_repo
        .list_messages(context.run.id, context.agent.id)
        .await?;
    let visible_messages = messages
        .into_iter()
        .filter(mailbox_message_visible)
        .map(mailbox_message_contract)
        .collect::<Vec<_>>();
    let state_view = state
        .repos
        .agent_run_mailbox_repo
        .get_state(context.run.id, context.agent.id)
        .await?;
    let paused =
        !visible_messages.is_empty() && state_view.as_ref().is_some_and(|state| state.paused);
    let can_resume = paused
        && context.run.created_by_user_id == current_user.user_id
        && !matches!(
            context.agent.status.as_str(),
            "completed" | "failed" | "cancelled" | "canceled"
        );
    Ok(Json(AgentRunMailboxView {
        state: MailboxStateView {
            paused,
            pause_reason: state_view
                .as_ref()
                .and_then(|state| state.pause_reason.clone()),
            message: state_view
                .as_ref()
                .and_then(|state| state.pause_message.clone()),
            can_resume,
            hide_system_steer_messages: false,
        },
        messages: visible_messages,
    }))
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
    validate_agent_run_product_command(
        state.as_ref(),
        &context,
        &current_user,
        &body.command,
        ConversationCommandKind::DeleteMailboxMessage,
    )
    .await?;
    let message_id = parse_uuid(&message_id, "message_id")?;
    let command_service =
        AgentRunProductCommandService::new(state.repos.agent_run_command_receipt_repo.as_ref());
    let claim = command_service
        .claim(
            context.run.id,
            context.agent.id,
            AgentRunCommandKind::MailboxDelete,
            body.client_command_id.clone(),
            &body,
        )
        .await?;
    if claim.duplicate {
        return Ok(Json(replay_mailbox_response(&claim)?));
    }
    let deleted = product_command_step(
        &command_service,
        claim.receipt_id,
        async {
            let message = state
                .repos
                .agent_run_mailbox_repo
                .get_message(message_id)
                .await?
                .ok_or_else(|| ApiError::NotFound("mailbox message 不存在".to_string()))?;
            ensure_mailbox_message_target(&context, &message)?;
            Ok(state
                .repos
                .agent_run_mailbox_repo
                .delete_message(message_id)
                .await?
                .unwrap_or(message))
        }
        .await,
    )
    .await?;
    let mut response = mailbox_control_response(
        body.client_command_id,
        AgentRunMessageCommandOutcome::Deleted,
        Some(deleted),
    );
    response.accepted_refs = Some(contract_agent_run_refs(&context));
    command_service
        .accept(claim.receipt_id, domain_agent_run_refs(&context), &response)
        .await?;
    Ok(Json(response))
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
    let message_id = parse_uuid(&message_id, "message_id")?;
    validate_agent_run_product_command(
        state.as_ref(),
        &context,
        &current_user,
        &body.command,
        ConversationCommandKind::PromoteMailboxMessage,
    )
    .await?;
    let command_service =
        AgentRunProductCommandService::new(state.repos.agent_run_command_receipt_repo.as_ref());
    let claim = command_service
        .claim(
            context.run.id,
            context.agent.id,
            AgentRunCommandKind::MailboxPromote,
            body.client_command_id.clone(),
            &body,
        )
        .await?;
    if claim.duplicate {
        return Ok(Json(replay_mailbox_response(&claim)?));
    }
    let promoted = product_command_step(
        &command_service,
        claim.receipt_id,
        async {
            let message = state
                .repos
                .agent_run_mailbox_repo
                .get_message(message_id)
                .await?
                .ok_or_else(|| ApiError::NotFound("mailbox message 不存在".to_string()))?;
            ensure_mailbox_message_target(&context, &message)?;
            if message.last_error.as_deref()
                == Some(agentdash_domain::agent_run_mailbox::MAILBOX_DELIVERY_RESULT_UNKNOWN)
            {
                return Err(ApiError::Conflict(
                    "mailbox message delivery result is unknown and cannot be promoted".to_string(),
                ));
            }
            Ok(state
                .repos
                .agent_run_mailbox_repo
                .update_message_policy(
                    message_id,
                    agentdash_domain::agent_run_mailbox::MailboxDelivery::SteerActiveTurn {
                        stop_effect: agentdash_domain::agent_run_mailbox::SteeringStopEffect::None,
                    },
                    agentdash_domain::agent_run_mailbox::ConsumptionBarrier::AgentLoopTurnBoundary,
                    agentdash_domain::agent_run_mailbox::MailboxDrainMode::All,
                    100,
                )
                .await?)
        }
        .await,
    )
    .await?;
    let target = agent_run_runtime_target(&context);
    spawn_runtime_mailbox_watcher(state.clone(), target);
    let mut response = mailbox_control_response(
        body.client_command_id,
        AgentRunMessageCommandOutcome::Queued,
        Some(promoted),
    );
    response.accepted_refs = Some(contract_agent_run_refs(&context));
    command_service
        .accept(claim.receipt_id, domain_agent_run_refs(&context), &response)
        .await?;
    Ok(Json(response))
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
    validate_agent_run_product_command(
        state.as_ref(),
        &context,
        &current_user,
        &body.command,
        ConversationCommandKind::MoveMailboxMessage,
    )
    .await?;
    let message_id = parse_uuid(&message_id, "message_id")?;
    let after_message_id = body
        .after_message_id
        .as_deref()
        .map(|value| parse_uuid(value, "after_message_id"))
        .transpose()?;
    if after_message_id == Some(message_id) {
        return Err(ApiError::BadRequest(
            "anchor message 不能是当前重排序消息".to_string(),
        ));
    }
    let command_service =
        AgentRunProductCommandService::new(state.repos.agent_run_command_receipt_repo.as_ref());
    let claim = command_service
        .claim(
            context.run.id,
            context.agent.id,
            AgentRunCommandKind::MailboxMove,
            body.client_command_id.clone(),
            &body,
        )
        .await?;
    if claim.duplicate {
        return Ok(Json(replay_product_command_json(&claim, "移动")?));
    }
    let moved = product_command_step(
        &command_service,
        claim.receipt_id,
        async {
            let message = state
                .repos
                .agent_run_mailbox_repo
                .get_message(message_id)
                .await?
                .ok_or_else(|| ApiError::NotFound("mailbox message 不存在".to_string()))?;
            ensure_mailbox_message_target(&context, &message)?;
            if let Some(anchor_id) = after_message_id {
                let anchor = state
                    .repos
                    .agent_run_mailbox_repo
                    .get_message(anchor_id)
                    .await?
                    .ok_or_else(|| ApiError::NotFound("anchor message 不存在".to_string()))?;
                ensure_mailbox_message_target(&context, &anchor)?;
            }
            Ok(state
                .repos
                .agent_run_mailbox_repo
                .move_message_after(
                    message_id,
                    after_message_id,
                    context.run.id,
                    context.agent.id,
                )
                .await?)
        }
        .await,
    )
    .await?;
    let response = serde_json::json!({ "ok": true, "order_key": moved.order_key });
    command_service
        .accept(claim.receipt_id, domain_agent_run_refs(&context), &response)
        .await?;
    Ok(Json(response))
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
    validate_agent_run_product_command(
        state.as_ref(),
        &context,
        &current_user,
        &body.command,
        ConversationCommandKind::ResumeMailbox,
    )
    .await?;
    let command_service =
        AgentRunProductCommandService::new(state.repos.agent_run_command_receipt_repo.as_ref());
    let claim = command_service
        .claim(
            context.run.id,
            context.agent.id,
            AgentRunCommandKind::MailboxResume,
            body.client_command_id.clone(),
            &body,
        )
        .await?;
    if claim.duplicate {
        return Ok(Json(replay_mailbox_response(&claim)?));
    }
    product_command_step(
        &command_service,
        claim.receipt_id,
        state
            .repos
            .agent_run_mailbox_repo
            .resume_state(context.run.id, context.agent.id)
            .await
            .map_err(ApiError::from),
    )
    .await?;
    let target = agent_run_runtime_target(&context);
    spawn_runtime_mailbox_watcher(state.clone(), target);
    let mut response = mailbox_control_response(
        body.client_command_id,
        AgentRunMessageCommandOutcome::Resumed,
        None,
    );
    response.accepted_refs = Some(contract_agent_run_refs(&context));
    command_service
        .accept(claim.receipt_id, domain_agent_run_refs(&context), &response)
        .await?;
    Ok(Json(response))
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
    let message = state
        .repos
        .agent_run_mailbox_repo
        .get_message(message_id)
        .await?
        .ok_or_else(|| ApiError::NotFound("mailbox message 不存在".to_string()))?;
    ensure_mailbox_message_target(&context, &message)?;
    if message.origin != agentdash_domain::agent_run_mailbox::MailboxMessageOrigin::User {
        return Err(ApiError::BadRequest(
            "只能召回 User 来源的消息内容".to_string(),
        ));
    }
    let input = message
        .payload_json
        .ok_or_else(|| ApiError::Conflict("mailbox message 内容已不可召回".to_string()))?;
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
    let view = validate_agent_run_product_command(
        state.as_ref(),
        &context,
        &current_user,
        &body.command,
        ConversationCommandKind::Cancel,
    )
    .await?;
    let command_service =
        AgentRunProductCommandService::new(state.repos.agent_run_command_receipt_repo.as_ref());
    let claim = command_service
        .claim(
            context.run.id,
            context.agent.id,
            AgentRunCommandKind::Cancel,
            body.client_command_id.clone(),
            &body,
        )
        .await?;
    if claim.duplicate {
        let mut response: AgentRunCommandReceipt =
            serde_json::from_value(replay_product_command_json(&claim, "取消")?)
                .map_err(|error| ApiError::Internal(error.to_string()))?;
        response.duplicate = true;
        return Ok(Json(response));
    }
    let command = guarded_agent_run_command_from_view(
        &context,
        &current_user,
        body.client_command_id.clone(),
        &view,
    )?;
    product_command_step(
        &command_service,
        claim.receipt_id,
        state
            .services
            .agent_run_runtime
            .interrupt_active_turn(command)
            .await
            .map_err(agent_run_runtime_error),
    )
    .await?;
    let response = AgentRunCommandReceipt {
        client_command_id: body.client_command_id,
        status: "accepted".to_string(),
        duplicate: false,
        message: None,
    };
    command_service
        .accept(claim.receipt_id, domain_agent_run_refs(&context), &response)
        .await?;
    Ok(Json(response))
}

async fn compact_agent_run_context(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path((run_id, agent_id)): Path<(String, String)>,
    Json(body): Json<AgentRunCommandOnlyRequest>,
) -> Result<Json<AgentRunContextCompactionCommandResponse>, ApiError> {
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
    let view = validate_agent_run_product_command(
        state.as_ref(),
        &context,
        &current_user,
        &body.command,
        ConversationCommandKind::CompactContext,
    )
    .await?;
    let command_service =
        AgentRunProductCommandService::new(state.repos.agent_run_command_receipt_repo.as_ref());
    let claim = command_service
        .claim(
            context.run.id,
            context.agent.id,
            AgentRunCommandKind::ContextCompact,
            body.client_command_id.clone(),
            &body,
        )
        .await?;
    if claim.duplicate {
        let mut response: AgentRunContextCompactionCommandResponse =
            serde_json::from_value(replay_product_command_json(&claim, "压缩")?)
                .map_err(|error| ApiError::Internal(error.to_string()))?;
        refresh_compaction_terminal(state.as_ref(), &mut response).await?;
        response.command_receipt.duplicate = true;
        return Ok(Json(response));
    }
    let snapshot = view
        .snapshot
        .as_ref()
        .ok_or_else(|| ApiError::Conflict("AgentRun runtime snapshot 不可用".to_string()))?;
    if snapshot.transcript.is_empty() {
        let response = AgentRunContextCompactionCommandResponse {
            command_receipt: AgentRunCommandReceipt {
                client_command_id: body.client_command_id,
                status: "accepted".to_string(),
                duplicate: false,
                message: None,
            },
            outcome: initial_compaction_outcome(true, snapshot.active_turn_id.is_some()),
            runtime_session_id: Some(snapshot.thread_id.to_string()),
            request_id: None,
            turn_id: None,
            message: Some("当前没有可压缩的消息。".to_string()),
        };
        command_service
            .accept(claim.receipt_id, domain_agent_run_refs(&context), &response)
            .await?;
        return Ok(Json(response));
    }
    let scheduled_for_active_turn = snapshot.active_turn_id.is_some();
    let command = guarded_agent_run_command_from_view(
        &context,
        &current_user,
        body.client_command_id.clone(),
        &view,
    )?;
    let runtime_result = if scheduled_for_active_turn {
        state
            .services
            .agent_run_runtime
            .schedule_context_compaction(command)
            .await
    } else {
        state
            .services
            .agent_run_runtime
            .compact_context(command)
            .await
    };
    let receipt = match runtime_result {
        Ok(receipt) => receipt,
        Err(error) => {
            let outcome = compact_error_outcome(&error);
            let blocked = outcome == AgentRunContextCompactionCommandOutcome::Blocked;
            let response = AgentRunContextCompactionCommandResponse {
                command_receipt: AgentRunCommandReceipt {
                    client_command_id: body.client_command_id,
                    status: if blocked {
                        "terminal_failed"
                    } else {
                        "accepted"
                    }
                    .to_string(),
                    duplicate: false,
                    message: None,
                },
                outcome,
                runtime_session_id: Some(snapshot.thread_id.to_string()),
                request_id: None,
                turn_id: None,
                message: Some(error.to_string()),
            };
            if blocked {
                command_service
                    .fail_with_result(claim.receipt_id, error.to_string(), &response)
                    .await?;
            } else {
                command_service
                    .accept(claim.receipt_id, domain_agent_run_refs(&context), &response)
                    .await?;
            }
            return Ok(Json(response));
        }
    };
    let mut response = AgentRunContextCompactionCommandResponse {
        command_receipt: AgentRunCommandReceipt {
            client_command_id: body.client_command_id,
            status: "accepted".to_string(),
            duplicate: receipt.duplicate,
            message: None,
        },
        outcome: initial_compaction_outcome(false, scheduled_for_active_turn),
        runtime_session_id: view
            .snapshot
            .as_ref()
            .map(|snapshot| snapshot.thread_id.to_string()),
        request_id: Some(receipt.operation_id.to_string()),
        turn_id: None,
        message: scheduled_for_active_turn
            .then(|| "已安排在当前 turn 结束后压缩上下文。".to_string()),
    };
    refresh_compaction_terminal(state.as_ref(), &mut response).await?;
    command_service
        .accept(claim.receipt_id, domain_agent_run_refs(&context), &response)
        .await?;
    Ok(Json(response))
}

fn initial_compaction_outcome(
    transcript_empty: bool,
    active_turn: bool,
) -> AgentRunContextCompactionCommandOutcome {
    if transcript_empty {
        AgentRunContextCompactionCommandOutcome::NoEligibleMessages
    } else if active_turn {
        AgentRunContextCompactionCommandOutcome::ScheduledNextTurn
    } else {
        AgentRunContextCompactionCommandOutcome::LaunchedCompactionTurn
    }
}

async fn refresh_compaction_terminal(
    state: &AppState,
    response: &mut AgentRunContextCompactionCommandResponse,
) -> Result<(), ApiError> {
    let Some(request_id) = response.request_id.as_deref() else {
        return Ok(());
    };
    let operation_id =
        agentdash_agent_runtime_contract::RuntimeOperationId::new(request_id.to_string())
            .map_err(|error| ApiError::Internal(error.to_string()))?;
    let Some(terminal) = state
        .services
        .agent_run_runtime
        .inspect_operation_terminal(operation_id)
        .await
        .map_err(agent_run_runtime_error)?
    else {
        return Ok(());
    };
    apply_compaction_terminal(response, terminal);
    Ok(())
}

fn apply_compaction_terminal(
    response: &mut AgentRunContextCompactionCommandResponse,
    terminal: agentdash_agent_runtime_contract::RuntimeOperationTerminal,
) {
    match terminal {
        agentdash_agent_runtime_contract::RuntimeOperationTerminal::Succeeded => {
            response.outcome = AgentRunContextCompactionCommandOutcome::Completed;
            response.message = Some("context compaction completed".to_string());
        }
        agentdash_agent_runtime_contract::RuntimeOperationTerminal::Failed { message, .. }
        | agentdash_agent_runtime_contract::RuntimeOperationTerminal::Lost { message, .. } => {
            response.outcome = AgentRunContextCompactionCommandOutcome::Failed;
            response.message = message.or_else(|| Some("context compaction failed".to_string()));
        }
    }
}

fn compact_error_outcome(error: &AgentRunRuntimeError) -> AgentRunContextCompactionCommandOutcome {
    use agentdash_agent_runtime_contract::RuntimeExecuteError;

    match error {
        AgentRunRuntimeError::BindingNotFound
        | AgentRunRuntimeError::StaleThread
        | AgentRunRuntimeError::StaleActiveTurn
        | AgentRunRuntimeError::ClientCommandConflict
        | AgentRunRuntimeError::Execute(
            RuntimeExecuteError::Unsupported { .. }
            | RuntimeExecuteError::InvalidCommand { .. }
            | RuntimeExecuteError::Incompatible { .. }
            | RuntimeExecuteError::RevisionConflict { .. }
            | RuntimeExecuteError::OperationConflict { .. }
            | RuntimeExecuteError::ContextCompactionInProgress { .. },
        ) => AgentRunContextCompactionCommandOutcome::Blocked,
        _ => AgentRunContextCompactionCommandOutcome::Failed,
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
struct AgentRunRuntimeInspectResponse {
    target: AgentRunRuntimeTarget,
    binding: Option<AgentRunRuntimeBinding>,
    snapshot: Option<RuntimeSnapshot>,
    binding_epoch: Option<agentdash_agent_runtime_contract::BindingEpoch>,
    recovery: agentdash_application_agentrun::agent_run::AgentRunRuntimeRecoverySummary,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
struct AgentRunRuntimeEventsQuery {
    after: Option<EventSequence>,
    #[serde(default)]
    include_transient: bool,
    transient_after: Option<agentdash_agent_runtime_contract::RuntimeTransientSequence>,
    stream_generation: Option<agentdash_agent_runtime_contract::RuntimeDriverGeneration>,
}

#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum AgentRunRuntimeEventStreamItem {
    Event {
        durable_cursor: Option<EventSequence>,
        transient_cursor: Option<agentdash_agent_runtime_contract::RuntimeTransientCoordinate>,
        envelope: Box<RuntimeEventEnvelope>,
    },
    Error {
        error: agentdash_agent_runtime_contract::RuntimeSubscribeError,
    },
}

async fn inspect_agent_run_runtime(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path((run_id, agent_id)): Path<(String, String)>,
) -> Result<Json<AgentRunRuntimeInspectResponse>, ApiError> {
    let context = resolve_agent_run_context(
        state.as_ref(),
        &current_user,
        &run_id,
        &agent_id,
        ProjectPermission::Use,
    )
    .await?;
    let AgentRunRuntimeView {
        target,
        binding,
        snapshot,
        binding_epoch,
        recovery,
    } = state
        .services
        .agent_run_runtime
        .inspect(agent_run_runtime_target(&context))
        .await
        .map_err(agent_run_runtime_error)?;
    Ok(Json(AgentRunRuntimeInspectResponse {
        target,
        binding,
        snapshot,
        binding_epoch,
        recovery,
    }))
}

async fn read_agent_run_runtime_context(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path((run_id, agent_id)): Path<(String, String)>,
) -> Result<Json<RuntimeContextView>, ApiError> {
    let context = resolve_agent_run_context(
        state.as_ref(),
        &current_user,
        &run_id,
        &agent_id,
        ProjectPermission::Use,
    )
    .await?;
    state
        .services
        .agent_run_runtime
        .read_context(agent_run_runtime_target(&context))
        .await
        .map(Json)
        .map_err(agent_run_runtime_error)
}

async fn get_agent_run_runtime_context_projection(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path((run_id, agent_id)): Path<(String, String)>,
) -> Result<impl IntoResponse, ApiError> {
    let context = resolve_agent_run_context(
        state.as_ref(),
        &current_user,
        &run_id,
        &agent_id,
        ProjectPermission::Use,
    )
    .await?;
    Ok(Json(
        runtime_traces::load_runtime_trace_context_projection(
            state.as_ref(),
            agent_run_runtime_target(&context),
        )
        .await?,
    ))
}

async fn stream_agent_run_runtime_events(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path((run_id, agent_id)): Path<(String, String)>,
    Query(query): Query<AgentRunRuntimeEventsQuery>,
) -> Result<Response, ApiError> {
    let context = resolve_agent_run_context(
        state.as_ref(),
        &current_user,
        &run_id,
        &agent_id,
        ProjectPermission::Use,
    )
    .await?;
    let mut events = state
        .services
        .agent_run_runtime
        .read_events(ReadAgentRunEvents {
            target: agent_run_runtime_target(&context),
            after: query.after,
            include_transient: query.include_transient,
            transient_after: query.transient_after,
            stream_generation: query.stream_generation,
        })
        .await
        .map_err(agent_run_runtime_error)?;
    let stream = async_stream::stream! {
        while let Some(next) = events.next().await {
            let terminal = next.is_err();
            let item = match next {
                Ok(envelope) => AgentRunRuntimeEventStreamItem::Event {
                    durable_cursor: envelope.sequence,
                    transient_cursor: envelope.transient.clone(),
                    envelope: Box::new(envelope),
                },
                Err(error) => AgentRunRuntimeEventStreamItem::Error { error },
            };
            if let Ok(mut bytes) = serde_json::to_vec(&item) {
                bytes.push(b'\n');
                yield Ok::<Bytes, Infallible>(Bytes::from(bytes));
            }
            if terminal {
                break;
            }
        }
    };
    Ok((
        [
            (
                axum::http::header::CONTENT_TYPE,
                "application/x-ndjson; charset=utf-8",
            ),
            (axum::http::header::CACHE_CONTROL, "no-cache, no-transform"),
            (axum::http::header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
        ],
        Body::from_stream(stream),
    )
        .into_response())
}

async fn list_agent_run_journal_events(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path((run_id, agent_id)): Path<(String, String)>,
    Query(query): Query<AgentRunJournalEventsQuery>,
) -> Result<Json<SessionEventsPageResponse>, ApiError> {
    let context = resolve_agent_run_context(
        state.as_ref(),
        &current_user,
        &run_id,
        &agent_id,
        ProjectPermission::Use,
    )
    .await?;
    let page = state
        .services
        .agent_run_journal
        .load_visible_journal_page(
            AgentRunJournalQuery {
                run_id: context.run.id,
                agent_id: context.agent.id,
            },
            query.after_seq.unwrap_or_default(),
            query.limit.unwrap_or(500).clamp(1, 2_000),
        )
        .await?;
    let journal_session_id =
        agentdash_application_agentrun::agent_run::agent_run_journal_session_id(
            context.run.id,
            context.agent.id,
        );
    Ok(Json(SessionEventsPageResponse {
        snapshot_seq: page.snapshot_seq,
        events: page
            .events
            .into_iter()
            .map(|event| journal_event_to_contract(event, &journal_session_id))
            .collect::<Result<Vec<_>, _>>()?,
        has_more: page.has_more,
        next_after_seq: page.next_after_seq,
    }))
}

async fn agent_run_journal_stream_route(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path((run_id, agent_id)): Path<(String, String)>,
    headers: HeaderMap,
    Query(query): Query<AgentRunJournalStreamQuery>,
) -> Result<Response, ApiError> {
    let context = resolve_agent_run_context(
        state.as_ref(),
        &current_user,
        &run_id,
        &agent_id,
        ProjectPermission::Use,
    )
    .await?;
    if let Some(presentation_thread_id) = context.presentation_thread_id.as_ref() {
        state.services.terminal_registry.bind_session(
            presentation_thread_id.as_str(),
            &context.run.id.to_string(),
            &context.agent.id.to_string(),
        );
    }
    let resume_from = parse_agent_run_journal_resume_from_header(&headers)?
        .or(query.since_id)
        .unwrap_or_default();
    let subscription = state
        .services
        .agent_run_journal
        .subscribe_visible_journal_stream(
            AgentRunJournalQuery {
                run_id: context.run.id,
                agent_id: context.agent.id,
            },
            resume_from,
        )
        .await?;
    let stream_state = subscription.state;
    let mut live = subscription.live;
    let journal_session_id = stream_state.journal_session_id.clone();
    let delivery_runtime_thread_id = stream_state.delivery_runtime_thread_id.clone();
    let stream = async_stream::stream! {
        let mut seq = resume_from;
        for event in stream_state.prefix_events.iter().cloned() {
            seq = event.journal_seq;
            let event = match journal_event_to_contract(event, &journal_session_id) {
                Ok(event) => event,
                Err(error) => {
                    let error_context = DiagnosticErrorContext::new(
                        "agent_run_journal.stream",
                        "project_inherited",
                    );
                    diag_error!(
                        Error,
                        Subsystem::Api,
                        context = &error_context,
                        error = &error,
                        run_id = %context.run.id,
                        agent_id = %context.agent.id,
                        "AgentRun inherited journal projection failed"
                    );
                    return;
                }
            };
            yield Ok::<Bytes, Infallible>(journal_ndjson_line(&SessionNdjsonEnvelope::event(event)));
        }
        for event in stream_state.backlog_events.iter().cloned() {
            seq = event.journal_seq;
            let event = match journal_event_to_contract(event, &journal_session_id) {
                Ok(event) => event,
                Err(error) => {
                    let error_context = DiagnosticErrorContext::new(
                        "agent_run_journal.stream",
                        "project_durable_backlog",
                    );
                    diag_error!(
                        Error,
                        Subsystem::Api,
                        context = &error_context,
                        error = &error,
                        run_id = %context.run.id,
                        agent_id = %context.agent.id,
                        "AgentRun durable journal projection failed"
                    );
                    return;
                }
            };
            yield Ok::<Bytes, Infallible>(journal_ndjson_line(&SessionNdjsonEnvelope::event(event)));
        }
        yield Ok::<Bytes, Infallible>(journal_ndjson_line(&SessionNdjsonEnvelope::connected(
            stream_state.connected_seq,
            stream_state.ephemeral_epoch,
        )));
        for event in stream_state.ephemeral_backlog_events.iter().cloned() {
            let event = match journal_event_to_contract(event, &journal_session_id) {
                Ok(event) => event,
                Err(error) => {
                    let error_context = DiagnosticErrorContext::new(
                        "agent_run_journal.stream",
                        "project_ephemeral_backlog",
                    );
                    diag_error!(
                        Error,
                        Subsystem::Api,
                        context = &error_context,
                        error = &error,
                        run_id = %context.run.id,
                        agent_id = %context.agent.id,
                        "AgentRun ephemeral journal projection failed"
                    );
                    return;
                }
            };
            yield Ok::<Bytes, Infallible>(journal_ndjson_line(&SessionNdjsonEnvelope::ephemeral_event(event)));
        }

        let mut heartbeat = tokio::time::interval(AGENT_RUN_JOURNAL_STREAM_HEARTBEAT_INTERVAL);
        heartbeat.set_missed_tick_behavior(MissedTickBehavior::Delay);
        loop {
            tokio::select! {
                next = live.recv() => match next {
                    Ok(record) => {
                        let envelope = match stream_state.project_live_record(record) {
                            Ok(AgentRunJournalLiveEvent::Durable(event)) => {
                                seq = event.journal_seq;
                                journal_event_to_contract(event, &journal_session_id)
                                    .map(SessionNdjsonEnvelope::event)
                            }
                            Ok(AgentRunJournalLiveEvent::Ephemeral(event)) => {
                                journal_event_to_contract(event, &journal_session_id)
                                    .map(SessionNdjsonEnvelope::ephemeral_event)
                            }
                            Ok(AgentRunJournalLiveEvent::StaleDurable | AgentRunJournalLiveEvent::Internal) => continue,
                            Err(error) => Err(ApiError::Internal(error.to_string())),
                        };
                        match envelope {
                            Ok(envelope) => yield Ok::<Bytes, Infallible>(journal_ndjson_line(&envelope)),
                            Err(error) => {
                                let error_context = DiagnosticErrorContext::new(
                                    "agent_run_journal.stream",
                                    "project_live",
                                );
                                diag_error!(
                                    Error,
                                    Subsystem::Api,
                                    context = &error_context,
                                    error = &error,
                                    run_id = %context.run.id,
                                    agent_id = %context.agent.id,
                                    runtime_thread_id = %delivery_runtime_thread_id,
                                    "AgentRun live journal projection failed"
                                );
                                break;
                            }
                        }
                    }
                    Err(error) => {
                        match agent_run_journal_live_receive_action(error) {
                            AgentRunJournalLiveReceiveAction::ContinueAfterLag(lagged) => {
                                diag!(Warn, Subsystem::Api,
                                    operation = "agent_run_journal.stream",
                                    stage = "live_lagged",
                                    run_id = %context.run.id,
                                    agent_id = %context.agent.id,
                                    runtime_thread_id = %delivery_runtime_thread_id,
                                    lagged,
                                    "AgentRun journal stream 订阅落后，继续等待后续事件"
                                );
                            }
                            AgentRunJournalLiveReceiveAction::BreakAfterClose => {
                                diag!(Info, Subsystem::Api,
                                    operation = "agent_run_journal.stream",
                                    stage = "live_closed",
                                    run_id = %context.run.id,
                                    agent_id = %context.agent.id,
                                    runtime_thread_id = %delivery_runtime_thread_id,
                                    last_seq = seq,
                                    "AgentRun journal stream 广播通道关闭"
                                );
                                break;
                            }
                        }
                    }
                },
                _ = heartbeat.tick() => {
                    yield Ok::<Bytes, Infallible>(journal_ndjson_line(&SessionNdjsonEnvelope::heartbeat_now()));
                }
            }
        }
    };
    Ok((
        [
            (
                axum::http::header::CONTENT_TYPE,
                "application/x-ndjson; charset=utf-8",
            ),
            (axum::http::header::CACHE_CONTROL, "no-cache, no-transform"),
            (axum::http::header::CONNECTION, "keep-alive"),
            (axum::http::header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
        ],
        Body::from_stream(stream),
    )
        .into_response())
}

fn journal_event_to_contract(
    event: AgentRunJournalEvent,
    journal_session_id: &str,
) -> Result<SessionEventResponse, ApiError> {
    let presentation = event.record.as_presentation().ok_or_else(|| {
        ApiError::Internal("internal Runtime fact entered session journal projection".to_string())
    })?;
    let carrier = event.record.carrier();
    let occurred_at_ms = i64::try_from(carrier.recorded_at_ms)
        .map_err(|_| ApiError::Internal("journal timestamp exceeds i64".to_string()))?;
    let observed_at = DateTime::<Utc>::from_timestamp_millis(occurred_at_ms)
        .ok_or_else(|| ApiError::Internal("journal timestamp is invalid".to_string()))?;
    let event_value = serde_json::to_value(&presentation.event)
        .map_err(|error| ApiError::Internal(format!("serialize journal event failed: {error}")))?;
    let session_update_type = event_value
        .get("type")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| {
            ApiError::Internal("serialized Backbone event has no typed discriminant".to_string())
        })?
        .to_string();
    Ok(SessionEventResponse {
        session_id: journal_session_id.to_string(),
        event_seq: event.journal_seq,
        occurred_at_ms,
        committed_at_ms: occurred_at_ms,
        session_update_type,
        turn_id: carrier.coordinate.source_turn_id.clone(),
        entry_index: carrier.coordinate.source_entry_index,
        tool_call_id: presentation_tool_call_id(&presentation.event, presentation.durability),
        notification: agentdash_agent_protocol::BackboneEnvelope {
            event: presentation.event.clone(),
            session_id: journal_session_id.to_string(),
            source: agentdash_agent_protocol::SourceInfo {
                connector_id: carrier
                    .coordinate
                    .source_thread_id
                    .clone()
                    .unwrap_or_else(|| event.source_runtime_thread_id.to_string()),
                connector_type: "managed_runtime".to_string(),
                executor_id: carrier.binding_id.as_ref().map(ToString::to_string),
            },
            trace: agentdash_agent_protocol::TraceInfo {
                turn_id: carrier.coordinate.source_turn_id.clone(),
                entry_index: carrier.coordinate.source_entry_index,
            },
            observed_at,
        },
    })
}

fn presentation_tool_call_id(
    event: &agentdash_agent_protocol::BackboneEvent,
    durability: agentdash_agent_runtime_contract::PresentationDurability,
) -> Option<String> {
    use agentdash_agent_protocol::BackboneEvent;
    use agentdash_agent_runtime_contract::PresentationDurability;
    let item = match (durability, event) {
        (PresentationDurability::Durable, BackboneEvent::ItemStarted(notification)) => {
            Some(&notification.item)
        }
        (PresentationDurability::Durable, BackboneEvent::ItemUpdated(notification))
        | (PresentationDurability::Ephemeral, BackboneEvent::ItemUpdated(notification)) => {
            Some(&notification.item)
        }
        (PresentationDurability::Durable, BackboneEvent::ItemCompleted(notification)) => {
            Some(&notification.item)
        }
        _ => None,
    };
    item.and_then(|item| item.tool_call_id().map(ToString::to_string))
}

fn parse_agent_run_journal_resume_from_header(
    headers: &HeaderMap,
) -> Result<Option<u64>, ApiError> {
    let Some(value) = headers.get("x-stream-since-id") else {
        return Ok(None);
    };
    let raw = value
        .to_str()
        .map_err(|_| ApiError::BadRequest("x-stream-since-id 不是有效 UTF-8".to_string()))?;
    let parsed = raw
        .parse::<i64>()
        .map_err(|_| ApiError::BadRequest("x-stream-since-id 不是有效整数".to_string()))?;
    if parsed < 0 {
        return Err(ApiError::BadRequest(
            "x-stream-since-id 不能为负数".to_string(),
        ));
    }
    Ok(Some(parsed as u64))
}

fn journal_ndjson_line(value: &SessionNdjsonEnvelope) -> Bytes {
    let mut bytes = serde_json::to_vec(value).expect("Session NDJSON envelope must serialize");
    bytes.push(b'\n');
    Bytes::from(bytes)
}

async fn list_agent_run_runtime_terminals(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path((run_id, agent_id)): Path<(String, String)>,
) -> Result<Json<Vec<TerminalState>>, ApiError> {
    let _ =
        resolve_agent_run_terminal_launch_target(&state, &current_user, &run_id, &agent_id).await?;
    Ok(Json(
        state
            .services
            .terminal_registry
            .list_terminals(&run_id, &agent_id),
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
    terminals::spawn_terminal_for_runtime_session(
        &state,
        &runtime_session_id,
        &run_id,
        &agent_id,
        launch_target,
        body,
    )
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
    // Ensure terminal registry knows this session -> AgentRun binding
    state
        .services
        .terminal_registry
        .bind_session(&runtime_session_id, run_id, agent_id);
    let launch_target =
        resolve_terminal_launch_target_for_runtime_session(state, &runtime_session_id).await?;
    if launch_target.project_id != context.run.project_id {
        return Err(ApiError::Conflict(format!(
            "AgentRun {} / {} 与 terminal runtime surface Project 不一致",
            context.run.id, context.agent.id
        )));
    }
    Ok((runtime_session_id, launch_target.target))
}

async fn get_agent_run_runtime_context_audit(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path((run_id, agent_id)): Path<(String, String)>,
    Query(query): Query<ContextAuditQuery>,
) -> Result<impl IntoResponse, ApiError> {
    // Validate the AgentRun exists and user has access
    let _runtime_session_id =
        resolve_agent_run_delivery_runtime(&state, &current_user, &run_id, &agent_id).await?;
    Ok(Json(
        runtime_traces::load_runtime_trace_context_audit(state.as_ref(), &run_id, &agent_id, query)
            .await?,
    ))
}

async fn respond_agent_run_interaction(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path((run_id, agent_id, interaction_id)): Path<(String, String, String)>,
    Json(response): Json<InteractionResponse>,
) -> Result<Json<OperationReceipt>, ApiError> {
    let context = resolve_agent_run_context(
        state.as_ref(),
        &current_user,
        &run_id,
        &agent_id,
        ProjectPermission::Use,
    )
    .await?;
    resolve_agent_run_interaction(
        state.as_ref(),
        &context,
        &current_user,
        interaction_id,
        response,
    )
    .await
    .map(Json)
}

#[derive(Debug, Deserialize)]
struct RejectToolApprovalRequest {
    #[serde(default)]
    reason: Option<String>,
}

async fn approve_agent_run_tool_call(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path((run_id, agent_id, tool_call_id)): Path<(String, String, String)>,
) -> Result<Json<AgentRunToolCallApprovalResponse>, ApiError> {
    let context = resolve_agent_run_context(
        state.as_ref(),
        &current_user,
        &run_id,
        &agent_id,
        ProjectPermission::Use,
    )
    .await?;
    let interaction_id =
        pending_approval_interaction_id(state.as_ref(), &context, &tool_call_id).await?;
    resolve_agent_run_interaction(
        state.as_ref(),
        &context,
        &current_user,
        interaction_id,
        InteractionResponse::Approved,
    )
    .await?;
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

async fn reject_agent_run_tool_call(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path((run_id, agent_id, tool_call_id)): Path<(String, String, String)>,
    Json(request): Json<RejectToolApprovalRequest>,
) -> Result<Json<AgentRunToolCallRejectionResponse>, ApiError> {
    let context = resolve_agent_run_context(
        state.as_ref(),
        &current_user,
        &run_id,
        &agent_id,
        ProjectPermission::Use,
    )
    .await?;
    let interaction_id =
        pending_approval_interaction_id(state.as_ref(), &context, &tool_call_id).await?;
    resolve_agent_run_interaction(
        state.as_ref(),
        &context,
        &current_user,
        interaction_id,
        InteractionResponse::Denied {
            reason: request.reason,
        },
    )
    .await?;
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

async fn pending_approval_interaction_id(
    state: &AppState,
    context: &AgentRunContext,
    tool_call_id: &str,
) -> Result<String, ApiError> {
    let view = state
        .services
        .agent_run_runtime
        .inspect(agent_run_runtime_target(context))
        .await
        .map_err(agent_run_runtime_error)?;
    exact_pending_approval_interaction_id(
        view.snapshot
            .into_iter()
            .flat_map(|snapshot| snapshot.pending_interaction_details)
            .map(|pending| (pending.interaction_id.to_string(), pending.request)),
        tool_call_id,
    )
}

fn exact_pending_approval_interaction_id(
    pending: impl IntoIterator<
        Item = (
            String,
            agentdash_agent_runtime_contract::RuntimeInteractionRequest,
        ),
    >,
    tool_call_id: &str,
) -> Result<String, ApiError> {
    let mut matches = pending
        .into_iter()
        .filter(|(_, request)| approval_request_item_id(request) == Some(tool_call_id))
        .map(|(interaction_id, _)| interaction_id);
    let interaction_id = matches.next().ok_or_else(|| {
        ApiError::NotFound(format!(
            "tool call {tool_call_id} 没有待处理 approval interaction"
        ))
    })?;
    if matches.next().is_some() {
        return Err(ApiError::Conflict(format!(
            "tool call {tool_call_id} 存在多个待处理 approval callback，必须使用 interaction endpoint 精确响应"
        )));
    }
    Ok(interaction_id)
}

fn approval_request_item_id(
    request: &agentdash_agent_runtime_contract::RuntimeInteractionRequest,
) -> Option<&str> {
    use agentdash_agent_runtime_contract::RuntimeInteractionRequest;
    match request {
        RuntimeInteractionRequest::CommandApproval { params } => Some(params.item_id.as_str()),
        RuntimeInteractionRequest::FileChangeApproval { params } => Some(params.item_id.as_str()),
        RuntimeInteractionRequest::PermissionApproval { params } => Some(params.item_id.as_str()),
        RuntimeInteractionRequest::UserInputRequest { .. }
        | RuntimeInteractionRequest::McpElicitation { .. }
        | RuntimeInteractionRequest::DynamicToolExecution { .. } => None,
    }
}

async fn resolve_agent_run_interaction(
    state: &AppState,
    context: &AgentRunContext,
    current_user: &agentdash_integration_api::AuthIdentity,
    interaction_id: String,
    response: InteractionResponse,
) -> Result<OperationReceipt, ApiError> {
    let interaction_id = RuntimeInteractionId::new(interaction_id)
        .map_err(|error| ApiError::BadRequest(format!("无效的 interaction_id: {error}")))?;
    let command = guarded_agent_run_command(
        state,
        context,
        current_user,
        format!("interaction-{}-{interaction_id}", current_user.user_id),
    )
    .await?;
    state
        .services
        .agent_run_runtime
        .resolve_interaction(ResolveAgentRunInteraction {
            command,
            interaction_id,
            response,
        })
        .await
        .map_err(agent_run_runtime_error)
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

fn agent_run_runtime_target(context: &AgentRunContext) -> AgentRunRuntimeTarget {
    AgentRunRuntimeTarget {
        run_id: context.run.id,
        agent_id: context.agent.id,
    }
}

fn runtime_agent_run_mailbox(state: &AppState) -> RuntimeAgentRunMailbox {
    RuntimeAgentRunMailbox::new(
        state.repos.agent_run_mailbox_repo.clone(),
        state.services.agent_run_runtime.clone(),
    )
}

fn spawn_runtime_mailbox_watcher(state: Arc<AppState>, target: AgentRunRuntimeTarget) {
    tokio::spawn(async move {
        let Ok(mut events) = state
            .services
            .agent_run_runtime
            .read_events(ReadAgentRunEvents {
                target: target.clone(),
                after: None,
                include_transient: false,
                transient_after: None,
                stream_generation: None,
            })
            .await
        else {
            return;
        };
        while let Some(event) = events.next().await {
            let Ok(event) = event else {
                return;
            };
            if !matches!(
                event.event,
                agentdash_agent_runtime_contract::RuntimeEvent::TurnTerminal { .. }
                    | agentdash_agent_runtime_contract::RuntimeEvent::InteractionTerminal { .. }
                    | agentdash_agent_runtime_contract::RuntimeEvent::ThreadStatusChanged { .. }
            ) {
                continue;
            }
            match runtime_agent_run_mailbox(state.as_ref())
                .recover_and_drain_once(&target)
                .await
            {
                Ok(Some(_)) | Err(_) => return,
                Ok(None) => {}
            }
        }
    });
}

fn runtime_actor(current_user: &agentdash_integration_api::AuthIdentity) -> RuntimeActor {
    RuntimeActor::User {
        subject: current_user.user_id.clone(),
    }
}

fn agent_run_command_guard(view: &AgentRunRuntimeView) -> Result<AgentRunCommandGuard, ApiError> {
    let binding = view.binding.as_ref().ok_or_else(|| {
        ApiError::Conflict(format!(
            "AgentRun {} / {} 尚未建立 runtime binding",
            view.target.run_id, view.target.agent_id
        ))
    })?;
    let snapshot = view.snapshot.as_ref().ok_or_else(|| {
        ApiError::Conflict(format!(
            "AgentRun {} / {} 尚无 canonical runtime snapshot",
            view.target.run_id, view.target.agent_id
        ))
    })?;
    if binding.thread_id != snapshot.thread_id {
        return Err(ApiError::Internal(
            "AgentRun runtime binding 与 snapshot thread 不一致".to_string(),
        ));
    }
    Ok(AgentRunCommandGuard {
        thread_id: snapshot.thread_id.clone(),
        expected_revision: snapshot.revision,
        expected_active_turn_id: snapshot.active_turn_id.clone(),
    })
}

async fn guarded_agent_run_command(
    state: &AppState,
    context: &AgentRunContext,
    current_user: &agentdash_integration_api::AuthIdentity,
    client_command_id: String,
) -> Result<GuardedAgentRunCommand, ApiError> {
    let target = agent_run_runtime_target(context);
    let view = state
        .services
        .agent_run_runtime
        .inspect(target.clone())
        .await
        .map_err(agent_run_runtime_error)?;
    Ok(GuardedAgentRunCommand {
        target,
        client_command_id,
        guard: agent_run_command_guard(&view)?,
        actor: runtime_actor(current_user),
    })
}

async fn validate_agent_run_product_command(
    state: &AppState,
    context: &AgentRunContext,
    current_user: &agentdash_integration_api::AuthIdentity,
    command: &AgentRunCommandPreconditionView,
    expected_kind: ConversationCommandKind,
) -> Result<AgentRunRuntimeView, ApiError> {
    if expected_kind != ConversationCommandKind::SubmitMessage
        && (context.run.created_by_user_id != current_user.user_id
            || context.agent.created_by_user_id != current_user.user_id)
    {
        return Err(ApiError::Forbidden(
            "只有 AgentRun 所有者可以执行会话命令".to_string(),
        ));
    }
    if command.command_kind != expected_kind {
        return Err(ApiError::Conflict(
            "stale_command: command kind 已变化".to_string(),
        ));
    }
    if command.stale_guard.run_id != context.run.id.to_string()
        || command.stale_guard.agent_id != context.agent.id.to_string()
    {
        return Err(ApiError::Conflict(
            "stale_command: command target 已变化".to_string(),
        ));
    }

    let target = agent_run_runtime_target(context);
    let view = state
        .services
        .agent_run_runtime
        .inspect(target)
        .await
        .map_err(agent_run_runtime_error)?;
    let workspace = super::agent_run_workspace::load(
        state,
        context.run.clone(),
        context.agent.clone(),
        &current_user.user_id,
    )
    .await?;
    let expected = workspace
        .conversation
        .as_ref()
        .and_then(|conversation| {
            conversation
                .commands
                .commands
                .iter()
                .find(|candidate| candidate.kind == expected_kind)
        })
        .ok_or_else(|| ApiError::Conflict("stale_command: command 已不可用".to_string()))?;
    if command.command_id != expected.command_id
        || command.stale_guard.snapshot_id != expected.stale_guard.snapshot_id
        || command.stale_guard.frame_id != expected.stale_guard.frame_id
        || command.stale_guard.active_turn_id != expected.stale_guard.active_turn_id
    {
        return Err(ApiError::Conflict(
            "stale_command: workspace snapshot 已变化，请刷新后重试".to_string(),
        ));
    }
    Ok(view)
}

fn guarded_agent_run_command_from_view(
    context: &AgentRunContext,
    current_user: &agentdash_integration_api::AuthIdentity,
    client_command_id: String,
    view: &AgentRunRuntimeView,
) -> Result<GuardedAgentRunCommand, ApiError> {
    Ok(GuardedAgentRunCommand {
        target: agent_run_runtime_target(context),
        client_command_id,
        guard: agent_run_command_guard(view)?,
        actor: runtime_actor(current_user),
    })
}

pub(crate) fn runtime_input_from_codex(
    input: Vec<codex::UserInput>,
) -> Result<Vec<RuntimeInput>, ApiError> {
    let input = input
        .into_iter()
        .filter_map(|item| match item {
            codex::UserInput::Text { text, .. } => {
                let text = text.trim().to_string();
                (!text.is_empty()).then_some(RuntimeInput::Text { text })
            }
            codex::UserInput::Image { url, .. } => Some(RuntimeInput::Image {
                mime_type: runtime_image_mime_type(&url),
                data_url: url,
            }),
            codex::UserInput::LocalImage { path, .. } => Some(RuntimeInput::FileReference {
                uri: path,
                media_type: Some("image".to_string()),
            }),
            codex::UserInput::Skill { name, path } => Some(RuntimeInput::FileReference {
                uri: path,
                media_type: Some(format!("application/x-agent-skill; name={name}")),
            }),
            codex::UserInput::Mention { name, path } => Some(RuntimeInput::FileReference {
                uri: path,
                media_type: Some(format!("application/x-agent-mention; name={name}")),
            }),
        })
        .collect::<Vec<_>>();
    if input.is_empty() {
        return Err(ApiError::BadRequest(
            "input 中没有可投递到 Agent Runtime 的内容".to_string(),
        ));
    }
    Ok(input)
}

fn runtime_image_mime_type(url: &str) -> String {
    url.strip_prefix("data:")
        .and_then(|value| value.split_once([',', ';']))
        .map(|(mime_type, _)| mime_type.trim())
        .filter(|mime_type| !mime_type.is_empty())
        .unwrap_or("application/octet-stream")
        .to_string()
}

pub(crate) fn agent_run_runtime_error(error: AgentRunRuntimeError) -> ApiError {
    use agentdash_agent_runtime_contract::{
        RuntimeExecuteError as Execute, RuntimePresentationAppendError as PresentationAppend,
        RuntimeSnapshotError as Snapshot, RuntimeSubscribeError as Events,
    };
    match error {
        AgentRunRuntimeError::BindingNotFound => {
            ApiError::Conflict("AgentRun 尚未建立 runtime binding".to_string())
        }
        AgentRunRuntimeError::Binding(error) => match error {
            AgentRunRuntimeBindingError::NotFound => {
                ApiError::NotFound("AgentRun runtime binding 不存在".to_string())
            }
            AgentRunRuntimeBindingError::Conflict => {
                ApiError::Conflict("AgentRun runtime binding 坐标冲突".to_string())
            }
            AgentRunRuntimeBindingError::Unavailable { reason, .. } => {
                ApiError::ServiceUnavailable(reason)
            }
            AgentRunRuntimeBindingError::Persistence { .. } => {
                ApiError::Internal("AgentRun runtime binding 持久化失败".to_string())
            }
        },
        AgentRunRuntimeError::Execute(error) => match error {
            Execute::Unsupported { .. }
            | Execute::InvalidCommand { .. }
            | Execute::Incompatible { .. } => ApiError::UnprocessableEntity(error.to_string()),
            Execute::Unavailable { reason, .. } => ApiError::ServiceUnavailable(reason),
            Execute::RevisionConflict { .. }
            | Execute::OperationConflict { .. }
            | Execute::ContextCompactionInProgress { .. } => ApiError::Conflict(error.to_string()),
            Execute::Persistence { .. } => ApiError::Internal(
                "Agent Runtime command acceptance persistence failed".to_string(),
            ),
        },
        AgentRunRuntimeError::Snapshot(error) => match error {
            Snapshot::NotFound => ApiError::NotFound("Agent Runtime thread 不存在".to_string()),
            Snapshot::RevisionUnavailable { .. } | Snapshot::ContextRevisionUnavailable { .. } => {
                ApiError::Conflict(error.to_string())
            }
            Snapshot::InconsistentContext { .. } => {
                ApiError::Internal("Agent Runtime context snapshot 不一致".to_string())
            }
            Snapshot::Unavailable { reason } => ApiError::ServiceUnavailable(reason),
        },
        AgentRunRuntimeError::Events(error) => match error {
            Events::NotFound => ApiError::NotFound("Agent Runtime event stream 不存在".to_string()),
            Events::InvalidCursor => {
                ApiError::BadRequest("无效的 runtime event cursor".to_string())
            }
            Events::CursorGap { .. } => ApiError::Conflict(error.to_string()),
            Events::Unavailable { reason, .. } => ApiError::ServiceUnavailable(reason),
        },
        AgentRunRuntimeError::PresentationAppend(error) => match error {
            PresentationAppend::Invalid(message) => ApiError::BadRequest(message),
            PresentationAppend::IdempotencyConflict => ApiError::Conflict(error.to_string()),
            PresentationAppend::ThreadNotFound => {
                ApiError::NotFound("Agent Runtime thread 不存在".to_string())
            }
            PresentationAppend::Unavailable => ApiError::ServiceUnavailable(error.to_string()),
        },
        AgentRunRuntimeError::StaleThread
        | AgentRunRuntimeError::StaleActiveTurn
        | AgentRunRuntimeError::StalePresentationTurn
        | AgentRunRuntimeError::ClientCommandConflict => ApiError::Conflict(error.to_string()),
        AgentRunRuntimeError::UnexpectedSnapshot => {
            ApiError::Internal("Agent Runtime 返回了非预期 snapshot 类型".to_string())
        }
        AgentRunRuntimeError::EmptyClientCommandId => {
            ApiError::BadRequest("client_command_id 不能为空".to_string())
        }
        AgentRunRuntimeError::InvalidPresentationInput => ApiError::BadRequest(error.to_string()),
    }
}

fn runtime_mailbox_error(error: RuntimeMailboxError) -> ApiError {
    match error {
        RuntimeMailboxError::Runtime(error) => agent_run_runtime_error(error),
        RuntimeMailboxError::InvalidPayload(message) => ApiError::Internal(message),
        RuntimeMailboxError::Persistence(error) => ApiError::from(error),
    }
}

#[cfg(test)]
mod journal_projection_tests {
    use super::*;
    use agentdash_agent_runtime_contract::{
        EventSequence, ImmutablePresentationEvent, PresentationDurability, RuntimeCarrierMetadata,
        RuntimeJournalFact, RuntimeJournalRecord, RuntimePresentationCoordinate, RuntimeRevision,
        RuntimeThreadId,
    };
    use agentdash_agent_runtime_test_support::session_parity::{
        PresentationDurability as ParityDurability, compare_ordered_presentation_events,
        normalize_current_presentation_event, normalize_main_ndjson_frame,
        normalize_main_session_event,
    };

    #[test]
    fn context_compaction_initial_outcomes_cover_main_behavior() {
        assert_eq!(
            initial_compaction_outcome(true, false),
            AgentRunContextCompactionCommandOutcome::NoEligibleMessages
        );
        assert_eq!(
            initial_compaction_outcome(false, true),
            AgentRunContextCompactionCommandOutcome::ScheduledNextTurn
        );
        assert_eq!(
            initial_compaction_outcome(false, false),
            AgentRunContextCompactionCommandOutcome::LaunchedCompactionTurn
        );
    }

    #[test]
    fn context_compaction_terminal_outcomes_cover_completed_and_failed() {
        let mut response = compaction_response();
        apply_compaction_terminal(
            &mut response,
            agentdash_agent_runtime_contract::RuntimeOperationTerminal::Succeeded,
        );
        assert_eq!(
            response.outcome,
            AgentRunContextCompactionCommandOutcome::Completed
        );

        apply_compaction_terminal(
            &mut response,
            agentdash_agent_runtime_contract::RuntimeOperationTerminal::Failed {
                retryable: false,
                message: Some("driver rejected compaction".to_string()),
            },
        );
        assert_eq!(
            response.outcome,
            AgentRunContextCompactionCommandOutcome::Failed
        );
        assert_eq!(
            response.message.as_deref(),
            Some("driver rejected compaction")
        );
    }

    #[test]
    fn context_compaction_distinguishes_blocked_from_failed_acceptance() {
        let operation_id = agentdash_agent_runtime_contract::RuntimeOperationId::new("op-1")
            .expect("operation id");
        assert_eq!(
            compact_error_outcome(&AgentRunRuntimeError::Execute(
                agentdash_agent_runtime_contract::RuntimeExecuteError::ContextCompactionInProgress {
                    operation_id,
                },
            )),
            AgentRunContextCompactionCommandOutcome::Blocked
        );
        assert_eq!(
            compact_error_outcome(&AgentRunRuntimeError::Execute(
                agentdash_agent_runtime_contract::RuntimeExecuteError::Unavailable {
                    reason: "driver offline".to_string(),
                    retryable: true,
                },
            )),
            AgentRunContextCompactionCommandOutcome::Failed
        );
    }

    fn compaction_response() -> AgentRunContextCompactionCommandResponse {
        AgentRunContextCompactionCommandResponse {
            command_receipt: AgentRunCommandReceipt {
                client_command_id: "compact-1".to_string(),
                status: "accepted".to_string(),
                duplicate: false,
                message: None,
            },
            outcome: AgentRunContextCompactionCommandOutcome::LaunchedCompactionTurn,
            runtime_session_id: Some("runtime-1".to_string()),
            request_id: Some("op-1".to_string()),
            turn_id: None,
            message: None,
        }
    }

    #[test]
    fn journal_projection_matches_fixed_main_replay_golden_strictly() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../agentdash-agent-runtime-test-support/fixtures/session-parity/main/journal-replay.json"
        ))
        .expect("Main journal replay fixture");
        let main_frame = fixture["frames"][0].clone();
        let protected: agentdash_agent_protocol::BackboneEvent =
            serde_json::from_value(main_frame["notification"]["event"].clone())
                .expect("typed Main protected event");
        let thread_id = RuntimeThreadId::new("runtime-thread").expect("thread id");
        let record = RuntimeJournalRecord::new(
            RuntimeCarrierMetadata {
                thread_id: thread_id.clone(),
                recorded_at_ms: 1_783_684_800_000,
                sequence: Some(EventSequence(7)),
                transient: None,
                revision: RuntimeRevision(4),
                operation_id: None,
                binding_id: None,
                append_idempotency_key: None,
                coordinate: RuntimePresentationCoordinate {
                    runtime_turn_id: None,
                    runtime_item_id: None,
                    interaction_id: None,
                    source_thread_id: Some("main-journal-fixture".to_string()),
                    source_turn_id: Some("turn-main-journal-1".to_string()),
                    source_item_id: Some("source-item".to_string()),
                    source_request_id: None,
                    source_entry_index: Some(0),
                },
            },
            RuntimeJournalFact::Presentation(ImmutablePresentationEvent::new(
                PresentationDurability::Durable,
                protected.clone(),
            )),
        )
        .expect("presentation record");
        let response = journal_event_to_contract(
            AgentRunJournalEvent {
                journal_seq: 1,
                segment_role: agentdash_application_agentrun::agent_run::AgentRunJournalSegmentRole::CurrentDelivery,
                source_runtime_thread_id: thread_id,
                source_event_seq: Some(EventSequence(7)),
                record,
            },
            "agentrun:11111111-1111-1111-1111-111111111111:22222222-2222-2222-2222-222222222222",
        )
        .expect("journal projection");
        assert_eq!(response.entry_index, Some(0));
        assert_eq!(response.notification.trace.entry_index, Some(0));
        assert_eq!(response.occurred_at_ms, 1_783_684_800_000);
        assert_eq!(response.committed_at_ms, 1_783_684_800_000);
        assert_eq!(
            response.tool_call_id, None,
            "source item identity must not be exposed as a tool call id"
        );
        let ndjson = normalize_main_ndjson_frame(
            serde_json::to_value(SessionNdjsonEnvelope::event(response.clone()))
                .expect("NDJSON wrapper"),
        )
        .expect("normalize NDJSON wrapper")
        .expect("event frame");

        let main = normalize_main_session_event(main_frame, ParityDurability::Durable)
            .expect("normalize fixed Main wrapper");
        let current = normalize_current_presentation_event(
            serde_json::json!({
                "runtime_thread_id": "another-runtime-thread",
                "runtime_revision": 99,
                "durable_sequence": 42,
                "presentation_event": protected,
            }),
            ParityDurability::Durable,
        )
        .expect("normalize immutable carrier");
        compare_ordered_presentation_events(&[main], &[current.clone()])
            .expect("protected event body must be byte-shape equivalent");
        compare_ordered_presentation_events(&[ndjson], &[current])
            .expect("GET and NDJSON must expose the same protected body");
    }

    #[test]
    fn resume_header_matches_main_validation() {
        let mut headers = HeaderMap::new();
        headers.insert("x-stream-since-id", "17".parse().expect("header"));
        assert_eq!(
            parse_agent_run_journal_resume_from_header(&headers).expect("valid cursor"),
            Some(17)
        );
        headers.insert("x-stream-since-id", "-1".parse().expect("header"));
        assert!(parse_agent_run_journal_resume_from_header(&headers).is_err());
    }

    #[tokio::test]
    async fn journal_controls_and_retention_gap_match_fixed_main_control_golden() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../agentdash-agent-runtime-test-support/fixtures/session-parity/main/journal-control.json"
        ))
        .expect("Main journal control fixture");
        let connected = serde_json::to_value(SessionNdjsonEnvelope::connected(4, 77))
            .expect("connected envelope");
        assert_eq!(connected, fixture["controls"]["connected"]);
        let heartbeat = serde_json::to_value(SessionNdjsonEnvelope::Heartbeat {
            timestamp: 1_783_684_800_015,
        })
        .expect("heartbeat envelope");
        assert_eq!(heartbeat, fixture["controls"]["heartbeat"]);

        let (sender, mut receiver) = tokio::sync::broadcast::channel(1);
        sender.send(1_u8).expect("first event");
        sender.send(2_u8).expect("second event");
        let lagged = receiver.recv().await.expect_err("receiver must lag");
        assert_eq!(
            agent_run_journal_live_receive_action(lagged),
            AgentRunJournalLiveReceiveAction::ContinueAfterLag(1)
        );
        assert_eq!(fixture["controls"]["lagged"]["action"], "continue");
        assert_eq!(receiver.recv().await.expect("latest event"), 2);
        drop(sender);
        let closed = receiver.recv().await.expect_err("receiver must close");
        assert_eq!(
            agent_run_journal_live_receive_action(closed),
            AgentRunJournalLiveReceiveAction::BreakAfterClose
        );
        assert_eq!(fixture["controls"]["closed"]["action"], "break");

        let earliest_available = fixture["retention_gap"]["earliest_available"]
            .as_u64()
            .expect("earliest available");
        let error = crate::app_state::ensure_agent_run_journal_full_history_available(
            EventSequence(earliest_available),
            "full refresh",
        )
        .expect_err("retention gap must fail full refresh");
        assert!(matches!(
            error,
            agentdash_application_agentrun::WorkflowApplicationError::Conflict(_)
        ));
        assert_eq!(fixture["retention_gap"]["expected_error"], "conflict");
    }

    #[test]
    fn internal_runtime_fact_cannot_enter_session_projection() {
        let thread_id = RuntimeThreadId::new("runtime-thread").expect("thread id");
        let record = RuntimeJournalRecord::new(
            RuntimeCarrierMetadata {
                thread_id: thread_id.clone(),
                recorded_at_ms: 1_783_684_800_123,
                sequence: Some(EventSequence(8)),
                transient: None,
                revision: RuntimeRevision(5),
                operation_id: None,
                binding_id: None,
                append_idempotency_key: None,
                coordinate: RuntimePresentationCoordinate {
                    runtime_turn_id: None,
                    runtime_item_id: None,
                    interaction_id: None,
                    source_thread_id: None,
                    source_turn_id: None,
                    source_item_id: None,
                    source_request_id: None,
                    source_entry_index: None,
                },
            },
            RuntimeJournalFact::Internal(
                agentdash_agent_runtime_contract::RuntimeEvent::ThreadStatusChanged {
                    status: agentdash_agent_runtime_contract::RuntimeThreadStatus::Active,
                },
            ),
        )
        .expect("internal record");
        let projected = journal_event_to_contract(
            AgentRunJournalEvent {
                journal_seq: 4,
                segment_role: agentdash_application_agentrun::agent_run::AgentRunJournalSegmentRole::CurrentDelivery,
                source_runtime_thread_id: thread_id,
                source_event_seq: Some(EventSequence(8)),
                record,
            },
            "agentrun:run:agent",
        );
        assert!(projected.is_err());
    }

    #[test]
    fn tool_call_id_matches_main_item_lifecycle_rule() {
        let item = agentdash_agent_protocol::AgentDashThreadItem::Codex(
            agentdash_agent_protocol::backbone::thread_item::dynamic_tool_call(
                "tool-call-id",
                "fixture",
                serde_json::json!({}),
                agentdash_agent_protocol::DynamicToolCallStatus::InProgress,
                None,
                None,
            ),
        );
        let started = agentdash_agent_protocol::BackboneEvent::ItemStarted(
            agentdash_agent_protocol::ItemStartedNotification {
                item: item.clone(),
                thread_id: "thread".to_string(),
                turn_id: "turn".to_string(),
                started_at_ms: 1,
            },
        );
        let updated = agentdash_agent_protocol::BackboneEvent::ItemUpdated(
            agentdash_agent_protocol::ItemUpdatedNotification {
                item,
                thread_id: "thread".to_string(),
                turn_id: "turn".to_string(),
                updated_at_ms: 2,
            },
        );
        assert_eq!(
            presentation_tool_call_id(&started, PresentationDurability::Durable).as_deref(),
            Some("tool-call-id")
        );
        assert_eq!(
            presentation_tool_call_id(&started, PresentationDurability::Ephemeral),
            None
        );
        assert_eq!(
            presentation_tool_call_id(&updated, PresentationDurability::Ephemeral).as_deref(),
            Some("tool-call-id")
        );
    }

    #[test]
    fn legacy_approval_maps_item_id_to_the_exact_distinct_interaction_id() {
        let request =
            agentdash_agent_runtime_contract::RuntimeInteractionRequest::temporary_command_approval(
                "thread-1",
                "turn-1",
                "tool-item-1",
                "echo exact",
            );
        assert_eq!(
            exact_pending_approval_interaction_id(
                [("interaction-callback-9".to_string(), request)],
                "tool-item-1",
            )
            .expect("exact approval"),
            "interaction-callback-9"
        );
    }

    #[test]
    fn legacy_approval_returns_not_found_without_an_exact_item_match() {
        let request =
            agentdash_agent_runtime_contract::RuntimeInteractionRequest::FileChangeApproval {
                params: Box::new(
                    serde_json::from_value(serde_json::json!({
                        "threadId": "thread-1", "turnId": "turn-1", "itemId": "another-item",
                        "grantRoot": null, "reason": "write", "startedAtMs": 1
                    }))
                    .expect("file approval"),
                ),
            };
        assert!(matches!(
            exact_pending_approval_interaction_id(
                [("interaction-1".to_string(), request)],
                "missing-item",
            ),
            Err(ApiError::NotFound(_))
        ));
    }

    #[test]
    fn legacy_approval_returns_conflict_for_ambiguous_callbacks() {
        let first =
            agentdash_agent_runtime_contract::RuntimeInteractionRequest::temporary_command_approval(
                "thread-1",
                "turn-1",
                "shared-item",
                "echo first",
            );
        let second = agentdash_agent_runtime_contract::RuntimeInteractionRequest::temporary_permission_approval(
            "thread-1",
            "turn-1",
            "shared-item",
            "permission".to_string(),
        );
        assert!(matches!(
            exact_pending_approval_interaction_id(
                [
                    ("interaction-1".to_string(), first),
                    ("interaction-2".to_string(), second),
                ],
                "shared-item",
            ),
            Err(ApiError::Conflict(_))
        ));
    }
}

fn delivery_runtime_session_from_agent_run_context(
    context: &AgentRunContext,
) -> Result<String, ApiError> {
    context
        .presentation_thread_id
        .as_ref()
        .map(ToString::to_string)
        .ok_or_else(|| {
            ApiError::Conflict(format!(
                "AgentRun {} / {} 缺少 delivery runtime",
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
        presentation_thread_id: delivery_runtime
            .as_ref()
            .map(|delivery| delivery.presentation_thread_id.clone()),
    })
}

async fn delivery_runtime_session_for_agent_run(
    state: &AppState,
    run_id: Uuid,
    agent_id: Uuid,
) -> Result<Option<AgentRunDeliveryRuntimeContext>, ApiError> {
    let binding = state
        .repos
        .agent_run_runtime_binding_repo
        .load(&AgentRunRuntimeTarget { run_id, agent_id })
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?;
    Ok(binding.map(|binding| AgentRunDeliveryRuntimeContext {
        presentation_thread_id: binding.presentation_thread_id,
    }))
}

fn parse_uuid(raw: &str, field: &str) -> Result<Uuid, ApiError> {
    Uuid::parse_str(raw).map_err(|_| ApiError::BadRequest(format!("无效的 {field}: {raw}")))
}
