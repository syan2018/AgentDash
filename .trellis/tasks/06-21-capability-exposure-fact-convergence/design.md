# Capability Exposure Design

## Design Decisions To Preserve

- AgentFrame 是 runtime capability / exposure 的唯一锚定事实源。
- PermissionGrant status 负责审批、审计和生命周期，不直接等于 runtime tool surface。
- Canvas / WorkspaceModule / hook runtime / VFS surface 都从 AgentFrame exposure 派生或刷新。

## Target Flow

```text
Command intent
  -> update / replace AgentFrame capability-exposure fact
  -> derive live VFS, WorkspaceModule visibility, hook runtime surface
  -> diagnostics records derivation failures
```

## Fact Model Decision

- runtime exposure / capability 变更通过新的 `AgentFrame` revision 表达。
- `AgentFrame` revision 是可更新可替换 runtime ability state 的唯一事实锚点；每个 revision 必须能恢复 effective capability、VFS、MCP 与 workspace module visibility。
- 不使用独立 exposure table 作为第一版事实源，原因是会引入第二套 owner/repository。
- 不原地更新当前 frame exposure 字段，原因是 revision history 将无法解释能力状态变化。

## Exposure Fact Shape

First implementation reuses existing `AgentFrame` columns rather than adding an exposure table:

- `effective_capability_json`: effective runtime `CapabilityState`.
- `vfs_surface_json`: VFS surface derived from that capability state.
- `mcp_surface_json`: MCP surface derived from that capability state.
- `visible_canvas_mount_ids_json`: Canvas mount exposure refs.
- `visible_workspace_module_refs_json`: runtime-visible workspace module refs such as `canvas:{mount_id}`.
- `created_by_kind` / `created_by_id`: command source such as `permission_grant_approve`, `permission_grant_revoke`, `permission_grant_expire`, `canvas_expose`, or `workspace_module_present`.

PermissionGrant effects, Canvas exposure and WorkspaceModule runtime visibility all write a new AgentFrame revision first. Any live runtime cache adoption happens after the persisted revision exists.

## Recovery Rule

On recovery, live VFS / WorkspaceModule visibility / hook runtime state must be reconstructed from AgentFrame exposure fact. Runtime caches may fail independently but cannot become the source of truth.

Recovery order:

1. Resolve the delivery runtime to the current AgentFrame.
2. Read effective capability/VFS/MCP and visible runtime refs from that frame revision.
3. Reconstruct live VFS and skill baseline from the frame-derived capability state.
4. Resolve WorkspaceModule visibility from base capability dimension plus frame runtime refs.
5. Rebuild hook runtime for the frame target and enqueue context/tool deltas.
6. Emit presentation/runtime events only after frame fact write and runtime adoption succeed.

## Implementation Boundary

CE05 is a prerequisite boundary check: active grants can compile runtime effects, but they must not become a direct runtime surface fact. CE02/CE03 share a helper that persists a new AgentFrame revision and then adopts it into active runtime when one exists.
