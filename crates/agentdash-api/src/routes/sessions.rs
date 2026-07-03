#![allow(clippy::items_after_test_module)]

use agentdash_diagnostics::{DiagnosticErrorContext, Subsystem, diag, diag_error};
use std::convert::Infallible;
use std::io;
use std::sync::Arc;
use std::time::Duration;

use axum::{
    Json,
    body::{Body, Bytes},
    extract::{Path, Query, State},
    http::HeaderMap,
    response::IntoResponse,
};
use tokio::time::MissedTickBehavior;
use uuid::Uuid;

use crate::routes::lifecycle_views::{
    agent_frame_runtime_to_view, presentation_read_model_error_to_api,
    session_runtime_control_status_to_contract,
};
use crate::{app_state::AppState, rpc::ApiError};
use agentdash_agent::MessageRef;
use agentdash_application_agentrun::agent_run as agentrun_read;
use agentdash_application_runtime_session::session::{
    SessionContextProjectionReadModel, SessionExecutionState, SessionForkRequest, SessionMeta,
    SessionProjectionRollbackRequest as ApplicationProjectionRollbackRequest, TitleSource,
};
use agentdash_contracts::session::{
    ApproveToolCallResponse, CreateSessionForkRequest, DeleteSessionResponse,
    RejectToolCallResponse, RollbackSessionProjectionRequest,
    SessionAttachmentContextContributionResponse, SessionContextUsageAnalysisResponse,
    SessionContextUsageCategoryResponse, SessionContextUsageItemResponse, SessionEventResponse,
    SessionEventsPageResponse, SessionForkChildSessionResponse, SessionForkResponse,
    SessionLineageViewResponse, SessionMessageContextBreakdownResponse, SessionMessageRefDto,
    SessionNdjsonEnvelope, SessionProjectionMessageRefResponse, SessionProjectionRollbackResponse,
    SessionProjectionSegmentProvenanceResponse, SessionProjectionSegmentViewResponse,
    SessionProjectionSourceRangeResponse, SessionProjectionViewResponse,
    SessionToolContextContributionResponse,
};
use agentdash_contracts::workflow as workflow_contract;
use agentdash_contracts::workflow::{
    RuntimeSessionExecutionAnchorDto, RuntimeSessionRefDto, SessionRuntimeControlPlaneView,
    SessionRuntimeControlView, SessionShellDto,
};
use agentdash_domain::workflow::{LifecycleRun, RuntimeSessionExecutionAnchor};

use crate::auth::{CurrentUser, ProjectPermission, load_project_with_permission};
use crate::dto::{
    ContextAuditEventDto, ContextAuditQuery, NdjsonStreamQuery, RejectToolApprovalRequest,
    SessionEventsQuery, SessionExecutionStateResponse, UpdateSessionMetaRequest,
};

/// Session trace 权限检查通过 RuntimeSessionExecutionAnchor 进入 LifecycleRun project。
pub async fn ensure_session_permission(
    state: &AppState,
    user: &agentdash_integration_api::AuthIdentity,
    session_id: &str,
    permission: ProjectPermission,
) -> Result<(), ApiError> {
    let _meta = state
        .services
        .session_core
        .get_session_meta(session_id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("会话 {session_id} 不存在")))?;
    let anchor = match state
        .repos
        .execution_anchor_repo
        .find_by_session(session_id)
        .await?
    {
        Some(anchor) => anchor,
        None => {
            return Err(ApiError::BadRequest(format!(
                "runtime session 缺少 RuntimeSessionExecutionAnchor: {session_id}"
            )));
        }
    };
    let run = load_lifecycle_run_for_session(state, anchor.run_id).await?;
    load_project_with_permission(state, user, run.project_id, permission).await?;
    Ok(())
}

const RUNTIME_TRACE_HEARTBEAT_INTERVAL: Duration = Duration::from_secs(20);

pub fn router() -> axum::Router<std::sync::Arc<crate::app_state::AppState>> {
    axum::Router::new()
        .route(
            "/sessions/{id}",
            axum::routing::get(get_session).delete(delete_session),
        )
        .route(
            "/sessions/{id}/runtime-control",
            axum::routing::get(get_session_runtime_control),
        )
        .route(
            "/sessions/{id}/meta",
            axum::routing::get(get_session_meta).patch(update_session_meta),
        )
        .route(
            "/sessions/{id}/state",
            axum::routing::get(get_session_state),
        )
        .route(
            "/sessions/{id}/events",
            axum::routing::get(list_session_events),
        )
        .route(
            "/sessions/{id}/context/projection",
            axum::routing::get(get_session_context_projection),
        )
        .route(
            "/sessions/{id}/lineage",
            axum::routing::get(get_session_lineage),
        )
        .route("/sessions/{id}/fork", axum::routing::post(fork_session))
        .route(
            "/sessions/{id}/projection/rollback",
            axum::routing::post(rollback_session_projection),
        )
        .route(
            "/sessions/{id}/context/audit",
            axum::routing::get(get_session_context_audit),
        )
        .route(
            "/sessions/{id}/tool-approvals/{tool_call_id}/approve",
            axum::routing::post(approve_tool_call),
        )
        .route(
            "/sessions/{id}/tool-approvals/{tool_call_id}/reject",
            axum::routing::post(reject_tool_call),
        )
        .route(
            "/sessions/{id}/stream/ndjson",
            axum::routing::get(session_stream_ndjson),
        )
}

pub async fn get_session(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(session_id): Path<String>,
) -> Result<Json<SessionMeta>, ApiError> {
    ensure_session_permission(
        state.as_ref(),
        &current_user,
        &session_id,
        ProjectPermission::Use,
    )
    .await?;
    let meta = state
        .services
        .session_core
        .get_session_meta(&session_id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("会话 {} 不存在", session_id)))?;
    Ok(Json(meta))
}

pub async fn get_session_runtime_control(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(runtime_session_id): Path<String>,
) -> Result<Json<SessionRuntimeControlView>, ApiError> {
    ensure_session_permission(
        state.as_ref(),
        &current_user,
        &runtime_session_id,
        ProjectPermission::Use,
    )
    .await?;
    let view = state
        .services
        .presentation_read_model_query
        .session_runtime_control(&runtime_session_id)
        .await
        .map_err(presentation_read_model_error_to_api)?;
    if let Some(project_id) = view.project_id {
        load_project_with_permission(
            state.as_ref(),
            &current_user,
            project_id,
            ProjectPermission::Use,
        )
        .await?;
    }
    Ok(Json(SessionRuntimeControlView {
        runtime_session_ref: RuntimeSessionRefDto {
            runtime_session_id: view.runtime_session_id,
        },
        session_meta: session_shell_dto(&view.session_meta),
        control_plane: SessionRuntimeControlPlaneView {
            status: session_runtime_control_status_to_contract(view.control_plane.status),
            reason: view.control_plane.reason,
        },
        anchor: view.anchor.as_ref().map(anchor_dto),
        run: view.run.map(presentation_lifecycle_run_to_contract),
        agent: view.agent.map(presentation_agent_run_to_contract),
        frame_runtime: view.frame_runtime.map(agent_frame_runtime_to_view),
        subject_associations: view
            .subject_associations
            .into_iter()
            .map(presentation_subject_association_to_contract)
            .collect(),
    }))
}

fn presentation_lifecycle_run_to_contract(
    view: agentrun_read::PresentationLifecycleRunView,
) -> workflow_contract::LifecycleRunView {
    workflow_contract::LifecycleRunView {
        run_ref: workflow_contract::LifecycleRunRefDto {
            run_id: view.run_ref.run_id,
        },
        project_id: view.project_id,
        topology: match view.topology {
            agentrun_read::PresentationLifecycleRunTopologyView::Plain => {
                workflow_contract::LifecycleRunTopology::Plain
            }
            agentrun_read::PresentationLifecycleRunTopologyView::WorkflowGraph => {
                workflow_contract::LifecycleRunTopology::WorkflowGraph
            }
        },
        status: presentation_lifecycle_status_to_contract(view.status),
        orchestrations: view
            .orchestrations
            .into_iter()
            .map(presentation_orchestration_to_contract)
            .collect(),
        active_runtime_node_refs: view
            .active_runtime_node_refs
            .into_iter()
            .map(|item| workflow_contract::ActiveRuntimeNodeRefDto {
                run_id: item.run_id,
                orchestration_id: item.orchestration_id,
                node_path: item.node_path,
                attempt: item.attempt,
                status: item.status,
            })
            .collect(),
        agents: view
            .agents
            .into_iter()
            .map(presentation_agent_run_to_contract)
            .collect(),
        subject_associations: view
            .subject_associations
            .into_iter()
            .map(presentation_subject_association_to_contract)
            .collect(),
        runtime_trace_refs: view
            .runtime_trace_refs
            .into_iter()
            .map(presentation_runtime_ref_to_contract)
            .collect(),
        execution_log: view
            .execution_log
            .into_iter()
            .map(presentation_execution_entry_to_contract)
            .collect(),
        created_at: view.created_at,
        updated_at: view.updated_at,
        last_activity_at: view.last_activity_at,
    }
}

fn presentation_lifecycle_status_to_contract(
    status: agentrun_read::PresentationLifecycleRunStatusView,
) -> workflow_contract::LifecycleRunStatus {
    match status {
        agentrun_read::PresentationLifecycleRunStatusView::Draft => {
            workflow_contract::LifecycleRunStatus::Draft
        }
        agentrun_read::PresentationLifecycleRunStatusView::Ready => {
            workflow_contract::LifecycleRunStatus::Ready
        }
        agentrun_read::PresentationLifecycleRunStatusView::Running => {
            workflow_contract::LifecycleRunStatus::Running
        }
        agentrun_read::PresentationLifecycleRunStatusView::Blocked => {
            workflow_contract::LifecycleRunStatus::Blocked
        }
        agentrun_read::PresentationLifecycleRunStatusView::Completed => {
            workflow_contract::LifecycleRunStatus::Completed
        }
        agentrun_read::PresentationLifecycleRunStatusView::Failed => {
            workflow_contract::LifecycleRunStatus::Failed
        }
        agentrun_read::PresentationLifecycleRunStatusView::Cancelled => {
            workflow_contract::LifecycleRunStatus::Cancelled
        }
    }
}

fn presentation_subject_ref_to_contract(
    subject: agentrun_read::PresentationSubjectRefView,
) -> workflow_contract::SubjectRefDto {
    workflow_contract::SubjectRefDto {
        kind: subject.kind,
        id: subject.id,
    }
}

fn presentation_runtime_ref_to_contract(
    runtime_ref: agentrun_read::PresentationRuntimeSessionRefView,
) -> workflow_contract::RuntimeSessionRefDto {
    workflow_contract::RuntimeSessionRefDto {
        runtime_session_id: runtime_ref.runtime_session_id,
    }
}

fn presentation_subject_association_to_contract(
    association: agentrun_read::PresentationLifecycleSubjectAssociationView,
) -> workflow_contract::LifecycleSubjectAssociationDto {
    workflow_contract::LifecycleSubjectAssociationDto {
        id: association.id,
        anchor_run_id: association.anchor_run_id,
        anchor_agent_id: association.anchor_agent_id,
        subject_ref: presentation_subject_ref_to_contract(association.subject_ref),
        role: association.role,
        metadata: association.metadata,
        created_at: association.created_at,
    }
}

fn presentation_agent_run_to_contract(
    agent: agentrun_read::PresentationAgentRunView,
) -> workflow_contract::AgentRunView {
    workflow_contract::AgentRunView {
        agent_ref: workflow_contract::AgentRunRefDto {
            run_id: agent.agent_ref.run_id,
            agent_id: agent.agent_ref.agent_id,
        },
        project_id: agent.project_id,
        source: agent.source,
        project_agent_id: agent.project_agent_id,
        status: agent.status,
        delivery_runtime_ref: agent
            .delivery_runtime_ref
            .map(presentation_runtime_ref_to_contract),
        last_delivery_status: agent.last_delivery_status,
        created_at: agent.created_at,
        updated_at: agent.updated_at,
    }
}

fn presentation_orchestration_to_contract(
    orchestration: agentrun_read::PresentationOrchestrationInstanceView,
) -> workflow_contract::OrchestrationInstanceView {
    workflow_contract::OrchestrationInstanceView {
        orchestration_id: orchestration.orchestration_id,
        role: orchestration.role,
        status: orchestration.status,
        plan_digest: orchestration.plan_digest,
        source_ref: orchestration.source_ref,
        ready_node_ids: orchestration.ready_node_ids,
        nodes: orchestration
            .nodes
            .into_iter()
            .map(presentation_runtime_node_to_contract)
            .collect(),
        created_at: orchestration.created_at,
        updated_at: orchestration.updated_at,
    }
}

fn presentation_runtime_node_to_contract(
    node: agentrun_read::PresentationRuntimeNodeView,
) -> workflow_contract::RuntimeNodeView {
    workflow_contract::RuntimeNodeView {
        node_id: node.node_id,
        node_path: node.node_path,
        kind: node.kind,
        status: node.status,
        attempt: node.attempt,
        executor_run_ref: node.executor_run_ref.map(|run_ref| match run_ref {
            agentrun_read::PresentationExecutorRunRefView::RuntimeSession { session_id } => {
                workflow_contract::ExecutorRunRef::RuntimeSession { session_id }
            }
            agentrun_read::PresentationExecutorRunRefView::FunctionRun { run_id } => {
                workflow_contract::ExecutorRunRef::FunctionRun { run_id }
            }
            agentrun_read::PresentationExecutorRunRefView::HumanDecision { decision_id } => {
                workflow_contract::ExecutorRunRef::HumanDecision { decision_id }
            }
        }),
        started_at: node.started_at,
        completed_at: node.completed_at,
        children: node
            .children
            .into_iter()
            .map(presentation_runtime_node_to_contract)
            .collect(),
    }
}

fn presentation_execution_entry_to_contract(
    entry: agentrun_read::PresentationLifecycleExecutionEntryView,
) -> workflow_contract::LifecycleExecutionEntry {
    workflow_contract::LifecycleExecutionEntry {
        timestamp: entry.timestamp,
        activity_key: entry.activity_key,
        event_kind: match entry.event_kind {
            agentrun_read::PresentationLifecycleExecutionEventKindView::ActivityActivated => {
                workflow_contract::LifecycleExecutionEventKind::ActivityActivated
            }
            agentrun_read::PresentationLifecycleExecutionEventKindView::ActivityCompleted => {
                workflow_contract::LifecycleExecutionEventKind::ActivityCompleted
            }
            agentrun_read::PresentationLifecycleExecutionEventKindView::ConstraintBlocked => {
                workflow_contract::LifecycleExecutionEventKind::ConstraintBlocked
            }
            agentrun_read::PresentationLifecycleExecutionEventKindView::CompletionEvaluated => {
                workflow_contract::LifecycleExecutionEventKind::CompletionEvaluated
            }
            agentrun_read::PresentationLifecycleExecutionEventKindView::ArtifactAppended => {
                workflow_contract::LifecycleExecutionEventKind::ArtifactAppended
            }
            agentrun_read::PresentationLifecycleExecutionEventKindView::ContextInjected => {
                workflow_contract::LifecycleExecutionEventKind::ContextInjected
            }
        },
        summary: entry.summary,
        detail: entry.detail,
    }
}

fn map_session_event(
    event: agentdash_application_runtime_session::session::PersistedSessionEvent,
) -> SessionEventResponse {
    event.into()
}

fn stream_event_payload(
    event: agentdash_application_runtime_session::session::PersistedSessionEvent,
) -> SessionNdjsonEnvelope {
    SessionNdjsonEnvelope::event(event)
}

pub async fn get_session_state(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(session_id): Path<String>,
) -> Result<Json<SessionExecutionStateResponse>, ApiError> {
    ensure_session_permission(
        state.as_ref(),
        &current_user,
        &session_id,
        ProjectPermission::Use,
    )
    .await?;

    let execution_state = state
        .services
        .session_core
        .inspect_session_execution_state(&session_id)
        .await?;

    let response = match execution_state {
        SessionExecutionState::Idle => SessionExecutionStateResponse {
            session_id,
            status: "idle".to_string(),
            turn_id: None,
            message: None,
        },
        SessionExecutionState::Running { turn_id } => SessionExecutionStateResponse {
            session_id,
            status: "running".to_string(),
            turn_id,
            message: None,
        },
        SessionExecutionState::Cancelling { turn_id } => SessionExecutionStateResponse {
            session_id,
            status: "cancelling".to_string(),
            turn_id,
            message: Some("当前执行正在取消中。".to_string()),
        },
        SessionExecutionState::Completed { turn_id } => SessionExecutionStateResponse {
            session_id,
            status: "completed".to_string(),
            turn_id: Some(turn_id),
            message: None,
        },
        SessionExecutionState::Failed { turn_id, message } => SessionExecutionStateResponse {
            session_id,
            status: "failed".to_string(),
            turn_id: Some(turn_id),
            message,
        },
        SessionExecutionState::Interrupted { turn_id, message } => SessionExecutionStateResponse {
            session_id,
            status: "interrupted".to_string(),
            turn_id,
            message,
        },
        SessionExecutionState::Lost { turn_id, message } => SessionExecutionStateResponse {
            session_id,
            status: "lost".to_string(),
            turn_id,
            message,
        },
    };

    Ok(Json(response))
}

async fn load_lifecycle_run_for_session(
    state: &AppState,
    run_id: Uuid,
) -> Result<LifecycleRun, ApiError> {
    state
        .repos
        .lifecycle_run_repo
        .get_by_id(run_id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("lifecycle_run 不存在: {run_id}")))
}

fn session_shell_dto(meta: &SessionMeta) -> SessionShellDto {
    SessionShellDto {
        id: meta.id.clone(),
        title: meta.title.clone(),
        title_source: serialized_string(&meta.title_source),
        created_at: meta.created_at,
        updated_at: meta.updated_at,
        last_event_seq: meta.last_event_seq,
        last_turn_id: meta.last_turn_id.clone(),
        last_delivery_status: serialized_string(&meta.last_delivery_status),
    }
}

fn anchor_dto(anchor: &RuntimeSessionExecutionAnchor) -> RuntimeSessionExecutionAnchorDto {
    RuntimeSessionExecutionAnchorDto {
        runtime_session_id: anchor.runtime_session_id.clone(),
        run_id: anchor.run_id.to_string(),
        agent_id: anchor.agent_id.to_string(),
        launch_frame_id: anchor.launch_frame_id.to_string(),
        orchestration_id: anchor.orchestration_id.map(|id| id.to_string()),
        node_path: anchor.node_path.clone(),
        node_attempt: anchor.node_attempt,
        created_by_kind: anchor.created_by_kind.clone(),
        created_at: anchor.created_at.to_rfc3339(),
        updated_at: anchor.updated_at.to_rfc3339(),
    }
}

fn serialized_string<T: serde::Serialize>(value: &T) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|value| value.as_str().map(ToOwned::to_owned))
        .unwrap_or_else(|| "unknown".to_string())
}

pub async fn list_session_events(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(session_id): Path<String>,
    Query(query): Query<SessionEventsQuery>,
) -> Result<Json<SessionEventsPageResponse>, ApiError> {
    ensure_session_permission(
        state.as_ref(),
        &current_user,
        &session_id,
        ProjectPermission::Use,
    )
    .await?;
    let after_seq = query.after_seq.unwrap_or(0);
    let limit = query.limit.unwrap_or(500).clamp(1, 2_000);
    let page = state
        .services
        .session_eventing
        .list_event_page(&session_id, after_seq, limit)
        .await
        .map_err(ApiError::from)?;

    Ok(Json(SessionEventsPageResponse {
        snapshot_seq: page.snapshot_seq,
        events: page.events.into_iter().map(map_session_event).collect(),
        has_more: page.has_more,
        next_after_seq: page.next_after_seq,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentdash_application_runtime_session::session::{
        ExecutionStatus, PromptLaunchPath, RuntimeTraceLaunchState,
        SessionAttachmentContextContribution, SessionContextUsageCategory, SessionContextUsageItem,
        SessionContextUsageReadModel, SessionMessageContextBreakdown,
        SessionProjectionMessageRefReadModel, SessionProjectionSegmentProvenanceReadModel,
        SessionProjectionSegmentReadModel, SessionProjectionSourceRangeReadModel,
        SessionRepositoryRehydrateMode, SessionToolContextContribution, TitleSource,
        resolve_prompt_launch_path,
    };

    fn test_meta(id: &str, event_seq: u64, executor_session_id: Option<&str>) -> SessionMeta {
        SessionMeta {
            id: id.to_string(),
            title: "测试".to_string(),
            title_source: TitleSource::Auto,
            created_at: 1,
            updated_at: 1,
            last_event_seq: event_seq,
            last_delivery_status: if event_seq > 0 {
                ExecutionStatus::Completed
            } else {
                ExecutionStatus::Idle
            },
            last_turn_id: if event_seq > 0 {
                Some("t-last".to_string())
            } else {
                None
            },
            last_terminal_message: None,
            executor_session_id: executor_session_id.map(String::from),
        }
    }

    fn trace_state(meta: &SessionMeta) -> RuntimeTraceLaunchState {
        RuntimeTraceLaunchState::from(meta)
    }

    #[test]
    fn prompt_launch_path_marks_pending_as_owner_bootstrap() {
        let meta = test_meta("sess-1", 0, None);
        assert_eq!(
            resolve_prompt_launch_path(&trace_state(&meta), false, false, true),
            PromptLaunchPath::OwnerBootstrap
        );
    }

    #[test]
    fn prompt_launch_path_requires_repository_rehydrate_after_cold_restart() {
        let meta = test_meta("sess-2", 12, None);
        assert_eq!(
            resolve_prompt_launch_path(&trace_state(&meta), false, false, false),
            PromptLaunchPath::RepositoryRehydrate(SessionRepositoryRehydrateMode::SystemContext,)
        );
        assert_eq!(
            resolve_prompt_launch_path(&trace_state(&meta), true, false, false),
            PromptLaunchPath::Plain
        );
    }

    #[test]
    fn prompt_launch_path_prefers_executor_follow_up_when_available() {
        let meta = test_meta("sess-3", 5, Some("exec-1"));
        assert_eq!(
            resolve_prompt_launch_path(&trace_state(&meta), false, true, false),
            PromptLaunchPath::Plain
        );
    }

    #[test]
    fn prompt_launch_path_uses_executor_state_restore_when_supported() {
        let meta = test_meta("sess-4", 7, None);
        assert_eq!(
            resolve_prompt_launch_path(&trace_state(&meta), false, true, false),
            PromptLaunchPath::RepositoryRehydrate(SessionRepositoryRehydrateMode::ExecutorState,)
        );
    }

    #[test]
    fn context_projection_mapper_preserves_usage_read_facts() {
        let response = session_context_projection_to_response(SessionContextProjectionReadModel {
            session_id: "sess-1".to_string(),
            projection_kind: "model_context".to_string(),
            projection_version: 2,
            head_event_seq: 42,
            active_compaction_id: Some("compaction-1".to_string()),
            token_estimate: Some(128),
            message_count: 1,
            segments: vec![SessionProjectionSegmentReadModel {
                id: "segment-1".to_string(),
                sort_order: 0,
                segment_type: "summary_chunk".to_string(),
                role: "compaction_summary".to_string(),
                origin: "projection".to_string(),
                synthetic: true,
                projection_kind: "compaction_summary".to_string(),
                message_ref: SessionProjectionMessageRefReadModel {
                    turn_id: "_projection:summary".to_string(),
                    entry_index: 0,
                },
                source_event_seq: None,
                source_range: Some(SessionProjectionSourceRangeReadModel {
                    start_event_seq: 1,
                    end_event_seq: 30,
                }),
                projection_segment_id: Some("segment-1".to_string()),
                preview: "summary".to_string(),
                token_estimate: Some(20),
                attachment_tokens: 0,
                attachment_names: Vec::new(),
                tool_names: vec!["read_file".to_string()],
                provenance: SessionProjectionSegmentProvenanceReadModel {
                    compaction_id: Some("compaction-1".to_string()),
                    projection_version: Some(2),
                    segment_type: Some("summary_chunk".to_string()),
                    strategy: Some("summary_prefix".to_string()),
                    trigger: Some("auto".to_string()),
                    phase: Some("pre_provider".to_string()),
                },
            }],
            context_usage: SessionContextUsageReadModel {
                categories: vec![SessionContextUsageCategory {
                    kind: "system_developer".to_string(),
                    label: "System / Developer".to_string(),
                    token_estimate: 12,
                    source: "context_frame".to_string(),
                    deferred: false,
                }],
                items: vec![SessionContextUsageItem {
                    kind: "system_developer".to_string(),
                    label: "System / Developer".to_string(),
                    name: "Identity".to_string(),
                    token_estimate: 12,
                    source: "context_frame".to_string(),
                    deferred: false,
                    source_event_seq: Some(8),
                    turn_id: Some("turn-1".to_string()),
                }],
                messages: SessionMessageContextBreakdown {
                    user_message_tokens: 1,
                    assistant_message_tokens: 2,
                    tool_call_tokens: 3,
                    tool_result_tokens: 4,
                    attachment_tokens: 5,
                },
                top_tools: vec![SessionToolContextContribution {
                    name: "read_file".to_string(),
                    call_tokens: 3,
                    result_tokens: 0,
                }],
                top_attachments: vec![SessionAttachmentContextContribution {
                    name: "image/png image #0".to_string(),
                    tokens: 5,
                }],
            },
        });

        assert_eq!(response.session_id, "sess-1");
        assert_eq!(
            response.segments[0]
                .source_range
                .as_ref()
                .unwrap()
                .end_event_seq,
            30
        );
        assert_eq!(
            response.segments[0].provenance.compaction_id.as_deref(),
            Some("compaction-1")
        );
        assert_eq!(
            response.context_usage.categories[0].kind,
            "system_developer"
        );
        assert_eq!(response.context_usage.items[0].source_event_seq, Some(8));
        assert_eq!(response.context_usage.messages.attachment_tokens, 5);
        assert_eq!(response.context_usage.top_tools[0].name, "read_file");
        assert_eq!(
            response.context_usage.top_attachments[0].name,
            "image/png image #0"
        );
    }

    #[test]
    fn session_message_ref_mapper_preserves_fork_point_coordinate() {
        let message_ref = session_message_ref_to_application(SessionMessageRefDto {
            turn_id: "turn-1".to_string(),
            entry_index: 7,
        });

        assert_eq!(message_ref.turn_id, "turn-1");
        assert_eq!(message_ref.entry_index, 7);
    }
}

/// Internal diagnostics: GET /sessions/{id}/context/projection — 返回当前模型可见上下文投影。
pub async fn get_session_context_projection(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(session_id): Path<String>,
) -> Result<Json<SessionProjectionViewResponse>, ApiError> {
    ensure_session_permission(
        state.as_ref(),
        &current_user,
        &session_id,
        ProjectPermission::Use,
    )
    .await?;
    let projection = state
        .services
        .session_eventing
        .build_context_projection_read_model(&session_id)
        .await
        .map_err(ApiError::from)?;

    Ok(Json(session_context_projection_to_response(projection)))
}

fn session_context_projection_to_response(
    projection: SessionContextProjectionReadModel,
) -> SessionProjectionViewResponse {
    SessionProjectionViewResponse {
        session_id: projection.session_id,
        projection_kind: projection.projection_kind,
        projection_version: projection.projection_version,
        head_event_seq: projection.head_event_seq,
        active_compaction_id: projection.active_compaction_id,
        token_estimate: projection.token_estimate,
        message_count: projection.message_count,
        segments: projection
            .segments
            .into_iter()
            .map(|segment| SessionProjectionSegmentViewResponse {
                id: segment.id,
                sort_order: segment.sort_order,
                segment_type: segment.segment_type,
                role: segment.role,
                origin: segment.origin,
                synthetic: segment.synthetic,
                projection_kind: segment.projection_kind,
                message_ref: SessionProjectionMessageRefResponse {
                    turn_id: segment.message_ref.turn_id,
                    entry_index: segment.message_ref.entry_index,
                },
                source_event_seq: segment.source_event_seq,
                source_range: segment.source_range.map(|range| {
                    SessionProjectionSourceRangeResponse {
                        start_event_seq: range.start_event_seq,
                        end_event_seq: range.end_event_seq,
                    }
                }),
                projection_segment_id: segment.projection_segment_id,
                preview: segment.preview,
                token_estimate: segment.token_estimate,
                attachment_tokens: segment.attachment_tokens,
                attachment_names: segment.attachment_names,
                tool_names: segment.tool_names,
                provenance: SessionProjectionSegmentProvenanceResponse {
                    compaction_id: segment.provenance.compaction_id,
                    projection_version: segment.provenance.projection_version,
                    segment_type: segment.provenance.segment_type,
                    strategy: segment.provenance.strategy,
                    trigger: segment.provenance.trigger,
                    phase: segment.provenance.phase,
                },
            })
            .collect(),
        context_usage: SessionContextUsageAnalysisResponse {
            categories: projection
                .context_usage
                .categories
                .into_iter()
                .map(|category| SessionContextUsageCategoryResponse {
                    kind: category.kind,
                    label: category.label,
                    token_estimate: category.token_estimate,
                    source: category.source,
                    deferred: category.deferred,
                })
                .collect(),
            items: projection
                .context_usage
                .items
                .into_iter()
                .map(|item| SessionContextUsageItemResponse {
                    kind: item.kind,
                    label: item.label,
                    name: item.name,
                    token_estimate: item.token_estimate,
                    source: item.source,
                    deferred: item.deferred,
                    source_event_seq: item.source_event_seq,
                    turn_id: item.turn_id,
                })
                .collect(),
            messages: SessionMessageContextBreakdownResponse {
                user_message_tokens: projection.context_usage.messages.user_message_tokens,
                assistant_message_tokens: projection
                    .context_usage
                    .messages
                    .assistant_message_tokens,
                tool_call_tokens: projection.context_usage.messages.tool_call_tokens,
                tool_result_tokens: projection.context_usage.messages.tool_result_tokens,
                attachment_tokens: projection.context_usage.messages.attachment_tokens,
            },
            top_tools: projection
                .context_usage
                .top_tools
                .into_iter()
                .map(|tool| SessionToolContextContributionResponse {
                    name: tool.name,
                    call_tokens: tool.call_tokens,
                    result_tokens: tool.result_tokens,
                })
                .collect(),
            top_attachments: projection
                .context_usage
                .top_attachments
                .into_iter()
                .map(|attachment| SessionAttachmentContextContributionResponse {
                    name: attachment.name,
                    tokens: attachment.tokens,
                })
                .collect(),
        },
    }
}

fn session_message_ref_to_application(value: SessionMessageRefDto) -> MessageRef {
    MessageRef {
        turn_id: value.turn_id,
        entry_index: value.entry_index,
    }
}

/// Internal diagnostics: POST /sessions/{id}/fork — 基于当前模型投影创建可恢复 trace child。
pub async fn fork_session(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(session_id): Path<String>,
    Json(req): Json<CreateSessionForkRequest>,
) -> Result<Json<SessionForkResponse>, ApiError> {
    ensure_session_permission(
        state.as_ref(),
        &current_user,
        &session_id,
        ProjectPermission::Use,
    )
    .await?;
    let result = state
        .services
        .session_branching
        .fork_session(SessionForkRequest {
            parent_session_id: session_id.clone(),
            title: req.title,
            fork_point_ref: req.fork_point_ref.map(session_message_ref_to_application),
            fork_point_compaction_id: req.fork_point_compaction_id,
            metadata_json: req.metadata_json.unwrap_or_else(|| serde_json::json!({})),
        })
        .await
        .map_err(api_error_from_io)?;

    Ok(Json(SessionForkResponse {
        parent_session_id: result.parent_session_id,
        child_session: SessionForkChildSessionResponse {
            id: result.child_session.id,
            title: result.child_session.title,
            created_at: result.child_session.created_at,
            updated_at: result.child_session.updated_at,
            last_event_seq: result.child_session.last_event_seq,
        },
        lineage: result.lineage.into(),
        child_initial_compaction_id: result.projection_commit.compaction.id,
        projection_version: result.projection_commit.head.projection_version,
        head_event_seq: result.projection_commit.head.head_event_seq,
    }))
}

/// Internal diagnostics: GET /sessions/{id}/lineage — 返回 runtime trace 的父边、祖先与直接 children。
pub async fn get_session_lineage(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(session_id): Path<String>,
) -> Result<Json<SessionLineageViewResponse>, ApiError> {
    ensure_session_permission(
        state.as_ref(),
        &current_user,
        &session_id,
        ProjectPermission::Use,
    )
    .await?;
    let view = state
        .services
        .session_branching
        .lineage_view(&session_id)
        .await
        .map_err(api_error_from_io)?;

    Ok(Json(SessionLineageViewResponse {
        session_id,
        lineage: view.lineage.map(Into::into),
        ancestors: view.ancestors.into_iter().map(Into::into).collect(),
        children: view.children.into_iter().map(Into::into).collect(),
    }))
}

/// Internal diagnostics: POST /sessions/{id}/projection/rollback — 移动模型可见 projection head，不删除审计事件。
pub async fn rollback_session_projection(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(session_id): Path<String>,
    Json(req): Json<RollbackSessionProjectionRequest>,
) -> Result<Json<SessionProjectionRollbackResponse>, ApiError> {
    ensure_session_permission(
        state.as_ref(),
        &current_user,
        &session_id,
        ProjectPermission::Use,
    )
    .await?;
    let result = state
        .services
        .session_branching
        .rollback_model_projection(ApplicationProjectionRollbackRequest {
            session_id: session_id.clone(),
            target_event_seq: req.target_event_seq,
            active_compaction_id: req.active_compaction_id,
            reason: req.reason,
        })
        .await
        .map_err(api_error_from_io)?;

    Ok(Json(SessionProjectionRollbackResponse {
        session_id,
        event: result.event.into(),
        head_event_seq: result.head.head_event_seq,
        active_compaction_id: result.head.active_compaction_id,
        projection_version: result.head.projection_version,
        updated_by_event_seq: result.head.updated_by_event_seq,
    }))
}

/// Internal diagnostics: GET /sessions/{id}/meta — 返回完整 runtime trace meta。
pub async fn get_session_meta(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(session_id): Path<String>,
) -> Result<Json<SessionMeta>, ApiError> {
    get_session(State(state), CurrentUser(current_user), Path(session_id)).await
}

/// Internal diagnostics: PATCH /sessions/{id}/meta — 修改 runtime trace meta。
pub async fn update_session_meta(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(session_id): Path<String>,
    Json(req): Json<UpdateSessionMetaRequest>,
) -> Result<Json<SessionMeta>, ApiError> {
    if req.title.is_none() {
        return Err(ApiError::BadRequest("必须提供 title".to_string()));
    }
    if let Some(title) = req.title.as_deref()
        && title.trim().is_empty()
    {
        return Err(ApiError::BadRequest("标题不能为空".to_string()));
    }
    ensure_session_permission(
        state.as_ref(),
        &current_user,
        &session_id,
        ProjectPermission::Use,
    )
    .await?;

    let meta = state
        .services
        .session_core
        .update_session_meta(&session_id, |meta| {
            if let Some(title) = req.title.as_deref() {
                let title = title.trim();
                if !title.is_empty() {
                    meta.title = title.to_string();
                    meta.title_source = TitleSource::User;
                }
            }
        })
        .await
        .map_err(ApiError::from)?
        .ok_or_else(|| ApiError::NotFound(format!("会话 {} 不存在", session_id)))?;

    Ok(Json(meta))
}

pub async fn delete_session(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(session_id): Path<String>,
) -> Result<Json<DeleteSessionResponse>, ApiError> {
    ensure_session_permission(
        state.as_ref(),
        &current_user,
        &session_id,
        ProjectPermission::Use,
    )
    .await?;
    state
        .services
        .session_core
        .delete_session(&session_id)
        .await?;
    Ok(Json(DeleteSessionResponse {
        deleted: true,
        session_id,
    }))
}

pub async fn approve_tool_call(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path((session_id, tool_call_id)): Path<(String, String)>,
) -> Result<Json<ApproveToolCallResponse>, ApiError> {
    ensure_session_permission(
        state.as_ref(),
        &current_user,
        &session_id,
        ProjectPermission::Use,
    )
    .await?;
    state
        .services
        .session_control
        .approve_tool_call(&session_id, &tool_call_id)
        .await
        .map_err(ApiError::from)?;

    Ok(Json(ApproveToolCallResponse {
        approved: true,
        session_id,
        tool_call_id,
    }))
}

pub async fn reject_tool_call(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path((session_id, tool_call_id)): Path<(String, String)>,
    Json(req): Json<RejectToolApprovalRequest>,
) -> Result<Json<RejectToolCallResponse>, ApiError> {
    ensure_session_permission(
        state.as_ref(),
        &current_user,
        &session_id,
        ProjectPermission::Use,
    )
    .await?;
    state
        .services
        .session_control
        .reject_tool_call(&session_id, &tool_call_id, req.reason.clone())
        .await
        .map_err(ApiError::from)?;

    Ok(Json(RejectToolCallResponse {
        rejected: true,
        session_id,
        tool_call_id,
    }))
}

/// Session trace stream（Fetch Streaming / NDJSON）
pub async fn session_stream_ndjson(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(session_id): Path<String>,
    headers: HeaderMap,
    Query(query): Query<NdjsonStreamQuery>,
) -> Result<impl IntoResponse, ApiError> {
    ensure_session_permission(
        state.as_ref(),
        &current_user,
        &session_id,
        ProjectPermission::Use,
    )
    .await?;
    let resume_from = parse_resume_from_header(&headers, "x-stream-since-id")?
        .or(query.since_id)
        .unwrap_or(0);
    diag!(Info, Subsystem::Api,

        session_id = %session_id,
        resume_from = resume_from,
        "Session trace stream 连接建立（NDJSON）"
    );

    let subscription = state
        .services
        .session_eventing
        .subscribe_after(&session_id, resume_from)
        .await
        .map_err(ApiError::from)?;
    let replayed = subscription.backlog.len();
    diag!(Info, Subsystem::Api,

        session_id = %session_id,
        replayed_count = replayed,
        snapshot_seq = subscription.snapshot_seq,
        "Session trace stream 历史补发完成（NDJSON）"
    );

    let ephemeral_epoch = state.services.session_eventing.ephemeral_epoch();

    let stream = async_stream::stream! {
        let mut seq = resume_from;
        for event in subscription.backlog {
            seq = event.event_seq;
            if let Some(line) = to_ndjson_line(&stream_event_payload(event)) {
                yield Ok::<Bytes, Infallible>(line);
            }
        }

        if let Some(line) = to_ndjson_line(&SessionNdjsonEnvelope::connected(seq, ephemeral_epoch)) {
            yield Ok::<Bytes, Infallible>(line);
        }

        // durable backlog + connected 之后、live loop 之前补发 ephemeral 快照（in-flight 进度态）。
        // 这些事件 event_seq 承载 ephemeral_seq，不影响 durable `seq` 游标；前端按 ephemeral_seq 去重，
        // 与后续 live ephemeral 广播的重叠由去重消解。
        for event in subscription.ephemeral_backlog {
            if let Some(line) = to_ndjson_line(&SessionNdjsonEnvelope::ephemeral_event(event)) {
                yield Ok::<Bytes, Infallible>(line);
            }
        }

        let mut heartbeat_tick = tokio::time::interval(RUNTIME_TRACE_HEARTBEAT_INTERVAL);
        heartbeat_tick.set_missed_tick_behavior(MissedTickBehavior::Delay);
        let mut rx = subscription.rx;

        loop {
            tokio::select! {
                next = rx.recv() => {
                    match next {
                        Ok(event) => {
                            if event.ephemeral {
                                // ephemeral 事件 event_seq=0，不参与 snapshot_seq 去重、不推进游标；
                                // 直接 emit 为 ephemeral envelope（live-only）。
                                if let Some(line) =
                                    to_ndjson_line(&SessionNdjsonEnvelope::ephemeral_event(event))
                                {
                                    yield Ok::<Bytes, Infallible>(line);
                                }
                                continue;
                            }
                            if event.event_seq <= subscription.snapshot_seq {
                                continue;
                            }
                            seq = event.event_seq;
                            if let Some(line) = to_ndjson_line(&stream_event_payload(event)) {
                                yield Ok::<Bytes, Infallible>(line);
                            }
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                            diag!(Warn, Subsystem::Api,

                                session_id = %session_id,
                                lagged = n,
                                "Session trace stream 订阅落后，部分消息被跳过（NDJSON）"
                            );
                            continue;
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                            diag!(Info, Subsystem::Api,

                                session_id = %session_id,
                                last_seq = seq,
                                "Session trace stream 连接关闭：广播通道关闭（NDJSON）"
                            );
                            break;
                        }
                    }
                }
                _ = heartbeat_tick.tick() => {
                    if let Some(line) = to_ndjson_line(&SessionNdjsonEnvelope::heartbeat_now()) {
                        yield Ok::<Bytes, Infallible>(line);
                    }
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
    ))
}

fn parse_resume_from_header(
    headers: &HeaderMap,
    header_name: &'static str,
) -> Result<Option<u64>, ApiError> {
    let Some(value) = headers.get(header_name) else {
        return Ok(None);
    };
    let raw = value
        .to_str()
        .map_err(|_| ApiError::BadRequest(format!("{header_name} 不是有效 UTF-8")))?;
    let parsed = raw
        .parse::<i64>()
        .map_err(|_| ApiError::BadRequest(format!("{header_name} 不是有效整数")))?;
    if parsed < 0 {
        return Err(ApiError::BadRequest(format!("{header_name} 不能为负数")));
    }
    Ok(Some(parsed as u64))
}

fn to_ndjson_line(value: &SessionNdjsonEnvelope) -> Option<Bytes> {
    match serde_json::to_vec(value) {
        Ok(mut bytes) => {
            bytes.push(b'\n');
            Some(Bytes::from(bytes))
        }
        Err(err) => {
            let context = DiagnosticErrorContext::new("session_trace.ndjson", "serialize_event");
            diag_error!(
                Error,
                Subsystem::Api,
                context = &context,
                error = &err,
                route = "/api/sessions/{id}/trace.ndjson",
                "序列化 Session NDJSON 消息失败"
            );
            None
        }
    }
}

fn api_error_from_io(error: io::Error) -> ApiError {
    match error.kind() {
        io::ErrorKind::InvalidInput | io::ErrorKind::InvalidData => {
            ApiError::BadRequest(error.to_string())
        }
        io::ErrorKind::NotFound => ApiError::NotFound(error.to_string()),
        io::ErrorKind::AlreadyExists => ApiError::Conflict(error.to_string()),
        _ => ApiError::Internal(String::from("内部 IO 错误")),
    }
}

// ═══════════════════════════════════════════════════════════════════
// Context Audit —— Bundle / Fragment 产出与消费的可观测轨迹（Step 10d）
// ═══════════════════════════════════════════════════════════════════

/// Content preview 的最大字节数（超过时截断）。
const CONTEXT_AUDIT_CONTENT_PREVIEW_MAX: usize = 2048;

fn parse_scope_tag(tag: &str) -> Option<agentdash_spi::FragmentScope> {
    match tag {
        "runtime_agent" => Some(agentdash_spi::FragmentScope::RuntimeAgent),
        "title_gen" => Some(agentdash_spi::FragmentScope::TitleGen),
        "summarizer" => Some(agentdash_spi::FragmentScope::Summarizer),
        "bridge_replay" => Some(agentdash_spi::FragmentScope::BridgeReplay),
        "audit" => Some(agentdash_spi::FragmentScope::Audit),
        _ => None,
    }
}

fn scope_set_to_tags(scope: agentdash_spi::FragmentScopeSet) -> Vec<String> {
    let mut tags = Vec::new();
    for (label, s) in [
        ("runtime_agent", agentdash_spi::FragmentScope::RuntimeAgent),
        ("title_gen", agentdash_spi::FragmentScope::TitleGen),
        ("summarizer", agentdash_spi::FragmentScope::Summarizer),
        ("bridge_replay", agentdash_spi::FragmentScope::BridgeReplay),
        ("audit", agentdash_spi::FragmentScope::Audit),
    ] {
        if scope.contains(s) {
            tags.push(label.to_string());
        }
    }
    tags
}

/// Internal diagnostics: `GET /sessions/{id}/context/audit` —— 返回 runtime trace 的 Fragment 审计时间线。
///
/// 返回按 `at_ms` 升序的事件列表（审计总线内部已保持插入顺序）。
pub async fn get_session_context_audit(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(session_id): Path<String>,
    Query(query): Query<ContextAuditQuery>,
) -> Result<Json<Vec<ContextAuditEventDto>>, ApiError> {
    ensure_session_permission(
        state.as_ref(),
        &current_user,
        &session_id,
        ProjectPermission::Use,
    )
    .await?;
    let scope = match query.scope.as_deref() {
        Some(raw) => match parse_scope_tag(raw) {
            Some(s) => Some(s),
            None => return Err(ApiError::BadRequest(format!("无效的 scope: {raw}"))),
        },
        None => None,
    };

    let filter = agentdash_application::context::AuditFilter {
        since_ms: query.since_ms,
        scope,
        slot: query.slot.clone(),
        source_prefix: query.source_prefix.clone(),
    };

    let events = state.services.audit_bus.query(&session_id, &filter);
    let dtos: Vec<ContextAuditEventDto> = events
        .into_iter()
        .map(|event| {
            let full_len = event.fragment.content.len();
            let truncated = full_len > CONTEXT_AUDIT_CONTENT_PREVIEW_MAX;
            let preview = if truncated {
                // 按字符边界截断，避免切断 UTF-8 多字节
                let mut end = CONTEXT_AUDIT_CONTENT_PREVIEW_MAX;
                while end > 0 && !event.fragment.content.is_char_boundary(end) {
                    end -= 1;
                }
                event.fragment.content[..end].to_string()
            } else {
                event.fragment.content.clone()
            };
            ContextAuditEventDto {
                event_id: event.event_id,
                bundle_id: event.bundle_id,
                session_id: event.session_id,
                bundle_session_uuid: event.bundle_session_uuid,
                at_ms: event.at_ms,
                trigger: event.trigger.as_tag(),
                slot: event.fragment.slot,
                label: event.fragment.label,
                source: event.fragment.source,
                order: event.fragment.order,
                scope: scope_set_to_tags(event.fragment.scope),
                content_preview: preview,
                content_hash: event.content_hash,
                full_content_available: truncated,
            }
        })
        .collect();

    Ok(Json(dtos))
}
