use agentdash_domain::canvas::{Canvas, CanvasRepository};
use agentdash_domain::project::ProjectAuthorizationContext;
use agentdash_spi::AgentToolError;
use schemars::JsonSchema;
use serde::Deserialize;
use uuid::Uuid;

use crate::canvas::runtime_surface::submit_existing_canvas_visibility_request;
use crate::runtime_tools::SharedSessionToolServicesHandle;
use crate::vfs::tools::fs::SharedRuntimeVfs;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct StartCanvasParams {
    /// Stable Canvas VFS mount identifier (`cvs-...`). If it matches an existing canvas, that canvas is attached to the current session; otherwise a new canvas is created with this id.
    pub canvas_mount_id: Option<String>,
    /// Title for the new canvas. Required when `canvas_mount_id` does not match an existing canvas.
    pub title: Option<String>,
    /// Optional description for the new canvas.
    pub description: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct BindCanvasDataParams {
    pub canvas_mount_id: String,
    pub alias: String,
    pub source_uri: String,
    pub content_type: Option<String>,
}

pub async fn request_existing_canvas_visibility_for_runtime(
    canvas_repo: &dyn CanvasRepository,
    project_id: Uuid,
    canvas_mount_id: &str,
    vfs: &SharedRuntimeVfs,
    session_services_handle: &SharedSessionToolServicesHandle,
    current_session_id: Option<&str>,
    current_user: Option<&ProjectAuthorizationContext>,
) -> Result<Canvas, AgentToolError> {
    submit_existing_canvas_visibility_request(
        canvas_repo,
        project_id,
        canvas_mount_id,
        Some(vfs),
        session_services_handle,
        current_session_id,
        current_user,
    )
    .await
}
