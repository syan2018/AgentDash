use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::value_objects::{CanvasDataBinding, CanvasFile, CanvasSandboxConfig, CanvasScope};

/// Canvas — Project 级可运行前端资产。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Canvas {
    pub id: Uuid,
    pub project_id: Uuid,
    pub owner_user_id: Option<String>,
    pub scope: CanvasScope,
    pub mount_id: String,
    pub title: String,
    pub description: String,
    pub entry_file: String,
    pub sandbox_config: CanvasSandboxConfig,
    pub files: Vec<CanvasFile>,
    pub bindings: Vec<CanvasDataBinding>,
    pub published_from_canvas_id: Option<Uuid>,
    pub shared_canvas_id: Option<Uuid>,
    pub cloned_from_canvas_id: Option<Uuid>,
    pub published_at: Option<DateTime<Utc>>,
    pub published_by_user_id: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Canvas {
    pub fn new(project_id: Uuid, mount_id: String, title: String, description: String) -> Self {
        Self::new_project_shared(project_id, mount_id, title, description, None, None)
    }

    pub fn new_personal(
        project_id: Uuid,
        owner_user_id: String,
        mount_id: String,
        title: String,
        description: String,
    ) -> Self {
        let mut canvas = Self::base(project_id, mount_id, title, description);
        canvas.owner_user_id = Some(owner_user_id);
        canvas.scope = CanvasScope::Personal;
        canvas
    }

    pub fn new_project_shared(
        project_id: Uuid,
        mount_id: String,
        title: String,
        description: String,
        published_from_canvas_id: Option<Uuid>,
        published_by_user_id: Option<String>,
    ) -> Self {
        let mut canvas = Self::base(project_id, mount_id, title, description);
        canvas.scope = CanvasScope::Project;
        canvas.owner_user_id = published_by_user_id.clone();
        canvas.published_from_canvas_id = published_from_canvas_id;
        canvas.published_at = published_by_user_id.as_ref().map(|_| canvas.created_at);
        canvas.published_by_user_id = published_by_user_id;
        canvas
    }

    fn base(project_id: Uuid, mount_id: String, title: String, description: String) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            project_id,
            owner_user_id: None,
            scope: CanvasScope::Project,
            mount_id,
            title,
            description,
            entry_file: "src/main.tsx".to_string(),
            sandbox_config: CanvasSandboxConfig::default(),
            files: vec![CanvasFile::default_entry()],
            bindings: Vec::new(),
            published_from_canvas_id: None,
            shared_canvas_id: None,
            cloned_from_canvas_id: None,
            published_at: None,
            published_by_user_id: None,
            created_at: now,
            updated_at: now,
        }
    }

    pub fn copy_authoring_from(&mut self, source: &Canvas) {
        self.title = source.title.clone();
        self.description = source.description.clone();
        self.entry_file = source.entry_file.clone();
        self.sandbox_config = source.sandbox_config.clone();
        self.files = source.files.clone();
        self.bindings = source.bindings.clone();
        self.touch();
    }

    pub fn touch(&mut self) {
        self.updated_at = Utc::now();
    }
}
