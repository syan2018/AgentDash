pub mod detection;
pub mod resolution;

pub use detection::{
    WorkspaceDetectionError, WorkspaceDetectionResult, detect_workspace_from_backend,
};
pub use resolution::{
    BackendAvailability, ResolvedWorkspaceBinding, WorkspaceResolutionError,
    resolve_workspace_binding,
};
