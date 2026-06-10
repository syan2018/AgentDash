pub mod apply_patch;
pub mod binding_resolver;
pub mod inline_persistence;
pub mod lifecycle_catalog;
pub mod materialization;
pub mod mount;
pub mod mutation_dispatcher;
pub(crate) mod mutation_queue;
pub mod path;
pub mod provider;
pub mod provider_canvas;
pub mod provider_inline;
pub mod provider_lifecycle;
pub mod provider_routine;
pub mod provider_skill_asset;
pub mod rewrite;
pub mod service;
pub mod surface;
pub mod surface_query;
pub mod tools;
pub mod types;

pub use agentdash_application_ports::vfs_materialization::VfsMaterializationTransport;
pub use apply_patch::{
    AffectedPaths as ApplyPatchAffectedPaths, ApplyPatchError, ApplyPatchTarget, FsPatchTarget,
    NormalizedPatchEntryTargets, ParseError as ApplyPatchParseError, PatchEntry, PatchPathTarget,
    apply_entries_to_target, apply_patch_to_target, normalize_patch_entry_targets,
    parse_patch as parse_patch_text, parse_patch_path_target,
};
pub use binding_resolver::{ResolveBindingsOutput, ResolvedBinding, resolve_context_bindings};
pub use materialization::{
    MaterializationRewrite, RewriteJsonArgumentsInput, RewriteJsonArgumentsOutput,
    RewriteShellCommandInput, RewriteShellCommandOutput, VfsMaterializationService,
};
pub use mount::{
    PROJECT_VFS_MOUNT_CONTAINER_ID, PROVIDER_CANVAS_FS, PROVIDER_INLINE_FS, PROVIDER_LIFECYCLE_VFS,
    PROVIDER_RELAY_FS, PROVIDER_ROUTINE_VFS, PROVIDER_SKILL_ASSET_FS, SessionMountTarget,
    append_agent_knowledge_mounts, append_canvas_mounts, append_lifecycle_skill_asset_projection,
    apply_agent_vfs_access_grants, build_canvas_mount, build_canvas_mount_id,
    build_context_container_mount, build_derived_vfs, build_lifecycle_mount,
    build_lifecycle_mount_with_node_scope, build_lifecycle_mount_with_ports,
    build_project_agent_knowledge_vfs, build_project_skill_asset_management_mount,
    build_project_vfs_mount_mount, build_routine_mount, build_workspace_vfs,
    effective_context_containers, list_inline_entries, mount_purpose, normalize_inline_files,
    parse_inline_mount_owner, selected_workspace_binding, workspace_mount,
};
pub use mutation_dispatcher::{
    BinaryMutationResult, InlineStorageKey, TextMutationResult, VfsMutationDispatcher,
    VfsMutationError, inline_storage_key_from_mount,
};
pub use path::{
    MountId, MountRelativePath, PathPolicy, RootRef, VfsUri, capability_name, format_mount_uri,
    join_root_ref, normalize_mount_relative_path, parse_mount_uri, resolve_mount, resolve_mount_id,
    validate_vfs,
};
pub use provider::{
    ConfigurableProviderInfo, GrepQuery, MountEditCapabilities, MountError, MountOperationContext,
    MountProvider, MountProviderRegistry, MountProviderRegistryBuilder, SearchMatch,
    SearchOutputMode, SearchQuery, SearchResult,
};
pub use provider_canvas::CanvasFsMountProvider;
pub use provider_inline::InlineFsMountProvider;
pub use provider_lifecycle::LifecycleMountProvider;
pub use provider_routine::RoutineMountProvider;
pub use provider_skill_asset::SkillAssetFsMountProvider;
pub use service::{TextSearchParams, VfsService};
pub use surface::{
    ResolvedMountEditCapabilities, ResolvedMountPurpose, ResolvedMountSummary, ResolvedVfsSurface,
    ResolvedVfsSurfaceSource,
};
pub use surface_query::{VfsSurfaceRuntimeProjection, build_surface_summary};
pub use types::{
    ApplyPatchRequest, ApplyPatchResult, BinaryReadResult, ExecRequest, ExecResult, ListOptions,
    ListResult, MultiMountPatchResult, PatchEntryError, ReadResult, ResourceRef, RuntimeFileEntry,
};
