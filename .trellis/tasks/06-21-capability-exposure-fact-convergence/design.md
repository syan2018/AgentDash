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

## Recovery Rule

On recovery, live VFS / WorkspaceModule visibility / hook runtime state must be reconstructed from AgentFrame exposure fact. Runtime caches may fail independently but cannot become the source of truth.
