use std::sync::{Arc, OnceLock};

use agentdash_domain::agent_run_target::AgentRunTarget;
use agentdash_domain::common::AgentConfig;
use agentdash_domain::workflow::{MountDirective, SubjectRef};
use agentdash_platform_spi::{CapabilityState, RuntimeMcpServer, Vfs};
use async_trait::async_trait;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentRunFrameSurfaceCommand {
    Construct(FrameConstructionCommand),
    Update(RuntimeSurfaceUpdateRequest),
}

impl AgentRunFrameSurfaceCommand {
    pub fn write_role(&self) -> AgentFrameWriteRole {
        match self {
            Self::Construct(command) => command.write_role(),
            Self::Update(_) => AgentFrameWriteRole::RuntimeSurfaceUpdate,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FrameConstructionCommand {
    DispatchLaunchAnchor {
        run_id: Uuid,
        agent_id: Uuid,
        target_frame_id: Option<Uuid>,
        subject_ref: Option<SubjectRef>,
        runtime_thread_id: Option<String>,
        created_by_id: Option<String>,
        execution_profile: Option<serde_json::Value>,
    },
    CommitAcceptedLaunch {
        runtime_thread_id: String,
        turn_id: String,
    },
}

impl FrameConstructionCommand {
    pub fn write_role(&self) -> AgentFrameWriteRole {
        match self {
            Self::CommitAcceptedLaunch { .. } => AgentFrameWriteRole::LaunchCommit,
            Self::DispatchLaunchAnchor { .. } => AgentFrameWriteRole::FrameConstruction,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeSurfaceUpdateRequest {
    pub target: AgentRunTarget,
    pub idempotency_key: String,
    pub created_by_kind: String,
    pub created_by_id: Option<String>,
    pub changes: Vec<RuntimeSurfaceChange>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeSurfaceChange {
    ReplaceCapabilityState { state: CapabilityState },
    ReplaceVfsSurface { vfs: Vfs },
    ReplaceMcpSurface { servers: Vec<RuntimeMcpServer> },
    ReplaceExecutionProfile { profile: AgentConfig },
    ApplyVfsDirectives { directives: Vec<MountDirective> },
    AllowWorkspaceModule { module_ref: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentFrameWriteRole {
    FrameConstruction,
    LaunchCommit,
    RuntimeSurfaceUpdate,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentRunFrameSurfaceCommandOutcome {
    pub role: AgentFrameWriteRole,
    pub frame_id: Option<Uuid>,
    pub agent_id: Option<Uuid>,
    pub runtime_thread_id: Option<String>,
    pub frame_revision: Option<u64>,
    pub applied_generation: Option<u64>,
    pub wrote_frame_revision: bool,
    pub adopted_active_runtime: bool,
    pub diagnostics: Vec<String>,
}

impl AgentRunFrameSurfaceCommandOutcome {
    pub fn new(role: AgentFrameWriteRole) -> Self {
        Self {
            role,
            frame_id: None,
            agent_id: None,
            runtime_thread_id: None,
            frame_revision: None,
            applied_generation: None,
            wrote_frame_revision: false,
            adopted_active_runtime: false,
            diagnostics: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AgentRunFrameSurfaceError {
    #[error("frame construction command rejected: {message}")]
    ConstructionRejected { message: String },
    #[error("runtime surface update request rejected: {message}")]
    RuntimeSurfaceUpdateRejected { message: String },
    #[error("runtime surface projection context unavailable: {message}")]
    ProjectionContextUnavailable { message: String },
    #[error("frame surface adapter returned {actual:?} for {expected:?}")]
    RoleMismatch {
        expected: AgentFrameWriteRole,
        actual: AgentFrameWriteRole,
    },
}

#[async_trait]
pub trait AgentRunFrameConstructionPort: Send + Sync {
    async fn execute_frame_construction_command(
        &self,
        command: FrameConstructionCommand,
    ) -> Result<AgentRunFrameSurfaceCommandOutcome, AgentRunFrameSurfaceError>;
}

/// Composition-time handle for the canonical frame-construction implementation.
///
/// Repository bootstrap precedes VFS bootstrap, while ProjectAgent owner surface
/// materialization requires the fully assembled VFS service. The composition root
/// injects clones of this handle into lifecycle services, then binds the single
/// application-owned implementation before the application state becomes visible.
#[derive(Clone, Default)]
pub struct SharedAgentRunFrameConstructionHandle {
    inner: Arc<OnceLock<Arc<dyn AgentRunFrameConstructionPort>>>,
}

impl SharedAgentRunFrameConstructionHandle {
    pub fn set(
        &self,
        frame_construction: Arc<dyn AgentRunFrameConstructionPort>,
    ) -> Result<(), Arc<dyn AgentRunFrameConstructionPort>> {
        self.inner.set(frame_construction)
    }

    pub fn is_bound(&self) -> bool {
        self.inner.get().is_some()
    }
}

#[async_trait]
impl AgentRunFrameConstructionPort for SharedAgentRunFrameConstructionHandle {
    async fn execute_frame_construction_command(
        &self,
        command: FrameConstructionCommand,
    ) -> Result<AgentRunFrameSurfaceCommandOutcome, AgentRunFrameSurfaceError> {
        let frame_construction =
            self.inner
                .get()
                .ok_or_else(|| AgentRunFrameSurfaceError::ConstructionRejected {
                    message: "AgentRun frame-construction composition 尚未完成绑定".to_string(),
                })?;
        frame_construction
            .execute_frame_construction_command(command)
            .await
    }
}

#[async_trait]
pub trait AgentRunRuntimeSurfaceUpdatePort: Send + Sync {
    async fn execute_runtime_surface_update(
        &self,
        request: RuntimeSurfaceUpdateRequest,
    ) -> Result<AgentRunFrameSurfaceCommandOutcome, AgentRunFrameSurfaceError>;
}

#[async_trait]
pub trait AgentRunFrameSurfaceCommandPort: Send + Sync {
    async fn execute_frame_surface_command(
        &self,
        command: AgentRunFrameSurfaceCommand,
    ) -> Result<AgentRunFrameSurfaceCommandOutcome, AgentRunFrameSurfaceError>;
}
