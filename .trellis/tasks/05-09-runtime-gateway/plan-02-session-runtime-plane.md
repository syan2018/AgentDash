# 主线 2：Session Runtime Plane

## Goal

把普通 Runtime Surface 明确收束到 Session，使 Agent、Canvas、Workflow node、会话 UI 的运行态调用都通过 `session_id` 找到同一个能力面：VFS、MCP servers、CapabilityState、working directory、identity、environment variables 和 active execution。

## Scope

- Session-bound `RuntimeSurface` 构建。
- `mcp.call_tool` Session Runtime Action。
- direct MCP / relay MCP provider 适配。
- `RuntimeActionToolAdapter`，让 Agent 通过 Gateway 调用 action。
- Session event / trace 投影的最小链路。

## Dependencies

- 依赖主线 1 的 Gateway Core Protocol。
- 依赖现有 `SessionHub::get_latest_capability_state`、`build_tools_for_execution_context`、`McpRelayProvider`、direct MCP discovery。

## Execution Plan

1. 定义 Session Runtime Context 装配入口：
   - 输入：`session_id`、actor、action_key、target、input。
   - 从 SessionHub / session persistence 读取当前 capability state、mcp servers、vfs、turn/session frame。
2. 构建 `RuntimeSurface`：
   - 从 `CapabilityState` 派生可见 action。
   - 将 direct MCP / relay MCP 工具映射为 `mcp.call_tool` target。
   - 明确 tool filter 作用在原始 MCP tool name 上。
3. 实现 MCP Runtime Provider：
   - direct MCP 路由。
   - relay MCP 路由。
   - 统一结果为 `RuntimeInvocationResult`。
4. 实现 `RuntimeActionToolAdapter`：
   - 将 Runtime Action 暴露为 AgentTool。
   - Agent tool call 进入 Gateway，而不是直接调 provider。
5. 定义 session trace 投影：
   - 每次 invocation 生成 trace_id。
   - 可选注入 session event，供前端和审计查看。
6. 测试：
   - capability 允许时 MCP action 可见。
   - capability 拒绝时 action 不可见或 invocation 被拒绝。
   - relay MCP provider 错误能归一化。
   - AgentToolAdapter 只调用 Gateway。

## Acceptance Criteria

- 普通 runtime action 没有 `session_id` 时必须失败。
- Session 是 Runtime Surface 的唯一一等宿主。
- Agent / Canvas / Workflow actor 不拥有独立工具面，只通过 session surface 调用。
- `mcp.call_tool` 端到端设计明确 direct 与 relay 两条路由。
- 现有 capability tool filter 不被绕过。

## Risks

- `SessionHub` 当前工具构建逻辑和 Gateway surface 可能出现重复发现，需要明确单一来源。
- MCP server 生命周期若仍由 executor 管，Gateway provider 需要避免抢占连接管理职责。
- ToolAdapter 如果直接持有 provider，会绕过 Gateway policy。

## First PR Shape

- 先实现只读 surface 查询和 `mcp.call_tool` invocation。
- 再接 AgentToolAdapter。
- Canvas / Workflow 暂不接入，只提供可复用 Session runtime plane。
