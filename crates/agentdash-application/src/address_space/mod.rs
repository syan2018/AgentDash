pub mod inline_persistence;
pub mod mount;
pub mod path;
pub mod provider;
pub mod provider_inline;
pub mod provider_lifecycle;
pub mod relay_service;
pub mod tools;
pub mod types;

pub use mount::{
    PROVIDER_INLINE_FS, PROVIDER_LIFECYCLE_VFS, PROVIDER_RELAY_FS, SessionMountTarget,
    build_context_container_mount, build_derived_address_space, build_lifecycle_mount,
    build_workspace_address_space, container_visible_for_target, effective_context_containers,
    inline_files_from_mount, list_inline_entries, map_container_capabilities,
    normalize_inline_files, selected_workspace_binding, workspace_mount_from_policy,
};
pub use path::{
    capability_name, format_mount_uri, join_root_ref, normalize_mount_relative_path,
    parse_mount_uri, resolve_mount, resolve_mount_id,
};
pub use provider::{
    ConfigurableProviderInfo, MountError, MountOperationContext, MountProvider,
    MountProviderRegistry, MountProviderRegistryBuilder, SearchMatch, SearchQuery, SearchResult,
};
pub use provider_inline::InlineFsMountProvider;
pub use provider_lifecycle::LifecycleMountProvider;
pub use relay_service::{RelayAddressSpaceService, TextSearchParams};
pub use types::{ExecRequest, ExecResult, ListOptions, ListResult, ReadResult, ResourceRef};
