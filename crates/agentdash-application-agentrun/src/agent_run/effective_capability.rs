use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use agentdash_application_ports::agent_run_surface as ports_agent_run_surface;
use agentdash_application_ports::runtime_session_live::{
    RuntimeSessionEffectiveCapabilityPort, RuntimeSessionLivePortError,
};
use agentdash_domain::permission::{PermissionGrant, PermissionGrantRepository};
use agentdash_domain::workflow::{
    AgentFrame, AgentFrameRepository, RuntimeSessionExecutionAnchor,
    RuntimeSessionExecutionAnchorRepository, ToolCapabilityPath,
};
use agentdash_spi::{CapabilityState, RuntimeMcpServer, ToolCapability, ToolCluster, Vfs};
use async_trait::async_trait;

use crate::agent_run::AgentFrameRuntimeTarget;
use crate::agent_run::runtime_capability::project_capability_state_from_frame;

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

impl From<AgentRunGrantProjection> for ports_agent_run_surface::AgentRunGrantProjection {
    fn from(value: AgentRunGrantProjection) -> Self {
        Self {
            admitted_tools: value.admitted_tools,
        }
    }
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

impl From<AgentRunEffectiveCapabilityView>
    for ports_agent_run_surface::AgentRunEffectiveCapabilityView
{
    fn from(value: AgentRunEffectiveCapabilityView) -> Self {
        Self {
            target: value.target,
            capability_state: value.capability_state,
            visible_capabilities: value.visible_capabilities,
            vfs_surface: value.vfs_surface,
            mcp_surface: value.mcp_surface,
            visible_workspace_module_refs: value.visible_workspace_module_refs,
            grant_projection: value.grant_projection.into(),
        }
    }
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

    pub async fn schema_visible_capability_state_for_runtime_session(
        runtime_session_id: &str,
        base_state: &CapabilityState,
        execution_anchor_repo: &dyn RuntimeSessionExecutionAnchorRepository,
        agent_frame_repo: &dyn AgentFrameRepository,
        permission_grant_repo: &dyn PermissionGrantRepository,
    ) -> Result<CapabilityState, agentdash_domain::DomainError> {
        let _projection = Self::grant_projection_for_runtime_session(
            runtime_session_id,
            execution_anchor_repo,
            agent_frame_repo,
            permission_grant_repo,
        )
        .await?;
        Ok(base_state.clone())
    }

    pub async fn grant_projection_for_runtime_session(
        runtime_session_id: &str,
        execution_anchor_repo: &dyn RuntimeSessionExecutionAnchorRepository,
        agent_frame_repo: &dyn AgentFrameRepository,
        permission_grant_repo: &dyn PermissionGrantRepository,
    ) -> Result<AgentRunGrantProjection, agentdash_domain::DomainError> {
        let Some(anchor) = execution_anchor_repo
            .find_by_session(runtime_session_id)
            .await?
        else {
            return Ok(AgentRunGrantProjection::empty());
        };
        let Some(current_frame) = agent_frame_repo.get_current(anchor.agent_id).await? else {
            return Ok(AgentRunGrantProjection::empty());
        };
        Self::grant_projection_for_effect_frame(current_frame.id, permission_grant_repo).await
    }

    pub async fn grant_projection_for_effect_frame(
        effect_frame_id: uuid::Uuid,
        permission_grant_repo: &dyn PermissionGrantRepository,
    ) -> Result<AgentRunGrantProjection, agentdash_domain::DomainError> {
        let active_grants = permission_grant_repo
            .list_active_by_frame(effect_frame_id)
            .await?;
        Ok(AgentRunGrantProjection::from_active_grants(&active_grants))
    }
}

#[derive(Clone)]
pub struct AgentRunEffectiveCapabilityAdapter {
    execution_anchor_repo: Arc<dyn RuntimeSessionExecutionAnchorRepository>,
    agent_frame_repo: Arc<dyn AgentFrameRepository>,
    permission_grant_repo: Arc<dyn PermissionGrantRepository>,
}

impl AgentRunEffectiveCapabilityAdapter {
    pub fn new(
        execution_anchor_repo: Arc<dyn RuntimeSessionExecutionAnchorRepository>,
        agent_frame_repo: Arc<dyn AgentFrameRepository>,
        permission_grant_repo: Arc<dyn PermissionGrantRepository>,
    ) -> Self {
        Self {
            execution_anchor_repo,
            agent_frame_repo,
            permission_grant_repo,
        }
    }

    async fn anchor_for_runtime_session(
        &self,
        runtime_session_id: &str,
    ) -> Result<
        RuntimeSessionExecutionAnchor,
        ports_agent_run_surface::AgentRunEffectiveCapabilityError,
    > {
        self.execution_anchor_repo
            .find_by_session(runtime_session_id)
            .await
            .map_err(|error| {
                ports_agent_run_surface::AgentRunEffectiveCapabilityError::Repository {
                    operation: "runtime session execution anchor",
                    message: error.to_string(),
                }
            })?
            .ok_or_else(|| {
                ports_agent_run_surface::AgentRunEffectiveCapabilityError::MissingRuntimeSession {
                    runtime_session_id: runtime_session_id.to_string(),
                }
            })
    }

    async fn effective_view_for_anchor(
        &self,
        anchor: RuntimeSessionExecutionAnchor,
    ) -> Result<
        AgentRunEffectiveCapabilityView,
        ports_agent_run_surface::AgentRunEffectiveCapabilityError,
    > {
        let frame = self
            .agent_frame_repo
            .get_current(anchor.agent_id)
            .await
            .map_err(|error| {
                ports_agent_run_surface::AgentRunEffectiveCapabilityError::Repository {
                    operation: "current AgentFrame",
                    message: error.to_string(),
                }
            })?
            .ok_or(
                ports_agent_run_surface::AgentRunEffectiveCapabilityError::MissingTarget {
                    run_id: anchor.run_id,
                    agent_id: anchor.agent_id,
                },
            )?;

        if frame.agent_id != anchor.agent_id {
            return Err(
                ports_agent_run_surface::AgentRunEffectiveCapabilityError::Projection {
                    message: format!(
                        "current AgentFrame agent mismatch: expected={}, actual={}",
                        anchor.agent_id, frame.agent_id
                    ),
                },
            );
        }

        let grant_projection =
            AgentRunEffectiveCapabilityService::grant_projection_for_effect_frame(
                frame.id,
                self.permission_grant_repo.as_ref(),
            )
            .await
            .map_err(|error| {
                ports_agent_run_surface::AgentRunEffectiveCapabilityError::Repository {
                    operation: "active permission grants by frame",
                    message: error.to_string(),
                }
            })?;

        Ok(
            AgentRunEffectiveCapabilityService::effective_view_from_frame_with_projection(
                AgentFrameRuntimeTarget {
                    frame_id: frame.id,
                    delivery_runtime_session_id: anchor.runtime_session_id,
                },
                &frame,
                &grant_projection,
            ),
        )
    }

    async fn effective_view_for_runtime_session(
        &self,
        runtime_session_id: &str,
    ) -> Result<
        AgentRunEffectiveCapabilityView,
        ports_agent_run_surface::AgentRunEffectiveCapabilityError,
    > {
        let anchor = self.anchor_for_runtime_session(runtime_session_id).await?;
        self.effective_view_for_anchor(anchor).await
    }
}

pub fn agent_run_effective_capability_port(
    execution_anchor_repo: Arc<dyn RuntimeSessionExecutionAnchorRepository>,
    agent_frame_repo: Arc<dyn AgentFrameRepository>,
    permission_grant_repo: Arc<dyn PermissionGrantRepository>,
) -> Arc<dyn ports_agent_run_surface::AgentRunEffectiveCapabilityPort> {
    Arc::new(AgentRunEffectiveCapabilityAdapter::new(
        execution_anchor_repo,
        agent_frame_repo,
        permission_grant_repo,
    ))
}

#[async_trait]
impl ports_agent_run_surface::AgentRunEffectiveCapabilityPort
    for AgentRunEffectiveCapabilityAdapter
{
    async fn effective_capability(
        &self,
        request: ports_agent_run_surface::AgentRunEffectiveCapabilityRequest,
    ) -> Result<
        ports_agent_run_surface::AgentRunEffectiveCapabilityView,
        ports_agent_run_surface::AgentRunEffectiveCapabilityError,
    > {
        let view = self
            .effective_view_for_runtime_session(&request.runtime_session_id)
            .await?;
        if view.target.delivery_runtime_session_id != request.runtime_session_id {
            return Err(
                ports_agent_run_surface::AgentRunEffectiveCapabilityError::Projection {
                    message: format!(
                        "runtime session mismatch: expected={}, actual={}",
                        request.runtime_session_id, view.target.delivery_runtime_session_id
                    ),
                },
            );
        }

        let anchor = self
            .anchor_for_runtime_session(&request.runtime_session_id)
            .await?;
        if anchor.run_id != request.agent_run_id || anchor.agent_id != request.agent_id {
            return Err(
                ports_agent_run_surface::AgentRunEffectiveCapabilityError::MissingTarget {
                    run_id: request.agent_run_id,
                    agent_id: request.agent_id,
                },
            );
        }

        Ok(view.into())
    }

    async fn admit_tool(
        &self,
        request: ports_agent_run_surface::AgentRunAdmissionRequest,
    ) -> Result<
        ports_agent_run_surface::AgentRunAdmissionDecision,
        ports_agent_run_surface::AgentRunEffectiveCapabilityError,
    > {
        let view = self
            .effective_view_for_runtime_session(&request.runtime_session_id)
            .await?;
        let decision = AgentRunEffectiveCapabilityService::admit_tool(
            &view,
            &AgentRunAdmissionRequest::tool(
                request.capability_key,
                request.tool_name,
                request.cluster,
            ),
        );
        Ok(ports_agent_run_surface::AgentRunAdmissionDecision {
            allowed: decision.allowed,
            reason: decision.reason,
        })
    }
}

#[derive(Clone)]
pub struct AgentRunRuntimeSessionEffectiveCapabilityAdapter {
    execution_anchor_repo: Arc<dyn RuntimeSessionExecutionAnchorRepository>,
    agent_frame_repo: Arc<dyn AgentFrameRepository>,
    permission_grant_repo: Arc<dyn PermissionGrantRepository>,
}

impl AgentRunRuntimeSessionEffectiveCapabilityAdapter {
    pub fn new(
        execution_anchor_repo: Arc<dyn RuntimeSessionExecutionAnchorRepository>,
        agent_frame_repo: Arc<dyn AgentFrameRepository>,
        permission_grant_repo: Arc<dyn PermissionGrantRepository>,
    ) -> Self {
        Self {
            execution_anchor_repo,
            agent_frame_repo,
            permission_grant_repo,
        }
    }
}

pub fn runtime_session_effective_capability_port(
    execution_anchor_repo: Arc<dyn RuntimeSessionExecutionAnchorRepository>,
    agent_frame_repo: Arc<dyn AgentFrameRepository>,
    permission_grant_repo: Arc<dyn PermissionGrantRepository>,
) -> Arc<dyn RuntimeSessionEffectiveCapabilityPort> {
    Arc::new(AgentRunRuntimeSessionEffectiveCapabilityAdapter::new(
        execution_anchor_repo,
        agent_frame_repo,
        permission_grant_repo,
    ))
}

#[async_trait]
impl RuntimeSessionEffectiveCapabilityPort for AgentRunRuntimeSessionEffectiveCapabilityAdapter {
    async fn schema_visible_capability_state_for_runtime_session(
        &self,
        runtime_session_id: &str,
        base_state: CapabilityState,
    ) -> Result<CapabilityState, RuntimeSessionLivePortError> {
        AgentRunEffectiveCapabilityService::schema_visible_capability_state_for_runtime_session(
            runtime_session_id,
            &base_state,
            self.execution_anchor_repo.as_ref(),
            self.agent_frame_repo.as_ref(),
            self.permission_grant_repo.as_ref(),
        )
        .await
        .map_err(|error| RuntimeSessionLivePortError::failed(error.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::MemoryAgentFrameRepository;
    use agentdash_application_ports::agent_run_surface::AgentRunEffectiveCapabilityPort as _;
    use agentdash_domain::DomainError;
    use agentdash_domain::permission::{GrantScope, PermissionGrant, PermissionGrantStatusFilter};
    use agentdash_domain::workflow::{
        RuntimeSessionExecutionAnchor, RuntimeSessionExecutionAnchorRepository, ToolCapabilityPath,
    };
    use agentdash_spi::ToolCapabilityFilter;
    use tokio::sync::Mutex;
    use uuid::Uuid;

    fn target() -> AgentFrameRuntimeTarget {
        AgentFrameRuntimeTarget {
            frame_id: Uuid::new_v4(),
            delivery_runtime_session_id: "session-a".to_string(),
        }
    }

    fn frame_with_state(state: &CapabilityState) -> AgentFrame {
        frame_with_agent_state(Uuid::new_v4(), state)
    }

    fn frame_with_agent_state(agent_id: Uuid, state: &CapabilityState) -> AgentFrame {
        let mut frame = AgentFrame::new_revision(agent_id, 1, "test");
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
    fn grant_projection_does_not_expand_execution_capability_state() {
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
            !view.capability_state.is_capability_tool_enabled(
                "workflow_management",
                "upsert_workflow_tool",
                None
            ),
            "tool-level grants must not alter schema-facing CapabilityState"
        );
        assert!(
            AgentRunEffectiveCapabilityService::admit_tool(
                &view,
                &AgentRunAdmissionRequest::tool(
                    "workflow_management",
                    "upsert_workflow_tool",
                    Some(ToolCluster::Workflow),
                ),
            )
            .allowed
        );
        assert!(!view.capability_state.is_capability_tool_enabled(
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

    #[derive(Default)]
    struct MemoryGrantRepository {
        grants: Mutex<Vec<PermissionGrant>>,
        active_frame_queries: Mutex<Vec<Uuid>>,
        active_run_queries: Mutex<Vec<Uuid>>,
    }

    impl MemoryGrantRepository {
        async fn insert(&self, grant: PermissionGrant) {
            self.grants.lock().await.push(grant);
        }

        async fn active_frame_queries(&self) -> Vec<Uuid> {
            self.active_frame_queries.lock().await.clone()
        }

        async fn active_run_queries(&self) -> Vec<Uuid> {
            self.active_run_queries.lock().await.clone()
        }
    }

    #[async_trait]
    impl PermissionGrantRepository for MemoryGrantRepository {
        async fn create(&self, grant: &PermissionGrant) -> Result<(), DomainError> {
            self.insert(grant.clone()).await;
            Ok(())
        }

        async fn update(&self, grant: &PermissionGrant) -> Result<(), DomainError> {
            let mut grants = self.grants.lock().await;
            if let Some(existing) = grants.iter_mut().find(|item| item.id == grant.id) {
                *existing = grant.clone();
            }
            Ok(())
        }

        async fn find_by_id(&self, id: Uuid) -> Result<Option<PermissionGrant>, DomainError> {
            Ok(self
                .grants
                .lock()
                .await
                .iter()
                .find(|grant| grant.id == id)
                .cloned())
        }

        async fn list_by_frame(
            &self,
            effect_frame_id: Uuid,
            status_filter: Option<PermissionGrantStatusFilter>,
        ) -> Result<Vec<PermissionGrant>, DomainError> {
            Ok(self
                .grants
                .lock()
                .await
                .iter()
                .filter(|grant| grant.effect_frame_id == Some(effect_frame_id))
                .filter(|grant| status_filter.is_none_or(|filter| status_matches(grant, filter)))
                .cloned()
                .collect())
        }

        async fn list_by_run(
            &self,
            run_id: Uuid,
            status_filter: Option<PermissionGrantStatusFilter>,
        ) -> Result<Vec<PermissionGrant>, DomainError> {
            Ok(self
                .grants
                .lock()
                .await
                .iter()
                .filter(|grant| grant.run_id == run_id)
                .filter(|grant| status_filter.is_none_or(|filter| status_matches(grant, filter)))
                .cloned()
                .collect())
        }

        async fn list_active_by_frame(
            &self,
            effect_frame_id: Uuid,
        ) -> Result<Vec<PermissionGrant>, DomainError> {
            self.active_frame_queries.lock().await.push(effect_frame_id);
            self.list_by_frame(effect_frame_id, Some(PermissionGrantStatusFilter::Active))
                .await
        }

        async fn list_active_by_run(
            &self,
            run_id: Uuid,
        ) -> Result<Vec<PermissionGrant>, DomainError> {
            self.active_run_queries.lock().await.push(run_id);
            self.list_by_run(run_id, Some(PermissionGrantStatusFilter::Active))
                .await
        }

        async fn find_active_escalation_grant(
            &self,
            effect_frame_id: Uuid,
            target_subject_kind: &str,
        ) -> Result<Option<PermissionGrant>, DomainError> {
            Ok(self
                .list_active_by_frame(effect_frame_id)
                .await?
                .into_iter()
                .find(|grant| {
                    grant
                        .scope_escalation_intent
                        .as_ref()
                        .is_some_and(|intent| intent.target_subject_kind == target_subject_kind)
                }))
        }

        async fn list_overdue_active(
            &self,
            now: chrono::DateTime<chrono::Utc>,
        ) -> Result<Vec<PermissionGrant>, DomainError> {
            Ok(self
                .grants
                .lock()
                .await
                .iter()
                .filter(|grant| grant.status.is_active())
                .filter(|grant| grant.expires_at.is_some_and(|expires_at| expires_at < now))
                .cloned()
                .collect())
        }
    }

    fn status_matches(grant: &PermissionGrant, filter: PermissionGrantStatusFilter) -> bool {
        match filter {
            PermissionGrantStatusFilter::Exact(status) => grant.status == status,
            PermissionGrantStatusFilter::Pending => {
                grant.status == agentdash_domain::permission::GrantStatus::PendingPolicy
                    || grant.status
                        == agentdash_domain::permission::GrantStatus::PendingUserApproval
            }
            PermissionGrantStatusFilter::Active => grant.status.is_active(),
            PermissionGrantStatusFilter::Terminal => grant.status.is_terminal(),
        }
    }

    fn active_tool_grant(
        run_id: Uuid,
        session_id: &str,
        effect_frame_id: Uuid,
        tool_path: &str,
    ) -> PermissionGrant {
        let mut grant = PermissionGrant::new(
            run_id,
            session_id,
            vec![ToolCapabilityPath::parse(tool_path).expect("tool path")],
            "temporary tool admission",
            GrantScope::AgentFrame,
            None,
        )
        .with_effect_frame(effect_frame_id);
        grant.submit_for_policy().expect("submit");
        grant
            .apply_policy_decision(agentdash_domain::permission::PolicyDecision {
                outcome: agentdash_domain::permission::PolicyOutcome::AutoApproved,
                matched_rules: Vec::new(),
                reason: "auto".to_string(),
            })
            .expect("policy");
        grant.mark_applied().expect("applied");
        grant
    }

    #[derive(Default)]
    struct MemoryAnchorRepository {
        anchors: Mutex<Vec<RuntimeSessionExecutionAnchor>>,
    }

    #[async_trait]
    impl RuntimeSessionExecutionAnchorRepository for MemoryAnchorRepository {
        async fn create_once(
            &self,
            anchor: &RuntimeSessionExecutionAnchor,
        ) -> Result<(), DomainError> {
            let mut anchors = self.anchors.lock().await;
            if let Some(existing) = anchors
                .iter()
                .find(|item| item.runtime_session_id == anchor.runtime_session_id)
            {
                if existing.has_same_launch_coordinates_as(anchor) {
                    return Ok(());
                }
                return Err(existing.immutable_conflict(anchor));
            }
            anchors.push(anchor.clone());
            Ok(())
        }

        async fn delete_by_session(&self, runtime_session_id: &str) -> Result<(), DomainError> {
            self.anchors
                .lock()
                .await
                .retain(|anchor| anchor.runtime_session_id != runtime_session_id);
            Ok(())
        }

        async fn find_by_session(
            &self,
            runtime_session_id: &str,
        ) -> Result<Option<RuntimeSessionExecutionAnchor>, DomainError> {
            Ok(self
                .anchors
                .lock()
                .await
                .iter()
                .find(|anchor| anchor.runtime_session_id == runtime_session_id)
                .cloned())
        }

        async fn list_by_run(
            &self,
            run_id: Uuid,
        ) -> Result<Vec<RuntimeSessionExecutionAnchor>, DomainError> {
            Ok(self
                .anchors
                .lock()
                .await
                .iter()
                .filter(|anchor| anchor.run_id == run_id)
                .cloned()
                .collect())
        }

        async fn list_by_agent(
            &self,
            agent_id: Uuid,
        ) -> Result<Vec<RuntimeSessionExecutionAnchor>, DomainError> {
            Ok(self
                .anchors
                .lock()
                .await
                .iter()
                .filter(|anchor| anchor.agent_id == agent_id)
                .cloned()
                .collect())
        }

        async fn list_by_project_session_ids(
            &self,
            runtime_session_ids: &[String],
        ) -> Result<Vec<RuntimeSessionExecutionAnchor>, DomainError> {
            Ok(self
                .anchors
                .lock()
                .await
                .iter()
                .filter(|anchor| runtime_session_ids.contains(&anchor.runtime_session_id))
                .cloned()
                .collect())
        }
    }

    async fn insert_anchor(
        anchors: &MemoryAnchorRepository,
        runtime_session_id: &str,
        run_id: Uuid,
        frame_id: Uuid,
        agent_id: Uuid,
    ) {
        anchors
            .create_once(&RuntimeSessionExecutionAnchor::new_dispatch(
                runtime_session_id,
                run_id,
                frame_id,
                agent_id,
            ))
            .await
            .expect("anchor");
    }

    fn effective_capability_adapter(
        anchors: Arc<MemoryAnchorRepository>,
        frames: Arc<MemoryAgentFrameRepository>,
        grants: Arc<MemoryGrantRepository>,
    ) -> AgentRunEffectiveCapabilityAdapter {
        AgentRunEffectiveCapabilityAdapter::new(anchors, frames, grants)
    }

    #[tokio::test]
    async fn product_port_effective_capability_projects_visible_state_without_grant_mutation() {
        let run_id = Uuid::new_v4();
        let agent_id = Uuid::new_v4();
        let mut state = CapabilityState::default();
        state
            .tool
            .capabilities
            .insert(ToolCapability::new("file_read"));
        let mut frame = frame_with_agent_state(agent_id, &state);
        frame.append_visible_workspace_module_ref("canvas:overview");

        let anchors = Arc::new(MemoryAnchorRepository::default());
        let frames = Arc::new(MemoryAgentFrameRepository::default());
        let grants = Arc::new(MemoryGrantRepository::default());
        frames.create(&frame).await.expect("frame");
        insert_anchor(&anchors, "session-a", run_id, frame.id, agent_id).await;
        grants
            .insert(active_tool_grant(
                run_id,
                "session-a",
                frame.id,
                "workflow_management::upsert_workflow_tool",
            ))
            .await;

        let view = effective_capability_adapter(anchors, frames, grants.clone())
            .effective_capability(
                ports_agent_run_surface::AgentRunEffectiveCapabilityRequest::for_runtime_session(
                    "session-a",
                    run_id,
                    agent_id,
                ),
            )
            .await
            .expect("view");

        assert_eq!(view.target.delivery_runtime_session_id, "session-a");
        assert!(
            view.visible_capabilities
                .contains(&ToolCapability::new("file_read"))
        );
        assert!(
            !view
                .visible_capabilities
                .contains(&ToolCapability::new("workflow_management")),
            "tool-level grants must not expand schema-facing visible capabilities"
        );
        assert_eq!(
            view.visible_workspace_module_refs,
            vec!["canvas:overview".to_string()]
        );
        assert_eq!(grants.active_frame_queries().await, vec![frame.id]);
        assert!(grants.active_run_queries().await.is_empty());
    }

    #[tokio::test]
    async fn product_port_admit_tool_allows_visible_policy_tool() {
        let run_id = Uuid::new_v4();
        let agent_id = Uuid::new_v4();
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
        let frame = frame_with_agent_state(agent_id, &state);
        let anchors = Arc::new(MemoryAnchorRepository::default());
        let frames = Arc::new(MemoryAgentFrameRepository::default());
        let grants = Arc::new(MemoryGrantRepository::default());
        frames.create(&frame).await.expect("frame");
        insert_anchor(&anchors, "session-a", run_id, frame.id, agent_id).await;

        let decision = effective_capability_adapter(anchors, frames, grants)
            .admit_tool(ports_agent_run_surface::AgentRunAdmissionRequest::tool(
                "session-a",
                "workflow_management",
                "get_workflow",
                Some(ToolCluster::Workflow),
            ))
            .await
            .expect("decision");

        assert!(decision.allowed);
    }

    #[tokio::test]
    async fn product_port_admit_tool_denies_without_visible_tool_or_grant() {
        let run_id = Uuid::new_v4();
        let agent_id = Uuid::new_v4();
        let frame = frame_with_agent_state(agent_id, &CapabilityState::default());
        let anchors = Arc::new(MemoryAnchorRepository::default());
        let frames = Arc::new(MemoryAgentFrameRepository::default());
        let grants = Arc::new(MemoryGrantRepository::default());
        frames.create(&frame).await.expect("frame");
        insert_anchor(&anchors, "session-a", run_id, frame.id, agent_id).await;

        let decision = effective_capability_adapter(anchors, frames, grants)
            .admit_tool(ports_agent_run_surface::AgentRunAdmissionRequest::tool(
                "session-a",
                "workflow_management",
                "upsert_workflow_tool",
                Some(ToolCluster::Workflow),
            ))
            .await
            .expect("decision");

        assert!(!decision.allowed);
        assert!(
            decision
                .reason
                .as_deref()
                .is_some_and(|reason| reason.contains("upsert_workflow_tool"))
        );
    }

    #[tokio::test]
    async fn product_port_admit_tool_uses_current_frame_surface_and_effect_frame_grants() {
        let run_id = Uuid::new_v4();
        let agent_id = Uuid::new_v4();
        let mut launch_frame = frame_with_agent_state(agent_id, &CapabilityState::default());
        launch_frame.revision = 1;
        let mut current_state = CapabilityState::default();
        current_state
            .tool
            .capabilities
            .insert(ToolCapability::new("file_read"));
        let mut current_frame = frame_with_agent_state(agent_id, &current_state);
        current_frame.revision = 2;
        let launch_frame_id = launch_frame.id;
        let current_frame_id = current_frame.id;
        let anchors = Arc::new(MemoryAnchorRepository::default());
        let frames = Arc::new(MemoryAgentFrameRepository::default());
        let grants = Arc::new(MemoryGrantRepository::default());
        frames.create(&launch_frame).await.expect("launch frame");
        frames.create(&current_frame).await.expect("current frame");
        insert_anchor(&anchors, "session-a", run_id, launch_frame_id, agent_id).await;
        grants
            .insert(active_tool_grant(
                run_id,
                "session-a",
                launch_frame_id,
                "workflow_management::old_launch_tool",
            ))
            .await;
        grants
            .insert(active_tool_grant(
                run_id,
                "session-a",
                current_frame_id,
                "workflow_management::upsert_workflow_tool",
            ))
            .await;
        let adapter = effective_capability_adapter(anchors, frames, grants.clone());

        let session_a_view = adapter
            .effective_capability(
                ports_agent_run_surface::AgentRunEffectiveCapabilityRequest::for_runtime_session(
                    "session-a",
                    run_id,
                    agent_id,
                ),
            )
            .await
            .expect("session a view");
        assert_eq!(session_a_view.target.frame_id, current_frame_id);
        assert!(
            session_a_view
                .visible_capabilities
                .contains(&ToolCapability::new("file_read")),
            "schema-visible surface must come from the current frame, not launch evidence"
        );
        assert!(
            !session_a_view
                .visible_capabilities
                .contains(&ToolCapability::new("workflow_management")),
            "tool-level grants must stay out of schema-visible capability expansion"
        );

        let allowed = adapter
            .admit_tool(ports_agent_run_surface::AgentRunAdmissionRequest::tool(
                "session-a",
                "workflow_management",
                "upsert_workflow_tool",
                Some(ToolCluster::Workflow),
            ))
            .await
            .expect("allowed decision");
        let denied = adapter
            .admit_tool(ports_agent_run_surface::AgentRunAdmissionRequest::tool(
                "session-a",
                "workflow_management",
                "old_launch_tool",
                Some(ToolCluster::Workflow),
            ))
            .await
            .expect("denied decision");

        assert!(allowed.allowed);
        assert!(!denied.allowed);
        let frame_queries = grants.active_frame_queries().await;
        assert!(!frame_queries.is_empty());
        assert!(!frame_queries.contains(&launch_frame_id));
        assert!(
            frame_queries
                .iter()
                .all(|frame_id| *frame_id == current_frame_id)
        );
        assert!(grants.active_run_queries().await.is_empty());
    }

    #[tokio::test]
    async fn runtime_session_grant_projection_queries_current_effect_frame_not_run() {
        let run_id = Uuid::new_v4();
        let agent_id = Uuid::new_v4();
        let mut launch_frame = frame_with_agent_state(agent_id, &CapabilityState::default());
        launch_frame.revision = 1;
        let mut current_frame = frame_with_agent_state(agent_id, &CapabilityState::default());
        current_frame.revision = 2;
        let launch_frame_id = launch_frame.id;
        let current_frame_id = current_frame.id;
        let anchors = MemoryAnchorRepository::default();
        let frames = MemoryAgentFrameRepository::default();
        frames.create(&launch_frame).await.expect("launch frame");
        frames.create(&current_frame).await.expect("current frame");
        anchors
            .create_once(&RuntimeSessionExecutionAnchor::new_dispatch(
                "session-b",
                run_id,
                launch_frame_id,
                agent_id,
            ))
            .await
            .expect("anchor");
        let grants = MemoryGrantRepository::default();
        grants
            .insert(active_tool_grant(
                run_id,
                "session-a",
                launch_frame_id,
                "workflow_management::upsert_workflow_tool",
            ))
            .await;
        grants
            .insert(active_tool_grant(
                run_id,
                "session-b",
                current_frame_id,
                "workflow_management::get_workflow",
            ))
            .await;

        let projection = AgentRunEffectiveCapabilityService::grant_projection_for_runtime_session(
            "session-b",
            &anchors,
            &frames,
            &grants,
        )
        .await
        .expect("projection");

        assert_eq!(grants.active_frame_queries().await, vec![current_frame_id]);
        assert!(grants.active_run_queries().await.is_empty());
        assert!(projection.admits_tool(&AgentRunAdmissionRequest::tool(
            "workflow_management",
            "get_workflow",
            Some(ToolCluster::Workflow),
        )));
        assert!(!projection.admits_tool(&AgentRunAdmissionRequest::tool(
            "workflow_management",
            "upsert_workflow_tool",
            Some(ToolCluster::Workflow),
        )));
    }

    #[tokio::test]
    async fn runtime_session_execution_capability_state_keeps_tool_grant_invisible() {
        let run_id = Uuid::new_v4();
        let agent_id = Uuid::new_v4();
        let frame = frame_with_agent_state(agent_id, &CapabilityState::default());
        let frame_id = frame.id;
        let anchors = MemoryAnchorRepository::default();
        let frames = MemoryAgentFrameRepository::default();
        frames.create(&frame).await.expect("frame");
        anchors
            .create_once(&RuntimeSessionExecutionAnchor::new_dispatch(
                "session-a",
                run_id,
                frame_id,
                agent_id,
            ))
            .await
            .expect("anchor");
        let grants = MemoryGrantRepository::default();
        grants
            .insert(active_tool_grant(
                run_id,
                "session-a",
                frame_id,
                "workflow_management::upsert_workflow_tool",
            ))
            .await;

        let state =
            AgentRunEffectiveCapabilityService::schema_visible_capability_state_for_runtime_session(
                "session-a",
                &CapabilityState::default(),
                &anchors,
                &frames,
                &grants,
            )
            .await
            .expect("state");

        assert_eq!(grants.active_frame_queries().await, vec![frame_id]);
        assert!(grants.active_run_queries().await.is_empty());
        assert!(
            !state
                .tool
                .capabilities
                .contains(&ToolCapability::new("workflow_management"))
        );
        assert!(!state.is_capability_tool_enabled(
            "workflow_management",
            "upsert_workflow_tool",
            Some(ToolCluster::Workflow),
        ));
    }
}
