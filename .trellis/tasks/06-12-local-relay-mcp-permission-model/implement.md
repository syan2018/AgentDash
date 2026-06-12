# 本机 relay MCP 权限模型收敛实施计划

## Checklist

- [x] 确认 protect mode 默认关闭。
- [ ] 确认 protect mode 配置入口。
- [ ] 梳理现有 relay MCP discovery 路由点，移除普通 Project relay MCP 对 `backend.capabilities.mcp_servers` 的准入依赖。
- [ ] 为 relay MCP discovery / call 引入 session-bound backend 路由，优先使用当前 session 的 backend execution placement 或 default VFS backend。
- [ ] 调整 local `McpClientManager`，允许 project-scoped declaration 按 transport 惰性连接。
- [ ] 增加 local protect mode policy：关闭时允许 project declaration，开启时按 allowlist / origin 拒绝。
- [ ] 将 relay MCP discovery 失败转换为结构化诊断，覆盖 backend offline、policy denied、transport failure、handshake failure。
- [ ] 增加针对 Hoyo `abc-copilot-tool` 的回归验证：local static MCP catalog 为空时也能发现工具；protect mode 开启且未允许时产生明确拒绝。
- [ ] 更新相关 spec / docs，记录 Project MCP Preset、local static MCP catalog、protect mode 的职责分工。

## Validation

- `pnpm run backend:clippy`
- 针对 MCP discovery / relay provider / local MCP manager 的 Rust 单元测试。
- Hoyo 手工或集成验证：`abc-copilot-tool` 出现在 PI_AGENT 工具目录中，工具名形如 `mcp_abc_copilot_tool_*`。
- protect mode 验证：相同 Project Preset 在策略拒绝时不会静默消失，而是输出可诊断原因。

## Risk Points

- relay discovery 当前缺少 session context 时，需要为非 session probe 场景保留合理 backend 选择策略。
- direct MCP 与 relay MCP 共用 tool naming / capability filtering，调整时要避免影响已可用的远端 direct MCP。
- local runtime 配置变更如果涉及桌面 UI，需要同步合同类型与前端设置入口。

## Review Gate Before Start

实现前需要确认：

- allowlist 初始粒度是否为 server name + transport origin。
- discovery 诊断落到 session context、API response、日志，还是三者都需要。
