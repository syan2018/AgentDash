# Lifecycle Step 兼容残留全量清理 — PRD

## 背景

`codex/lifecycle-activity-executor-redesign` 重构把 workflow 运行时模型从 step/node 收敛到 Activity，相对 main 净增 ~9k 行。Activity 模型已经是 domain / application / persistence / API DTO / 前端 type 的主表达，但**对外契约层**仍残留旧 step 兜底路径：

- 后端 `WorkflowRun` API DTO 同时序列化 `step_states[]` 和 `activity_state`
- 前端 `WorkflowRun.step_states` 字段、mapper、`lifecycle-session-view.tsx` / `ContextOverviewTab.tsx` / `SessionPage.tsx` 走双路径渲染
- 前端 store / editor 内部命名仍是 `stepKey` / `selectStep` / `addLifecycleEditorStep` / `workflowDraftsByStepKey`，与外部类型 (`activities[]`) 形成"语义=Activity，命名=Step"的术语断层

后端深层投影（`projection.rs::ActiveWorkflowProjection.active_step` 反向构造 `LifecycleStepDefinition`、`step_activation.rs`、`vfs/provider_lifecycle.rs`、`hooks/provider.rs`/`workflow_contribution.rs`、`LifecycleRun.step_states` 字段）是**内部兼容投影**，不是 fallback，迁移代价大且需要替代品。本次任务**不**触碰这部分，明确归入后续独立任务。

## 目标

让"新 Activity 模型是唯一对外契约"，所有用户可见或前端可见的 step fallback 路径退场。

## 范围

### In scope

1. **API DTO**：`/lifecycle-runs/*` 与 `/sessions/*/workflow-runs` 等返回 `WorkflowRun` 的接口停止序列化 `step_states` 字段（保持 schema 向前兼容：字段缺失而非空数组）。
2. **前端类型**：`packages/app-web/src/types/workflow.ts` 删除 `WorkflowStepState`、`WorkflowStepExecutionStatus`、`WorkflowRun.step_states`；保留 `LifecycleExecutionEntry` 等仍在用的类型。
3. **前端 mapper**：`services/workflow.ts` 删除 `mapWorkflowStepState` 与对 `step_states` 的解析。
4. **前端 store 重命名**（仅内部）：
   - `selectedStepKey` → `selectedActivityKey`
   - `selectLifecycleStep` → `selectLifecycleActivity`
   - `updateLifecycleEditorStep` → `updateLifecycleEditorActivity`
   - `updateStepWorkflowDraft` → `updateActivityWorkflowDraft`
   - `addLifecycleEditorStep` → `addLifecycleEditorActivity`
   - `removeLifecycleEditorStep` → `removeLifecycleEditorActivity`
   - `cloneWorkflowIntoStep` → `cloneWorkflowIntoActivity`
   - `workflowDraftsByStepKey` → `workflowDraftsByActivityKey`
   - `LifecycleDraftSeed.initial_step_key` 字段移除（保留 `initial_activity_key`）
   - `createStepWorkflowDraft` → `createActivityWorkflowDraft`
5. **前端渲染**：`lifecycle-session-view.tsx`、`ContextOverviewTab.tsx`、`SessionPage.tsx` 移除 `step_states` 三元/`??` fallback 分支，统一从 `activity_state.attempts` 读取。
6. **DAG 节点**：`ui/dag-node.tsx` 的 `stepKey` prop 重命名为 `activityKey`，`ui/lifecycle-dag-canvas.tsx` 的相关回调跟进。
7. **测试更新**：`workflowStore.test.ts`、`lifecycle-editor-shell.test.tsx`、`step-inspector.test.tsx`、`SessionPage.hook-runtime.test.tsx`、`workflow.test.ts` 跟进类型/接口重命名。

### Out of scope（明确归入后续独立任务）

- 后端 domain 层 `LifecycleRun.step_states`、`LifecycleDefinition.entry_step_key/steps/edges` 字段及其状态机方法
- `agentdash-application/src/workflow/projection.rs` 的 `LifecycleStepDefinition` 反向构造
- `step_activation.rs`、`vfs/provider_lifecycle.rs`、`hooks/provider.rs`/`workflow_contribution.rs` 的 step 心智
- DB migration 删除旧列（在 domain 层退场后再做）
- 前端 Lifecycle Editor 的 UX 重设计（独立任务 `05-21-lifecycle-editor-activity-redesign`）
- 前端 Lifecycle Runtime View 的 UX 重设计（独立任务 `05-21-lifecycle-runtime-view-activity-redesign`）

## 验收标准

1. `git grep -n "step_states" packages/app-web/src` 仅在 archive/changelog 出现，运行代码无引用。
2. 前端类型文件不再导出 `WorkflowStepState` / `WorkflowStepExecutionStatus`；`WorkflowRun` 没有 `step_states` 字段。
3. `lifecycle-session-view.tsx`、`ContextOverviewTab.tsx`、`SessionPage.tsx` 中无 `??` / `?:` 形式的 step fallback 分支，统一从 `activity_state.attempts` 读取。
4. store 公共函数命名全部 `*Activity*`，旧命名彻底删除（不留 alias）。
5. `pnpm --filter app-web typecheck`、`pnpm --filter app-web lint`、`pnpm --filter app-web test workflow`、`pnpm --filter app-web test sessionpage` 全部通过。
6. `cargo check -p agentdash-api` 通过；新增/修改的后端 DTO 测试通过。
7. 浏览器手动验证：单 activity 与多 activity lifecycle 的运行视图展示正常，artifact 面板正常，human decision 提交流程正常。
8. 完成 spec 更新：`.trellis/spec/frontend/workflow-activity-lifecycle.md` 增补"对外契约层不再保留 step fallback"段落。

## 风险

- **API schema 变更**：移除 `step_states` 是契约缩窄，旧客户端/旧前端版本若读取该字段会拿到 `undefined`。当前 monorepo 内只有 `app-web` 一个消费者，可控。
- **测试漏洞**：仅依赖 mocked `step_states` 的旧测试需要重写为 `activity_state` 形态。
