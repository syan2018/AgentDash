# Implementation Plan

## Slice 1: MCP discovery port

- [x] 在 `agentdash-application-ports` 新增 `mcp_discovery` port 和 discovery entry 类型。
- [x] 在 `agentdash-executor::mcp` 新增 `ExecutorMcpToolDiscovery` adapter，集中合并 direct / relay discovery。
- [x] 将 `SessionRuntimeInner` / `TurnPreparationDeps` 的 MCP discovery 依赖切换为 port。
- [x] 在 API session bootstrap 注入 executor adapter。
- [x] 移除 `agentdash-application` 对 `agentdash-executor` 的 Cargo 依赖。
- [x] 运行 `cargo check -p agentdash-application -p agentdash-api -p agentdash-executor`。

## Later slices

- [ ] relay wire DTO port 去耦：application ports 不暴露 `agentdash_relay` command/response payload。
- [ ] frontend contract DTO 去耦：application 输出 read model，API adapter 构造 `agentdash-contracts` DTO。
- [ ] legacy identity cleanup。
- [ ] workspace tab store / registry 边界 cleanup。
