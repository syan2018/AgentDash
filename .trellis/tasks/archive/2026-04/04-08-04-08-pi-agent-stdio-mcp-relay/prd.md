# 云端 Pi Agent MCP Relay 信道

## 背景

Pi Agent 的 AgentLoop 运行在云端（agentdash-cloud）进程内。Agent 会话的 MCP 配置支持多种类型
（stdio / http / sse），但存在两类云端无法直达的情况：

1. **stdio 类型**：需要 spawn 本地进程，云端没有执行本地进程的机能
2. **localhost HTTP/SSE**：MCP server 运行在用户本机，云端网络无法触达 `localhost:xxx`

本机（agentdash-local）已有完整的 relay 通信信道（WebSocket 到云端），且已经按照
"云端命令 → 本机执行 → 结果返回"的模式处理 file_read、shell_exec 等工具调用。

**目标**：在现有 relay 体系上扩展通用 MCP relay 能力，使云端 Pi Agent 能透明地使用
本机侧 MCP server 提供的工具——无论其 transport 类型。

---

## 断裂现状

| 层次 | 文件 | 状态 |
|------|------|------|
| 配置解析（云端） | `project_agents.rs:parse_preset_mcp_servers()` | ✅ 正确解析所有类型 |
| relay 传输解析（本机） | `command_handler.rs:parse_relay_mcp_servers()` | ✅ 解析所有类型 |
| Pi Agent 工具发现 | `pi_agent_mcp.rs:discover_mcp_tools()` | ❌ 非 HTTP 类型直接 skip |
| 本机 MCP client 管理 | 无 | ❌ 不存在 |
| relay 协议 MCP 命令 | `relay/protocol.rs` | ❌ 无 MCP 相关消息 |
| backend MCP 能力上报 | 无 | ❌ 不存在 |

---

## 需求

### R1：MCP 配置增加 relay 标记

MCP server 配置新增 `relay: bool` 字段，控制该 server 是否通过本机 relay 执行：

- `stdio` 类型：**默认 true**（stdio 只能在本机执行）
- `http` / `sse` 类型：**默认 false**（假定公网可达），用户可显式设为 true
- 配置层面在 `parse_preset_mcp_servers()` 中解析，`McpServer` 增加 `relay()` 访问方法

路由判断统一用 `server.relay()` 而非 transport 类型。

### R2：Backend MCP 能力上报

Backend 连接时（或能力变更时）主动上报自身可提供的 MCP server 列表：

**上报时机**：
- backend WebSocket 握手成功后的 capabilities 上报阶段
- backend 本地 MCP 配置变更时（通过现有 `EventCapabilitiesChanged` 通道）

**上报内容**（扩展 `CapabilitiesPayload`）：
```rust
pub struct CapabilitiesPayload {
    // 现有字段...
    /// 该 backend 实例可提供的 MCP server 列表
    pub mcp_servers: Vec<McpServerCapability>,
}

pub struct McpServerCapability {
    /// server 标识名（与 agent 配置中的 name 匹配）
    pub name: String,
    /// transport 类型提示（stdio / http / sse）
    pub transport: String,
}
```

**云端侧维护映射**：`BackendRegistry` 内部建立 `HashMap<server_name, Vec<backend_id>>`，
随 backend 上下线自动更新。一个 server_name 可能存在于多个 backend（多台开发机装了同一工具）。

### R2-附录：本机 MCP 配置与上报实现细节

#### 配置来源

本机读取 `{accessible_roots[0]}/.agentdash/mcp-servers.json`：

```jsonc
{
  "servers": [
    {
      "name": "my-code-tool",
      "transport": "stdio",
      "command": "node",
      "args": ["./mcp-server.js"],
      "env": [{ "name": "DEBUG", "value": "1" }]
    },
    {
      "name": "local-api",
      "transport": "http",
      "url": "http://localhost:8080/mcp"
    }
  ]
}
```

- 文件不存在 / 解析失败时视为空列表，不阻塞启动
- MVP 阶段启动时读一次即可；后续可加 file watcher + 触发 `EventCapabilitiesChanged`

#### 上报链路（本机侧）

```
main.rs 启动
  → 读取 mcp-servers.json
  → 生成 Vec<McpServerCapability>
  → 传入 build_capabilities()
       CapabilitiesPayload {
         executors: handler.list_executors(),  // 现有
         mcp_servers: mcp_caps,               // 新增
         supports_cancel: true,
         supports_discover_options: false,
       }
  → 随 Register 消息上报
```

构建 `McpServerCapability` 只需 name + transport，不上报 command/args/url 等敏感细节
（这些信息在实际调用时由 `mcp_list_tools` 的 `server_config` 携带，或本机侧自行从配置文件读取）。

#### 消费链路（云端侧）

```
BackendRegistry
  ├── try_register() 时：遍历 capabilities.mcp_servers，写入反向索引
  │     mcp_index: HashMap<server_name, Vec<backend_id>>
  │
  ├── unregister() 时：清理该 backend_id 的所有索引条目
  │
  └── handle EventCapabilitiesChanged 时：
        ⚠ 当前只打了 log，需要补上：
        1. 更新 ConnectedBackend.capabilities
        2. 重建该 backend 的索引条目
```

`BackendRegistry` 新增公开方法：
```rust
/// 查询哪些在线 backend 提供了指定 server_name
pub async fn backends_for_mcp_server(&self, server_name: &str) -> Vec<String>;
```

供 `RelayMcpProvider`（R6）路由时调用。

### R3：Relay 协议扩展

在 `RelayMessage` 枚举中新增：

```
command.mcp_list_tools   云端→本机
  payload: { session_key, server_name, server_config: McpServerConfig }

command.mcp_call_tool    云端→本机
  payload: { session_key, server_name, tool_name, arguments: Option<JsonObject> }

command.mcp_close        云端→本机（可选，会话结束清理）
  payload: { session_key, server_name }

response.mcp_list_tools  本机→云端
  payload: { tools: Vec<McpToolDescriptor> } | error

response.mcp_call_tool   本机→云端
  payload: { content: Vec<McpContent>, is_error: bool } | error
```

`session_key` = 会话 ID，用于将同一会话内的多次调用绑定到同一个 MCP client 实例。

`McpServerConfig` 是通用描述，携带 transport 类型 + 连接参数（stdio 的 command/args/env，
http 的 url，等等），本机侧据此决定如何连接。

### R4：本机 McpClientManager

新增 `crates/agentdash-local/src/mcp_client_manager.rs`：

- 维护 `HashMap<(session_key, server_name), McpClientEntry>`
- `McpClientEntry` 内部根据 transport 类型持有不同的 client：
  - **stdio**：spawn 子进程，使用 rmcp stdio transport
  - **http/sse**：作为 HTTP client 连接 `localhost:xxx`
- 收到 `mcp_list_tools` 时：若 client 未建立则按 `server_config` 创建；列举工具
- 收到 `mcp_call_tool` 时：通过已有 client 调用工具
- relay 断连时 / `mcp_close` 时：关闭所有 client（stdio 进程 kill、HTTP 连接 drop）
- client 异常时：从 map 中移除，下次请求时重新创建

`CommandHandler` 持有 `McpClientManager`，新增对应 handle 方法。

**能力上报**：`McpClientManager` 启动时扫描本地 MCP 配置，生成 `Vec<McpServerCapability>`
供 capabilities 上报使用。配置变更时触发重新上报。

### R5：McpRelayProvider SPI trait

在 `agentdash-spi` 中定义：

```rust
#[async_trait]
pub trait McpRelayProvider: Send + Sync {
    /// 对标记为 relay 的 MCP server，通过 relay 信道转发到本机，
    /// 返回可调用的 DynAgentTool 列表。
    ///
    /// - `user_id`：发起请求的用户标识，用于 backend 路由。
    /// - `session_id`：会话 ID，用于 MCP client 复用。
    async fn discover_relay_tools(
        &self,
        user_id: Option<&str>,
        session_id: &str,
        servers: &[McpServer],
    ) -> Result<Vec<DynAgentTool>, ConnectorError>;
}
```

内部只处理 `server.relay() == true` 的条目，其余忽略。

### R6：RelayMcpProvider + RelayMcpToolAdapter（云端侧适配）

**`RelayMcpProvider`** 在 `agentdash-application` 中实现 `McpRelayProvider`：
- 持有 `BackendRegistry` 引用
- 对每个 relay server，从 registry 的 server→backend 映射中查找目标 backend
- 向选中的 backend 发送 `command.mcp_list_tools` 并等待响应
- 为每个工具创建 `RelayMcpToolAdapter` 实例

**`RelayMcpToolAdapter`** 在 `agentdash-executor` 中实现 `AgentTool`：
- `execute()` 发送 `command.mcp_call_tool`，等待 `response.mcp_call_tool`，转换为 `AgentToolResult`
- 持有 `backend_id`（创建时已由 provider 解析并固定）和 relay sender 引用

### R7：Backend 路由策略

路由逻辑统一在 `RelayMcpProvider` 内部：

```
resolve_backend_for_mcp(server_name, user_id) -> Result<String>
  1. 从 BackendRegistry 的 server→backend 映射中查找提供该 server_name 的 backend 列表
  2. 若列表非空：
     a. 若 user_id 有值，优先选择 owner_user_id == user_id 的 backend
     b. 多个匹配时取 connected_at 最近的
     c. 无 owner 匹配则取列表中任意一个
  3. 若映射中无该 server_name（backend 未上报此能力）：
     → 回退到 owner 匹配策略（兼容未升级的 backend）
  4. 无在线 backend → 返回 ConnectorError
```

解析结果按 `(session_id, server_name)` 缓存，避免同一会话内路由漂移。
缓存的 backend 下线时自动清除、下次调用重解析。

### R8：PiAgentConnector 集成

在 `PiAgentConnector` 中：
- 新增 `mcp_relay: Option<Arc<dyn McpRelayProvider>>` 字段及 setter
- `prompt()` 构建工具时（`is_new_agent` 分支）：
  - 现有 `discover_mcp_tools()` 只处理 `server.relay() == false` 的 HTTP/SSE server
  - 新增 `mcp_relay.discover_relay_tools()` 处理 `server.relay() == true` 的 server
  - 两者结果合并后 `agent.set_tools()`
- 若未注入 `mcp_relay` 且存在 relay server，记录 warn 日志

### R9：BackendConfig 增加归属标识

当前 `BackendConfig` 不携带 owner 信息，需要扩展：
```rust
pub struct BackendConfig {
    // 现有字段...
    pub owner_user_id: Option<String>,
}
```

写入时机：backend 注册 API（`POST /backends`），该接口已有 `CurrentUser`。
DB migration：`backend_config` 表新增 `owner_user_id TEXT` 列（nullable）。

---

## 验收标准

- [ ] 云端 Pi Agent 会话配置 stdio MCP server 后，工具列表中能看到 MCP 工具
- [ ] 配置 `relay: true` 的 HTTP MCP server（localhost）同样能正常发现和调用
- [ ] 调用工具时，relay 链路正确路由到上报了该 server 的 backend
- [ ] 同一 session 多次调用同一 server 的工具时，client 只创建一次（复用）
- [ ] relay 断连时本机 MCP client 被清理（stdio 无孤儿进程，HTTP 连接关闭）
- [ ] `relay: false` 的公网 HTTP MCP server 不受影响（云端直连）
- [ ] 多 backend 场景下，路由优先选择上报了对应 server 能力的 backend
- [ ] backend 未上报能力时，回退到 owner 匹配策略
- [ ] 无在线 backend 时返回清晰错误而非 panic

---

## 技术约束

- 云端代码不能直接访问本地文件系统或 spawn 进程
- 本机代码不直接读写业务数据库
- 新 relay 消息需向后兼容（旧版本本机收到未知命令时应忽略并返回 error，不崩溃）
- 能力上报需向后兼容（旧版本 backend 不上报 mcp_servers 字段，云端视为空列表）

---

## 待定问题

1. **生命周期边界**：session 结束时是否主动发 `command.mcp_close`？
   还是依赖 relay 断连做兜底清理即可？建议优先实现兜底清理，`mcp_close` 作为可选优化。

2. ~~**本机 MCP 配置来源**~~ → **已决策**：本机独立配置文件（`.agentdash/mcp-servers.json`），
   保持本机自治。详见 R2-附录。

---

## 影响范围

| Crate | 变更类型 |
|-------|---------|
| `agent_client_protocol` | `McpServer` 增加 `relay` 字段 / 访问方法 |
| `agentdash-domain` | `BackendConfig` 新增 `owner_user_id` 字段 + DB migration |
| `agentdash-relay` | 新增 5 个 RelayMessage 变体 + McpServerConfig 类型 |
| `agentdash-spi` | 新增 McpRelayProvider trait |
| `agentdash-local` | 新增 McpClientManager，扩展 CommandHandler + 能力上报 |
| `agentdash-executor` | 新增 RelayMcpToolAdapter，扩展 PiAgentConnector |
| `agentdash-application` | 新增 RelayMcpProvider 实现（含路由逻辑） |
| `agentdash-api` | backends 注册 API 写 owner_user_id；CapabilitiesPayload 扩展；构建时注入 McpRelayProvider |
