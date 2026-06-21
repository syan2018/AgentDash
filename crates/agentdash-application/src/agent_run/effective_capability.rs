use std::collections::BTreeSet;

use agentdash_domain::workflow::AgentFrame;
use agentdash_spi::{CapabilityState, RuntimeMcpServer, ToolCapability, ToolCluster, Vfs};
use uuid::Uuid;

use crate::session::{AgentFrameRuntimeTarget, project_capability_state_from_frame};

/// AgentRun runtime capability/admission 的入口请求标识。
///
/// 当前 CE05 只建立边界类型；后续 CE02/CE04 可以在服务实现中补齐 run/agent
/// 坐标选择与 Grant projection。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentRunEffectiveCapabilityRequest {
    pub agent_run_id: Uuid,
    pub agent_id: Uuid,
    pub command_key: Option<String>,
}

/// AgentRun-scoped 授权/护栏投影。
///
/// CE05 先保留空投影，确保 Grant state 的唯一消费点已经位于 AgentRun
/// boundary；CE02 在这里补齐 approve/revoke/expire 分类后的 admission/toolset
/// projection。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AgentRunGrantProjection;

impl AgentRunGrantProjection {
    pub fn empty() -> Self {
        Self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentRunEffectiveCapabilityView {
    pub target: AgentFrameRuntimeTarget,
    pub capability_state: CapabilityState,
    pub visible_capabilities: BTreeSet<ToolCapability>,
    pub vfs_surface: Vfs,
    pub mcp_surface: Vec<RuntimeMcpServer>,
    pub visible_workspace_module_refs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentRunAdmissionRequest {
    pub capability_key: String,
    pub tool_name: String,
    pub cluster: Option<ToolCluster>,
}

impl AgentRunAdmissionRequest {
    pub fn tool(
        capability_key: impl Into<String>,
        tool_name: impl Into<String>,
        cluster: Option<ToolCluster>,
    ) -> Self {
        Self {
            capability_key: capability_key.into(),
            tool_name: tool_name.into(),
            cluster,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentRunAdmissionDecision {
    pub allowed: bool,
    pub reason: Option<String>,
}

impl AgentRunAdmissionDecision {
    pub fn allow() -> Self {
        Self {
            allowed: true,
            reason: None,
        }
    }

    pub fn deny(reason: impl Into<String>) -> Self {
        Self {
            allowed: false,
            reason: Some(reason.into()),
        }
    }
}

pub struct AgentRunEffectiveCapabilityService;

impl AgentRunEffectiveCapabilityService {
    pub fn effective_view_from_frame(
        target: AgentFrameRuntimeTarget,
        frame: &AgentFrame,
    ) -> AgentRunEffectiveCapabilityView {
        Self::effective_view_from_frame_with_projection(
            target,
            frame,
            &AgentRunGrantProjection::empty(),
        )
    }

    pub fn effective_view_from_frame_with_projection(
        target: AgentFrameRuntimeTarget,
        frame: &AgentFrame,
        _grant_projection: &AgentRunGrantProjection,
    ) -> AgentRunEffectiveCapabilityView {
        let capability_state = project_capability_state_from_frame(frame);
        Self::effective_view_from_state(
            target,
            capability_state,
            frame.visible_workspace_module_refs(),
        )
    }

    pub fn effective_view_from_state(
        target: AgentFrameRuntimeTarget,
        capability_state: CapabilityState,
        visible_workspace_module_refs: Vec<String>,
    ) -> AgentRunEffectiveCapabilityView {
        AgentRunEffectiveCapabilityView {
            target,
            visible_capabilities: capability_state.tool.capabilities.clone(),
            vfs_surface: capability_state.vfs.active.clone().unwrap_or_default(),
            mcp_surface: capability_state.tool.mcp_servers.clone(),
            capability_state,
            visible_workspace_module_refs,
        }
    }

    pub fn admit_tool(
        view: &AgentRunEffectiveCapabilityView,
        request: &AgentRunAdmissionRequest,
    ) -> AgentRunAdmissionDecision {
        if view.capability_state.is_capability_tool_enabled(
            &request.capability_key,
            &request.tool_name,
            request.cluster,
        ) {
            return AgentRunAdmissionDecision::allow();
        }

        AgentRunAdmissionDecision::deny(format!(
            "tool `{}` is not admitted for capability `{}`",
            request.tool_name, request.capability_key
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentdash_spi::ToolCapabilityFilter;

    fn target() -> AgentFrameRuntimeTarget {
        AgentFrameRuntimeTarget {
            frame_id: Uuid::new_v4(),
            delivery_runtime_session_id: "session-a".to_string(),
        }
    }

    fn frame_with_state(state: &CapabilityState) -> AgentFrame {
        let mut frame = AgentFrame::new_revision(Uuid::new_v4(), 1, "test");
        frame.effective_capability_json = serde_json::to_value(state).ok();
        frame
    }

    #[test]
    fn effective_view_projects_selected_frame_surface() {
        let mut state = CapabilityState::default();
        state
            .tool
            .capabilities
            .insert(ToolCapability::new("workflow_management"));
        let mut frame = frame_with_state(&state);
        frame.append_visible_workspace_module_ref("canvas:dashboard");
        let target = target();

        let view =
            AgentRunEffectiveCapabilityService::effective_view_from_frame(target.clone(), &frame);

        assert_eq!(view.target, target);
        assert!(
            view.visible_capabilities
                .contains(&ToolCapability::new("workflow_management"))
        );
        assert_eq!(
            view.visible_workspace_module_refs,
            vec!["canvas:dashboard".to_string()]
        );
    }

    #[test]
    fn admission_uses_tool_policy_from_agent_run_view() {
        let mut state = CapabilityState::default();
        state
            .tool
            .capabilities
            .insert(ToolCapability::new("workflow_management"));
        state.tool.enabled_clusters.insert(ToolCluster::Workflow);
        state.tool.tool_policy.insert(
            "workflow_management".to_string(),
            ToolCapabilityFilter {
                include_only: BTreeSet::from(["get_workflow".to_string()]),
                exclude: BTreeSet::new(),
            },
        );
        let frame = frame_with_state(&state);
        let view = AgentRunEffectiveCapabilityService::effective_view_from_frame(target(), &frame);

        let allowed = AgentRunEffectiveCapabilityService::admit_tool(
            &view,
            &AgentRunAdmissionRequest::tool(
                "workflow_management",
                "get_workflow",
                Some(ToolCluster::Workflow),
            ),
        );
        let denied = AgentRunEffectiveCapabilityService::admit_tool(
            &view,
            &AgentRunAdmissionRequest::tool(
                "workflow_management",
                "upsert_workflow_tool",
                Some(ToolCluster::Workflow),
            ),
        );

        assert!(allowed.allowed);
        assert!(!denied.allowed);
        assert!(
            denied
                .reason
                .as_deref()
                .is_some_and(|reason| reason.contains("upsert_workflow_tool"))
        );
    }
}
