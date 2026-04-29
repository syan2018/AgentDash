mod entity;
mod identity_contract;
mod repository;
mod value_objects;

pub use entity::Workspace;
pub use identity_contract::{
    GitWorkspaceIdentityContract, GitWorkspaceIdentityHints, GitWorkspaceMatchMode,
    identity_payload_from_detected_facts, identity_payload_matches_detected_facts,
    LocalDirIdentityContract, LocalDirIdentityHints, LocalDirMatchMode,
    P4WorkspaceIdentityContract, P4WorkspaceIdentityHints, P4WorkspaceMatchMode,
    identity_payload_matches, normalize_git_remote, normalize_identity_payload, normalize_path_key,
};
pub use repository::WorkspaceRepository;
pub use value_objects::{
    WorkspaceBinding, WorkspaceBindingStatus, WorkspaceIdentityKind, WorkspaceResolutionPolicy,
    WorkspaceStatus,
};
