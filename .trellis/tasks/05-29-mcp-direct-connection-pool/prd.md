# executor direct MCP 连接池（S5-A descope）

> parent: `05-29-slop-cleanup-wave2`。从 `quickfix-swarm` 的 S5 拆出（S5 的 render 去重已在 commit `4e693849` 落地；连接池部分因属"有状态优化、非正确性、M-effort"不适合 quickfix，descope 至此）。

## Goal

消除 `agentdash-executor` direct MCP 适配器**每次工具调用都全握手 + 拆解连接**的开销，改为按 server 复用连接（参考 `agentdash-local::McpClientManager` 已有的懒连接池）。

## 现状证据

`crates/agentdash-executor/src/mcp/direct.rs`：
- `execute()`（约 :98/:109）每次 `connect_http_server(&self.server.url).await` 后 `let _ = client.cancel().await;` —— 单次调用即建连 + 拆连。
- `discover_mcp_tool_entries`（约 :149/:156）同样模式。

对照 `crates/agentdash-local/src/mcp_client_manager.rs` 的 `ensure_connected`：按 server_name 维护 `RwLock<HashMap<.., RunningService<..>>>` 懒连接池，复用已建连接。

## 性质与风险（为何 descope 出来单独做）

- **优化非正确性**：当前代码功能正确，只是每调用一次重连，增加握手延迟/连接 churn。
- **有状态**：需引入连接池容器 + 生命周期管理（失效检测、dead-connection 重连、并发安全、关闭清理），属 M-effort，远超 quickfix 的机械范畴。
- 草率塞进 swarm 易引入连接态 bug，故独立成任务、独立验证。

## Scope

1. 给 direct 适配器引入按 url（或 server 标识）复用的连接池，`execute()`/`discover` 改为 `ensure_connected` 复用，去掉每调用的 `client.cancel()`。
2. 处理连接失效/错误后的重连与剔除；关闭路径清理。
3. 评估能否与 `agentdash-local::McpClientManager` 抽出**共享的 MCP client 池抽象**（两处目前各有一份 MCP client 管理），但以不过度抽象为前提——先就近实现，若自然收敛再共享。

## 设计先行
M-effort + 有状态并发，**执行前先出 design.md**（池的 key/失效策略/并发模型/与 local 池是否合并），再 `task.py start`。

## Acceptance Criteria（硬指标）

- [x] `rg "connect_http_server" crates/agentdash-executor/src/mcp/direct.rs` 的调用不再出现在每次 `execute`/`discover` 路径（改 ensure/复用）；`rg "client.cancel\(\).await" direct.rs` 在每调用路径 = 0
- [x] direct 适配器持有连接池容器（`RwLock<HashMap` 或复用 manager）
- [x] 连接失效后能重连（有测试或明确说明）
- [x] `cargo check -p agentdash-executor` + 相关测试通过
- [x] 行为等价：工具调用结果与去重后一致（沿用 `agentdash_mcp::render_content`）

## 验收记录

- `DirectMcpClientPool` 按 server URL 维护 `RwLock<HashMap<String, Arc<Mutex<RunningService<RoleClient, ()>>>>>`，`McpToolAdapter` 持有 pool clone。
- `discover_mcp_tool_entries()` 创建每批 direct tools 共用的 pool，discovery 阶段通过 pool `list_tools()`，adapter 执行阶段通过 pool `call_tool()`。
- `ServiceError` 后会 `invalidate()` 对应 URL；当前 tool call 不自动重试，避免有副作用工具重复执行，后续调用通过 `ensure_client()` 重建连接。
- `rg "connect_http_server" crates/agentdash-executor/src/mcp/direct.rs -n` 仅剩池内建连调用与 helper 定义。
- `rg "client.cancel\(\).await" crates/agentdash-executor/src/mcp/direct.rs -n` 无结果。
- `cargo fmt --check`、`cargo check -p agentdash-executor`、`cargo test -p agentdash-executor` 通过。

## 非目标
- 不改 render 渲染（已在 S5 commit `4e693849` 收敛）。
- 不强行与 local 池合并，除非自然收敛。
