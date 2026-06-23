use agentdash_domain::canvas::{Canvas, CanvasRepository};
use agentdash_spi::{AgentToolError, Vfs};
use std::sync::Arc;
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::agent_run::{
    AgentRunFrameSurfaceCommandOutcome, AgentRunFrameSurfaceError, AgentRunFrameSurfaceService,
    AgentRunRuntimeSurfaceUpdateAdapter, CanvasVisibilityReason, RejectingFrameConstructionAdapter,
    RuntimeSurfaceUpdateRequest,
};
use crate::canvas::normalize_canvas_mount_id;
use crate::runtime_tools::SharedSessionToolServicesHandle;
use crate::session::SessionCapabilityService;
use crate::vfs::tools::fs::SharedRuntimeVfs;

pub(crate) async fn submit_canvas_runtime_surface_update(
    vfs: Option<&SharedRuntimeVfs>,
    session_services_handle: &SharedSessionToolServicesHandle,
    current_session_id: Option<&str>,
    canvas: &Canvas,
    request: RuntimeSurfaceUpdateRequest,
) -> Result<(), AgentToolError> {
    ensure_canvas_surface_request_targets_canvas(&request, canvas)?;
    let session_services = session_services_handle.get().await.ok_or_else(|| {
        AgentToolError::ExecutionFailed(format!(
            "Session services 尚未完成初始化，无法提交 Canvas runtime surface request: {request:?}"
        ))
    })?;
    let session_id = current_session_id.ok_or_else(|| {
        AgentToolError::ExecutionFailed(format!(
            "当前工具调用缺少 RuntimeSession id，无法提交 Canvas runtime surface request: {request:?}"
        ))
    })?;

    let active_vfs = Arc::new(Mutex::new(None));
    let adapter = CanvasRuntimeSurfaceUpdateAdapter {
        capability: session_services.capability,
        session_id: session_id.to_string(),
        canvas: canvas.clone(),
        active_vfs: active_vfs.clone(),
    };
    let service = AgentRunFrameSurfaceService::new(
        Arc::new(RejectingFrameConstructionAdapter),
        Arc::new(adapter),
    );
    service
        .update_runtime_surface(request.clone())
        .await
        .map_err(|error| {
            AgentToolError::ExecutionFailed(format!(
                "Canvas runtime surface request `{request:?}` 写入 AgentFrame 失败: {error}"
            ))
        })?;
    if let Some(vfs) = vfs {
        if let Some(active_vfs) = active_vfs.lock().await.take() {
            vfs.replace(active_vfs).await;
        }
    }
    Ok(())
}

struct CanvasRuntimeSurfaceUpdateAdapter {
    capability: SessionCapabilityService,
    session_id: String,
    canvas: Canvas,
    active_vfs: Arc<Mutex<Option<Vfs>>>,
}

#[async_trait::async_trait]
impl AgentRunRuntimeSurfaceUpdateAdapter for CanvasRuntimeSurfaceUpdateAdapter {
    async fn execute_runtime_surface_update(
        &self,
        request: RuntimeSurfaceUpdateRequest,
    ) -> Result<AgentRunFrameSurfaceCommandOutcome, AgentRunFrameSurfaceError> {
        ensure_canvas_surface_request_targets_canvas(&request, &self.canvas).map_err(|error| {
            AgentRunFrameSurfaceError::RuntimeSurfaceUpdateRejected(error.to_string())
        })?;
        let active_vfs = self
            .capability
            .expose_canvas_mount_revision_and_adopt(&self.session_id, &self.canvas)
            .await
            .map_err(AgentRunFrameSurfaceError::RuntimeSurfaceUpdateRejected)?;
        *self.active_vfs.lock().await = Some(active_vfs);
        let mut outcome = AgentRunFrameSurfaceCommandOutcome::runtime_surface_update();
        outcome.runtime_session_id = Some(self.session_id.clone());
        outcome.wrote_frame_revision = true;
        outcome.adopted_active_runtime = true;
        Ok(outcome)
    }
}

pub(crate) async fn submit_existing_canvas_visibility_request(
    canvas_repo: &dyn CanvasRepository,
    project_id: Uuid,
    canvas_mount_id: &str,
    vfs: Option<&SharedRuntimeVfs>,
    session_services_handle: &SharedSessionToolServicesHandle,
    current_session_id: Option<&str>,
) -> Result<Canvas, AgentToolError> {
    let canvas = load_canvas_by_project_mount_id(canvas_repo, project_id, canvas_mount_id).await?;
    submit_canvas_runtime_surface_update(
        vfs,
        session_services_handle,
        current_session_id,
        &canvas,
        RuntimeSurfaceUpdateRequest::CanvasVisibilityRequested {
            canvas_mount_id: canvas.mount_id.clone(),
            reason: CanvasVisibilityReason::Presented,
        },
    )
    .await?;
    Ok(canvas)
}

fn ensure_canvas_surface_request_targets_canvas(
    request: &RuntimeSurfaceUpdateRequest,
    canvas: &Canvas,
) -> Result<(), AgentToolError> {
    let canvas_mount_id = match request {
        RuntimeSurfaceUpdateRequest::CanvasBindingChanged { canvas_mount_id }
        | RuntimeSurfaceUpdateRequest::CanvasVisibilityRequested {
            canvas_mount_id, ..
        } => canvas_mount_id,
        _ => {
            return Err(AgentToolError::ExecutionFailed(format!(
                "Canvas adapter received non-Canvas runtime surface request: {request:?}"
            )));
        }
    };
    if canvas_mount_id == &canvas.mount_id {
        Ok(())
    } else {
        Err(AgentToolError::ExecutionFailed(format!(
            "Canvas runtime surface request target `{canvas_mount_id}` does not match Canvas `{}`",
            canvas.mount_id
        )))
    }
}

async fn load_canvas_by_project_mount_id(
    canvas_repo: &dyn CanvasRepository,
    expected_project_id: Uuid,
    raw_canvas_mount_id: &str,
) -> Result<Canvas, AgentToolError> {
    let canvas_mount_id = normalize_canvas_mount_id(raw_canvas_mount_id)
        .map_err(|error| AgentToolError::InvalidArguments(error.to_string()))?;

    let canvas = canvas_repo
        .get_by_mount_id(expected_project_id, &canvas_mount_id)
        .await
        .map_err(|error| AgentToolError::ExecutionFailed(error.to_string()))?;
    let canvas = canvas.ok_or_else(|| {
        AgentToolError::ExecutionFailed(format!("Canvas 不存在: {canvas_mount_id}"))
    })?;
    if canvas.project_id != expected_project_id {
        return Err(AgentToolError::ExecutionFailed(
            "当前 session 无权操作其它 Project 的 Canvas".to_string(),
        ));
    }
    Ok(canvas)
}
