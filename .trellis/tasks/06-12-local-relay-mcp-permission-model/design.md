# 本机 relay MCP 权限模型收敛设计

## Current Shape

当前链路中存在三类事实：

- Project MCP Preset：项目级 MCP server 声明，包含 key、display name、transport、route policy、installed source。
- CapabilityState：本轮 Agent 能力状态，包含 `tool.capabilities` 与 `tool.mcp_servers`。
- Local backend capabilities：本机 runtime 上线时上报的 executor 与本机静态 MCP catalog。

Hoyo `abc-copilot-tool` 的前两类事实已经完整；第三类事实为空，导致 relay discovery 在进入本机 list_tools 前被拦截。

## Target Model

Project relay MCP 的运行权威应由 Project MCP Preset + CapabilityState 组成：

1. Agent config 提供 `mcp_preset_keys`。
2. application 解析 Project MCP Preset，生成 `RuntimeMcpServerDeclaration { name, transport, uses_relay }`。
3. owner bootstrap 将 declaration 写入 frame MCP surface 与 `CapabilityState.tool.mcp_servers`。
4. executor MCP discovery 根据 `uses_relay` 选择 direct 或 relay 路径。
5. relay 路径把完整 declaration 下发给当前 session 绑定的 local backend。
6. local backend 按 declaration transport 惰性连接 MCP server。

Local backend capabilities 继续表示本机预配置能力，但只作为 catalog / allowlist / debug 信号，不作为普通 Project relay MCP 的唯一准入条件。

## Protect Mode

`protect mode` 是 local runtime 的本机安全策略。开启后，本机只接受本机策略允许的 relay MCP declaration。

建议初始策略：

- 默认关闭，保持项目级 MCP Preset 可直接驱动本机 relay execution；本机 runtime / 桌面设置可手动开启。
- 开启后按 server name 与 transport origin 校验。
- 对被拒绝的 declaration 返回结构化诊断，例如 `local_policy_denied`。
- local static MCP catalog 可作为默认 allowlist 来源。

这样可同时满足两种使用方式：

- 预研/开发：Project Preset 是权威，减少手工同步配置。
- 受控环境：local runtime 手动进入保护模式，只允许预提供工具。

## Data Flow

```text
ProjectAgent.config.mcp_preset_keys
  -> resolve_preset_mcp_presets(project_id, keys)
  -> RuntimeMcpServerDeclaration { uses_relay }
  -> CapabilityState.tool.mcp_servers
  -> McpToolDiscovery
  -> relay provider routes by session/backend placement
  -> local backend policy check
  -> MCP list_tools / call_tool
```

## Code Boundaries

- `agentdash-application`: session/frame construction、CapabilityState 与 MCP declaration 的权威投影。
- `agentdash-executor`: MCP discovery 分 direct / relay 路径，并把 discovered MCP tools 注册为 AgentTool。
- `agentdash-api`: relay provider 根据 session/backend placement 路由 MCP command。
- `agentdash-local`: 按 declaration 惰性连接 MCP server，并执行 protect mode policy。
- `agentdash-contracts` / frontend settings：如需要暴露 protect mode 配置，补齐 DTO 与 UI。

## Trade-Offs

- 保留本机静态 catalog 作为普通准入条件实现简单，但需要用户同步两份配置，且会继续制造 capability surface 与工具 schema 不一致。
- 允许 project-scoped declaration 惰性连接能让 Project MCP Preset 成为单一业务事实源；安全边界转移到 local policy 与 session/workspace 绑定上。
- protect mode 提供本机最终裁决，适合受控环境，但需要清晰诊断，否则会把策略拒绝误判为工具缺失。

## Migration Notes

项目仍处于预研期，不需要兼容旧行为。若数据库或配置需要迁移，应直接迁移到当前正确模型。

现有 local static MCP catalog 可继续保留为本机配置文件格式；它的语义从“Project relay MCP 准入前置”收敛为“本机预配置 catalog 和 protect mode 默认 allowlist”。
