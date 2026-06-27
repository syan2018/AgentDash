use agentdash_application_agentrun::agent_run::{
    AgentFrameSurfaceExt, DeliveryRuntimeSelectionPolicy, DeliveryRuntimeSelectionService,
};
use agentdash_application_vfs::PROVIDER_CANVAS_FS;
use agentdash_domain::canvas::{
    Canvas, CanvasInteractionEvent, CanvasInteractionSnapshot, CanvasRuntimeDiagnostic,
    CanvasRuntimeDocumentState, CanvasRuntimeObservation, CanvasRuntimeObservationStatus,
    CanvasRuntimeViewport,
};
use agentdash_domain::workflow::{AgentFrame, LifecycleAgent, LifecycleRun};
use agentdash_workspace_module::canvas::canvas_module_id;
use chrono::{DateTime, Utc};
use serde_json::Value;
use uuid::Uuid;

use crate::error::ApplicationError;
use crate::repository_set::RepositorySet;

#[derive(Debug, Clone)]
pub struct CanvasAgentRunContext {
    pub run: LifecycleRun,
    pub agent: LifecycleAgent,
    pub canvas: Canvas,
    pub delivery_trace_ref: Option<String>,
    pub runtime_session_id: String,
    pub current_agent_frame: AgentFrame,
    pub agent_run_canvas_ref: String,
}

#[derive(Debug, Clone)]
pub struct CanvasRuntimeObservationInput {
    pub frame_id: String,
    pub generation: i32,
    pub captured_at: Option<DateTime<Utc>>,
    pub status: CanvasRuntimeObservationStatus,
    pub message: Option<String>,
    pub viewport: CanvasRuntimeViewport,
    pub document: CanvasRuntimeDocumentState,
    pub diagnostics: Vec<CanvasRuntimeDiagnostic>,
    pub screenshot_ref: Option<String>,
}

#[derive(Debug, Clone)]
pub struct CanvasInteractionSnapshotInput {
    pub frame_id: String,
    pub updated_at: Option<DateTime<Utc>>,
    pub state: Value,
    pub recent_events: Vec<CanvasInteractionEvent>,
}

pub async fn resolve_agent_run_canvas_context(
    repos: &RepositorySet,
    run_id: Uuid,
    agent_id: Uuid,
    canvas_mount_id: &str,
) -> Result<CanvasAgentRunContext, ApplicationError> {
    let canvas_mount_id = canvas_mount_id.trim();
    if canvas_mount_id.is_empty() {
        return Err(ApplicationError::BadRequest(
            "canvas_mount_id 不能为空".to_string(),
        ));
    }

    let run = repos
        .lifecycle_run_repo
        .get_by_id(run_id)
        .await?
        .ok_or_else(|| ApplicationError::NotFound(format!("LifecycleRun 不存在: {run_id}")))?;
    let agent = repos
        .lifecycle_agent_repo
        .get(agent_id)
        .await?
        .ok_or_else(|| ApplicationError::NotFound(format!("LifecycleAgent 不存在: {agent_id}")))?;
    if agent.run_id != run.id || agent.project_id != run.project_id {
        return Err(ApplicationError::Conflict(format!(
            "AgentRun context 不匹配: run_id={}, agent.run_id={}, run.project_id={}, agent.project_id={}",
            run.id, agent.run_id, run.project_id, agent.project_id
        )));
    }

    let canvas = repos
        .canvas_repo
        .get_by_mount_id(run.project_id, canvas_mount_id)
        .await?
        .ok_or_else(|| {
            ApplicationError::NotFound(format!(
                "Project {} 中不存在 Canvas mount {}",
                run.project_id, canvas_mount_id
            ))
        })?;
    if canvas.project_id != run.project_id {
        return Err(ApplicationError::Conflict(format!(
            "Canvas {} 不属于 AgentRun Project {}",
            canvas.id, run.project_id
        )));
    }

    let agent_run_repos = repos.to_agent_run_repository_set();
    let delivery = DeliveryRuntimeSelectionService::from_repository_set(&agent_run_repos)
        .select(DeliveryRuntimeSelectionPolicy::CurrentDelivery { run_id, agent_id })
        .await
        .map_err(agent_run_selection_error)?;
    let current_agent_frame = repos
        .agent_frame_repo
        .get(delivery.current_frame_id)
        .await?
        .ok_or_else(|| {
            ApplicationError::NotFound(format!("AgentFrame 不存在: {}", delivery.current_frame_id))
        })?;
    ensure_canvas_visible_in_frame(&current_agent_frame, canvas_mount_id)?;

    Ok(CanvasAgentRunContext {
        run,
        agent,
        canvas,
        delivery_trace_ref: Some(format!("runtime_session:{}", delivery.runtime_session_id)),
        runtime_session_id: delivery.runtime_session_id,
        current_agent_frame,
        agent_run_canvas_ref: format!("{run_id}:{agent_id}:{}", canvas_mount_id),
    })
}

pub async fn upsert_runtime_observation(
    repos: &RepositorySet,
    context: &CanvasAgentRunContext,
    input: CanvasRuntimeObservationInput,
) -> Result<CanvasRuntimeObservation, ApplicationError> {
    let observation = CanvasRuntimeObservation {
        observation_id: Uuid::new_v4(),
        run_id: context.run.id,
        agent_id: context.agent.id,
        agent_run_canvas_ref: context.agent_run_canvas_ref.clone(),
        canvas_id: context.canvas.id,
        canvas_mount_id: context.canvas.mount_id.clone(),
        delivery_trace_ref: context.delivery_trace_ref.clone(),
        current_agent_frame_id: Some(context.current_agent_frame.id),
        frame_id: input.frame_id,
        generation: input.generation,
        captured_at: input.captured_at.unwrap_or_else(Utc::now),
        status: input.status,
        message: input.message,
        viewport: input.viewport,
        document: input.document,
        diagnostics: input.diagnostics,
        screenshot_ref: input.screenshot_ref,
    };
    repos
        .canvas_runtime_state_repo
        .upsert_runtime_observation(observation)
        .await
        .map_err(ApplicationError::from)
}

pub async fn latest_runtime_observation(
    repos: &RepositorySet,
    context: &CanvasAgentRunContext,
) -> Result<Option<CanvasRuntimeObservation>, ApplicationError> {
    repos
        .canvas_runtime_state_repo
        .latest_runtime_observation(context.run.id, context.agent.id, &context.canvas.mount_id)
        .await
        .map_err(ApplicationError::from)
}

pub async fn upsert_interaction_snapshot(
    repos: &RepositorySet,
    context: &CanvasAgentRunContext,
    input: CanvasInteractionSnapshotInput,
) -> Result<CanvasInteractionSnapshot, ApplicationError> {
    let snapshot = CanvasInteractionSnapshot {
        snapshot_id: Uuid::new_v4(),
        run_id: context.run.id,
        agent_id: context.agent.id,
        agent_run_canvas_ref: context.agent_run_canvas_ref.clone(),
        canvas_id: context.canvas.id,
        canvas_mount_id: context.canvas.mount_id.clone(),
        delivery_trace_ref: context.delivery_trace_ref.clone(),
        current_agent_frame_id: Some(context.current_agent_frame.id),
        frame_id: input.frame_id,
        updated_at: input.updated_at.unwrap_or_else(Utc::now),
        state: input.state,
        recent_events: input.recent_events,
    };
    repos
        .canvas_runtime_state_repo
        .upsert_interaction_snapshot(snapshot)
        .await
        .map_err(ApplicationError::from)
}

pub async fn latest_interaction_snapshot(
    repos: &RepositorySet,
    context: &CanvasAgentRunContext,
) -> Result<Option<CanvasInteractionSnapshot>, ApplicationError> {
    repos
        .canvas_runtime_state_repo
        .latest_interaction_snapshot(context.run.id, context.agent.id, &context.canvas.mount_id)
        .await
        .map_err(ApplicationError::from)
}

fn ensure_canvas_visible_in_frame(
    frame: &AgentFrame,
    canvas_mount_id: &str,
) -> Result<(), ApplicationError> {
    let module_ref = canvas_module_id(canvas_mount_id);
    let listed_as_canvas = frame
        .visible_canvas_mount_ids()
        .iter()
        .any(|mount_id| mount_id == canvas_mount_id);
    let listed_as_module = frame
        .visible_workspace_module_refs()
        .iter()
        .any(|module| module == &module_ref);
    let mounted_in_vfs = frame.typed_vfs().is_some_and(|vfs| {
        vfs.mounts
            .iter()
            .any(|mount| mount.id == canvas_mount_id && mount.provider == PROVIDER_CANVAS_FS)
    });
    if listed_as_canvas || listed_as_module || mounted_in_vfs {
        return Ok(());
    }
    Err(ApplicationError::Forbidden(format!(
        "Canvas {canvas_mount_id} 不在当前 AgentRun delivery frame 的可见 Canvas/module surface 中"
    )))
}

fn agent_run_selection_error(
    error: agentdash_application_agentrun::agent_run::DeliveryRuntimeSelectionError,
) -> ApplicationError {
    match error {
        agentdash_application_agentrun::agent_run::DeliveryRuntimeSelectionError::RunNotFound {
            ..
        }
        | agentdash_application_agentrun::agent_run::DeliveryRuntimeSelectionError::AgentNotFound {
            ..
        }
        | agentdash_application_agentrun::agent_run::DeliveryRuntimeSelectionError::CurrentFrameNotFound {
            ..
        }
        | agentdash_application_agentrun::agent_run::DeliveryRuntimeSelectionError::LaunchFrameNotFound {
            ..
        }
        | agentdash_application_agentrun::agent_run::DeliveryRuntimeSelectionError::SubjectNotFound {
            ..
        } => ApplicationError::NotFound(error.to_string()),
        agentdash_application_agentrun::agent_run::DeliveryRuntimeSelectionError::Repository(
            source,
        ) => ApplicationError::from(source),
        other => ApplicationError::Conflict(other.to_string()),
    }
}
