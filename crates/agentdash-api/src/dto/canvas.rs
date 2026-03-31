use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

use agentdash_domain::canvas::{Canvas, CanvasDataBinding, CanvasFile, CanvasSandboxConfig};

#[derive(Debug, Serialize)]
pub struct CanvasResponse {
    pub id: Uuid,
    pub project_id: Uuid,
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
