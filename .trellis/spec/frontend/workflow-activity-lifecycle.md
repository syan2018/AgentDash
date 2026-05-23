# Activity Lifecycle Frontend Contract

Activity lifecycle 是前端 Workflow 资产编辑与运行观察的主模型。前端直接映射后端 snake_case DTO，并以 Activity / Executor / Attempt / Transition 作为编辑和运行视角。

## Invariants

- 前端读写 Activity lifecycle definition，并展示 `LifecycleRun.activity_state`。
- Editor draft fields 与后端 Activity lifecycle DTO 保持同构。
- Run view 只能读取 `activity_state.attempts` / `outputs` / `inputs`。
- `LifecycleRun.step_states` 不上线到 API 或前端。
- 前端 mapper 不解析 `raw.step_states`。
- `activity_state` 为空时显示初始化边界态，不读取 step 字段作为替代路径。

## API Surface

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

## Definition Contract

Definition request/response fields:

- `entry_activity_key: string`
- `activities: ActivityDefinition[]`
- `transitions: ActivityTransition[]`
- `ActivityDefinition.executor.kind: "agent" | "function" | "human"`
- Agent executor carries `workflow_key` and `session_policy`.
- Human approval carries `form_schema_key`; completion policy carries `decision_port`.
- Transition condition supports `always` and explicit condition variants owned by backend workflow schema.

Run response fields:

- `WorkflowRun.activity_state?: ActivityLifecycleRunState | null`
- `activity_state.attempts[]` contains `activity_key`, `attempt`, `status`, `executor_run`
- `activity_state.outputs[]` and `activity_state.inputs[]` are rendered as latest/history artifact views

## Mapper Boundary

- Unknown enum value -> throw with field-specific message.
- Required object field missing -> throw at mapper boundary.
- Validation endpoint returns `issues[]`; editor stores `WorkflowValidationResult` and blocks save on error severity.
- Human decision submit API error keeps current run visible and lets caller reload through store polling.

## Editor Model

Lifecycle editor selection is owned by store:

```ts
type LifecycleSelection =
  | { kind: "activity"; activityKey: string }
  | { kind: "transition"; transitionId: string }
  | null
```

`selectedActivityKey` may exist as a derived compatibility field inside the store, but the source of truth is `selection.kind === "activity"`.

Store actions that mutate lifecycle draft should preserve API isomorphism:

- activity CRUD updates `activities`
- transition CRUD updates `transitions`
- entry updates `entry_activity_key`
- workflow draft index remains keyed by activity

## Activity Is The Only On-The-Wire Run State

Backend `LifecycleRun.step_states` must not appear in wire JSON. Frontend public types should not contain:

- `WorkflowRun.step_states`
- `WorkflowStepState`
- `WorkflowStepExecutionStatus`

UI surfaces that display run state consume:

- `activity_state.attempts`
- `activity_state.outputs`
- `activity_state.inputs`

## Related Specs

- [Backend Workflow Architecture](../backend/workflow/architecture.md)
- [Activity Lifecycle Backend Contract](../backend/workflow/activity-lifecycle.md)
- [Lifecycle Edge](../backend/workflow/lifecycle-edge.md)
