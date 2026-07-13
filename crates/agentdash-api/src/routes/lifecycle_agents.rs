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
use agentdash_application::agent_run_product::{
    AgentRunCurrentFrameModel, AgentRunProductLineageAgentModel, AgentRunProductLineageModel,
    AgentRunProductLineageRuntimeModel, AgentRunProductModel, AgentRunProductQueryInput,
};
use agentdash_application_agentrun::agent_run::terminal_registry::TerminalState;
use agentdash_application_agentrun::agent_run::{
    AgentRunCommandGuard, AgentRunJournalEvent, AgentRunJournalLiveEvent, AgentRunJournalQuery,
    AgentRunPresentationInput, AgentRunRuntimeError, AgentRunRuntimeView,
    ConversationModelConfigSourceModel, ConversationModelConfigStatusModel,
    EnqueueRuntimeMailboxMessage, GuardedAgentRunCommand, ReadAgentRunEvents,
    ResolveAgentRunInteraction, RuntimeAgentRunMailbox, RuntimeMailboxError,
    RuntimeMailboxSubmitOutcome, SteerAgentRunTurn,
};
use agentdash_application_ports::agent_run_runtime::{
    AgentRunRuntimeBinding, AgentRunRuntimeBindingError, AgentRunRuntimeTarget,
};
use agentdash_application_ports::agent_run_surface::AgentRunTerminalLaunchTarget;
use agentdash_contracts::agent_run_mailbox::{
    AgentRunCommandReceipt, AgentRunComposerDeliveryIntent, AgentRunComposerSubmitRequest,
    AgentRunMessageCommandOutcome, AgentRunMessageCommandResponse,
};
use agentdash_contracts::session::{
    SessionEventResponse, SessionEventsPageResponse, SessionNdjsonEnvelope,
};
use agentdash_contracts::workflow::{
    AgentFrameRefDto, AgentRunCurrentFrameView, AgentRunListChildView, AgentRunListEntryView,
    AgentRunListRuntimeSummaryView, AgentRunListRuntimeThreadStatus,
    AgentRunProductLineageAgentView, AgentRunProductLineageView, AgentRunProductShellView,
    AgentRunProductView, AgentRunRefDto, AgentRunRuntimeCommandRequest,
    ConversationEffectiveExecutorConfigView, ConversationModelConfigSource,
    ConversationModelConfigStatus, ConversationModelConfigView, LifecycleRunRefDto,
    ProjectAgentRunListView, SubjectRefDto,
};
use agentdash_domain::workflow::{LifecycleAgent, LifecycleRun};
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
    vfs_surface_runtime::ApiVfsSurfaceRuntimeProjection,
};

struct AgentRunContext {
    run: LifecycleRun,
    agent: LifecycleAgent,
    presentation_thread_id: Option<PresentationThreadId>,
}

struct AgentRunDeliveryRuntimeContext {
    presentation_thread_id: PresentationThreadId,
}

const AGENT_RUN_JOURNAL_STREAM_HEARTBEAT_INTERVAL: Duration = Duration::from_secs(15);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AgentRunJournalLiveReceiveAction {
    ContinueAfterLag(u64),
    BreakAfterClose,
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
            "/agent-runs/{run_id}/agents/{agent_id}/composer-submit",
            axum::routing::post(submit_agent_run_composer_input),
        )
        .route(
            "/agent-runs/{run_id}/agents/{agent_id}/cancel",
            axum::routing::post(cancel_agent_run),
        )
        .route(
            "/agent-runs/{run_id}/agents/{agent_id}/runtime",
            axum::routing::get(inspect_agent_run_runtime),
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
) -> Result<Json<AgentRunProductView>, ApiError> {
    let context = resolve_agent_run_context(
        state.as_ref(),
        &current_user,
        &run_id,
        &agent_id,
        ProjectPermission::Use,
    )
    .await?;
    let runtime_projection = ApiVfsSurfaceRuntimeProjection::new(
        state.services.backend_registry.clone(),
        state.services.mount_provider_registry.clone(),
    );
    let product = state
        .services
        .agent_run_product_query
        .get(AgentRunProductQueryInput {
            run: &context.run,
            agent: &context.agent,
            has_runtime_binding: context.presentation_thread_id.is_some(),
            runtime_projection: &runtime_projection,
        })
        .await?;

    Ok(Json(agent_run_product_to_contract(product)))
}

fn agent_run_product_to_contract(model: AgentRunProductModel) -> AgentRunProductView {
    AgentRunProductView {
        run_ref: LifecycleRunRefDto {
            run_id: model.run_id.clone(),
        },
        agent_ref: AgentRunRefDto {
            run_id: model.run_id,
            agent_id: model.agent_id,
        },
        project_id: model.project_id,
        shell: AgentRunProductShellView {
            display_title: model.shell.display_title,
            title_source: model.shell.title_source,
            lifecycle_status: model.shell.lifecycle_status,
            last_activity_at: model.shell.last_activity_at,
        },
        agent: super::lifecycle_contracts::agent_run_to_contract(model.agent),
        current_frame: model.current_frame.map(agent_run_current_frame_to_contract),
        subject_associations: model
            .subject_associations
            .into_iter()
            .map(super::lifecycle_contracts::subject_association_to_contract)
            .collect(),
        lineage: agent_run_product_lineage_to_contract(model.lineage),
        resource_surface: model
            .resource_surface
            .map(super::vfs_surfaces::dto::surface_from_application),
    }
}

fn agent_run_product_lineage_to_contract(
    model: AgentRunProductLineageModel,
) -> AgentRunProductLineageView {
    AgentRunProductLineageView {
        parent: model
            .parent
            .map(agent_run_product_lineage_agent_to_contract),
        children: model
            .children
            .into_iter()
            .map(agent_run_product_lineage_agent_to_contract)
            .collect(),
    }
}

fn agent_run_product_lineage_agent_to_contract(
    model: AgentRunProductLineageAgentModel,
) -> AgentRunProductLineageAgentView {
    AgentRunProductLineageAgentView {
        run_ref: LifecycleRunRefDto {
            run_id: model.run_id.clone(),
        },
        agent_ref: AgentRunRefDto {
            run_id: model.run_id,
            agent_id: model.agent_id,
        },
        title: model.title,
        lifecycle_status: model.lifecycle_status,
        last_activity_at: model.last_activity_at,
        runtime: model
            .runtime
            .map(agent_run_product_lineage_runtime_to_contract),
        children: model
            .children
            .into_iter()
            .map(agent_run_product_lineage_agent_to_contract)
            .collect(),
    }
}

fn agent_run_product_lineage_runtime_to_contract(
    model: AgentRunProductLineageRuntimeModel,
) -> AgentRunListRuntimeSummaryView {
    agent_run_runtime_summary_to_contract(model.thread_status, model.active_turn_id)
}

fn agent_run_current_frame_to_contract(
    model: AgentRunCurrentFrameModel,
) -> AgentRunCurrentFrameView {
    let effective_executor_config = model.model_config.effective_executor_config.map(|model| {
        ConversationEffectiveExecutorConfigView {
            executor: model.executor,
            provider_id: model.provider_id,
            model_id: model.model_id,
            agent_id: model.agent_id,
            thinking_level: model.thinking_level,
            permission_policy: model.permission_policy,
            source: match model.source {
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
    });
    AgentRunCurrentFrameView {
        frame_ref: AgentFrameRefDto {
            agent_id: model.agent_id,
            frame_id: model.frame_id,
            revision: Some(model.revision),
        },
        capability_surface: model.capability_surface,
        context_slice: model.context_slice,
        vfs_surface: model.vfs_surface,
        mcp_surface: model.mcp_surface,
        execution_profile: model.execution_profile,
        model_config: ConversationModelConfigView {
            status: match model.model_config.status {
                ConversationModelConfigStatusModel::Resolved => {
                    ConversationModelConfigStatus::Resolved
                }
                ConversationModelConfigStatusModel::ModelRequired => {
                    ConversationModelConfigStatus::ModelRequired
                }
            },
            effective_executor_config,
            missing_fields: model.model_config.missing_fields,
            message: model.model_config.message,
        },
    }
}

/// AgentRun 列表分页查询参数。
#[derive(serde::Deserialize)]
pub struct AgentRunListQuery {
    pub limit: Option<u32>,
    pub cursor: Option<String>,
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
    if context.run.created_by_user_id != current_user.user_id {
        return Err(ApiError::Forbidden(
            "只有 AgentRun 所有者可以提交输入".to_string(),
        ));
    }
    let target = agent_run_runtime_target(&context);
    let presentation_input = AgentRunPresentationInput {
        content: req.input.clone(),
        source: agentdash_agent_protocol::UserInputSource::core_composer(),
        submission_kind: agentdash_agent_protocol::UserInputSubmissionKind::Prompt,
        started_at_seconds: chrono::Utc::now().timestamp(),
    };
    let runtime_input = runtime_input_from_codex(req.input)?;
    let view = state
        .services
        .agent_run_runtime
        .inspect(target.clone())
        .await
        .map_err(agent_run_runtime_error)?;
    let response = if req.delivery_intent == Some(AgentRunComposerDeliveryIntent::Steer) {
        let guard = agent_run_command_guard(&view)?;
        let receipt = state
            .services
            .agent_run_runtime
            .steer_active_turn(SteerAgentRunTurn {
                command: GuardedAgentRunCommand {
                    target,
                    client_command_id: req.client_command_id.clone(),
                    guard,
                    actor: runtime_actor(&current_user),
                },
                input: runtime_input,
            })
            .await
            .map_err(agent_run_runtime_error)?;
        agent_run_message_command_response(
            req.client_command_id,
            AgentRunMessageCommandOutcome::Steered,
            Some(receipt),
            None,
        )
    } else {
        let outcome = runtime_agent_run_mailbox(state.as_ref())
            .submit(EnqueueRuntimeMailboxMessage {
                target: target.clone(),
                presentation_thread_id: context.presentation_thread_id.clone().ok_or_else(
                    || {
                        ApiError::Conflict(format!(
                            "AgentRun {} / {} 缺少 delivery runtime",
                            context.run.id, context.agent.id
                        ))
                    },
                )?,
                presentation_input,
                client_command_id: req.client_command_id.clone(),
                input: runtime_input,
                actor: runtime_actor(&current_user),
                identity: Some(current_user.clone()),
                source: agentdash_domain::agent_run_mailbox::MailboxSourceIdentity::composer(),
                backend_selection: None,
            })
            .await
            .map_err(runtime_mailbox_error)?;
        match outcome {
            RuntimeMailboxSubmitOutcome::Queued { message } => {
                spawn_runtime_mailbox_watcher(state.clone(), target);
                agent_run_message_command_response(
                    req.client_command_id,
                    AgentRunMessageCommandOutcome::Queued,
                    None,
                    Some(message.id.to_string()),
                )
            }
            RuntimeMailboxSubmitOutcome::Dispatched { receipt, .. } => {
                agent_run_message_command_response(
                    req.client_command_id,
                    AgentRunMessageCommandOutcome::Dispatched,
                    Some(receipt),
                    None,
                )
            }
        }
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
    mailbox_message_id: Option<String>,
) -> AgentRunMessageCommandResponse {
    let accepted_runtime_operation_id = receipt
        .as_ref()
        .map(|receipt| receipt.operation_id.to_string());
    AgentRunMessageCommandResponse {
        command_receipt: AgentRunCommandReceipt {
            client_command_id,
            status: match outcome {
                AgentRunMessageCommandOutcome::Queued => "queued",
                AgentRunMessageCommandOutcome::Dispatched
                | AgentRunMessageCommandOutcome::Steered => "accepted",
            }
            .to_string(),
            duplicate: receipt.as_ref().is_some_and(|receipt| receipt.duplicate),
            accepted_runtime_operation_id,
            message: None,
        },
        outcome,
        mailbox_message_id,
    }
}

async fn cancel_agent_run(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path((run_id, agent_id)): Path<(String, String)>,
    Json(body): Json<AgentRunRuntimeCommandRequest>,
) -> Result<Json<OperationReceipt>, ApiError> {
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
    let command = guarded_agent_run_command(
        state.as_ref(),
        &context,
        &current_user,
        body.client_command_id,
    )
    .await?;
    let receipt = state
        .services
        .agent_run_runtime
        .interrupt_active_turn(command)
        .await
        .map_err(agent_run_runtime_error)?;
    Ok(Json(receipt))
}

async fn compact_agent_run_context(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path((run_id, agent_id)): Path<(String, String)>,
    Json(body): Json<AgentRunRuntimeCommandRequest>,
) -> Result<Json<OperationReceipt>, ApiError> {
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
    let command = guarded_agent_run_command(
        state.as_ref(),
        &context,
        &current_user,
        body.client_command_id,
    )
    .await?;
    let receipt = state
        .services
        .agent_run_runtime
        .compact_context(command)
        .await
        .map_err(agent_run_runtime_error)?;
    Ok(Json(receipt))
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

fn agent_run_runtime_error(error: AgentRunRuntimeError) -> ApiError {
    use agentdash_agent_runtime_contract::{
        RuntimeExecuteError as Execute, RuntimeSnapshotError as Snapshot,
        RuntimeSubscribeError as Events,
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
        AgentRunRuntimeError::StaleThread
        | AgentRunRuntimeError::StaleActiveTurn
        | AgentRunRuntimeError::ClientCommandConflict => ApiError::Conflict(error.to_string()),
        AgentRunRuntimeError::UnexpectedSnapshot => {
            ApiError::Internal("Agent Runtime 返回了非预期 snapshot 类型".to_string())
        }
        AgentRunRuntimeError::EmptyClientCommandId => {
            ApiError::BadRequest("client_command_id 不能为空".to_string())
        }
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
