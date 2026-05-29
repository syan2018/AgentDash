# api handler 瘦身（业务下沉 application / routes 拆分 / Json-Value 类型化）

> parent: `05-29-slop-cleanup-wave2`。盲审病灶 4（C）。类：丙。Wave 3，**依赖 `error-model-unify`**。

## Goal

让 api crate 回到"薄 handler + 厚 application"契约：handler 只做 extract→call→Json，业务/编排/对象组装下沉 application，错误经结构化 `From` 映射。

## 现状证据

- handler 绕过 application 直查 domain repo：91 处/18 文件。`llm_providers.rs:92-165`（slug 校验 + `list_all` 算 max_sort + 11 字段映射 + 加密 + repo.create）、`backends.rs:164-239`（合并 registry+DB+health 还内联造假 `BackendConfig` ~15 默认）、`acp_sessions.rs:145-178`（create+binding+bootstrap+freeform 跨三 repo/service）。
- `session_use_cases/construction.rs`(1250) + `context_query.rs`(255)：核心 launch 用例，自承"承接原本挂在 routes/acp_sessions.rs 的组装逻辑"，却放在 transport crate 且返回 `ApiError` → application 无法复用。
- DTO 三处分裂：89 个 inline route DTO + `dto/` 模块 + `agentdash_contracts`，手写逐字段 re-map 遍布。
- 29 处 `Json<serde_json::Value>` 手拼（`acp_sessions.rs:881-905/936/982-986/...`），同文件 key 大小写不一致（`sessionId` vs `session_id`）。
- `routes.rs` 701 行单 router 表，资源交错，unsecured 集合（:674-690）易漏。

## Scope

1. 把直查 repo 的 handler（`llm_providers`/`backends`/`projects`/`stories`/`canvases`/`acp_sessions`）业务下沉 application service（返回 `ApplicationError`），handler 变薄；以现有 `task_execution`/`mcp_presets` 为范式。
2. `session_use_cases/construction.rs` + `context_query.rs` 迁回 `agentdash_application::session`，返回 application error，api 侧 `From`-map。**与甲类 `session-assembly-converge` 重审协调**（同改 session 装配，避免冲突）。
3. DTO 单向收敛：request/response 契约归 `agentdash_contracts`（与 `contract-pipeline-unify` 对齐），api-only view DTO 归 `dto/`，禁 inline DTO。
4. 29 处 `Json<Value>` 改具名 `#[serde(rename_all="camelCase")]` struct，修大小写不一致。
5. `routes.rs` 拆 per-module `pub fn router()`，secured/unsecured 各一显式 Router 组合。

## 依赖与协调

- **前置**：`error-model-unify`（handler 改 `?` 需结构化错误）。
- **协调**：`contract-pipeline-unify`（DTO 归属）、甲类 `session-assembly-converge`（construction 迁移同改 session）。

## Acceptance Criteria（硬指标 + 验收命令）

- [ ] 目标路由文件（`llm_providers`/`backends`/`projects`/`stories`/`canvases`/`acp_sessions`）中对 `*_repo`/`repos.` 的直接调用计数降至「保留清单」内（其余下沉 application；清单逐项注明）
- [ ] `crates/agentdash-api/src/session_use_cases/` 目录移除；`construction`/`context_query` 出现在 `crates/agentdash-application/src/session/` 且不返回 `ApiError`（grep `ApiError` 在该两文件 = 0）
- [ ] `rg "Json<serde_json::Value>" crates/agentdash-api/src/routes | wc -l` = **0**（改具名 `#[serde(rename_all=\"camelCase\")]` struct）
- [ ] `rg "struct \w+(Response|Request|Dto)" crates/agentdash-api/src/routes | wc -l` = **0**（inline DTO 归 contracts/dto）
- [ ] `routes.rs` 每资源模块导出 `pub fn router(`（`rg -c "pub fn router" crates/agentdash-api/src/routes` ≥ 目标模块数）；secured/unsecured 各一显式组合
- [ ] `cargo check --workspace` exit 0 + api 测试通过

### repo 直调保留清单（执行时填写）

| 路由 | 保留直调的理由 |
|---|---|
| `session_construction.rs` adapter | composition / transport adapter：从 `AppState` 组装 application use case deps，并补充 runtime-only VFS surface projection；退出条件是后续若 runtime projection 也抽为 application port，再移除 adapter 内的直接 repo/runtime 读取 |
| `canvases.rs` runtime session scope | Canvas CRUD 已下沉 application；runtime snapshot/invoke 仍需在 API 校验 session 与 Canvas 是否属于同一 Project，当前保留 session binding 查询作为 transport/runtime scope adapter，退出条件是 session scope 校验收敛为 application session access use case |
| `projects.rs` sharing/auth adapter | Project create/detail/update/delete/clone 已下沉 application；grant/revoke/list grants 与 user/group directory 存在性校验仍留在 API sharing/auth adapter，退出条件是 Project sharing management use case 落地 |
