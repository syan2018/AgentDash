# Design

## Boundary

本任务是 PR78 的快速收口修复，不重新设计完整权限、VFS 或 RuntimeGateway 架构。目标是关闭 post-review 中已证实的旧路径残留：

- runtime VFS policy 必须随 `ExecutionContext.session.vfs` 一起进入所有 runtime-facing tool/provider bridge。
- AgentRun effective capability 与 grant projection 必须以 current/effect frame 为事实源，不再把 launch frame 当作 current surface。
- RuntimeGateway dynamic action catalog 的 resolved owner 必须成为 WorkspaceModule runtime action ownership 的输入，或 duplicate `action_key` 在 catalog owner 处被拒绝。

## Data Flow

### VFS policy

Canonical runtime path:

```text
AgentFrame / runtime surface
  -> FrameLaunchEnvelope / LaunchPlan
  -> ExecutionSessionFrame { vfs, vfs_access_policy }
  -> Runtime tool providers / workspace-module runtime bridge
  -> SharedRuntimeVfs::new_with_policy(vfs, policy)
  -> VfsService normalized policy enforcement
```

`SharedRuntimeVfs::new(vfs)` 和 `replace(vfs)` 可以继续服务非 runtime / test convenience，但 runtime bridge 必须使用 policy-aware API。

### AgentRun effective capability

Canonical projection:

```text
runtime_session_id
  -> RuntimeSessionExecutionAnchor
  -> run / agent identity
  -> current AgentFrame for agent
  -> active grants for effective/current frame
  -> visible capability view + tool admission decision
```

`launch_frame_id` 保留为 launch evidence / provenance，不再作为 current capability surface 的默认读取坐标。

### Runtime action catalog owner

Canonical ownership:

```text
Project enabled extension installations
  -> RuntimeGateway dynamic provider resolved descriptor
  -> descriptor carries or implies the same extension owner used by invocation
  -> WorkspaceModule projection groups runtime actions by that resolved owner
```

如果当前 descriptor 无法低风险携带 owner，则 duplicate `action_key` 应在 Gateway dynamic provider 处 fail closed，避免展示与执行分叉。

## Tradeoffs

- 快速修复优先保证 runtime-facing call path 正确，不在本任务中清理所有 non-runtime convenience API。
- PermissionGrant VFS policy 的 exact union/intersection 语义若需要更大设计，本任务至少补齐负例测试和文档边界；能局部修正则直接修正。
- WorkspacePlacement transaction 边界属于更大 unit-of-work 设计，本任务不强行补一个半成品事务抽象。

## Validation

- Focused Rust tests for AgentRun effective capability / runtime surface.
- Focused Rust tests for workspace-module runtime VFS policy preservation.
- Focused Rust tests for runtime gateway duplicate action key or owner projection.
- `git diff --check origin/main..HEAD`.
