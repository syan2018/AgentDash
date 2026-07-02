pub mod authorization;
pub mod management;
pub mod mcp_probe_target;
pub mod project_access;
pub mod runner_registration;
pub mod runtime_summary;

pub use authorization::{
    BackendAuthorizationError, BackendAuthorizationService, BackendPermission,
    can_manage_global_backend_scope,
};
pub use management::{
    CreateBackendInput, EnrollLocalBackendRequest, EnrollLocalBackendResult, EnrollmentSource,
    EnsureLocalRuntimeInput, LocalRuntimeScopeInput, REGISTRATION_SOURCE_DESKTOP_ACCESS_TOKEN,
    REGISTRATION_SOURCE_RUNNER_REGISTRATION_TOKEN, add_backend_record, enroll_local_backend,
    ensure_local_runtime_record, generate_backend_auth_token, remove_backend_record,
};
pub use mcp_probe_target::{
    McpProbeBackendTarget, McpProbeBackendTargetResolutionError, ResolvedMcpProbeBackendTarget,
    resolve_mcp_probe_backend_target,
};
pub use project_access::{
    EnsureProjectBackendAccessGrantInput, EnsureProjectBackendAccessGrantResult,
    PROJECT_BACKEND_ACCESS_NOTE_RUNNER_REGISTRATION_TOKEN, ProjectBackendAccessGrantSource,
    ensure_project_backend_access_grant,
};
pub use runtime_summary::{
    BackendRuntimeExecutorSnapshot, BackendRuntimeExecutorSummary, BackendRuntimeOnlineSnapshot,
    BackendRuntimeSummary, list_backend_runtime_summaries, project_backend_runtime_summaries,
};
