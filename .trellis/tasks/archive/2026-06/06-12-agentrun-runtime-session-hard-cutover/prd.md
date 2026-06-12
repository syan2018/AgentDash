# AgentRun 与 RuntimeSession Hard Cutover 收束

## Goal

把 AgentRun / AgentFrame / RuntimeSession 模型从标准收束推进到 hard cutover 状态：AgentFrame 是 runtime surface revision 的唯一事实源，AgentRun workspace 是用户命令面，RuntimeSession 只承担 delivery、trace、event、connector continuation、runtime action adapter 和持久化证据。

本轮不维护旧测试 fixture 兼容面。与目标模型冲突的过渡字段、fallback accessor、旧 projection helper 和测试壳应删除，必要测试改为直接构造真实 `FrameSurfaceDraft` / AgentFrame surface。

## Confirmed Facts

- `RuntimeMcpServerDeclaration` / `McpRuntimeBindingContext` 命名已经完成标准收束，旧 `SessionMcpServer` / `SessionRuntimeMcpContext` 不应再作为生产概念出现。
- `FrameLaunchEnvelope` 当前仍同时携带 `surface_draft` 和 `executor_config` / `capability_state` / `vfs` / `mcp_servers` 过渡字段，`launch_*` accessor 仍有 fallback。
- `RuntimeContextInspectionPlan.projections.frame_surface_draft` 已存在，但同一结构仍保留 `mcp_servers` / `capability_state` fixture 字段，并有 `surface_draft_or_fixture_projection` 合并逻辑。
- `session::assembler` 仍负责 Project / Story owner bootstrap 的 VFS、capability、MCP、context bundle composition，并通过 `compose_owner_bootstrap_to_frame` 交给 frame construction。
- AgentRun workspace API 已使用 AgentRun control/action DTO；RuntimeSession control DTO 已限定在 runtime trace/detail 入口。
- RuntimeSession persistence 当前保存 trace-head meta、events、delivery commands、terminal effects、projection checkpoint 与 lineage，不保存 AgentRun current executable surface truth。

## Requirements

- 删除 `FrameLaunchEnvelope` 上与 `FrameSurfaceDraft` 并列表达运行面的过渡字段。
- 删除 `FrameLaunchEnvelope::sync_transitional_fields_from_surface_draft` 和带 fallback 语义的 `launch_*` accessor；若保留 accessor，只能无条件读取 non-optional typed launch surface。
- 将 `FrameSurfaceDraft` 或新的 launch surface 结构调整为 launch-ready typed handoff，进入 planner 前必须保证 capability、VFS、MCP、execution profile 完整。
- 删除 `RuntimeContextInspectionPlan.projections.mcp_servers` / `capability_state` 以及 `surface_draft_or_fixture_projection`。
- 删除或重写依赖旧 projection fixture 的测试，不保留仅为旧 fixture 服务的兼容 helper。
- 将 owner bootstrap / owner scope / VFS+capability+MCP+context composition 从 session 层收束到 frame construction 或更贴近 AgentRun/AgentFrame 的 composer 边界。
- 保留 RuntimeSession 作为 delivery runtime session、trace id、runtime action context、event persistence 和 connector continuation 的语义。
- 保留 `CapabilityState.tool.mcp_servers` 作为 capability/draft projection 和 tool policy / replay 输入；它必须从 AgentFrame/launch surface 派生，不能成为并列事实源。
- 更新 backend、runtime gateway、session startup、execution context、capability、frontend contract 相关 spec，使文档描述目标状态而不是迁移状态。
- 所有生产路径必须编译、lint 和 targeted tests 通过；如果不涉及 schema，仍运行 migration guard 证明无需 migration。

## Acceptance Criteria

- [x] `FrameLaunchEnvelope` 不再包含 `executor_config`、`capability_state`、`vfs`、`mcp_servers` 四个并列 surface 字段。
- [x] 生产 launch planner、turn preparation、connector context、MCP discovery 只读取 typed launch surface / AgentFrame surface / active execution snapshot。
- [x] `RuntimeContextInspectionPlan.projections` 不再保留 `mcp_servers` / `capability_state` 旧 fixture 字段。
- [x] `surface_draft_or_fixture_projection` 不存在；旧测试 fixture 要么删除，要么改为真实 `FrameSurfaceDraft` / AgentFrame 构造。
- [x] `session::assembler` 不再作为 owner bootstrap composition 的归属模块；frame construction 负责业务 surface composition，session 模块只保留 runtime launch/delivery/event/persistence 边界。
- [x] `rg` 检查确认旧 fixture/fallback 命名无生产残留：`surface_draft_or_fixture_projection`、`sync_transitional_fields_from_surface_draft`、`projections.mcp_servers`、`projections.capability_state`。
- [x] AgentRun workspace API/UI 仍使用 AgentRun command/control 模型，SessionRuntime control 仍只在 runtime trace/detail 入口。
- [x] RuntimeSession persistence 边界保持 trace / event / command / terminal effect / projection / lineage，不引入 current surface 字段。
- [x] Specs 与实现一致，文档写目标状态和原因，不保留迁移期说明。
- [x] 验证通过：`git diff --check`、`pnpm run backend:clippy`、`cargo check -p agentdash-application`、`pnpm run contracts:check`、`pnpm run migration:guard`、session launch / hub / runtime gateway / capability / MCP targeted tests。

## Out Of Scope

- 不重新设计 `RuntimeSession` event log、terminal effect outbox、lineage、compaction projection 的数据库模型。
- 不移除 `RuntimeSession` 作为 delivery/trace/runtime action identity。
- 不为了外部兼容保留旧字段、旧 DTO alias 或旧测试 fixture。

## Open Questions

- 无阻塞开放问题。用户已明确要求彻底收束并清理旧测试 fixture。
