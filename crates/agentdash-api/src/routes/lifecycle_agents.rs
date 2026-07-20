use std::sync::Arc;

use agentdash_agent_runtime_contract::RuntimeChangeSequence;
use agentdash_agent_service_api::AgentServiceErrorCode;
use agentdash_application::agent_run_list::{
    AgentRunListChildModel, AgentRunListRuntimeSummaryModel, ProjectAgentRunListInput,
    ProjectAgentRunListQuery, ProjectAgentRunListQueryDeps,
};
use agentdash_application_agentrun::agent_run::{
    AgentRunProductCommand, AgentRunProductCommandError, AgentRunProductCommandRequest,
    AgentRunProductDeleteError, AgentRunProductDeleteRequest, AgentRunProductDeleteService,
    AgentRunProductForkError, AgentRunProductForkMessageRef, AgentRunProductForkRequest,
    AgentRunProductForkResult, AgentRunProductForkService, AgentRunProductInputDeliveryError,
    AgentRunProductProjectionError, AgentRunTerminalChangeSequence, DeliverAgentRunProductInput,
};
use agentdash_contracts::agent_run_mailbox::{
    AgentRunCommandOnlyRequest, AgentRunCommandReceipt, AgentRunComposerSubmitRequest,
    AgentRunForkLineageView, AgentRunForkOutcomeView, AgentRunForkResponse,
    AgentRunForkSubmitRequest, AgentRunMessageAcceptedRefs, AgentRunMessageCommandOutcome,
    AgentRunMessageCommandResponse,
};
use agentdash_contracts::agent_run_product_projection as product_projection_contract;
use agentdash_contracts::session::SessionMessageRefDto;
use agentdash_contracts::workflow::{AgentFrameRefDto, AgentRunRefDto, LifecycleRunRefDto};
use agentdash_domain::agent_run_target::AgentRunTarget;
use agentdash_workspace_module::workspace_module::presentation_protocol::{
    WorkspaceModulePresentationAcknowledgeRequest, WorkspaceModulePresentationChangeSequence,
    WorkspaceModulePresentationIntentId,
};
use axum::{
    Json,
    body::{Body, Bytes},
    extract::{Path, Query, State},
    http::header,
    response::IntoResponse,
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
            "/agent-runs/{run_id}/agents/{agent_id}/runtime/live",
            axum::routing::get(get_agent_run_live_events),
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
            "/agent-runs/{run_id}/agents/{agent_id}/fork",
            axum::routing::post(fork_agent_run),
        )
        .route(
            "/agent-runs/{run_id}/agents/{agent_id}/fork-submit",
            axum::routing::post(fork_submit_agent_run),
        )
        .route(
            "/agent-runs/{run_id}/agents/{agent_id}/cancel",
            axum::routing::post(cancel_agent_run),
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

async fn fork_agent_run(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path((run_id, agent_id)): Path<(String, String)>,
    Json(body): Json<agentdash_contracts::agent_run_mailbox::AgentRunForkRequest>,
) -> Result<Json<AgentRunForkResponse>, ApiError> {
    let target = authorize_agent_run_target(
        state.as_ref(),
        &current_user,
        &run_id,
        &agent_id,
        ProjectPermission::Use,
    )
    .await?;
    let result = execute_product_fork(
        state.as_ref(),
        target,
        current_user.user_id,
        body.client_command_id.clone(),
        body.title,
        body.fork_point_ref,
        body.metadata_json,
    )
    .await?;
    Ok(Json(fork_response(&result, body.client_command_id)))
}

async fn fork_submit_agent_run(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path((run_id, agent_id)): Path<(String, String)>,
    Json(body): Json<AgentRunForkSubmitRequest>,
) -> Result<Json<AgentRunMessageCommandResponse>, ApiError> {
    validate_fork_submit_preconditions(&body)?;
    if body.executor_config.is_some() || body.backend_selection.is_some() {
        return Err(ApiError::BadRequest(
            "fork-submit 的 child 继承已提交 Product frame；当前请求不能覆盖 executor 或 backend"
                .to_owned(),
        ));
    }
    let parent = authorize_agent_run_target(
        state.as_ref(),
        &current_user,
        &run_id,
        &agent_id,
        ProjectPermission::Use,
    )
    .await?;
    let fork = execute_product_fork(
        state.as_ref(),
        parent,
        current_user.user_id,
        format!("{}:fork", body.client_command_id),
        body.title,
        body.fork_point_ref,
        body.metadata_json,
    )
    .await?;
    let child = AgentRunTarget {
        run_id: fork.saga.child().run_id,
        agent_id: fork.saga.child().agent_id,
    };
    let delivery = state
        .services
        .agent_run_product_input_delivery
        .deliver(DeliverAgentRunProductInput {
            target: child.clone(),
            content: body.input,
            source: agentdash_domain::agent_run_mailbox::MailboxSourceIdentity::composer(),
            origin: agentdash_domain::agent_run_mailbox::MailboxMessageOrigin::User,
            client_command_id: body.client_command_id.clone(),
        })
        .await
        .map_err(product_input_delivery_error)?;
    let duplicate = fork.replayed
        || delivery
            .operation_receipt
            .as_ref()
            .is_some_and(|receipt| receipt.duplicate);
    Ok(Json(AgentRunMessageCommandResponse {
        command_receipt: AgentRunCommandReceipt {
            client_command_id: body.client_command_id,
            status: if delivery.queued {
                "queued".to_owned()
            } else {
                "accepted".to_owned()
            },
            duplicate,
            message: None,
        },
        outcome: if delivery.queued {
            AgentRunMessageCommandOutcome::Queued
        } else {
            AgentRunMessageCommandOutcome::Launched
        },
        mailbox_message: None,
        accepted_refs: Some(agent_run_child_message_refs(&fork)),
        fork: Some(fork_outcome_view(&fork)),
    }))
}

fn validate_fork_submit_preconditions(body: &AgentRunForkSubmitRequest) -> Result<(), ApiError> {
    let client_command_id = body.client_command_id.trim();
    if client_command_id.is_empty() || client_command_id.len() > 256 {
        return Err(ApiError::BadRequest(
            "fork-submit client_command_id 必须为 1..=256 字节".to_owned(),
        ));
    }
    let has_content = body.input.iter().any(|content| {
        let value = match content {
            agentdash_agent_service_api::AgentInputContent::Text { text } => text,
            agentdash_agent_service_api::AgentInputContent::Image { source, .. } => source,
            agentdash_agent_service_api::AgentInputContent::Resource { uri, .. } => uri,
            agentdash_agent_service_api::AgentInputContent::Structured { schema, .. } => schema,
        };
        !value.trim().is_empty()
    });
    if !has_content {
        return Err(ApiError::BadRequest(
            "fork-submit input 必须包含可投递内容".to_owned(),
        ));
    }
    Ok(())
}

async fn execute_product_fork(
    state: &AppState,
    target: AgentRunTarget,
    requested_by_user_id: String,
    client_command_id: String,
    title: Option<String>,
    fork_point_ref: Option<SessionMessageRefDto>,
    metadata_json: Option<serde_json::Value>,
) -> Result<AgentRunProductForkResult, ApiError> {
    AgentRunProductForkService::new(
        state.services.agent_run_product_projection.clone(),
        state.services.agent_run_product_protocol.clone(),
    )
    .fork(AgentRunProductForkRequest {
        target,
        client_command_id,
        requested_by_user_id,
        title,
        fork_point_ref: fork_point_ref.map(|point| AgentRunProductForkMessageRef {
            turn_id: point.turn_id,
            entry_index: point.entry_index,
        }),
        metadata_json,
    })
    .await
    .map_err(product_fork_error)
}

fn fork_response(
    result: &AgentRunProductForkResult,
    client_command_id: String,
) -> AgentRunForkResponse {
    let outcome = fork_outcome_view(result);
    AgentRunForkResponse {
        command_receipt: AgentRunCommandReceipt {
            client_command_id,
            status: "completed".to_owned(),
            duplicate: result.replayed,
            message: None,
        },
        outcome: outcome.outcome,
        parent_refs: outcome.parent_refs,
        child_refs: outcome.child_refs,
        lineage: outcome.lineage,
        redirect: outcome.redirect,
    }
}

fn fork_outcome_view(result: &AgentRunProductForkResult) -> AgentRunForkOutcomeView {
    let saga = &result.saga;
    let intent = saga
        .product_intent()
        .expect("successful Product fork must retain immutable Product intent");
    let parent = AgentRunTarget {
        run_id: saga.parent().run_id,
        agent_id: saga.parent().agent_id,
    };
    let child = AgentRunTarget {
        run_id: saga.child().run_id,
        agent_id: saga.child().agent_id,
    };
    let parent_refs = agent_run_message_refs(&parent);
    let child_refs = agent_run_child_message_refs(result);
    AgentRunForkOutcomeView {
        outcome: "forked".to_owned(),
        parent_refs: parent_refs.clone(),
        child_refs: child_refs.clone(),
        lineage: AgentRunForkLineageView {
            id: saga.request_id().0.to_string(),
            parent: parent_refs,
            child: child_refs,
            relation_kind: "fork".to_owned(),
            fork_point_event_seq: None,
            fork_point_ref: intent
                .source_entry_index
                .map(|entry_index| SessionMessageRefDto {
                    turn_id: intent.source_turn_id.clone(),
                    entry_index,
                }),
            forked_by_user_id: intent.requested_by_user_id.clone(),
            created_at: intent.requested_at.to_rfc3339(),
        },
        redirect: AgentRunRefDto {
            run_id: child.run_id.to_string(),
            agent_id: child.agent_id.to_string(),
        },
    }
}

fn agent_run_child_message_refs(result: &AgentRunProductForkResult) -> AgentRunMessageAcceptedRefs {
    let child = result.saga.child();
    AgentRunMessageAcceptedRefs {
        run_ref: LifecycleRunRefDto {
            run_id: child.run_id.to_string(),
        },
        agent_ref: AgentRunRefDto {
            run_id: child.run_id.to_string(),
            agent_id: child.agent_id.to_string(),
        },
        frame_ref: Some(AgentFrameRefDto {
            agent_id: child.agent_id.to_string(),
            frame_id: child.frame_id.to_string(),
            revision: None,
        }),
        agent_run_turn_id: None,
        protocol_turn_id: None,
    }
}

async fn cancel_agent_run(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path((run_id, agent_id)): Path<(String, String)>,
    Json(body): Json<AgentRunCommandOnlyRequest>,
) -> Result<Json<AgentRunCommandReceipt>, ApiError> {
    let target = authorize_agent_run_target(
        state.as_ref(),
        &current_user,
        &run_id,
        &agent_id,
        ProjectPermission::Use,
    )
    .await?;
    let receipt = state
        .services
        .agent_run_product_projection_composition
        .commands
        .execute(AgentRunProductCommandRequest {
            target,
            client_command_id: body.client_command_id.clone(),
            command: AgentRunProductCommand::Interrupt,
        })
        .await
        .map_err(agent_run_product_command_error)?;
    Ok(Json(AgentRunCommandReceipt {
        client_command_id: body.client_command_id,
        status: "accepted".to_owned(),
        duplicate: receipt.duplicate,
        message: None,
    }))
}

async fn submit_agent_run_composer_input(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path((run_id, agent_id)): Path<(String, String)>,
    Json(body): Json<AgentRunComposerSubmitRequest>,
) -> Result<Json<AgentRunMessageCommandResponse>, ApiError> {
    let target = authorize_agent_run_target(
        state.as_ref(),
        &current_user,
        &run_id,
        &agent_id,
        ProjectPermission::Use,
    )
    .await?;
    let delivery = state
        .services
        .agent_run_product_input_delivery
        .deliver(DeliverAgentRunProductInput {
            target: target.clone(),
            content: body.input,
            source: agentdash_domain::agent_run_mailbox::MailboxSourceIdentity::composer(),
            origin: agentdash_domain::agent_run_mailbox::MailboxMessageOrigin::User,
            client_command_id: body.client_command_id.clone(),
        })
        .await
        .map_err(product_input_delivery_error)?;
    let duplicate = delivery
        .operation_receipt
        .as_ref()
        .is_some_and(|receipt| receipt.duplicate);
    Ok(Json(AgentRunMessageCommandResponse {
        command_receipt: AgentRunCommandReceipt {
            client_command_id: body.client_command_id,
            status: if delivery.queued {
                "queued".to_owned()
            } else {
                "accepted".to_owned()
            },
            duplicate,
            message: None,
        },
        outcome: AgentRunMessageCommandOutcome::Launched,
        mailbox_message: None,
        accepted_refs: Some(agent_run_message_refs(&target)),
        fork: None,
    }))
}

fn agent_run_message_refs(target: &AgentRunTarget) -> AgentRunMessageAcceptedRefs {
    AgentRunMessageAcceptedRefs {
        run_ref: LifecycleRunRefDto {
            run_id: target.run_id.to_string(),
        },
        agent_ref: AgentRunRefDto {
            run_id: target.run_id.to_string(),
            agent_id: target.agent_id.to_string(),
        },
        frame_ref: None,
        agent_run_turn_id: None,
        protocol_turn_id: None,
    }
}

fn product_input_delivery_error(error: AgentRunProductInputDeliveryError) -> ApiError {
    match error {
        AgentRunProductInputDeliveryError::EmptyInput
        | AgentRunProductInputDeliveryError::InvalidClientCommandId => {
            ApiError::BadRequest(error.to_string())
        }
        AgentRunProductInputDeliveryError::Command(_) => {
            ApiError::ServiceUnavailable(error.to_string())
        }
    }
}

fn product_fork_error(error: AgentRunProductForkError) -> ApiError {
    match error {
        AgentRunProductForkError::InvalidRequest => ApiError::BadRequest(error.to_string()),
        AgentRunProductForkError::TargetNotBound
        | AgentRunProductForkError::ForkUnavailable
        | AgentRunProductForkError::CompletedTurnMissing
        | AgentRunProductForkError::ForkPointNotFound
        | AgentRunProductForkError::RequestConflict
        | AgentRunProductForkError::Failed(_) => ApiError::Conflict(error.to_string()),
        AgentRunProductForkError::RecoveryPending { .. } | AgentRunProductForkError::Lost(_) => {
            ApiError::ServiceUnavailable(error.to_string())
        }
        AgentRunProductForkError::Projection(_)
        | AgentRunProductForkError::Persistence(_)
        | AgentRunProductForkError::Protocol(_) => ApiError::Internal(error.to_string()),
    }
}

#[derive(Debug, Deserialize)]
struct AgentRunListQuery {
    limit: Option<usize>,
    cursor: Option<String>,
}

async fn get_project_agent_runs(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(project_id): Path<String>,
    Query(query): Query<AgentRunListQuery>,
) -> Result<Json<agentdash_contracts::workflow::ProjectAgentRunListView>, ApiError> {
    let project_id = parse_uuid(&project_id, "project_id")?;
    load_project_with_permission(
        state.as_ref(),
        &current_user,
        project_id,
        ProjectPermission::Use,
    )
    .await?;
    let page = ProjectAgentRunListQuery::new(ProjectAgentRunListQueryDeps {
        run_repo: state.repos.lifecycle_run_repo.clone(),
        agent_repo: state.repos.lifecycle_agent_repo.clone(),
        lineage_repo: state.repos.agent_lineage_repo.clone(),
        subject_repo: state.repos.lifecycle_subject_association_repo.clone(),
        project_agent_repo: state.repos.project_agent_repo.clone(),
        product_projection: state.services.agent_run_product_projection.clone(),
    })
    .list(ProjectAgentRunListInput {
        project_id,
        limit: query.limit,
        cursor: query.cursor.as_deref(),
    })
    .await
    .map_err(|error| ApiError::Internal(error.to_string()))?;
    Ok(Json(
        agentdash_contracts::workflow::ProjectAgentRunListView {
            project_id: page.project_id.to_string(),
            agent_runs: page
                .entries
                .into_iter()
                .map(
                    |entry| agentdash_contracts::workflow::AgentRunListEntryView {
                        run_ref: agentdash_contracts::workflow::LifecycleRunRefDto {
                            run_id: entry.run_id.to_string(),
                        },
                        agent_ref: agentdash_contracts::workflow::AgentRunRefDto {
                            run_id: entry.run_id.to_string(),
                            agent_id: entry.agent_id.to_string(),
                        },
                        title: entry.title,
                        lifecycle_status: entry.lifecycle_status,
                        last_activity_at: entry.last_activity_at,
                        project_agent_label: entry.project_agent_label,
                        source: entry.source,
                        runtime: entry.runtime.map(runtime_summary_view),
                        subagent_count: entry.subagent_count,
                        children: entry
                            .children
                            .into_iter()
                            .map(agent_run_child_view)
                            .collect(),
                        subject_ref: entry.subject.as_ref().map(|subject| {
                            agentdash_contracts::workflow::SubjectRefDto {
                                kind: subject.kind.clone(),
                                id: subject.id.to_string(),
                            }
                        }),
                        subject_label: entry.subject.and_then(|subject| subject.label),
                    },
                )
                .collect(),
            next_cursor: page.next_cursor,
        },
    ))
}

fn agent_run_child_view(
    child: AgentRunListChildModel,
) -> agentdash_contracts::workflow::AgentRunListChildView {
    agentdash_contracts::workflow::AgentRunListChildView {
        run_ref: agentdash_contracts::workflow::LifecycleRunRefDto {
            run_id: child.run_id.to_string(),
        },
        agent_ref: agentdash_contracts::workflow::AgentRunRefDto {
            run_id: child.run_id.to_string(),
            agent_id: child.agent_id.to_string(),
        },
        title: child.title,
        lifecycle_status: child.lifecycle_status,
        last_activity_at: child.last_activity_at,
        project_agent_label: child.project_agent_label,
        source: child.source,
        runtime: child.runtime.map(runtime_summary_view),
        children: child
            .children
            .into_iter()
            .map(agent_run_child_view)
            .collect(),
    }
}

fn runtime_summary_view(
    runtime: AgentRunListRuntimeSummaryModel,
) -> agentdash_contracts::workflow::AgentRunListRuntimeSummaryView {
    use agentdash_agent_runtime_contract::ManagedRuntimeLifecycleStatus as Source;
    use agentdash_contracts::workflow::AgentRunListRuntimeThreadStatus as Target;
    let thread_status = match runtime.thread_status {
        Source::Active => Target::Active,
        Source::Suspended => Target::Suspended,
        Source::Provisioning => Target::Desynchronized,
        Source::Closed => Target::Closed,
        Source::Lost => Target::Lost,
    };
    agentdash_contracts::workflow::AgentRunListRuntimeSummaryView {
        thread_status,
        active_turn_id: runtime.active_turn_id,
        thread_name: runtime.thread_name,
    }
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
    let outcome = AgentRunProductDeleteService::new(state.repos.lifecycle_run_repo.clone())
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
        AgentRunProductDeleteError::Repository(_) => ApiError::Internal(error.to_string()),
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

async fn get_agent_run_live_events(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path((run_id, agent_id)): Path<(String, String)>,
) -> Result<impl IntoResponse, ApiError> {
    let target = authorize_agent_run_target(
        state.as_ref(),
        &current_user,
        &run_id,
        &agent_id,
        ProjectPermission::Use,
    )
    .await?;
    let mut live = state
        .services
        .agent_run_product_projection
        .runtime_live_events(&target)
        .await
        .map_err(agent_run_product_projection_error)?;
    let stream = async_stream::stream! {
        loop {
            match live.next().await {
                Ok(Some(event)) => match serde_json::to_vec(&event) {
                    Ok(mut raw) => {
                        raw.push(b'\n');
                        yield Ok::<Bytes, std::convert::Infallible>(Bytes::from(raw));
                    }
                    Err(_) => break,
                },
                Ok(None) | Err(_) => break,
            }
        }
    };
    Ok((
        [
            (header::CONTENT_TYPE, "application/x-ndjson; charset=utf-8"),
            (header::CACHE_CONTROL, "no-cache, no-transform"),
            (header::CONNECTION, "keep-alive"),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
        ],
        Body::from_stream(stream),
    ))
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
        product_projection_contract::AgentRunProductRuntimeCommand::Rebind => {
            AgentRunProductCommand::Rebind
        }
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
            command,
        })
        .await
        .map(Json)
        .map_err(agent_run_product_command_error)
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
        AgentRunProductProjectionError::TargetMismatch => ApiError::Internal(error.to_string()),
    }
}

fn agent_run_product_command_error(error: AgentRunProductCommandError) -> ApiError {
    match error {
        AgentRunProductCommandError::TargetNotBound
        | AgentRunProductCommandError::TargetMismatch
        | AgentRunProductCommandError::ActiveTurnMissing => ApiError::Conflict(error.to_string()),
        AgentRunProductCommandError::InvalidClientCommandId
        | AgentRunProductCommandError::InvalidCommand(_) => ApiError::BadRequest(error.to_string()),
        AgentRunProductCommandError::Unavailable(_)
        | AgentRunProductCommandError::InspectionPending => {
            ApiError::ServiceUnavailable(error.to_string())
        }
        AgentRunProductCommandError::Agent(ref source) => match source.code {
            AgentServiceErrorCode::InvalidArgument | AgentServiceErrorCode::Unsupported => {
                ApiError::BadRequest(error.to_string())
            }
            AgentServiceErrorCode::Conflict | AgentServiceErrorCode::StaleBindingGeneration => {
                ApiError::Conflict(error.to_string())
            }
            AgentServiceErrorCode::Unavailable | AgentServiceErrorCode::DeadlineExceeded => {
                ApiError::ServiceUnavailable(error.to_string())
            }
            AgentServiceErrorCode::NotFound
            | AgentServiceErrorCode::ProtocolViolation
            | AgentServiceErrorCode::Internal => ApiError::Internal(error.to_string()),
        },
        AgentRunProductCommandError::Binding(_) => ApiError::Internal(error.to_string()),
    }
}

fn parse_uuid(raw: &str, field: &str) -> Result<Uuid, ApiError> {
    Uuid::parse_str(raw).map_err(|_| ApiError::BadRequest(format!("无效的 {field}: {raw}")))
}

#[cfg(test)]
mod tests {
    use agentdash_agent_service_api::AgentInputContent;

    use super::*;

    fn fork_submit(
        client_command_id: &str,
        input: Vec<AgentInputContent>,
    ) -> AgentRunForkSubmitRequest {
        AgentRunForkSubmitRequest {
            input,
            client_command_id: client_command_id.to_owned(),
            executor_config: None,
            title: None,
            fork_point_ref: None,
            metadata_json: None,
            backend_selection: None,
        }
    }

    #[test]
    fn fork_submit_rejects_invalid_input_before_product_fork_can_start() {
        assert!(validate_fork_submit_preconditions(&fork_submit("", Vec::new())).is_err());
        assert!(
            validate_fork_submit_preconditions(&fork_submit(
                "fork-submit-1",
                vec![AgentInputContent::Text {
                    text: "   ".to_owned(),
                }],
            ))
            .is_err()
        );
        assert!(
            validate_fork_submit_preconditions(&fork_submit(
                "fork-submit-1",
                vec![AgentInputContent::Text {
                    text: "continue".to_owned(),
                }],
            ))
            .is_ok()
        );
    }
}
