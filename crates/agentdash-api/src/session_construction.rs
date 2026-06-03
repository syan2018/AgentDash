use std::sync::Arc;

use agentdash_application::workflow::frame_surface::AgentFrameSurfaceExt;
use agentdash_domain::workflow::AgentFrame;
use agentdash_plugin_api::AuthIdentity;
use agentdash_spi::Vfs;
use uuid::Uuid;

use crate::app_state::AppState;
use crate::auth::{ProjectPermission, load_project_with_permission};
use crate::rpc::ApiError;

/// session shell → anchor → frame → typed VFS 查询结果。
///
/// 从 `AgentFrame` 直接反序列化 `vfs_surface_json` 获得，
/// 替代此前 `RuntimeContextInspectionPlanner::plan_project_context_query` 重建完整 plan 的路径。
pub(crate) struct SessionFrameVfsResult {
    pub vfs: Option<Vfs>,
    #[allow(dead_code)]
    pub frame: AgentFrame,
}

/// 通过 runtime session id 的控制面 anchor 查 AgentFrame，返回 frame 上记录的 typed VFS。
///
/// 同时完成 project 权限校验（session shell project → permission check → anchor run project 校验）。
pub(crate) async fn resolve_session_frame_vfs(
    state: &Arc<AppState>,
    current_user: &AuthIdentity,
    session_id: &str,
) -> Result<SessionFrameVfsResult, ApiError> {
    let meta = state
        .services
        .session_core
        .get_session_meta(session_id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("会话 {session_id} 不存在")))?;
    let session_project_id_raw = meta.project_id.as_deref().ok_or_else(|| {
        ApiError::BadRequest(format!("runtime session 缺少 project shell: {session_id}"))
    })?;
    let session_project_id = parse_uuid(session_project_id_raw, "session_project_id")?;

    load_project_with_permission(
        state.as_ref(),
        current_user,
        session_project_id,
        ProjectPermission::View,
    )
    .await?;

    let anchor = state
        .repos
        .execution_anchor_repo
        .find_by_session(session_id)
        .await
        .map_err(ApiError::from)?
        .ok_or_else(|| {
            ApiError::NotFound(format!(
                "runtime_session 缺少 RuntimeSessionExecutionAnchor: {session_id}"
            ))
        })?;

    let agent = state
        .repos
        .lifecycle_agent_repo
        .get(anchor.agent_id)
        .await
        .map_err(ApiError::from)?
        .ok_or_else(|| {
            ApiError::NotFound(format!("lifecycle_agent 不存在: {}", anchor.agent_id))
        })?;
    let run = state
        .repos
        .lifecycle_run_repo
        .get_by_id(anchor.run_id)
        .await
        .map_err(ApiError::from)?
        .ok_or_else(|| ApiError::NotFound(format!("lifecycle_run 不存在: {}", anchor.run_id)))?;
    if run.project_id != session_project_id
        || agent.run_id != run.id
        || agent.project_id != run.project_id
    {
        return Err(ApiError::BadRequest(format!(
            "runtime session project 与 anchor 控制面不一致: {session_id}"
        )));
    }

    let frame = state
        .repos
        .agent_frame_repo
        .get_current(agent.id)
        .await
        .map_err(ApiError::from)?
        .or(state
            .repos
            .agent_frame_repo
            .get(anchor.launch_frame_id)
            .await?)
        .ok_or_else(|| {
            ApiError::NotFound(format!(
                "lifecycle_agent {} 没有 current AgentFrame",
                agent.id
            ))
        })?;

    let vfs = frame.typed_vfs();
    Ok(SessionFrameVfsResult { vfs, frame })
}

fn parse_uuid(raw: &str, field: &str) -> Result<Uuid, ApiError> {
    Uuid::parse_str(raw).map_err(|_| ApiError::BadRequest(format!("无效的 {field}: {raw}")))
}
