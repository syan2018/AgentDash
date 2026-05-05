# MCP Preset probe 支持 Stdio / Relay 下发

## Goal

补齐 MCP Preset probe 对 **Stdio transport** 的支持 —— 当前后端对 stdio
preset 直接返回 `unsupported`，用户在前端无法验证本地 MCP server 的连通性和工具列表。

方案：通过已有的 **relay WebSocket 通道**，把 probe 指令下发给本机
`agentdash-local`（它持有 stdio 子进程能力），由 local 端执行
`tools/list` 后回传结果。

## What I already know

### 已有基础

- **上层 task 04-23-mcp-preset-tool-discovery** 已交付：
  - `POST /api/projects/{project_id}/mcp-presets/probe` 端点接受
    `McpTransportConfig` body，对 Http/Sse 直连 probe；Stdio 返回 unsupported
  - 前端卡片 auto-probe + detail dialog 即时验证 + workflow 编辑器集成

- **Relay 协议已有 Mcp 命令**（`crates/agentdash-relay/src/protocol.rs`）：
  - `CommandMcpListTools { id, payload: CommandMcpListToolsPayload }`
  - `ResponseMcpListTools { id, payload: Option<ResponseMcpListToolsPayload> }`
  - `CommandMcpCallTool / ResponseMcpCallTool`
  - 通道由 `McpRelayProvider` trait 管理（`crates/agentdash-spi/src/mcp_relay.rs`）

- **Local 端已能执行 stdio MCP**（`crates/agentdash-local/src/mcp_client_manager.rs`）：
  - `McpClientManager::list_tools(server_name)` 支持 stdio / http / sse
  - 当前从 `.agentdash/mcp-servers.json` 读配置，按 name 查找

### 缺失能力

1. **Relay probe 指令**：现有 `CommandMcpListTools` 按 `server_name` 引用
   local 预注册的 server，**但 probe 需要动态传入 transport 配置**
   （"临时连接、探测一次、立即关闭"，不污染 local 的 client 池）。
2. **后端 stdio 分支**：`probe_transport()` 对 Stdio 直接返回 unsupported，
   需要改为"通过 relay 下发指令，等待 response"。
3. **Local 端 probe handler**：需要接收临时 transport，spawn 一次 stdio 进程，
   list_tools，然后清理。

## Requirements

### R1: Relay 协议扩展 —— 一次性 Probe 指令

在 `agentdash-relay` 协议中新增：

```rust
Command::McpProbeTransport {
    id: RequestId,
    payload: CommandMcpProbeTransportPayload,
}

CommandMcpProbeTransportPayload {
    transport: McpTransportConfig,  // 复用 domain 类型，或 relay 内重新定义
}

Response::McpProbeTransport {
    id: RequestId,
    payload: Option<ResponseMcpProbeTransportPayload>,
}

ResponseMcpProbeTransportPayload {
    status: "ok" | "error" | "unsupported",
    latency_ms: Option<u64>,
    tools: Option<Vec<McpToolInfoRelay>>,
    error: Option<String>,
}
```

与现有 `CommandMcpListTools` 区别：不要求 server 已注册到 local 的
`.agentdash/mcp-servers.json`，transport 由云端请求体提供。

### R2: Local 端 handler — 临时 stdio 连接 + tools/list

在 `crates/agentdash-local/src/command_handler.rs` 中为
`Command::McpProbeTransport` 加分支：

- 接收 transport 配置
- 使用 `TokioChildProcess` + `().serve(transport)` 建立 **临时** 连接
- 调 `list_all_tools()`
- `client.cancel()` 关闭
- 所有操作包在 15s `tokio::time::timeout` 中
- 不进入 `McpClientManager` 的连接池（一次性）

### R3: 云端 probe_transport() 分派 stdio

修改 `crates/agentdash-application/src/mcp_preset/probe.rs`：

```rust
pub async fn probe_transport(
    transport: &McpTransportConfig,
    relay: Option<&dyn McpRelayProvider>,  // 新增依赖
) -> ProbeResult {
    match transport {
        Http/Sse => probe_http(url),
        Stdio { .. } => match relay {
            Some(relay) => probe_via_relay(relay, transport).await,
            None => Unsupported { reason: "relay 不可用" },
        },
    }
}
```

API handler 从 `AppState` 拿到 `McpRelayProvider` 传入。

### R4: 前端解除 Stdio disable 限制

- `McpPresetDetailDialog::ProbePanel`：去掉 `isStdio` disable 和 stdio hint
- `McpPresetCard` 的 auto-probe 保留 —— 现在 stdio 也能返回真实结果

保留 "当 relay 离线时 probe 会返回 unsupported / error" 的用户可见文案。

## Acceptance Criteria

- [ ] `CommandMcpProbeTransport` 在 relay protocol 中新增并通过序列化测试
- [ ] Local 端 command handler 对 stdio transport 执行一次性 probe
      （spawn → list → cancel），15s 超时
- [ ] 云端 `probe_transport` 对 stdio 经由 relay 获取工具列表（relay 在线场景）
- [ ] Relay 离线时 probe stdio 返回 error 状态（而非挂起）
- [ ] 前端 detail dialog stdio "Test Connection" 按钮可点，正确展示工具列表
- [ ] 前端卡片对 stdio preset auto-probe 能展示工具 capsule

## Definition of Done

- Relay protocol 新增 command/response 对
- Local / 云端 / 前端三端改动 + 覆盖单测
- Lint / typecheck / CI green
- 至少一个 stdio MCP server（如 `@modelcontextprotocol/server-filesystem`）
  端到端验证成功路径

## Out of Scope

- Probe 结果的跨用户 / 全局缓存（仍保持临时探测语义）
- Relay 路径的性能优化（当前 stdio spawn 每次都要进程冷启动，接受）
- 非 stdio transport 通过 relay 路由（当前 http/sse 仍云端直连）

## Technical Approach

### 关键组件

| 层 | 文件 | 作用 |
|---|---|---|
| Relay 协议 | `crates/agentdash-relay/src/protocol.rs` | 新增 `CommandMcpProbeTransport / ResponseMcpProbeTransport` |
| SPI | `crates/agentdash-spi/src/mcp_relay.rs` | `McpRelayProvider` trait 加 `probe_transport()` 方法 |
| SPI 实现 | `crates/agentdash-api/src/relay/mcp_relay_impl.rs` | 通过 relay 通道下发命令、等待 response |
| Local | `crates/agentdash-local/src/command_handler.rs` | 处理新命令；复用 `mcp_client_manager` 里的 stdio 连接模式但不入池 |
| 云端 | `crates/agentdash-application/src/mcp_preset/probe.rs` | `probe_transport(transport, relay)` |
| API | `crates/agentdash-api/src/routes/mcp_presets.rs` | handler 从 AppState 拿 relay provider |
| 前端 | `frontend/src/features/assets-panel/categories/McpPresetCategoryPanel.tsx` | 移除 stdio disabled |

### 失败模式

| 场景 | 预期 |
|---|---|
| 用户未部署 local backend（relay 未连） | `ProbeResult::Error { "本机 relay 未连接" }` |
| Stdio 进程启动失败（command 不存在） | `ProbeResult::Error { "<spawn 错误>" }` |
| MCP 握手失败 | `ProbeResult::Error { "<rmcp 错误>" }` |
| Relay RTT > 15s | `ProbeResult::Error { "探测超时" }` |

### 风险

- Stdio spawn 每次都有进程冷启动开销（npx 可能 1-3s）—— 接受
- Relay provider 可能为 None（非 cloud 模式下单机运行）—— 分支返回 unsupported
- rmcp crate 在 local / application 都要 stdio feature —— 检查 Cargo.toml 是否需补

## Technical Notes

- Task 04-23-mcp-preset-tool-discovery 已把 probe 基础落地，本 task 仅需
  在 stdio 分支接入 relay 路径，**不改前端的 probe 调用签名**
- Relay 协议目前已有 `CommandMcpListTools`（按预注册 server_name 查询），
  与本 task 需要的"临时 transport probe"语义不同 —— 不复用，新增命令
- `rmcp::transport::child_process::TokioChildProcess` 即 stdio 连接核心，
  local 端已有使用范例（`mcp_client_manager.rs::connect_stdio`）
