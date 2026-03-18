# AgentDash Relay Protocol — WebSocket 通信协议规范

> **版本**: v0.2  
> **状态**: 设计阶段  
> **更新**: 2026-03-10

---

## 1. 概述

AgentDash 采用**云端 + 本机**双后端架构。云端（Cloud）是数据中枢和用户入口；本机（Local）提供文件系统和第三方 Agent 执行能力。二者通过 WebSocket 长连接通信。

**关键约束：**
- WebSocket 由本机主动发起（NAT / 防火墙友好）
- 云端是服务端，暴露端口并分配鉴权
- 所有业务数据归云端所有，本机提供执行环境
- **一个本机后端 = 一台物理机器**，可管理多个工作空间目录（`accessible_roots`）

### 1.1 两类 Agent 执行模型

AgentDash 支持两类 Agent，运行位置和调度方式完全不同：

| | 云端原生 Agent（PiAgent） | 本地第三方 Agent |
|---|---|---|
| **代表** | PiAgent（AgentDash 内置） | Claude Code、Codex、AMP 等 |
| **运行位置** | 云端后端 | 本机后端（本地子进程） |
| **LLM 调用** | 云端直接发起 API 请求 | Agent 自主管理 |
| **工具执行** | 通过 Relay 路由到本机（`command.tool.*`） | Agent 自带工具链，直接操作本机 |
| **上下文** | 直接访问云端 DB（Story/Task/Context/Injection） | 仅接收 prompt 中传入的信息 |
| **多工作空间** | 支持（tool call 可路由到不同本机） | 不支持（绑定单一工作空间） |
| **调度** | 云端 AgentLoop 直接驱动 | 通过 `command.prompt` 下发到本机 |

**设计决策**：PiAgent 的 AgentLoop 运行在云端，因为：
1. 云端持有全部业务数据，PiAgent 可直接访问 Story 上下文、注入规则、编排状态，无需中继
2. PiAgent 的 tool call（文件读写、Shell 执行）可通过 Relay 路由到任意在线本机，天然支持多工作空间操作
3. PiAgent 可作为编排层的"Agent PM"，直接参与 Task 拆解和调度
4. 本地第三方 Agent 是不可控的外部进程，只能在本机运行；PiAgent 完全由我们定义，运行位置可选

因此 WebSocket 协议需要同时支持：
- **`command.prompt`**：云端向本机下发第三方 Agent 执行命令
- **`command.tool.*`**：云端 PiAgent 的 tool call 路由到本机执行

---

## 2. 连接生命周期

```
                   本机 (Client)                     云端 (Server)
                       │                                  │
                       │──── WebSocket Handshake ────────►│
                       │     GET /ws/backend?token=xxx    │
                       │                                  │
                       │◄──── 101 Switching Protocols ────│
                       │                                  │
                       │──── register ────────────────────►│  ← 本机上报能力
                       │◄──── register_ack ───────────────│  ← 云端确认注册
                       │                                  │
                       │          双向消息循环              │
                       │◄──── command.* ──────────────────│  ← 云端下发命令
                       │──── response.* ──────────────────►│  ← 本机返回结果
                       │──── event.* ─────────────────────►│  ← 本机主动上报
                       │                                  │
                       │◄──── ping ───────────────────────│  ← 心跳
                       │──── pong ────────────────────────►│
                       │                                  │
                       │──── close ───────────────────────►│  ← 优雅断开
```

### 2.1 连接建立

本机启动时通过 CLI 参数指定云端地址：

```bash
agentdash-local --cloud-url wss://cloud.example.com/ws/backend --token <auth-token>
```

**WebSocket 握手**：token 通过 query parameter 传递。

```
GET /ws/backend?token=<auth-token> HTTP/1.1
Upgrade: websocket
Connection: Upgrade
```

云端验证 token，验证失败（缺少 token / token 无效 / token 绑定异常）直接返回 `401 Unauthorized`，验证成功后才升级为 WebSocket。

### 2.2 注册（Register）

连接建立后，本机发送的**第一条消息**必须是 `register`：

```json
{
  "type": "register",
  "id": "msg-001",
  "payload": {
    "backend_id": "backend-abc123",
    "name": "开发机-Alice",
    "version": "0.1.0",
    "capabilities": {
      "executors": [
        {
          "id": "CLAUDE_CODE",
          "name": "Claude Code",
          "variants": ["opus", "sonnet"],
          "available": true
        },
        {
          "id": "CODEX",
          "name": "OpenAI Codex",
          "variants": [],
          "available": false
        }
      ],
      "supports_cancel": true,
      "supports_workspace_files": true,
      "supports_discover_options": true
    },
    "accessible_roots": [
      "/home/alice/projects/my-app",
      "/home/alice/projects/another-repo"
    ]
  }
}
```

云端成功注册后返回：

```json
{
  "type": "register_ack",
  "id": "msg-001",
  "payload": {
    "backend_id": "backend-abc123",
    "status": "online",
    "server_time": 1741612800000
  }
}
```

注册失败（如 backend 已禁用、`backend_id` 与 token 绑定值不匹配、重复注册等）：

```json
{
  "type": "error",
  "id": "msg-001",
  "error": {
    "code": "FORBIDDEN",
    "message": "token 绑定 backend `backend-abc123`，不能注册为 `backend-other`"
  }
}
```

说明：

- 握手阶段失败：直接返回 HTTP `401`，不会进入 WebSocket 注册阶段
- 注册阶段失败：云端返回一条 `type = "error"` 消息后关闭连接

### 2.3 心跳

云端每 30 秒发送 `ping`，本机需在 10 秒内回复 `pong`。
连续 2 次未收到 pong，云端标记本机为 `offline`。

```json
// 云端 → 本机
{ "type": "ping", "id": "hb-042", "payload": { "server_time": 1741612830000 } }

// 本机 → 云端
{ "type": "pong", "id": "hb-042", "payload": { "client_time": 1741612830015 } }
```

### 2.4 断线重连

本机断线后使用指数退避重连：

| 重试次数 | 等待时间 | 说明 |
|---------|---------|------|
| 1 | 1s | 立即重试 |
| 2 | 2s | |
| 3 | 4s | |
| 4 | 8s | |
| 5 | 16s | |
| 6 | 32s | |
| 7+ | 60s | 上限 |

重连成功后需重新发送 `register` 消息。云端收到已知 backend_id 的重新注册时，恢复其 `online` 状态。

---

## 3. 消息格式

### 3.1 通用信封（Envelope）

所有消息遵循统一信封格式：

```typescript
interface RelayMessage {
  type: string;           // 消息类型（点号分隔的层级命名）
  id: string;             // 消息 ID（用于 request-response 配对）
  payload: object;        // 消息体
  error?: RelayError;     // 仅在错误响应中出现
}

interface RelayError {
  code: string;           // 机器可读的错误码
  message: string;        // 人类可读的错误描述
}
```

**消息 ID 约定：**
- 请求方生成唯一 ID（推荐格式：`cmd-<timestamp>-<random>`）
- 响应方在 `id` 字段中回传相同 ID
- 事件（event）使用自身生成的唯一 ID

### 3.2 消息分类

| 方向 | 前缀 | 语义 | 是否需要响应 |
|------|------|------|-------------|
| 云端 → 本机 | `command.*` | 命令请求 | 是，本机返回 `response.*` |
| 云端 → 本机 | `ping` | 心跳探测 | 是，本机返回 `pong` |
| 本机 → 云端 | `response.*` | 命令响应 | 否 |
| 本机 → 云端 | `event.*` | 主动事件 | 否 |
| 本机 → 云端 | `register` | 注册请求 | 是，云端返回 `register_ack` |
| 任一方 | `error` | 错误通知 | 否 |

---

## 4. 命令详细定义（云端 → 本机）

### 4.1 `command.prompt` — 执行 Agent Prompt

触发本机的 ExecutorHub 开始一次 prompt 执行。

**请求：**
```json
{
  "type": "command.prompt",
  "id": "cmd-1741612800000-a1b2",
  "payload": {
    "session_id": "sess-17416-abcd1234",
    "follow_up_session_id": null,
    "prompt": "实现用户登录功能",
    "prompt_blocks": null,
    "workspace_root": "/home/alice/projects/my-app",
    "working_dir": "src/auth",
    "env": { "NODE_ENV": "development" },
    "executor_config": {
      "executor": "CLAUDE_CODE",
      "variant": "sonnet",
      "model_id": null,
      "permission_policy": "auto-edit"
    },
    "mcp_servers": []
  }
}
```

**字段说明：**

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `session_id` | string | 是 | ACP 会话 ID（云端生成） |
| `follow_up_session_id` | string? | 否 | 底层执行器的续跑 session ID |
| `prompt` | string? | 二选一 | 纯文本 prompt |
| `prompt_blocks` | ContentBlock[]? | 二选一 | ACP ContentBlock 数组 |
| `workspace_root` | string | 是 | 目标工作空间的绝对路径（云端从 `Workspace.container_ref` 获取） |
| `working_dir` | string? | 否 | 相对于 `workspace_root` 的子目录 |
| `env` | Map<string,string> | 否 | 额外环境变量 |
| `executor_config` | object | 否 | 执行器配置，不传则使用默认 |
| `mcp_servers` | McpServer[] | 否 | ACP per-session MCP 列表 |

**成功响应：**
```json
{
  "type": "response.prompt",
  "id": "cmd-1741612800000-a1b2",
  "payload": {
    "turn_id": "t1741612800123",
    "status": "started"
  }
}
```

**错误响应：**
```json
{
  "type": "response.prompt",
  "id": "cmd-1741612800000-a1b2",
  "error": {
    "code": "SESSION_BUSY",
    "message": "该会话有正在执行的 prompt"
  }
}
```

**执行输出**通过 `event.session_notification` 实时上报（见 §5.2）。

### 4.2 `command.cancel` — 取消执行

```json
{
  "type": "command.cancel",
  "id": "cmd-1741612801000-c3d4",
  "payload": {
    "session_id": "sess-17416-abcd1234"
  }
}
```

**响应：**
```json
{
  "type": "response.cancel",
  "id": "cmd-1741612801000-c3d4",
  "payload": { "status": "cancelled" }
}
```

### 4.3 `command.discover` — 查询本机能力

请求本机刷新并返回当前可用的执行器列表。

```json
{
  "type": "command.discover",
  "id": "cmd-1741612802000-e5f6",
  "payload": {}
}
```

**响应：**
```json
{
  "type": "response.discover",
  "id": "cmd-1741612802000-e5f6",
  "payload": {
    "executors": [
      { "id": "CLAUDE_CODE", "name": "Claude Code", "variants": ["opus", "sonnet"], "available": true },
      { "id": "CODEX", "name": "OpenAI Codex", "variants": [], "available": false }
    ]
  }
}
```

### 4.4 `command.discover_options` — 查询执行器选项流

请求本机启动一个执行器选项发现流（用于前端选择器渲染）。

```json
{
  "type": "command.discover_options",
  "id": "cmd-1741612803000-g7h8",
  "payload": {
    "executor": "CLAUDE_CODE",
    "variant": null,
    "working_dir": null
  }
}
```

**响应（流式）：** 本机通过多条 `event.discover_options_patch` 事件上报 JSON Patch：

```json
{
  "type": "event.discover_options_patch",
  "id": "evt-local-001",
  "payload": {
    "request_id": "cmd-1741612803000-g7h8",
    "patch": [
      { "op": "replace", "path": "/models", "value": ["claude-4-sonnet", "claude-4-opus"] }
    ],
    "done": false
  }
}
```

流结束时 `done: true`。

### 4.5 `command.workspace_files.list` — 列出工作空间文件

```json
{
  "type": "command.workspace_files.list",
  "id": "cmd-1741612804000-i9j0",
  "payload": {
    "workspace_id": "ws-uuid-1234",
    "path": "src/",
    "pattern": "*.rs"
  }
}
```

**响应：**
```json
{
  "type": "response.workspace_files.list",
  "id": "cmd-1741612804000-i9j0",
  "payload": {
    "files": [
      { "path": "src/main.rs", "size": 1024, "modified_at": 1741612800000 },
      { "path": "src/lib.rs", "size": 512, "modified_at": 1741612700000 }
    ]
  }
}
```

### 4.6 `command.workspace_files.read` — 读取文件内容

```json
{
  "type": "command.workspace_files.read",
  "id": "cmd-1741612805000-k1l2",
  "payload": {
    "workspace_id": "ws-uuid-1234",
    "path": "src/main.rs"
  }
}
```

**响应：**
```json
{
  "type": "response.workspace_files.read",
  "id": "cmd-1741612805000-k1l2",
  "payload": {
    "path": "src/main.rs",
    "content": "fn main() { ... }",
    "encoding": "utf-8"
  }
}
```

### 4.7 `command.workspace_detect_git` — 检测 Git 信息

```json
{
  "type": "command.workspace_detect_git",
  "id": "cmd-1741612806000-m3n4",
  "payload": {
    "path": "/home/alice/projects/my-app"
  }
}
```

**响应：**
```json
{
  "type": "response.workspace_detect_git",
  "id": "cmd-1741612806000-m3n4",
  "payload": {
    "is_git": true,
    "default_branch": "main",
    "current_branch": "feature/auth",
    "remote_url": "git@github.com:alice/my-app.git"
  }
}
```

### 4.8 `command.tool.*` — PiAgent Tool Call 路由

云端 PiAgent 的 AgentLoop 运行在云端，但其工具需要在本机文件系统上执行。云端将 PiAgent 的 tool call 封装为 `command.tool.*` 下发到对应本机。

#### 4.8.1 `command.tool.file_read` — 读取文件

```json
{
  "type": "command.tool.file_read",
  "id": "cmd-1741612807000-o5p6",
  "payload": {
    "call_id": "tc-001",
    "path": "src/main.rs",
    "workspace_root": "/home/alice/projects/my-app"
  }
}
```

**响应：**
```json
{
  "type": "response.tool.file_read",
  "id": "cmd-1741612807000-o5p6",
  "payload": {
    "call_id": "tc-001",
    "content": "fn main() { ... }",
    "encoding": "utf-8"
  }
}
```

#### 4.8.2 `command.tool.file_write` — 写入文件

```json
{
  "type": "command.tool.file_write",
  "id": "cmd-1741612808000-q7r8",
  "payload": {
    "call_id": "tc-002",
    "path": "src/auth.rs",
    "content": "pub fn login() { ... }",
    "workspace_root": "/home/alice/projects/my-app"
  }
}
```

**响应：**
```json
{
  "type": "response.tool.file_write",
  "id": "cmd-1741612808000-q7r8",
  "payload": { "call_id": "tc-002", "status": "ok" }
}
```

#### 4.8.3 `command.tool.shell_exec` — 执行 Shell 命令

```json
{
  "type": "command.tool.shell_exec",
  "id": "cmd-1741612809000-s9t0",
  "payload": {
    "call_id": "tc-003",
    "command": "cargo test",
    "workspace_root": "/home/alice/projects/my-app",
    "timeout_ms": 30000
  }
}
```

**响应：**
```json
{
  "type": "response.tool.shell_exec",
  "id": "cmd-1741612809000-s9t0",
  "payload": {
    "call_id": "tc-003",
    "exit_code": 0,
    "stdout": "test result: ok. 12 passed; 0 failed",
    "stderr": ""
  }
}
```

#### 4.8.4 `command.tool.file_list` — 列出目录内容

```json
{
  "type": "command.tool.file_list",
  "id": "cmd-1741612810000-u1v2",
  "payload": {
    "call_id": "tc-004",
    "path": "src/",
    "workspace_root": "/home/alice/projects/my-app",
    "pattern": "*.rs",
    "recursive": true
  }
}
```

**响应：**
```json
{
  "type": "response.tool.file_list",
  "id": "cmd-1741612810000-u1v2",
  "payload": {
    "call_id": "tc-004",
    "entries": [
      { "path": "src/main.rs", "is_dir": false, "size": 1024 },
      { "path": "src/lib.rs", "is_dir": false, "size": 512 }
    ]
  }
}
```

> **设计要点**：
> - `command.tool.*` 与 `command.workspace_files.*` 的区别：前者是 PiAgent 的工具调用（含 `call_id`，支持写操作和 Shell 执行），后者是前端发起的只读文件浏览。底层文件 I/O 实现应共享，避免重复。
> - 所有涉及文件系统的命令都通过 `workspace_root`（绝对路径）指定目标工作空间。云端从 `Workspace.container_ref` 获取该路径，按 `Workspace.backend_id` 路由到正确的本机。
> - 后续 PiAgent 扩展新工具时，只需在 `command.tool.*` 命名空间下新增子类型。

---

## 5. 事件详细定义（本机 → 云端）

### 5.1 `event.capabilities_changed` — 能力变更

当本机安装/卸载执行器、或执行器可用性变化时主动上报。

```json
{
  "type": "event.capabilities_changed",
  "id": "evt-local-002",
  "payload": {
    "executors": [
      { "id": "CLAUDE_CODE", "name": "Claude Code", "variants": ["opus", "sonnet"], "available": true },
      { "id": "CODEX", "name": "OpenAI Codex", "variants": [], "available": true }
    ]
  }
}
```

### 5.2 `event.session_notification` — ACP 会话通知

**这是最高频的消息类型**，用于将 Agent 执行的实时输出从本机转发到云端。

消息 payload 直接包含 ACP `SessionNotification` 对象（与现有 `agentdash-executor` 产出的格式完全一致）：

```json
{
  "type": "event.session_notification",
  "id": "evt-local-003",
  "payload": {
    "session_id": "sess-17416-abcd1234",
    "notification": {
      "sessionId": "sess-17416-abcd1234",
      "update": {
        "kind": "agentMessageChunk",
        "content": {
          "type": "text",
          "text": "好的，我来帮你实现登录功能。"
        },
        "_meta": {
          "agentdash": {
            "v": 1,
            "source": {
              "connectorId": "vibe-kanban",
              "connectorType": "local_executor",
              "executorId": "CLAUDE_CODE",
              "variant": "sonnet"
            },
            "trace": {
              "turnId": "t1741612800123",
              "entryIndex": 0
            }
          }
        }
      }
    }
  }
}
```

**设计要点：**
- `notification` 字段即完整的 ACP `SessionNotification`，不做任何转换
- `_meta.agentdash` 元信息由本机的 ExecutorHub 注入，云端直接透传到前端
- 云端收到后：①缓存到 session 历史 ②转发到前端 SSE/NDJSON 流
- `session_id` 在顶层冗余一份，方便云端快速路由到正确的前端订阅者

### 5.3 `event.session_state_changed` — 会话执行状态变更

当一次 prompt 执行的状态发生关键变更时上报：

```json
{
  "type": "event.session_state_changed",
  "id": "evt-local-004",
  "payload": {
    "session_id": "sess-17416-abcd1234",
    "turn_id": "t1741612800123",
    "state": "completed",
    "message": null
  }
}
```

**`state` 取值：**

| 值 | 含义 |
|---|------|
| `started` | 本轮 prompt 开始执行 |
| `completed` | 本轮 prompt 成功完成 |
| `failed` | 本轮 prompt 执行失败 |
| `cancelled` | 本轮 prompt 被取消 |

---

## 6. 错误码定义

### 6.1 通用错误码

| 错误码 | HTTP 类比 | 说明 |
|--------|-----------|------|
| `AUTH_FAILED` | 401 | Token 无效或过期 |
| `FORBIDDEN` | 403 | 无权限执行该操作 |
| `CONFLICT` | 409 | 当前资源状态冲突（如 backend 重复在线注册） |
| `NOT_FOUND` | 404 | 目标资源不存在（session、file 等） |
| `SESSION_BUSY` | 409 | 会话正在执行，无法接受新 prompt |
| `EXECUTOR_NOT_FOUND` | 404 | 指定的执行器不存在 |
| `EXECUTOR_UNAVAILABLE` | 503 | 执行器存在但当前不可用 |
| `SPAWN_FAILED` | 500 | Agent 进程启动失败 |
| `RUNTIME_ERROR` | 500 | Agent 执行中发生运行时错误 |
| `IO_ERROR` | 500 | 文件 I/O 错误 |
| `INVALID_MESSAGE` | 400 | 消息格式错误 |
| `TIMEOUT` | 408 | 操作超时 |

### 6.2 错误消息格式

```json
{
  "type": "error",
  "id": "cmd-xxx",
  "error": {
    "code": "SESSION_BUSY",
    "message": "该会话有正在执行的 prompt，请等待完成或取消后再试"
  }
}
```

或作为 response 的一部分：

```json
{
  "type": "response.prompt",
  "id": "cmd-xxx",
  "payload": null,
  "error": {
    "code": "EXECUTOR_NOT_FOUND",
    "message": "未知执行器 'UNKNOWN_AGENT'"
  }
}
```

---

## 7. 安全性考量（预研阶段简化）

### 7.1 鉴权流程

```
操作者 → POST /api/backends/register → 获得 { backend_id, token }
       → 将 token 配置到本机启动参数
本机   → WebSocket ?token=xxx → 云端验证 → 注册为 online
```

- Token 为不透明字符串（云端生成的 UUID 或 JWT）
- 预研阶段：云端维护 `HashMap<token, backend_id>` 即可
- 生产阶段可升级为 JWT + 刷新机制

### 7.2 权限边界

本机只能：
- 响应云端下发的命令
- 上报事件（不能读写云端数据库）
- 不能冒充其他本机（token 绑定 backend_id）

---

## 8. Backend 与 Workspace 关系

### 8.1 一个 Backend = 一台机器，多个 Workspace

本机后端是 **per-machine** 的：一台物理机器（或虚拟机/容器）启动一个 `agentdash-local` 进程。该进程可访问本机文件系统中的多个目录，每个目录对应云端的一个 `Workspace` 实体。

```
本机后端 A（Alice 的开发机）
├── /home/alice/projects/frontend-app   → 云端 Workspace W1
├── /home/alice/projects/backend-api    → 云端 Workspace W2
└── /home/alice/projects/infra          → 云端 Workspace W3

本机后端 B（Bob 的 GPU 服务器）
└── /data/ml-pipeline                   → 云端 Workspace W4
```

### 8.2 注册时声明可访问路径

本机在 `register` 消息中通过 `accessible_roots` 上报自己可访问的目录列表。云端用此信息：
- 验证 Workspace 的 `container_ref` 确实可达
- 在 UI 中辅助用户选择/创建 Workspace 时显示可用目录

### 8.3 命令路由基于 Workspace

云端路由命令时，权威的路由键是 **Workspace 所在的 Backend**（通过 `Workspace.backend_id` 查询），而非 Story 或 Project 上的 `backend_id`：

```
前端 POST /tasks/{id}/start
  → 云端读取 Task.workspace_id
  → 查 Workspace.backend_id → 找到目标本机
  → 通过 WS 下发 command.prompt（携带 Workspace.container_ref 作为 workspace_root）
```

`Project.backend_id` 和 `Story.backend_id` 作为**默认偏好**使用（创建 Task/Workspace 时继承），不参与运行时路由。

### 8.4 与现有代码的差异（需实现时修正）

| 现状 | 目标 |
|------|------|
| `Workspace` 无 `backend_id` 字段 | 需新增 `backend_id: String`，标记物理文件所在本机 |
| Task 路由走 `Story.backend_id` | 改为走 `Task.workspace_id → Workspace.backend_id` |
| `Project/Story.backend_id` 是唯一路由依据 | 降级为默认偏好，仅在创建子实体时继承 |

---

## 9. 与现有系统的兼容映射

### 9.1 本地第三方 Agent — 与 `AgentConnector` trait 的映射

| AgentConnector 方法 | Relay Command | 说明 |
|---------------------|--------------|------|
| `list_executors()` | `command.discover` | 查询本机能力 |
| `discover_options_stream()` | `command.discover_options` | 选项发现流 |
| `prompt()` | `command.prompt` + `event.session_notification` 流 | 执行 + 流式输出 |
| `cancel()` | `command.cancel` | 取消执行 |

云端实现 `RelayConnector`（实现 `AgentConnector` trait），将方法调用转换为 WebSocket command/response。上层 ExecutorHub 对中继透明。

### 9.2 云端原生 PiAgent — AgentLoop 与 Tool Call 的映射

PiAgent 的 AgentLoop 运行在云端，其工具调用通过 `command.tool.*` 路由到本机：

| PiAgent Tool | Relay Command | 说明 |
|-------------|--------------|------|
| `file_read` | `command.tool.file_read` | 读取本机文件 |
| `file_write` | `command.tool.file_write` | 写入本机文件 |
| `shell_exec` | `command.tool.shell_exec` | 在本机执行命令 |
| `file_list` | `command.tool.file_list` | 列出本机目录 |
| （后续扩展） | `command.tool.<name>` | 新工具类型 |

PiAgent 的优势在于其 tool call 可以携带不同的 `backend_id`，实现跨本机操作。

### 9.3 与 `SessionNotification` 的关系

**本地第三方 Agent**：`event.session_notification.payload.notification` 直接就是 ACP `SessionNotification`。云端无需理解其内部结构，只需缓存 + 转发到前端。

**云端 PiAgent**：PiAgent 的 AgentLoop 在云端直接产生 `SessionNotification`（无需经过 WebSocket），云端直接缓存并推送到前端。

### 9.4 与 `_meta.agentdash` 的关系

- **本地第三方 Agent**：元信息由本机 ExecutorHub 注入，云端透传不修改。
- **云端 PiAgent**：元信息由云端 AgentLoop 直接注入（`source.connectorType = "cloud_native"`）。

---

## 10. 消息序列示例

### 10.1 典型 Prompt 执行流

```
本机                              云端                              前端
  │                                 │                                 │
  │◄── command.prompt ──────────────│◄── POST /tasks/{id}/start ──────│
  │                                 │                                 │
  │──► response.prompt ────────────►│                                 │
  │    { turn_id: "t123" }          │──► 200 { turn_id: "t123" } ────►│
  │                                 │                                 │
  │──► event.session_notification ──►│                                │
  │    (agent_message_chunk)         │──► SSE: session_notification ──►│
  │                                 │                                 │
  │──► event.session_notification ──►│                                │
  │    (tool_call)                   │──► SSE: session_notification ──►│
  │                                 │                                 │
  │   ... 持续流式输出 ...            │                                │
  │                                 │                                 │
  │──► event.session_state_changed ──►│                               │
  │    { state: "completed" }        │──► SSE: turn_completed ───────►│
```

### 10.2 取消执行流

```
前端                              云端                              本机
  │                                 │                                 │
  │── POST /tasks/{id}/cancel ─────►│                                │
  │                                 │──► command.cancel ─────────────►│
  │                                 │                                 │
  │                                 │◄── response.cancel ────────────│
  │◄── 200 OK ─────────────────────│                                 │
  │                                 │                                 │
  │                                 │◄── event.session_state_changed ─│
  │◄── SSE: turn_cancelled ────────│    { state: "cancelled" }       │
```

### 10.3 PiAgent 云端执行流（tool call 路由到本机）

```
前端                              云端                              本机
  │                                 │                                 │
  │── POST /tasks/{id}/start ──────►│                                 │
  │   { executor: "PI_AGENT" }      │                                 │
  │                                 │  AgentLoop 启动                  │
  │                                 │  调用 LLM API（云端直连）         │
  │                                 │                                 │
  │◄── SSE: agent_message_chunk ───│  LLM 返回 tool_call: file_read   │
  │                                 │                                 │
  │                                 │──► command.tool.file_read ──────►│
  │                                 │                                 │
  │                                 │◄── response.tool.file_read ─────│
  │                                 │    { content: "..." }            │
  │                                 │                                 │
  │                                 │  将 tool result 送回 LLM         │
  │                                 │  LLM 返回 tool_call: file_write  │
  │                                 │                                 │
  │◄── SSE: tool_call ─────────────│                                  │
  │                                 │──► command.tool.file_write ─────►│
  │                                 │                                 │
  │                                 │◄── response.tool.file_write ────│
  │                                 │                                 │
  │◄── SSE: agent_message_chunk ───│  LLM 返回最终文本                 │
  │                                 │                                 │
  │◄── SSE: turn_completed ────────│  AgentLoop 结束                   │
```

> **注意**：PiAgent 的 `SessionNotification` 由云端 AgentLoop 直接生成并推送到前端 SSE 流，**不经过** WebSocket 中继（区别于本地第三方 Agent 的流式回传）。只有 tool call 的执行通过 WebSocket 路由到本机。

### 10.4 本机断线与重连

```
本机                              云端                              前端
  │                                 │                                 │
  │ ╳ 连接断开                       │                                │
  │                                 │── 检测到 pong 超时 ──►          │
  │                                 │   标记 backend offline          │
  │                                 │   运行中 Task → interrupted     │
  │                                 │──► SSE: backend_offline ───────►│
  │                                 │                                 │
  │ (指数退避等待)                    │                                │
  │                                 │                                 │
  │──── WebSocket 重连 ────────────►│                                │
  │──── register ──────────────────►│                                │
  │                                 │   恢复 backend online           │
  │◄──── register_ack ─────────────│                                 │
  │                                 │──► SSE: backend_online ────────►│
```

---

## 11. 暂不定义（实现阶段考虑）

- [ ] **消息背压与流控**：`event.session_notification` 是最高频消息，当 Agent 产出速度远超云端转发速度（或网络抖动）时，WebSocket 缓冲可能堆积。需要定义背压策略（丢弃 / 暂停 / 缓冲上限）
- [ ] **tool call 超时与重试**：`command.tool.shell_exec` 已有 `timeout_ms`，但 `file_read/write/list` 未定义超时。如果 tool call response 因断线丢失，AgentLoop 的恢复策略待定义
- [ ] **大文件传输**：`command.tool.file_read/write` 的内容以 JSON 字符串传输，大文件（>10MB）可能导致内存和序列化性能问题。可能需要分片或 binary frame
- [ ] **accessible_roots 动态更新**：本机新增/移除工作空间目录时，是否需要重新注册或通过事件通知云端
- [ ] **安全沙箱**：`command.tool.shell_exec` 当前无执行限制，PiAgent 的 tool call 可能执行任意命令

---

*版本: v0.3*  
*更新: 2026-03-10 — 增加 §8 Backend-Workspace 关系澄清，修正 register/command 中的 workspace 路由字段，增加 §11 暂不定义事项*  
*更新: 2026-03-10 — v0.2 增加两类 Agent 执行模型、PiAgent tool call 路由协议*
