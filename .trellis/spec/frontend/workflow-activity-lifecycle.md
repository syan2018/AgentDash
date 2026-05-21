# Activity Lifecycle 前端契约

> Activity lifecycle 是前端 Workflow 资产编辑与运行观察的主模型。前端直接映射后端 snake_case DTO，并把旧编辑器的节点心智收敛为 Activity / Executor / Attempt / Transition。

## Scenario: Activity Lifecycle Editor And Run View

### 1. Scope / Trigger

- Trigger: 前端读写 Activity lifecycle definition，并展示 `LifecycleRun.activity_state`。
- Scope: `packages/app-web/src/types/workflow.ts`、`services/workflow.ts`、`stores/workflowStore.ts`、`features/workflow/**`。
- Why: UI 创建、运行、观察的是同一套 Activity lifecycle 模型，避免编辑态、保存态、运行态出现不同字段语义。

### 2. Signatures

- `GET /activity-lifecycle-definitions?project_id={id}&binding_kind={kind}`
- `POST /activity-lifecycle-definitions`
- `GET /activity-lifecycle-definitions/{id}`
- `PUT /activity-lifecycle-definitions/{id}`
- `POST /activity-lifecycle-definitions/validate`
- `POST /lifecycle-runs/{run_id}/activities/{activity_key}/attempts/{attempt}/human-decision`

Frontend service signatures:

```ts
fetchActivityLifecycleDefinitions(opts): Promise<ActivityLifecycleDefinition[]>
createActivityLifecycleDefinition(input): Promise<ActivityLifecycleDefinition>
updateActivityLifecycleDefinition(id, input): Promise<ActivityLifecycleDefinition>
validateActivityLifecycleDefinition(input): Promise<WorkflowValidationResult>
submitHumanDecision(input): Promise<WorkflowRun>
```

### 3. Contracts

Definition request/response fields:

- `entry_activity_key: string`
- `activities: ActivityDefinition[]`
- `transitions: ActivityTransition[]`
- `ActivityDefinition.executor.kind: "agent" | "function" | "human"`
- Agent executor carries `workflow_key` and `session_policy`.
- Human approval carries `form_schema_key`; completion policy carries `decision_port`.
- Transition condition MVP supports `always` and `human_decision_equals` in the editor.

Run response fields:

- `WorkflowRun.activity_state?: ActivityLifecycleRunState | null`
- `activity_state.attempts[]` contains `activity_key`, `attempt`, `status`, `executor_run`.
- `activity_state.outputs[]` and `activity_state.inputs[]` are rendered as latest/history artifact views.

### 4. Validation & Error Matrix

- API mapper receives unknown enum value -> throw with field-specific message.
- Activity definition missing required object fields -> throw at mapper boundary.
- Human decision submit returns API error -> component keeps current run visible and lets caller reload through store polling.
- Validation endpoint returns `issues[]` -> editor stores `WorkflowValidationResult` and blocks save on `severity === "error"`.

### 5. Good/Base/Bad Cases

- Good: Plan agent activity -> Approval human activity -> Implement agent activity, with approved/rejected transitions.
- Base: single Agent activity with `spawn_child`, no transitions, executor terminal completion.
- Bad: transition condition references a non-existing activity; backend validation reports the issue and editor displays it.

### 6. Tests Required

- Store: draft create/add/remove/rename updates `activities`, `transitions`, `entry_activity_key`, workflow draft index.
- Service: activity lifecycle mapper preserves executor, completion policy, ports, run activity state.
- UI: inspector renders executor kind, Agent session policy, Human approval fields.
- Model: DAG artifact transition sync maps artifact bindings to Activity ports.
- Command gate: `pnpm --filter app-web typecheck`, `pnpm --filter app-web test workflow`, `pnpm --filter app-web lint`.

### 7. Correct Contrast

Editor draft fields should stay isomorphic to API payloads:

```ts
// Correct: editor draft mirrors backend Activity lifecycle DTO.
{
  entry_activity_key: "plan",
  activities: [{ key: "plan", executor: { kind: "agent", workflow_key, session_policy } }],
  transitions: [{ from: "plan", to: "approval", condition, artifact_bindings: [] }],
}
```

The same rule explains why run view reads `activity_state.attempts` as the only display path. There is no `step_states` fallback.

## Scenario: Lifecycle Designer 信息架构

### 1. Scope / Trigger

- Trigger: 用户进入 lifecycle 编辑器（创建或编辑现有 lifecycle definition）。
- Scope: `packages/app-web/src/features/workflow/lifecycle-editor-shell.tsx`、`features/workflow/ui/{activity-inspector,transition-inspector,lifecycle-dag-canvas,dag-node,ArtifactBindingsEditor,conditions/ConditionEditor}.tsx`、`stores/workflowStore.ts`。
- Why: Lifecycle Designer 已升级为 Activity 一等公民编辑体验，新模型每一项能力都需要可视化覆盖；UI 信息架构必须与 Activity / Transition 的语义边界一致，避免"workflow contract 平铺 + 一个步骤"的旧心智回流。

### 2. Layout & Selection

- 单一布局：左侧 DAG 画布常驻 + 右侧固定 sidebar。**不存在** Form / DAG 双模式，**不存在** sticky_dag 粘性 localStorage。
- selection 模型由 store 持有：
  - `selection: { kind: "activity"; activityKey } | { kind: "transition"; transitionId } | null`
  - `selectedActivityKey` 字段保留为派生（向后兼容），但来源真相是 `selection.kind === "activity"`。
- sidebar 路由（`SidebarRouter` 内部）：
  - `selection.kind === "activity"` → `<ActivityInspector />`
  - `selection.kind === "transition"` → `<TransitionInspector />`
  - `selection === null` → `<LifecycleHeader />`（Lifecycle 顶层信息）
- transition id 派生：`${from}-->${to}#${idx}`（store 内导出 `transitionId(t, idx)`，与 canvas edge id 共用）。

### 3. ActivityInspector 双 tab 信息架构

`<ActivityInspector />` 顶部以 tab 切分两个语义层（Agent activity 时两个 tab 都存在；非 Agent 时只渲染 Activity tab）：

#### Activity tab —— lifecycle 编排视角

1. **Identity**：key / description（主字段常驻）；iteration_policy 与 join_policy 折在 `<details>` "高级（迭代 / 汇聚）"，summary 显示 `iter:N/alias · join:kind` 概要。
2. **Executor**：kind select（agent/human/function；entry activity 时 function disabled）+ per-kind 主字段：
   - Agent：workflow_key + session_policy。
   - Function：function.type select；api_request 渲染 method + url_template；bash_exec 渲染 command + args；body_template / working_directory 折在「高级」。
   - Human：form_schema_key 常驻；title 折在「高级」。
   - 切换 executor.kind 调用 `workflowStore.setActivityExecutor`，store 内部走 `ensurePolicyForExecutor` 自动调整 completion_policy；reset=true 时 inspector 顶部 toast 提示。
3. **Ports & Policy**：input_ports / output_ports（按 contract 来源标记 "标准" 只读，extras 可编辑）+ completion_policy 编辑器（5 种 kind 切换：output_ports / executor_terminal / human_decision / hook_gate / open_ended）。

#### Contract tab（Agent only）—— workflow 资产标准接口视角

Injection / Capability / HookRules / Contract Ports（沿用 panels）。Contract.ports 改动通过 `mergeContractIntoStep` 合并到 activity.ports，保留 activity-extra；activity.ports 改动不回流。
顶部提示卡说明 `workflowDraft.key` 标注，并展示项目内可参考的 workflow 数。

### 4. TransitionInspector

- sticky header：`from → to` + 关闭。
- `transition.kind` switch（flow / artifact）：从 artifact 改成 flow 时 confirm + 调 `setTransitionKind`，store 内部清空 artifact_bindings。
- ConditionEditor 4 种 kind：always / artifact_field_equals / human_decision_equals / agent_signal_equals，按 kind 渲染相应字段（activity / port / path / decision_port / signal_key / value）。
- max_traversals 数字 + 无限复选。
- ArtifactBindingsEditor（仅 kind=artifact 显示）：from_activity select / from_port input+datalist / to_port input+datalist / alias select / 删除 + "添加 binding" 按钮。

### 5. DAG Canvas 节点视觉

- 极简徽章：executor icon（agent indigo / human amber / function emerald）+ activity.key + completion_policy 4 字动词代号（**PORT / TERM / DECI / GATE / OPEN**，分别对应 output_ports / executor_terminal / human_decision / hook_gate / open_ended）。
- entry：ring-2 ring-primary。
- validation 角标：节点右上红色徽章 + 数字（`countValidationIssuesForActivity` 按 issue.field_path 关联）。
- description 截断到两行；其余深度信息（iteration / join / workflow_key / executor 详情）放 native title tooltip。
- edge 视觉：
  - flow=实线、artifact=虚线（strokeDasharray "6 4"）。
  - 颜色按 condition.kind：human_decision_equals=蓝 (`hsl(217 91% 60%)`)；其余按 flow=primary、artifact=border。
  - label：flow=condition 摘要；artifact=binding 摘要（最多 2 条 + "+N"）；max_traversals>1 时附 `↻N`。

### 6. Store actions

新增（与 inspector / canvas 一一对应）：

```ts
selectLifecycleTransition(id | null);
setActivityExecutor(activityKey, executor): { reset, previous } | null;
setActivityCompletionPolicy(activityKey, policy);
setActivityIterationPolicy(activityKey, patch);
setActivityJoinPolicy(activityKey, policy);
updateLifecycleEditorTransition(id, patch);
setTransitionKind(id, kind);  // artifact → flow 清空 bindings
addArtifactBinding(id, binding);
updateArtifactBinding(id, idx, patch);
removeArtifactBinding(id, idx);
```

### 7. Tests Required

- Store: `ensurePolicyForExecutor` 矩阵、`setActivityExecutor` reset 返回值、selection 模型（activity / transition / null）、transition 编辑（kind 切换清空 bindings、bindings 增删改）。
- UI: `activity-inspector.test.tsx` 覆盖 4 种 executor.kind 的字段渲染 + 5 种 completion_policy + iteration/join policy；`transition-inspector.test.tsx` 覆盖 flow/artifact 与 4 种 condition.kind 字段。
- Shell: `lifecycle-editor-shell.test.tsx` 覆盖 selection 路由（不再有 Form/DAG mode 判定测试）。
- Command gate: `pnpm --filter app-web typecheck`、`pnpm --filter app-web test workflow`、`pnpm --filter app-web lint`。

### 8. Out of scope

- hook_gate.hook_key 的 preset 选择器（保持纯字符串输入）。
- form_schema_key 的可视化 schema 编辑（保持纯字符串引用）。
- workflow contract 跨 activity 复用的 "提升为标准接口" 专用 UI。
- 后端 schema / migration。
- 运行时视图重设计（独立任务）。

## Scenario: Activity Is The Only On-The-Wire Run State

### 1. Scope / Trigger

- Trigger: 前端读取 `WorkflowRun` / `LifecycleRun`，以及编辑器 store / 渲染层引用 lifecycle node 概念。
- Scope: `crates/agentdash-domain/src/workflow/entity.rs` (`LifecycleRun.step_states`)、`packages/app-web/src/types/workflow.ts`、`services/workflow.ts`、`stores/workflowStore.ts`、`features/workflow/lifecycle-session-view.tsx`、`features/workspace-panel/ContextOverviewTab.tsx`、`pages/SessionPage.tsx`。
- Why: Activity 模型已收敛为 workflow 运行的唯一对外契约。后端 domain 内部仍在用 `step_states` 服务 step_activation/vfs/hooks 兼容投影，但**不能**以任何形式上线到 API 或前端。

### 2. Contracts

- 后端 `LifecycleRun.step_states` 必须标记为 `#[serde(skip_serializing)]`，确保 axum `Json<LifecycleRun>` 输出不含 `step_states` key。Domain 内部代码可继续读写。
- 前端 `WorkflowRun` 类型不含 `step_states` 字段；不再存在 `WorkflowStepState` / `WorkflowStepExecutionStatus` 类型。
- 前端 mapper 不解析 `raw.step_states`，store 内部命名统一为 `selectedActivityKey` / `workflowDraftsByActivityKey` / `selectLifecycleActivity` / `addLifecycleEditorActivity` / `removeLifecycleEditorActivity` / `updateLifecycleEditorActivity` / `updateActivityWorkflowDraft` / `cloneWorkflowIntoActivity` / `createActivityWorkflowDraft`。
- `LifecycleSessionView`、`ContextOverviewTab`、`SessionPage` 只能读 `activity_state.attempts`；`activity_state` 为空时显示"初始化中"边界态，**禁止 fallback 到旧 step 字段**。

### 3. Validation & Error Matrix

- Domain 序列化测试断言 `serde_json::to_value(&LifecycleRun)` 不含 `"step_states"` key。
- 前端 typecheck 不允许出现 `WorkflowRun.step_states` 引用；任何此类引用代表 spec 违反。

### 4. Tests Required

- `cargo test -p agentdash-domain workflow::entity::tests::lifecycle_run_does_not_serialize_step_states_to_wire`
- `pnpm --filter app-web typecheck`
- `pnpm --filter app-web test workflow`、`pnpm --filter app-web test sessionpage`
