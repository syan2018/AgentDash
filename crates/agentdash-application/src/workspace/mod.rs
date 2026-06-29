pub mod backend_sync;
pub mod detection;
pub mod resolution;

pub use backend_sync::{
    WorkspaceBindingSyncResult, WorkspaceDirectoryFact, WorkspaceDirectoryFactApplyResult,
    WorkspaceInventoryCandidate, apply_workspace_directory_fact,
    derive_workspace_status_from_bindings, directory_fact_matches_identity,
    list_project_workspace_candidates, sync_project_backend_workspace_bindings,
    workspace_directory_fact_from_detection, workspace_inventory_from_detection,
    workspace_matches_directory_fact,
};
pub use detection::{
    WorkspaceDetectionError, WorkspaceDetectionResult, detect_workspace_from_backend,
};
pub use resolution::{
    BackendAvailability, ResolvedWorkspaceBinding, WorkspaceResolutionError,
    resolve_workspace_binding, resolve_workspace_binding_with_allowed_backends,
};
