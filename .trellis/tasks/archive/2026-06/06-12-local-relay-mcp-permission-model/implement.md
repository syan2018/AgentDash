# 本机 relay MCP 权限模型收敛实施计划

## Checklist

- [x] 确认 protect mode 默认关闭。
- [x] 确认 protect mode 配置入口：`local-backend.json` 的 `mcp_protect_mode`，默认关闭。
- [x] 梳理现有 relay MCP discovery 路由点，移除普通 Project relay MCP 对 `backend.capabilities.mcp_servers` 的准入依赖。
- [x] 为 relay MCP discovery / call 引入 session-bound backend 路由，优先使用当前 session 的 backend execution placement 或 default VFS backend。
- [x] 调整 local `McpClientManager`，允许 project-scoped declaration 按 transport 惰性连接。
- [x] 增加 local protect mode policy：关闭时允许 project declaration，开启时按 allowlist / origin 拒绝。
- [x] 将 local policy denied 转换为可诊断错误信息；backend offline、transport failure、handshake failure 继续沿现有 relay/runtime error path 返回。
- [x] 增加针对 Hoyo `abc-copilot-tool` 断点的回归验证：local static MCP catalog 为空时云端仍能按 session route 发现 relay MCP；protect mode 开启且未允许时产生明确拒绝。
- [x] 更新相关 spec / docs，记录 Project MCP Preset、local static MCP catalog、protect mode 的职责分工。

## Validation

- `cargo test -p agentdash-local mcp_client_manager -- --nocapture` 通过。
- `cargo test -p agentdash-api relay::registry -- --nocapture` 通过。
- `cargo test -p agentdash-executor mcp::relay -- --nocapture` 通过。
- `pnpm run backend:clippy` 未运行，本轮按用户要求控制验证范围。

## Risk Points

- relay discovery 当前缺少 session context 时，需要为非 session probe 场景保留合理 backend 选择策略。
- direct MCP 与 relay MCP 共用 tool naming / capability filtering，调整时要避免影响已可用的远端 direct MCP。
- local runtime 配置变更如果涉及桌面 UI，需要同步合同类型与前端设置入口。

## Review Gate Before Start

实现前需要确认：

- allowlist 初始粒度是否为 server name + transport origin。
- discovery 诊断落到 session context、API response、日志，还是三者都需要。
