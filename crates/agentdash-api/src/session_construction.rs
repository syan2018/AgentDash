use std::sync::Arc;

use agentdash_application::agent_run::frame::surface::AgentFrameSurfaceExt;
use agentdash_application::lifecycle::resolve_current_frame_from_delivery_trace_ref;
use agentdash_domain::workflow::AgentFrame;
use agentdash_integration_api::AuthIdentity;
use agentdash_spi::{RuntimeBackendAnchor, Vfs};

use crate::app_state::AppState;
use crate::auth::{ProjectPermission, load_project_with_permission};
use crate::rpc::ApiError;

/// session shell → anchor → frame → typed VFS 查询结果。
///
/// 从 `AgentFrame` 直接反序列化 `vfs_surface_json`，复用 frame construction 主链路产物。
pub(crate) struct SessionFrameVfsResult {
    pub vfs: Option<Vfs>,
    pub runtime_backend_anchor: Option<RuntimeBackendAnchor>,
    #[allow(dead_code)]
    pub frame: AgentFrame,
}

/// 通过 runtime session id 的控制面 anchor 查当前 AgentFrame，返回 frame 上记录的 typed VFS。
///
/// 同时完成 project 权限校验（anchor run project → permission check）。
pub(crate) async fn resolve_session_frame_vfs(
    state: &Arc<AppState>,
    current_user: &AuthIdentity,
    session_id: &str,
) -> Result<SessionFrameVfsResult, ApiError> {
    let _meta = state
        .services
        .session_core
        .get_session_meta(session_id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("会话 {session_id} 不存在")))?;
    let (anchor, agent, frame) = resolve_current_frame_from_delivery_trace_ref(
        session_id,
        state.repos.execution_anchor_repo.as_ref(),
        state.repos.lifecycle_agent_repo.as_ref(),
        state.repos.agent_frame_repo.as_ref(),
    )
    .await
    .map_err(ApiError::from)?
    .ok_or_else(|| {
        ApiError::NotFound(format!(
            "runtime_session 缺少可用 RuntimeSessionExecutionAnchor/AgentFrame: {session_id}"
        ))
    })?;
    let run = state
        .repos
        .lifecycle_run_repo
        .get_by_id(anchor.run_id)
        .await
        .map_err(ApiError::from)?
        .ok_or_else(|| ApiError::NotFound(format!("lifecycle_run 不存在: {}", anchor.run_id)))?;
    load_project_with_permission(
        state.as_ref(),
        current_user,
        run.project_id,
        ProjectPermission::View,
    )
    .await?;
    if agent.run_id != run.id || agent.project_id != run.project_id {
        return Err(ApiError::BadRequest(format!(
            "runtime session anchor 控制面不一致: {session_id}"
        )));
    }

    let runtime_backend_anchor = state
        .services
        .session_capability
        .get_current_runtime_backend_anchor(session_id)
        .await
        .ok();
    let vfs = frame.typed_vfs();
    Ok(SessionFrameVfsResult {
        vfs,
        runtime_backend_anchor,
        frame,
    })
}
