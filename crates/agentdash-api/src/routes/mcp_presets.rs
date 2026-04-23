//! MCP Preset HTTP 路由——Project 级 MCP Server 配置模板的 CRUD + builtin 管理。
//!
//! 路由前缀统一为 `/api/projects/{project_id}/mcp-presets`，与 Canvas 对齐。
//! 编辑/删除 builtin 会返回 409 Conflict；复制 builtin 为 user 产生可编辑副本。

use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, Query, State};
use serde::Deserialize;
use uuid::Uuid;

use agentdash_application::mcp_preset::{
    CloneMcpPresetInput, CreateMcpPresetInput, McpPresetApplicationError, McpPresetService,
    UpdateMcpPresetInput, probe_transport,
};
use agentdash_domain::mcp_preset::{McpPreset, McpTransportConfig};

use crate::app_state::AppState;
use crate::auth::{CurrentUser, ProjectPermission, load_project_with_permission};
use crate::dto::{
    BootstrapMcpPresetRequest, CloneMcpPresetRequest, CreateMcpPresetRequest, ListMcpPresetQuery,
    McpPresetResponse, ProbeMcpPresetResponse, UpdateMcpPresetRequest,
};
use crate::rpc::ApiError;

#[derive(Debug, Deserialize)]
pub struct ProjectMcpPresetsPath {
    pub project_id: String,
}

#[derive(Debug, Deserialize)]
pub struct McpPresetItemPath {
    pub project_id: String,
    pub id: String,
}

/// GET `/api/projects/:project_id/mcp-presets`
///
/// 支持 `?source=user|builtin` 过滤；传入其他非空值会返回 400。
pub async fn list_mcp_presets(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(path): Path<ProjectMcpPresetsPath>,
    Query(query): Query<ListMcpPresetQuery>,
) -> Result<Json<Vec<McpPresetResponse>>, ApiError> {
    let project_id = parse_project_id(&path.project_id)?;
    load_project_with_permission(
        state.as_ref(),
        &current_user,
        project_id,
        ProjectPermission::View,
    )
    .await?;

    let service = McpPresetService::new(state.repos.mcp_preset_repo.as_ref());
    let mut presets = service.list(project_id).await?;

    // source 过滤在 API 层做即可——服务层保留通用 list
    match query.source.as_deref() {
        Some("user") => presets.retain(|p| !p.is_builtin()),
        Some("builtin") => presets.retain(|p| p.is_builtin()),
        Some(other) if !other.is_empty() => {
            return Err(ApiError::BadRequest(format!(
                "无效的 source 过滤值: {other}（可选 user | builtin）"
            )));
        }
        _ => {}
    }

    Ok(Json(presets.into_iter().map(Into::into).collect()))
}

/// POST `/api/projects/:project_id/mcp-presets`
///
/// 创建 user Preset。builtin Preset 须走 `/bootstrap` 端点。
pub async fn create_mcp_preset(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(path): Path<ProjectMcpPresetsPath>,
    Json(req): Json<CreateMcpPresetRequest>,
) -> Result<Json<McpPresetResponse>, ApiError> {
    let project_id = parse_project_id(&path.project_id)?;
    load_project_with_permission(
        state.as_ref(),
        &current_user,
        project_id,
        ProjectPermission::Edit,
    )
    .await?;

    let service = McpPresetService::new(state.repos.mcp_preset_repo.as_ref());
    let preset = service
        .create(CreateMcpPresetInput {
            project_id,
            key: req.key,
            display_name: req.display_name,
            description: req.description,
            transport: req.transport,
            route_policy: req.route_policy,
        })
        .await?;
    Ok(Json(preset.into()))
}

/// GET `/api/projects/:project_id/mcp-presets/:id`
pub async fn get_mcp_preset(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(path): Path<McpPresetItemPath>,
) -> Result<Json<McpPresetResponse>, ApiError> {
    let (project_id, preset) = load_preset_with_project(
        state.as_ref(),
        &current_user,
        &path,
        ProjectPermission::View,
    )
    .await?;
    debug_assert_eq!(preset.project_id, project_id);
    Ok(Json(preset.into()))
}

/// PATCH `/api/projects/:project_id/mcp-presets/:id`
///
/// 更新 user Preset；builtin 会收到 409 Conflict。
pub async fn update_mcp_preset(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(path): Path<McpPresetItemPath>,
    Json(req): Json<UpdateMcpPresetRequest>,
) -> Result<Json<McpPresetResponse>, ApiError> {
    let (_project_id, preset) = load_preset_with_project(
        state.as_ref(),
        &current_user,
        &path,
        ProjectPermission::Edit,
    )
    .await?;

    let service = McpPresetService::new(state.repos.mcp_preset_repo.as_ref());
    let updated = service
        .update(
            preset.id,
            UpdateMcpPresetInput {
                key: req.key,
                display_name: req.display_name,
                description: req.description,
                transport: req.transport,
                route_policy: req.route_policy,
            },
        )
        .await?;
    Ok(Json(updated.into()))
}

/// DELETE `/api/projects/:project_id/mcp-presets/:id`
///
/// 仅 user 可删；builtin 会收到 409 Conflict。
pub async fn delete_mcp_preset(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(path): Path<McpPresetItemPath>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let (_project_id, preset) = load_preset_with_project(
        state.as_ref(),
        &current_user,
        &path,
        ProjectPermission::Edit,
    )
    .await?;

    let service = McpPresetService::new(state.repos.mcp_preset_repo.as_ref());
    service.delete(preset.id).await?;
    Ok(Json(serde_json::json!({ "deleted": preset.id })))
}

/// POST `/api/projects/:project_id/mcp-presets/:id/clone`
///
/// 复制 Preset 为新的 user 副本；builtin / user 来源均可复制。
pub async fn clone_mcp_preset(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(path): Path<McpPresetItemPath>,
    Json(req): Json<CloneMcpPresetRequest>,
) -> Result<Json<McpPresetResponse>, ApiError> {
    let (_project_id, source) = load_preset_with_project(
        state.as_ref(),
        &current_user,
        &path,
        ProjectPermission::Edit,
    )
    .await?;

    let new_key = req
        .key
        .and_then(|raw| {
            let trimmed = raw.trim().to_string();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed)
            }
        })
        .unwrap_or_else(|| format!("{}-copy", source.key));
    let new_display_name = req.display_name.and_then(|raw| {
        let trimmed = raw.trim().to_string();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    });

    let service = McpPresetService::new(state.repos.mcp_preset_repo.as_ref());
    let cloned = service
        .clone_as_user(CloneMcpPresetInput {
            source_id: source.id,
            new_key,
            new_display_name,
        })
        .await?;
    Ok(Json(cloned.into()))
}

/// POST `/api/projects/:project_id/mcp-presets/bootstrap`
///
/// 装载 builtin 模板。请求体为空时装载全部；指定 `builtin_key` 时仅装载对应模板。
///
/// TODO(assets-bootstrap-on-project-create): 当前 builtin 装载是显式端点触发，
/// 新 Project 创建后不会自动 seed builtin Preset；后续可在 `CreateProjectUseCase`
/// 里补调 `bootstrap_builtins`（见父 PRD「活引用语义铺垫」章节）。
pub async fn bootstrap_mcp_presets(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(path): Path<ProjectMcpPresetsPath>,
    Json(req): Json<BootstrapMcpPresetRequest>,
) -> Result<Json<Vec<McpPresetResponse>>, ApiError> {
    let project_id = parse_project_id(&path.project_id)?;
    load_project_with_permission(
        state.as_ref(),
        &current_user,
        project_id,
        ProjectPermission::Edit,
    )
    .await?;

    let service = McpPresetService::new(state.repos.mcp_preset_repo.as_ref());
    let presets = match req.builtin_key {
        Some(key) if !key.trim().is_empty() => {
            let preset = service
                .bootstrap_builtin_by_key(project_id, key.trim())
                .await?;
            vec![preset]
        }
        _ => service.bootstrap_builtins(project_id).await?,
    };
    Ok(Json(presets.into_iter().map(Into::into).collect()))
}

/// POST `/api/projects/:project_id/mcp-presets/probe`
///
/// 对任意 transport 配置进行 probe —— 不绑定已落库的 Preset，调用方直接
/// 传入当前要验证的 transport（卡片传已保存的；detail dialog 传编辑中的）。
///
/// - Http/Sse：云端直连，返回 tools 列表 + 延迟
/// - Stdio：返回 unsupported（后续通过 relay 下发给 local 端，当前不支持）
/// - 连接失败/超时：返回 error 状态 + 错误信息
///
/// 需要 project View 权限（project id 仅用于鉴权，transport 不落库）。
/// 超时上限 15 秒。
pub async fn probe_mcp_transport_handler(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(path): Path<ProjectMcpPresetsPath>,
    Json(transport): Json<McpTransportConfig>,
) -> Result<Json<ProbeMcpPresetResponse>, ApiError> {
    let project_id = parse_project_id(&path.project_id)?;
    load_project_with_permission(
        state.as_ref(),
        &current_user,
        project_id,
        ProjectPermission::View,
    )
    .await?;

    let result = probe_transport(&transport).await;
    Ok(Json(result))
}

/// 载入并校验：preset 存在 + 属于路径中的 project + 当前用户具备所需权限。
async fn load_preset_with_project(
    state: &AppState,
    current_user: &agentdash_plugin_api::AuthIdentity,
    path: &McpPresetItemPath,
    permission: ProjectPermission,
) -> Result<(Uuid, McpPreset), ApiError> {
    let project_id = parse_project_id(&path.project_id)?;
    let preset_id = parse_preset_id(&path.id)?;

    load_project_with_permission(state, current_user, project_id, permission).await?;

    let service = McpPresetService::new(state.repos.mcp_preset_repo.as_ref());
    let preset = service.get(preset_id).await.map_err(|err| match err {
        McpPresetApplicationError::NotFound(msg) => ApiError::NotFound(msg),
        other => other.into(),
    })?;

    if preset.project_id != project_id {
        // 不直接回显 project mismatch 细节，避免被当成探测枚举
        return Err(ApiError::NotFound(format!(
            "mcp_preset 不存在: {preset_id}"
        )));
    }
    Ok((project_id, preset))
}

fn parse_project_id(raw: &str) -> Result<Uuid, ApiError> {
    Uuid::parse_str(raw).map_err(|_| ApiError::BadRequest("无效的 Project ID".into()))
}

fn parse_preset_id(raw: &str) -> Result<Uuid, ApiError> {
    Uuid::parse_str(raw).map_err(|_| ApiError::BadRequest("无效的 mcp_preset ID".into()))
}

#[cfg(test)]
mod tests {
    //! 路由层纯函数测试——涵盖 path / query 参数解析和错误映射边界。
    //!
    //! 端到端 CRUD 依赖完整 AppState，已由 `McpPresetService` 单测
    //! （`agentdash-application`）及 `PostgresMcpPresetRepository` 集成测试
    //! （`agentdash-infrastructure`）覆盖，这里只做路由层独有的契约校验。

    use super::*;
    use crate::rpc::ApiError;

    #[test]
    fn parse_project_id_rejects_invalid() {
        let err = parse_project_id("not-a-uuid").expect_err("invalid uuid");
        assert!(matches!(err, ApiError::BadRequest(_)));
    }

    #[test]
    fn parse_project_id_accepts_valid_uuid() {
        let id = Uuid::new_v4().to_string();
        assert!(parse_project_id(&id).is_ok());
    }

    #[test]
    fn parse_preset_id_rejects_invalid() {
        let err = parse_preset_id("abc").expect_err("invalid uuid");
        assert!(matches!(err, ApiError::BadRequest(_)));
    }

    #[test]
    fn mcp_preset_application_error_maps_to_expected_api_error() {
        // BadRequest → 400
        let err: ApiError = McpPresetApplicationError::BadRequest("bad".to_string()).into();
        assert!(matches!(err, ApiError::BadRequest(_)));

        // NotFound → 404
        let err: ApiError = McpPresetApplicationError::NotFound("missing".to_string()).into();
        assert!(matches!(err, ApiError::NotFound(_)));

        // Conflict → 409
        let err: ApiError = McpPresetApplicationError::Conflict("dup".to_string()).into();
        assert!(matches!(err, ApiError::Conflict(_)));

        // Internal(非 unique 信息) → 500
        let err: ApiError = McpPresetApplicationError::Internal("io error".to_string()).into();
        assert!(matches!(err, ApiError::Internal(_)));

        // Internal(unique 关键字) → 409（兜底 race 场景）
        let err: ApiError = McpPresetApplicationError::Internal(
            "duplicate key value violates unique constraint \"idx_mcp_presets_project_key\""
                .to_string(),
        )
        .into();
        assert!(matches!(err, ApiError::Conflict(_)));
    }
}
