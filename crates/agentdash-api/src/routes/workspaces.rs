use std::sync::Arc;

use axum::Json;
use axum::extract::{Path as AxumPath, State};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use uuid::Uuid;

use agentdash_application::backend_transport::BackendTransport;
use agentdash_domain::common::MountCapability;
use agentdash_domain::workspace::{
    Workspace, WorkspaceBinding, WorkspaceBindingStatus, WorkspaceIdentityKind,
    WorkspaceResolutionPolicy, WorkspaceStatus, identity_payload_matches,
    normalize_identity_payload,
};

use crate::app_state::AppState;
use crate::auth::{
    CurrentUser, ProjectPermission, load_project_with_permission,
    load_workspace_and_project_with_permission,
};
use crate::dto::{WorkspaceBindingResponse, WorkspaceResponse};
use crate::rpc::ApiError;
use crate::workspace_resolution::detect_workspace_from_backend;

#[derive(Debug, Clone, Deserialize)]
pub struct WorkspaceBindingInput {
    pub id: Option<Uuid>,
    pub backend_id: String,
    pub root_ref: String,
    pub status: Option<WorkspaceBindingStatus>,
    pub detected_facts: Option<Value>,
    pub priority: Option<i32>,
}

#[derive(Debug, Deserialize)]
pub struct CreateWorkspaceRequest {
    pub name: String,
    pub identity_kind: Option<WorkspaceIdentityKind>,
    pub identity_payload: Option<Value>,
    pub resolution_policy: Option<WorkspaceResolutionPolicy>,
    pub default_binding_id: Option<Uuid>,
    pub bindings: Option<Vec<WorkspaceBindingInput>>,
    pub shortcut_binding: Option<WorkspaceBindingInput>,
    pub mount_capabilities: Option<Vec<MountCapability>>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateWorkspaceRequest {
    pub name: Option<String>,
    pub identity_kind: Option<WorkspaceIdentityKind>,
    pub identity_payload: Option<Value>,
    pub resolution_policy: Option<WorkspaceResolutionPolicy>,
    pub default_binding_id: Option<Uuid>,
    pub bindings: Option<Vec<WorkspaceBindingInput>>,
    pub mount_capabilities: Option<Vec<MountCapability>>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateWorkspaceStatusRequest {
    pub status: WorkspaceStatus,
}

#[derive(Debug, Deserialize)]
pub struct DetectWorkspaceRequest {
    pub backend_id: String,
    pub root_ref: String,
}

#[derive(Debug, Serialize)]
pub struct DetectWorkspaceResponse {
    pub identity_kind: WorkspaceIdentityKind,
    pub identity_payload: Value,
    pub binding: WorkspaceBindingResponse,
    pub confidence: String,
    pub warnings: Vec<String>,
    pub matched_workspace_ids: Vec<Uuid>,
}

#[derive(Debug, Deserialize)]
pub struct DetectGitRequest {
    pub root_ref: String,
    pub backend_id: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct DetectGitResponse {
    pub resolved_root_ref: String,
    pub is_git_repo: bool,
    pub source_repo: Option<String>,
    pub branch: Option<String>,
    pub commit_hash: Option<String>,
}

#[derive(Debug)]
pub(crate) struct GitDetectionResult {
    pub(crate) is_git_repo: bool,
    pub(crate) source_repo: Option<String>,
    pub(crate) branch: Option<String>,
    pub(crate) commit_hash: Option<String>,
}

pub async fn list_workspaces(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    AxumPath(project_id): AxumPath<String>,
) -> Result<Json<Vec<WorkspaceResponse>>, ApiError> {
    let project_id = parse_project_id(&project_id)?;
    load_project_with_permission(
        state.as_ref(),
        &current_user,
        project_id,
        ProjectPermission::View,
    )
    .await?;

    let workspaces = state
        .repos
        .workspace_repo
        .list_by_project(project_id)
        .await?;
    Ok(Json(
        workspaces
            .into_iter()
            .map(WorkspaceResponse::from)
            .collect(),
    ))
}

pub async fn create_workspace(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    AxumPath(project_id): AxumPath<String>,
    Json(req): Json<CreateWorkspaceRequest>,
) -> Result<Json<WorkspaceResponse>, ApiError> {
    let project_id = parse_project_id(&project_id)?;
    load_project_with_permission(
        state.as_ref(),
        &current_user,
        project_id,
        ProjectPermission::Edit,
    )
    .await?;

    let workspace_name = normalize_workspace_name(&req.name)?;
    let (identity_kind, identity_payload, initial_bindings) = derive_workspace_shape(
        &state,
        req.identity_kind,
        req.identity_payload,
        req.bindings,
        req.shortcut_binding,
    )
    .await?;

    let mut workspace = Workspace::new(
        project_id,
        workspace_name,
        identity_kind,
        identity_payload,
        req.resolution_policy
            .unwrap_or(WorkspaceResolutionPolicy::PreferOnline),
    );
    workspace.set_bindings(initial_bindings);
    workspace.default_binding_id = req.default_binding_id.or(workspace.default_binding_id);
    workspace.mount_capabilities = req.mount_capabilities.unwrap_or_default();
    workspace.status = derive_workspace_status(&workspace.bindings);
    workspace.refresh_default_binding();

    state.repos.workspace_repo.create(&workspace).await?;
    let stored = state
        .repos
        .workspace_repo
        .get_by_id(workspace.id)
        .await?
        .ok_or_else(|| ApiError::Internal("Workspace 创建后读取失败".into()))?;
    Ok(Json(WorkspaceResponse::from(stored)))
}

pub async fn get_workspace(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    AxumPath(id): AxumPath<String>,
) -> Result<Json<WorkspaceResponse>, ApiError> {
    let workspace_id = parse_workspace_id(&id)?;
    let (workspace, _) = load_workspace_and_project_with_permission(
        state.as_ref(),
        &current_user,
        workspace_id,
        ProjectPermission::View,
    )
    .await?;

    Ok(Json(WorkspaceResponse::from(workspace)))
}

pub async fn update_workspace(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    AxumPath(id): AxumPath<String>,
    Json(req): Json<UpdateWorkspaceRequest>,
) -> Result<Json<WorkspaceResponse>, ApiError> {
    let workspace_id = parse_workspace_id(&id)?;
    let (mut workspace, _) = load_workspace_and_project_with_permission(
        state.as_ref(),
        &current_user,
        workspace_id,
        ProjectPermission::Edit,
    )
    .await?;

    if let Some(name) = req.name {
        workspace.name = normalize_workspace_name(&name)?;
    }
    if let Some(identity_kind) = req.identity_kind {
        workspace.identity_kind = identity_kind;
    }
    if let Some(identity_payload) = req.identity_payload {
        workspace.identity_payload = normalize_workspace_identity_payload(
            workspace.identity_kind.clone(),
            identity_payload,
        )?;
    }
    if let Some(resolution_policy) = req.resolution_policy {
        workspace.resolution_policy = resolution_policy;
    }
    if let Some(bindings) = req.bindings {
        let next_bindings = bindings
            .into_iter()
            .map(|binding| binding_input_to_binding(workspace.id, binding))
            .collect::<Result<Vec<_>, _>>()?;
        workspace.set_bindings(next_bindings);
    }
    if let Some(default_binding_id) = req.default_binding_id {
        workspace.default_binding_id = Some(default_binding_id);
    }
    if let Some(mount_capabilities) = req.mount_capabilities {
        workspace.mount_capabilities = mount_capabilities;
    }
    workspace.status = derive_workspace_status(&workspace.bindings);
    workspace.refresh_default_binding();

    state.repos.workspace_repo.update(&workspace).await?;
    let stored = state
        .repos
        .workspace_repo
        .get_by_id(workspace.id)
        .await?
        .ok_or_else(|| ApiError::Internal("Workspace 更新后读取失败".into()))?;
    Ok(Json(WorkspaceResponse::from(stored)))
}

pub async fn update_workspace_status(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    AxumPath(id): AxumPath<String>,
    Json(req): Json<UpdateWorkspaceStatusRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let workspace_id = parse_workspace_id(&id)?;
    let (mut workspace, _) = load_workspace_and_project_with_permission(
        state.as_ref(),
        &current_user,
        workspace_id,
        ProjectPermission::Edit,
    )
    .await?;
    workspace.status = req.status;
    workspace.refresh_default_binding();
    state.repos.workspace_repo.update(&workspace).await?;

    Ok(Json(serde_json::json!({ "updated": id })))
}

pub async fn delete_workspace(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    AxumPath(id): AxumPath<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let workspace_id = parse_workspace_id(&id)?;
    load_workspace_and_project_with_permission(
        state.as_ref(),
        &current_user,
        workspace_id,
        ProjectPermission::Edit,
    )
    .await?;

    state.repos.workspace_repo.delete(workspace_id).await?;
    Ok(Json(serde_json::json!({ "deleted": id })))
}

pub async fn detect_workspace(
    State(state): State<Arc<AppState>>,
    CurrentUser(current_user): CurrentUser,
    AxumPath(project_id): AxumPath<String>,
    Json(req): Json<DetectWorkspaceRequest>,
) -> Result<Json<DetectWorkspaceResponse>, ApiError> {
    let project_id = parse_project_id(&project_id)?;
    load_project_with_permission(
        state.as_ref(),
        &current_user,
        project_id,
        ProjectPermission::Edit,
    )
    .await?;

    let detected = detect_workspace_from_backend(&state, &req.backend_id, &req.root_ref).await?;
    let existing = state
        .repos
        .workspace_repo
        .list_by_project(project_id)
        .await?;
    let matched_workspace_ids = existing
        .into_iter()
        .filter(|workspace| {
            workspace.identity_kind == detected.identity_kind
                && identity_payload_matches(
                    workspace.identity_kind.clone(),
                    &workspace.identity_payload,
                    &detected.identity_payload,
                    Some(&detected.binding.detected_facts),
                )
        })
        .map(|workspace| workspace.id)
        .collect();

    Ok(Json(DetectWorkspaceResponse {
        identity_kind: detected.identity_kind,
        identity_payload: detected.identity_payload,
        binding: WorkspaceBindingResponse::from(detected.binding),
        confidence: detected.confidence,
        warnings: detected.warnings,
        matched_workspace_ids,
    }))
}

pub async fn detect_git(
    State(state): State<Arc<AppState>>,
    Json(req): Json<DetectGitRequest>,
) -> Result<Json<DetectGitResponse>, ApiError> {
    let root_ref = req.root_ref.trim();
    if root_ref.is_empty() {
        return Err(ApiError::BadRequest("root_ref 不能为空".into()));
    }

    let backend_id = require_backend_id(req.backend_id.as_deref())?;
    let result = detect_git_via_backend(&state, backend_id, root_ref).await?;

    Ok(Json(DetectGitResponse {
        resolved_root_ref: root_ref.to_string(),
        is_git_repo: result.is_git_repo,
        source_repo: result.source_repo,
        branch: result.branch,
        commit_hash: result.commit_hash,
    }))
}

async fn derive_workspace_shape(
    state: &Arc<AppState>,
    identity_kind: Option<WorkspaceIdentityKind>,
    identity_payload: Option<Value>,
    bindings: Option<Vec<WorkspaceBindingInput>>,
    shortcut_binding: Option<WorkspaceBindingInput>,
) -> Result<(WorkspaceIdentityKind, Value, Vec<WorkspaceBinding>), ApiError> {
    let raw_bindings = if let Some(bindings) = bindings {
        bindings
    } else if let Some(shortcut_binding) = shortcut_binding {
        vec![shortcut_binding]
    } else {
        Vec::new()
    };

    let mut parsed_bindings = raw_bindings
        .into_iter()
        .map(|binding| binding_input_to_binding(Uuid::nil(), binding))
        .collect::<Result<Vec<_>, _>>()?;

    if let Some(identity_kind) = identity_kind {
        let identity_payload = identity_payload.ok_or_else(|| {
            ApiError::BadRequest("显式提供 identity_kind 时，identity_payload 不能为空".into())
        })?;
        return Ok((
            identity_kind.clone(),
            normalize_workspace_identity_payload(identity_kind, identity_payload)?,
            parsed_bindings,
        ));
    }

    let Some(first_binding) = parsed_bindings.first() else {
        return Err(ApiError::BadRequest(
            "创建 Workspace 时，必须提供 identity 或至少一个 binding".into(),
        ));
    };

    let detected =
        detect_workspace_from_backend(state, &first_binding.backend_id, &first_binding.root_ref)
            .await?;
    let replacement_binding = WorkspaceBinding {
        id: first_binding.id,
        workspace_id: Uuid::nil(),
        backend_id: detected.binding.backend_id,
        root_ref: detected.binding.root_ref,
        status: detected.binding.status,
        detected_facts: detected.binding.detected_facts,
        last_verified_at: detected.binding.last_verified_at,
        priority: first_binding.priority,
        created_at: first_binding.created_at,
        updated_at: first_binding.updated_at,
    };
    parsed_bindings[0] = replacement_binding;

    let detected_identity_kind = detected.identity_kind.clone();
    Ok((
        detected.identity_kind,
        identity_payload
            .map(|payload| {
                normalize_workspace_identity_payload(detected_identity_kind.clone(), payload)
            })
            .transpose()?
            .unwrap_or(detected.identity_payload),
        parsed_bindings,
    ))
}

fn binding_input_to_binding(
    workspace_id: Uuid,
    binding: WorkspaceBindingInput,
) -> Result<WorkspaceBinding, ApiError> {
    let backend_id = binding.backend_id.trim().to_string();
    if backend_id.is_empty() {
        return Err(ApiError::BadRequest("binding.backend_id 不能为空".into()));
    }
    let root_ref = binding.root_ref.trim().to_string();
    if root_ref.is_empty() {
        return Err(ApiError::BadRequest("binding.root_ref 不能为空".into()));
    }

    let mut created = WorkspaceBinding::new(
        workspace_id,
        backend_id,
        root_ref,
        binding.detected_facts.unwrap_or_else(|| json!({})),
    );
    if let Some(id) = binding.id {
        created.id = id;
    }
    created.status = binding.status.unwrap_or(WorkspaceBindingStatus::Pending);
    created.priority = binding.priority.unwrap_or_default();
    Ok(created)
}

fn derive_workspace_status(bindings: &[WorkspaceBinding]) -> WorkspaceStatus {
    if bindings
        .iter()
        .any(|binding| matches!(binding.status, WorkspaceBindingStatus::Ready))
    {
        WorkspaceStatus::Ready
    } else if bindings
        .iter()
        .any(|binding| matches!(binding.status, WorkspaceBindingStatus::Error))
    {
        WorkspaceStatus::Error
    } else {
        WorkspaceStatus::Pending
    }
}

fn normalize_workspace_identity_payload(
    kind: WorkspaceIdentityKind,
    payload: Value,
) -> Result<Value, ApiError> {
    normalize_identity_payload(kind, &payload).map_err(ApiError::BadRequest)
}

fn normalize_workspace_name(raw: &str) -> Result<String, ApiError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(ApiError::BadRequest("工作空间名称不能为空".into()));
    }
    Ok(trimmed.to_string())
}

fn parse_project_id(raw: &str) -> Result<Uuid, ApiError> {
    Uuid::parse_str(raw).map_err(|_| ApiError::BadRequest("无效的 Project ID".into()))
}

fn parse_workspace_id(raw: &str) -> Result<Uuid, ApiError> {
    Uuid::parse_str(raw).map_err(|_| ApiError::BadRequest("无效的 Workspace ID".into()))
}

fn require_backend_id(raw: Option<&str>) -> Result<&str, ApiError> {
    raw.map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| ApiError::BadRequest("detect_git 必须显式提供 backend_id".into()))
}

pub(crate) async fn detect_git_via_backend(
    state: &Arc<AppState>,
    backend_id: &str,
    root_ref: &str,
) -> Result<GitDetectionResult, ApiError> {
    let transport: &dyn BackendTransport = state.services.backend_registry.as_ref();
    let info = transport
        .detect_git_repo(backend_id, root_ref)
        .await
        .map_err(|e| ApiError::Internal(format!("detect_git 失败: {e}")))?;

    Ok(GitDetectionResult {
        is_git_repo: info.is_git_repo,
        source_repo: info.source_repo,
        branch: info.branch,
        commit_hash: info.commit_hash,
    })
}
