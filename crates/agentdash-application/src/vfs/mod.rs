pub mod apply_patch;
pub mod binding_resolver;
pub mod inline_persistence;
pub mod lifecycle_catalog;
pub mod materialization;
pub mod mount;
pub(crate) mod mutation_queue;
pub mod path;
pub mod provider;
pub mod provider_canvas;
pub mod provider_inline;
pub mod provider_lifecycle;
pub mod provider_skill_asset;
pub mod relay_service;
pub mod rewrite;
pub mod surface;
pub mod tools;
pub mod types;

pub use apply_patch::{
    AffectedPaths as ApplyPatchAffectedPaths, ApplyPatchError, ApplyPatchTarget,
    ParseError as ApplyPatchParseError, PatchEntry, apply_entries_to_target, apply_patch_to_fs,
    apply_patch_to_inline_files, apply_patch_to_target, parse_patch as parse_patch_text,
};
pub use binding_resolver::{ResolveBindingsOutput, ResolvedBinding, resolve_context_bindings};
pub use materialization::{
    MaterializationRewrite, RewriteJsonArgumentsInput, RewriteJsonArgumentsOutput,
    RewriteShellCommandInput, RewriteShellCommandOutput, VfsMaterializationService,
    VfsMaterializationTransport,
};
pub use mount::{
    PROJECT_VFS_MOUNT_CONTAINER_ID, PROVIDER_CANVAS_FS, PROVIDER_INLINE_FS, PROVIDER_LIFECYCLE_VFS,
    PROVIDER_RELAY_FS, PROVIDER_SKILL_ASSET_FS, SessionMountTarget, append_agent_knowledge_mounts,
    append_canvas_mounts, append_skill_asset_projection, apply_agent_vfs_access_grants,
    build_canvas_mount, build_canvas_mount_id, build_context_container_mount, build_derived_vfs,
    build_lifecycle_mount, build_lifecycle_mount_with_ports, build_project_agent_knowledge_vfs,
    build_project_skill_asset_management_mount, build_project_vfs_mount_mount,
    build_skill_asset_mount, build_workspace_vfs,
    effective_context_containers, list_inline_entries, mount_container_id, mount_owner_id,
    mount_owner_kind, mount_purpose, normalize_inline_files, parse_inline_mount_owner,
    selected_workspace_binding, workspace_mount,
};
pub use path::{
    MountId, MountRelativePath, PathPolicy, RootRef, VfsUri, capability_name, format_mount_uri,
    join_root_ref, normalize_mount_relative_path, parse_mount_uri, resolve_mount, resolve_mount_id,
    validate_vfs,
};
pub use provider::{
    ConfigurableProviderInfo, MountEditCapabilities, MountError, MountOperationContext,
    MountProvider, MountProviderRegistry, MountProviderRegistryBuilder, SearchMatch, SearchQuery,
    SearchResult,
};
pub use provider_canvas::CanvasFsMountProvider;
pub use provider_inline::InlineFsMountProvider;
pub use provider_lifecycle::LifecycleMountProvider;
pub use provider_skill_asset::SkillAssetFsMountProvider;
pub use relay_service::{RelayVfsService, TextSearchParams};
pub use surface::{
    ResolvedMountEditCapabilities, ResolvedMountOwnerKind, ResolvedMountPurpose,
    ResolvedMountSummary, ResolvedVfsSurface, ResolvedVfsSurfaceSource,
};
pub use types::{
    ApplyPatchRequest, ApplyPatchResult, BinaryReadResult, ExecRequest, ExecResult, ListOptions,
    ListResult, MultiMountPatchResult, PatchEntryError, ReadResult, ResourceRef, RuntimeFileEntry,
};
