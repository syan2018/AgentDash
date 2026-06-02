use std::sync::Arc;

use agentdash_application::workflow::frame_surface::AgentFrameSurfaceExt;
use agentdash_domain::workflow::AgentFrame;
use agentdash_plugin_api::AuthIdentity;
use agentdash_spi::Vfs;

use crate::app_state::AppState;
use crate::auth::{ProjectPermission, load_project_with_permission};
use crate::rpc::ApiError;

/// session → frame → typed VFS 查询结果。
///
/// 从 `AgentFrame` 直接反序列化 `vfs_surface_json` 获得，
/// 替代此前 `RuntimeContextInspectionPlanner::plan_project_context_query` 重建完整 plan 的路径。
pub(crate) struct SessionFrameVfsResult {
    pub vfs: Option<Vfs>,
    #[allow(dead_code)]
    pub frame: AgentFrame,
}

/// 通过 runtime session id 查 AgentFrame，返回 frame 上记录的 typed VFS。
///
/// 同时完成 project 权限校验（frame → lifecycle_agent → project_id → permission check）。
pub(crate) async fn resolve_session_frame_vfs(
    state: &Arc<AppState>,
    current_user: &AuthIdentity,
    session_id: &str,
) -> Result<SessionFrameVfsResult, ApiError> {
    let frame = state
        .repos
        .agent_frame_repo
        .find_by_runtime_session(session_id)
        .await
        .map_err(ApiError::from)?
        .ok_or_else(|| {
            ApiError::NotFound(format!("runtime_session 未附着到 AgentFrame: {session_id}"))
        })?;

    let agent = state
        .repos
        .lifecycle_agent_repo
        .get(frame.agent_id)
        .await
        .map_err(ApiError::from)?
        .ok_or_else(|| ApiError::NotFound(format!("lifecycle_agent 不存在: {}", frame.agent_id)))?;

    load_project_with_permission(
        state.as_ref(),
        current_user,
        agent.project_id,
        ProjectPermission::View,
    )
    .await?;

    let vfs = frame.typed_vfs();
    Ok(SessionFrameVfsResult { vfs, frame })
}
