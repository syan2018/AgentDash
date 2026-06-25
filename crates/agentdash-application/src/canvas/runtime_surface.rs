use agentdash_domain::canvas::{Canvas, CanvasRepository};
use agentdash_domain::project::ProjectAuthorizationContext;
use agentdash_spi::AgentToolError;
use uuid::Uuid;

use crate::agent_run::{CanvasVisibilityReason, RuntimeSurfaceUpdateRequest};
use crate::canvas::normalize_canvas_mount_id;
use crate::runtime_tools::SharedSessionToolServicesHandle;
use crate::vfs::tools::fs::SharedRuntimeVfs;

pub(crate) async fn submit_canvas_runtime_surface_update(
    vfs: Option<&SharedRuntimeVfs>,
    session_services_handle: &SharedSessionToolServicesHandle,
    current_session_id: Option<&str>,
    current_user: Option<&ProjectAuthorizationContext>,
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
    let active_vfs = session_services
        .runtime_surface_update
        .expose_canvas_mount(session_id, canvas, current_user)
        .await
        .map_err(|error| {
            AgentToolError::ExecutionFailed(format!(
                "Canvas runtime surface request `{request:?}` 写入 AgentFrame 失败: {error}"
            ))
        })?;
    if let Some(vfs) = vfs {
        vfs.replace(active_vfs).await;
    }
    Ok(())
}

pub(crate) async fn submit_existing_canvas_visibility_request(
    canvas_repo: &dyn CanvasRepository,
    project_id: Uuid,
    canvas_mount_id: &str,
    vfs: Option<&SharedRuntimeVfs>,
    session_services_handle: &SharedSessionToolServicesHandle,
    current_session_id: Option<&str>,
    current_user: Option<&ProjectAuthorizationContext>,
) -> Result<Canvas, AgentToolError> {
    let canvas = load_canvas_by_project_mount_id(canvas_repo, project_id, canvas_mount_id).await?;
    submit_canvas_runtime_surface_update(
        vfs,
        session_services_handle,
        current_session_id,
        current_user,
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
