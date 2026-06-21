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

## Fact Model Options

| Option | Description | Notes |
| --- | --- | --- |
| Frame revision carries exposure | exposure changes create new frame revision | strongest audit, potentially more frame churn |
| AgentFrame exposure table | separate exposure rows keyed by frame | keeps frame stable, adds table/repository |
| Capability dimension transition | exposure enters existing capability transition model | aligns with capability state, requires clear projection owner |

Decision must pick one primary model before broad implementation.

## Recovery Rule

On recovery, live VFS / WorkspaceModule visibility / hook runtime state must be reconstructed from AgentFrame exposure fact. Runtime caches may fail independently but cannot become the source of truth.

