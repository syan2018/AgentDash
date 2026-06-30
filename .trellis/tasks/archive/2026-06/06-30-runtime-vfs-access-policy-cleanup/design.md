# Design

## Boundary

D9 separates three concerns that are currently easy to conflate:

- Tool visibility: whether a runtime tool is present for the Agent. Owner: capability/tool policy.
- Provider capability: whether a mount/provider supports an operation. Owner: VFS mount/provider.
- Runtime VFS admission: whether this AgentRun/session may use an operation on a normalized mount/path. Owner: runtime VFS access policy.

The runtime policy must sit between tool input normalization and provider dispatch/materialization.

## Proposed Model

```rust
pub enum RuntimeVfsOperation {
    Read,
    List,
    Search,
    Write,
    ApplyPatch,
    Exec,
}

pub struct RuntimeVfsAccessRule {
    pub mount_id: String,
    pub path_pattern: RuntimeVfsPathPattern,
    pub operations: BTreeSet<RuntimeVfsOperation>,
    pub source: RuntimeVfsAccessSource,
}

pub struct RuntimeVfsAccessPolicy {
    pub rules: Vec<RuntimeVfsAccessRule>,
}
```

Path pattern should start conservative:

- `All` for existing whole-mount project exposure;
- `Prefix(String)` / exact normalized prefix for path-level PermissionGrant;
- no glob semantics unless current PermissionGrant facts already require it.

## Enforcement Point

VFS tool resolution already centralizes address handling under `vfs/tools/common.rs` and concrete handlers under `vfs/tools/fs/`. D9 should add a shared admission check after:

1. tool input parsed;
2. mount resolved;
3. mount-relative path normalized;
4. operation inferred.

Only then dispatch to provider/materialization/mutation.

## Compiler Inputs

- Project VFS mount exposure/preset grants compile to whole-mount rules that preserve current behavior.
- PermissionGrant active facts compile to path-level rules when requested paths target VFS scopes.
- System/runtime mounts such as lifecycle evidence may compile read-only or operation-specific rules from their runtime projection facts.

## Staging

This is a bigger slice than D5/D6. Use research before code:

1. Map current VFS tool normalization and provider dispatch paths.
2. Map `AgentVfsAccessGrant` / Project VFS grant naming and compiler path.
3. Map PermissionGrant requested path shape and whether VFS path grants already exist or need a typed extension.
4. Decide an MVP implementation that preserves existing behavior while making the policy boundary real.

If the current PermissionGrant domain does not yet encode path-level VFS grants, document the missing contract and implement the model/compiler for Project exposure first. Do not fake path-level grants with string parsing that has no durable contract.

## Research Decision

Research confirmed three implementation constraints:

- `PermissionGrant.requested_paths` is currently `ToolCapabilityPath` only. It has no durable mount/path/operation contract, so this slice must not parse VFS path policy from those strings.
- The honest MVP compiler input is the already assembled runtime `Vfs` plus the existing ProjectAgent Project VFS mount exposure facts. This preserves current behavior by compiling whole-mount rules for currently exposed mounts.
- `RuntimeVfsAccessPolicy` should travel with the typed runtime VFS surface: AgentRun/FrameLaunch surface -> RuntimeSession launch envelope -> `ExecutionContext.session` -> `SharedRuntimeVfs`. It should not live inside `CapabilityState.vfs` as the primary owner.

MVP scope:

1. Add policy model and whole-mount compiler from runtime `Vfs`.
2. Thread policy beside `Vfs` through connector/runtime tool state.
3. Enforce policy in shared normalized VFS dispatch and the known bypasses: apply_patch and shell materialization.
4. Rename Project VFS preset grant terms so old names no longer imply generic VFS authorization.
5. Document the missing typed PermissionGrant VFS path contract as a future design point rather than faking it.

## Non-Goals

- Do not redesign the entire permission state machine.
- Do not change provider storage or inline storage owner semantics.
- Do not add frontend UI for new path grants in this slice.
- Do not preserve old Project VFS grant names if they are misleading and local rename is feasible.

## Validation

- Unit tests for policy compiler and matcher.
- VFS tool tests for read/write/search/exec allow/deny after path normalization.
- Static search proving mount capability alone is not the only gate for tool execution.
- Permission tests proving tool-level grants do not expand mount/path access.

## Implementation Summary

Implemented D9 as an explicit runtime admission carrier:

- `RuntimeVfsAccessPolicy` and operation/path/source rule types live in SPI and travel beside `Vfs`
  through launch/session/runtime tool contexts.
- `SharedRuntimeVfs` carries `Vfs + RuntimeVfsAccessPolicy`, so runtime VFS tools consume the same
  address space and admission facts.
- VFS provider dispatch checks policy after mount-relative path normalization. `apply_patch`,
  shell exec, platform shell commands, shell URI materialization, and relay MCP JSON argument
  materialization pass the runtime policy explicitly so they do not fall back to mount capability
  as authorization.
- ProjectAgent preset VFS configuration is now named Project VFS mount exposure across backend and
  frontend. The old `vfs_access_grants` field is rejected rather than accepted as a compatibility
  alias.

Remaining design gap: typed PermissionGrant VFS path rules. Current grants only carry
`ToolCapabilityPath`, so there is no durable source for mount/path/operation policy rules yet.
That contract should be designed before PermissionGrant contributes to `RuntimeVfsAccessPolicy`.
