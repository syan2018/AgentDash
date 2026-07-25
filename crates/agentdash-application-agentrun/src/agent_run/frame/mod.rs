pub(crate) mod builder;
pub(crate) mod launch_envelope_provider;
pub(crate) mod runtime_launch;
pub(crate) mod surface;

pub use builder::{
    AgentFrameActivationSurface, AgentFrameActivationSurfaceInput, AgentFrameBuilder,
    build_lifecycle_activation_surface,
};
pub use launch_envelope_provider::{
    FrameLaunchEnvelopeConstructionInput, PromptLaunchPath, RuntimeTraceLaunchState,
    SessionRepositoryRehydrateMode, resolve_prompt_launch_path,
};
pub use runtime_launch::{
    FrameLaunchContextProjection, FrameLaunchDiagnostics, FrameLaunchEnvelope, FrameLaunchFrameRef,
    FrameLaunchIntent, FrameLaunchRuntimeSurface, FrameLaunchSurface, FrameRuntimeSurface,
    LaunchResolutionTrace, TerminalHookEffectBinding, runtime_backend_anchor_from_vfs,
};
pub use surface::{
    AgentContextSourceFragment, AgentContextSourceSnapshot, AgentFrameSurfaceExt,
    FrameContextBundleSummary, FrameSurfaceDraft,
};
