use std::collections::{BTreeMap, BTreeSet};

use agentdash_domain::permission::{PermissionGrant, PermissionGrantRepository};
use agentdash_domain::workflow::{
    AgentFrame, RuntimeSessionExecutionAnchorRepository, ToolCapabilityPath,
};
use agentdash_spi::platform::tool_capability::capability_to_tool_clusters;
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentRunGrantEffectClass {
    AdmissionProjection,
    AgentFrameSurfaceRevision,
}

/// AgentRun-scoped 授权/护栏投影。
///
/// 只有工具级 Grant 会进入这里作为工具执行准入。能力级 / MCP server 级 Grant
/// 会写入新的 AgentFrame revision，并由 frame 的 capability surface 表达模型可见效果。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AgentRunGrantProjection {
    admitted_tools: BTreeMap<String, BTreeSet<String>>,
}

impl AgentRunGrantProjection {
    pub fn empty() -> Self {
        Self::default()
    }

    pub fn is_empty(&self) -> bool {
        self.admitted_tools.is_empty()
    }

    pub fn from_active_grants(grants: &[PermissionGrant]) -> Self {
        let mut projection = Self::empty();
        for grant in grants.iter().filter(|grant| grant.status.is_active()) {
            projection.add_admission_paths(&grant.requested_paths);
        }
        projection
    }

    pub fn classify_path(path: &ToolCapabilityPath) -> AgentRunGrantEffectClass {
        if path.tool.is_some() {
            AgentRunGrantEffectClass::AdmissionProjection
        } else {
            AgentRunGrantEffectClass::AgentFrameSurfaceRevision
        }
    }

    pub fn partition_paths(
        paths: &[ToolCapabilityPath],
    ) -> (Vec<ToolCapabilityPath>, Vec<ToolCapabilityPath>) {
        let mut admission_paths = Vec::new();
        let mut surface_paths = Vec::new();

        for path in paths {
            match Self::classify_path(path) {
                AgentRunGrantEffectClass::AdmissionProjection => {
                    admission_paths.push(path.clone());
                }
                AgentRunGrantEffectClass::AgentFrameSurfaceRevision => {
                    surface_paths.push(path.clone());
                }
            }
        }

        (admission_paths, surface_paths)
    }

    pub fn add_admission_paths(&mut self, paths: &[ToolCapabilityPath]) {
        for path in paths {
            if let Some(tool) = &path.tool {
                self.admitted_tools
                    .entry(path.capability.clone())
                    .or_default()
                    .insert(tool.clone());
            }
        }
    }

    pub fn admits_tool(&self, request: &AgentRunAdmissionRequest) -> bool {
        self.admitted_tools
            .get(&request.capability_key)
            .is_some_and(|tools| tools.contains(&request.tool_name))
    }

    pub fn apply_to_execution_capability_state(&self, state: &CapabilityState) -> CapabilityState {
        if self.is_empty() {
            return state.clone();
        }

        let mut next = state.clone();
        for (capability_key, tool_names) in &self.admitted_tools {
            let capability = ToolCapability::new(capability_key.clone());
            let capability_was_visible = next.tool.capabilities.contains(&capability);
            next.tool.capabilities.insert(capability.clone());
            next.tool
                .enabled_clusters
                .extend(capability_to_tool_clusters(&capability));

            if !capability_was_visible {
                next.tool
                    .tool_policy
                    .entry(capability_key.clone())
                    .or_default()
                    .include_only
                    .extend(tool_names.iter().cloned());
                continue;
            }

            if let Some(filter) = next.tool.tool_policy.get_mut(capability_key) {
                for tool_name in tool_names {
                    filter.exclude.remove(tool_name);
                    if !filter.include_only.is_empty() {
                        filter.include_only.insert(tool_name.clone());
                    }
                }
                if filter.is_empty() {
                    next.tool.tool_policy.remove(capability_key);
                }
            }
        }
        next
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
    pub grant_projection: AgentRunGrantProjection,
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
        grant_projection: &AgentRunGrantProjection,
    ) -> AgentRunEffectiveCapabilityView {
        let capability_state = project_capability_state_from_frame(frame);
        Self::effective_view_from_state_with_projection(
            target,
            capability_state,
            frame.visible_workspace_module_refs(),
            grant_projection.clone(),
        )
    }

    pub fn effective_view_from_state(
        target: AgentFrameRuntimeTarget,
        capability_state: CapabilityState,
        visible_workspace_module_refs: Vec<String>,
    ) -> AgentRunEffectiveCapabilityView {
        Self::effective_view_from_state_with_projection(
            target,
            capability_state,
            visible_workspace_module_refs,
            AgentRunGrantProjection::empty(),
        )
    }

    pub fn effective_view_from_state_with_projection(
        target: AgentFrameRuntimeTarget,
        capability_state: CapabilityState,
        visible_workspace_module_refs: Vec<String>,
        grant_projection: AgentRunGrantProjection,
    ) -> AgentRunEffectiveCapabilityView {
        AgentRunEffectiveCapabilityView {
            target,
            visible_capabilities: capability_state.tool.capabilities.clone(),
            vfs_surface: capability_state.vfs.active.clone().unwrap_or_default(),
            mcp_surface: capability_state.tool.mcp_servers.clone(),
            capability_state,
            visible_workspace_module_refs,
            grant_projection,
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

        if view.grant_projection.admits_tool(request) {
            return AgentRunAdmissionDecision::allow();
        }

        AgentRunAdmissionDecision::deny(format!(
            "tool `{}` is not admitted for capability `{}`",
            request.tool_name, request.capability_key
        ))
    }

    pub async fn execution_capability_state_for_runtime_session(
        runtime_session_id: &str,
        base_state: &CapabilityState,
        execution_anchor_repo: &dyn RuntimeSessionExecutionAnchorRepository,
        permission_grant_repo: &dyn PermissionGrantRepository,
    ) -> Result<CapabilityState, agentdash_domain::DomainError> {
        let Some(anchor) = execution_anchor_repo
            .find_by_session(runtime_session_id)
            .await?
        else {
            return Ok(base_state.clone());
        };
        let active_grants = permission_grant_repo
            .list_active_by_run(anchor.run_id)
            .await?;
        let projection = AgentRunGrantProjection::from_active_grants(&active_grants);
        Ok(projection.apply_to_execution_capability_state(base_state))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentdash_domain::permission::{GrantScope, PermissionGrant};
    use agentdash_domain::workflow::ToolCapabilityPath;
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

    #[test]
    fn grant_projection_admits_tool_without_visible_capability_change() {
        let path = ToolCapabilityPath::parse("workflow_management::upsert_workflow_tool")
            .expect("tool path");
        let mut grant = PermissionGrant::new(
            Uuid::new_v4(),
            "session-a",
            vec![path],
            "temporary tool admission",
            GrantScope::AgentFrame,
            None,
        );
        grant.submit_for_policy().expect("submit");
        grant
            .apply_policy_decision(agentdash_domain::permission::PolicyDecision {
                outcome: agentdash_domain::permission::PolicyOutcome::AutoApproved,
                matched_rules: Vec::new(),
                reason: "auto".to_string(),
            })
            .expect("policy");
        grant.mark_applied().expect("applied");

        let projection = AgentRunGrantProjection::from_active_grants(&[grant]);
        let frame = frame_with_state(&CapabilityState::default());
        let view = AgentRunEffectiveCapabilityService::effective_view_from_frame_with_projection(
            target(),
            &frame,
            &projection,
        );

        assert!(
            !view
                .visible_capabilities
                .contains(&ToolCapability::new("workflow_management")),
            "tool-internal grants must not expand the model-visible toolset"
        );
        let decision = AgentRunEffectiveCapabilityService::admit_tool(
            &view,
            &AgentRunAdmissionRequest::tool(
                "workflow_management",
                "upsert_workflow_tool",
                Some(ToolCluster::Workflow),
            ),
        );
        assert!(decision.allowed);
    }

    #[test]
    fn grant_projection_expands_execution_state_for_tool_surface_only() {
        let path = ToolCapabilityPath::parse("workflow_management::upsert_workflow_tool")
            .expect("tool path");
        let mut grant = PermissionGrant::new(
            Uuid::new_v4(),
            "session-a",
            vec![path],
            "temporary tool admission",
            GrantScope::AgentFrame,
            None,
        );
        grant.submit_for_policy().expect("submit");
        grant
            .apply_policy_decision(agentdash_domain::permission::PolicyDecision {
                outcome: agentdash_domain::permission::PolicyOutcome::AutoApproved,
                matched_rules: Vec::new(),
                reason: "auto".to_string(),
            })
            .expect("policy");
        grant.mark_applied().expect("applied");

        let projection = AgentRunGrantProjection::from_active_grants(&[grant]);
        let execution_state =
            projection.apply_to_execution_capability_state(&CapabilityState::default());

        assert!(execution_state.is_capability_tool_enabled(
            "workflow_management",
            "upsert_workflow_tool",
            None
        ));
        assert!(!execution_state.is_capability_tool_enabled(
            "workflow_management",
            "get_workflow",
            None
        ));
    }

    #[test]
    fn grant_projection_classifies_tool_paths_separately_from_surface_paths() {
        let paths = vec![
            ToolCapabilityPath::parse("workflow_management").expect("cap path"),
            ToolCapabilityPath::parse("workflow_management::upsert_workflow_tool")
                .expect("tool path"),
        ];

        let (admission_paths, surface_paths) = AgentRunGrantProjection::partition_paths(&paths);

        assert_eq!(admission_paths.len(), 1);
        assert_eq!(
            admission_paths[0].to_qualified_string(),
            "workflow_management::upsert_workflow_tool"
        );
        assert_eq!(surface_paths.len(), 1);
        assert_eq!(
            surface_paths[0].to_qualified_string(),
            "workflow_management"
        );
    }
}
