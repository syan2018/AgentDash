use agentdash_domain::canvas::CanvasDataBinding;
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
        runtime_session_id: Option<String>,
        created_by_id: Option<String>,
    },
    CommitAcceptedLaunch {
        runtime_session_id: String,
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
pub enum RuntimeSurfaceUpdateRequest {
    CanvasBindingChanged {
        canvas_mount_id: String,
        binding: CanvasDataBinding,
    },
    CanvasVisibilityRequested {
        canvas_mount_id: String,
        reason: CanvasVisibilityReason,
    },
    PermissionGrantApplied {
        grant_id: Uuid,
    },
    PermissionGrantRevoked {
        grant_id: Uuid,
    },
    McpPresetChanged {
        preset_key: String,
    },
    ProjectVfsMountChanged {
        mount_id: String,
    },
    WorkspaceModuleVisibilityChanged {
        module_ref: String,
    },
    SkillInventoryChanged {
        provider_key: String,
    },
    AgentProcedureContractChanged {
        run_id: Uuid,
        agent_id: Uuid,
        orchestration_id: Uuid,
        node_path: String,
        attempt: u32,
    },
}

impl RuntimeSurfaceUpdateRequest {
    pub fn surface_kind(&self) -> RuntimeSurfaceKind {
        match self {
            Self::CanvasBindingChanged { .. } | Self::CanvasVisibilityRequested { .. } => {
                RuntimeSurfaceKind::Canvas
            }
            Self::PermissionGrantApplied { .. } | Self::PermissionGrantRevoked { .. } => {
                RuntimeSurfaceKind::Permission
            }
            Self::McpPresetChanged { .. } => RuntimeSurfaceKind::Mcp,
            Self::ProjectVfsMountChanged { .. } => RuntimeSurfaceKind::Vfs,
            Self::WorkspaceModuleVisibilityChanged { .. } => RuntimeSurfaceKind::WorkspaceModule,
            Self::SkillInventoryChanged { .. } => RuntimeSurfaceKind::SkillInventory,
            Self::AgentProcedureContractChanged { .. } => RuntimeSurfaceKind::AgentProcedure,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CanvasVisibilityReason {
    Created,
    Presented,
    ExplicitRefresh,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeSurfaceKind {
    Canvas,
    Permission,
    Mcp,
    Vfs,
    WorkspaceModule,
    SkillInventory,
    AgentProcedure,
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
    pub runtime_session_id: Option<String>,
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
            runtime_session_id: None,
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
