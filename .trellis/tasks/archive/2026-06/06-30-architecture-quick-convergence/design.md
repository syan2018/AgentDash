# 架构快速收束设计

## Batch Strategy

本父任务按“可独立验证的收束线”组织，而不是按原始 review issue 编号逐条开任务。

拆分原则：

- 同一事实源或同一执行边界内的问题合并。
- 需要 migration 的项独立成工作项。
- 涉及状态机/launch/gate/delegate 大设计的项排除。
- 工作项之间尽量保持文件集合低重叠。

## Work Lines

### Authority quick fix

优先级最高。它处理两个 P0：

- tool-level PermissionGrant 不再扩大模型可见 `CapabilityState`。
- runtime admission projection 不再按 run 查询 active grant，而是按 current/effect frame 查询。

该任务允许做最小 production boundary 修正，但不要求完成完整 `AgentRunEffectiveCapabilityPort` 设计。

### Extension / WorkspaceModule consistency

处理同一产品 surface 的明显分叉：

- workspace module schema validator 复用 extension runtime validator。
- extension invocation workspace resolver 统一。
- Canvas promoted extension loadability 变成 renderer-aware projection。

### VFS / Local guard rails

处理低耦合 guard/invariant：

- callable runtime tool name 唯一性。
- workspace root validation 共用 guard。
- local relay scheduling 由 handler 声明。
- builtin VFS skill discovery 传 identity。

### Mailbox steering consistency

局部收束 mailbox delivery 语义：

- delegate steering 与 scheduler steering 共享执行器。
- receipt/status/error semantics 统一。

### Settings preference convergence

独立处理 migration：

- 旧 `user_preferences` 迁入 scoped settings。
- AgentRun workspace 消费 scoped setting。
- BackendRepository 移除 user preference 职责。

## Integration Risks

- Authority quick fix 会影响 runtime tool exposure/admission，必须 targeted 覆盖 VFS/MCP/workspace module tool assembly。
- Settings migration 会触碰 database migration，必须跑 migration guard。
- VFS/local guard rails 中 scheduling 与 root guard 可能影响 local relay behavior，必须跑 `agentdash-local` targeted tests/check。
