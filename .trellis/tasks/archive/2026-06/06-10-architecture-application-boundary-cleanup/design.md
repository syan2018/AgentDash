# Design: application 边界去耦

## Scope

本任务分阶段切断 `agentdash-application` 对外层实现和 wire DTO 的直接依赖。已完成 MCP discovery slice：`agentdash-application` 不再直接调用 `agentdash_executor::mcp::discover_*`，MCP 工具发现通过 application-owned port 注入。

本轮 slice 聚焦 frontend contract DTO 去耦：`agentdash-application` 不再依赖 `agentdash-contracts`，workspace module 与 lifecycle run view builder 输出 application-owned read model，API route 层负责映射为浏览器 contract DTO。

本次 relay DTO port slice 聚焦 `agentdash-application-ports`：extension runtime action/channel 与 VFS materialization transport 只暴露 application-owned request/response、enum 与错误；relay command/response payload 由 API/local/integration adapter 边界负责互转。

## Ownership

- `agentdash-application-ports::mcp_discovery` 拥有 MCP discovery port。
- `DiscoveredMcpTool` 放在 application port，原因是 application 的 runtime gateway 需要读取工具描述并保留可执行 `DynAgentTool`。
- `McpToolDiscovery` 只表达 application 需要的发现语义：输入 session MCP server、`CapabilityState` 和可选 relay call context，输出 capability-filtered tool entries。
- `agentdash-executor::mcp::ExecutorMcpToolDiscovery` 实现该 port，并在 executor 内集中合并 direct / relay MCP 工具发现。
- API bootstrap 是 composition root，负责把现有 `McpRelayProvider` 装配进 executor adapter，再注入 `SessionRuntimeBuilder`。
- `agentdash-application::workspace_module` 拥有 workspace module read model，原因是 Agent 工具与 API route 都需要同一份应用语义投影，但浏览器 wire DTO 属于 API/contract 边界。
- `agentdash-application::workflow::lifecycle_run_view_builder` 拥有 lifecycle / subject / project active agents read model，原因是该 builder 组合 repository facts，输出的是应用查询结果而不是 HTTP wire contract。
- `agentdash-api::routes::workspace_module` 拥有 workspace module read model 到 `agentdash-contracts::workspace_module` 的 mapper。
- `agentdash-api::routes::lifecycle_contracts` 拥有 lifecycle read model 到 `agentdash-contracts::workflow` view DTO 的 mapper，并由 workflows / lifecycle_views / story_runs route 复用。
- `agentdash-application-ports::extension_runtime` 拥有 extension runtime transport 的应用层 payload，原因是 runtime gateway 只需要表达扩展调用语义，不应知道 relay wire message shape。
- `agentdash-application-ports::vfs_materialization` 拥有 VFS materialization request/response 与 plan/content enum，原因是 application 负责构建物化计划，relay 只是远端执行传输。
- `agentdash-api::relay::extension_runtime_impl` 与 `agentdash-api::vfs_materialization` 拥有 application payload 到 `agentdash-relay` command/response payload 的 mapper。

## Boundaries

- application 可以依赖 `agentdash-application-ports` 和 `agentdash-spi` 类型，但不能依赖 executor discovery 实现。
- executor 可以依赖 application port 来实现外层 adapter。
- relay MCP transport 与 tool call 仍通过 `agentdash-spi::McpRelayProvider` 表达。
- Extension runtime 与 VFS materialization transport 的 relay wire DTO 只出现在 API/local/relay adapter 边界；application-ports 不依赖 `agentdash-relay`。
- runtime tool provider 与 MCP discovery 在 application 的 session preparation/hub 中合并，原因是 runtime tools 与 MCP tools 共同组成 connector-facing tool surface。
- application read model 可以 derive serde 供 Agent 工具 details 序列化，但不能引用 `agentdash-contracts` 或 ts-rs frontend DTO。
- API route 可以依赖 `agentdash-contracts`，原因是 route 是浏览器 HTTP contract 边界。
- lifecycle route mapper 只覆盖 `LifecycleRunView` / `SubjectExecutionView` / `ProjectActiveAgentsView` 相关读模型；`AgentProcedure`、`WorkflowGraph` 与 canvas route 的 DTO 收敛留给后续 slice。

## Non-goals For This Slice

- 不修改 extension runtime action。
- 不修改 extension manifest、legacy identity、workspace tab。
- 不处理 AgentProcedure / WorkflowGraph / canvas routes 的全面 DTO 收敛。
- 不处理 legacy identity 或 workspace tab。
