pub mod builder;
pub mod construction;
pub mod hook_runtime;
pub mod runtime_launch;
pub mod surface;
pub mod surface_service;

pub use builder::AgentFrameBuilder;
pub use hook_runtime::AgentFrameHookRuntime;
pub use runtime_launch::{FrameLaunchEnvelope, FrameLaunchIntent, FrameRuntimeSurface};
pub use surface::{AgentFrameSurfaceExt, FrameContextBundleSummary, FrameSurfaceDraft};
pub use surface_service::{
    AGENT_FRAME_WRITE_BOUNDARIES, AgentFrameWriteBoundary, AgentFrameWritePrimitive,
    AgentFrameWriteRole, AgentRunFrameConstructionAdapter, AgentRunFrameSurfaceCommand,
    AgentRunFrameSurfaceCommandOutcome, AgentRunFrameSurfaceError, AgentRunFrameSurfaceService,
    AgentRunRuntimeSurfaceUpdateAdapter, CanvasVisibilityReason, FrameConstructionCommand,
    FrameConstructionReason, RuntimeSurfaceKind, RuntimeSurfaceUpdateRequest,
    agent_frame_write_boundaries,
};
