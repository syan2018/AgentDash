# Implementation Plan

## Phase 0: Preserve Current Merge Rehearsal

- [x] 确认当前分支是 `codex/pr33-main-merge-check`，并记录 staged / unstaged 状态。
- [x] 保护已解冲突结果，避免切分支时丢失 `compaction_checkpoint.rs` 与 `context_projector.rs` 的吸附解法。
- [x] 明确新建 Trellis 子任务文件与已有 PR33 merge staged 内容的边界，不把无关文件混入临时保护提交。

## Phase 1: Move Work Back To PR Branch

- [x] `git fetch --prune origin`。
- [x] 切到 `codex/session-tree-branching`，必要时从 `origin/codex/session-tree-branching` 创建 tracking branch。
- [x] 合入 `origin/main`。
- [x] 解决冲突，采用已确认的 checkpoint-layer segment materialization 方案。
- [x] 确认无 unmerged 文件，并执行 `git diff --check`。

## Phase 2: PR Metadata Alignment

- [x] 将 PR #33 base 调整为 `main`。
- [x] 更新 `.trellis/tasks/04-08-session-tree-branching/task.json` 的 base branch / next action / 状态描述。
- [x] 确认本子任务 metadata 的 branch 是 `codex/session-tree-branching`，base branch 是 `main`。

## Phase 3: Backend Semantic Fixes

- [x] 修改 rollback 边界：target event seq 必须不超过当前 projection head。
- [x] 修改 active compaction 解析：只有 committed projection compaction 可继续作为 active head。
- [x] 约束 fork point 与 compaction id：禁止不一致的混合输入，或强校验 boundary 一致。
- [x] 为上述语义增加 focused unit tests 或至少补充现有测试断言；是否执行测试取决于用户是否允许。

## Phase 4: Lineage List Absorption

- [x] 替换项目 session 列表中的 `lineage_view(...).ok()` 静默降级。
- [x] 优先读取直接 parent edge；如新增 bulk API 成本低，则实现 bulk/direct-parent store 方法。
- [x] 确保 API 错误路径保留足够上下文，前端不会收到伪装成 root 的 child session。

## Phase 5: Frontend Relation Absorption

- [x] 调整 session grouping 数据结构，使 parent children 带 relation kind。
- [x] 替换 active session list / shortcut rows 中把所有 parent child 称为 companion 的命名和文案。
- [x] 更新对应前端测试，覆盖 fork / rollback_branch / companion 的展示差异。

## Phase 6: Static Verification And Review

- [x] `git status --short --branch`
- [x] `git diff --name-only --diff-filter=U`
- [x] `git diff --check`
- [x] `rg -n "parent_relation_kind|companions|companion|isCompanion" packages/app-web/src`
- [x] `rg -n "lineage_view|rollback_model_projection|resolve_active_compaction_after_rollback|fork_point_compaction_id" crates`

## Phase 7: Merge Readiness Quality Gate

- [x] `cargo fmt --all --check`
- [x] `pnpm run contracts:check`
- [x] `pnpm run backend:check`
- [x] `pnpm run backend:clippy`
- [x] `pnpm run backend:test`
- [x] `pnpm run frontend:check`
- [x] `pnpm run frontend:lint`
- [x] `pnpm run frontend:test`
- [ ] `pnpm run e2e:test:critical` 的测试进程未进入用例阶段；当前阻塞点是 Playwright webServer 启动参数契约，已按用户指示不纳入本轮修复。

## Risk Points

- 当前工作树已有 PR33 merge rehearsal 的 staged 内容，执行前必须先隔离或保护，避免 Trellis 文件与 PR 代码混成不可读提交。
- PR base 改到 `main` 后 GitHub diff 会重算，需要确认 PR #33 不再显示 PR32 已合入内容。
- Frontend relation rename 可能触及测试快照和本地列表交互，需要保持 scope 聚焦在 relation 语义，不做 UI 大改。
