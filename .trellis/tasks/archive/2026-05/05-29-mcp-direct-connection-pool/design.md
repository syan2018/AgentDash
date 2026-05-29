# executor direct MCP 连接池设计

## 背景与边界

`agentdash-executor::mcp::direct` 负责把 Session 中声明为 direct 的 HTTP MCP server 转成 Agent 可调用工具。它属于执行器的外部协议适配边界，不承载领域规则，也不改变 `CapabilityState` 的工具裁决语义。

当前 `discover_mcp_tool_entries()` 与 `McpToolAdapter::execute()` 都在调用路径中临时 `connect_http_server()`，请求结束后立即 `client.cancel().await`。这让每次工具发现和每次工具调用都重复完成 streamable-http 握手。目标是让同一批 direct MCP tools 共享按 server URL 缓存的 HTTP client，并在连接失效后剔除，让后续调用自然重连。

## DDD / 分层判断

- Domain 不依赖 MCP 协议 client、RMCP `RunningService` 或 wire DTO。
- `agentdash-executor` 是外层协议适配器，可以依赖 `agentdash-spi` 的 `SessionMcpServer` 与 `agentdash-domain` 中经 SPI re-export 的 MCP transport 值对象。
- 连接池是执行期基础设施状态，不进入 domain/application，不写入 session 持久化，也不改变 capability fact source。
- MCP tool 是否暴露仍只由 `CapabilityState` 决定；连接池只优化已允许工具的 transport 生命周期。

## 目标设计

在 `crates/agentdash-executor/src/mcp/direct.rs` 内新增 direct 专用池：

```rust
struct DirectMcpClientPool {
    clients: RwLock<HashMap<String, Arc<Mutex<RunningService<RoleClient, ()>>>>>,
}
```

核心方法：

- `ensure_client(&self, server: &McpHttpServerSpec)`：按 key 返回可用 client；若不存在或已关闭则创建新连接。
- `list_tools(&self, server: &McpHttpServerSpec)`：复用池内 client 执行 `list_all_tools()`。
- `call_tool(&self, server: &McpHttpServerSpec, request: CallToolRequestParams)`：复用池内 client 执行 `call_tool()`。
- `invalidate(&self, server: &McpHttpServerSpec)`：请求返回 `ServiceError` 后移除对应 client；被移除的 `RunningService` 由 drop guard 触发取消。

池 key 使用 server URL。原因是 direct HTTP transport 的物理连接身份由 URL 决定；server name 只影响 Agent-facing tool namespace 与 capability key。若同一 URL 以多个 session server name 暴露，复用同一连接符合 transport 事实。

## 生命周期

`discover_mcp_tool_entries()` 每次构建一个 `Arc<DirectMcpClientPool>`：

1. discovery 阶段用这个 pool 对每个 direct HTTP server 执行 `list_tools()`。
2. 为发现出的每个 `McpToolAdapter` 注入同一个 `Arc<DirectMcpClientPool>`。
3. 同一批 adapter 在后续工具调用中复用 discovery 已建立的连接。
4. 当 connector 替换工具集或 session 结束后，adapter 与 pool 一起 drop；RMCP `RunningService` 的 drop guard 负责取消后台服务。

不采用全局 static pool。原因是 direct MCP server 列表属于 session turn runtime surface，全局池会让连接生命周期脱离 session/tool set，后续还需要额外 TTL、权限隔离和关闭编排。当前任务的正确边界是“同一批工具实例复用连接”。

## 并发模型

- 池外层使用 `tokio::sync::RwLock<HashMap<...>>`，读取路径只克隆 `Arc<Mutex<RunningService<...>>>`，不在持有 map guard 时执行 MCP 请求。
- 单个 client 使用 `tokio::sync::Mutex` 串行化同一连接上的 `list_tools()` / `call_tool()`，避免同一 `RunningService` 上并发 request 与失效剔除交错。
- `ensure_client()` 在写锁内做 double-check，防止同一 URL 的并发请求重复建连。direct MCP server 数量通常较少，连接建立期间短暂串行化池写入是可接受的。

## 失效与重连策略

`RunningService::is_closed()` 为 true 时，`ensure_client()` 会移除旧 client 并重新建立连接。

`list_tools()` 或 `call_tool()` 返回 `ServiceError` 时：

- 立即 `invalidate()` 对应 URL，确保坏连接不会继续复用。
- 当前调用返回原错误，不自动重试 `call_tool()`，避免工具请求可能已经抵达 server 时产生副作用重复。
- 后续 discovery 或工具调用会通过 `ensure_client()` 建立新连接。

`list_tools()` 理论上可安全重试一次，但为了保持错误语义一致，本任务先不引入隐式 retry；验收重点是“失败后剔除，下一次可重连”。

## 与 local McpClientManager 的关系

`agentdash-local::McpClientManager` 已有懒连接池，但它的职责包含：

- stdio 进程生命周期；
- 本机 HTTP/SSE MCP server；
- local relay command 的 `list_tools` / `call_tool` / `close` 语义；
- 基于 local config 的 server name 查找。

executor direct 池只服务云端执行器中的 HTTP direct MCP adapter，key 与生命周期都由 session tool set 决定。强行合并会把 local runtime 的进程/close 语义带入 executor，或让 executor 依赖 local crate。当前只复用设计经验，不抽共享 manager。若未来 executor/local/infrastructure probe 出现第三处稳定复用，再把“HTTP worker 构造 + client pool trait”下沉到 `agentdash-mcp` 或新的 runtime transport crate。

## 验收映射

- `connect_http_server` 只保留在池的建连方法中，`execute()` / `discover_mcp_tool_entries()` 不再直接建连。
- `client.cancel().await` 不再出现在 direct 每调用路径。
- `McpToolAdapter` 持有 `Arc<DirectMcpClientPool>`。
- `ServiceError` 后移除池内 client；下一次 `ensure_client()` 重建。
- `cargo check -p agentdash-executor` 通过；必要时补充 direct pool 单元测试或在实现中用小 helper 测试 key / invalidation 行为。
