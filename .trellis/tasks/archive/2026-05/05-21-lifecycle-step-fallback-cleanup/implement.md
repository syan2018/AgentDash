# Lifecycle Step Fallback Cleanup — Implementation Plan

## Order of operations

按依赖关系顺序推进：① 后端 DTO 收紧 → ② 前端类型层删除 → ③ mapper / store 重命名 → ④ 渲染层切到 Activity-only → ⑤ 测试与 spec。

每个 step 后跑对应的 typecheck，修红再走下一步。

## Steps

### 1. 后端 — `LifecycleRun.step_states` 不再上线

- [ ] 在 `crates/agentdash-domain/src/workflow/entity.rs` 的 `LifecycleRun.step_states` 字段加 `#[serde(skip_serializing)]`
- [ ] 在 `crates/agentdash-api/src/routes/workflows.rs` 或新增 `crates/agentdash-api/tests/workflows.rs` 加测试，断言 `serde_json::to_value(&run)` 不含 `"step_states"` key
- [ ] `cargo check -p agentdash-domain && cargo check -p agentdash-api && cargo test -p agentdash-api`

### 2. 前端类型层

- [ ] 编辑 `packages/app-web/src/types/workflow.ts`：
  - 删除 `WorkflowStepExecutionStatus`、`WorkflowStepState`
  - 从 `WorkflowRun` 删除 `step_states` 字段
  - 检查 `index.ts` 是否 re-export 这些类型（如有，同步删除）

### 3. 前端 mapper

- [ ] 编辑 `packages/app-web/src/services/workflow.ts`：
  - 删除 `mapWorkflowStepState` 函数
  - `mapWorkflowRun` 删除 `step_states: asRecordArray(raw.step_states).map(mapWorkflowStepState)` 行
  - 检查并删除 `WorkflowStepState` import

### 4. 前端 store 重命名

- [ ] 编辑 `packages/app-web/src/stores/workflowStore.ts`：
  - 字段 / 方法 / 局部变量按 design.md 表格全量改名
  - `LifecycleDraftSeed` 删除 `initial_step_key`，仅保留 `initial_activity_key`
  - `createEmptyLifecycleDraft` 内 `seed.initial_step_key ?? ...` 删掉，仅读 `initial_activity_key`
  - 函数 `createStepWorkflowDraft` 重命名为 `createActivityWorkflowDraft`，参数 `stepKey` → `activityKey`
  - 注释里的 "step" 措辞改为 "activity"
- [ ] 跟进 callsite：
  - `packages/app-web/src/features/workflow/lifecycle-editor-shell.tsx`
  - `packages/app-web/src/features/workflow/ui/lifecycle-dag-canvas.tsx`
  - `packages/app-web/src/features/workflow/ui/step-inspector.tsx`
  - `packages/app-web/src/pages/LifecycleEditorShellPage.tsx`（`seedInitialStepKey` 改名为 `seedInitialActivityKey`，向 store 传 `initial_activity_key`）
- [ ] DAG node 类型：`packages/app-web/src/features/workflow/ui/dag-node.tsx` 中 `WorkflowStepData.stepKey` → `activityKey`

### 5. 前端渲染层去 fallback

- [ ] 编辑 `packages/app-web/src/features/workflow/lifecycle-session-view.tsx`：
  - 删除 `LifecycleNodeCard` 组件和 `nodeStatusBadgeClass` 中专为 step 设计的分支（合并到 ActivityAttemptCard 用的同名函数即可）
  - `LifecycleProgressBar` 签名改为 `({ attempts }: { attempts: ActivityAttemptState[] })`，按 attempt status 统计
  - 主组件 `LifecycleSessionView`：
    - 删除 `activeRun.activity_state ? ... : activeRun.step_states.map(...)` 二选一渲染，统一走 attempts；`activity_state == null` 时显示空状态
    - `LifecycleProgressBar` 调用改为传 attempts
- [ ] 编辑 `packages/app-web/src/features/workspace-panel/ContextOverviewTab.tsx`：
  - `run.step_states.find(...)` 全部改为基于 `run.activity_state?.attempts`
  - `completedCount/totalCount` 改为基于 attempts
  - 新增 helper：`pickActiveAttempt(attempts: ActivityAttemptState[]): ActivityAttemptState | null`，按 running/claiming → ready → latest 顺序
- [ ] 编辑 `packages/app-web/src/pages/SessionPage.tsx` L461：
  - `run.step_states.some((step) => Boolean(step.session_id))` → `run.activity_state?.attempts.some((a) => a.executor_run?.kind === "agent_session" && Boolean(a.executor_run.session_id)) ?? false`

### 6. 测试更新

- [ ] `packages/app-web/src/stores/workflowStore.test.ts`：所有 `initial_step_key:` 改为 `initial_activity_key:`，被重命名的 store 方法跟进
- [ ] `packages/app-web/src/features/workflow/lifecycle-editor-shell.test.tsx`：同上
- [ ] `packages/app-web/src/features/workflow/ui/step-inspector.test.tsx`：检查是否有依赖 `selectedStepKey` 等旧 API
- [ ] `packages/app-web/src/pages/SessionPage.hook-runtime.test.tsx`：mock 的 `step_states: [...]` 改为 `activity_state: { attempts: [{ activity_key: "check", attempt: 1, status: "running", executor_run: { kind: "agent_session", session_id: "..." } }], outputs: [], inputs: [], status: "running" }`
- [ ] `packages/app-web/src/services/workflow.test.ts`：删除 step_states 相关断言

### 7. Spec & memory

- [ ] `.trellis/spec/frontend/workflow-activity-lifecycle.md` 增补 Scenario「Activity is the only on-the-wire run state」
- [ ] memory 中如有相关记录跟进（无则跳过）

### 8. Verification gates

- [ ] `cargo check -p agentdash-api`
- [ ] `cargo test -p agentdash-api`
- [ ] `pnpm --filter app-web typecheck`
- [ ] `pnpm --filter app-web lint`
- [ ] `pnpm --filter app-web test workflow`
- [ ] `pnpm --filter app-web test sessionpage`
- [ ] `pnpm --filter app-web test session`（覆盖 ContextOverviewTab）
- [ ] **不**自动 commit；等待用户做浏览器手动验收

## Rollback points

每个 step 都是独立改动；rollback 只需 git checkout 对应文件。后端 `#[serde(skip_serializing)]` 是单行修改，最易回退。

## 已知风险

- `LifecycleEditorShellPage.tsx` 路由参数名可能仍然是 `seedInitialStepKey`（URL 兼容），改动需注意路由层是否要保留旧 query name；建议保留 URL query 名，仅在内部转译为 `initial_activity_key`
- `WorkflowRun.activity_state` 在 freeform 补齐之前可能为空。LifecycleSessionView 的"空状态"分支必须明确这是边界态，不是降级
