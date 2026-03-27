pub mod resolution;

pub use resolution::{
    BackendAvailability, ResolvedWorkspaceBinding, WorkspaceResolutionError,
    resolve_workspace_binding,
};
