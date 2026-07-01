# 实施计划

## 1. 跨层 capability health contract

定义 `CapabilityHealthStatus`（ready/degraded/unavailable）、`CapabilityHealthDomain`（mcp/executor，non_exhaustive）、`CapabilityHealthItem`（6 字段：id, domain, status, label, summary, actions）。

- Rust contract 层新增类型，`generate_ts` 同步生成 TS 类型。
- Relay protocol `CapabilitiesPayload` 增加可选 `capability_health: Vec<CapabilityHealthItem>`。

相关文件：

- `crates/agentdash-contracts/src/backend/contract.rs`
- `crates/agentdash-contracts/src/generate_ts.rs`
- `crates/agentdash-relay/src/protocol/handshake.rs`
- `packages/app-web/src/generated/backend-contracts.ts`
- `packages/core/src/local-runtime/index.ts`

## 2. Local runtime MCP health producer

- `McpClientManager` 或 runtime 层维护 MCP health 快照（per-server `CapabilityHealthItem`）。
- `probe_mcp_server` 成功/失败时更新对应 server health。
- `ensure_connected` / `list_tools` / `call_tool` 成功/失败时更新 health。
- `LocalRuntimeStatus` 增加 `capability_health` 列表，保留现有 `mcp_server_count`。
- 错误摘要取用户可理解文本，细节留 `diag!`。

相关文件：

- `crates/agentdash-local/src/mcp_client_manager.rs`
- `crates/agentdash-local/src/handlers/mcp_relay.rs`
- `crates/agentdash-local/src/runtime.rs`

## 3. Relay / backend runtime summary 投影

- register 和 capabilities changed 路径透传 `capability_health`。
- Backend runtime summary 增加 typed `capability_health` 投影。
- Executor health 在 summary 层派生：已声明 executor 在线可分配 → ready，不可分配 → degraded，runtime 离线 → unavailable。
- 未由 runtime 声明的 executor 不创建 health 项。

相关文件：

- `crates/agentdash-api/src/relay/ws_handler.rs`
- `crates/agentdash-application/src/backend/runtime_summary.rs`
- `crates/agentdash-api/src/dto/backend.rs`

## 4. 前端展示

- Local Runtime diagnostics 增加声明能力列表，MCP per-server 状态 + actions 按钮。
- Backend/runtime 选择入口展示 executor health，不可用时提示影响并禁用相关操作。
- Session 侧：runtime surface 中有 degraded/unavailable 时，在对话区顶部展示可收起的 inline notice。

相关文件：

- `packages/views/src/local-runtime/LocalRuntimeView.tsx`
- Session 相关组件由实现阶段基于入口定位

## 5. 验证

- contract 生成后确认 TS 与 Rust 同步。
- Rust 测试覆盖：MCP health 状态流转、executor health 派生、runtime summary 输出。
- Frontend 测试覆盖：diagnostics 列表渲染、session notice 条件展示。
- 命令：
  - `cargo test -p agentdash-relay`
  - `cargo test -p agentdash-local mcp`
  - `cargo test -p agentdash-application runtime_summary`
  - `pnpm run shared:check`
  - `pnpm run frontend:check`
