pub mod backend_sync;
pub mod detection;
pub mod resolution;

pub use backend_sync::{
    WorkspaceBindingSyncResult, WorkspaceInventoryCandidate, list_project_workspace_candidates,
    sync_project_backend_workspace_bindings,
};
pub use detection::{
    WorkspaceDetectionError, WorkspaceDetectionResult, detect_workspace_from_backend,
};
pub use resolution::{
    BackendAvailability, ResolvedWorkspaceBinding, WorkspaceResolutionError,
    resolve_workspace_binding, resolve_workspace_binding_with_allowed_backends,
};
