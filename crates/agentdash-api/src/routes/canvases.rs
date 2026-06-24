use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, Query, State};
use uuid::Uuid;

use agentdash_application::agent_run::RuntimeSurfaceQueryPurpose;
use agentdash_application::canvas::{
    CanvasExtensionPackageInput, CanvasListScopeFilter, CanvasMutationInput,
    CanvasRuntimeBridgeSnapshot, CanvasRuntimeSnapshot, CanvasWithAccess, CopyCanvasInput,
    CreatePersonalCanvasInput, PublishCanvasInput, build_canvas_extension_package,
    build_runtime_snapshot_with_bindings, canvas_vfs_mount_id, copy_canvas_to_personal,
    create_personal_canvas, delete_canvas_record, list_canvases_for_user,
    load_canvas_by_project_mount_id, load_canvas_with_access, publish_canvas_to_project,
    unpublish_project_canvas, update_canvas_record,
};
use agentdash_application::extension_package::{
    ExtensionPackageArtifactUseCaseError, InstallExtensionPackageArtifactInput,
    StoreExtensionPackageArchiveInput, install_extension_package_artifact,
    store_extension_package_archive,
};
use agentdash_application_runtime_gateway::{
    RuntimeActionKey, RuntimeActionKind, RuntimeActor, RuntimeContext, RuntimeInvocationRequest,
    RuntimeInvocationResult, RuntimeSurface,
};
use agentdash_contracts::canvas::{
    CanvasAccessDto, CanvasDataBindingDto, CanvasFileDto, CanvasImportMapDto, CanvasListScopeDto,
    CanvasResponse, CanvasRuntimeBindingDto, CanvasRuntimeBridgeSnapshotDto, CanvasRuntimeFileDto,
    CanvasRuntimeSnapshotDto, CanvasSandboxConfigDto, CanvasScopeDto, CopyCanvasToPersonalRequest,
    CreateCanvasRequest, DeleteCanvasResponse, ListCanvasesQuery, PublishCanvasToProjectRequest,
    RuntimeActionDescriptorDto, RuntimeActionKindDto, RuntimeContextDto,
    RuntimeInvocationOutputDto, RuntimeInvocationResultDto, RuntimePolicyDto, RuntimeSurfaceDto,
    RuntimeTraceDto, UnpublishCanvasResponse, UpdateCanvasRequest,
};
use agentdash_contracts::extension_package::ExtensionPackageInstallationResponse;
use agentdash_domain::canvas::{
    Canvas, CanvasAccessAction, CanvasAccessProjection, CanvasDataBinding, CanvasFile,
    CanvasImportMap, CanvasSandboxConfig, CanvasScope,
};

use crate::agent_run_runtime_surface::{
    ApiCurrentRuntimeSurface, resolve_current_runtime_surface_for_project_for_api,
};
use crate::app_state::AppState;
use crate::auth::{
    CurrentUser, ProjectPermission, load_project_with_permission, project_authorization_context,
};
use crate::dto::{
    CanvasRuntimeInvokeRequest, CanvasRuntimeSnapshotQuery, ListProjectCanvasesPath,
    PromoteCanvasToExtensionRequest,
};
use crate::rpc::ApiError;

pub async fn list_project_canvases(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(path): Path<ListProjectCanvasesPath>,
    Query(query): Query<ListCanvasesQuery>,
) -> Result<Json<Vec<CanvasResponse>>, ApiError> {
    let project_id = parse_project_id(&path.project_id)?;
    let current_user_context = project_authorization_context(&current_user);
    let canvases = list_canvases_for_user(
        &state.repos,
        &current_user_context,
        project_id,
        canvas_list_scope_filter(query),
    )
    .await?;

    Ok(Json(
        canvases
            .into_iter()
            .map(canvas_with_access_to_contract)
            .collect(),
    ))
}

pub fn router() -> axum::Router<std::sync::Arc<crate::app_state::AppState>> {
    axum::Router::new()
        .route(
            "/projects/{project_id}/canvases",
            axum::routing::get(list_project_canvases).post(create_canvas),
        )
        .route(
            "/projects/{project_id}/canvases/by-mount/{canvas_mount_id}",
            axum::routing::get(get_canvas_by_mount),
        )
        .route(
            "/canvases/{id}",
            axum::routing::get(get_canvas)
                .put(update_canvas)
                .delete(delete_canvas),
        )
        .route(
            "/canvases/{id}/runtime-snapshot",
            axum::routing::get(get_canvas_runtime_snapshot),
        )
        .route(
            "/canvases/{id}/runtime-invoke",
            axum::routing::post(invoke_canvas_runtime_action),
        )
        .route(
            "/canvases/{id}/promote-extension",
            axum::routing::post(promote_canvas_to_extension),
        )
        .route(
            "/canvases/{id}/publish-to-project",
            axum::routing::post(publish_canvas_to_project_route),
        )
        .route(
            "/canvases/{id}/copy-to-personal",
            axum::routing::post(copy_canvas_to_personal_route),
        )
        .route(
            "/canvases/{id}/unpublish",
            axum::routing::post(unpublish_canvas_route),
        )
}

pub async fn create_canvas(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(path): Path<ListProjectCanvasesPath>,
    Json(req): Json<CreateCanvasRequest>,
) -> Result<Json<CanvasResponse>, ApiError> {
    let project_id = parse_project_id(&path.project_id)?;
    let current_user_context = project_authorization_context(&current_user);

    let canvas = create_personal_canvas(
        &state.repos,
        &current_user_context,
        CreatePersonalCanvasInput {
            project_id,
            mount_id: req.canvas_mount_id,
            title: req.title,
            description: req.description,
            mutation: CanvasMutationInput {
                entry_file: req.entry_file,
                sandbox_config: req.sandbox_config.map(sandbox_config_from_contract),
                files: req
                    .files
                    .map(|files| files.into_iter().map(canvas_file_from_contract).collect()),
                bindings: req.bindings.map(|bindings| {
                    bindings
                        .into_iter()
                        .map(canvas_data_binding_from_contract)
                        .collect()
                }),
                ..CanvasMutationInput::default()
            },
        },
    )
    .await?;

    Ok(Json(canvas_with_access_to_contract(canvas)))
}

pub async fn get_canvas(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(id): Path<String>,
) -> Result<Json<CanvasResponse>, ApiError> {
    let canvas =
        load_canvas_for_action(state.as_ref(), &current_user, &id, CanvasAccessAction::View)
            .await?;

    Ok(Json(canvas_with_access_to_contract(canvas)))
}

pub async fn get_canvas_by_mount(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path((project_id, canvas_mount_id)): Path<(String, String)>,
) -> Result<Json<CanvasResponse>, ApiError> {
    let project_id = parse_project_id(&project_id)?;
    load_project_with_permission(
        state.as_ref(),
        &current_user,
        project_id,
        ProjectPermission::View,
    )
    .await?;
    let canvas =
        load_canvas_by_project_mount_id(&state.repos, project_id, &canvas_mount_id).await?;
    let canvas = load_canvas_for_action_by_id(
        state.as_ref(),
        &current_user,
        canvas.id,
        CanvasAccessAction::View,
    )
    .await?;

    Ok(Json(canvas_with_access_to_contract(canvas)))
}

pub async fn update_canvas(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(id): Path<String>,
    Json(req): Json<UpdateCanvasRequest>,
) -> Result<Json<CanvasResponse>, ApiError> {
    let CanvasWithAccess { canvas, access } = load_canvas_for_action(
        state.as_ref(),
        &current_user,
        &id,
        CanvasAccessAction::EditSource,
    )
    .await?;

    let canvas = update_canvas_record(
        &state.repos,
        canvas,
        CanvasMutationInput {
            title: req.title,
            description: req.description,
            entry_file: req.entry_file,
            sandbox_config: req.sandbox_config.map(sandbox_config_from_contract),
            files: req
                .files
                .map(|files| files.into_iter().map(canvas_file_from_contract).collect()),
            bindings: req.bindings.map(|bindings| {
                bindings
                    .into_iter()
                    .map(canvas_data_binding_from_contract)
                    .collect()
            }),
        },
    )
    .await?;

    Ok(Json(canvas_to_contract(canvas, access)))
}

pub async fn delete_canvas(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(id): Path<String>,
) -> Result<Json<DeleteCanvasResponse>, ApiError> {
    let canvas =
        load_canvas_for_action(state.as_ref(), &current_user, &id, CanvasAccessAction::View)
            .await?;
    match canvas_delete_plan(&canvas)? {
        CanvasDeletePlan::DeletePersonal => {
            delete_canvas_record(&state.repos, &canvas.canvas).await?;
        }
        CanvasDeletePlan::UnpublishShared => {
            unpublish_project_canvas(
                &state.repos,
                &project_authorization_context(&current_user),
                canvas.canvas.id,
            )
            .await?;
        }
    }

    Ok(Json(DeleteCanvasResponse { deleted: id }))
}

fn canvas_with_access_to_contract(value: CanvasWithAccess) -> CanvasResponse {
    canvas_to_contract(value.canvas, value.access)
}

fn canvas_to_contract(canvas: Canvas, access: CanvasAccessProjection) -> CanvasResponse {
    let vfs_mount_id = canvas_vfs_mount_id(&canvas);
    CanvasResponse {
        canvas_id: canvas.id.to_string(),
        project_id: canvas.project_id.to_string(),
        owner_user_id: canvas.owner_user_id,
        scope: canvas_scope_to_contract(canvas.scope),
        access: canvas_access_to_contract(access),
        canvas_mount_id: canvas.mount_id,
        vfs_mount_id,
        title: canvas.title,
        description: canvas.description,
        entry_file: canvas.entry_file,
        sandbox_config: sandbox_config_to_contract(canvas.sandbox_config),
        files: canvas
            .files
            .into_iter()
            .map(canvas_file_to_contract)
            .collect(),
        bindings: canvas
            .bindings
            .into_iter()
            .map(canvas_data_binding_to_contract)
            .collect(),
        published_from_canvas_id: canvas.published_from_canvas_id.map(|id| id.to_string()),
        shared_canvas_id: canvas.shared_canvas_id.map(|id| id.to_string()),
        cloned_from_canvas_id: canvas.cloned_from_canvas_id.map(|id| id.to_string()),
        published_at: canvas.published_at.map(|value| value.to_rfc3339()),
        published_by_user_id: canvas.published_by_user_id,
        created_at: canvas.created_at.to_rfc3339(),
        updated_at: canvas.updated_at.to_rfc3339(),
    }
}

fn canvas_scope_to_contract(scope: CanvasScope) -> CanvasScopeDto {
    match scope {
        CanvasScope::Personal => CanvasScopeDto::Personal,
        CanvasScope::Project => CanvasScopeDto::Project,
    }
}

fn canvas_access_to_contract(access: CanvasAccessProjection) -> CanvasAccessDto {
    CanvasAccessDto {
        can_view: access.can_view,
        can_edit_source: access.can_edit_source,
        can_publish: access.can_publish,
        can_manage_shared: access.can_manage_shared,
        can_copy: access.can_copy,
        runtime_write_allowed: access.runtime_write_allowed,
    }
}

fn canvas_list_scope_filter(query: ListCanvasesQuery) -> CanvasListScopeFilter {
    match query.scope.unwrap_or(CanvasListScopeDto::All) {
        CanvasListScopeDto::All => CanvasListScopeFilter::All,
        CanvasListScopeDto::Mine => CanvasListScopeFilter::Mine,
        CanvasListScopeDto::Shared => CanvasListScopeFilter::Shared,
    }
}

fn sandbox_config_to_contract(config: CanvasSandboxConfig) -> CanvasSandboxConfigDto {
    CanvasSandboxConfigDto {
        libraries: config.libraries,
        import_map: CanvasImportMapDto {
            imports: config.import_map.imports,
        },
    }
}

fn sandbox_config_from_contract(config: CanvasSandboxConfigDto) -> CanvasSandboxConfig {
    CanvasSandboxConfig {
        libraries: config.libraries,
        import_map: CanvasImportMap {
            imports: config.import_map.imports,
        },
    }
}

fn canvas_file_to_contract(file: CanvasFile) -> CanvasFileDto {
    CanvasFileDto {
        path: file.path,
        content: file.content,
    }
}

fn canvas_file_from_contract(file: CanvasFileDto) -> CanvasFile {
    CanvasFile {
        path: file.path,
        content: file.content,
    }
}

fn canvas_data_binding_to_contract(binding: CanvasDataBinding) -> CanvasDataBindingDto {
    CanvasDataBindingDto {
        alias: binding.alias,
        source_uri: binding.source_uri,
        content_type: binding.content_type,
    }
}

fn canvas_data_binding_from_contract(binding: CanvasDataBindingDto) -> CanvasDataBinding {
    CanvasDataBinding::with_content_type(
        binding.alias,
        binding.source_uri,
        Some(binding.content_type),
    )
}

pub async fn promote_canvas_to_extension(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(id): Path<String>,
    Json(req): Json<PromoteCanvasToExtensionRequest>,
) -> Result<Json<ExtensionPackageInstallationResponse>, ApiError> {
    let CanvasWithAccess { canvas, .. } = load_canvas_for_action(
        state.as_ref(),
        &current_user,
        &id,
        CanvasAccessAction::Publish,
    )
    .await?;
    let package = build_canvas_extension_package(
        &canvas,
        CanvasExtensionPackageInput {
            package_version: req.package_version,
            asset_version: req.asset_version,
        },
    )?;
    let artifact = store_extension_package_archive(
        &state.repos,
        state.services.extension_package_artifact_storage.as_ref(),
        StoreExtensionPackageArchiveInput {
            project_id: canvas.project_id,
            archive_bytes: package.archive_bytes,
            expected_archive_digest: Some(package.archive_digest),
        },
    )
    .await
    .map_err(extension_package_error_to_api)?;
    let installation = install_extension_package_artifact(
        &state.repos,
        InstallExtensionPackageArtifactInput {
            project_id: canvas.project_id,
            artifact_id: artifact.id,
            extension_key: req.extension_key,
            display_name: req
                .display_name
                .or_else(|| Some(canvas.title.trim().to_string())),
            overwrite: req.overwrite,
        },
    )
    .await?;
    let artifact = installation.package_artifact.ok_or_else(|| {
        ApiError::Internal("Canvas promoted extension installation 缺少 artifact 引用".into())
    })?;

    Ok(Json(ExtensionPackageInstallationResponse {
        installation_id: installation.id.to_string(),
        extension_key: installation.extension_key,
        extension_id: installation.manifest.extension_id,
        package_artifact_id: artifact.artifact_id.to_string(),
        archive_digest: artifact.archive_digest,
    }))
}

pub async fn publish_canvas_to_project_route(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(id): Path<String>,
    Json(req): Json<PublishCanvasToProjectRequest>,
) -> Result<Json<CanvasResponse>, ApiError> {
    let canvas_id = parse_canvas_id(&id)?;
    let current_user_context = project_authorization_context(&current_user);
    let canvas = publish_canvas_to_project(
        &state.repos,
        &current_user_context,
        canvas_id,
        PublishCanvasInput {
            mount_id: req.canvas_mount_id,
            title: req.title,
            description: req.description,
        },
    )
    .await?;

    Ok(Json(canvas_with_access_to_contract(canvas)))
}

pub async fn copy_canvas_to_personal_route(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(id): Path<String>,
    Json(req): Json<CopyCanvasToPersonalRequest>,
) -> Result<Json<CanvasResponse>, ApiError> {
    let canvas_id = parse_canvas_id(&id)?;
    let current_user_context = project_authorization_context(&current_user);
    let canvas = copy_canvas_to_personal(
        &state.repos,
        &current_user_context,
        canvas_id,
        CopyCanvasInput {
            mount_id: req.canvas_mount_id,
            title: req.title,
            description: req.description,
        },
    )
    .await?;

    Ok(Json(canvas_with_access_to_contract(canvas)))
}

pub async fn unpublish_canvas_route(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(id): Path<String>,
) -> Result<Json<UnpublishCanvasResponse>, ApiError> {
    let canvas_id = parse_canvas_id(&id)?;
    let current_user_context = project_authorization_context(&current_user);
    let result = unpublish_project_canvas(&state.repos, &current_user_context, canvas_id).await?;

    Ok(Json(UnpublishCanvasResponse {
        unpublished_canvas_id: result.unpublished_canvas_id.to_string(),
        source_canvas_id: result.source_canvas_id.map(|id| id.to_string()),
    }))
}

pub async fn get_canvas_runtime_snapshot(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(id): Path<String>,
    Query(query): Query<CanvasRuntimeSnapshotQuery>,
) -> Result<Json<CanvasRuntimeSnapshotDto>, ApiError> {
    let CanvasWithAccess { canvas, .. } =
        load_canvas_for_action(state.as_ref(), &current_user, &id, CanvasAccessAction::View)
            .await?;

    let runtime_surface =
        resolve_canvas_runtime_surface(&state, &current_user, &canvas, query.session_id.as_deref())
            .await?;
    let mut snapshot = build_runtime_snapshot_with_bindings(
        &canvas,
        query.session_id.clone(),
        runtime_surface.as_ref().map(|surface| &surface.vfs),
        state.services.vfs_service.as_ref(),
    )
    .await;
    if let Some(session_id) = query.session_id.as_deref() {
        snapshot.runtime_bridge =
            build_canvas_runtime_bridge_surface(state.as_ref(), &canvas, session_id)?;
    }

    Ok(Json(canvas_runtime_snapshot_to_contract(snapshot)))
}

pub async fn invoke_canvas_runtime_action(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    Path(id): Path<String>,
    Json(req): Json<CanvasRuntimeInvokeRequest>,
) -> Result<Json<RuntimeInvocationResultDto>, ApiError> {
    let CanvasWithAccess { canvas, .. } =
        load_canvas_for_action(state.as_ref(), &current_user, &id, CanvasAccessAction::View)
            .await?;
    let session_id = req.session_id.trim();
    if session_id.is_empty() {
        return Err(ApiError::BadRequest(
            "Canvas runtime invoke 缺少 session_id".to_string(),
        ));
    }
    resolve_current_runtime_surface_for_project_for_api(
        &state,
        &current_user,
        session_id,
        canvas.project_id,
        RuntimeSurfaceQueryPurpose::new("canvas_runtime_invoke"),
        "Canvas runtime invoke",
    )
    .await?;

    let action_key = RuntimeActionKey::parse(req.action_key)
        .map_err(|error| ApiError::BadRequest(error.to_string()))?;
    let request = RuntimeInvocationRequest::new(
        action_key,
        RuntimeActor::UserCanvas {
            session_id: session_id.to_string(),
            canvas_id: Some(canvas.id),
        },
        RuntimeContext::Session {
            session_id: session_id.to_string(),
            project_id: Some(canvas.project_id),
            workspace_id: None,
        },
        req.input,
    );

    let result = state.services.runtime_gateway.invoke(request).await?;
    Ok(Json(runtime_invocation_result_to_contract(result)))
}

fn canvas_runtime_snapshot_to_contract(
    snapshot: CanvasRuntimeSnapshot,
) -> CanvasRuntimeSnapshotDto {
    CanvasRuntimeSnapshotDto {
        canvas_id: snapshot.canvas_id.to_string(),
        canvas_mount_id: snapshot.canvas_mount_id,
        vfs_mount_id: snapshot.vfs_mount_id,
        session_id: snapshot.session_id,
        resource_surface_ref: snapshot.resource_surface_ref,
        entry: snapshot.entry,
        files: snapshot
            .files
            .into_iter()
            .map(|file| CanvasRuntimeFileDto {
                path: file.path,
                content: file.content,
                file_type: file.file_type,
            })
            .collect(),
        bindings: snapshot
            .bindings
            .into_iter()
            .map(|binding| CanvasRuntimeBindingDto {
                alias: binding.alias,
                source_uri: binding.source_uri,
                data_path: binding.data_path,
                content_type: binding.content_type,
                resolved: binding.resolved,
            })
            .collect(),
        import_map: CanvasImportMapDto {
            imports: snapshot.import_map.imports,
        },
        libraries: snapshot.libraries,
        runtime_bridge: canvas_runtime_bridge_to_contract(snapshot.runtime_bridge),
    }
}

fn canvas_runtime_bridge_to_contract(
    bridge: CanvasRuntimeBridgeSnapshot,
) -> CanvasRuntimeBridgeSnapshotDto {
    CanvasRuntimeBridgeSnapshotDto {
        enabled: bridge.enabled,
        surface: bridge.surface.map(runtime_surface_to_contract),
        disabled_reason: bridge.disabled_reason,
    }
}

fn runtime_invocation_result_to_contract(
    result: RuntimeInvocationResult,
) -> RuntimeInvocationResultDto {
    RuntimeInvocationResultDto {
        action_key: result.action_key.to_string(),
        trace: RuntimeTraceDto {
            trace_id: result.trace.trace_id,
            invocation_id: result.trace.invocation_id,
            parent_trace_id: result.trace.parent_trace_id,
            created_at: result.trace.created_at.to_rfc3339(),
        },
        output: RuntimeInvocationOutputDto {
            output: result.output.output,
            metadata: result.output.metadata,
        },
    }
}

fn runtime_surface_to_contract(surface: RuntimeSurface) -> RuntimeSurfaceDto {
    RuntimeSurfaceDto {
        context: runtime_context_to_contract(surface.context),
        actions: surface
            .actions
            .into_iter()
            .map(|action| RuntimeActionDescriptorDto {
                action_key: action.action_key.to_string(),
                kind: runtime_action_kind_to_contract(action.kind),
                description: action.description,
                input_schema: action.input_schema,
                output_schema: action.output_schema,
                default_policy: RuntimePolicyDto {
                    required_capabilities: action.default_policy.required_capabilities,
                    timeout_ms: action.default_policy.timeout_ms.map(|value| value as i64),
                    allow_background: action.default_policy.allow_background,
                },
            })
            .collect(),
    }
}

fn runtime_context_to_contract(context: RuntimeContext) -> RuntimeContextDto {
    match context {
        RuntimeContext::Session {
            session_id,
            project_id,
            workspace_id,
        } => RuntimeContextDto::Session {
            session_id,
            project_id: project_id.map(|id| id.to_string()),
            workspace_id: workspace_id.map(|id| id.to_string()),
        },
        RuntimeContext::Setup {
            project_id,
            workspace_id,
            backend_id,
            root_ref,
        } => RuntimeContextDto::Setup {
            project_id: project_id.map(|id| id.to_string()),
            workspace_id: workspace_id.map(|id| id.to_string()),
            backend_id,
            root_ref,
        },
    }
}

fn runtime_action_kind_to_contract(kind: RuntimeActionKind) -> RuntimeActionKindDto {
    match kind {
        RuntimeActionKind::SessionRuntime => RuntimeActionKindDto::SessionRuntime,
        RuntimeActionKind::Setup => RuntimeActionKindDto::Setup,
    }
}

fn build_canvas_runtime_bridge_surface(
    state: &AppState,
    canvas: &agentdash_domain::canvas::Canvas,
    session_id: &str,
) -> Result<CanvasRuntimeBridgeSnapshot, ApiError> {
    let surface = state.services.runtime_gateway.surface_for_actor(
        RuntimeActor::UserCanvas {
            session_id: session_id.to_string(),
            canvas_id: Some(canvas.id),
        },
        RuntimeContext::Session {
            session_id: session_id.to_string(),
            project_id: Some(canvas.project_id),
            workspace_id: None,
        },
    )?;

    Ok(CanvasRuntimeBridgeSnapshot::enabled(surface))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CanvasDeletePlan {
    DeletePersonal,
    UnpublishShared,
}

fn canvas_delete_plan(value: &CanvasWithAccess) -> Result<CanvasDeletePlan, ApiError> {
    match value.canvas.scope {
        CanvasScope::Personal if value.access.can_edit_source => {
            Ok(CanvasDeletePlan::DeletePersonal)
        }
        CanvasScope::Personal => Err(ApiError::Forbidden(format!(
            "当前用户无权删除个人 Canvas {}",
            value.canvas.id
        ))),
        CanvasScope::Project if value.access.can_manage_shared => {
            Ok(CanvasDeletePlan::UnpublishShared)
        }
        CanvasScope::Project => Err(ApiError::Forbidden(format!(
            "当前用户无权删除项目共用 Canvas {}",
            value.canvas.id
        ))),
    }
}

async fn load_canvas_for_action(
    state: &AppState,
    current_user: &agentdash_integration_api::AuthIdentity,
    raw_canvas_id: &str,
    action: CanvasAccessAction,
) -> Result<CanvasWithAccess, ApiError> {
    let canvas_id = parse_canvas_id(raw_canvas_id)?;
    load_canvas_for_action_by_id(state, current_user, canvas_id, action).await
}

async fn load_canvas_for_action_by_id(
    state: &AppState,
    current_user: &agentdash_integration_api::AuthIdentity,
    canvas_id: Uuid,
    action: CanvasAccessAction,
) -> Result<CanvasWithAccess, ApiError> {
    let current_user_context = project_authorization_context(current_user);
    load_canvas_with_access(&state.repos, &current_user_context, canvas_id, action)
        .await
        .map_err(ApiError::from)
}

fn parse_canvas_id(raw_canvas_id: &str) -> Result<Uuid, ApiError> {
    Uuid::parse_str(raw_canvas_id)
        .map_err(|_| ApiError::BadRequest("Canvas route 只接受 canvas_id UUID".into()))
}

fn parse_project_id(raw_project_id: &str) -> Result<Uuid, ApiError> {
    Uuid::parse_str(raw_project_id).map_err(|_| ApiError::BadRequest("无效的 Project ID".into()))
}

fn extension_package_error_to_api(error: ExtensionPackageArtifactUseCaseError) -> ApiError {
    match error {
        ExtensionPackageArtifactUseCaseError::Domain(error) => ApiError::from(error),
        ExtensionPackageArtifactUseCaseError::Storage(error) => {
            tracing::error!(error = %error, "extension package artifact storage error");
            ApiError::Internal(String::from("扩展包存储错误"))
        }
        ExtensionPackageArtifactUseCaseError::BadRequest(error) => ApiError::BadRequest(error),
        ExtensionPackageArtifactUseCaseError::NotFound(error) => ApiError::NotFound(error),
        ExtensionPackageArtifactUseCaseError::Forbidden(error) => ApiError::Forbidden(error),
        ExtensionPackageArtifactUseCaseError::Conflict(error) => ApiError::Conflict(error),
        ExtensionPackageArtifactUseCaseError::Integrity(error) => ApiError::Internal(error),
    }
}

async fn resolve_canvas_runtime_surface(
    state: &Arc<AppState>,
    current_user: &agentdash_integration_api::AuthIdentity,
    canvas: &agentdash_domain::canvas::Canvas,
    session_id: Option<&str>,
) -> Result<Option<ApiCurrentRuntimeSurface>, ApiError> {
    let Some(session_id) = session_id else {
        return Ok(None);
    };

    Ok(Some(
        resolve_current_runtime_surface_for_project_for_api(
            state,
            current_user,
            session_id,
            canvas.project_id,
            RuntimeSurfaceQueryPurpose::new("canvas_runtime_snapshot"),
            "Canvas runtime bridge manifest",
        )
        .await?,
    ))
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};
    use serde_json::json;

    use super::*;

    #[test]
    fn canvas_response_maps_scope_access_and_lineage() {
        let project_id = Uuid::new_v4();
        let source_id = Uuid::new_v4();
        let shared_id = Uuid::new_v4();
        let clone_id = Uuid::new_v4();
        let published_at = Utc
            .with_ymd_and_hms(2026, 6, 24, 9, 30, 15)
            .single()
            .expect("valid timestamp");
        let mut canvas = Canvas::new_project_shared(
            project_id,
            "cvs-shared".to_string(),
            "Shared".to_string(),
            "Team canvas".to_string(),
            Some(source_id),
            Some("alice".to_string()),
        );
        canvas.id = shared_id;
        canvas.shared_canvas_id = Some(shared_id);
        canvas.cloned_from_canvas_id = Some(clone_id);
        canvas.published_at = Some(published_at);

        let response = canvas_to_contract(
            canvas,
            CanvasAccessProjection {
                can_view: true,
                can_edit_source: false,
                can_publish: true,
                can_manage_shared: true,
                can_copy: true,
                runtime_write_allowed: false,
            },
        );

        assert_eq!(response.canvas_id, shared_id.to_string());
        assert_eq!(response.project_id, project_id.to_string());
        assert_eq!(response.owner_user_id.as_deref(), Some("alice"));
        assert_eq!(response.scope, CanvasScopeDto::Project);
        assert!(response.access.can_view);
        assert!(!response.access.can_edit_source);
        assert!(response.access.can_publish);
        assert!(response.access.can_manage_shared);
        assert!(response.access.can_copy);
        assert!(!response.access.runtime_write_allowed);
        assert_eq!(
            response.published_from_canvas_id.as_deref(),
            Some(source_id.to_string().as_str())
        );
        assert_eq!(
            response.shared_canvas_id.as_deref(),
            Some(shared_id.to_string().as_str())
        );
        assert_eq!(
            response.cloned_from_canvas_id.as_deref(),
            Some(clone_id.to_string().as_str())
        );
        assert_eq!(
            response.published_at.as_deref(),
            Some("2026-06-24T09:30:15+00:00")
        );
        assert_eq!(response.published_by_user_id.as_deref(), Some("alice"));
    }

    #[test]
    fn canvas_list_scope_query_defaults_to_all_and_maps_variants() {
        assert_eq!(
            canvas_list_scope_filter(ListCanvasesQuery::default()),
            CanvasListScopeFilter::All
        );
        assert_eq!(
            canvas_list_scope_filter(ListCanvasesQuery {
                scope: Some(CanvasListScopeDto::Mine)
            }),
            CanvasListScopeFilter::Mine
        );
        assert_eq!(
            canvas_list_scope_filter(ListCanvasesQuery {
                scope: Some(CanvasListScopeDto::Shared)
            }),
            CanvasListScopeFilter::Shared
        );
        assert!(
            serde_json::from_value::<ListCanvasesQuery>(json!({ "scope": "invalid" })).is_err()
        );
    }

    #[test]
    fn canvas_delete_plan_allows_personal_owner_only_for_personal_source() {
        let value = canvas_with_access(
            CanvasScope::Personal,
            CanvasAccessProjection {
                can_view: true,
                can_edit_source: true,
                ..CanvasAccessProjection::default()
            },
        );

        assert_eq!(
            canvas_delete_plan(&value).expect("owner can delete personal"),
            CanvasDeletePlan::DeletePersonal
        );

        let value = canvas_with_access(
            CanvasScope::Personal,
            CanvasAccessProjection {
                can_view: true,
                can_edit_source: false,
                ..CanvasAccessProjection::default()
            },
        );

        assert!(matches!(
            canvas_delete_plan(&value),
            Err(ApiError::Forbidden(_))
        ));
    }

    #[test]
    fn canvas_delete_plan_uses_unpublish_for_project_shared_managers() {
        let value = canvas_with_access(
            CanvasScope::Project,
            CanvasAccessProjection {
                can_view: true,
                can_manage_shared: true,
                ..CanvasAccessProjection::default()
            },
        );

        assert_eq!(
            canvas_delete_plan(&value).expect("manager can unpublish shared"),
            CanvasDeletePlan::UnpublishShared
        );

        let value = canvas_with_access(
            CanvasScope::Project,
            CanvasAccessProjection {
                can_view: true,
                can_manage_shared: false,
                ..CanvasAccessProjection::default()
            },
        );

        assert!(matches!(
            canvas_delete_plan(&value),
            Err(ApiError::Forbidden(_))
        ));
    }

    fn canvas_with_access(scope: CanvasScope, access: CanvasAccessProjection) -> CanvasWithAccess {
        let project_id = Uuid::new_v4();
        let canvas = match scope {
            CanvasScope::Personal => Canvas::new_personal(
                project_id,
                "alice".to_string(),
                "cvs-personal".to_string(),
                "Personal".to_string(),
                String::new(),
            ),
            CanvasScope::Project => Canvas::new_project_shared(
                project_id,
                "cvs-shared".to_string(),
                "Shared".to_string(),
                String::new(),
                None,
                Some("alice".to_string()),
            ),
        };
        CanvasWithAccess { canvas, access }
    }
}
