# Agent 层自定义 MCP 连接：完整三种 transport 前后端支持

## 目标

`AgentPreset` 通过 `config["mcp_servers"]` 声明 Agent 执行时可用的自定义 MCP 连接。
目前只有云端原生 Agent（PiAgent）的 Http 路径部分可用，且解析极为草率（只取 `name`+`url`，
丢掉了 headers、SSE、Stdio 三种场景）。本任务目标：

**前后端完整支持 Http / SSE / Stdio 三种 transport 的 MCP server 配置，并打通 relay 路径的透传。**

---

## 类型体系（已存在，需要正确使用）

`agent_client_protocol` crate（v0.9.4）已定义完整的类型：

```rust
pub enum McpServer {
    Http(McpServerHttp),
    Sse(McpServerSse),
    #[serde(untagged)]
    Stdio(McpServerStdio),
}

pub struct McpServerHttp {
    pub name: String,
    pub url: String,
    pub headers: Vec<HttpHeader>,   // ← 已存在，当前解析完全忽略
    pub meta: Option<Meta>,
}

pub struct McpServerSse {           // ← 当前完全不支持
    pub name: String,
    pub url: String,
    pub headers: Vec<HttpHeader>,
    pub meta: Option<Meta>,
}

pub struct McpServerStdio {         // ← 当前完全不支持
    pub name: String,
    pub command: PathBuf,
    pub args: Vec<String>,
    pub env: Vec<EnvVariable>,      // ← 环境变量
    pub meta: Option<Meta>,
}

pub struct HttpHeader { pub name: String, pub value: String }
pub struct EnvVariable { pub name: String, pub value: String }
```

---

## 当前问题定位

### 1. `build_preset_bridge` 解析草率（`project_agents.rs:527-543`）

```rust
// 当前代码：只解析 name + url，Http only，无 headers，无 SSE，无 Stdio
.filter_map(|entry| {
    let name = entry.get("name")?.as_str()?.to_string();
    let url  = entry.get("url")?.as_str()?.to_string();
    Some(McpServer::Http(McpServerHttp::new(name, url)))  // headers 被丢弃
})
```

需要改为：按 `type` 字段分发，完整解析所有字段。

### 2. relay 路径硬编码丢弃（`task_execution_gateway.rs:1369`）

```rust
mcp_servers: vec![],   // built.mcp_servers 未透传
```

第三方 Agent（Claude Code 等）走 relay 执行时，即使 preset 有 MCP 声明也收不到。

### 3. local 端硬编码丢弃（`command_handler.rs:164`）

```rust
mcp_servers: vec![],   // payload.mcp_servers 被忽略
```

relay 协议中的 `CommandPromptPayload.mcp_servers` 到 local 端直接丢弃。

### 4. Stdio 类型在 relay 路径的特殊性

Http / SSE 类型：Agent 直连目标地址，云端直接把 `McpServer::Http/Sse` 注入 `ExecutionContext` 即可。

Stdio 类型：需要在 **local backend** 拉起子进程，无法在云端直连。因此：
- 云端：把 Stdio 描述序列化后放入 `CommandPromptPayload.mcp_servers`（已有 `Vec<serde_json::Value>`）
- local：收到后拉起子进程，构造本地 McpServer 连接，注入到 Agent

---

## 需要做的事

### 后端

**1. 完整解析 `AgentPreset.config["mcp_servers"]`**

`crates/agentdash-api/src/routes/project_agents.rs:527-543`

重写 `build_preset_bridge` 中的解析逻辑：

```
type: "http" 或有 url 字段且无 command 字段
  → McpServer::Http { name, url, headers: Vec<HttpHeader> }

type: "sse"
  → McpServer::Sse { name, url, headers: Vec<HttpHeader> }

type: "stdio" 或有 command 字段
  → 不直接转 McpServer（需要在 local 拉起）
  → 单独存为 Vec<StdioMcpDecl>，在 relay 路径序列化传递
```

`preset_mcp_servers` 拆分为两个字段（或用枚举统一）：
- `http_sse_servers: Vec<McpServer>` — Http/SSE 类型，直接注入 ExecutionContext
- `stdio_decls: Vec<StdioMcpDecl>` — Stdio 类型，需要 relay 传递到 local 拉起

或保持单一 `Vec<McpServer>` 并添加 relay 序列化辅助函数（Http/SSE 直传，Stdio 序列化后转 relay 格式）。

**2. 打通 relay 透传**

`crates/agentdash-api/src/bootstrap/task_execution_gateway.rs:1361-1370`

`relay_start_prompt` 构建 `CommandPromptPayload` 时：

```rust
mcp_servers: mcp_servers_to_relay_json(&built.mcp_servers),
// Http/SSE → { type: "http"/"sse", name, url, headers: [...] }
// Stdio    → { type: "stdio", name, command, args, env: [...] }
```

**3. local 端：读取并处理 mcp_servers**

`crates/agentdash-local/src/command_handler.rs:152-168`

```rust
let (resolved_mcp_servers, _cleanup_handles) =
    resolve_mcp_servers(&payload.mcp_servers).await;

let req = PromptSessionRequest {
    // ...
    mcp_servers: resolved_mcp_servers,
    // ...
};
```

`resolve_mcp_servers` 函数：
- `type: "http"` / `type: "sse"` → 直接转为 `McpServer::Http/Sse`（含 headers）
- `type: "stdio"` → `tokio::process::Command` 拉起子进程，stdio pipe，构造 `McpServer::Stdio`
- 返回 `(Vec<McpServer>, Vec<ChildProcessHandle>)`，handle 绑定 session 生命周期

**4. relay 协议类型定义（可选增强）**

`crates/agentdash-relay/src/protocol.rs:379`

`CommandPromptPayload.mcp_servers: Vec<serde_json::Value>` 可升级为强类型：

```rust
pub mcp_servers: Vec<RelayMcpServer>,

pub enum RelayMcpServer {
    Http { name: String, url: String, headers: Vec<(String, String)> },
    Sse  { name: String, url: String, headers: Vec<(String, String)> },
    Stdio { name: String, command: String, args: Vec<String>, env: Vec<(String, String)> },
}
```

backward compat：serde untagged 或 `#[serde(try_from = "serde_json::Value")]`。

### 前端

**5. 前端 Agent 预设配置 UI**

`AgentPreset.config` 目前是 `serde_json::Value`，前端侧需要给 `mcp_servers` 字段提供结构化编辑 UI：

- `frontend/src/types/index.ts` — 新增 `McpServerDecl` 类型定义：
  ```ts
  type McpServerDecl =
    | { type: 'http'; name: string; url: string; headers?: { name: string; value: string }[] }
    | { type: 'sse';  name: string; url: string; headers?: { name: string; value: string }[] }
    | { type: 'stdio'; name: string; command: string; args?: string[]; env?: { name: string; value: string }[] }
  ```

- Project 设置页的 Agent 预设配置区域（当前可能是纯 JSON 编辑）：
  - 为每个预设提供 `mcp_servers` 列表的结构化编辑界面
  - 每条 MCP server 可选 Http / SSE / Stdio 三种类型
  - Http/SSE：name、url、headers 列表（key-value 行）
  - Stdio：name、command、args 列表、env 列表（key-value 行）
  - 支持增/删/排序

- Session 页 / Task 详情页：只读展示当前 Agent 绑定的 MCP server 列表（type badge + name + endpoint）

---

## `AgentPreset.config["mcp_servers"]` 声明格式

统一 JSON schema（前后端共用）：

```json
[
  {
    "type": "http",
    "name": "github-mcp",
    "url": "https://api.github.com/mcp",
    "headers": [
      { "name": "Authorization", "value": "Bearer ${GITHUB_TOKEN}" }
    ]
  },
  {
    "type": "sse",
    "name": "custom-sse-mcp",
    "url": "https://my-server.com/mcp/sse",
    "headers": []
  },
  {
    "type": "stdio",
    "name": "sqlite-mcp",
    "command": "uvx",
    "args": ["mcp-server-sqlite", "--db-path", "/data/app.db"],
    "env": [
      { "name": "LOG_LEVEL", "value": "debug" }
    ]
  }
]
```

向后兼容：无 `type` 字段时，有 `url` 视为 `http`，有 `command` 视为 `stdio`。

---

## 关键文件

**后端：**
- `crates/agentdash-api/src/routes/project_agents.rs:527-543` — `build_preset_bridge` 解析（当前草率实现）
- `crates/agentdash-api/src/bootstrap/task_execution_gateway.rs:1361-1370` — relay 路径硬编码 `vec![]`
- `crates/agentdash-local/src/command_handler.rs:152-168` — local 端硬编码 `vec![]`
- `crates/agentdash-relay/src/protocol.rs:379` — `CommandPromptPayload.mcp_servers`
- `third_party/agent-client-protocol/src/agent.rs:2477+` — 类型定义参考（实际用 crates.io v0.9.4）

**前端：**
- `frontend/src/types/index.ts` — 新增 `McpServerDecl` 类型
- Project 设置页 Agent 预设配置区域（具体文件待确认）
- Session / Task 详情页 MCP server 只读展示

---

## Acceptance Criteria

- [ ] Http 类型：`headers` 字段被正确解析和透传，Agent 收到带 header 的 MCP 连接
- [ ] SSE 类型：完整支持，含 headers
- [ ] Stdio 类型：local backend 正确拉起子进程，Agent session 期间可用
- [ ] 第三方 Agent（relay 路径）能收到所有三种类型的 MCP 连接
- [ ] local 端不再丢弃 `payload.mcp_servers`
- [ ] 向后兼容：旧格式 `{ name, url }` 仍正常解析为 Http 类型
- [ ] 前端：可在 Agent 预设配置中结构化编辑 MCP server 列表（三种类型）
- [ ] 前端：Session / Task 页可只读展示当前可用的 MCP server 列表
- [ ] Stdio MCP 子进程在 session 结束时被清理
