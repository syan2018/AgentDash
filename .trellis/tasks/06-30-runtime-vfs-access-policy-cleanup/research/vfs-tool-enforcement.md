# Research: VFS tool enforcement map

- Query: Map mount/path parsing, normalization, existing authorization gates, and test coverage for `mounts_list`, `fs_read`, `fs_glob`, `fs_grep`, write/apply_patch, and `shell_exec`.
- Scope: internal
- Date: 2026-06-30

## Findings

### Files found

- `crates/agentdash-application-vfs/src/tools/common.rs` — Agent tool URI resolution helper and shared runtime VFS handle.
- `crates/agentdash-application-vfs/src/tools/mounts.rs` — `mounts_list` implementation.
- `crates/agentdash-application-vfs/src/tools/fs/read.rs` — `fs_read` argument parsing and read dispatch.
- `crates/agentdash-application-vfs/src/tools/fs/glob.rs` — `fs_glob` argument parsing and list dispatch.
- `crates/agentdash-application-vfs/src/tools/fs/grep.rs` — `fs_grep` argument parsing and search dispatch.
- `crates/agentdash-application-vfs/src/tools/fs/apply_patch.rs` — `fs_apply_patch` facade, patch path lock-key parsing.
- `crates/agentdash-application-vfs/src/tools/fs/shell.rs` — `shell_exec` OS shell path, platform shell path, and materialization rewrite entry.
- `crates/agentdash-application-vfs/src/tools/fs/platform_shell.rs` — VFS-backed platform shell parser and read/write/list command mapping.
- `crates/agentdash-application-vfs/src/path.rs` — canonical VFS URI, mount id, path normalization, mount capability resolver.
- `crates/agentdash-application-vfs/src/service.rs` — smallest current provider dispatch choke point for most read/list/search/write/exec operations.
- `crates/agentdash-application-vfs/src/materialization.rs` — shell command VFS URI rewrite/materialization path.
- `crates/agentdash-application-vfs/src/mutation_dispatcher.rs` — non-agent surface mutation write/apply_patch path.
- `crates/agentdash-application-vfs/src/mount_project.rs` — current Project VFS grant pruning mutates mount capabilities.
- `crates/agentdash-domain/src/common/agent_config.rs` — `AgentVfsAccessGrant` shape.
- `crates/agentdash-application-agentrun/src/agent_run/effective_capability.rs` — PermissionGrant active facts currently project tool-level admission only.
- `crates/agentdash-application-agentrun/src/agent_run/permission_runtime_surface_update.rs` — PermissionGrant surface update currently writes tool capability state.
- `crates/agentdash-local/src/workspace_root_guard.rs` and `crates/agentdash-local/src/process_executor.rs` — local shell workspace-root guard.

### Parsing and normalization map

- `mounts_list`: has no mount/path input. It snapshots `SharedRuntimeVfs`, calls `VfsService::list_mounts`, and prints raw mount capabilities. There is no effective access filtering or operation-level policy projection in this path (`tools/mounts.rs:51-64`).
- `fs_read`: JSON args deserialize to `FsReadParams`, then `resolve_uri_path(&vfs, &params.path)` returns a normalized `ResourceRef` before service dispatch (`tools/fs/read.rs:130-135`). Binary detection first calls `service.stat` (`tools/fs/read.rs:136-149`), then text read calls `service.read_text_range` (`tools/fs/read.rs:151-163`), and binary image read calls `service.read_binary` (`tools/fs/read.rs:328-337`). The service re-checks provider capability and normalizes path via `resolve_provider_dispatch` (`service.rs:77-107`, `service.rs:125-132`, `service.rs:246-253`).
- `fs_glob`: optional `path` defaults to `"."`, `resolve_uri_path` parses/normalizes it, then `service.list` dispatches with `MountCapability::List` (`tools/fs/glob.rs:84-110`; `service.rs:751-766`).
- `fs_grep`: optional `path` defaults to `"."`, `resolve_uri_path` parses/normalizes it, then `grep_text_extended` dispatches with `MountCapability::Search` (`tools/fs/grep.rs:151-164`, `tools/fs/grep.rs:180-203`; `service.rs:968-980`).
- write/create/delete/rename outside the agent tool facade: `VfsMutationDispatcher` normalizes after `resolve_mount(..., MountCapability::Write)` for create/write/delete/rename (`mutation_dispatcher.rs:98-109`, `mutation_dispatcher.rs:149-159`, `mutation_dispatcher.rs:184-194`, `mutation_dispatcher.rs:224-237`). This is route/surface mutation infrastructure, not the current Agent-facing `fs_apply_patch` tool.
- `fs_apply_patch`: the tool parses patch args, computes mutation lock keys by `parse_patch_text` plus `normalize_patch_entry_targets`, and rejects bare/cross-mount paths before queueing (`tools/fs/apply_patch.rs:122-139`, `tools/fs/apply_patch.rs:172-189`). `VfsService::apply_patch_multi` parses the patch again, normalizes each entry, groups by mount, and calls `apply_entry_group` (`service.rs:639-663`, `service.rs:667-700`). Per-mount execution currently checks only `resolve_mount(..., MountCapability::Write)` before inline/provider dispatch (`service.rs:704-748`). Single-mount `apply_patch` also checks only write capability before dispatch (`service.rs:565-612`).
- `shell_exec` OS shell: `cwd` must be explicit `mount_id://relative/path`; `resolve_uri_path` normalizes it, then `resolve_mount(..., Exec)` gates the cwd mount (`tools/fs/shell.rs:156-181`). `service.exec` repeats provider dispatch with `MountCapability::Exec` and normalized cwd (`service.rs:849-878`).
- `shell_exec` command materialization: `rewrite_shell_command` scans command text for current mount URIs, `parse_mount_uri` normalizes each candidate, then checks source `Read` and exec mount `Exec` before direct root-ref join or materialization transport dispatch (`materialization.rs:40-75`). This path can bypass provider read/list dispatch for same-backend relay mounts via direct local path construction (`materialization.rs:64-67`).
- `shell_exec` platform shell: empty cwd / `platform://` uses `PlatformShellCwd`; `platform://mount/path` normalizes path at parse time (`tools/fs/platform_shell.rs:18-41`). Command args with VFS URI go through `parse_mount_uri`; relative args are joined against platform cwd and normalized (`tools/fs/platform_shell.rs:369-386`). Operations then call `VfsService` read/list/write/delete/rename (`tools/fs/platform_shell.rs:191-210`, `tools/fs/platform_shell.rs:389-408`).
- Canonical normalization rejects absolute and parent escape in `normalize_mount_relative_path` (`path.rs:298-335`). `parse_mount_uri` also resolves VFS links after URI parse (`path.rs:193-229`).

### Current checks that are not sufficient runtime VFS authorization

- Tool visibility is checked only at tool assembly with `CapabilityState::is_capability_tool_enabled`: read tools and `mounts_list` under `file_read`, `fs_apply_patch` under `file_write`, `shell_exec` under `shell_execute` (`tools/factory.rs:46-139`). This decides whether the tool exists, not which mount/path/op it may touch.
- Provider operation support is mostly represented by `MountCapability` in `resolve_mount` (`path.rs:246-260`) and `VfsService::resolve_provider_dispatch` (`service.rs:77-107`). All service operations rely on that helper or direct `resolve_mount` calls.
- `mounts_list` exposes provider mount capabilities directly (`tools/mounts.rs:51-64`); it does not distinguish provider support from effective session authorization.
- `apply_patch`, `apply_patch_multi`, and `VfsMutationDispatcher` direct write paths rely on `MountCapability::Write` plus optional edit capabilities, not runtime path policy (`service.rs:565-612`, `service.rs:704-748`, `mutation_dispatcher.rs:98-159`, `mutation_dispatcher.rs:283-303`).
- `shell_exec` relies on `MountCapability::Exec` for OS shell cwd and `MountCapability::Read` for command URI materialization sources (`tools/fs/shell.rs:172-181`, `materialization.rs:58-75`). Same-backend relay materialization directly joins `root_ref` with normalized path (`materialization.rs:64-67`), so policy must run before that direct local-path branch.
- Platform shell checks tool-level read/write capability before commands (`tools/fs/platform_shell.rs:411-433`) and then relies on service-level mount capability. Tool capability grants here can unlock command verbs, but they do not answer mount/path admission.
- Project VFS grants currently mutate mount capabilities: absent grant clears project mount capabilities; present grant intersects capabilities; empty project mounts are pruned (`mount_project.rs:136-162`). The grant shape is only `{ mount_id, capabilities }` (`agent_config.rs:262-267`), so it cannot express path scopes.
- PermissionGrant facts currently use `ToolCapabilityPath` (`permission_runtime_surface_update.rs:216-240`) and classify tool-level paths separately from capability surface paths (`effective_capability.rs:59-87`). They do not carry typed VFS mount/path rules today.
- Local process execution has a separate workspace-root guard: `WorkspaceRootGuard::validate_workspace_root` permits only registered canonical roots when configured (`workspace_root_guard.rs:19-52`), and `ProcessExecutor::resolve_cwd` rejects absolute cwd and resolves relative cwd under the validated workspace root (`process_executor.rs:35-53`). This is a local filesystem guard, not runtime VFS authorization.

### Smallest correct enforcement point

The smallest productive cleanup point is a shared normalized-target admission helper inside `agentdash-application-vfs`, used by existing dispatch paths instead of adding a parallel authorization service.

Recommended shape:

```text
parse/resolve URI or patch target
-> normalize mount-relative path / resolve links
-> infer RuntimeVfsOperation
-> check mount provider capability
-> check RuntimeVfsAccessPolicy against normalized mount/path/op
-> provider dispatch or materialization dispatch
```

Implementation-wise, `VfsService::resolve_provider_dispatch` is the current choke point for `read_text(_range)`, `read_binary`, `list`, `search`, `grep`, `write_text`, `delete_text`, `rename_text`, `stat`, `suggest_paths`, and `exec` (`service.rs:77-107`). It should call a smaller helper after line `89` path normalization and before provider lookup/dispatch. That helper should take the normalized `mount_id`, normalized path, and `RuntimeVfsOperation`, and return a denied error before any provider or overlay path runs.

However, `resolve_provider_dispatch` alone is not sufficient:

- `apply_patch` and `apply_entry_group` bypass it and call `resolve_mount(..., Write)` directly (`service.rs:565-612`, `service.rs:704-748`). They need to normalize each patch target, then admit every affected primary and move target before constructing inline/provider patch targets.
- `VfsMaterializationService::rewrite_shell_command` and `local_path_for_uri` bypass provider dispatch for direct relay mounts (`materialization.rs:58-75`, `materialization.rs:206-231`). They need explicit `Read` admission for each source URI after `parse_mount_uri`, plus `Exec` admission for the cwd/exec mount before rewrite/materialize dispatch.
- `mounts_list` needs either policy-filtered effective operation display or clear split display of provider capabilities versus effective operations; otherwise it will continue advertising mount support as authorization.

Avoid a new long-lived `VfsAccessService` unless old paths are removed or routed through it. A small policy model plus pure matcher/compiler is fine; enforcement should be wired into the existing VFS resolution/dispatch helpers so mount-capability/grant misuse is deleted, not shadowed.

### Existing tests and gaps

Existing coverage:

- URI/default-mount normalization and absolute/escape rejection: `tools/common.rs:91-166`; `path.rs:664-681`.
- VFS link and provider capability validation: `path.rs:566-739`.
- `fs_read` behavior but not policy allow/deny: schema, range, binary/image, size guard, dedup, suggestions (`tools/fs/read.rs:613-856`).
- `fs_glob` behavior but not policy allow/deny: schema, pattern recursion, result cap, VCS filtering (`tools/fs/glob.rs:322-486`).
- `fs_grep` behavior but not policy allow/deny: regex, output modes, type shortcut, pagination, context, VCS filtering (`tools/fs/grep.rs:510-763`).
- `fs_apply_patch` path lock parsing: rejects bare paths/bare move target/cross-mount move, normalizes explicit paths (`tools/fs/apply_patch.rs:196-290`). Service-level patch target tests also cover normalized move target and escaping move target (`service.rs:1440-1491`).
- Low-level filesystem patch target rejects path escape and missing edit capabilities (`apply_patch.rs:944-1054`).
- `shell_exec` cwd rejects local relative/pseudo mount, unresolved VFS URI rejection, rewrite notice formatting (`tools/fs/shell.rs:640-756`).
- Platform shell checks tool-level read/write capability; missing write rejects `cp` (`tools/fs/platform_shell.rs:708-752`).
- Local workspace guard tests: unavailable configured roots fail closed (`workspace_root_guard.rs:80-97`) and local executor tests cover workspace root/cwd guards (`tool_executor.rs`, discovered tests around `validate_workspace_root_*` and `resolve_shell_cwd_*`).
- Project VFS grant test proves project grants do not constrain Agent memory mount, but still by mutating project mount capabilities (`mount_project.rs:480-520`).
- AgentRun PermissionGrant tests prove tool-level grants do not alter schema-facing `CapabilityState` and classify tool-level paths separately from surface paths (`effective_capability.rs:713-755`).

Gaps:

- No runtime VFS access policy model/matcher tests exist.
- No tests prove `MountCapability` support plus policy deny blocks read/list/search/write/apply_patch/exec.
- No tests prove tool-level PermissionGrant for `file_read::fs_read`, `file_write::fs_apply_patch`, or `shell_execute::shell_exec` does not expand mount/path access.
- No tests cover shell materialization deny before direct same-backend root-ref rewrite.
- No tests cover `mounts_list` effective display/filtering under policy.

Recommended test locations:

- Policy model/compiler/matcher: new focused unit module in `crates/agentdash-application-vfs/src/` next to VFS runtime policy code, e.g. `runtime_access_policy.rs` or `tools/access_policy.rs`.
- Shared enforcement helper: tests near `VfsService::resolve_provider_dispatch` if the helper is service-owned; include normalized allow/deny for read/list/search/write/exec.
- Tool behavior tests: colocate operation-specific regression tests in `tools/fs/read.rs`, `glob.rs`, `grep.rs`, `apply_patch.rs`, and `shell.rs` only for tool argument/dispatch edges that are not covered by the shared helper.
- Materialization policy tests: `materialization.rs`, specifically source URI policy deny before `can_directly_reference_local_path` and before transport materialization.
- `mounts_list` effective exposure tests: `tools/mounts.rs`.
- PermissionGrant/tool-level non-expansion tests: `crates/agentdash-application-agentrun/src/agent_run/effective_capability.rs` or the runtime policy compiler tests, depending on whether VFS path facts become typed PermissionGrant paths in this slice.

### Related specs

- `.trellis/spec/backend/vfs/architecture.md` — states agent-facing VFS tool boundaries and runtime tool composition.
- `.trellis/spec/backend/vfs/vfs-access.md` — requires mount-relative normalization before application layer and defines runtime tool semantics.
- `.trellis/spec/backend/vfs/vfs-materialization.md` — defines shell VFS URI rewrite/materialization constraints.
- `.trellis/spec/backend/permission/grant-lifecycle.md` — PermissionGrant requested paths are `ToolCapabilityPath` facts today.
- `.trellis/spec/backend/session/architecture.md` — RuntimeSession exposes tool surface but does not own Grant authorization.
- `.trellis/tasks/06-30-design-backlog-review/design-review.md` D9 — says Project VFS preset grants are not generic authorization and runtime policy is the owner.
- `.trellis/tasks/06-30-design-backlog-review/implementation-slices.md` Slice 7 — orders Runtime VFS Access Policy work.

### External references

- None. This research is internal code/spec mapping only.

## Caveats / Not Found

- I did not run Rust builds or test suites per task constraint.
- No typed VFS path grant shape was found in `PermissionGrant`; current durable path model is `ToolCapabilityPath`, which only addresses tool capabilities and optional tool names.
- No existing `RuntimeVfsAccessPolicy` type or matcher was found.
- `VfsService::resolve_provider_dispatch` is the best existing choke point for provider dispatch, but it cannot be the only enforcement site because `apply_patch` and materialization currently bypass it in important branches.
- If implementation only adds policy checks at tool facades, platform shell sub-operations, materialization rewrite, and service/internal mutation paths can drift. Cleanup should route these through one helper and remove the old interpretation of Project VFS grants as generic mount capability authorization.
