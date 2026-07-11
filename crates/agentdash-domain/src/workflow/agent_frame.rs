use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

/// AgentFrame revision 的 canonical runtime surface document。
///
/// 旧 split columns 仍可作为读投影存在，但 repository 读写时以这个 document
/// 作为同一 revision 的单一 surface 形态。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct AgentFrameSurfaceDocument {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capability_state: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_slice: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vfs_surface: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mcp_surface: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub execution_profile: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hook_plan: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub visible_canvas_mount_ids: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub visible_workspace_module_refs: Option<Value>,
}

impl AgentFrameSurfaceDocument {
    pub fn is_empty(&self) -> bool {
        self.capability_state.is_none()
            && self.context_slice.is_none()
            && self.vfs_surface.is_none()
            && self.mcp_surface.is_none()
            && self.execution_profile.is_none()
            && self.hook_plan.is_none()
            && self.visible_canvas_mount_ids.is_none()
            && self.visible_workspace_module_refs.is_none()
    }
}

/// AgentFrame revision row — effective runtime surface snapshot。
///
/// 每次 capability/context/VFS/MCP surface 变更产生新 revision。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentFrame {
    pub id: Uuid,
    pub agent_id: Uuid,
    pub revision: i32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub surface: Option<AgentFrameSurfaceDocument>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effective_capability_json: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_slice_json: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vfs_surface_json: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mcp_surface_json: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub execution_profile_json: Option<serde_json::Value>,
    /// 当前 revision 的 immutable HookPlan requirements。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hook_plan: Option<serde_json::Value>,
    /// 当前 revision 的 Canvas mount 可见性投影。
    ///
    /// 可见性变更通过新的 AgentFrame revision 物化，既有 revision 保持不可变。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub visible_canvas_mount_ids_json: Option<serde_json::Value>,
    /// 当前 revision 的动态 WorkspaceModule 可见性投影。
    ///
    /// 声明式 workspace module 可见性来自 `effective_capability_json` 中的
    /// `CapabilityState.workspace_module` 维度；Canvas create / present / user-open 等
    /// 运行期授权通过新的 frame revision 物化。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub visible_workspace_module_refs_json: Option<serde_json::Value>,
    pub created_by_kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_by_id: Option<String>,
    pub created_at: DateTime<Utc>,
}

impl AgentFrame {
    pub fn new_initial(agent_id: Uuid) -> Self {
        Self {
            id: Uuid::new_v4(),
            agent_id,
            revision: 1,
            surface: None,
            effective_capability_json: None,
            context_slice_json: None,
            vfs_surface_json: None,
            mcp_surface_json: None,
            execution_profile_json: None,
            hook_plan: None,
            visible_canvas_mount_ids_json: None,
            visible_workspace_module_refs_json: None,
            created_by_kind: "backfill".to_string(),
            created_by_id: None,
            created_at: Utc::now(),
        }
    }

    pub fn new_revision(agent_id: Uuid, revision: i32, created_by_kind: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            agent_id,
            revision,
            surface: None,
            effective_capability_json: None,
            context_slice_json: None,
            vfs_surface_json: None,
            mcp_surface_json: None,
            execution_profile_json: None,
            hook_plan: None,
            visible_canvas_mount_ids_json: None,
            visible_workspace_module_refs_json: None,
            created_by_kind: created_by_kind.into(),
            created_by_id: None,
            created_at: Utc::now(),
        }
    }

    pub fn visible_canvas_mount_ids(&self) -> Vec<String> {
        let Some(Value::Array(ids)) = &self.visible_canvas_mount_ids_json else {
            return Vec::new();
        };
        ids.iter()
            .filter_map(|v| {
                let value = v.as_str()?.trim();
                (!value.is_empty()).then(|| value.to_string())
            })
            .collect()
    }

    pub fn visible_workspace_module_refs(&self) -> Vec<String> {
        let Some(Value::Array(refs)) = &self.visible_workspace_module_refs_json else {
            return Vec::new();
        };
        refs.iter()
            .filter_map(|v| {
                let value = v.as_str()?.trim();
                (!value.is_empty()).then(|| value.to_string())
            })
            .collect()
    }

    pub fn surface_document(&self) -> AgentFrameSurfaceDocument {
        self.surface
            .clone()
            .unwrap_or_else(|| AgentFrameSurfaceDocument {
                capability_state: self.effective_capability_json.clone(),
                context_slice: self.context_slice_json.clone(),
                vfs_surface: self.vfs_surface_json.clone(),
                mcp_surface: self.mcp_surface_json.clone(),
                execution_profile: self.execution_profile_json.clone(),
                hook_plan: self.hook_plan.clone(),
                visible_canvas_mount_ids: self.visible_canvas_mount_ids_json.clone(),
                visible_workspace_module_refs: self.visible_workspace_module_refs_json.clone(),
            })
    }

    pub fn apply_surface_projection(&mut self) {
        let surface = self.surface_document();
        if surface.is_empty() {
            return;
        }
        self.effective_capability_json = surface.capability_state.clone();
        self.context_slice_json = surface.context_slice.clone();
        self.vfs_surface_json = surface.vfs_surface.clone();
        self.mcp_surface_json = surface.mcp_surface.clone();
        self.execution_profile_json = surface.execution_profile.clone();
        self.hook_plan = surface.hook_plan.clone();
        self.visible_canvas_mount_ids_json = surface.visible_canvas_mount_ids.clone();
        self.visible_workspace_module_refs_json = surface.visible_workspace_module_refs.clone();
        self.surface = Some(surface);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visible_workspace_module_refs_default_empty() {
        let frame = AgentFrame::new_initial(Uuid::new_v4());
        assert!(frame.visible_workspace_module_refs().is_empty());
        assert!(frame.visible_workspace_module_refs_json.is_none());
    }

    #[test]
    fn visible_workspace_module_refs_read_persisted_projection() {
        let mut frame = AgentFrame::new_initial(Uuid::new_v4());
        frame.visible_workspace_module_refs_json = Some(serde_json::json!([
            "ext:demo",
            "",
            "canvas:cvs-dashboard-a",
            42,
        ]));
        assert_eq!(
            frame.visible_workspace_module_refs(),
            vec!["ext:demo".to_string(), "canvas:cvs-dashboard-a".to_string()]
        );
    }

    #[test]
    fn new_revision_does_not_carry_workspace_module_refs() {
        let frame = AgentFrame::new_revision(Uuid::new_v4(), 2, "test");
        assert!(frame.visible_workspace_module_refs_json.is_none());
    }

    #[test]
    fn surface_document_projects_split_columns() {
        let mut frame = AgentFrame::new_initial(Uuid::new_v4());
        frame.effective_capability_json = Some(serde_json::json!({"tool": {"mcp_servers": []}}));
        frame.visible_canvas_mount_ids_json = Some(serde_json::json!(["canvas:a"]));

        let surface = frame.surface_document();

        assert_eq!(
            surface.capability_state,
            Some(serde_json::json!({"tool": {"mcp_servers": []}}))
        );
        assert_eq!(
            surface.visible_canvas_mount_ids,
            Some(serde_json::json!(["canvas:a"]))
        );
    }

    #[test]
    fn surface_document_overrides_split_projection() {
        let mut frame = AgentFrame::new_initial(Uuid::new_v4());
        frame.effective_capability_json = Some(serde_json::json!({"stale": true}));
        frame.surface = Some(AgentFrameSurfaceDocument {
            capability_state: Some(serde_json::json!({"canonical": true})),
            ..Default::default()
        });

        frame.apply_surface_projection();

        assert_eq!(
            frame.effective_capability_json,
            Some(serde_json::json!({"canonical": true}))
        );
    }
}
