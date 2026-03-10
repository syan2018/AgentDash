# 构建云端后端：用户 API + WebSocket 服务端 + MCP

## Goal

构建面向用户的云端后端 `agentdash-cloud`，它是整个系统的**中枢和数据主人**：
1. 持有所有业务数据（Projects/Stories/Tasks/Workspaces/Backends）
2. 为前端提供 REST API 和实时事件流
3. 为外部工具提供 MCP Server
4. 通过 WebSocket 接受本机后端注册，并将执行命令中继到对应本机
5. 运行 PiAgent AgentLoop（云端原生 Agent），直接访问业务数据，tool call 路由到本机

**云端后端的定位：用户入口 + 数据中枢 + 调度中继 + 云端原生 Agent 引擎。**

## Requirements

### R1: 新建 `agentdash-cloud` binary

创建 `crates/agentdash-cloud/` 或在 `crates/agentdash-api/` 中新增 `src/bin/agentdash_cloud.rs`，作为云端后端入口。

**启动配置（环境变量 / CLI 参数）：**
```
agentdash-cloud \
  --port 8080 \
  --database-url <db-url>  # SQLite 或 PostgreSQL
```

**启动流程：**
1. 连接数据库、执行 migration
2. 初始化 BackendRegistry（管理在线本机）
3. 挂载 REST API 路由
4. 挂载 WebSocket 端点 `/ws/backend`
5. 挂载 MCP 路由
6. 开始监听

### R2: 用户 REST API（从现有 agentdash-server 迁移）

**直接迁移（云端自身处理）：**
- `GET/POST /api/projects` + `GET/PUT/DELETE /api/projects/{id}` — Project CRUD
- `GET/POST /api/projects/{project_id}/workspaces` — Workspace 列表/创建
- `GET/PUT/DELETE /api/workspaces/{id}` — Workspace 操作
- `GET/POST /api/stories` + CRUD — Story 管理
- `GET/POST /api/stories/{id}/tasks` + Task CRUD — Task 管理
- `GET/POST /api/backends` — Backend 管理（包括在线状态）
- `GET/PUT /api/settings` — 设置
- `GET /api/address-spaces` + entries — 寻址空间
- Story Sessions（绑定）
- 全局事件流 SSE/NDJSON

**中继到本机（云端代理，实际由本机执行）：**
- `POST /api/tasks/{id}/start` → 根据 executor 类型分流：
  - **第三方 Agent**（Claude Code 等）：查找 Task.backend_id → 通过 WS 下发 `command.prompt`
  - **PiAgent**：在云端启动 AgentLoop，tool call 通过 WS 路由到本机 `command.tool.*`
- `POST /api/tasks/{id}/cancel` → 通过 WS 下发 `command.cancel`（第三方 Agent）/ 直接终止 AgentLoop（PiAgent）
- `GET /api/tasks/{id}/session` → 查本地缓存 + 可选查询本机
- `POST /api/sessions/{id}/prompt` → 通过 WS 中继
- `POST /api/sessions/{id}/cancel` → 通过 WS 中继
- `GET /api/acp/sessions/{id}/stream` → 云端缓存 + 实时 WS 转发
- `GET /api/agents/discovery` → 聚合所有在线本机的能力
- `POST /api/workspaces/pick-directory` → 中继到指定本机
- `POST /api/workspaces/detect-git` → 中继到指定本机
- `GET/POST /api/workspace-files/*` → 中继到指定本机

### R3: WebSocket 服务端

**端点：** `GET /ws/backend`

**连接建立：**
1. 本机发起 WebSocket 握手
2. 云端验证 token（query param 或首条消息）
3. 注册到 BackendRegistry，标记为 online
4. 开始双向消息循环

**BackendRegistry（新组件）：**
```rust
struct BackendRegistry {
    // backend_id → WebSocket 发送端 + 能力声明 + 在线时间
    backends: HashMap<String, ConnectedBackend>,
}

struct ConnectedBackend {
    ws_tx: mpsc::Sender<WsMessage>,
    capabilities: Vec<ExecutorInfo>,
    connected_at: DateTime,
    last_heartbeat: DateTime,
}
```

**职责：**
- 维护在线本机列表
- 路由命令到正确的本机
- 检测心跳超时（60s 无响应 → 标记 offline）
- 转发本机上报的 SessionNotification 到对应的前端事件流

### R4: 执行中继逻辑（第三方 Agent）

当前端请求执行第三方 Agent Task 时：

```
POST /api/tasks/{id}/start  (executor != PI_AGENT)
  → 从数据库读取 Task → Task.workspace_id → Workspace.backend_id
  → 查 BackendRegistry → 找到对应本机的 WS 通道
  → 发送 command.prompt（携带 session_id、prompt、executor_config、workspace_root、working_dir）
  → 本机执行，通过 WS 回传 event.session_notification 流
  → 云端缓存每条 notification（用于 Resume）
  → 同时推送到前端 SSE/NDJSON 流
```

**错误处理：**
- 目标本机不在线 → 返回 `503 Backend Offline`
- 本机执行失败 → event 中携带错误信息
- WS 连接中断 → Task 状态标记为 interrupted，等本机重连后可恢复

### R4.1: PiAgent 云端执行引擎

当前端请求执行 PiAgent Task 时：

```
POST /api/tasks/{id}/start  (executor == PI_AGENT)
  → 从数据库读取 Task → 加载 Story 上下文 + Injection 规则
  → 在云端启动 PiAgent AgentLoop
  → AgentLoop 调用 LLM API（云端直连）
  → LLM 返回 tool_call → 通过 BackendRegistry 路由到目标本机
    → command.tool.file_read / file_write / shell_exec / file_list
    → 本机执行 → response.tool.* 返回结果
    → 将 tool result 送回 LLM 继续对话
  → AgentLoop 直接产生 SessionNotification（不经过 WS 中继）
  → 推送到前端 SSE/NDJSON 流
```

**PiAgent 特有能力：**
- 直接访问云端 DB 获取 Story/Task/Context/Injection 等完整上下文
- tool call 可携带不同 `backend_id`，实现跨本机操作
- 可作为编排层的"Agent PM"角色，参与 Task 拆解和调度

**新增核心组件：**
- `PiAgentLoop` — AgentLoop 主循环（LLM 对话 → tool call → result → 继续）
- `CloudContextProvider` — 从云端 DB 加载 PiAgent 所需的完整上下文
- `ToolCallRouter` — 将 PiAgent tool call 路由到目标本机的 BackendRegistry

### R5: MCP Server（从现有代码迁移）

将现有 `agentdash-mcp` 直接迁移到云端：
- `McpServices` 持有云端的 Repository 实例
- MCP 路由挂载在云端路由器上
- 无需本机参与（MCP 操作的都是云端数据）

### R6: 事件流缓存与 Resume

云端需要缓存从本机收到的 ACP SessionNotification：

**缓存策略：**
- 每条 notification 写入数据库或内存缓冲（按 session_id 分组）
- 前端通过 `GET /api/acp/sessions/{id}/stream` 连接时，先回放历史，再实时跟踪
- Resume 语义与现有 SSE Last-Event-ID 机制对齐

### R7: Backend 注册与管理 API

**新增/增强 API：**
- `POST /api/backends/register` — 生成 backend_id + auth_token，返回给操作者
- `GET /api/backends` — 列表中包含 `online` 状态字段
- `GET /api/backends/{id}` — 包含在线状态、已连接时间、能力列表
- `DELETE /api/backends/{id}` — 吊销 token + 断开 WS

## Acceptance Criteria

- [ ] `agentdash-cloud` 可独立启动，监听端口并服务前端
- [ ] 前端可通过云端 API 完成 Project/Story/Task 全生命周期操作
- [ ] 云端 WebSocket 端点可接受本机连接并完成注册
- [ ] `POST /api/tasks/{id}/start` 能中继第三方 Agent 到在线本机并返回执行流
- [ ] `POST /api/tasks/{id}/start` 能在云端启动 PiAgent AgentLoop 并路由 tool call 到本机
- [ ] 前端事件流（SSE/NDJSON）能实时收到第三方 Agent 回传的输出和 PiAgent 云端产出
- [ ] 本机断线时，BackendRegistry 正确标记 offline
- [ ] MCP Server 可正常服务外部工具调用
- [ ] Backend 注册 API 可生成 token 供本机使用

## Definition of Done

- 代码通过 lint 和 typecheck
- 前端指向云端后可完成基本的 Story → Task → 执行流程（需 Task 2 的本机配合）
- 文档已更新

## Out of Scope

- 不实现本机侧逻辑（由 Task 2 负责）
- 不迁移数据库到 PostgreSQL（初期可继续用 SQLite，后续升级）
- 不实现用户登录/注册（预研阶段无多用户需求）
- 不实现前端改动（需要时另开 Task）
- 不实现负载均衡（单一云端实例）

## Technical Notes

### 代码迁移清单

| 源文件 | 动作 | 说明 |
|--------|------|------|
| `routes/projects.rs` | 直接复用 | 云端自身处理 |
| `routes/workspaces.rs` | 拆分 | 元数据 CRUD 留云端，物理操作中继到本机 |
| `routes/stories.rs` | 直接复用 | 云端自身处理 |
| `routes/task_execution.rs` | 重写 | 改为通过 WS 中继到本机 |
| `routes/acp_sessions.rs` | 拆分 | CRUD 留云端，prompt/stream 中继到本机 |
| `routes/backends.rs` | 增强 | 添加注册 API、在线状态 |
| `routes/settings.rs` | 直接复用 | 云端自身处理 |
| `routes/discovery.rs` | 重写 | 聚合所有在线本机的能力 |
| `stream.rs` | 直接复用 + 增强 | 增加 WS notification 转发 |
| `agentdash-mcp/` | 直接复用 | 挂载在云端 |
| `agentdash-agent/` | 完整复用 | **PiAgent AgentLoop（从本机移至云端）** |
| `agentdash-domain/` | 完整复用 | 云端的核心领域层 |
| `agentdash-infrastructure/` | 完整复用 | 云端的存储实现 |

### 新增核心组件

```
crates/agentdash-cloud/
├── Cargo.toml
├── src/
│   ├── main.rs            # 入口
│   ├── app_state.rs       # 云端应用状态（含 BackendRegistry）
│   ├── backend_registry.rs # 在线本机管理
│   ├── relay.rs           # 命令中继逻辑
│   ├── ws_handler.rs      # WebSocket 端点处理
│   ├── pi_agent_loop.rs   # PiAgent AgentLoop 主循环
│   ├── cloud_context.rs   # 云端上下文加载器
│   ├── tool_call_router.rs # PiAgent tool call 路由
│   └── routes.rs          # 路由注册（复用 + 新增）
```

### BackendRegistry 与 ExecutorHub 的关系

**现在（单体）：** `ExecutorHub` 直接持有 `AgentConnector`，调用本地执行。
**重构后（云端）：** 双轨执行模型：
- **第三方 Agent**：`BackendRegistry` + `RelayConnector` 通过 WS 中继到本机
- **PiAgent**：`PiAgentLoop` 在云端直接运行，tool call 通过 `ToolCallRouter` → `BackendRegistry` 路由到本机
- 两类 Agent 的 `SessionNotification` 统一缓存和转发到前端

## Dependencies

- **前置**: Task 1（通信协议设计）— 需要消息格式定义
- **并行**: 可与 Task 2 并行开发（共享 `agentdash-relay` crate）
- **集成测试**: Task 2 + Task 3 均完成后进行端到端集成测试
