//! AgentRun frame/surface command boundary.
//!
//! This module is the application-facing facade for AgentFrame surface writes.
//! Business domains submit typed construction/update intent here; they do not
//! own `AgentFrameBuilder`, full `CapabilityState` projection, or live-runtime
//! adoption timing.

use std::path::PathBuf;
use std::sync::Arc;

use agentdash_domain::workflow::AgentFrame;
use agentdash_spi::{AuthIdentity, CapabilityState, RuntimeBackendAnchor, RuntimeMcpServer, Vfs};
use thiserror::Error;
use uuid::Uuid;

use crate::agent_run::AgentFrameRuntimeTarget;

/// The single command boundary for AgentRun frame construction and runtime
/// surface mutation.
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

/// Construction-side commands that create or commit AgentFrame revisions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FrameConstructionCommand {
    /// Dispatch-time launch evidence frame. It records run/agent/runtime anchor
    /// identity, but not the full model-visible runtime surface.
    DispatchLaunchAnchor {
        run_id: Uuid,
        agent_id: Uuid,
        runtime_session_id: String,
        created_by_id: Option<String>,
    },
    /// Connector-accepted boundary that persists the pending launch frame or
    /// writes the accepted launch revision.
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

/// Runtime surface update requests. Callers provide stable changed-resource
/// identity only; AgentRun projection context must be resolved by the service.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeSurfaceUpdateRequest {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeSurfaceKind {
    Permission,
    Mcp,
    Vfs,
    WorkspaceModule,
    SkillInventory,
    AgentProcedure,
}

/// Resolver input for AgentRun runtime surface projection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentRunSurfaceProjectionContextSource {
    /// Resolve current AgentRun surface from a delivery RuntimeSession.
    DeliveryRuntimeSession { runtime_session_id: String },
    /// Resolve and validate a concrete frame target against its delivery runtime.
    RuntimeTarget { target: AgentFrameRuntimeTarget },
    /// Resolve the AgentRun that owns an existing effect frame.
    EffectFrame {
        effect_frame_id: Uuid,
        runtime_thread_id: String,
    },
}

impl AgentRunSurfaceProjectionContextSource {
    pub fn runtime_thread_id(&self) -> &str {
        match self {
            Self::DeliveryRuntimeSession { runtime_session_id } => runtime_session_id,
            Self::RuntimeTarget { target } => target.runtime_thread_id.as_str(),
            Self::EffectFrame {
                runtime_thread_id, ..
            } => runtime_thread_id,
        }
    }
}

/// AgentRun-owned projection facts used by runtime surface update adapters.
///
/// Business domains must not construct this object from partial local facts.
/// It is resolved from the current delivery runtime, active turn and current
/// AgentFrame so VFS/MCP/capability/identity move as one observable surface.
#[derive(Debug, Clone)]
pub struct AgentRunSurfaceProjectionContext {
    pub target: AgentFrameRuntimeTarget,
    pub runtime_thread_id: String,
    pub active_turn_id: Option<String>,
    pub current_frame: AgentFrame,
    pub identity: Option<AuthIdentity>,
    pub active_vfs: Option<Vfs>,
    pub mcp_servers: Vec<RuntimeMcpServer>,
    pub runtime_backend_anchor: Option<RuntimeBackendAnchor>,
    pub capability_state: CapabilityState,
    pub skill_discovery_provider_count: usize,
    pub extra_skill_dirs: Vec<PathBuf>,
}

impl AgentRunSurfaceProjectionContext {
    pub fn has_active_turn(&self) -> bool {
        self.active_turn_id.is_some()
    }

    pub fn require_identity(&self) -> Result<&AuthIdentity, AgentRunFrameSurfaceError> {
        self.identity.as_ref().ok_or_else(|| {
            AgentRunFrameSurfaceError::ProjectionContextUnavailable(format!(
                "delivery RuntimeSession `{}` 缺少 active turn identity",
                self.runtime_thread_id
            ))
        })
    }

    pub fn require_active_vfs(&self) -> Result<&Vfs, AgentRunFrameSurfaceError> {
        self.active_vfs.as_ref().ok_or_else(|| {
            AgentRunFrameSurfaceError::ProjectionContextUnavailable(format!(
                "AgentFrame `{}` 缺少 active VFS surface",
                self.current_frame.id
            ))
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentFrameWriteRole {
    FrameConstruction,
    LaunchCommit,
    RuntimeSurfaceUpdate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentFrameWritePrimitive {
    AgentFrameBuilder,
    PersistedFrameRevisionCommit,
    PersistedRevisionAdoption,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AgentFrameWriteBoundary {
    pub owner: &'static str,
    pub role: AgentFrameWriteRole,
    pub primitive: AgentFrameWritePrimitive,
}

/// Production AgentFrame write/adoption owners. This list is the static guardrail
/// for keeping business domains out of direct frame surface writes.
pub const AGENT_FRAME_WRITE_BOUNDARIES: &[AgentFrameWriteBoundary] = &[
    AgentFrameWriteBoundary {
        owner: "agent_run::frame::construction::FrameConstructionService",
        role: AgentFrameWriteRole::FrameConstruction,
        primitive: AgentFrameWritePrimitive::AgentFrameBuilder,
    },
    AgentFrameWriteBoundary {
        owner: "agent_run::frame::construction::composer_lifecycle_node",
        role: AgentFrameWriteRole::FrameConstruction,
        primitive: AgentFrameWritePrimitive::AgentFrameBuilder,
    },
    AgentFrameWriteBoundary {
        owner: "lifecycle::dispatch_service::dispatch_launch_anchor",
        role: AgentFrameWriteRole::FrameConstruction,
        primitive: AgentFrameWritePrimitive::AgentFrameBuilder,
    },
    AgentFrameWriteBoundary {
        owner: "agent_run::frame::launch_commit::AgentRunAcceptedLaunchCommitAdapter",
        role: AgentFrameWriteRole::LaunchCommit,
        primitive: AgentFrameWritePrimitive::PersistedFrameRevisionCommit,
    },
    AgentFrameWriteBoundary {
        owner: "agent_run::frame::AgentRunFrameSurfaceService",
        role: AgentFrameWriteRole::RuntimeSurfaceUpdate,
        primitive: AgentFrameWritePrimitive::AgentFrameBuilder,
    },
    AgentFrameWriteBoundary {
        owner: "agent_run::frame::AgentRunFrameSurfaceService",
        role: AgentFrameWriteRole::RuntimeSurfaceUpdate,
        primitive: AgentFrameWritePrimitive::PersistedRevisionAdoption,
    },
];

pub fn agent_frame_write_boundaries() -> &'static [AgentFrameWriteBoundary] {
    AGENT_FRAME_WRITE_BOUNDARIES
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

    pub fn frame_construction() -> Self {
        Self::new(AgentFrameWriteRole::FrameConstruction)
    }

    pub fn launch_commit() -> Self {
        Self::new(AgentFrameWriteRole::LaunchCommit)
    }

    pub fn runtime_surface_update() -> Self {
        Self::new(AgentFrameWriteRole::RuntimeSurfaceUpdate)
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum AgentRunFrameSurfaceError {
    #[error("frame construction command rejected: {0}")]
    ConstructionRejected(String),
    #[error("runtime surface update request rejected: {0}")]
    RuntimeSurfaceUpdateRejected(String),
    #[error("runtime surface projection context unavailable: {0}")]
    ProjectionContextUnavailable(String),
    #[error("frame surface adapter returned {actual:?} for {expected:?}")]
    RoleMismatch {
        expected: AgentFrameWriteRole,
        actual: AgentFrameWriteRole,
    },
}

/// Resolves AgentRun-owned facts for runtime surface update adapters.
#[async_trait::async_trait]
pub trait AgentRunSurfaceProjectionContextResolver: Send + Sync {
    async fn resolve_surface_projection_context(
        &self,
        source: AgentRunSurfaceProjectionContextSource,
    ) -> Result<AgentRunSurfaceProjectionContext, AgentRunFrameSurfaceError>;
}

/// Adapter implemented by the existing frame construction path.
#[async_trait::async_trait]
pub trait AgentRunFrameConstructionAdapter: Send + Sync {
    async fn execute_frame_construction_command(
        &self,
        command: FrameConstructionCommand,
    ) -> Result<AgentRunFrameSurfaceCommandOutcome, AgentRunFrameSurfaceError>;
}

/// Construction adapter for runtime-update-only call sites.
#[derive(Debug, Default)]
pub struct RejectingFrameConstructionAdapter;

#[async_trait::async_trait]
impl AgentRunFrameConstructionAdapter for RejectingFrameConstructionAdapter {
    async fn execute_frame_construction_command(
        &self,
        command: FrameConstructionCommand,
    ) -> Result<AgentRunFrameSurfaceCommandOutcome, AgentRunFrameSurfaceError> {
        Err(AgentRunFrameSurfaceError::ConstructionRejected(format!(
            "runtime surface adapter cannot execute frame construction command: {command:?}"
        )))
    }
}

/// Adapter implemented by the runtime surface projector/adoption path.
#[async_trait::async_trait]
pub trait AgentRunRuntimeSurfaceUpdateAdapter: Send + Sync {
    async fn execute_runtime_surface_update(
        &self,
        request: RuntimeSurfaceUpdateRequest,
    ) -> Result<AgentRunFrameSurfaceCommandOutcome, AgentRunFrameSurfaceError>;
}

/// Facade for all AgentRun frame/surface writes.
pub struct AgentRunFrameSurfaceService {
    construction: Arc<dyn AgentRunFrameConstructionAdapter>,
    runtime_update: Arc<dyn AgentRunRuntimeSurfaceUpdateAdapter>,
}

impl AgentRunFrameSurfaceService {
    pub fn new(
        construction: Arc<dyn AgentRunFrameConstructionAdapter>,
        runtime_update: Arc<dyn AgentRunRuntimeSurfaceUpdateAdapter>,
    ) -> Self {
        Self {
            construction,
            runtime_update,
        }
    }

    pub async fn execute(
        &self,
        command: AgentRunFrameSurfaceCommand,
    ) -> Result<AgentRunFrameSurfaceCommandOutcome, AgentRunFrameSurfaceError> {
        let expected_role = command.write_role();
        let outcome = match command {
            AgentRunFrameSurfaceCommand::Construct(command) => {
                self.construction
                    .execute_frame_construction_command(command)
                    .await?
            }
            AgentRunFrameSurfaceCommand::Update(request) => {
                self.runtime_update
                    .execute_runtime_surface_update(request)
                    .await?
            }
        };
        ensure_role(outcome, expected_role)
    }

    pub async fn construct(
        &self,
        command: FrameConstructionCommand,
    ) -> Result<AgentRunFrameSurfaceCommandOutcome, AgentRunFrameSurfaceError> {
        self.execute(AgentRunFrameSurfaceCommand::Construct(command))
            .await
    }

    pub async fn update_runtime_surface(
        &self,
        request: RuntimeSurfaceUpdateRequest,
    ) -> Result<AgentRunFrameSurfaceCommandOutcome, AgentRunFrameSurfaceError> {
        self.execute(AgentRunFrameSurfaceCommand::Update(request))
            .await
    }
}

fn ensure_role(
    outcome: AgentRunFrameSurfaceCommandOutcome,
    expected: AgentFrameWriteRole,
) -> Result<AgentRunFrameSurfaceCommandOutcome, AgentRunFrameSurfaceError> {
    if outcome.role == expected {
        Ok(outcome)
    } else {
        Err(AgentRunFrameSurfaceError::RoleMismatch {
            expected,
            actual: outcome.role,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};
    use std::sync::Mutex;

    #[derive(Default)]
    struct RecordingConstructionAdapter {
        commands: Mutex<Vec<FrameConstructionCommand>>,
        forced_role: Option<AgentFrameWriteRole>,
    }

    #[async_trait::async_trait]
    impl AgentRunFrameConstructionAdapter for RecordingConstructionAdapter {
        async fn execute_frame_construction_command(
            &self,
            command: FrameConstructionCommand,
        ) -> Result<AgentRunFrameSurfaceCommandOutcome, AgentRunFrameSurfaceError> {
            let role = self.forced_role.unwrap_or_else(|| command.write_role());
            self.commands.lock().unwrap().push(command);
            Ok(AgentRunFrameSurfaceCommandOutcome::new(role))
        }
    }

    #[derive(Default)]
    struct RecordingRuntimeUpdateAdapter {
        requests: Mutex<Vec<RuntimeSurfaceUpdateRequest>>,
    }

    #[async_trait::async_trait]
    impl AgentRunRuntimeSurfaceUpdateAdapter for RecordingRuntimeUpdateAdapter {
        async fn execute_runtime_surface_update(
            &self,
            request: RuntimeSurfaceUpdateRequest,
        ) -> Result<AgentRunFrameSurfaceCommandOutcome, AgentRunFrameSurfaceError> {
            self.requests.lock().unwrap().push(request);
            Ok(AgentRunFrameSurfaceCommandOutcome::runtime_surface_update())
        }
    }

    #[tokio::test]
    async fn facade_routes_construct_and_update_commands_to_typed_adapters() {
        let construction = Arc::new(RecordingConstructionAdapter::default());
        let runtime_update = Arc::new(RecordingRuntimeUpdateAdapter::default());
        let service =
            AgentRunFrameSurfaceService::new(construction.clone(), runtime_update.clone());

        let construct = FrameConstructionCommand::DispatchLaunchAnchor {
            run_id: Uuid::new_v4(),
            agent_id: Uuid::new_v4(),
            runtime_session_id: "runtime-a".to_string(),
            created_by_id: None,
        };
        let update = RuntimeSurfaceUpdateRequest::PermissionGrantApplied {
            grant_id: Uuid::new_v4(),
        };

        let construct_outcome = service
            .construct(construct.clone())
            .await
            .expect("construct");
        let update_outcome = service
            .update_runtime_surface(update.clone())
            .await
            .expect("update");

        assert_eq!(
            construct_outcome.role,
            AgentFrameWriteRole::FrameConstruction
        );
        assert_eq!(
            update_outcome.role,
            AgentFrameWriteRole::RuntimeSurfaceUpdate
        );
        assert_eq!(
            construction.commands.lock().unwrap().as_slice(),
            std::slice::from_ref(&construct)
        );
        assert_eq!(
            runtime_update.requests.lock().unwrap().as_slice(),
            std::slice::from_ref(&update)
        );
    }

    #[tokio::test]
    async fn facade_rejects_adapter_role_mismatch() {
        let construction = Arc::new(RecordingConstructionAdapter {
            commands: Mutex::new(Vec::new()),
            forced_role: Some(AgentFrameWriteRole::RuntimeSurfaceUpdate),
        });
        let runtime_update = Arc::new(RecordingRuntimeUpdateAdapter::default());
        let service = AgentRunFrameSurfaceService::new(construction, runtime_update);

        let error = service
            .construct(FrameConstructionCommand::CommitAcceptedLaunch {
                runtime_session_id: "runtime-a".to_string(),
                turn_id: "turn-a".to_string(),
            })
            .await
            .expect_err("role mismatch");

        assert_eq!(
            error,
            AgentRunFrameSurfaceError::RoleMismatch {
                expected: AgentFrameWriteRole::LaunchCommit,
                actual: AgentFrameWriteRole::RuntimeSurfaceUpdate,
            }
        );
    }

    #[test]
    fn frame_construction_commands_have_explicit_write_roles() {
        let command = FrameConstructionCommand::CommitAcceptedLaunch {
            runtime_session_id: "runtime-a".to_string(),
            turn_id: "turn-a".to_string(),
        };
        assert_eq!(command.write_role(), AgentFrameWriteRole::LaunchCommit);

        let command = FrameConstructionCommand::DispatchLaunchAnchor {
            run_id: Uuid::new_v4(),
            agent_id: Uuid::new_v4(),
            runtime_session_id: "runtime-a".to_string(),
            created_by_id: None,
        };
        assert_eq!(command.write_role(), AgentFrameWriteRole::FrameConstruction);
    }

    #[test]
    fn runtime_update_requests_are_surface_kind_only() {
        let request = RuntimeSurfaceUpdateRequest::PermissionGrantApplied {
            grant_id: Uuid::new_v4(),
        };
        assert_eq!(request.surface_kind(), RuntimeSurfaceKind::Permission);
        assert_eq!(
            AgentRunFrameSurfaceCommand::Update(request).write_role(),
            AgentFrameWriteRole::RuntimeSurfaceUpdate
        );
    }

    #[test]
    fn projection_context_exposes_runtime_update_target_and_identity() {
        let agent_id = Uuid::new_v4();
        let frame = AgentFrame::new_revision(agent_id, 7, "test");
        let target = AgentFrameRuntimeTarget {
            frame_id: frame.id,
            runtime_thread_id: agentdash_agent_runtime_contract::RuntimeThreadId::new("runtime-a")
                .expect("runtime thread id"),
        };
        let identity = AuthIdentity::system_routine("surface-context-test");

        let context = AgentRunSurfaceProjectionContext {
            target: target.clone(),
            runtime_thread_id: "runtime-a".to_string(),
            active_turn_id: Some("turn-a".to_string()),
            current_frame: frame,
            identity: Some(identity.clone()),
            active_vfs: None,
            mcp_servers: Vec::new(),
            runtime_backend_anchor: None,
            capability_state: CapabilityState::default(),
            skill_discovery_provider_count: 0,
            extra_skill_dirs: Vec::new(),
        };

        assert!(context.has_active_turn());
        assert_eq!(context.target, target);
        assert_eq!(
            context.require_identity().expect("identity").user_id,
            identity.user_id
        );
        assert_eq!(
            AgentRunFrameSurfaceCommand::Update(
                RuntimeSurfaceUpdateRequest::SkillInventoryChanged {
                    provider_key: "external".to_string(),
                },
            )
            .write_role(),
            AgentFrameWriteRole::RuntimeSurfaceUpdate
        );
    }

    #[test]
    fn agent_frame_write_boundary_allowlist_excludes_business_domains() {
        let boundaries = agent_frame_write_boundaries();

        assert!(boundaries.iter().any(|boundary| {
            boundary.owner == "agent_run::frame::construction::FrameConstructionService"
                && boundary.role == AgentFrameWriteRole::FrameConstruction
        }));
        assert!(boundaries.iter().any(|boundary| {
            boundary.owner == "agent_run::frame::AgentRunFrameSurfaceService"
                && boundary.role == AgentFrameWriteRole::RuntimeSurfaceUpdate
                && boundary.primitive == AgentFrameWritePrimitive::PersistedRevisionAdoption
        }));

        let forbidden_prefixes = [
            "canvas::",
            "workspace_module::",
            "permission::",
            "agentdash-api::",
        ];
        for boundary in boundaries {
            assert!(
                !forbidden_prefixes
                    .iter()
                    .any(|prefix| boundary.owner.starts_with(prefix)),
                "{} must submit typed RuntimeSurfaceUpdateRequest instead of writing AgentFrame",
                boundary.owner
            );
        }
    }

    fn workspace_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .canonicalize()
            .expect("workspace root")
    }

    fn read_workspace_file(relative_path: &str) -> String {
        std::fs::read_to_string(workspace_root().join(relative_path))
            .unwrap_or_else(|error| panic!("failed to read {relative_path}: {error}"))
    }

    #[test]
    fn business_modules_and_api_routes_do_not_direct_adopt_runtime_surface() {
        for path in [
            "crates/agentdash-workspace-module/src/workspace_module/tools.rs",
            "crates/agentdash-workspace-module/src/canvas/management.rs",
            "crates/agentdash-workspace-module/src/canvas/visibility.rs",
            "crates/agentdash-application/src/canvas/promotion.rs",
            "crates/agentdash-application/src/permission/service.rs",
            "crates/agentdash-api/src/routes/permission_grants.rs",
        ] {
            let source = read_workspace_file(path);
            assert!(
                !source.contains("adopt_persisted_agent_frame_revision"),
                "{path} must submit typed runtime surface requests instead of directly adopting persisted AgentFrame revisions"
            );
        }
    }

    #[test]
    fn business_modules_and_api_routes_do_not_direct_expose_canvas_mount_revision() {
        for path in [
            "crates/agentdash-workspace-module/src/workspace_module/tools.rs",
            "crates/agentdash-workspace-module/src/canvas/management.rs",
            "crates/agentdash-workspace-module/src/canvas/visibility.rs",
            "crates/agentdash-application/src/canvas/promotion.rs",
            "crates/agentdash-application/src/permission/service.rs",
            "crates/agentdash-api/src/routes/permission_grants.rs",
        ] {
            let source = read_workspace_file(path);
            assert!(
                !source.contains("expose_canvas_mount_revision_and_adopt"),
                "{path} must route Canvas visibility through the AgentRun frame/surface boundary"
            );
        }
    }
}
