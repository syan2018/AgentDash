# AgentRun 与 RuntimeSession 层级关系收束执行计划

## Current Status

Phase 0 evidence refresh 已完成，Phase 1 MCP declaration naming convergence 已实现并进入检查收口。Phase 2+ 尚未开始；本任务后续仍需按阶段推进 FrameSurfaceDraft、runtime launch surface 读取、AgentRun workspace API/UI 与持久化清理。

## Phase 0: Evidence Refresh

Status: completed. Evidence captured in `research/phase-0-surface-fact-source-audit.md`.

目标：在实现前刷新当前代码事实，确保设计与最新 main 对齐。

- [x] 复查 `RuntimeMcpServerDeclaration`、`RuntimeMcpServer`、`McpRuntimeBindingContext`、`RuntimeSessionMcpAccess` 的当前引用分布。
- [x] 复查 AgentRun workspace API、`AgentFrame.mcp_surface_json`、`RuntimeSessionExecutionAnchor` 的读写路径。
- [x] 复查 session startup、capability resolver、runtime gateway、frame construction 的 Trellis spec。
- [x] 补充一份 research note，记录当前事实源写入和读取路径。

验证重点：

- 引用扫描覆盖 `crates/`、`packages/app-web/src/`、`.trellis/spec/`。
- research note 能回答 “当前谁写入 surface，谁读取 surface，谁负责同步”。

## Phase 1: MCP Declaration Naming Convergence

目标：先移除最误导的 Session 命名，为后续事实源迁移建立正确词汇。

- [x] 以 `RuntimeMcpServerDeclaration` 作为 canonical runtime-resolved declaration。
- [x] 以 `McpRuntimeBindingContext` 作为 MCP runtime binding resolver context。
- [x] 将相关 helper 命名改为 declaration / binding context 语义。
- [x] 更新 MCP runtime binding、session startup、capability pipeline 相关 spec。
- [x] 保持 wire DTO `McpServerDeclarationRelay` 的 relay 边界语义。

验证重点：

- Rust compile / clippy 覆盖重命名后的跨 crate 引用。
- MCP preset runtime binding tests 继续证明 preset key 与 `mcp:<preset>` 产出一致 declaration。
- relay/direct MCP 测试证明 resolved declaration 仍正确投影到执行层。

## Phase 2: FrameSurfaceDraft Introduction

目标：把 construction pipeline 的输出从 session projections 逐步收束为 AgentFrame surface draft。

- [ ] 设计 `FrameSurfaceDraft` 或等价结构，承载 capability、VFS、MCP、context、execution profile surface。
- [ ] 让 capability resolver、workspace facts、context builder、execution profile 输出汇入 draft。
- [ ] 让 `AgentFrameBuilder` 从 draft 写入 AgentFrame revision。
- [ ] 将 `SessionConstructionPlan.projections` 的职责改为持有或转交 draft。

验证重点：

- AgentFrame revision 中 surface 字段完整且可反序列化。
- construction validation 能证明 draft 与最终 AgentFrame surface 一致。
- 现有 launch path 在过渡期仍能取得执行所需 surface。

## Phase 3: Runtime Launch Reads AgentFrame Surface

目标：让 RuntimeSession 从 AgentFrame surface 启动和发现能力。

- [ ] 梳理 runtime launch 对 `CapabilityState`、`SessionConstructionPlan.projections`、session runtime state 的读取。
- [ ] 将 launch-time MCP/VFS/capability/context 读取切到 AgentFrame surface。
- [ ] 保留 `RuntimeSessionExecutionAnchor` 作为 trace backlink。
- [ ] 调整 `SessionRuntimeInner`，使其更像执行适配器和 trace coordinator。

验证重点：

- AgentRun send/steer/enqueue/cancel 流程仍能正确投递。
- MCP discovery 与 tool call 使用 AgentFrame surface 中的 declaration。
- runtime session trace 能反查 AgentRun、AgentFrame 和 LifecycleRun。

## Phase 4: AgentRun Workspace API And UI Model

目标：用户侧工作台表达 AgentRun command/control，而 runtime session 作为详情和 trace 信息呈现。

- [ ] 引入或调整 `AgentRunWorkspaceControlPlaneView` / `AgentRunWorkspaceActionSetView`。
- [ ] 让 AgentRun workspace 页面以 AgentRun command model 为主。
- [ ] 将 RuntimeSession ID、trace meta、delivery status 放在详情或运行证据区域。
- [ ] 评估 `SessionChatView` 复用边界，必要时抽出 AgentRun chat/control facade。

验证重点：

- AgentRun workspace 首屏不要求用户理解 SessionRuntime 控制面。
- 继续支持查看 runtime trace 和复制 RuntimeSession ID。
- 前端 typecheck、相关 vitest 通过。

## Phase 5: Persistence And Long-Term State Cleanup

目标：完成事实源迁移后的长期结构整理。

- [ ] 评估 `CapabilityState.tool.mcp_servers` 是否继续作为 draft 中间结构，或从长期 state 中移除。
- [ ] 评估 `SessionConstructionPlan.projections` 是否被 `FrameSurfaceDraft` 完全取代。
- [ ] 审核 session persistence 中 runtime command、event、projection、lineage 的边界。
- [ ] 如 schema 需要调整，编写 migration 并更新 migration guard。

验证重点：

- 长期事实源指向 AgentFrame revision。
- RuntimeSession persistence 聚焦 trace / event / command / terminal effect / lineage。
- migration guard 通过。

## Cross-Phase Validation

- [ ] `pnpm run backend:clippy`
- [ ] `pnpm --dir packages/app-web run typecheck`
- [ ] 相关 Rust tests：session startup、capability resolver、runtime gateway、workflow frame surface、MCP direct/relay。
- [ ] 相关 frontend tests：AgentRun workspace、workspace-panel runtime state、lifecycle services。
- [ ] `pnpm run migration:guard` 在涉及 schema 或 generated contracts 时执行。
- [ ] 必要时用 `pnpm dev` 做启动证明。

## Risk Areas

- `SessionRuntimeInner` 当前同时连接 execution、events、capability、commands，阶段 3 需要控制改动面。
- `AgentFrame.mcp_surface_json` 是 JSON surface，目标收束需要避免每个消费者各自 parse。
- generated contracts 与 frontend hand-written types 需要一起更新。
- MCP direct / relay / local prompt wire shape 有独立边界，重命名时要保留 wire DTO 语义。

## Review Gate Before Start

- [ ] 用户确认长期目标采用目标收束。
- [ ] 用户确认第一批实现范围是否限定为 MCP declaration / runtime binding context 命名收束。
- [ ] 用户确认是否需要拆父子任务：命名收束、FrameSurfaceDraft、runtime launch、AgentRun workspace、persistence cleanup 可独立验证。
