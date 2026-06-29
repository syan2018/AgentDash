# Research: runtime surface/session assembly map

- Query: Research C: runtime surface/session assembly map for `RuntimeVfsAccessPolicy` carrier and runtime call sites.
- Scope: internal
- Date: 2026-06-30

## Findings

### Files found

- `crates/agentdash-spi/src/connector/mod.rs` — connector-facing `ExecutionContext`, `ExecutionSessionFrame.vfs`, and `CapabilityState.vfs` definitions.
- `crates/agentdash-application-agentrun/src/agent_run/frame/runtime_launch.rs` — AgentRun-owned `FrameLaunchSurface` and `FrameLaunchEnvelope` assembly/gate before RuntimeSession launch.
- `crates/agentdash-application-ports/src/frame_launch_envelope.rs` — neutral RuntimeSession launch DTO carrying typed `FrameLaunchSurface`.
- `crates/agentdash-application-runtime-session/src/session/launch/plan.rs` — `LaunchPlan::build` copies launch VFS into `ExecutionContext.session.vfs`.
- `crates/agentdash-application-runtime-session/src/session/launch/preparation.rs` and `connector_start.rs` — prepared turn tool assembly and connector launch surface.
- `crates/agentdash-application-agentrun/src/agent_run/runtime_surface.rs` and `crates/agentdash-application-ports/src/agent_run_surface.rs` — AgentRun current runtime surface query/adoption shape.
- `crates/agentdash-application/src/runtime_tools/provider.rs` and `vfs_provider.rs` — session runtime tool composer and VFS runtime tool provider.
- `crates/agentdash-application-vfs/src/tools/common.rs`, `factory.rs`, `tools/fs/*.rs`, `materialization.rs` — runtime VFS handle, tool construction, tool normalization, shell/materialization surfaces.
- `crates/agentdash-application-vfs/src/mount_project.rs`, `crates/agentdash-domain/src/common/agent_config.rs` — current ProjectAgent VFS grant shape and mount capability pruning.
- `crates/agentdash-application-agentrun/src/agent_run/effective_capability.rs`, `permission_runtime_surface_update.rs`, `crates/agentdash-domain/src/workflow/value_objects/capability.rs`, `permission/entity.rs` — PermissionGrant projection and requested path model.

### Code patterns

#### Typed VFS assembly into `ExecutionContext.session.vfs`

- `ExecutionSessionFrame` owns connector-facing per-turn environment and has `pub vfs: Option<Vfs>` at `crates/agentdash-spi/src/connector/mod.rs:64` and `:75`. `ExecutionContext` simply pairs `session` and `turn` at `:127`.
- `CapabilityState` also contains `pub vfs: VfsDimension` at `crates/agentdash-spi/src/connector/mod.rs:365` and `:371`; `VfsDimension.active` is `Option<Vfs>` at `:290` and `:292`. This is a surface projection, not a separate authorization fact.
- AgentRun launch has a typed `FrameLaunchSurface { capability_state, vfs, mcp_servers, execution_profile }` at `crates/agentdash-application-agentrun/src/agent_run/frame/runtime_launch.rs:95`. `FrameLaunchSurface::new` rejects mismatch when `capability_state.vfs.active != vfs` at `:139`, so launch treats independent VFS and embedded capability VFS as one consistent surface.
- `FrameLaunchSurface::from_surface_draft` is the launch-ready gate at `crates/agentdash-application-agentrun/src/agent_run/frame/runtime_launch.rs:160`. The neutral ports DTO mirrors the same `FrameLaunchSurface` at `crates/agentdash-application-ports/src/frame_launch_envelope.rs:43` and exposes `launch_vfs()` at `:83`.
- `LaunchPlan::build` reads `let vfs = input.launch_envelope.launch_vfs().clone();` at `crates/agentdash-application-runtime-session/src/session/launch/plan.rs:176`, then creates `ExecutionSessionFrame { ... vfs: if has_vfs { Some(vfs) } else { None }, ... }` at `:283` and `:289`. This is the concrete assembly point into `ExecutionContext.session.vfs`.
- `TurnPreparer` assembles runtime tools before connector start: `deps.assemble_tool_surface(&session_id, &context)` at `crates/agentdash-application-runtime-session/src/session/launch/preparation.rs:110`, writes `context.turn.assembled_tools` at `:112`, activates the turn with `context.session.clone()` at `:194`, and returns `connector_context: Some(context)` at `:350`.
- `ConnectorStarter` consumes that exact `connector_context` at `crates/agentdash-application-runtime-session/src/session/launch/connector_start.rs:30` and passes it to `connector.prompt(...)` at `:43`.

#### AgentRun frame/runtime launch surfaces

- `AgentFrameBuilder::with_capability_state` splits `CapabilityState` into frame surfaces at `crates/agentdash-application-agentrun/src/agent_run/frame/builder.rs:140`; `with_vfs_typed` writes the independent VFS surface at `:152`; `with_surface_draft` applies both at `:196`.
- `project_capability_state_from_frame` loads `effective_capability_json` and then lets `vfs_surface_json` override `state.vfs.active` at `crates/agentdash-application-agentrun/src/agent_run/runtime_capability.rs:50` and `:62`. `capability_state_to_frame_surfaces` extracts `state.vfs.active` back into `vfs_surface_json` at `:82`.
- `AgentRunRuntimeSurfaceQuery::resolve_surface` requires current frame typed capability and typed VFS at `crates/agentdash-application-agentrun/src/agent_run/runtime_surface.rs:326` and `:334`, then projects capability from frame at `:342` and returns `AgentRunRuntimeSurface { capability_state: projected_capability_state, vfs, ... }` at `:357` and `:370`.
- The public port struct carries both `capability_state` and `vfs` in `AgentRunRuntimeSurface` at `crates/agentdash-application-ports/src/agent_run_surface.rs:45` and `:55`.

#### Runtime tool surface construction

- API bootstrap composes domain providers, with VFS first, into `SessionRuntimeToolComposer` at `crates/agentdash-api/src/bootstrap/session.rs:440` and `:465`.
- `SessionRuntimeToolComposer` calls each provider's `build_tools(context)` at `crates/agentdash-application/src/runtime_tools/provider.rs:60` and `:68`.
- `shared_runtime_vfs_from_context` clones `context.session.vfs` into `SharedRuntimeVfs::new(vfs)` at `crates/agentdash-application/src/runtime_tools/provider.rs:89` and `:95`.
- `VfsRuntimeToolProvider::build_tools` obtains that shared VFS at `crates/agentdash-application/src/runtime_tools/vfs_provider.rs:61` and `:66`, then passes it to `VfsToolFactoryInput` at `:76`.
- `SharedRuntimeVfs` currently wraps only `Vfs`; it is constructed at `crates/agentdash-application-vfs/src/tools/common.rs:49`, snapshots at `:54`, and replaces at `:59`.
- `VfsToolFactoryInput` currently contains `shared_vfs`, `overlay`, `identity`, `session_id`, `turn_id`, and `flow` at `crates/agentdash-application-vfs/src/tools/factory.rs:143`. No runtime access policy carrier exists here.

#### Runtime VFS tool and materialization call sites needing policy

- `fs_read` normalizes to `ResourceRef` with `resolve_uri_path` at `crates/agentdash-application-vfs/src/tools/fs/read.rs:134`, then calls `service.stat`, `read_text_range`, and `read_binary` on the target.
- `fs_glob` normalizes with `resolve_uri_path` at `crates/agentdash-application-vfs/src/tools/fs/glob.rs:87`, then calls `service.list`.
- `fs_grep` normalizes with `resolve_uri_path` at `crates/agentdash-application-vfs/src/tools/fs/grep.rs:158`, then continues into service search. Policy must gate the normalized search root before provider search.
- `fs_apply_patch` passes the patch to `service.apply_patch_multi` at `crates/agentdash-application-vfs/src/tools/fs/apply_patch.rs:131`; because patch headers can address multiple mount/path targets, policy either belongs in `apply_patch_multi` after patch target normalization or in a shared preflight that normalizes all patch entry targets.
- Real OS `shell_exec` normalizes `cwd` through `resolve_uri_path` at `crates/agentdash-application-vfs/src/tools/fs/shell.rs:172`, checks only provider mount `Exec` via `resolve_mount(... Exec)` at `:180`, then invokes materialization rewrite at `:185`.
- Platform shell uses tool-level checks only: `ensure_read` at `crates/agentdash-application-vfs/src/tools/fs/platform_shell.rs:411` and `ensure_write` at `:423`. It directly performs VFS operations after resolving args: `parse_mount_uri` at `:371`, `normalize_mount_relative_path` at `:385`, and service calls for list/read/write/delete/rename around `:177`, `:235`, `:239`, `:264`, `:291`, `:333`, `:354`.
- Materialization rewrite parses command VFS URI candidates at `crates/agentdash-application-vfs/src/materialization.rs:63`, checks only source mount `Read` at `:64`, normalizes at `:66`, and later repeats parse/read planning in `local_path_for_uri` at `:207`, `:210`, and `plan_entries` at `:242`. These need policy for `Read` on source URIs and `Exec` on the exec mount context before rewriting/transport.

#### Grant and PermissionGrant limitations

- Current ProjectAgent VFS preset grant is `AgentVfsAccessGrant { mount_id, capabilities }` at `crates/agentdash-domain/src/common/agent_config.rs:263`; config carries `vfs_access_grants` at `:70`.
- Current VFS mount pruning mutates provider mount capabilities in `apply_agent_vfs_access_grants` at `crates/agentdash-application-vfs/src/mount_project.rs:136`. This is the wrong authorization path D9 wants to converge away from: Project preset exposure should compile policy, not rewrite provider capability semantics as generic admission.
- `PermissionGrant.requested_paths` is `Vec<ToolCapabilityPath>` at `crates/agentdash-domain/src/permission/entity.rs:17` and `:30`.
- `ToolCapabilityPath` is `capability` or `capability::tool` only, at `crates/agentdash-domain/src/workflow/value_objects/capability.rs:34` and parse validation around `:111`-`:136`.
- `AgentRunGrantProjection::from_active_grants` only adds tool admission paths at `crates/agentdash-application-agentrun/src/agent_run/effective_capability.rs:51` and `:87`; `classify_path` treats `path.tool.is_some()` as admission and otherwise frame surface revision at `:59`.
- Permission runtime surface update partitions requested paths at `crates/agentdash-application-agentrun/src/agent_run/permission_runtime_surface_update.rs:123` and applies capability paths into `CapabilityState` at `:151`, with no typed VFS mount/path rule contract.

### Recommended carrier location

Recommended carrier: `RuntimeVfsAccessPolicy` should be part of the same runtime VFS surface as typed `Vfs`, carried in the AgentRun/RuntimeSession launch and runtime surface DTO adjacent to `Vfs`, then wrapped together in runtime VFS tool state.

Concretely:

- Add the model in the VFS/application runtime boundary, e.g. `agentdash-application-vfs` if only runtime tools and materialization consume it, or `agentdash-application-ports` if AgentRun surface and RuntimeSession launch DTOs must carry it across package boundaries.
- Extend `FrameLaunchSurface` with `vfs_access_policy: RuntimeVfsAccessPolicy` next to `vfs`; `FrameLaunchSurface::new/from_surface_draft` should validate that policy mount ids refer to the same `vfs.mounts`. This mirrors the existing `capability_state.vfs.active == vfs` gate and keeps launch-ready facts closed.
- Extend neutral `agentdash_application_ports::frame_launch_envelope::FrameLaunchSurface` and `AgentRunRuntimeSurface` with the same policy next to `vfs`, because these are already the launch/query DTOs that express current AgentRun runtime surface.
- Extend `ExecutionSessionFrame` with `vfs_access_policy: Option<RuntimeVfsAccessPolicy>` or a required policy when `vfs` is present. If staged, `None` should be compiled as “legacy whole-mount exposure for current VFS” only at the launch boundary, not inside tool handlers.
- Replace `SharedRuntimeVfs { inner: Vfs }` with a shared runtime VFS state, e.g. `{ vfs: Vfs, access_policy: RuntimeVfsAccessPolicy }`, and make `replace` update both together. This prevents policy/VFS drift during active runtime adoption.

Do not put `RuntimeVfsAccessPolicy` inside `CapabilityState.vfs` as the primary carrier. `CapabilityState.vfs.active` is already the typed VFS surface projection and is mirrored by `vfs_surface_json`; adding authorization there would blur the existing surface/exposure dimension and make policy look like another capability grant. Do not put it only in VFS-specific tool instances either; shell materialization, platform shell, runtime surface query/adoption, and future relay/MCP rewrites all need the same runtime fact. Do not create a standalone `VfsAccessService` that recomputes from grants at tool time; that would be a second source of truth beside the AgentFrame/runtime surface.

### Smallest staged implementation preserving behavior

1. Define `RuntimeVfsOperation`, `RuntimeVfsAccessRule`, and `RuntimeVfsAccessPolicy` plus matcher over already-normalized mount-relative paths. Include `All` and `Prefix` path patterns, but initially compile only `All` for existing Project exposure.
2. Add a compiler from current launch VFS/Project preset exposure into whole-mount rules. Preserve existing behavior by granting each currently exposed mount exactly its current provider operations as runtime policy rules. This makes policy a real runtime fact without narrowing behavior yet.
3. Thread the policy through `FrameLaunchSurface -> FrameLaunchEnvelope -> LaunchPlan::build -> ExecutionSessionFrame -> VfsRuntimeToolProvider -> SharedRuntimeVfs` in lockstep with `Vfs`.
4. Enforce policy after `resolve_uri_path`/`parse_mount_uri` normalization and before provider dispatch/materialization: `fs_read` Read, `fs_glob` List, `fs_grep` Search, `fs_apply_patch` ApplyPatch/Write per patch target, real `shell_exec` Exec for cwd and Read for materialized source URIs, platform shell Read/List/Write per resolved operation.
5. Only after policy gates are in place, rename/narrow `AgentVfsAccessGrant` and `apply_agent_vfs_access_grants` semantics so Project preset grants compile exposure/policy instead of mutating provider mount capability as authorization. Provider `MountCapability` remains support.
6. Leave PermissionGrant VFS path rules as a documented gap unless/until a typed PermissionGrant path model exists. Current `ToolCapabilityPath` cannot carry mount/path/operation, so string parsing would create an unstable contract.

### External references

None. This was internal code/spec research only.

## Related specs

- `.trellis/spec/backend/vfs/architecture.md:16` — runtime VFS handle and URI resolution are centralized in VFS tools common state.
- `.trellis/spec/backend/vfs/architecture.md:62` — session runtime tool composition root uses `ExecutionContext` and domain providers.
- `.trellis/spec/backend/vfs/vfs-access.md:385` — runtime tools include mounts/fs/shell and shell operates through VFS materialization contract.
- `.trellis/spec/backend/vfs/vfs-materialization.md:1` — shell/materialization contract requires VFS URI rewrite/materialization before local shell execution.
- `.trellis/spec/backend/session/architecture.md:55` — frame construction emits launch-ready final facts for connector launch.
- `.trellis/spec/backend/session/architecture.md:66` — RuntimeSession bridges AgentRun admission but does not own Grant authorization.
- `.trellis/tasks/06-30-runtime-vfs-access-policy-cleanup/design.md` — D9 design requires policy between normalization and provider dispatch/materialization.

## Caveats / Not Found

- No existing `RuntimeVfsAccessPolicy` type or equivalent carrier was found.
- No typed PermissionGrant VFS mount/path rule model was found. Current `ToolCapabilityPath` only supports `capability` and `capability::tool`.
- The spec mentions VFS provider files under `vfs/tools/provider.rs` and `vfs/tools/vfs_provider.rs`, but current code places runtime tool provider/composer under `crates/agentdash-application/src/runtime_tools/` and VFS tool implementation under `crates/agentdash-application-vfs/src/tools/`.
- Existing materialization APIs receive `Vfs` but not policy; shell command VFS URI rewrite therefore currently checks provider mount capabilities only.
