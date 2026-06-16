pub mod builder;
pub mod construction;
pub mod hook_runtime;
pub mod runtime_launch;
pub mod surface;

pub use builder::AgentFrameBuilder;
pub use hook_runtime::AgentFrameHookRuntime;
pub use runtime_launch::{FrameLaunchEnvelope, FrameLaunchIntent, FrameRuntimeSurface};
pub use surface::{AgentFrameSurfaceExt, FrameContextBundleSummary, FrameSurfaceDraft};
