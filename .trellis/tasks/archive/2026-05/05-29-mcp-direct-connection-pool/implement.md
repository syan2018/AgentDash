# executor direct MCP 连接池执行计划

## 顺序

1. 基线确认
   - 复读 `prd.md` / `design.md`。
   - 确认 `direct.rs` 中 `connect_http_server`、`client.cancel().await` 的现状 grep。
   - 确认 `agentdash-local::McpClientManager` 只作为参考，不引入 crate 依赖。

2. 引入池类型
   - 在 `direct.rs` 内新增 `DirectMcpClientPool`、client type alias、按 URL key 的 HashMap。
   - 把 `connect_http_server()` 改成池私有建连 helper。
   - 添加 `ensure_client()`、`invalidate()`、`list_tools()`、`call_tool()`。

3. 改 discovery
   - `discover_mcp_tool_entries()` 开始创建 `Arc<DirectMcpClientPool>`。
   - discovery 阶段通过 pool `list_tools()`，不直接 connect/cancel。
   - `McpToolAdapter::from_tool()` 接收 pool clone。

4. 改 execute
   - `McpToolAdapter` 新增 `pool: Arc<DirectMcpClientPool>`。
   - `execute()` 构造 `CallToolRequestParams` 后调用 pool `call_tool()`。
   - 保留原 `AgentToolError::InvalidArguments` 与 `ExecutionFailed` 输出语义。

5. 失效处理
   - `list_tools()` / `call_tool()` 捕获 `ServiceError` 后调用 `invalidate()`。
   - 不自动重试 `call_tool()`；后续调用重连。
   - 若实现中可轻量测试，补充 invalidation/key 行为测试；若需要完整 RMCP HTTP server 才能测，保留代码注释和 PRD 验收说明，避免为连接池引入大型测试 harness。

6. 验收
   - `rg "connect_http_server" crates/agentdash-executor/src/mcp/direct.rs -n`
   - `rg "client.cancel\\(\\).await" crates/agentdash-executor/src/mcp/direct.rs -n`
   - `cargo check -p agentdash-executor`
   - 若新增测试：运行对应 `cargo test -p agentdash-executor <test_name>`

## 风险点

- 不把池做成全局静态，避免 session/tool set 生命周期泄漏。
- 不在 tool call 失败后自动重试，避免有副作用 MCP tool 被重复执行。
- 不让 `agentdash-executor` 依赖 `agentdash-local`。
- 不改变 `CapabilityState` 裁决路径。

## 回滚点

- 池类型集中在 `direct.rs`，如出现 RMCP 并发使用问题，可回退到“每批 tools 一个池、每个 URL 串行请求”的保守实现。
- 若 `RunningService` 无法满足共享请求语义，退回到只让 discovery 与 adapter 共享同一连接，但保持 per-client mutex 串行化。
