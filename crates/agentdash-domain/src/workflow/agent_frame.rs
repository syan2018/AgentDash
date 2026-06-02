use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use uuid::Uuid;

pub const RUNTIME_SESSION_REF_KIND: &str = "runtime_session";

/// RuntimeSession refs are delivery/provenance refs attached to an AgentFrame.
/// Callers must name their selection policy instead of silently taking the
/// first stored ref.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeSessionSelectionPolicy {
    /// The runtime session currently handled by the runtime adapter.
    Specific { runtime_session_id: String },
    /// The first runtime session attached to the frame lineage.
    LaunchPrimary,
    /// The latest runtime session attached to the frame lineage.
    LatestAttached,
}

/// AgentFrame revision row — effective runtime surface snapshot。
///
/// 每次 capability/context/VFS/MCP surface 变更产生新 revision。
/// `runtime_session_refs_json` 是 trace/delivery refs，不是 subject association。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentFrame {
    pub id: Uuid,
    pub agent_id: Uuid,
    pub revision: i32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub procedure_id: Option<Uuid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub graph_instance_id: Option<Uuid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub activity_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effective_capability_json: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_slice_json: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vfs_surface_json: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mcp_surface_json: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime_session_refs_json: Option<serde_json::Value>,
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
    pub fn new_initial(agent_id: Uuid, runtime_session_refs: Option<serde_json::Value>) -> Self {
        Self {
            id: Uuid::new_v4(),
            agent_id,
            revision: 1,
            procedure_id: None,
            graph_instance_id: None,
            activity_key: None,
            effective_capability_json: None,
            context_slice_json: None,
            vfs_surface_json: None,
            mcp_surface_json: None,
            runtime_session_refs_json: runtime_session_refs,
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
            procedure_id: None,
            graph_instance_id: None,
            activity_key: None,
            effective_capability_json: None,
            context_slice_json: None,
            vfs_surface_json: None,
            mcp_surface_json: None,
            runtime_session_refs_json: None,
            execution_profile_json: None,
            visible_canvas_mount_ids_json: None,
            created_by_kind: created_by_kind.into(),
            created_by_id: None,
            created_at: Utc::now(),
        }
    }

    pub fn runtime_session_ref_json(session_id: impl AsRef<str>) -> Value {
        json!({
            "kind": RUNTIME_SESSION_REF_KIND,
            "session_id": session_id.as_ref(),
        })
    }

    pub fn runtime_session_refs_json<I, S>(session_ids: I) -> Option<Value>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let refs = session_ids
            .into_iter()
            .map(Self::runtime_session_ref_json)
            .collect::<Vec<_>>();
        (!refs.is_empty()).then_some(Value::Array(refs))
    }

    pub fn runtime_session_ids(&self) -> Vec<String> {
        let Some(Value::Array(refs)) = &self.runtime_session_refs_json else {
            return Vec::new();
        };
        refs.iter()
            .filter_map(|value| match value {
                Value::Object(object)
                    if object.get("kind").and_then(Value::as_str)
                        == Some(RUNTIME_SESSION_REF_KIND) =>
                {
                    object
                        .get("session_id")
                        .and_then(Value::as_str)
                        .map(str::to_string)
                }
                _ => None,
            })
            .collect()
    }

    pub fn select_runtime_session_id(
        &self,
        policy: RuntimeSessionSelectionPolicy,
    ) -> Option<String> {
        let ids = self.runtime_session_ids();
        match policy {
            RuntimeSessionSelectionPolicy::Specific { runtime_session_id } => {
                ids.into_iter().find(|id| id == &runtime_session_id)
            }
            RuntimeSessionSelectionPolicy::LaunchPrimary => ids.into_iter().next(),
            RuntimeSessionSelectionPolicy::LatestAttached => ids.into_iter().next_back(),
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

    pub fn attach_runtime_session_ref(&mut self, session_id: impl AsRef<str>) {
        let session_id = session_id.as_ref();
        if session_id.trim().is_empty() {
            return;
        }
        let already_attached = self
            .runtime_session_ids()
            .iter()
            .any(|existing| existing == session_id);
        if already_attached {
            return;
        }
        let next_ref = Self::runtime_session_ref_json(session_id);
        match &mut self.runtime_session_refs_json {
            Some(Value::Array(refs)) => refs.push(next_ref),
            _ => self.runtime_session_refs_json = Some(Value::Array(vec![next_ref])),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_session_selection_requires_explicit_policy() {
        let mut frame = AgentFrame::new_revision(Uuid::new_v4(), 1, "test");
        frame.runtime_session_refs_json =
            AgentFrame::runtime_session_refs_json(["session-1", "session-2"]);

        assert_eq!(
            frame
                .select_runtime_session_id(RuntimeSessionSelectionPolicy::LaunchPrimary)
                .as_deref(),
            Some("session-1")
        );
        assert_eq!(
            frame
                .select_runtime_session_id(RuntimeSessionSelectionPolicy::LatestAttached)
                .as_deref(),
            Some("session-2")
        );
        assert_eq!(
            frame
                .select_runtime_session_id(RuntimeSessionSelectionPolicy::Specific {
                    runtime_session_id: "session-2".to_string(),
                })
                .as_deref(),
            Some("session-2")
        );
        assert!(
            frame
                .select_runtime_session_id(RuntimeSessionSelectionPolicy::Specific {
                    runtime_session_id: "missing".to_string(),
                })
                .is_none()
        );
    }
}
