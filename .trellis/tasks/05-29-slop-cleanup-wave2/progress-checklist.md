# Slop Cleanup 全程推进 Checklist

> 压缩恢复入口：每次恢复本目标时，先读本文件，再读当前 active child 的 `prd.md` / `design.md` / `implement.md`。本文件记录跨 child 的真实推进顺序、当前状态和验收证据。

## 当前恢复状态

- 当前分支：`refactor/architecture-slop-cleanup`
- 当前推进 child：`05-29-api-handler-thinning`（已启动；正在按批次瘦身 API handler）
- 当前 child 状态：`in_progress`（`session_use_cases`、低风险 CRUD、`llm_providers` catalog、`backends` 写命令已推进；`Json<Value>` route response 已清零；API-only DTO 搬迁第一批已完成；当前继续 inline DTO / router 收尾）
- 当前主线步骤：`error-model-unify`、`contract-pipeline-unify`、`mcp-direct-connection-pool`、`vfs-dedup`、`infra-residual` 已提交并归档；当前推进 `api-handler-thinning`。
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
| 3 | `05-29-mcp-direct-connection-pool` | 已归档 | 规划提交 `10c33f64`；实现提交 `79872c0c`；归档提交 `93a64a05`；`DirectMcpClientPool` 已接入 discovery/execute；`client.cancel().await` grep 清零；`connect_http_server` 仅剩池内建连；失效后 invalidate、后续 ensure 重连；`cargo check -p agentdash-executor` / `cargo test -p agentdash-executor` 通过 |
| 4 | `05-29-vfs-dedup` | 已归档 | 提交 `4d2e9105` / `05016cf0` / `b7db5bbc` / `6641d289`；归档提交 `c815b4ba`；provider SPI `watch` / `MountEventReceiver` 已清零；`ProviderDescriptor` / `MountIo` / `MountSearch` 已落地；`FsPatchTarget` 已接入本机 `ToolExecutor`；`apply_patch_to_fs` / `apply_patch_to_inline_files` grep 清零；`VfsService::resolve_provider_dispatch` 已集中 provider dispatch；`PROVIDER_INLINE_FS` 在 service 内仅剩 `is_inline_mount()` 1 处；service 内 `map_err(|e| e.to_string())` grep 清零；orchestrator output port JSON fallback grep 清零；`cargo test -p agentdash-application vfs`、`cargo test -p agentdash-application activity_outputs`、`cargo check --workspace` 通过；workflow spec 已记录 output port JSON contract |
| 5 | `05-29-infra-residual` | 已归档 | 规划提交 `27cd34e7`；实现提交 `d500c892` / `f346b96c` / `f93112d9`；本机 runtime 已切到 `PostgresRuntime` + `PostgresSessionRepository`；sqlite repository 目录已删除；`SqliteSessionRepository` / `SqlitePool` / `SqliteConnectOptions` grep 清零；Session persistence trait、`session_core.rs`、`PostgresSessionRepository` 已改为 `SessionStoreError` / `SessionStoreResult`，application 边缘显式映射；`io::Result` 在 SPI/session_core/Postgres session repo grep 清零；历史 migrations 中 `*_at TEXT` 已改 `TIMESTAMPTZ`，新增 `0069_timestamp_columns_timestamptz.sql` 迁移已有开发库，repository bind/read 已改 `DateTime<Utc>`，`parse_pg_timestamp_checked` 删除，infra `to_rfc3339` grep 为 0；`cargo test -p agentdash-infrastructure`、`cargo check --workspace` 通过；archive 位于 `.trellis/tasks/archive/2026-05/05-29-infra-residual` |
| 6 | `05-29-api-handler-thinning` | in-progress | 已补齐 `design.md` / `implement.md`；`session_use_cases` 迁移 slice 已提交 `ab52be01`；`canvases.rs` CRUD 已提交 `ee51ce88`；`projects.rs` CRUD 已提交 `54ea4818`；`stories.rs` Story 聚合 CRUD 已提交 `8923888a`；`llm_providers.rs` catalog 已提交 `80a97d6a`；`backends.rs` add/remove/ensure-local-runtime 写命令已提交 `08fc0574`；route direct repo grep 显式剩余 runtime read projection adapter；`Json<serde_json::Value>` / `Json<Value>` route response grep 已清零并提交 `0b0b8d78`；API-only DTO 第一批搬到 `agentdash-api/src/dto` 后 inline route DTO 从 109 降到 98；`task_execution` DTO 搬迁后 inline DTO 降到 93；file picker / terminal / vfs / discovered options / skill asset blob DTO 搬迁后 inline DTO 降到 80；`cargo check -p agentdash-api` 通过；下一步继续处理业务 HTTP DTO 与 router 拆分 |
| 7 | `05-29-capability-state-unify` | 待小闭环 | `hooks::CapabilityDelta` 并入 `SetDelta`；trait merge 争议有新证据结论 |
| 8 | `05-29-frontend-server-state-refactor` | 待执行 | server-state 真迁 react-query；active project 单源；跨 store 命令式耦合清理；目标 god component 拆分 |
| 9 | `05-29-session-assembly-converge` | 待复核/拆分 | resolver 争议完成复核；builder/compose helper 拆分；VFS 单存储派生有明确落地或证据结论 |
| 10 | `05-29-structural-splits` | 待 design | `agentdash-application-ports` crate 存在；session 目录按职责重排；重叠前端/session 项已从本 child 排除或交叉标注 |
| 11 | `05-29-domain-purification` | 待 contract | domain `ts-rs/schemars` 移除；session id 假 alias 消失；contracts 生成仍完整 |
| 12 | 父级集成 review | 待所有 child | wave2 parent AC 全部逐条验收；第一波三个 reopen child 已给出最终结论；父任务可归档 |
| 13 | （用户补充）整理远端pr合并 | 首尾和补充 | 将远端 #pr38 拉到本地完成 merge，解决相关模块重构和当前项目重构产生的冲突；包含 migrations（需重排序号） & 模块约束处理相关，随后推送至远端创建一个新的重构分支pr   |

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

1. 继续阶段 6：inline DTO 搬迁；当前 `Json<Value>` route response 已清零，inline route DTO 仍有 80 处。
2. DTO 搬迁后拆 `routes.rs`：当前 `rg -c "pub fn router" crates/agentdash-api/src/routes` 无命中，根 router 仍为单表组合。
3. 收尾前修复既知 application test-only `SessionStoreError` 编译债务，并运行 child PRD 指定验收命令。

## 今日子代理可行性复核

- `api-handler-thinning`：DTO/router 收尾可行；最后 baseline 为 `Json<Value>` 26 处，已在本轮清零；inline route DTO 仍 109 处。下一批先搬 `projects/stories/workspaces/acp_sessions/workflows/project_agents/project_sessions/story_sessions/task_execution` 到 contracts，再搬 `backend_access/backends/routines/settings/canvases`，最后拆 router。
- `frontend-server-state-refactor`：React Query 已接入但采用面很窄；建议先迁 `llmProviderStore`，再迁 `routineStore`，再清 `eventStore.activeProjectId` 与跨 store `getState()`，最后处理 `workflowStore.selectedActivityKey`；`storyStore` 延后。
- `capability-state-unify`：小闭环可行；`hooks::CapabilityDelta` 与 `connector::capability_delta::SetDelta` 结构重复，先把 `CapabilityDelta` 合并到 `SetDelta` / `SetDelta::compute`；`CapabilityDimensionModule` 与 `DimensionDelta` 分属 validate/replay 与 render/section 两条轴，trait merge 推迟。
- `session-assembly-converge`：不适合直接抽完整 resolver；建议先拆 `SessionAssemblyBuilder` 到独立文件，再拆 `compose_owner_bootstrap` / `compose_story_step` helper，最后设计 `surface.vfs` / `context_projection.vfs` 单存储派生 accessor。
- `domain-purification`：DDD 方向确认：domain 不引用 contract/protocol DTO，contract/API/protocol 层依赖 domain 并转换。当前关键风险是 `agentdash-contracts::workflow` 直接 re-export domain workflow 类型；先在 contracts 侧复制/定义 workflow wire DTO 并保留 domain -> contract mapper，再移除 domain 的 `ts-rs` / `schemars`。
- `structural-splits`：可行但需控制依赖上限；第一批只迁 `backend_transport.rs` 这类纯 port，第二批只迁 `ExtensionRuntimeActionTransport` / `ExtensionRuntimeChannelTransport` / error，第三批只迁 `VfsMaterializationTransport`；provider/use case/service 留 application，避免新 crate 反向依赖 application。

## 全局验收 Gates

- 每个 child 完成时运行其 PRD 中的 grep/count AC。
- Rust 相关 child 最终运行 `cargo check --workspace`。
- 当前已知测试构建债务：`cargo test -p agentdash-application canvas::management` 会先编译 application 全量 test target，并命中既存 session persistence test/mock 仍返回 `std::io::Error`、未同步 `SessionStoreError` 的错误；后续 child 收尾前需要修复这批 test-only impl。
- 前端相关 child 最终运行 `pnpm -C packages/app-web exec tsc --noEmit`。
- contract 相关 child 运行 `pnpm run contracts:check`。
- 涉及 DB schema 的 child 需要 migration，并验证 migration up。
- 缩窄 scope 时在 child PRD 的豁免/结论区域与 journal 中同步记录理由和复核建议。
