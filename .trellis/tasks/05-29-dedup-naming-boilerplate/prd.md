# 重复类型 / 命名 / 样板低风险清理

> 病灶 4/5/6 低风险机械部分。事实源：`docs/reviews/2026-05-29-slop-cleanup-review/00-synthesis.md`。
> 依赖：`drop-step-lifecycle` 之后（避免改到将删的代码）。

## Scope（互不耦合的小重构，逐项 commit）
1. **`McpTransportConfig` 去重**：domain(`mcp_preset/value_objects.rs:19`) 与 spi(`connector/mod.rs:464`) 各一份。归 domain，spi `use`，统一 header 名。涉 ~36 文件 import。
2. **`SessionPersistence` supertrait**：`spi/session_persistence.rs:825` 手抄 7 子 trait ~35 方法 → `trait SessionPersistence: SessionMetaStore + ... {}` 空 body，各 impl 留一行。
3. **命名去歧义**：`RelayVfsService`→`VfsService`（`vfs/relay_service.rs`→`service.rs`）；删 `runtime.rs`/`runtime_bridge.rs`（类型上移 spi 或并入消费方）；`runtime_gateway/`→`action_gateway/`（可选）。
4. **`From<XxxError> for ApiError`**：补全后删 api routes 71 处 `.map_err(|e| ApiError::Internal(e.to_string()))`。
5. **bridge spawn helper**：`bridges/` 抽 `spawn_bridge_stream` + `check_http_response`，消 4 bridge 复制。
6. **MCP HTTP 连接 helper**：`agentdash-relay` 抽 `connect_mcp_http`/`probe_mcp_transport`，消 executor/local/relay 三处重复。
7. **枚举 boilerplate**（可选）：spi 状态枚举用 strum 或复用 `#[serde(rename_all)]` 替手写 `as_str`/`TryFrom`。

## Acceptance
- [ ] `McpTransportConfig` 全工作区单一定义
- [ ] `SessionPersistence` 为空 body supertrait
- [ ] 无 `RelayVfsService`/`runtime_bridge` 残留引用
- [ ] api routes 样板 `map_err` 大幅减少
- [ ] `cargo check --workspace` 通过

## Constraints
- 仅改 `crates/`。**不要 git commit**（orchestrator 逐项 gate；每项一 commit 便于回溯）。
- 纯机械，不改行为。每项可独立交付。
