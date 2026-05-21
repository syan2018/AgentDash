# Lifecycle Step Fallback Cleanup — Design

## 设计总则

「内部兼容投影」与「对外契约 fallback」必须分开处理。本任务只动**对外契约**：HTTP API 序列化形态、前端类型/渲染/store 命名。后端 domain `LifecycleRun.step_states` 字段、`LifecycleStepDefinition` projection、`step_activation/vfs/hooks` 的 step 心智都保持现状，由后续 domain-level cleanup 任务整体替换。

## 后端：API DTO 不再序列化 step_states

### 现状

- `crates/agentdash-api/src/routes/workflows.rs` 直接 `Json<LifecycleRun>` 返回 domain entity
- `LifecycleRun` 在 `crates/agentdash-domain/src/workflow/entity.rs:294` 定义 `pub step_states: Vec<LifecycleStepState>`，参与 `Serialize/Deserialize`
- 持久化 `workflow_repository.rs` 把 `step_states` 序列化到独立 JSON 列：`serde_json::to_string(&run.step_states)`，**不**通过 parent struct 整体序列化

### 决策

在 `LifecycleRun.step_states` 字段上加 `#[serde(skip_serializing)]`：

- axum `Json<LifecycleRun>` 序列化时跳过该字段 ✓
- 持久化层显式序列化单字段不受影响 ✓
- HTTP 入参不会传 `step_states`（路由用独立请求 DTO），因此无需 `skip_deserializing` ✓
- Domain 内部代码路径（`step_activation`、`hooks`、`vfs`）继续读写该字段不受影响 ✓

### 测试

新增/调整 `crates/agentdash-api/tests` 或 `routes/workflows.rs` 的内联测试，断言 `Json<LifecycleRun>` 序列化结果不含 `step_states` key。

## 前端：类型 / mapper / 渲染 / store 全量去 fallback

### 类型层（`packages/app-web/src/types/workflow.ts`）

删除：
- `WorkflowStepExecutionStatus`
- `WorkflowStepState`
- `WorkflowRun.step_states` 字段

保留：
- `LifecycleExecutionEntry` / `LifecycleExecutionEventKind`（`execution_log` 仍在用）
- `WorkflowRun.activity_state`（变为非可空？后端总是返回，但保留 `null` 表示边界态——在 domain freeform 补齐之前可能短暂为空）

`activity_state` 仍保持 `?: ActivityLifecycleRunState | null`，因为 `LifecycleRun.activity_state` 在 domain 中是 `Option<...>`。

### Mapper（`packages/app-web/src/services/workflow.ts`）

- 删除 `mapWorkflowStepState`
- `mapWorkflowRun` 不再读取 `raw.step_states`
- 保留 `mapLifecycleExecutionEntry`

### 渲染层

#### `lifecycle-session-view.tsx`

删除整段 `LifecycleNodeCard`（依赖 `WorkflowStepState`），只保留 `ActivityAttemptCard`。`LifecycleProgressBar` 改为按 `ActivityAttemptState` 统计：

```ts
function LifecycleProgressBar({ attempts }: { attempts: ActivityAttemptState[] }) { ... }
```

`activeRun.activity_state` 为 `null` 时，组件展示空状态卡片（"等待 Activity 状态"）而非 fallback 渲染。

#### `ContextOverviewTab.tsx`

`run.step_states.find(...)` 改为基于 `run.activity_state.attempts` 的 latest-by-status 查询（按 `running` → `ready` → 最新 attempt 顺序）。`completedCount/totalCount` 同样改为基于 attempts。

#### `SessionPage.tsx`

L461 `run.step_states.some((step) => Boolean(step.session_id))` → 基于 `activity_state.attempts.some(a => a.executor_run?.kind === "agent_session")`。

### Store 命名清算（`packages/app-web/src/stores/workflowStore.ts`）

| 旧 | 新 |
|---|---|
| `selectedStepKey` | `selectedActivityKey` |
| `selectLifecycleStep` | `selectLifecycleActivity` |
| `updateLifecycleEditorStep` | `updateLifecycleEditorActivity` |
| `updateStepWorkflowDraft` | `updateActivityWorkflowDraft` |
| `addLifecycleEditorStep` | `addLifecycleEditorActivity` |
| `removeLifecycleEditorStep` | `removeLifecycleEditorActivity` |
| `cloneWorkflowIntoStep` | `cloneWorkflowIntoActivity` |
| `workflowDraftsByStepKey` | `workflowDraftsByActivityKey` |
| `createStepWorkflowDraft` | `createActivityWorkflowDraft` |
| `LifecycleDraftSeed.initial_step_key` | （删除，仅保留 `initial_activity_key`） |

callsites 在 `lifecycle-editor-shell.tsx`、`lifecycle-dag-canvas.tsx`、`step-inspector.tsx`、`LifecycleEditorShellPage.tsx`、各 `*.test.ts(x)` 跟进。

### DAG 节点（`packages/app-web/src/features/workflow/ui/dag-node.tsx`）

`WorkflowStepData.stepKey` → `activityKey`。`lifecycle-dag-canvas.tsx` 中创建 node data 时同步更新。

### Story / Task 字段保持

`packages/app-web/src/types/index.ts`、`stores/storyStore.ts` 中的 `lifecycle_step_key` 是 task/story 元数据字段（来自后端 task 模型），**与 workflow run 的 step_states 无关**，本任务不动。

## Spec 更新

`.trellis/spec/frontend/workflow-activity-lifecycle.md` 增补 Scenario：「Activity 是 WorkflowRun 唯一在线渲染源」，明确删除 `WorkflowRun.step_states`、组件不允许 fallback 到旧字段。

## 影响半径

- 后端：1 处 serde 标注 + 1 个测试断言
- 前端：~14 个 TS 文件改动，其中 `workflowStore.ts`/`lifecycle-editor-shell.tsx`/`lifecycle-session-view.tsx` 是主战场

## 校验门

- `cargo check -p agentdash-api`
- `cargo test -p agentdash-api workflows -- --nocapture`（断言序列化结果）
- `pnpm --filter app-web typecheck`
- `pnpm --filter app-web lint`
- `pnpm --filter app-web test`
- 浏览器手动：单 activity / 多 activity lifecycle run 视图、artifact 面板、human decision 提交
