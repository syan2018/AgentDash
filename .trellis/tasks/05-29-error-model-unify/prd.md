# 错误模型统一（DomainError / ApplicationError / 去 stringly）

> parent: `05-29-slop-cleanup-wave2`。盲审病灶 2 / 旧 parent 病灶6。类：乙（前轮列了没做）。Wave 2。

## Goal

把全栈三套互不兼容的错误制度收敛为结构化模型，让上层能正确映射 HTTP 语义、不再向客户端泄漏内部细节、不再吞错。此任务是 `api-handler-thinning` 与 `infra-residual` 的前置：先建立错误语义骨架，再让 handler / repository / session port 改用同一套可映射错误。

## 现状证据

- `DomainError`（`domain/common/error.rs`）当前为 `NotFound/InvalidTransition/Serialization/InvalidConfig`，缺少数据库、冲突、权限等可映射语义；infra 仍有 **204 处** `InvalidConfig(...to_string())`。
- application 8 模块仍用 `Result<_,String>`：`routine/executor.rs`、`project/management.rs`、`companion/tools.rs`、`companion/skill_projection.rs`、`context/workspace_sources.rs`、`hooks/provider.rs`、`hooks/script_engine.rs`、`mcp_preset/definition.rs`，与多套局部 `*ApplicationError` 枚举并存。
- api 当前 **126 处** `ApiError::Internal(e.to_string())` 把底层错误原文回传风险留在 handler 层，再用 `looks_like_unique_violation`/`looks_like_skill_asset_unique_violation` 字符串嗅探把 500 救回 409（`rpc.rs:91-128`）——错误语义没有在 infra/application 边界保住。
- session SPI port 用 `io::Result`（`NotFound` 靠 `ErrorKind` + 中文串）。

## Scope（先定型→再机械 fan-out）

1. **定型（串行，单人）**：
   - `DomainError` 增 `Database`/`Conflict`/`Forbidden`（或等价语义变体），保留 `InvalidConfig` 表达用户/配置输入错误。
   - 新建全局 `ApplicationError`（thiserror，`#[from] DomainError`、`#[from] ConnectorError`，结构化 `BadRequest/NotFound/Forbidden/Conflict/InvalidConfig/Internal/Unavailable`），局部 application error 可先 `From<ApplicationError>` 或逐步内联迁移，避免一次性重写所有 use case。
   - infra `postgres/mod.rs` 单一 `db_err` 检视 `sqlx::Error`（`RowNotFound`→NotFound、`is_unique_violation`→Conflict，连接/协议类→Database），替换 204 处 `InvalidConfig(e.to_string())`（与 swarm S8 的 helper 合并衔接）。
2. **fan-out（可并行）**：逐模块把 8 个 `Result<_,String>` 改 `ApplicationError`（先 `routine`/`project`/`mcp_preset::definition`）；api 删 126 处 `Internal(e.to_string())` 改 `?` 经结构化 `From` 路由，删字符串嗅探。
3. session SPI port `io::Result` → `SessionStoreError` 由后续 `infra-residual` 承接；本 child 只在 API/Application 映射处先把 `std::io::Error` 固定为不泄露内部细节的 500 语义，避免与 session 持久化端口重构双改。

## 依赖与协调

- 是 `api-handler-thinning`、`infra-residual` 的**前置**（它们的错误路径依赖本 child 的类型骨架）。
- 与 swarm S8（db_err 机械合并）衔接：S8 只合并签名，本 child 加语义变体。

## Acceptance Criteria（硬指标 + 验收命令）

- [x] `DomainError` 含 `Database`/`Conflict`/`Forbidden` 变体（`rg "enum DomainError" -A20 crates/agentdash-domain/src/common/error.rs` 可见）；`ApplicationError` 类型定义存在
- [x] `rg "InvalidConfig\(.*to_string" crates/agentdash-infrastructure | wc -l` = **0**（确属内部错误的豁免逐条列入 prd「豁免清单」+ journal，否则视为未完成）
- [x] `rg "ApiError::Internal\(.*to_string" crates/agentdash-api | wc -l` = **0**（同上豁免规则）
- [x] 8 个命名模块 `rg "Result<[^>]*, *String>" <各模块>` 均 = **0** 或在「豁免清单」逐项注明理由
- [x] `rg "looks_like_unique_violation|looks_like_skill_asset_unique_violation" crates/agentdash-api` = **0**（唯一冲突改由错误类型识别）
- [x] 新增/现有测试断言：触发 DB 错误的 handler 响应体不含原始 sqlx/Postgres 错误串
- [x] `cargo check --workspace` exit 0

## 验收记录

- `rg "InvalidConfig\(.*to_string" crates/agentdash-infrastructure`：无输出。
- `rg "ApiError::Internal\(.*to_string" crates/agentdash-api`：无输出。
- `rg "looks_like_unique_violation|looks_like_skill_asset_unique_violation" crates/agentdash-api`：无输出。
- `rg "Result<[^>]*, *String>" crates/agentdash-application/src/routine/executor.rs crates/agentdash-application/src/project/management.rs crates/agentdash-application/src/companion/tools.rs crates/agentdash-application/src/companion/skill_projection.rs crates/agentdash-application/src/context/workspace_sources.rs crates/agentdash-application/src/hooks/provider.rs crates/agentdash-application/src/hooks/script_engine.rs crates/agentdash-application/src/mcp_preset/definition.rs`：无输出。
- `cargo test -p agentdash-api append_required_story_change_maps_repo_failure_to_internal_error`：通过。
- `cargo check --workspace`：通过；仍有既存 `agentdash-application` warning。

### 豁免清单（执行时填写，空表示无豁免）

| 位置 | 保留为 Internal/String 的理由 |
|---|---|
| 无 | |
