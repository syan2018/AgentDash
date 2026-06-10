# Design: application 边界去耦

## Scope

本任务分阶段切断 `agentdash-application` 对外层实现和 wire DTO 的直接依赖。本轮只实现 MCP discovery slice：`agentdash-application` 不再直接调用 `agentdash_executor::mcp::discover_*`，MCP 工具发现通过 application-owned port 注入。

## Ownership

- `agentdash-application-ports::mcp_discovery` 拥有 MCP discovery port。
- `DiscoveredMcpTool` 放在 application port，原因是 application 的 runtime gateway 需要读取工具描述并保留可执行 `DynAgentTool`。
- `McpToolDiscovery` 只表达 application 需要的发现语义：输入 session MCP server、`CapabilityState` 和可选 relay call context，输出 capability-filtered tool entries。
- `agentdash-executor::mcp::ExecutorMcpToolDiscovery` 实现该 port，并在 executor 内集中合并 direct / relay MCP 工具发现。
- API bootstrap 是 composition root，负责把现有 `McpRelayProvider` 装配进 executor adapter，再注入 `SessionRuntimeBuilder`。

## Boundaries

- application 可以依赖 `agentdash-application-ports` 和 `agentdash-spi` 类型，但不能依赖 executor discovery 实现。
- executor 可以依赖 application port 来实现外层 adapter。
- relay MCP transport 与 tool call 仍通过 `agentdash-spi::McpRelayProvider` 表达；本轮不改 relay wire DTO port。
- runtime tool provider 与 MCP discovery 在 application 的 session preparation/hub 中合并，原因是 runtime tools 与 MCP tools 共同组成 connector-facing tool surface。

## Non-goals For This Slice

- 不修改 relay command/response DTO port。
- 不修改 extension runtime action。
- 不修改 frontend contracts DTO 构造边界。
- 不处理 legacy identity 或 workspace tab。
