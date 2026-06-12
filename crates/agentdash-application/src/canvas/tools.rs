use agentdash_domain::canvas::{
    CANVAS_SYSTEM_SKILL_NAME, CANVAS_SYSTEM_SKILL_PATH, Canvas, CanvasDataBinding, CanvasRepository,
};
use agentdash_spi::AgentToolError;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::canvas::{build_canvas, upsert_canvas_binding};
use crate::session::AgentFrameRuntimeTarget;
use crate::vfs::build_canvas_mount_id;
use crate::vfs::tools::SharedSessionToolServicesHandle;
use crate::vfs::tools::fs::SharedRuntimeVfs;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct StartCanvasParams {
    /// Stable canvas identifier. If it matches an existing canvas, that canvas is attached to the current session; otherwise a new canvas is created with this id.
    pub canvas_id: Option<String>,
    /// Title for the new canvas. Required when `canvas_id` does not match an existing canvas.
    pub title: Option<String>,
    /// Optional description for the new canvas.
    pub description: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct BindCanvasDataParams {
    pub canvas_id: String,
    pub alias: String,
    pub source_uri: String,
    pub content_type: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) struct CanvasToolResult {
    pub action: String,
    pub canvas_id: String,
    pub mount_id: String,
    pub title: String,
    pub entry_file: String,
    pub skill_name: String,
    pub skill_path: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) struct CanvasBindingToolResult {
    pub canvas_id: String,
    pub mount_id: String,
    pub bindings: Vec<CanvasDataBinding>,
    pub alias: String,
    pub source_uri: String,
    pub content_type: String,
}

pub(crate) async fn create_or_attach_canvas_for_session(
    canvas_repo: &dyn CanvasRepository,
    project_id: Uuid,
    vfs: &SharedRuntimeVfs,
    session_services_handle: &SharedSessionToolServicesHandle,
    current_session_id: Option<&str>,
    params: StartCanvasParams,
) -> Result<(Canvas, CanvasToolResult), AgentToolError> {
    let requested_canvas_id = params
        .canvas_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);

    let (canvas, action) = if let Some(canvas_id) = requested_canvas_id {
        let existing_canvas = canvas_repo
            .get_by_mount_id(project_id, &canvas_id)
            .await
            .map_err(|error| AgentToolError::ExecutionFailed(error.to_string()))?;

        if let Some(canvas) = existing_canvas {
            ensure_canvas_project(canvas.project_id, project_id)?;
            (canvas, "attached".to_string())
        } else {
            let title = params
                .title
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| {
                    AgentToolError::InvalidArguments(
                        "title is required when canvas_id does not match an existing canvas"
                            .to_string(),
                    )
                })?;

            let canvas = build_canvas(
                project_id,
                Some(canvas_id),
                title.to_string(),
                params.description.unwrap_or_default(),
                Default::default(),
            )
            .map_err(|error| AgentToolError::ExecutionFailed(error.to_string()))?;
            canvas_repo
                .create(&canvas)
                .await
                .map_err(|error| AgentToolError::ExecutionFailed(error.to_string()))?;
            (canvas, "created".to_string())
        }
    } else {
        let title = params
            .title
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                AgentToolError::InvalidArguments(
                    "title is required when creating a new canvas".to_string(),
                )
            })?;

        let canvas = build_canvas(
            project_id,
            None,
            title.to_string(),
            params.description.unwrap_or_default(),
            Default::default(),
        )
        .map_err(|error| AgentToolError::ExecutionFailed(error.to_string()))?;
        if canvas_repo
            .get_by_mount_id(project_id, &canvas.mount_id)
            .await
            .map_err(|error| AgentToolError::ExecutionFailed(error.to_string()))?
            .is_some()
        {
            return Err(AgentToolError::ExecutionFailed(format!(
                "canvas id already exists: {}",
                canvas.mount_id
            )));
        }
        canvas_repo
            .create(&canvas)
            .await
            .map_err(|error| AgentToolError::ExecutionFailed(error.to_string()))?;
        (canvas, "created".to_string())
    };

    expose_canvas_to_session(vfs, session_services_handle, current_session_id, &canvas).await?;

    let result = CanvasToolResult {
        action,
        canvas_id: canvas.mount_id.clone(),
        mount_id: build_canvas_mount_id(&canvas),
        title: canvas.title.clone(),
        entry_file: canvas.entry_file.clone(),
        skill_name: CANVAS_SYSTEM_SKILL_NAME.to_string(),
        skill_path: format!(
            "{}://{}",
            build_canvas_mount_id(&canvas),
            CANVAS_SYSTEM_SKILL_PATH
        ),
    };
    Ok((canvas, result))
}

pub(crate) async fn bind_canvas_data_for_project(
    canvas_repo: &dyn CanvasRepository,
    project_id: Uuid,
    params: BindCanvasDataParams,
) -> Result<CanvasBindingToolResult, AgentToolError> {
    let mut canvas = load_canvas_by_ref(canvas_repo, project_id, &params.canvas_id).await?;

    let mut binding = CanvasDataBinding::new(params.alias, params.source_uri);
    if let Some(content_type) = params.content_type {
        binding.content_type = content_type;
    }
    let alias = binding.alias.clone();
    let source_uri = binding.source_uri.clone();
    let content_type = binding.content_type.clone();
    upsert_canvas_binding(&mut canvas, binding)
        .map_err(|error| AgentToolError::ExecutionFailed(error.to_string()))?;
    canvas_repo
        .update(&canvas)
        .await
        .map_err(|error| AgentToolError::ExecutionFailed(error.to_string()))?;

    Ok(CanvasBindingToolResult {
        canvas_id: canvas.mount_id.clone(),
        mount_id: build_canvas_mount_id(&canvas),
        bindings: canvas.bindings.clone(),
        alias,
        source_uri,
        content_type,
    })
}

pub(crate) async fn expose_existing_canvas_for_session(
    canvas_repo: &dyn CanvasRepository,
    project_id: Uuid,
    canvas_id: &str,
    vfs: &SharedRuntimeVfs,
    session_services_handle: &SharedSessionToolServicesHandle,
    current_session_id: Option<&str>,
) -> Result<Canvas, AgentToolError> {
    let canvas = load_canvas_by_ref(canvas_repo, project_id, canvas_id).await?;
    expose_canvas_to_session(vfs, session_services_handle, current_session_id, &canvas).await?;
    Ok(canvas)
}

async fn load_canvas_by_ref(
    canvas_repo: &dyn CanvasRepository,
    expected_project_id: Uuid,
    raw_canvas_id: &str,
) -> Result<agentdash_domain::canvas::Canvas, AgentToolError> {
    let trimmed = raw_canvas_id.trim();
    if trimmed.is_empty() {
        return Err(AgentToolError::InvalidArguments(
            "canvas_id 不能为空".to_string(),
        ));
    }

    let canvas = canvas_repo
        .get_by_mount_id(expected_project_id, trimmed)
        .await
        .map_err(|error| AgentToolError::ExecutionFailed(error.to_string()))?;
    let canvas = canvas
        .ok_or_else(|| AgentToolError::ExecutionFailed(format!("Canvas 不存在: {trimmed}")))?;
    ensure_canvas_project(canvas.project_id, expected_project_id)?;
    Ok(canvas)
}

fn ensure_canvas_project(
    canvas_project_id: Uuid,
    expected_project_id: Uuid,
) -> Result<(), AgentToolError> {
    if canvas_project_id == expected_project_id {
        Ok(())
    } else {
        Err(AgentToolError::ExecutionFailed(
            "当前 session 无权操作其它 Project 的 Canvas".to_string(),
        ))
    }
}

pub(crate) async fn expose_canvas_to_session(
    vfs: &SharedRuntimeVfs,
    session_services_handle: &SharedSessionToolServicesHandle,
    current_session_id: Option<&str>,
    canvas: &Canvas,
) -> Result<(), AgentToolError> {
    vfs.append_canvas_mount(canvas).await;
    if let Some(session_services) = session_services_handle.get().await
        && let Some(session_id) = current_session_id
    {
        let mount_id = canvas.mount_id.clone();
        if let Err(error) = session_services
            .capability
            .append_visible_canvas_mount_to_frame(session_id, &mount_id)
            .await
        {
            return Err(AgentToolError::ExecutionFailed(format!(
                "Canvas mount 写入 AgentFrame 失败: {error}"
            )));
        }
        sync_canvas_mount_capability_state_for_runtime_delivery(
            vfs,
            &session_services,
            session_id,
            canvas,
        )
        .await?;
        let module_ref = format!("canvas:{}", canvas.mount_id);
        if let Err(error) = session_services
            .capability
            .append_visible_workspace_module_ref_to_frame(session_id, &module_ref)
            .await
        {
            return Err(AgentToolError::ExecutionFailed(format!(
                "Canvas module ref 写入 AgentFrame 失败: {error}"
            )));
        }
    }
    Ok(())
}

async fn sync_canvas_mount_capability_state_for_runtime_delivery(
    vfs: &SharedRuntimeVfs,
    session_services: &crate::vfs::tools::SessionToolServices,
    session_id: &str,
    canvas: &Canvas,
) -> Result<(), AgentToolError> {
    let Some(before_state) = session_services
        .capability
        .get_latest_capability_state(session_id)
        .await
    else {
        tracing::debug!(
            session_id = %session_id,
            canvas_id = %canvas.mount_id,
            "Canvas mount 已写入 VFS，但当前 session 尚无 CapabilityState 可同步"
        );
        return Ok(());
    };

    let target = session_services
        .capability
        .resolve_runtime_session_target(session_id)
        .await
        .map_err(AgentToolError::ExecutionFailed)?;

    let Some(hook_runtime) = session_services
        .hooks
        .get_hook_runtime_for_target(&target)
        .await
        .map_err(|error| AgentToolError::ExecutionFailed(error.to_string()))?
    else {
        tracing::debug!(
            session_id = %session_id,
            canvas_id = %canvas.mount_id,
            "Canvas mount 已写入 VFS，但当前 session 尚无 Hook runtime 可接收能力状态热更新"
        );
        return Ok(());
    };

    sync_canvas_mount_capability_state(vfs, session_services, target, before_state, hook_runtime)
        .await
}

async fn sync_canvas_mount_capability_state(
    vfs: &SharedRuntimeVfs,
    session_services: &crate::vfs::tools::SessionToolServices,
    target: AgentFrameRuntimeTarget,
    before_state: crate::session::CapabilityState,
    hook_runtime: agentdash_spi::hooks::SharedHookRuntime,
) -> Result<(), AgentToolError> {
    let active_vfs = vfs.snapshot().await;
    session_services
        .capability
        .apply_live_vfs_capability_state(
            &hook_runtime,
            target,
            before_state,
            active_vfs,
            "canvas",
            "canvas_visible",
        )
        .await
        .map_err(AgentToolError::ExecutionFailed)?;
    Ok(())
}
