use agentdash_diagnostics::{Subsystem, diag};
use std::convert::Infallible;
use std::sync::Arc;

use crate::routes::runtime_traces;
use agentdash_agent_protocol::codex_app_server_protocol as codex;
use agentdash_agent_runtime_contract::{
    EventSequence, InteractionResponse, OperationReceipt, RuntimeActor, RuntimeContextView,
    RuntimeEventEnvelope, RuntimeInput, RuntimeInteractionId, RuntimeSnapshot,
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
    AgentRunCommandGuard, AgentRunRuntimeError, AgentRunRuntimeView,
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
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::dto::{ContextAuditQuery, SpawnTerminalBody};
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
    runtime_thread_id: Option<String>,
}

struct AgentRunDeliveryRuntimeContext {
    runtime_session_id: String,
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
            has_runtime_binding: context.runtime_thread_id.is_some(),
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
}

#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum AgentRunRuntimeEventStreamItem {
    Event {
        durable_cursor: Option<EventSequence>,
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
        })
        .await
        .map_err(agent_run_runtime_error)?;
    let stream = async_stream::stream! {
        while let Some(next) = events.next().await {
            let terminal = next.is_err();
            let item = match next {
                Ok(envelope) => AgentRunRuntimeEventStreamItem::Event {
                    durable_cursor: envelope.sequence,
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

fn delivery_runtime_session_from_agent_run_context(
    context: &AgentRunContext,
) -> Result<String, ApiError> {
    context.runtime_thread_id.clone().ok_or_else(|| {
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
        runtime_thread_id: delivery_runtime
            .as_ref()
            .map(|delivery| delivery.runtime_session_id.clone()),
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
        runtime_session_id: binding.thread_id.to_string(),
    }))
}

fn parse_uuid(raw: &str, field: &str) -> Result<Uuid, ApiError> {
    Uuid::parse_str(raw).map_err(|_| ApiError::BadRequest(format!("无效的 {field}: {raw}")))
}
