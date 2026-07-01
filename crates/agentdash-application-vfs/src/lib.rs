pub mod access_policy;
pub mod apply_patch;
pub mod binding_resolver;
pub mod inline_persistence;
pub mod materialization;
pub mod mount;
pub mod mount_file_discovery;
pub mod mount_inline;
pub mod mount_project;
pub mod mount_routine;
pub mod mount_skill_asset;
pub mod mount_workspace;
pub mod mutation_dispatcher;
pub(crate) mod mutation_queue;
pub mod path;
pub mod provider;
pub(crate) mod provider_inline;
pub(crate) mod provider_routine;
pub(crate) mod provider_skill_asset;
pub mod rewrite;
pub mod search;
pub mod service;
pub mod surface;
pub mod surface_query;
pub mod tools;
pub mod types;

pub use access_policy::{
    compile_whole_mount_runtime_vfs_access_policy,
    compile_whole_mount_runtime_vfs_access_policy_with_source, runtime_vfs_path_pattern_matches,
    runtime_vfs_policy_admits,
};
pub use agentdash_application_ports::vfs_materialization::VfsMaterializationTransport;
pub use agentdash_application_ports::vfs_surface_runtime::VfsSurfaceRuntimeProjection;
pub use agentdash_spi::{
    RuntimeVfsAccessPolicy, RuntimeVfsAccessRule, RuntimeVfsAccessSource, RuntimeVfsOperation,
    RuntimeVfsPathPattern,
};
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
    mount_purpose,
};
pub use mount_file_discovery::{
    BUILTIN_GUIDELINE_RULES, BUILTIN_SKILL_RULES, DiscoveredMountFile,
    MountFileDiscoveryDiagnostic, MountFileDiscoveryResult, MountFileDiscoveryRule,
    discover_memory_vfs_files, discover_mount_files,
};
pub use mount_inline::{
    build_context_container_mount, list_inline_entries, normalize_inline_files,
    parse_inline_mount_owner,
};
pub use mount_project::{
    append_agent_knowledge_mounts, apply_project_vfs_mount_exposure_grants, build_derived_vfs,
    build_project_agent_knowledge_vfs, build_project_vfs_mount_mount, effective_context_containers,
};
pub use mount_routine::build_routine_mount;
pub use mount_skill_asset::{
    append_lifecycle_skill_asset_projection, build_project_skill_asset_management_mount,
    lifecycle_mount_has_skill_asset_projection, list_lifecycle_skill_asset_projection,
    read_lifecycle_skill_asset_projection, search_lifecycle_skill_asset_projection,
};
pub use mount_workspace::{build_workspace_vfs, selected_workspace_binding, workspace_mount};
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
pub use provider_inline::InlineFsMountProvider;
pub use provider_routine::RoutineMountProvider;
pub use provider_skill_asset::SkillAssetFsMountProvider;
pub use search::TextSearchParams;
pub use service::{BasicTextSearchRequest, VfsService};
pub use surface::{
    ResolvedMountEditCapabilities, ResolvedMountPurpose, ResolvedMountSummary, ResolvedVfsSurface,
    ResolvedVfsSurfaceSource,
};
pub use surface_query::build_surface_summary;
pub use types::{
    ApplyPatchRequest, ApplyPatchResult, BinaryReadResult, ExecRequest, ExecResult, ListOptions,
    ListResult, MultiMountPatchResult, PatchEntryError, ReadResult, ResourceRef, RuntimeFileEntry,
};
