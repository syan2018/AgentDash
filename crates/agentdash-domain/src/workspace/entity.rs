use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use super::value_objects::{
    WorkspaceBinding, WorkspaceIdentityKind, WorkspaceResolutionPolicy, WorkspaceStatus,
};

/// Workspace — 逻辑工作空间聚合。
///
/// 表达 Project 依赖的“工作空间身份”，而不是某个 backend 上的单一目录。
/// 物理目录、backend 与探测事实都通过 bindings 挂在该聚合下。
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Workspace {
    pub id: Uuid,
    pub project_id: Uuid,
    pub name: String,
    pub identity_kind: WorkspaceIdentityKind,
    pub identity_payload: Value,
    pub resolution_policy: WorkspaceResolutionPolicy,
    pub default_binding_id: Option<Uuid>,
    pub status: WorkspaceStatus,
    pub bindings: Vec<WorkspaceBinding>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Workspace {
    pub fn new(
        project_id: Uuid,
        name: String,
        identity_kind: WorkspaceIdentityKind,
        identity_payload: Value,
        resolution_policy: WorkspaceResolutionPolicy,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            project_id,
            name,
            identity_kind,
            identity_payload,
            resolution_policy,
            default_binding_id: None,
            status: WorkspaceStatus::Pending,
            bindings: Vec::new(),
            created_at: now,
            updated_at: now,
        }
    }

    pub fn set_bindings(&mut self, mut bindings: Vec<WorkspaceBinding>) {
        for binding in &mut bindings {
            binding.workspace_id = self.id;
        }
        self.bindings = bindings;
        self.updated_at = Utc::now();
        self.refresh_default_binding();
    }

    pub fn refresh_default_binding(&mut self) {
        if let Some(default_binding_id) = self.default_binding_id
            && self.bindings.iter().any(|binding| binding.id == default_binding_id)
        {
            return;
        }
        self.default_binding_id = self.bindings.first().map(|binding| binding.id);
    }
}
