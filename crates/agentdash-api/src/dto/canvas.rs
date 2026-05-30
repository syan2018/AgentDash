use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use agentdash_domain::canvas::{Canvas, CanvasDataBinding, CanvasFile, CanvasSandboxConfig};

#[derive(Debug, Deserialize)]
pub struct ListProjectCanvasesPath {
    pub project_id: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateCanvasRequest {
    pub mount_id: Option<String>,
    pub title: String,
    pub description: Option<String>,
    pub entry_file: Option<String>,
    pub sandbox_config: Option<CanvasSandboxConfig>,
    pub files: Option<Vec<CanvasFile>>,
    pub bindings: Option<Vec<CanvasDataBinding>>,
}

#[derive(Debug, Deserialize, Default)]
pub struct UpdateCanvasRequest {
    pub title: Option<String>,
    pub description: Option<String>,
    pub entry_file: Option<String>,
    pub sandbox_config: Option<CanvasSandboxConfig>,
    pub files: Option<Vec<CanvasFile>>,
    pub bindings: Option<Vec<CanvasDataBinding>>,
}

#[derive(Debug, Deserialize, Default)]
pub struct CanvasRuntimeSnapshotQuery {
    pub session_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CanvasRuntimeInvokeRequest {
    pub session_id: String,
    pub action_key: String,
    #[serde(default)]
    pub input: Value,
}

#[derive(Debug, Deserialize)]
pub struct PromoteCanvasToExtensionRequest {
    pub extension_key: Option<String>,
    pub display_name: Option<String>,
    pub package_version: Option<String>,
    pub asset_version: Option<String>,
    #[serde(default = "default_promote_overwrite")]
    pub overwrite: bool,
}

#[derive(Debug, Serialize)]
pub struct CanvasResponse {
    pub id: Uuid,
    pub project_id: Uuid,
    pub mount_id: String,
    pub title: String,
    pub description: String,
    pub entry_file: String,
    pub sandbox_config: CanvasSandboxConfig,
    pub files: Vec<CanvasFile>,
    pub bindings: Vec<CanvasDataBinding>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<Canvas> for CanvasResponse {
    fn from(canvas: Canvas) -> Self {
        Self {
            id: canvas.id,
            project_id: canvas.project_id,
            mount_id: canvas.mount_id,
            title: canvas.title,
            description: canvas.description,
            entry_file: canvas.entry_file,
            sandbox_config: canvas.sandbox_config,
            files: canvas.files,
            bindings: canvas.bindings,
            created_at: canvas.created_at,
            updated_at: canvas.updated_at,
        }
    }
}

fn default_promote_overwrite() -> bool {
    true
}
