# Research: grants and policy inputs

- Query: Research B - grants and policy inputs for RuntimeVfsAccessPolicy cleanup
- Scope: internal
- Date: 2026-06-30

## Findings

### Files Found

- `.trellis/tasks/06-30-runtime-vfs-access-policy-cleanup/prd.md` - task requirements: split tool capability, provider support, and runtime VFS policy.
- `.trellis/tasks/06-30-runtime-vfs-access-policy-cleanup/design.md` - proposed `RuntimeVfsAccessPolicy` model and staging guidance.
- `.trellis/tasks/06-30-runtime-vfs-access-policy-cleanup/implement.md` - research split and cleanup-first implementation candidate.
- `.trellis/spec/backend/vfs/architecture.md` - VFS ownership, tool composition, and runtime mount invariants.
- `.trellis/spec/backend/vfs/vfs-access.md` - address model, Project VFS Mount contract, runtime tools, and path normalization contract.
- `.trellis/spec/backend/vfs/vfs-materialization.md` - shell/materialization path and mount capability constraints.
- `.trellis/spec/backend/permission/grant-lifecycle.md` - durable `PermissionGrant` requested path lifecycle and active grant projection.
- `.trellis/spec/backend/session/architecture.md` - launch/frame/runtime surface boundary and grant/admission ownership.
- `crates/agentdash-domain/src/common/agent_config.rs` - `AgentPresetConfig.vfs_access_grants` and `AgentVfsAccessGrant`.
- `crates/agentdash-application-vfs/src/mount_project.rs` - Project VFS mount builder and grant pruning.
- `crates/agentdash-application/src/frame_construction/owner_bootstrap.rs` - Project owner bootstrap applies preset VFS grants.
- `crates/agentdash-application/src/frame_construction/composer_project_agent.rs` - ProjectAgent preset grants copied into owner bootstrap.
- `crates/agentdash-application/src/frame_construction/request_assembler.rs` - companion selected ProjectAgent applies preset grants to prepared/sliced VFS.
- `crates/agentdash-domain/src/workflow/value_objects/capability.rs` - `ToolCapabilityPath` only models capability/tool paths.
- `crates/agentdash-domain/src/permission/entity.rs` - `PermissionGrant.requested_paths` stores `Vec<ToolCapabilityPath>`.
- `crates/agentdash-application/src/permission/compiler.rs` - PermissionGrant compiler emits only tool capability directives.
- `crates/agentdash-application-agentrun/src/agent_run/effective_capability.rs` - active grants are projected to tool-admission paths by effect frame.
- `crates/agentdash-application-agentrun/src/agent_run/permission_runtime_surface_update.rs` - capability-level grants mutate visible `CapabilityState`; tool-level grants remain admission-only.
- `crates/agentdash-spi/src/connector/mod.rs` - `ExecutionContext` carries session VFS and turn capability state, but no runtime VFS policy.
- `crates/agentdash-application/src/runtime_tools/provider.rs` and `vfs_provider.rs` - runtime tool composition turns `ExecutionContext.session.vfs` into `SharedRuntimeVfs` and uses `CapabilityState` for tool assembly.
- `crates/agentdash-application-vfs/src/tools/common.rs` - tool path resolution normalizes user path to `ResourceRef`.
- `crates/agentdash-application-vfs/src/path.rs` - `parse_mount_uri` normalizes paths and `resolve_mount` checks mount capability only.
- `crates/agentdash-application-vfs/src/materialization.rs` - shell URI rewrite checks only exec/read mount capabilities.

### Current Project VFS Grant Shape

- `AgentPresetConfig` still names the field `vfs_access_grants`, with a comment saying it is the Project VFS mounts the Agent can access (`crates/agentdash-domain/src/common/agent_config.rs:68`).
- The persisted/runtime grant type is `AgentVfsAccessGrant { mount_id, capabilities }`; it has no surface ref, no path scope, no operation source, and no non-project mount typing (`crates/agentdash-domain/src/common/agent_config.rs:262`).
- ProjectAgent composition copies `preset_config.vfs_access_grants` into `OwnerBootstrapSpec.agent_vfs_access_grants` (`crates/agentdash-application/src/frame_construction/composer_project_agent.rs:100`, `crates/agentdash-application/src/frame_construction/composer_project_agent.rs:110`).
- Owner bootstrap applies those grants only when the owner is Project-scoped; it appends agent knowledge first, then calls `apply_agent_vfs_access_grants` (`crates/agentdash-application/src/frame_construction/owner_bootstrap.rs:363`, `crates/agentdash-application/src/frame_construction/owner_bootstrap.rs:369`).
- Companion paths also apply the selected ProjectAgent preset grants to prepared/sliced VFS (`crates/agentdash-application/src/frame_construction/request_assembler.rs:298`, `crates/agentdash-application/src/frame_construction/request_assembler.rs:857`).

### Build, Apply, Name, and Prune Behavior

- Project VFS mount construction marks mounts with `agentdash_project_vfs_mount = true` metadata and fills mount-level `capabilities`; empty project mount capabilities default to read/list/search (`crates/agentdash-application-vfs/src/mount_project.rs:183`, `crates/agentdash-application-vfs/src/mount_project.rs:199`, `crates/agentdash-application-vfs/src/mount_project.rs:203`).
- `apply_agent_vfs_access_grants` builds a map by trimmed mount id (`crates/agentdash-application-vfs/src/mount_project.rs:136`).
- It skips every mount that is not a Project VFS mount (`crates/agentdash-application-vfs/src/mount_project.rs:144`).
- Missing grant for a Project VFS mount clears mount capabilities (`crates/agentdash-application-vfs/src/mount_project.rs:148`).
- Present grant intersects the existing provider/project mount capabilities with grant capabilities (`crates/agentdash-application-vfs/src/mount_project.rs:152`).
- If write is removed, `default_write` is disabled (`crates/agentdash-application-vfs/src/mount_project.rs:156`).
- Project VFS mounts with empty capabilities are pruned from `vfs.mounts` (`crates/agentdash-application-vfs/src/mount_project.rs:161`).
- The existing regression test states the actual scope: ProjectAgent VFS grants do not constrain the Agent memory mount (`crates/agentdash-application-vfs/src/mount_project.rs:480`).

Conclusion: this is not generic runtime VFS authorization. It is ProjectAgent preset-driven Project VFS mount exposure plus mount capability narrowing/pruning.

### PermissionGrant Requested Paths

- `PermissionGrant.requested_paths` is `Vec<ToolCapabilityPath>` and is documented as capability paths (`crates/agentdash-domain/src/permission/entity.rs:29`).
- `ToolCapabilityPath` has only `capability: String` and optional `tool: Option<String>` (`crates/agentdash-domain/src/workflow/value_objects/capability.rs:33`).
- Its parser accepts either `capability` or `capability::tool`, rejects multiple `::`, and does not model mount id, VFS path, path pattern, surface ref, or operation set (`crates/agentdash-domain/src/workflow/value_objects/capability.rs:101`, `crates/agentdash-domain/src/workflow/value_objects/capability.rs:135`).
- The application compiler maps every requested path to a `tool` dimension `capability_directive`; it emits no VFS dimension/effect (`crates/agentdash-application/src/permission/compiler.rs:27`, `crates/agentdash-application/src/permission/compiler.rs:32`, `crates/agentdash-application/src/permission/compiler.rs:46`).
- AgentRun active grant projection treats tool-level paths as admission projection and capability-level paths as frame surface revision (`crates/agentdash-application-agentrun/src/agent_run/effective_capability.rs:59`, `crates/agentdash-application-agentrun/src/agent_run/effective_capability.rs:67`).
- Permission runtime surface update applies only those surface paths to `CapabilityState.tool`; tool-level grants short-circuit as no-surface updates (`crates/agentdash-application-agentrun/src/agent_run/permission_runtime_surface_update.rs:123`, `crates/agentdash-application-agentrun/src/agent_run/permission_runtime_surface_update.rs:150`, `crates/agentdash-application-agentrun/src/agent_run/permission_runtime_surface_update.rs:339`).
- The wire response exposes `requested_paths` as `Vec<String>`, but this is DTO flattening of tool capability paths, not a VFS path contract (`crates/agentdash-contracts/src/system/permission.rs:80`).

Conclusion: `PermissionGrant.requested_paths` currently has no typed VFS mount/path contract. It is only a capability/tool path domain. Any MVP that parses strings like `vfs:main:/foo` out of `ToolCapabilityPath` would fake unsupported semantics.

### Runtime VFS Tool and Materialization Inputs

- `ExecutionSessionFrame` carries `vfs: Option<Vfs>` and `ExecutionTurnFrame` carries `capability_state`; there is no runtime VFS access policy field (`crates/agentdash-spi/src/connector/mod.rs:75`, `crates/agentdash-spi/src/connector/mod.rs:104`, `crates/agentdash-spi/src/connector/mod.rs:127`).
- `FrameLaunchSurface` likewise carries `capability_state`, `vfs`, MCP servers, and execution profile, but no access policy (`crates/agentdash-application-ports/src/frame_launch_envelope.rs:42`).
- Tool composition builds `SharedRuntimeVfs` directly from `context.session.vfs` (`crates/agentdash-application/src/runtime_tools/provider.rs:89`) and passes `context.turn.capability_state` to the VFS tool factory (`crates/agentdash-application/src/runtime_tools/vfs_provider.rs:76`, `crates/agentdash-application/src/runtime_tools/vfs_provider.rs:82`).
- `VfsToolFactory` uses `CapabilityState` to decide which VFS tools are injected (`crates/agentdash-application-vfs/src/tools/factory.rs:46`, `crates/agentdash-application-vfs/src/tools/factory.rs:52`).
- Individual tools normalize path first via `resolve_uri_path`: `fs_read` (`crates/agentdash-application-vfs/src/tools/fs/read.rs:132`), `fs_glob` (`crates/agentdash-application-vfs/src/tools/fs/glob.rs:86`), `fs_grep` (`crates/agentdash-application-vfs/src/tools/fs/grep.rs:157`), and `shell_exec.cwd` (`crates/agentdash-application-vfs/src/tools/fs/shell.rs:172`).
- `parse_mount_uri` returns a normalized `ResourceRef` and resolves links (`crates/agentdash-application-vfs/src/path.rs:193`); tests assert normalization before return (`crates/agentdash-application-vfs/src/path.rs:665`).
- `resolve_mount` only checks mount existence and mount capability support (`crates/agentdash-application-vfs/src/path.rs:246`).
- VFS service dispatch checks mount capability for read/write/list but has no runtime policy input (`crates/agentdash-application-vfs/src/service.rs:125`, `crates/agentdash-application-vfs/src/service.rs:290`, `crates/agentdash-application-vfs/src/service.rs:497`).
- Shell materialization rewrite checks `Exec` on the cwd mount and `Read` on URI source mounts (`crates/agentdash-application-vfs/src/materialization.rs:58`, `crates/agentdash-application-vfs/src/materialization.rs:63`).

### Recommended MVP Compiler Boundary

The minimal honest compiler input for `RuntimeVfsAccessPolicy` is:

1. The normalized launch/runtime VFS surface after Project owner composition and existing Project VFS preset pruning.
2. The ProjectAgent preset Project VFS exposure facts currently named `AgentVfsAccessGrant`, but treated as Project VFS mount exposure/preset input only.
3. Optional system/runtime projection facts that already have typed mount/path/operation semantics, if present near the projector that creates the mount. Do not infer them from `PermissionGrant.requested_paths`.

The MVP compiler should produce whole-mount rules for Project VFS preset exposure, with `source = ProjectPreset` or equivalent. It should not compile `PermissionGrant` into path-level VFS policy until a new typed request/fact exists, for example a distinct `VfsAccessGrantRequest { surface_ref, mount_id, path_scope, operations }` or a separate PermissionGrant effect payload that is not `ToolCapabilityPath`.

The enforcement boundary should be after `resolve_uri_path`/`parse_mount_uri` returns normalized `ResourceRef`, and before provider dispatch/materialization. This matches the existing normalization contract and avoids admitting absolute paths or `..` escapes into policy matching.

### Names That Should Change

- `AgentVfsAccessGrant` implies generic Agent VFS authorization; current behavior is ProjectAgent Project VFS mount preset exposure. Rename/narrow toward `ProjectVfsMountExposureGrant`, `ProjectVfsMountPresetGrant`, or `ProjectAgentProjectVfsMountGrant`.
- `AgentPresetConfig.vfs_access_grants` implies all VFS access. Rename toward `project_vfs_mount_exposures` or `project_vfs_mount_grants`.
- `OwnerBootstrapSpec.agent_vfs_access_grants` should follow the narrowed name because owner bootstrap only applies it in Project owner composition.
- `apply_agent_vfs_access_grants` should become a Project-scoped function name, such as `apply_project_vfs_mount_exposures` or `apply_project_vfs_mount_preset_grants`.
- Tests named `project_agent_vfs_grants_*` should use the narrowed term so future readers do not assume these grants constrain `agent`, `lifecycle`, `routine`, `canvas`, `skill_asset`, or workspace mounts.

The cleanup-first path is to rename/narrow the current Project VFS preset first, then add `RuntimeVfsAccessPolicy` as the single runtime admission model. Do not add a parallel VFS grant service while leaving `AgentVfsAccessGrant` as a generic-sounding authorization type.

## Related Specs

- `.trellis/spec/backend/vfs/architecture.md` - runtime mount/provider/tool composition invariants.
- `.trellis/spec/backend/vfs/vfs-access.md` - address model and Project VFS Mount/runtime tool contracts.
- `.trellis/spec/backend/vfs/vfs-materialization.md` - materialization path and mount capability rules.
- `.trellis/spec/backend/permission/grant-lifecycle.md` - PermissionGrant requested path lifecycle.
- `.trellis/spec/backend/session/architecture.md` - `ExecutionContext`, AgentRun admission, and frame surface ownership.

## External References

- None. This research used internal specs and code only.

## Caveats / Not Found

- No current typed VFS path/mount contract was found inside `PermissionGrant.requested_paths`.
- No existing `RuntimeVfsAccessPolicy` field was found in `ExecutionContext`, `FrameLaunchSurface`, `SharedRuntimeVfs`, or VFS tool factory inputs.
- No evidence was found that PermissionGrant can honestly contribute VFS path-level rules today without a new typed schema/domain fact.
- The report intentionally does not recommend a parallel grant path that preserves misleading generic `AgentVfsAccessGrant` semantics.
