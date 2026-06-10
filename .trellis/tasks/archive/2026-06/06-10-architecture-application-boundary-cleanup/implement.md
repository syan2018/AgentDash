# Implementation Plan

## Slice 1: MCP discovery port

- [x] 在 `agentdash-application-ports` 新增 `mcp_discovery` port 和 discovery entry 类型。
- [x] 在 `agentdash-executor::mcp` 新增 `ExecutorMcpToolDiscovery` adapter，集中合并 direct / relay discovery。
- [x] 将 `SessionRuntimeInner` / `TurnPreparationDeps` 的 MCP discovery 依赖切换为 port。
- [x] 在 API session bootstrap 注入 executor adapter。
- [x] 移除 `agentdash-application` 对 `agentdash-executor` 的 Cargo 依赖。
- [x] 运行 `cargo check -p agentdash-application -p agentdash-api -p agentdash-executor`。

## Slice 2: frontend contract DTO boundary

- [x] 在 `agentdash-application::workspace_module` 定义 application-owned WorkspaceModule read model。
- [x] 将 WorkspaceModule Agent 工具切换为内部 read model，并保持工具 details JSON 形态由 serde 序列化。
- [x] 在 `agentdash-api::routes::workspace_module` 映射 application read model 到 `agentdash-contracts::workspace_module` DTO。
- [x] 在 `agentdash-application::workflow::lifecycle_run_view_builder` 定义 lifecycle / subject / project active agents read model。
- [x] 在 `agentdash-api::routes::lifecycle_contracts` 映射 lifecycle read model 到 `agentdash-contracts::workflow` DTO。
- [x] 将 workflows lifecycle view、lifecycle_views、story_runs route 接入 API mapper。
- [x] 移除 `agentdash-application` 对 `agentdash-contracts` 的 Cargo 依赖。
- [x] 运行 `cargo check -p agentdash-application`。
- [ ] 运行 `cargo check -p agentdash-application -p agentdash-api`（当前被 `crates/agentdash-api/src/routes/canvases.rs` 的 `Option<u64>` / `Option<i64>` 外部类型错误阻断）。
- [x] 运行 `pnpm run contracts:check`。

## Slice 3: application-ports relay wire DTO boundary

- [x] 在 `agentdash-application-ports::extension_runtime` 定义 application-owned extension action/channel request/response payload。
- [x] 在 `agentdash-application-ports::vfs_materialization` 定义 application-owned materialization request/response、plan enum、target enum、access mode、cache scope 与 content entry。
- [x] 移除 `agentdash-application-ports` 对 `agentdash-relay` 的 Cargo 依赖。
- [x] 将 `agentdash-application::runtime_gateway::extension_actions` 和 `agentdash-application::vfs::materialization` 切换为构造 application-owned payload。
- [x] 在 `agentdash-api::relay::extension_runtime_impl` 和 `agentdash-api::vfs_materialization` 增加 application payload 与 relay wire payload 的互转。
- [x] 运行 `cargo check -p agentdash-application-ports -p agentdash-application`。
- [ ] 运行 `cargo check -p agentdash-application-ports -p agentdash-application -p agentdash-api`。

## Later slices

- [ ] AgentProcedure / WorkflowGraph / canvas routes 的全面 DTO 收敛。
- [ ] legacy identity cleanup。
- [ ] workspace tab store / registry 边界 cleanup。
