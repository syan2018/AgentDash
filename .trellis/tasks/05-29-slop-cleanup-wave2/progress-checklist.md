# Slop Cleanup 全程推进 Checklist

> 压缩恢复入口：每次恢复本目标时，先读本文件，再读当前 active child 的 `prd.md` / `design.md` / `implement.md`。本文件记录跨 child 的真实推进顺序、当前状态和验收证据。

## 当前恢复状态

- 当前分支：`refactor/architecture-slop-cleanup`
- 当前推进 child：无；`05-29-domain-purification` 已归档；正在处理父级 row 12 的最终集成 review。
- 当前 child 状态：无 active child。
- 当前主线步骤：`error-model-unify`、`contract-pipeline-unify`、`mcp-direct-connection-pool`、`vfs-dedup`、`infra-residual`、`api-handler-thinning`、`capability-state-unify`、`frontend-server-state-refactor`、`session-assembly-converge`、`structural-splits`、`domain-purification` 已提交并归档；PR38 的 `LifecycleRunLink`、SessionBinding 移除、Permission Grant 合入已提交；父级集成 review 正在复核硬 AC，已发现并修复 PR38 带回的 session_core `io::Result` 与 `ApiError::Internal(error.to_string())` 回归。
- 已完成的 `error-model-unify` 代码进展：
  - `DomainError` 增加 `Conflict` / `Forbidden` / `Database` 语义变体。
  - 新增 `agentdash_application::ApplicationError`。
  - `ApiError` 增加 `From<ApplicationError>`，并更新 `From<DomainError>` 的结构化映射。
  - Postgres `db_err` / `sql_err_for` 开始按 SQLSTATE 映射 `RowNotFound` / unique / FK / exclusion / database error。
  - Postgres repository 中机械 sqlx 错误路径已批量改用 `super::db_err`，`crates/agentdash-infrastructure` 的 `InvalidConfig(...to_string())` 计数从 203 降到 0。
  - API `ApiError::Internal(e.to_string())` 计数降到 0，`looks_like_unique_violation` / `looks_like_skill_asset_unique_violation` 已删除。
  - 8 个指定 application 模块的 `Result<_, String>` grep 已清零，迁到 `ApplicationError` 或局部结构化错误。
  - `backend/error-handling.md` 与 `backend/database-guidelines.md` 已同步当前错误语义契约。
- `error-model-unify` 最近验证状态：
  - `cargo check -p agentdash-domain` 已通过。
  - `cargo check -p agentdash-application` 已通过（仅既存 warning）。
  - `cargo check -p agentdash-infrastructure` 已通过（含 Postgres fan-out 后复验）。
  - `cargo check -p agentdash-api` 已通过（仅 application 既存 warning）。
  - `cargo test -p agentdash-api append_required_story_change_maps_repo_failure_to_internal_error` 已通过。
  - `cargo check --workspace` 已通过（仅 application 既存 warning）。
- 已提交记录：
  - `c2fb8f78 refactor(error): 统一后端错误模型并清理 stringly 映射`
  - `8f1d232d docs(task): 归档错误模型统一子任务`
- `contract-pipeline-unify` 已完成并归档：
  - `Task/Story/Workspace/Project` response 已进入 `agentdash-contracts::core`，前端生成 `core-contracts.ts`。
  - API 旧 `dto/project.rs`、`dto/story.rs`、`dto/task.rs`、`dto/workspace.rs` 已删除，route 通过 contract response 输出。
  - `packages/app-web/src/types/index.ts` 的 Project / Workspace / Story / Task wire 类型改为 generated alias，手写 core interface/type grep 已清零。
  - 根 `package.json` 默认 `check` 链路已加入 `contracts:check`。
  - `common-contracts.ts` 已成为 generated 目录唯一 `JsonValue` 定义。
  - `extensionRuntime.ts` 已删除逐字段 mapper，内部 endpoint 直接信任 generated contract response。
  - `McpTransportConfig` / `MountCapability` / `ProjectVfsMountContent` 纯镜像命名副本已清到 PRD grep 为 0。
  - `services/session.ts` 已移除 generated DTO identity mapper；仅保留 view model / route-local 过渡 mapper。
  - cross-layer/frontend spec 已同步为“内部 API 信任 generated wire；mapper 只保留 view model、外部输入、route-local 过渡 DTO”。
  - 验证通过：`pnpm run contracts:check`、`pnpm -C packages/app-web exec tsc --noEmit`、`cargo check --workspace`。

## 全程推进队列

| 顺序 | 任务 | 状态 | 完成证据 |
|---:|---|---|---|
| 0 | `05-29-quickfix-swarm` | 已归档 | archive 中 task completed；quickfix commit 已存在 |
| 1 | `05-29-error-model-unify` | 已归档 | 提交 `c2fb8f78`；归档提交 `8f1d232d`；本 child AC 全满足；`cargo check --workspace` 通过；stringly error grep 清零；无豁免 |
| 2 | `05-29-contract-pipeline-unify` | 已归档 | 提交 `0edb6833` / `5a5316c4` / `2dea9bf9` / `eb026433`；归档提交 `a4336c55`；`Task/Story/Workspace/Project` 已进 contracts；前端 core 手写类型 grep 清零；`JsonValue` 单源；mirror grep 清零；mapper 保留清单已写；spec 已同步；`contracts:check` / `cargo check --workspace` / app-web `tsc --noEmit` 通过 |
| 3 | `05-29-mcp-direct-connection-pool` | 已归档 | 规划提交 `10c33f64`；实现提交 `79872c0c`；归档提交 `93a64a05`；`DirectMcpClientPool` 已接入 discovery/execute；`crates/agentdash-executor/src/mcp/direct.rs` 内 `client.cancel().await` 无结果；`connect_http_server` 仅剩池内建连；失效后 invalidate、后续 ensure 重连；`cargo check -p agentdash-executor` / `cargo test -p agentdash-executor` 通过 |
| 4 | `05-29-vfs-dedup` | 已归档 | 提交 `4d2e9105` / `05016cf0` / `b7db5bbc` / `6641d289`；归档提交 `c815b4ba`；provider SPI `watch` / `MountEventReceiver` 已清零；`ProviderDescriptor` / `MountIo` / `MountSearch` 已落地；`FsPatchTarget` 已接入本机 `ToolExecutor`；`apply_patch_to_fs` / `apply_patch_to_inline_files` grep 清零；`VfsService::resolve_provider_dispatch` 已集中 provider dispatch；`PROVIDER_INLINE_FS` 在 service 内仅剩 `is_inline_mount()` 1 处；service 内 `map_err(|e| e.to_string())` grep 清零；orchestrator output port JSON fallback grep 清零；`cargo test -p agentdash-application vfs`、`cargo test -p agentdash-application activity_outputs`、`cargo check --workspace` 通过；workflow spec 已记录 output port JSON contract |
| 5 | `05-29-infra-residual` | 已归档 | 规划提交 `27cd34e7`；实现提交 `d500c892` / `f346b96c` / `f93112d9`；本机 runtime 已切到 `PostgresRuntime` + `PostgresSessionRepository`；sqlite repository 目录已删除；`SqliteSessionRepository` / `SqlitePool` / `SqliteConnectOptions` grep 清零；Session persistence trait、`session_core.rs`、`PostgresSessionRepository` 已改为 `SessionStoreError` / `SessionStoreResult`，application 边缘显式映射；父级 review 复查时修复 PR38 带回的 `session_core.rs` `io::Result` 回归，`io::Result` 在 SPI/session_core/Postgres session repo grep 清零；历史 migrations 中 `*_at TEXT` 已改 `TIMESTAMPTZ`，新增 `0069_timestamp_columns_timestamptz.sql` 迁移已有开发库，repository bind/read 已改 `DateTime<Utc>`，`parse_pg_timestamp_checked` 删除，infra `to_rfc3339` grep 为 0；`cargo test -p agentdash-infrastructure`、`cargo check --workspace` 通过；archive 位于 `.trellis/tasks/archive/2026-05/05-29-infra-residual` |
| 6 | `05-29-api-handler-thinning` | 已归档 | 已补齐 `design.md` / `implement.md`；`session_use_cases` 迁移 slice 已提交 `ab52be01`；`canvases.rs` CRUD 已提交 `ee51ce88`；`projects.rs` CRUD 已提交 `54ea4818`；`stories.rs` Story 聚合 CRUD 已提交 `8923888a`；`llm_providers.rs` catalog 已提交 `80a97d6a`；`backends.rs` add/remove/ensure-local-runtime 写命令已提交 `08fc0574`；route direct repo grep 显式剩余 runtime read projection adapter并写入 PRD 保留清单；`Json<serde_json::Value>` / `Json<Value>` route response grep 清零；inline route DTO grep 清零；32 个 route module 已导出 `pub fn router()`，根 `routes.rs` 已收敛为 secured/public router 组合；`cargo check --workspace` / `pnpm run contracts:check` / `cargo test -p agentdash-api` 通过；archive 位于 `.trellis/tasks/archive/2026-05/05-29-api-handler-thinning` |
| 7 | `05-29-capability-state-unify` | 已归档 | 提交 `35d94547`；archive 位于 `.trellis/tasks/archive/2026-05/05-29-capability-state-unify`；`hooks::CapabilityDelta` 已删除并并入 `SetDelta`；`SetDelta::compute` 承接旧 diff；`rg "CapabilityDelta" crates` 清零；`cargo check --workspace` 通过；指定 application lib 测试仍命中既存 test-only `std::io::Error`/`SessionStoreError` 债务 |
| 8 | `05-29-frontend-server-state-refactor` | 已归档 | 提交 `045cfa3d` / `eff74a2f` / `4421000f` / `7ee7ff15`；archive 位于 `.trellis/tasks/archive/2026-05/05-29-frontend-server-state-refactor`；features/stores `useQuery|useMutation` 命中 28（迁移前 0）；store loading/error/saving 命中 233→178；`llmProviderStore` / `routineStore` 删除；`eventStore.activeProjectId`、store 内 `getState().handleStateChange/fetchBackends`、`workflowStore.selectedActivityKey` grep 清零；`SettingsPageContent.tsx` 255 行，`activity-inspector.tsx` 336 行，`workspace-layout.tsx` 442 行；`pnpm -C packages/app-web exec tsc --noEmit` 与相关 Vitest 通过 |
| 9 | `05-29-session-assembly-converge` | 已归档 | 提交 `462f8ee3`；归档提交 `f82846ec`；archive 位于 `.trellis/tasks/archive/2026-05/05-29-session-assembly-converge`；重新复核确认不抽跨路径 `SessionSurfaceResolver`，只抽路径内 helper；`SessionAssemblyBuilder` 已拆出；`compose_owner_bootstrap` 约 66 行、`compose_story_step` 约 51 行；VFS 投影改由 `SessionConstructionPlan` helper 集中同步；test-only session persistence mock 已对齐 `SessionStoreError`；`cargo check --workspace` 与 `cargo test -p agentdash-application --lib`（595 passed）通过 |
| 10 | `05-29-structural-splits` | 已归档 | 提交 `97d12d15`；归档提交 `d899b162`；archive 位于 `.trellis/tasks/archive/2026-05/05-29-structural-splits`；`agentdash-application-ports` crate 已建立并迁入 backend/extension/VFS transport port；`vfs::tools::provider` 内部引用 grep 清零；`memory_persistence.rs` 已移出 `src/` 到 test-support；`SessionChatView.tsx` 584 行；`workspace-list.tsx` 4 行目录入口；`extension-runtime↔workspace-panel↔canvas-panel` 双向循环已打断；`cargo check --workspace`、`cargo test -p agentdash-application --lib`、`pnpm -C packages/app-web exec tsc --noEmit` 通过 |
| 11 | `05-29-domain-purification` | 已归档 | 提交 `d831111a`；归档提交 `7776f73e`；archive 位于 `.trellis/tasks/archive/2026-05/05-29-domain-purification`；domain `ts-rs/schemars` 依赖与 derive/import 已清零；`contracts::workflow` 不再 re-export domain workflow 类型；MCP schema 不再要求 domain `JsonSchema`；session id 假 alias 已删除；`pnpm run contracts:check` / `cargo check --workspace` / `cargo test -p agentdash-domain --lib` / `cargo test -p agentdash-mcp --lib` / app-web `tsc --noEmit` 通过 |
| 12 | 父级集成 review | 进行中 | 正在逐条复核 wave2 parent AC；已复查并修复 `ApiError::Internal(error.to_string())` 与 `session_core.rs` `io::Result` 回归；`cargo test -p agentdash-application --lib`、`cargo test -p agentdash-api --lib`、`cargo check --workspace`、`pnpm run contracts:check`、app-web `tsc --noEmit`、`git diff --check` 已通过；待归档决策与剩余语义审计 |
| 13 | （用户补充）整理远端pr合并 | 已提交 | 提交 `4c8b0fed`；已放弃宽 merge，改为按功能 cherry-pick PR38：保留 wave2 cleanup 基线，合入 `LifecycleRunLink` / run-oriented story API / SessionBinding 移除 / Permission Grant 后端与前端骨架；migration 已重排为 `0070_lifecycle_run_links`、`0071_drop_session_bindings`、`0072_permission_grants`，并改成 PostgreSQL + TIMESTAMPTZ/JSONB；`cargo check --workspace`、`pnpm run contracts:check`、app-web `tsc --noEmit`、`cargo test -p agentdash-domain --lib`、`cargo test -p agentdash-application --lib`、`cargo test -p agentdash-api --lib` 已通过 |

## 每次恢复的固定检查

- `git status --short --branch`
- `python ./.trellis/scripts/get_context.py`
- 当前 active child 的：
  - `.trellis/tasks/<child>/prd.md`
  - `.trellis/tasks/<child>/design.md`
  - `.trellis/tasks/<child>/implement.md`
- 当前 child 的 grep/count AC。
- 若上一轮留下了未完成验证，先完成验证，再继续下一步代码。

## 当前 child 下一步

1. 进入父级集成 review，逐条复核 wave2 parent AC。
2. 复核 row 0-11 的归档证据、验收命令与 spec 更新是否足够支撑 parent closure。
3. 完成父级最终 gates；若无新缺口，提交 review 修正并归档 wave2 parent。

## 今日子代理可行性复核

- `api-handler-thinning`：已归档；`Json<Value>` route response 与 inline route DTO grep 均清零，32 个 route module 已导出 `pub fn router()`，根 router 已收敛为 secured/public 组合。
- `frontend-server-state-refactor`：硬验收已通过；React Query 真实采用进入 feature model，LLM Provider / Routine server-state 已迁移；active project 双源、跨 store 命令式耦合、`workflowStore.selectedActivityKey` 已清理；`SettingsPageContent` / `activity-inspector` 已拆分。
- `capability-state-unify`：已归档；`hooks::CapabilityDelta` 与 `connector::capability_delta::SetDelta` 已合并为 `SetDelta` / `SetDelta::compute`；`CapabilityDimensionModule` 与 `DimensionDelta` 分属 validate/replay 与 render/section 两条轴，trait merge 推迟。
- `session-assembly-converge`：已归档；不抽完整 resolver，因 launch/query 已共享 bootstrap/finalize 收敛点且剩余差异是真契约差异；`SessionAssemblyBuilder` 与 compose helper 已拆；`surface.vfs` / `context_projection.vfs` 保留双投影但改由 `SessionConstructionPlan` helper 集中同步。
- `domain-purification`：DDD 方向确认：domain 不引用 contract/protocol DTO，contract/API/protocol 层位于外层并解析 wire payload 后进入 domain/application。已验证 `agentdash-contracts::workflow` 不再 re-export domain workflow 类型；MCP 工具 schema 通过 JSON payload 边界解析复杂 domain 片段，避免 domain 重新派生 `JsonSchema`。
- `structural-splits`：可行但需控制依赖上限；第一批只迁 `backend_transport.rs` 这类纯 port，第二批只迁 `ExtensionRuntimeActionTransport` / `ExtensionRuntimeChannelTransport` / error，第三批只迁 `VfsMaterializationTransport`；provider/use case/service 留 application，避免新 crate 反向依赖 application。

## 全局验收 Gates

- 每个 child 完成时运行其 PRD 中的 grep/count AC。
- Rust 相关 child 最终运行 `cargo check --workspace`。
- `cargo test -p agentdash-api` 已通过。`cargo test -p agentdash-application --lib` 已在 `session-assembly-converge` 通过（595 passed），此前 test-only `std::io::Error` / `SessionStoreError` mock 债务已修复。
- 前端相关 child 最终运行 `pnpm -C packages/app-web exec tsc --noEmit`。
- contract 相关 child 运行 `pnpm run contracts:check`。
- 涉及 DB schema 的 child 需要 migration，并验证 migration up。
- 缩窄 scope 时在 child PRD 的豁免/结论区域与 journal 中同步记录理由和复核建议。
