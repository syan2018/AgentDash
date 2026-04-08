# 云端 Pi Agent stdio MCP 转发信道

## 背景

Pi Agent 的 AgentLoop 运行在云端（agentdash-cloud）进程内。Agent 会话的 MCP 配置支持 stdio 类型
（通过命令行拉起本地进程），但云端没有执行本地进程的机能，因此 stdio MCP 工具目前被静默丢弃。

本机（agentdash-local）已有完整的 relay 通信信道（WebSocket 到云端），且已经按照
"云端命令 → 本机执行 → 结果返回"的模式处理 file_read、shell_exec 等工具调用。

**目标**：在现有 relay 体系上扩展，使云端 Pi Agent 能够透明地使用本机 stdio MCP server 提供的工具。

---

## 断裂现状

| 层次 | 文件 | 状态 |
|------|------|------|
| 配置解析（云端） | `project_agents.rs:parse_preset_mcp_servers()` | ✅ 正确解析 McpServer::Stdio |
| relay 传输解析（本机） | `command_handler.rs:parse_relay_mcp_servers()` | ✅ 解析 Stdio 并构造对象 |
| Pi Agent 工具发现 | `pi_agent_mcp.rs:discover_mcp_tools()` | ❌ 非 HTTP 类型直接 skip |
| 本机 stdio 进程管理 | 无 | ❌ 不存在 |
| relay 协议 MCP 命令 | `relay/protocol.rs` | ❌ 无 MCP 相关消息 |

---

## 需求

### R1：Relay 协议扩展

在 `RelayMessage` 枚举中新增：

```
command.mcp_list_tools   云端→本机
  payload: { session_key: String, server_name: String,
             command: String, args: Vec<String>, env: Vec<EnvVariable> }

command.mcp_call_tool    云端→本机
  payload: { session_key: String, server_name: String,
             tool_name: String, arguments: Option<JsonObject> }

command.mcp_close        云端→本机（可选，会话结束清理）
  payload: { session_key: String, server_name: String }

response.mcp_list_tools  本机→云端
  payload: { tools: Vec<McpToolDescriptor> } | error

response.mcp_call_tool   本机→云端
  payload: { content: Vec<McpContent>, is_error: bool } | error
```

`session_key` = 会话 ID，用于将同一会话内的多次调用绑定到同一个 stdio 进程实例。

### R2：本机 McpProcessManager

新增 `crates/agentdash-local/src/mcp_process_manager.rs`：
- 维护 `HashMap<(session_key, server_name), StdioMcpClient>`
- 收到 `mcp_list_tools` 时：若进程未运行则 spawn（用 rmcp stdio transport）；列举工具
- 收到 `mcp_call_tool` 时：通过已有 client 调用工具
- relay 断连时 / `mcp_close` 时：优雅关闭进程
- 进程崩溃时：从 map 中移除，下次请求时重新 spawn

`CommandHandler` 持有 `McpProcessManager`，新增对应 handle 方法。

### R3：McpProxyProvider SPI trait

在 `agentdash-spi` 中定义：

```rust
#[async_trait]
pub trait McpProxyProvider: Send + Sync {
    /// 对 servers 中的 Stdio 类型条目，通过 relay 转发到本机，返回可调用的 DynAgentTool 列表。
    /// 非 Stdio 类型由调用方（Pi Agent connector）的 HTTP 路径处理，此处忽略。
    ///
    /// - `user_id`：发起请求的用户标识，用于 backend owner 匹配（None 时跳过 owner 优先，
    ///   直接回退到任意在线 backend）。
    /// - `session_id`：会话 ID，用于 stdio 进程复用（同一 session + server 复用同一进程）。
    async fn discover_stdio_tools(
        &self,
        user_id: Option<&str>,
        session_id: &str,
        servers: &[McpServer],
    ) -> Result<Vec<DynAgentTool>, ConnectorError>;
}
```

> `user_id` 类型为 `Option<&str>`，对齐 `AuthIdentity.user_id: String`（`agentdash-spi/src/auth.rs:69`）。

### R4：RelayMcpProxyProvider + RelayMcpToolAdapter（云端侧适配）

**`RelayMcpProxyProvider`** 在 `agentdash-application` 中实现 `McpProxyProvider`：
- 持有 `BackendRegistry` + `BackendRepository` 引用（用于 backend 解析）
- 内部包含 `BackendResolver`（具体 struct，不需要 trait 抽象），负责运行时选择目标 backend
- 对每个 Stdio server，向选中的 backend 发送 `command.mcp_list_tools` 并等待响应
- 为每个工具创建 `RelayMcpToolAdapter` 实例

**`RelayMcpToolAdapter`** 在 `agentdash-executor` 中实现 `AgentTool`：
- `execute()` 发送 `command.mcp_call_tool`，等待 `response.mcp_call_tool`，转换为 `AgentToolResult`
- 持有 `backend_id`（创建时已由 provider 解析并固定）和 relay sender 引用

### R5：PiAgentConnector 集成

在 `PiAgentConnector` 中：
- 新增 `mcp_proxy: Option<Arc<dyn McpProxyProvider>>` 字段及 setter
- `prompt()` 构建工具时（`is_new_agent` 分支）：
  - 保留现有 `discover_mcp_tools()` 处理 HTTP server
  - 额外调用 `mcp_proxy.discover_stdio_tools()` 处理 Stdio server
  - 两者结果合并后 `agent.set_tools()`
- 若未注入 `mcp_proxy` 且存在 Stdio server，记录 warn 日志

### R6：BackendConfig 增加归属标识（前置依赖）

当前 `BackendConfig`（`agentdash-domain/src/backend/entity.rs`）只有：
`{ id, name, endpoint, auth_token, enabled, backend_type }`，不携带任何 owner 信息。

需要扩展：
```rust
pub struct BackendConfig {
    // 现有字段...
    /// 注册此后端的用户标识（None 表示共享/系统级后端）
    /// 对齐 AuthIdentity.user_id（String 类型）
    pub owner_user_id: Option<String>,
}
```

写入时机：backend **注册** API（`POST /backends`，`routes/backends.rs`），
该接口已有 `CurrentUser`，直接 `config.owner_user_id = Some(current_user.user_id.clone())`。
WS handler 侧无需改动（握手仅验证机器 auth_token，不涉及用户身份）。

> 注：此字段同时服务于未来的多租户隔离场景，不只是 MCP 路由。

### R7：运行时 Backend 解析策略

**不**在 session 创建时绑定 `backend_id`，而是在首次需要 relay MCP 调用时动态解析：

```
BackendResolver::resolve_for_mcp(user_id: Option<&str>) -> Result<String, ConnectorError>
  1. 从 BackendRegistry.list_online() 获取当前在线 backend 列表
  2. 对每个 online backend_id，通过 BackendRepository.get_backend(id) 查 owner_user_id
  3. 若 user_id 为 Some，优先选择 owner_user_id == user_id 的 backend
  4. 若有多个 owner 匹配，取 connected_at 最近的（即最后上线的那台）
  5. 若无 owner 匹配（或 user_id 为 None），回退到任意在线 backend（按 connected_at 排序取第一）
  6. 若无在线 backend，返回 ConnectorError::ConnectionFailed("无在线 backend")
```

`BackendResolver` 是 `agentdash-application` 中的具体 struct（不需要 trait 抽象），
由 `RelayMcpProxyProvider` 内部持有。

解析结果在 `RelayMcpProxyProvider` 内**按 session 缓存**（首次调用后不再重解析），
避免同一会话内工具调用路由到不同机器。若缓存的 backend 中途下线，下次调用时
自动重解析（清除缓存后重试一次）。

trait 签名见 R3（`user_id: Option<&str>` 对齐 `AuthIdentity.user_id: String`）。

调用方取值路径：`context.identity.as_ref().map(|id| id.user_id.as_str())`
（`ExecutionContext.identity: Option<AuthIdentity>`，`connector.rs:64`），无需新增字段。

---

## 验收标准

- [ ] 云端 Pi Agent 会话配置 stdio MCP server 后，工具列表中能看到 MCP 工具（命名格式 `mcp_{server}_{tool}`）
- [ ] 调用该工具时，relay 链路正确路由到对应本机，本机 spawn 进程、执行、返回结果
- [ ] 同一 session 多次调用同一 server 的工具时，进程只 spawn 一次（复用）
- [ ] relay 断连时本机 stdio 进程被清理，不产生孤儿进程
- [ ] HTTP MCP server 的现有行为不受影响
- [ ] 如果本机不在线（无 relay 连接），返回清晰错误而非 panic

---

## 技术约束

- 云端代码不能直接访问本地文件系统或 spawn 进程
- 本机代码不直接读写业务数据库
- 新 relay 消息需向后兼容（旧版本本机收到未知命令时应忽略并返回 error，不崩溃）

---

## 待定问题

1. ~~**backend_id 路由**~~ → **已决策**：运行时动态解析（见 R7），不在 session 创建时绑定。

2. ~~**多本机场景**~~ → **已决策**：优先 owner 匹配，回退任意在线（见 R7 解析策略）。

3. **生命周期边界**：session 结束时是否主动发 `command.mcp_close`？
   还是依赖 relay 断连做兜底清理即可？建议优先实现兜底清理，`mcp_close` 作为可选优化。

4. ~~**BackendConfig.owner_user_id 写入时机**~~ → **已决策**：
   backend 注册 API（`POST /backends`）写入，WS handler 无需改动（见 R6）。
   DB migration：`backend_config` 表新增 `owner_user_id TEXT` 列（nullable）。

---

## 影响范围

| Crate | 变更类型 |
|-------|---------|
| `agentdash-domain` | `BackendConfig` 新增 `owner_user_id` 字段 + DB migration |
| `agentdash-relay` | 新增 5 个 RelayMessage 变体 + 对应 payload 类型 |
| `agentdash-spi` | 新增 McpProxyProvider trait |
| `agentdash-local` | 新增 McpProcessManager，扩展 CommandHandler |
| `agentdash-executor` | 新增 RelayMcpToolAdapter，扩展 PiAgentConnector |
| `agentdash-application` | 新增 RelayMcpProxyProvider + BackendResolver 实现 |
| `agentdash-api` | backends 注册 API 写 owner_user_id；构建 PiAgentConnector 时注入 McpProxyProvider |
