use std::collections::{BTreeMap, BTreeSet};

use agentdash_agent_runtime_contract::PresentationThreadId;
use agentdash_domain::backend::{RuntimeBackendAnchor, RuntimeBackendAnchorError};
use agentdash_spi::{
    AuthIdentity, CapabilityState, RuntimeMcpServer, ToolCapability, ToolCluster, Vfs,
};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::lifecycle_surface_projection::AgentRunLifecycleSurface;
use crate::runtime_surface_adoption::AgentFrameRuntimeTarget;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeSurfaceQueryPurpose {
    pub component: String,
}

impl RuntimeSurfaceQueryPurpose {
    pub fn new(component: impl Into<String>) -> Self {
        Self {
            component: component.into(),
        }
    }

    pub fn resource_surface() -> Self {
        Self::new("agent_run_resource_surface")
    }
}

impl From<&str> for RuntimeSurfaceQueryPurpose {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentRunRuntimeAddress {
    pub run_id: Uuid,
    pub agent_id: Uuid,
    pub frame_id: Uuid,
}

#[derive(Debug, Clone)]
pub struct AgentRunRuntimeSurface {
    pub runtime_session_id: String,
    pub presentation_thread_id: PresentationThreadId,
    pub run_id: Uuid,
    pub project_id: Uuid,
    pub agent_id: Uuid,
    pub runtime_address: AgentRunRuntimeAddress,
    pub launch_evidence_frame_id: Uuid,
    pub current_surface_frame_id: Uuid,
    pub surface_revision: i32,
    pub capability_state: CapabilityState,
    pub visible_workspace_module_refs: Vec<String>,
    pub vfs: Vfs,
    pub vfs_access_policy: agentdash_spi::RuntimeVfsAccessPolicy,
    pub mcp_servers: Vec<RuntimeMcpServer>,
    pub runtime_backend_anchor: Option<RuntimeBackendAnchor>,
    pub active_turn_id: Option<String>,
    pub identity: Option<AuthIdentity>,
    pub provenance: AgentRunRuntimeSurfaceProvenance,
    pub closure: AgentRunRuntimeSurfaceClosure,
}

#[derive(Debug, Clone)]
pub struct AgentRunResourceSurface {
    pub runtime: AgentRunRuntimeSurface,
    pub lifecycle_surface: AgentRunLifecycleSurface,
}

#[derive(Debug, Clone)]
pub struct AgentRunRuntimeSurfaceWithBackend {
    pub surface: AgentRunRuntimeSurface,
    pub runtime_backend_anchor: RuntimeBackendAnchor,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentRunTerminalLaunchTarget {
    pub backend_id: String,
    pub mount_root_ref: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentRunRuntimeSurfaceProvenance {
    pub launch_evidence_frame_id: Uuid,
    pub launch_created_by_kind: String,
    pub current_surface_frame_id: Uuid,
    pub surface_revision: i32,
    pub surface_created_by_kind: String,
    pub anchor_updated_at: DateTime<Utc>,
    pub orchestration_id: Option<Uuid>,
    pub node_path: Option<String>,
    pub node_attempt: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentRunRuntimeSurfaceClosure {
    pub capability_field_present: bool,
    pub vfs_field_present: bool,
    pub mcp_field_present: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AgentRunRuntimeSurfaceQueryError {
    #[error(
        "runtime surface query missing anchor: component={component}, session_id={runtime_session_id}",
        component = purpose.component
    )]
    MissingAnchor {
        purpose: RuntimeSurfaceQueryPurpose,
        runtime_session_id: String,
    },
    #[error(
        "runtime surface query missing lifecycle run: component={component}, session_id={runtime_session_id}, run_id={run_id}",
        component = purpose.component
    )]
    MissingLifecycleRun {
        purpose: RuntimeSurfaceQueryPurpose,
        runtime_session_id: String,
        run_id: Uuid,
    },
    #[error(
        "runtime surface query missing lifecycle agent: component={component}, session_id={runtime_session_id}, agent_id={agent_id}",
        component = purpose.component
    )]
    MissingLifecycleAgent {
        purpose: RuntimeSurfaceQueryPurpose,
        runtime_session_id: String,
        agent_id: Uuid,
    },
    #[error(
        "runtime surface query missing current frame: component={component}, session_id={runtime_session_id}, agent_id={agent_id}",
        component = purpose.component
    )]
    MissingCurrentFrame {
        purpose: RuntimeSurfaceQueryPurpose,
        runtime_session_id: String,
        agent_id: Uuid,
    },
    #[error(
        "runtime surface query missing immutable surface closure: component={component}, session_id={runtime_session_id}, frame_id={frame_id}, field={field}",
        component = purpose.component
    )]
    MissingSurfaceClosure {
        purpose: RuntimeSurfaceQueryPurpose,
        runtime_session_id: String,
        frame_id: Uuid,
        field: &'static str,
    },
    #[error("runtime surface query backend anchor failed: {source}")]
    RuntimeBackendAnchor { source: RuntimeBackendAnchorError },
    #[error("runtime surface query repository failed: operation={operation}, message={message}")]
    Repository {
        operation: &'static str,
        message: String,
    },
    #[error("runtime surface query projection failed: {message}")]
    Projection { message: String },
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AgentRunResourceSurfaceQueryError {
    #[error("{0}")]
    RuntimeSurface(#[from] AgentRunRuntimeSurfaceQueryError),
    #[error(
        "agent run resource surface missing delivery anchor: run_id={run_id}, agent_id={agent_id}"
    )]
    MissingDeliveryAnchor { run_id: Uuid, agent_id: Uuid },
    #[error(
        "agent run resource surface control-plane mismatch: field={field}, expected={expected}, actual={actual}"
    )]
    ControlPlaneMismatch {
        field: &'static str,
        expected: String,
        actual: String,
    },
    #[error("agent run resource surface projection failed: {message}")]
    Projection { message: String },
    #[error(
        "agent run resource surface repository failed: operation={operation}, message={message}"
    )]
    Repository {
        operation: &'static str,
        message: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AgentRunTerminalLaunchTargetError {
    #[error("runtime surface has no relay mount for backend anchor root_ref: {root_ref}")]
    MissingAnchorMount { root_ref: String },
    #[error("runtime surface has no mount available for terminal launch")]
    MissingMount,
    #[error("runtime surface mount `{mount_id}` uses unsupported provider `{provider}`")]
    UnsupportedMountProvider { mount_id: String, provider: String },
    #[error("runtime backend anchor has no backend_id")]
    MissingBackendId,
    #[error("runtime surface mount `{mount_id}` has no root_ref")]
    MissingMountRootRef { mount_id: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentRunEffectiveCapabilityRequest {
    pub runtime_session_id: String,
    pub agent_run_id: Uuid,
    pub agent_id: Uuid,
    pub command_key: Option<String>,
}

impl AgentRunEffectiveCapabilityRequest {
    pub fn for_runtime_session(
        runtime_session_id: impl Into<String>,
        agent_run_id: Uuid,
        agent_id: Uuid,
    ) -> Self {
        Self {
            runtime_session_id: runtime_session_id.into(),
            agent_run_id,
            agent_id,
            command_key: None,
        }
    }

    pub fn with_command_key(mut self, command_key: impl Into<String>) -> Self {
        self.command_key = Some(command_key.into());
        self
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AgentRunGrantProjection {
    pub admitted_tools: BTreeMap<String, BTreeSet<String>>,
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
    pub runtime_session_id: String,
    pub capability_key: String,
    pub tool_name: String,
    pub cluster: Option<ToolCluster>,
}

impl AgentRunAdmissionRequest {
    pub fn tool(
        runtime_session_id: impl Into<String>,
        capability_key: impl Into<String>,
        tool_name: impl Into<String>,
        cluster: Option<ToolCluster>,
    ) -> Self {
        Self {
            runtime_session_id: runtime_session_id.into(),
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

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AgentRunEffectiveCapabilityError {
    #[error(
        "agent run capability runtime session was not found: runtime_session_id={runtime_session_id}"
    )]
    MissingRuntimeSession { runtime_session_id: String },
    #[error("agent run capability target was not found: run_id={run_id}, agent_id={agent_id}")]
    MissingTarget { run_id: Uuid, agent_id: Uuid },
    #[error("agent run capability projection failed: {message}")]
    Projection { message: String },
    #[error("agent run capability repository failed: operation={operation}, message={message}")]
    Repository {
        operation: &'static str,
        message: String,
    },
}

#[async_trait]
pub trait AgentRunRuntimeSurfaceQueryPort: Send + Sync {
    async fn current_runtime_surface(
        &self,
        runtime_session_id: &str,
        purpose: RuntimeSurfaceQueryPurpose,
    ) -> Result<AgentRunRuntimeSurface, AgentRunRuntimeSurfaceQueryError>;

    async fn current_runtime_surface_with_backend(
        &self,
        runtime_session_id: &str,
        purpose: RuntimeSurfaceQueryPurpose,
    ) -> Result<AgentRunRuntimeSurfaceWithBackend, AgentRunRuntimeSurfaceQueryError>;
}

#[async_trait]
pub trait AgentRunResourceSurfaceQueryPort: Send + Sync {
    async fn resource_surface_for_runtime_session(
        &self,
        runtime_session_id: &str,
    ) -> Result<AgentRunResourceSurface, AgentRunResourceSurfaceQueryError>;

    async fn resource_surface_for_agent_run(
        &self,
        run_id: Uuid,
        agent_id: Uuid,
    ) -> Result<AgentRunResourceSurface, AgentRunResourceSurfaceQueryError>;
}

#[async_trait]
pub trait AgentRunRuntimePlacementPort: Send + Sync {
    async fn terminal_launch_target_for_runtime_session(
        &self,
        runtime_session_id: &str,
        purpose: RuntimeSurfaceQueryPurpose,
    ) -> Result<AgentRunTerminalLaunchTarget, AgentRunTerminalLaunchTargetError>;
}

#[async_trait]
pub trait AgentRunEffectiveCapabilityPort: Send + Sync {
    async fn effective_capability(
        &self,
        request: AgentRunEffectiveCapabilityRequest,
    ) -> Result<AgentRunEffectiveCapabilityView, AgentRunEffectiveCapabilityError>;

    async fn admit_tool(
        &self,
        request: AgentRunAdmissionRequest,
    ) -> Result<AgentRunAdmissionDecision, AgentRunEffectiveCapabilityError>;
}
