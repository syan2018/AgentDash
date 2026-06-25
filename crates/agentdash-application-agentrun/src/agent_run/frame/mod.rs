pub(crate) mod builder;
pub(crate) mod hook_runtime;
pub(crate) mod launch_commit;
pub(crate) mod launch_envelope_provider;
pub(crate) mod lifecycle_materialization;
pub(crate) mod runtime_launch;
pub(crate) mod surface;
pub(crate) mod surface_service;

pub use builder::{
    AgentFrameActivationSurface, AgentFrameActivationSurfaceInput, AgentFrameBuilder,
    build_lifecycle_activation_surface,
};
pub use hook_runtime::{
    AgentFrameHookRuntime, AgentRunHookTargetRuntimeAdapter, hook_target_runtime_port,
};
pub use launch_commit::{
    AgentRunAcceptedLaunchCommitAdapter, AgentRunAcceptedLaunchCommitDeps,
    accepted_launch_commit_port,
};
pub use launch_envelope_provider::{
    CompanionLaunchSource, CompanionLaunchWorkflowSource, FrameLaunchEnvelopeConstructionInput,
    RoutineLaunchSource,
};
pub use lifecycle_materialization::AgentRunLaunchAnchorFrameConstructionAdapter;
pub use runtime_launch::{
    FrameLaunchEnvelope, FrameLaunchIntent, FrameLaunchSurface, FrameRuntimeSurface,
    LaunchResolutionTrace,
};
pub use surface::{AgentFrameSurfaceExt, FrameContextBundleSummary, FrameSurfaceDraft};
pub use surface_service::{
    AGENT_FRAME_WRITE_BOUNDARIES, AgentFrameWriteBoundary, AgentFrameWritePrimitive,
    AgentFrameWriteRole, AgentRunFrameConstructionAdapter, AgentRunFrameSurfaceCommand,
    AgentRunFrameSurfaceCommandOutcome, AgentRunFrameSurfaceError, AgentRunFrameSurfaceService,
    AgentRunRuntimeSurfaceUpdateAdapter, AgentRunSurfaceProjectionContext,
    AgentRunSurfaceProjectionContextResolver, AgentRunSurfaceProjectionContextSource,
    CanvasVisibilityReason, FrameConstructionCommand, FrameConstructionReason,
    RejectingFrameConstructionAdapter, RuntimeSurfaceKind, RuntimeSurfaceUpdateRequest,
    agent_frame_write_boundaries,
};
