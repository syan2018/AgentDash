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
    /// Immutable, normalized source snapshot used to compile model context and its presentation.
    ///
    /// `context_slice` remains a control-plane summary. This payload owns the complete source
    /// fragments for the exact AgentFrame revision.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_source_snapshot: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vfs_surface: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mcp_surface: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub execution_profile: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hook_plan: Option<Value>,
}

impl AgentFrameSurfaceDocument {
    pub fn is_empty(&self) -> bool {
        self.capability_state.is_none()
            && self.context_slice.is_none()
            && self.context_source_snapshot.is_none()
            && self.vfs_surface.is_none()
            && self.mcp_surface.is_none()
            && self.execution_profile.is_none()
            && self.hook_plan.is_none()
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
            created_by_kind: "backfill".to_string(),
            created_by_id: None,
            created_at: Utc::now(),
        }
    }

    pub fn new_revision(agent_id: Uuid, revision: i32, created_by_kind: impl Into<String>) -> Self {
        Self::new_revision_with_id(Uuid::new_v4(), agent_id, revision, created_by_kind)
    }

    pub fn new_revision_with_id(
        id: Uuid,
        agent_id: Uuid,
        revision: i32,
        created_by_kind: impl Into<String>,
    ) -> Self {
        Self {
            id,
            agent_id,
            revision,
            surface: None,
            effective_capability_json: None,
            context_slice_json: None,
            vfs_surface_json: None,
            mcp_surface_json: None,
            execution_profile_json: None,
            hook_plan: None,
            created_by_kind: created_by_kind.into(),
            created_by_id: None,
            created_at: Utc::now(),
        }
    }

    pub fn surface_document(&self) -> AgentFrameSurfaceDocument {
        self.surface
            .clone()
            .unwrap_or_else(|| AgentFrameSurfaceDocument {
                capability_state: self.effective_capability_json.clone(),
                context_slice: self.context_slice_json.clone(),
                context_source_snapshot: None,
                vfs_surface: self.vfs_surface_json.clone(),
                mcp_surface: self.mcp_surface_json.clone(),
                execution_profile: self.execution_profile_json.clone(),
                hook_plan: self.hook_plan.clone(),
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
        self.surface = Some(surface);
    }

    /// Attach the immutable HookPlan to the canonical surface document and refresh its split
    /// read projection in one operation.
    ///
    /// HookPlan compilation needs the already allocated frame ID, so frame construction attaches
    /// this snapshot after `AgentFrameBuilder` has produced the uncommitted revision. Updating only
    /// the split `hook_plan` field would be lost as soon as the canonical surface is projected.
    pub fn attach_immutable_hook_plan(&mut self, hook_plan: Value) {
        let mut surface = self.surface_document();
        surface.hook_plan = Some(hook_plan);
        self.surface = Some(surface);
        self.apply_surface_projection();
    }

    /// Replaces the canonical VFS facts for this uncommitted revision and refreshes the split read
    /// projections from that document.
    pub fn attach_immutable_vfs_surface(
        &mut self,
        vfs_surface: Value,
        capability_state: Option<Value>,
    ) {
        let mut surface = self.surface_document();
        surface.vfs_surface = Some(vfs_surface);
        if let Some(capability_state) = capability_state {
            surface.capability_state = Some(capability_state);
        }
        self.surface = Some(surface);
        self.apply_surface_projection();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn surface_document_projects_split_columns() {
        let mut frame = AgentFrame::new_initial(Uuid::new_v4());
        frame.effective_capability_json = Some(serde_json::json!({"tool": {"mcp_servers": []}}));

        let surface = frame.surface_document();

        assert_eq!(
            surface.capability_state,
            Some(serde_json::json!({"tool": {"mcp_servers": []}}))
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

    #[test]
    fn immutable_hook_plan_is_written_to_canonical_surface_and_split_projection() {
        let mut frame = AgentFrame::new_initial(Uuid::new_v4());
        frame.surface = Some(AgentFrameSurfaceDocument {
            capability_state: Some(serde_json::json!({"canonical": true})),
            ..Default::default()
        });
        let hook_plan = serde_json::json!({"revision": 1, "requirements": [], "digest": "v1"});

        frame.attach_immutable_hook_plan(hook_plan.clone());

        assert_eq!(frame.hook_plan, Some(hook_plan.clone()));
        assert_eq!(frame.surface_document().hook_plan, Some(hook_plan));
    }
}
