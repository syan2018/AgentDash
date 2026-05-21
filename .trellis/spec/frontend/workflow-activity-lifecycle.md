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
