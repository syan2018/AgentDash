# Slop Cleanup 全程推进 Checklist

> 压缩恢复入口：每次恢复本目标时，先读本文件，再读当前 active child 的 `prd.md` / `design.md` / `implement.md`。本文件记录跨 child 的真实推进顺序、当前状态和验收证据。

## 当前恢复状态

- 当前分支：`refactor/architecture-slop-cleanup`
- 当前 active child：`05-29-contract-pipeline-unify`
- 当前 child 状态：`in_progress`（已启动；正在补齐复杂任务规划后进入实现）
- 当前主线步骤：`error-model-unify` 已提交并归档；下一步先补齐 `contract-pipeline-unify` 缺失的 `design.md` / `implement.md`，然后按契约单源顺序推进实现。
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
- 当前待处理：
  - `contract-pipeline-unify` 的 PRD 已存在，但缺少复杂任务必需的 `design.md` / `implement.md`。
  - 当前 spec 仍描述前端 service mapper 做 `unknown -> typed object` 校验；本 child 已拍板改为内部端点信任 generated wire，因此实现阶段必须同步更新 cross-layer / frontend spec。
  - 仓库未发现 `.github` CI 配置；契约 drift gate 先落到根 `package.json` 的 `check` 链路和 `pnpm run contracts:check`，后续若补 CI 则复用该脚本。

## 全程推进队列

| 顺序 | 任务 | 状态 | 完成证据 |
|---:|---|---|---|
| 0 | `05-29-quickfix-swarm` | 已归档 | archive 中 task completed；quickfix commit 已存在 |
| 1 | `05-29-error-model-unify` | 已归档 | 提交 `c2fb8f78`；归档提交 `8f1d232d`；本 child AC 全满足；`cargo check --workspace` 通过；stringly error grep 清零；无豁免 |
| 2 | `05-29-contract-pipeline-unify` | 进行中：补规划后实现 | `Task/Story/Workspace/Project` 进入 contracts；前端手写类型删除；`contracts:check` 通过；mapper/spec 冲突按“前端信任 wire”同步落 spec |
| 3 | `05-29-mcp-direct-connection-pool` | 待 design | `direct.rs` 每次 connect/cancel 路径消除；连接池失效/重连策略有测试或说明 |
| 4 | `05-29-vfs-dedup` | 待执行 | VFS dispatch 单一 helper；patch executor 单份；`MountProvider` 拆 trait；VFS `to_string()` 抹平显著收敛 |
| 5 | `05-29-infra-residual` | 待 error-model | sqlite 后端移除；TIMESTAMPTZ migration；session port 错误类型化；DB spec 同步当前决策 |
| 6 | `05-29-api-handler-thinning` | 待 error/contract/session | API handler repo 直调下沉；`session_use_cases` 迁 application；`Json<Value>` 和 inline DTO 清零 |
| 7 | `05-29-capability-state-unify` | 待小闭环 | `hooks::CapabilityDelta` 并入 `SetDelta`；trait merge 争议有新证据结论 |
| 8 | `05-29-frontend-server-state-refactor` | 待执行 | server-state 真迁 react-query；active project 单源；跨 store 命令式耦合清理；目标 god component 拆分 |
| 9 | `05-29-session-assembly-converge` | 待复核/拆分 | resolver 争议完成复核；builder/compose helper 拆分；VFS 单存储派生有明确落地或证据结论 |
| 10 | `05-29-structural-splits` | 待 design | `agentdash-application-ports` crate 存在；session 目录按职责重排；重叠前端/session 项已从本 child 排除或交叉标注 |
| 11 | `05-29-domain-purification` | 待 contract | domain `ts-rs/schemars` 移除；session id 假 alias 消失；contracts 生成仍完整 |
| 12 | 父级集成 review | 待所有 child | wave2 parent AC 全部逐条验收；第一波三个 reopen child 已给出最终结论；父任务可归档 |

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

1. 进入实现第一批：core DTO 进入 `agentdash-contracts`，API 改用 contract response。
2. 注册 `generate_ts.rs` 并生成 TS contract。
3. 删除前端 `types/index.ts` 中与 generated 重复的 Project / Workspace / Story / Task wire 类型。
4. 运行 `cargo check -p agentdash-contracts -p agentdash-api` 与 `pnpm -C packages/app-web exec tsc --noEmit`。

## 全局验收 Gates

- 每个 child 完成时运行其 PRD 中的 grep/count AC。
- Rust 相关 child 最终运行 `cargo check --workspace`。
- 前端相关 child 最终运行 `pnpm -C packages/app-web exec tsc --noEmit`。
- contract 相关 child 运行 `pnpm run contracts:check`。
- 涉及 DB schema 的 child 需要 migration，并验证 migration up。
- 缩窄 scope 时在 child PRD 的豁免/结论区域与 journal 中同步记录理由和复核建议。
