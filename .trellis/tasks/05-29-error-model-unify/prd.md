# 错误模型统一（DomainError / ApplicationError / 去 stringly）

> parent: `05-29-slop-cleanup-wave2`。盲审病灶 2 / 旧 parent 病灶6。类：乙（前轮列了没做）。Wave 2。

## Goal

把全栈三套互不兼容的错误制度收敛为结构化模型，让上层能正确映射 HTTP 语义、不再向客户端泄漏内部细节、不再吞错。

## 现状证据

- `DomainError`（`domain/common/error.rs`）仅 `NotFound/Serialization/InvalidConfig` 三变体 → infra **158 处** `InvalidConfig(e.to_string())`；唯一约束冲突靠"插入后数 row"反推（`backend_repository.rs:132`）。
- application 8 模块用 `Result<_,String>`：`routine/executor.rs`、`project/management.rs`、`companion/tools.rs`、`companion/skill_projection.rs`、`context/workspace_sources.rs`、`hooks/provider.rs`、`hooks/script_engine.rs`、`mcp_preset/definition.rs`，与 24 个 `*Error` 枚举并存。
- api **124 处** `ApiError::Internal(e.to_string())` 把 Postgres 报错原文/约束名回客户端，再用 `looks_like_unique_violation`/`looks_like_skill_asset_unique_violation` 字符串嗅探把 500 救回 409（`rpc.rs:91-128`）——自承错误处理有缺陷。
- session SPI port 用 `io::Result`（`NotFound` 靠 `ErrorKind` + 中文串）。

## Scope（先定型→再机械 fan-out）

1. **定型（串行，单人）**：
   - `DomainError` 增 `Database`/`Conflict`/`Forbidden`（或等价 transient 标记）。
   - 新建 `ApplicationError`（thiserror，`#[from] DomainError`、`#[from] ConnectorError`，结构化 `NotFound/Forbidden/Conflict/InvalidConfig/Internal`）。
   - infra `postgres/mod.rs` 单一 `db_err` 检视 `sqlx::Error`（`RowNotFound`→NotFound、`is_unique_violation`→Conflict），替换 158 处 `InvalidConfig(e.to_string())`（与 swarm S8 的 helper 合并衔接）。
2. **fan-out（可并行）**：逐模块把 8 个 `Result<_,String>` 改 `ApplicationError`（先 `routine`/`project`/`mcp_preset::definition`）；api 删 124 处 `Internal(e.to_string())` 改 `?` 经结构化 `From` 路由，删字符串嗅探。
3. session SPI port `io::Result` → `SessionStoreError`（与 `infra-residual` 协调，避免双改）。

## 依赖与协调

- 是 `api-handler-thinning`、`infra-residual` 的**前置**（它们的错误路径依赖本 child 的类型骨架）。
- 与 swarm S8（db_err 机械合并）衔接：S8 只合并签名，本 child 加语义变体。

## Acceptance Criteria（硬指标 + 验收命令）

- [ ] `DomainError` 含 `Database`/`Conflict`/`Forbidden` 变体（`rg "enum DomainError" -A20 crates/agentdash-domain/src/common/error.rs` 可见）；`ApplicationError` 类型定义存在
- [ ] `rg "InvalidConfig\(.*to_string" crates/agentdash-infrastructure | wc -l` = **0**（确属内部错误的豁免逐条列入 prd「豁免清单」+ journal，否则视为未完成）
- [ ] `rg "ApiError::Internal\(.*to_string" crates/agentdash-api | wc -l` = **0**（同上豁免规则）
- [ ] 8 个命名模块 `rg "Result<[^>]*, *String>" <各模块>` 均 = **0** 或在「豁免清单」逐项注明理由
- [ ] `rg "looks_like_unique_violation|looks_like_skill_asset_unique_violation" crates/agentdash-api` = **0**（唯一冲突改由错误类型识别）
- [ ] 新增/现有测试断言：触发 DB 错误的 handler 响应体不含原始 sqlx/Postgres 错误串
- [ ] `cargo check --workspace` exit 0

### 豁免清单（执行时填写，空表示无豁免）

| 位置 | 保留为 Internal/String 的理由 |
|---|---|
| （执行中按需补） | |
