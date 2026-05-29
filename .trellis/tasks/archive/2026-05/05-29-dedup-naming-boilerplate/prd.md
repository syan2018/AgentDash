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

## 执行记录 / 偏差说明

- **第 3 项「删 `runtime.rs`/`runtime_bridge.rs`」跳过**：核实后发现两者均为 live 共享代码，非死代码、亦无「唯一消费方」可并入。
  - `application/src/runtime.rs` 定义 `RuntimeMcpServer`（含 `name/transport_label/target` 方法），被 `context/builder.rs`、`context/builtins.rs`、`session/assembler.rs`、`session/bootstrap.rs`、`session/plan.rs` 等多处消费。
  - `application/src/runtime_bridge.rs` 是 `RuntimeMcpServer ↔ SessionMcpServer` 双向转换工具，被 `assembler.rs`（3 处）、`construction_planner.rs`（2 处）、`task/context_builder.rs`、`context/builtins.rs` 消费。
  - `api/src/runtime_bridge.rs` 是另一独立模块（`relay_file_entries_to_runtime`），与 application 同名文件无关。
  - 将 `RuntimeMcpServer` 上移 spi 并重接 ~7 文件属于「行为面重构」而非本任务约定的机械改名，风险与范围都超界，故按「与 prd 不符则跳过并记录」原则跳过，留待后续 `runtime*` 命名簇专项处理。
  - 第 3 项的核心低风险部分 `RelayVfsService → VfsService`（文件 `vfs/relay_service.rs → vfs/service.rs`、模块路径 `vfs::relay_service → vfs::service`、`mutation_dispatcher` 私有字段 `relay_service → vfs_service`）已全部完成。

- **第 4 项「删 71 处样板 `map_err`」按「行为不变」原则收窄**：原 77 处 `.map_err(|e| ApiError::Internal(e.to_string()))` 内层错误类型并不同质。
  - 大量站点内层是 `DomainError`，而 `From<DomainError> for ApiError` 已存在且会把 `NotFound`/`InvalidConfig` 映射为 404/400；若直接换 `?` 会改变 HTTP 状态码（行为变更），超出本任务「纯机械、行为不变」约束，故**不动**。
  - 另有站点用自定义中文消息（如 `format!("批量读取 session meta 失败: {e}")`），换 `?` 会丢失消息前缀，亦属行为变更，**不动**。
  - 真正可安全机械化的子集：session 持久化层统一返回 `io::Result`，对外恒为 500 Internal。已在 `rpc.rs` 新增 `From<std::io::Error> for ApiError`（→ Internal），并将 `acp_sessions.rs` / `story_sessions.rs` 中调用 `session_core` 这些 `io::Result` 方法（`list_sessions`/`create_session`/`delete_session`/`get_session_meta`/`mark_owner_bootstrap_pending`/`inspect_session_execution_state`）的 9 处样板 `.map_err(...)?` 收敛为裸 `?`，语义完全等价。
  - 进一步成片删除需逐站点确认内层错误类型并接受状态码语义统一，属后续「错误模型收敛」专项，不在本机械任务范围内。

- **第 5 项 bridge spawn helper 已完成**：`bridges/mod.rs` 新增 `spawn_bridge_stream`（channel + spawn + 错误转发脚手架）与 `check_http_response`（统一 `{label} 返回 {status}: {body}` 校验）。anthropic / openai_completions / openai_responses 三个 bridge 的 `stream_complete` 与 HTTP 状态校验全部复用；codex bridge 复用 `spawn_bridge_stream`，但其 HTTP 校验含 `friendly_codex_error` 与二段校验、错误前缀为 `Codex API`，属 bespoke 逻辑，保留不动以维持行为。

- **第 6 项 MCP HTTP helper 的落点与 prd 描述不符，按实际依赖图收窄**：prd 称「在 `agentdash-relay`（executor/local 都依赖）抽」，但核实依赖图后发现：
  - `agentdash-executor` **并不依赖** `agentdash-relay`；
  - `agentdash-relay` 是精简协议 crate，**不依赖 `rmcp`/`reqwest`**。
  - 若强行把 helper 放进 `agentdash-relay`，需给协议 crate 引入 `rmcp`+`reqwest` 重依赖、并让 executor 反向依赖 relay——属架构层改动而非机械去重，且污染 lean crate，故不采纳。
  - 实际可安全机械化的部分：`local/mcp_client_manager.rs:152` 与 `local/handlers/mcp_relay.rs:47` 同属 `agentdash-local` crate，二者逐字复制 streamable-http worker 构造（`StreamableHttpClientWorker::new(reqwest::Client::new(), …with_uri(url))`）。已抽 `agentdash-local::mcp_connect::mcp_http_worker(url)`，两处复用，且**保留各自原有的握手与错误消息**（`HTTP MCP 连接失败` / `连接 MCP Server 失败`），行为不变。
  - `executor/mcp/direct.rs:193` 的 `connect_http_server` 在另一 crate、错误类型为 `ConnectorError`，与 local 无共享 home（除非新增跨 crate 依赖），按依赖图保持不动。三处中已消两处的真实复制。
