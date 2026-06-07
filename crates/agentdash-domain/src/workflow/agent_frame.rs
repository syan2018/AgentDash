use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

/// AgentFrame revision row — effective runtime surface snapshot。
///
/// 每次 capability/context/VFS/MCP surface 变更产生新 revision。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentFrame {
    pub id: Uuid,
    pub agent_id: Uuid,
    pub revision: i32,
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
    /// 当前可见的 Canvas mount ids（运行时追加，不随 revision 复制）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub visible_canvas_mount_ids_json: Option<serde_json::Value>,
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
            effective_capability_json: None,
            context_slice_json: None,
            vfs_surface_json: None,
            mcp_surface_json: None,
            execution_profile_json: None,
            visible_canvas_mount_ids_json: None,
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
            effective_capability_json: None,
            context_slice_json: None,
            vfs_surface_json: None,
            mcp_surface_json: None,
            execution_profile_json: None,
            visible_canvas_mount_ids_json: None,
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
            .filter_map(|v| v.as_str().map(str::to_string))
            .collect()
    }

    pub fn append_visible_canvas_mount(&mut self, mount_id: &str) {
        if mount_id.trim().is_empty() {
            return;
        }
        let already = self
            .visible_canvas_mount_ids()
            .iter()
            .any(|existing| existing == mount_id);
        if already {
            return;
        }
        let next = Value::String(mount_id.to_string());
        match &mut self.visible_canvas_mount_ids_json {
            Some(Value::Array(ids)) => ids.push(next),
            _ => self.visible_canvas_mount_ids_json = Some(Value::Array(vec![next])),
        }
    }
}
