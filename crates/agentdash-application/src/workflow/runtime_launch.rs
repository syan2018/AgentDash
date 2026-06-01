//! RuntimeLaunchRequest — 从 AgentFrame 投影出的 runtime adapter 请求。
//!
//! ## 设计定位
//!
//! `RuntimeLaunchRequest` 是 connector launch 的唯一输入来源：
//!
//! ```text
//! AgentFrame revision
//!   → RuntimeLaunchRequest::from_frame()
//!   → connector ExecutionContext
//!   → RuntimeSession events
//! ```
//!
//! 后续 connector 改造时，launch 路径从 `SessionConstructionPlan → LaunchPlan →
//! ConnectorInputPlan → ExecutionContext` 迁移到
//! `AgentFrame → RuntimeLaunchRequest → ExecutionContext`。
//!
//! 本步建立投影骨架，不修改现有 connector 代码。

use agentdash_domain::workflow::{AgentFrame, AgentProcedureRef};
use uuid::Uuid;

/// 从 AgentFrame 投影出的 runtime adapter 请求。
///
/// connector 通过此结构获取启动所需的全部 surface 数据，
/// 不再从 session / business owner 反查。
#[derive(Debug, Clone)]
pub struct RuntimeLaunchRequest {
    pub agent_id: Uuid,
    pub frame_id: Uuid,
    pub frame_revision: i32,
    pub procedure_ref: Option<AgentProcedureRef>,
    pub capability_surface: serde_json::Value,
    pub context_slice: serde_json::Value,
    pub vfs_surface: serde_json::Value,
    pub mcp_surface: serde_json::Value,
    pub runtime_session_id: Option<Uuid>,
    pub graph_instance_id: Option<Uuid>,
    pub activity_key: Option<String>,
}

impl RuntimeLaunchRequest {
    /// 从一个 AgentFrame revision 投影出 launch request。
    ///
    /// JSON 字段 fallback 到 `serde_json::Value::Null`，
    /// connector 侧按需做 nullable 检查。
    pub fn from_frame(frame: &AgentFrame) -> Self {
        let runtime_session_id = frame
            .runtime_session_refs_json
            .as_ref()
            .and_then(|v| v.as_array())
            .and_then(|arr| arr.first())
            .and_then(|v| v.as_str())
            .and_then(|s| Uuid::parse_str(s).ok());

        let procedure_ref = frame.procedure_id.map(AgentProcedureRef::ById);

        Self {
            agent_id: frame.agent_id,
            frame_id: frame.id,
            frame_revision: frame.revision,
            procedure_ref,
            capability_surface: frame
                .effective_capability_json
                .clone()
                .unwrap_or(serde_json::Value::Null),
            context_slice: frame
                .context_slice_json
                .clone()
                .unwrap_or(serde_json::Value::Null),
            vfs_surface: frame
                .vfs_surface_json
                .clone()
                .unwrap_or(serde_json::Value::Null),
            mcp_surface: frame
                .mcp_surface_json
                .clone()
                .unwrap_or(serde_json::Value::Null),
            runtime_session_id,
            graph_instance_id: frame.graph_instance_id,
            activity_key: frame.activity_key.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentdash_domain::workflow::AgentFrame;

    #[test]
    fn from_frame_projects_all_fields() {
        let agent_id = Uuid::new_v4();
        let proc_id = Uuid::new_v4();
        let gi_id = Uuid::new_v4();
        let session_id = Uuid::new_v4();

        let mut frame = AgentFrame::new_revision(agent_id, 3, "test");
        frame.procedure_id = Some(proc_id);
        frame.graph_instance_id = Some(gi_id);
        frame.activity_key = Some("implement".to_string());
        frame.effective_capability_json = Some(serde_json::json!({"file_read": true}));
        frame.context_slice_json = Some(serde_json::json!({"project": "demo"}));
        frame.vfs_surface_json = Some(serde_json::json!({"mounts": []}));
        frame.mcp_surface_json = Some(serde_json::json!({"servers": []}));
        frame.runtime_session_refs_json = Some(serde_json::json!([session_id.to_string()]));

        let request = RuntimeLaunchRequest::from_frame(&frame);

        assert_eq!(request.agent_id, agent_id);
        assert_eq!(request.frame_id, frame.id);
        assert_eq!(request.frame_revision, 3);
        assert_eq!(request.graph_instance_id, Some(gi_id));
        assert_eq!(request.activity_key.as_deref(), Some("implement"));
        assert_eq!(request.runtime_session_id, Some(session_id));
        assert!(matches!(
            request.procedure_ref,
            Some(AgentProcedureRef::ById(id)) if id == proc_id
        ));
        assert_eq!(
            request.capability_surface,
            serde_json::json!({"file_read": true})
        );
        assert_eq!(
            request.context_slice,
            serde_json::json!({"project": "demo"})
        );
    }

    #[test]
    fn from_frame_handles_empty_fields() {
        let agent_id = Uuid::new_v4();
        let frame = AgentFrame::new_initial(agent_id, None);

        let request = RuntimeLaunchRequest::from_frame(&frame);

        assert_eq!(request.agent_id, agent_id);
        assert_eq!(request.frame_revision, 1);
        assert!(request.procedure_ref.is_none());
        assert!(request.runtime_session_id.is_none());
        assert!(request.graph_instance_id.is_none());
        assert!(request.activity_key.is_none());
        assert!(request.capability_surface.is_null());
        assert!(request.context_slice.is_null());
        assert!(request.vfs_surface.is_null());
        assert!(request.mcp_surface.is_null());
    }

    #[test]
    fn from_frame_picks_first_session_ref() {
        let agent_id = Uuid::new_v4();
        let s1 = Uuid::new_v4();
        let s2 = Uuid::new_v4();
        let mut frame = AgentFrame::new_revision(agent_id, 2, "test");
        frame.runtime_session_refs_json =
            Some(serde_json::json!([s1.to_string(), s2.to_string()]));

        let request = RuntimeLaunchRequest::from_frame(&frame);
        assert_eq!(request.runtime_session_id, Some(s1));
    }
}
