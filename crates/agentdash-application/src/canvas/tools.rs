use agentdash_domain::canvas::{
    CANVAS_SYSTEM_SKILL_NAME, Canvas, CanvasDataBinding, CanvasRepository,
};
use agentdash_spi::AgentToolError;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::canvas::{build_canvas, upsert_canvas_binding};
use crate::runtime_tools::SharedSessionToolServicesHandle;
use crate::vfs::build_canvas_mount_id;
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
        skill_path: format!("lifecycle://skills/{CANVAS_SYSTEM_SKILL_NAME}/SKILL.md"),
    };
    Ok((canvas, result))
}

pub(crate) async fn bind_canvas_data_for_project(
    canvas_repo: &dyn CanvasRepository,
    project_id: Uuid,
    params: BindCanvasDataParams,
) -> Result<(Canvas, CanvasBindingToolResult), AgentToolError> {
    let mut canvas = load_canvas_by_ref(canvas_repo, project_id, &params.canvas_id).await?;

    let binding =
        CanvasDataBinding::with_content_type(params.alias, params.source_uri, params.content_type);
    let alias = binding.alias.clone();
    let source_uri = binding.source_uri.clone();
    let content_type = binding.content_type.clone();
    upsert_canvas_binding(&mut canvas, binding)
        .map_err(|error| AgentToolError::ExecutionFailed(error.to_string()))?;
    canvas_repo
        .update(&canvas)
        .await
        .map_err(|error| AgentToolError::ExecutionFailed(error.to_string()))?;

    let result = CanvasBindingToolResult {
        canvas_id: canvas.mount_id.clone(),
        mount_id: build_canvas_mount_id(&canvas),
        bindings: canvas.bindings.clone(),
        alias,
        source_uri,
        content_type,
    };
    Ok((canvas, result))
}

pub async fn expose_existing_canvas_for_session(
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
    let session_services = session_services_handle.get().await.ok_or_else(|| {
        AgentToolError::ExecutionFailed(
            "Session services 尚未完成初始化，无法暴露 Canvas".to_string(),
        )
    })?;
    let session_id = current_session_id.ok_or_else(|| {
        AgentToolError::ExecutionFailed(
            "当前工具调用缺少 RuntimeSession id，无法暴露 Canvas".to_string(),
        )
    })?;
    let active_vfs = session_services
        .capability
        .expose_canvas_mount_revision_and_adopt(session_id, canvas)
        .await
        .map_err(|error| {
            AgentToolError::ExecutionFailed(format!(
                "Canvas exposure 写入 AgentFrame 失败: {error}"
            ))
        })?;
    vfs.replace(active_vfs).await;
    Ok(())
}
