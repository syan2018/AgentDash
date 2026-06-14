# 本机 relay MCP 权限模型收敛

## Goal

收敛 Project 级 relay MCP 的声明、权限、发现和本机执行模型，让 Agent 能稳定使用项目已授权的本机 MCP 工具，同时保留本机运行时可手动开启的保护模式，用于只允许预提供 MCP server 被使用。

## Background

当前 Hoyo `abc-copilot-tool` 的 seed、商城安装、Project Agent 配置和 runtime frame 投影均已形成完整链路：

- Hoyo seed 将 `abc-copilot-tool` 声明为 `relay`，其它 ABC MCP 为 `direct`。
- AgentTemplate 安装会把 `mcp_dependencies` 安装成 Project MCP Preset，并写入 `ProjectAgent.config.mcp_preset_keys`。
- owner bootstrap 会把 Project Agent 的 MCP Preset 合入 `CapabilityState.tool.mcp_servers` 和 frame `mcp_surface_json`。
- live DB 中最近 Agent frame 已包含 `mcp:abc-copilot-tool` 以及 `uses_relay: true` 的 `abc-copilot-tool` server 声明。

运行时断点出现在 relay MCP discovery：云端先通过在线 backend 上报的 `capabilities.mcp_servers` 反查哪个本机 backend 提供 `abc-copilot-tool`，而当前 local runtime 上报的 `mcp_servers` 为空。于是 Agent 能看到 capability 和 MCP surface，但 PI_AGENT 工具目录没有 `mcp_abc_copilot_tool_*` schema。

## Requirements

- Project MCP Preset 是项目级 MCP server 声明的权威事实源；Agent runtime 以 `CapabilityState` 判断本轮是否可用。
- `route_policy = relay` 的 MCP server 通过当前 session / workspace 绑定的本机 backend 执行，不能依赖本机 backend 启动时预声明同名 MCP server。
- local runtime 支持手动开启 `protect mode`，默认关闭；开启后只允许本机预提供或本机策略允许的 relay MCP server 被连接。
- 未开启 `protect mode` 时，项目级 relay MCP server 可以按其 transport 惰性连接本机 MCP server，并进入 MCP discovery。
- relay MCP discovery 失败时，需要返回可诊断的错误信息，让 UI / session context 能区分 backend 离线、transport 拒绝、MCP 握手失败、local policy 拒绝等原因。
- 保留本机静态 MCP catalog 的价值：用于本机预配置、调试展示、候选来源或保护模式 allowlist，而不是作为普通 Project relay MCP 的唯一准入事实。

## Acceptance Criteria

- [ ] Project Agent 仅配置 Project MCP Preset、local backend 未静态预声明该 MCP server 时，`route_policy = relay` 的 MCP 能被 relay discovery 发现并注册为 PI_AGENT 工具 schema。
- [ ] 开启 local `protect mode` 后，未被本机策略允许的 project relay MCP server 不会被连接，并产生明确诊断。
- [ ] direct MCP 路径保持现有行为，ABC 的远端 MCP 仍可被发现和调用。
- [ ] runtime frame / capability surface / MCP discovery 的事实源一致：不会出现 context 显示 MCP 已注入但模型工具目录静默缺失的状态。
- [ ] Hoyo `abc-copilot-tool` 场景有针对性验证，覆盖 `mcp:abc-copilot-tool` capability 已声明但 local static MCP catalog 为空的情况。
- [ ] 相关接口和文档说明 local MCP catalog、Project MCP Preset、protect mode 三者的职责边界。

## Open Questions

- `protect mode` 的 allowlist 粒度：建议先以 MCP server name + transport origin 为主，后续再扩展到 tool 级策略。
