# Local Backend 轻量管理界面

## 背景

agentdash-local 目前是纯 CLI 启动、无界面的后台进程。随着 MCP relay 能力的引入
（见 `04-08-pi-agent-stdio-mcp-relay`），本机侧需要管理的配置和状态越来越多：

- MCP server 配置（新增/编辑/删除，查看连接状态）
- Backend 连接状态（与云端的 WebSocket 连接、心跳、重连）
- stdio 进程监控（哪些 MCP 进程在运行、资源占用、手动重启/停止）
- accessible_roots 管理

目前这些都需要手动编辑配置文件或查看日志，体验差。
需要一个轻量的本地 Web 界面作为 local backend 的管理入口。

## 目标

为 agentdash-local 提供一个 **仅监听 localhost** 的轻量 Web 界面，支持：

1. **MCP 配置管理**（首要场景）
   - 可视化编辑 `.agentdash/mcp-servers.json`
   - 每个 server 的连接状态指示（idle / connected / error）
   - 快速测试连接（手动触发 list_tools 验证配通）

2. **连接状态总览**
   - 云端 WebSocket 连接状态
   - 当前注册的 backend 信息（id, name, 上线时长）
   - 已上报的 capabilities 快照

3. **进程监控**（MCP relay 上线后）
   - 当前活跃的 stdio MCP 进程列表
   - 按 session 分组查看
   - 手动停止/重启进程

4. **日志查看**
   - 近期 relay 命令日志（可按类型过滤）
   - 错误高亮

## 技术方向（待讨论）

- **框架选型**：嵌入式 Web server（axum/actix-web）+ 静态前端？还是 TUI？
- **端口**：默认 `localhost:19840`（或随机端口 + 启动时打印）
- **前端**：可以极简——单页 HTML + vanilla JS，或嵌入一个轻量 React build
- **安全**：仅 localhost 绑定，无需 auth

## 前置依赖

- `04-08-pi-agent-stdio-mcp-relay` 的 MCP 配置文件格式确定后再设计编辑界面
- McpClientManager 的 API 确定后再设计进程监控界面

## 非目标

- 不是云端 AgentDashboard 的替代——这只是本机侧的运维工具
- 不处理 session 管理、agent 配置等云端职责
