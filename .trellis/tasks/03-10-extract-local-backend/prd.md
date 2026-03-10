# 拆分本机后端：提取执行引擎 + WebSocket 客户端

## Goal

将当前单体 `agentdash-server` 中属于"本机执行"的部分提取为独立的 `agentdash-local` binary，使其：
1. 只保留 Agent 执行、工作空间文件访问等本机能力
2. 启动时主动连接云端 WebSocket，注册自身并接受命令
3. 将执行结果实时通过 WebSocket 回传给云端

**本机后端的定位：第三方 Agent 的执行工人 + PiAgent 的远程工具执行环境，被云端调度。**

## Requirements

### R1: 新建 `agentdash-local` binary

在 `crates/agentdash-api/` 中新增 `src/bin/agentdash_local.rs`（或创建独立 crate `crates/agentdash-local/`），作为本机后端入口。

**CLI 参数：**
```
agentdash-local \
  --cloud-url wss://cloud.example.com/ws/backend \
  --token <auth-token> \
  --accessible-roots <path1>,<path2>  # 本机可访问的工作空间目录列表
```

**启动流程：**
1. 解析 CLI 参数（含 `accessible-roots` 列表）
2. 初始化本地 ExecutorHub（复用现有逻辑）
3. 初始化 ToolExecutor（以 `accessible_roots` 配置可访问目录）
4. 扫描本机可用的 AgentConnector 能力
5. 建立到云端的 WebSocket 连接
6. 发送注册消息（携带 token + 能力声明 + `accessible_roots`）
7. 进入命令监听循环

### R2: WebSocket 客户端实现

新建 crate `crates/agentdash-relay/`（或在 `agentdash-local` 内实现模块），负责：

**连接管理：**
- 使用 `tokio-tungstenite` 建立 WebSocket 连接
- 自动重连（指数退避：1s → 2s → 4s → ... → 60s 上限）
- 心跳发送（每 30s 发送 pong 响应 / 主动 ping）
- 连接状态追踪和日志

**命令处理：**
- 接收云端 `command.prompt` → 路由到 ExecutorHub.start_prompt()（第三方 Agent）
- 接收 `command.cancel` → 路由到 ExecutorHub.cancel()
- 接收 `command.discover` → 返回 connector.list_executors()（仅第三方 Agent）
- 接收 `command.workspace_files` → 读取本地文件
- 接收 `command.tool.file_read` → ToolExecutor 读取文件（PiAgent tool call）
- 接收 `command.tool.file_write` → ToolExecutor 写入文件（PiAgent tool call）
- 接收 `command.tool.shell_exec` → ToolExecutor 执行 Shell（PiAgent tool call）
- 接收 `command.tool.file_list` → ToolExecutor 列出目录（PiAgent tool call）

**事件上报：**
- Agent 执行产生的 `SessionNotification` 流实时通过 WS 发送
- 使用 `event.session_notification` 消息类型
- 包含 session_id 以便云端关联到正确的 Task

### R3: 从当前 `agentdash-server` 移除云端职责

**保留（本机需要）：**
- `agentdash-executor` — 完整保留（第三方 Agent 执行）
- `agentdash-injection` — 按需保留（本地上下文组装可能需要）
- 本地 SQLite — 仅存储执行缓存和 session 历史（非业务数据）

**移至云端（不再由本机依赖）：**
- `agentdash-agent` — PiAgent AgentLoop 运行在云端（详见 Task 3）

**新增（本机需要）：**
- `ToolExecutor` 模块 — 处理 PiAgent 的 `command.tool.*` 调用（文件读写、Shell 执行）

**移除（归云端所有）：**
- `routes/projects.rs` — Project CRUD
- `routes/stories.rs` — Story CRUD + Task CRUD
- `routes/story_sessions.rs` — Session 绑定
- `routes/backends.rs` — Backend 管理
- `routes/settings.rs` — 设置管理
- `routes/address_spaces.rs` — 寻址空间
- `stream.rs` — 全局事件流（SSE/NDJSON）— 归云端
- MCP 路由（`agentdash-mcp`）— 归云端

**灰色地带（需要设计决策）：**
- `routes/workspaces.rs` — 元数据归云端，但 `pick-directory`、`detect-git`、文件读取等操作需要本机执行
- `routes/workspace_files.rs` — 文件读取需要在本机执行，但由云端 API 中继
- `routes/acp_sessions.rs` — session CRUD 归云端，但 stream 数据从本机产生
- `routes/discovery.rs` — 能力发现需要查询本机，但由云端聚合

### R4: 保留本地 HTTP API（可选）

本机后端可能仍需要一个最小的 HTTP 端口用于：
- 本地调试（`/health`、`/debug/*`）
- 本地前端直连（单机模式 / 开发模式）

设计上应支持 `--local-port <port>` 可选参数，不指定则不启动 HTTP。

### R5: 现有 `agentdash-server` 的过渡策略

当前 `agentdash-server` 在重构期间应保持可运行：
- 暂时不删除，作为"单机模式"的回退方案
- 新的 `agentdash-local` 从其中提取代码，而非修改原 binary
- 重构完成后，`agentdash-server` 可能合并回 cloud 或正式废弃

## Acceptance Criteria

- [ ] `agentdash-local` 可通过 `cargo run --bin agentdash-local -- --cloud-url ... --token ... --accessible-roots ...` 启动
- [ ] 启动后成功连接到云端 WebSocket 并完成注册
- [ ] 能通过 WS 接收 `command.prompt` 并触发本地第三方 Agent 执行
- [ ] 第三方 Agent 执行过程中的 SessionNotification 实时通过 WS 回传
- [ ] 支持 `command.cancel` 取消正在执行的任务
- [ ] 支持 `command.discover` 返回本机第三方执行器能力列表
- [ ] 支持 `command.tool.*`（file_read/file_write/shell_exec/file_list）处理 PiAgent 工具调用
- [ ] WebSocket 断线后自动重连，重连后重新注册
- [ ] 原有 `agentdash-server` 不受影响，仍可独立运行

## Definition of Done

- 代码通过 lint 和 typecheck
- 能力发现和 prompt 执行的端到端流程可通过手工测试验证
- 文档已更新（启动命令、CLI 参数说明）

## Out of Scope

- 不实现云端侧逻辑（由 Task 3 负责）
- 不实现前端改动
- 不实现数据库迁移（本机只用 SQLite 缓存）
- 不实现生产级鉴权（token 直接比对即可）
- 不考虑 TLS 证书管理（wss 由基础设施层处理）

## Technical Notes

### 代码提取清单

| 源文件 | 动作 | 目标 |
|--------|------|------|
| `agentdash-executor/` | 完整复用 | agentdash-local 核心依赖（第三方 Agent） |
| `agentdash-agent/` | **不再依赖** | PiAgent 移至云端（agentdash-cloud 依赖） |
| `app_state.rs` | 精简版 | 只保留 executor_hub、connector、tool_executor、task_lock 等 |
| `bootstrap/task_execution_gateway.rs` | 适配 | 改为监听 WS 命令而非 HTTP 请求 |
| `routes/workspace_files.rs` | 改为内部模块 | 被 WS command handler 调用 |

### 新增 crate 结构

```
crates/agentdash-relay/
├── Cargo.toml
├── src/
│   ├── lib.rs
│   ├── protocol.rs     # WS 消息类型定义（共享于 cloud 和 local）
│   ├── client.rs       # WS 客户端（本机使用）
│   ├── server.rs       # WS 服务端（云端使用，Task 3）
│   ├── handler.rs      # 命令处理路由
│   └── tool_executor.rs # PiAgent tool call 本地执行器（多 workspace 目录感知）
```

`agentdash-relay` 作为共享 crate，同时被 local 和 cloud binary 依赖。

### 关键依赖

- `tokio-tungstenite` — 已在 workspace patch 中
- `clap` — CLI 参数解析（新增）
- `agentdash-executor` — 第三方 Agent 执行能力
- `agent-client-protocol` — ACP 类型
- ~~`agentdash-agent`~~ — **不再依赖**，PiAgent 移至云端

## Dependencies

- **前置**: Task 1（通信协议设计）— 需要消息格式定义
- **并行**: 可与 Task 3 并行开发（共享 `agentdash-relay` crate 的 protocol 模块）
