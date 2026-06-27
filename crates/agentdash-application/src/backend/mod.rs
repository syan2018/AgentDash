pub mod authorization;
pub mod management;
pub mod runner_registration;

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
