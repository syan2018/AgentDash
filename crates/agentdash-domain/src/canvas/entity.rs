use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::value_objects::{CanvasDataBinding, CanvasFile, CanvasSandboxConfig};

/// Canvas — Project 级可运行前端资产。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Canvas {
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

impl Canvas {
    pub fn new(project_id: Uuid, mount_id: String, title: String, description: String) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            project_id,
            mount_id,
            title,
            description,
            entry_file: "src/main.tsx".to_string(),
            sandbox_config: CanvasSandboxConfig::default(),
            files: vec![CanvasFile::default_entry()],
            bindings: Vec::new(),
            created_at: now,
            updated_at: now,
        }
    }

    pub fn touch(&mut self) {
        self.updated_at = Utc::now();
    }
}
