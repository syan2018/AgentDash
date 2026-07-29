use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

/// AgentFrame revision 的 canonical runtime surface document。
///
/// 每个字段只在这个 document 中存储一次；消费者不得维护并列 mirror。
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
    #[serde(default)]
    pub surface: AgentFrameSurfaceDocument,
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
            surface: AgentFrameSurfaceDocument::default(),
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
            surface: AgentFrameSurfaceDocument::default(),
            created_by_kind: created_by_kind.into(),
            created_by_id: None,
            created_at: Utc::now(),
        }
    }

    pub fn surface_document(&self) -> AgentFrameSurfaceDocument {
        self.surface.clone()
    }

    /// Attach the immutable HookPlan to the canonical surface document.
    ///
    /// HookPlan compilation needs the already allocated frame ID, so frame construction attaches
    /// this snapshot after `AgentFrameBuilder` has produced the uncommitted revision.
    pub fn attach_immutable_hook_plan(&mut self, hook_plan: Value) {
        let mut surface = self.surface_document();
        surface.hook_plan = Some(hook_plan);
        self.surface = surface;
    }

    /// Replaces the canonical VFS facts for this uncommitted revision.
    pub fn attach_immutable_vfs_surface(&mut self, vfs_surface: Value) {
        let mut surface = self.surface_document();
        surface.vfs_surface = Some(vfs_surface);
        self.surface = surface;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn surface_document_owns_capability_state_once() {
        let mut frame = AgentFrame::new_initial(Uuid::new_v4());
        frame.surface.capability_state = Some(serde_json::json!({"tool": {"capabilities": []}}));

        let surface = frame.surface_document();

        assert_eq!(
            surface.capability_state,
            Some(serde_json::json!({"tool": {"capabilities": []}}))
        );
    }

    #[test]
    fn surface_document_is_the_only_runtime_surface() {
        let mut frame = AgentFrame::new_initial(Uuid::new_v4());
        frame.surface = AgentFrameSurfaceDocument {
            capability_state: Some(serde_json::json!({"canonical": true})),
            ..Default::default()
        };

        assert_eq!(
            frame.surface_document().capability_state,
            Some(serde_json::json!({"canonical": true}))
        );
    }

    #[test]
    fn immutable_hook_plan_is_written_to_canonical_surface() {
        let mut frame = AgentFrame::new_initial(Uuid::new_v4());
        frame.surface = AgentFrameSurfaceDocument {
            capability_state: Some(serde_json::json!({"canonical": true})),
            ..Default::default()
        };
        let hook_plan = serde_json::json!({"revision": 1, "requirements": [], "digest": "v1"});

        frame.attach_immutable_hook_plan(hook_plan.clone());

        assert_eq!(frame.surface_document().hook_plan, Some(hook_plan));
    }
}
