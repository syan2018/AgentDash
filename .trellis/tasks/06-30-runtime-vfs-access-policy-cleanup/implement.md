# Implementation Plan

## Operating Rules

- Follow Trellis workflow and start this task before code edits.
- Every subagent prompt must start with `Active task: .trellis/tasks/06-30-runtime-vfs-access-policy-cleanup`.
- Cleanup-first constraint: this review exists to converge architecture from first principles. Removing old grant/mount capability misuse is more important than adding feature surface.
- Do not add compatibility paths or another authorization service that leaves old Project VFS grant semantics as generic authorization.
- Research workers are read-only. Implementation workers must not run large Rust builds or broad suites.
- While subagents run, the main session should not interrupt them repeatedly; wait or work on non-overlapping synthesis/docs.

## Research Split

1. VFS tool enforcement map
   - Read `crates/agentdash-application-vfs/src/tools/common.rs` and `tools/fs/*`.
   - Identify where mount/path normalization happens for read/list/search/write/apply_patch/shell.
   - Identify the smallest shared enforcement point.

2. Grant and policy input map
   - Read AgentRun effective capability and PermissionGrant projection code.
   - Identify `AgentVfsAccessGrant` or equivalent current grant shape.
   - Determine whether PermissionGrant has typed VFS path rule facts or only tool capability paths.

3. Runtime surface / session assembly map
   - Read runtime-session launch/frame VFS assembly and AgentRun frame surface builder.
   - Identify where a `RuntimeVfsAccessPolicy` should be carried in `ExecutionContext` or adjacent runtime surface.

## Implementation Candidate

After research confirms scope:

1. Add `RuntimeVfsAccessPolicy` model near VFS runtime tool/common code or shared SPI if connector-facing.
2. Compile current Project VFS exposure to whole-mount rules preserving behavior.
3. Add matcher tests for normalized path and operation sets.
4. Thread policy into `SharedRuntimeVfs` / VFS tool resolver.
5. Enforce policy in read/list/search/write/apply_patch/shell handlers.
6. Rename misleading Project VFS grant terms where local and safe.
7. Document any PermissionGrant path-rule contract that needs separate schema/domain work.

## Research Result And Ordered Implementation

The three research reports under `research/` agree on an MVP:

1. Policy/carrier first
   - Add `RuntimeVfsAccessPolicy` and matcher/compiler.
   - Compile whole-mount rules from the current runtime `Vfs`.
   - Thread policy with `Vfs` through `ExecutionContext.session` and `SharedRuntimeVfs`.

2. Project VFS preset naming
   - Rename `AgentVfsAccessGrant` and related fields/functions to Project VFS mount exposure/preset terminology.
   - Preserve current behavior while making it clear these grants do not authorize arbitrary VFS mounts/paths.

3. Enforcement
   - Use the policy in the shared VFS service dispatch path after normalization.
   - Add explicit policy checks for `apply_patch` and shell/materialization bypasses.
   - Keep PermissionGrant path-level policy as documented future contract because no typed domain fact exists yet.

Subagent split:

- Implement A: policy model/carrier and whole-mount compiler.
- Implement B: Project VFS preset grant rename/narrowing.
- Implement C: VFS operation enforcement after A lands.
- Check: targeted review that old mount-capability-only paths are not still the only gate.

## Validation Commands

```powershell
python ./.trellis/scripts/task.py validate .trellis/tasks/06-30-runtime-vfs-access-policy-cleanup
git diff --check
cargo fmt --check --package agentdash-application-vfs --package agentdash-application-runtime-session --package agentdash-application-agentrun --package agentdash-spi
cargo test -p agentdash-application-vfs vfs_access --lib
cargo test -p agentdash-application-vfs tools --lib
cargo test -p agentdash-application-agentrun effective_capability --lib
rg -n "AgentVfsAccessGrant|MountCapability::Exec|MountCapability::Write|resolve_mount\\(" crates/agentdash-application-vfs crates/agentdash-application-runtime-session crates/agentdash-application-agentrun
```

Adjust test filters after research identifies actual module names.

## Validation Result

Executed targeted validation after implementation:

```powershell
python ./.trellis/scripts/task.py validate .trellis/tasks/06-30-runtime-vfs-access-policy-cleanup
git diff --check
cargo fmt --check --package agentdash-domain --package agentdash-application-vfs --package agentdash-application-runtime-session --package agentdash-application-agentrun --package agentdash-spi --package agentdash-api --package agentdash-application-runtime-gateway --package agentdash-application-ports --package agentdash-workspace-module --package agentdash-application --package agentdash-executor
cargo test -p agentdash-domain preset_config --lib
cargo test -p agentdash-application-vfs access_policy --lib
cargo test -p agentdash-application-vfs runtime_policy_deny --lib
cargo test -p agentdash-application-vfs apply_patch_policy_deny --lib
cargo test -p agentdash-application-vfs json_argument_rewrite_uses_runtime_vfs_access_policy --lib
cargo test -p agentdash-application-runtime-session launch_plan_projects_connector_context_and_summary --lib
cargo test -p agentdash-application-runtime-gateway idle_mcp_list_tools_uses_runtime_surface_backend_anchor --lib
cargo test -p agentdash-application-agentrun default_mount_backend_id_generates_anchor --lib
pnpm run frontend:check
rg -n "AgentVfsAccessGrant|agent_vfs_access_grants|apply_agent_vfs_access_grants|vfs_access_grants" crates packages --glob '!target' --glob '!node_modules'
rg -n "rewrite_json_arguments\\(|RewriteJsonArgumentsInput \\{|rewrite_shell_command_with_policy|rewrite_shell_command\\(" crates --glob '!target'
```

Result: all checks passed. The old `vfs_access_grants` string remains only in the domain rejection
path and tests. PermissionGrant VFS path-level rules remain a documented design gap because the
current domain fact is only `ToolCapabilityPath`.
