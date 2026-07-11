//! MCP Preset HTTP 路由——Project 级 MCP Server 配置模板的 CRUD。
//!
//! 路由前缀统一为 `/api/projects/{project_id}/mcp-presets`，与 Canvas 对齐。

use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, Query, State};
use serde::Deserialize;
use uuid::Uuid;

use crate::operation_runtime::{SetupOperationScope, invoke_setup_operation};
use agentdash_application::mcp_preset::{
    CloneMcpPresetInput, CreateMcpPresetInput, McpPresetApplicationError, McpPresetService,
    UpdateMcpPresetInput,
};
use agentdash_application_operation_gateway::{
    MCP_PROBE_TRANSPORT_ACTION, McpProbeTarget, McpProbeTransportInput,
};
use agentdash_contracts::mcp_preset::{
    CloneMcpPresetRequest, CreateMcpPresetRequest, DeleteMcpPresetResponse, ListMcpPresetQuery,
    McpEnvVar, McpHttpHeader, McpPresetResponse, McpPresetSourceTag, McpProbeTargetDto,
    McpRoutePolicy, McpRuntimeBindingConfigDto, McpRuntimeBindingRuleDto,
    McpRuntimeBindingSourceDto, McpRuntimeBindingTargetDto, McpTransportConfigDto,
    ProbeMcpPresetRequest, ProbeMcpPresetResponse, UpdateMcpPresetRequest,
};
use agentdash_domain::mcp_preset::{
    McpEnvVar as DomainMcpEnvVar, McpHttpHeader as DomainMcpHttpHeader, McpPreset,
    McpRoutePolicy as DomainMcpRoutePolicy,
    McpRuntimeBindingConfig as DomainMcpRuntimeBindingConfig,
    McpRuntimeBindingRule as DomainMcpRuntimeBindingRule,
    McpRuntimeBindingSource as DomainMcpRuntimeBindingSource,
    McpRuntimeBindingTarget as DomainMcpRuntimeBindingTarget,
    McpTransportConfig as DomainMcpTransportConfig,
};

use crate::app_state::AppState;
use crate::auth::{CurrentUser, ProjectPermission, load_project_with_permission};
use crate::rpc::ApiError;

#[derive(Debug, Deserialize)]
pub struct ProjectMcpPresetsPath {
    pub project_id: String,
}

pub fn router() -> axum::Router<std::sync::Arc<crate::app_state::AppState>> {
    axum::Router::new()
        .route(
            "/projects/{project_id}/mcp-presets",
            axum::routing::get(list_mcp_presets).post(create_mcp_preset),
        )
        .route(
            "/projects/{project_id}/mcp-presets/probe",
            axum::routing::post(probe_mcp_transport_handler),
        )
        .route(
            "/projects/{project_id}/mcp-presets/{id}",
            axum::routing::get(get_mcp_preset)
                .patch(update_mcp_preset)
                .delete(delete_mcp_preset),
        )
        .route(
            "/projects/{project_id}/mcp-presets/{id}/clone",
            axum::routing::post(clone_mcp_preset),
        )
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
        ProjectPermission::Use,
    )
    .await?;

    let service = McpPresetService::new(state.repos.mcp_preset_repo.as_ref());
    let mut presets = service.list(project_id).await?;

    // source 过滤在 API 层做即可——服务层保留通用 list
    match query.source {
        Some(McpPresetSourceTag::User) => presets.retain(|p| !p.is_builtin()),
        Some(McpPresetSourceTag::Builtin) => presets.retain(|p| p.is_builtin()),
        None => {}
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
        ProjectPermission::Configure,
    )
    .await?;

    let service = McpPresetService::new(state.repos.mcp_preset_repo.as_ref());
    let preset = service
        .create(create_mcp_preset_input(project_id, req))
        .await?;
    Ok(Json(preset.into()))
}

/// GET `/api/projects/:project_id/mcp-presets/:id`
pub async fn get_mcp_preset(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(path): Path<McpPresetItemPath>,
) -> Result<Json<McpPresetResponse>, ApiError> {
    let (project_id, preset) =
        load_preset_with_project(state.as_ref(), &current_user, &path, ProjectPermission::Use)
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
        ProjectPermission::Configure,
    )
    .await?;

    let service = McpPresetService::new(state.repos.mcp_preset_repo.as_ref());
    let updated = service
        .update(preset.id, update_mcp_preset_input(req))
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
) -> Result<Json<DeleteMcpPresetResponse>, ApiError> {
    let (_project_id, preset) = load_preset_with_project(
        state.as_ref(),
        &current_user,
        &path,
        ProjectPermission::Configure,
    )
    .await?;

    let service = McpPresetService::new(state.repos.mcp_preset_repo.as_ref());
    service.delete(preset.id).await?;
    Ok(Json(DeleteMcpPresetResponse {
        deleted: preset.id.to_string(),
    }))
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
        ProjectPermission::Configure,
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

/// POST `/api/projects/:project_id/mcp-presets/probe`
///
/// 对任意 transport 配置进行 probe —— 不绑定已落库的 Preset，调用方直接
/// 传入当前要验证的 transport（卡片传已保存的；detail dialog 传编辑中的）。
///
/// - Http/Sse：云端直连，返回 tools 列表 + 延迟
/// - Stdio：通过 relay 下发给本机后端探测
/// - 连接失败/超时：返回 error 状态 + 错误信息
///
/// 需要 project View 权限（project id 仅用于鉴权，transport 不落库）。
/// 超时上限 15 秒。
pub async fn probe_mcp_transport_handler(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(path): Path<ProjectMcpPresetsPath>,
    Json(req): Json<ProbeMcpPresetRequest>,
) -> Result<Json<ProbeMcpPresetResponse>, ApiError> {
    let project_id = parse_project_id(&path.project_id)?;
    load_project_with_permission(
        state.as_ref(),
        &current_user,
        project_id,
        ProjectPermission::Use,
    )
    .await?;

    let input = serde_json::to_value(probe_mcp_transport_input(req, current_user.clone()))
        .map_err(|error| ApiError::BadRequest(format!("MCP probe 请求非法: {error}")))?;
    let output = invoke_setup_operation(
        state.as_ref(),
        &current_user,
        MCP_PROBE_TRANSPORT_ACTION,
        input,
        SetupOperationScope {
            project_id: Some(project_id),
            workspace_id: None,
            backend_id: None,
        },
    )
    .await?;
    let result = serde_json::from_value::<ProbeMcpPresetResponse>(output)
        .map_err(|error| ApiError::Internal(format!("MCP probe 返回值解析失败: {error}")))?;
    Ok(Json(result))
}

/// 载入并校验：preset 存在 + 属于路径中的 project + 当前用户具备所需权限。
async fn load_preset_with_project(
    state: &AppState,
    current_user: &agentdash_integration_api::AuthIdentity,
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

fn create_mcp_preset_input(project_id: Uuid, req: CreateMcpPresetRequest) -> CreateMcpPresetInput {
    CreateMcpPresetInput {
        project_id,
        key: req.key,
        display_name: req.display_name,
        description: req.description,
        transport: mcp_transport_config(req.transport),
        route_policy: mcp_route_policy(req.route_policy),
        runtime_binding: req.runtime_binding.map(mcp_runtime_binding_config),
    }
}

fn update_mcp_preset_input(req: UpdateMcpPresetRequest) -> UpdateMcpPresetInput {
    UpdateMcpPresetInput {
        key: req.key,
        display_name: req.display_name,
        description: req.description,
        transport: req.transport.map(mcp_transport_config),
        route_policy: req.route_policy.map(mcp_route_policy),
        runtime_binding: req
            .runtime_binding
            .map(|runtime_binding| runtime_binding.map(mcp_runtime_binding_config)),
    }
}

fn probe_mcp_transport_input(
    req: ProbeMcpPresetRequest,
    current_user: agentdash_integration_api::AuthIdentity,
) -> McpProbeTransportInput {
    McpProbeTransportInput {
        transport: mcp_transport_config(req.transport),
        route_policy: mcp_route_policy(req.route_policy),
        probe_target: req.probe_target.map(mcp_probe_target).unwrap_or_default(),
        current_user,
        runtime_binding: req.runtime_binding.map(mcp_runtime_binding_config),
    }
}

fn mcp_probe_target(target: McpProbeTargetDto) -> McpProbeTarget {
    match target {
        McpProbeTargetDto::DefaultUserLocal => McpProbeTarget::DefaultUserLocal,
        McpProbeTargetDto::Backend { backend_id } => McpProbeTarget::Backend { backend_id },
    }
}

fn mcp_transport_config(config: McpTransportConfigDto) -> DomainMcpTransportConfig {
    match config {
        McpTransportConfigDto::Http { url, headers } => DomainMcpTransportConfig::Http {
            url,
            headers: headers.into_iter().map(mcp_http_header).collect(),
        },
        McpTransportConfigDto::Sse { url, headers } => DomainMcpTransportConfig::Sse {
            url,
            headers: headers.into_iter().map(mcp_http_header).collect(),
        },
        McpTransportConfigDto::Stdio {
            command,
            args,
            env,
            cwd,
        } => DomainMcpTransportConfig::Stdio {
            command,
            args,
            env: env.into_iter().map(mcp_env_var).collect(),
            cwd,
        },
    }
}

fn mcp_http_header(header: McpHttpHeader) -> DomainMcpHttpHeader {
    DomainMcpHttpHeader {
        name: header.name,
        value: header.value,
    }
}

fn mcp_env_var(env: McpEnvVar) -> DomainMcpEnvVar {
    DomainMcpEnvVar {
        name: env.name,
        value: env.value,
    }
}

fn mcp_runtime_binding_config(config: McpRuntimeBindingConfigDto) -> DomainMcpRuntimeBindingConfig {
    DomainMcpRuntimeBindingConfig {
        mount_id: config.mount_id,
        bindings: config
            .bindings
            .into_iter()
            .map(mcp_runtime_binding_rule)
            .collect(),
    }
}

fn mcp_runtime_binding_rule(rule: McpRuntimeBindingRuleDto) -> DomainMcpRuntimeBindingRule {
    DomainMcpRuntimeBindingRule {
        source: mcp_runtime_binding_source(rule.source),
        target: mcp_runtime_binding_target(rule.target),
        required: rule.required,
    }
}

fn mcp_runtime_binding_source(source: McpRuntimeBindingSourceDto) -> DomainMcpRuntimeBindingSource {
    match source {
        McpRuntimeBindingSourceDto::VfsRootRef => DomainMcpRuntimeBindingSource::VfsRootRef,
        McpRuntimeBindingSourceDto::RuntimeBackendAnchorBackendId => {
            DomainMcpRuntimeBindingSource::RuntimeBackendAnchorBackendId
        }
        McpRuntimeBindingSourceDto::WorkspaceId => DomainMcpRuntimeBindingSource::WorkspaceId,
        McpRuntimeBindingSourceDto::WorkspaceBindingId => {
            DomainMcpRuntimeBindingSource::WorkspaceBindingId
        }
        McpRuntimeBindingSourceDto::WorkspaceIdentity { path } => {
            DomainMcpRuntimeBindingSource::WorkspaceIdentity { path }
        }
        McpRuntimeBindingSourceDto::WorkspaceDetectedFact { path } => {
            DomainMcpRuntimeBindingSource::WorkspaceDetectedFact { path }
        }
    }
}

fn mcp_runtime_binding_target(target: McpRuntimeBindingTargetDto) -> DomainMcpRuntimeBindingTarget {
    match target {
        McpRuntimeBindingTargetDto::HttpQuery { name } => {
            DomainMcpRuntimeBindingTarget::HttpQuery { name }
        }
        McpRuntimeBindingTargetDto::HttpHeader { name } => {
            DomainMcpRuntimeBindingTarget::HttpHeader { name }
        }
        McpRuntimeBindingTargetDto::StdioEnv { name } => {
            DomainMcpRuntimeBindingTarget::StdioEnv { name }
        }
        McpRuntimeBindingTargetDto::StdioCwd => DomainMcpRuntimeBindingTarget::StdioCwd,
    }
}

fn mcp_route_policy(policy: McpRoutePolicy) -> DomainMcpRoutePolicy {
    match policy {
        McpRoutePolicy::Auto => DomainMcpRoutePolicy::Auto,
        McpRoutePolicy::Relay => DomainMcpRoutePolicy::Relay,
        McpRoutePolicy::Direct => DomainMcpRoutePolicy::Direct,
    }
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
    use agentdash_integration_api::AuthMode;

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
    fn create_request_mapping_preserves_transport_runtime_binding_and_route_policy() {
        let project_id = Uuid::new_v4();
        let input = create_mcp_preset_input(
            project_id,
            CreateMcpPresetRequest {
                key: "perforce".to_string(),
                display_name: "Perforce".to_string(),
                description: Some("demo".to_string()),
                transport: McpTransportConfigDto::Stdio {
                    command: "p4-mcp".to_string(),
                    args: vec!["serve".to_string()],
                    env: vec![McpEnvVar {
                        name: "P4PORT".to_string(),
                        value: "ssl:p4:1666".to_string(),
                    }],
                    cwd: Some("C:/workspace".to_string()),
                },
                route_policy: McpRoutePolicy::Relay,
                runtime_binding: Some(sample_runtime_binding_dto(true)),
            },
        );

        assert_eq!(input.project_id, project_id);
        assert_eq!(input.route_policy, DomainMcpRoutePolicy::Relay);
        assert_eq!(
            input.transport,
            DomainMcpTransportConfig::Stdio {
                command: "p4-mcp".to_string(),
                args: vec!["serve".to_string()],
                env: vec![DomainMcpEnvVar {
                    name: "P4PORT".to_string(),
                    value: "ssl:p4:1666".to_string(),
                }],
                cwd: Some("C:/workspace".to_string()),
            }
        );
        assert!(input.runtime_binding.expect("binding").bindings[0].required);
    }

    #[test]
    fn update_request_mapping_preserves_runtime_binding_tri_state() {
        let missing = update_mcp_preset_input(UpdateMcpPresetRequest::default());
        assert!(missing.runtime_binding.is_none());

        let clear = update_mcp_preset_input(UpdateMcpPresetRequest {
            runtime_binding: Some(None),
            ..Default::default()
        });
        assert_eq!(clear.runtime_binding, Some(None));

        let replace = update_mcp_preset_input(UpdateMcpPresetRequest {
            runtime_binding: Some(Some(sample_runtime_binding_dto(false))),
            ..Default::default()
        });
        let binding = replace
            .runtime_binding
            .expect("field should be present")
            .expect("binding should replace");
        assert_eq!(binding.mount_id.as_deref(), Some("main"));
        assert!(!binding.bindings[0].required);
    }

    #[test]
    fn probe_request_mapping_preserves_optional_runtime_binding() {
        let input = probe_mcp_transport_input(
            ProbeMcpPresetRequest {
                transport: McpTransportConfigDto::Http {
                    url: "http://127.0.0.1:1/mcp".to_string(),
                    headers: vec![McpHttpHeader {
                        name: "x-demo".to_string(),
                        value: "1".to_string(),
                    }],
                },
                route_policy: McpRoutePolicy::Relay,
                probe_target: None,
                runtime_binding: Some(sample_runtime_binding_dto(true)),
            },
            test_identity(),
        );

        assert_eq!(
            input.transport,
            DomainMcpTransportConfig::Http {
                url: "http://127.0.0.1:1/mcp".to_string(),
                headers: vec![DomainMcpHttpHeader {
                    name: "x-demo".to_string(),
                    value: "1".to_string(),
                }],
            }
        );
        assert_eq!(input.route_policy, DomainMcpRoutePolicy::Relay);
        assert_eq!(input.probe_target, McpProbeTarget::DefaultUserLocal);
        assert_eq!(input.current_user.user_id, "user-1");
        assert!(input.runtime_binding.expect("binding").bindings[0].required);
    }

    fn test_identity() -> agentdash_integration_api::AuthIdentity {
        agentdash_integration_api::AuthIdentity {
            auth_mode: AuthMode::Personal,
            user_id: "user-1".to_string(),
            subject: "user-1".to_string(),
            display_name: None,
            email: None,
            avatar_url: None,
            groups: Vec::new(),
            is_admin: false,
            provider: None,
            extra: serde_json::Value::Null,
        }
    }

    fn sample_runtime_binding_dto(required: bool) -> McpRuntimeBindingConfigDto {
        McpRuntimeBindingConfigDto {
            mount_id: Some("main".to_string()),
            bindings: vec![McpRuntimeBindingRuleDto {
                source: McpRuntimeBindingSourceDto::WorkspaceDetectedFact {
                    path: vec!["p4".to_string(), "client_name".to_string()],
                },
                target: McpRuntimeBindingTargetDto::HttpQuery {
                    name: "p4_client".to_string(),
                },
                required,
            }],
        }
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

        // Internal 保持 500；唯一冲突应由仓储/应用层结构化抛出 Conflict。
        let err: ApiError = McpPresetApplicationError::Internal(
            "duplicate key value violates unique constraint \"idx_mcp_presets_project_key\""
                .to_string(),
        )
        .into();
        assert!(matches!(err, ApiError::Internal(_)));
    }
}
