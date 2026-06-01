# WorkflowGraph / LifecycleRun Frontend Contract

前端运行观察的目标模型是 `LifecycleRunView -> WorkflowGraphInstanceView -> ActivityState / ActivityAttemptState`，并通过 `LifecycleAgentView`、`AgentFrameRuntimeView`、`SubjectExecutionView` 进入 runtime trace。前端不以 RuntimeSession id 或单 graph id 作为 lifecycle 主索引。

## Invariants

- 前端读写 `WorkflowGraph` definition；当前 `ActivityLifecycleDefinition` 是迁移来源。
- Run view 必须支持同一个 `LifecycleRun` 下多个 `WorkflowGraphInstance`。
- Frontend store normalize by run、graph instance、subject、agent、frame。
- `/session/:id` 是 `RuntimeTraceView`，不是业务 runtime root。
- Read view 不能作为 command input；write command 使用 `ExecutionIntent`、`SubjectRef`、run/graph/agent/frame refs。
- `session_id` 只在 runtime trace refs 中出现，不能作为 lifecycle 主键。

## Target Views

| View | 用途 |
| --- | --- |
| `LifecycleRunView` | 展示 run、graph instances、agents、gates、subject associations 与 runtime trace refs |
| `WorkflowGraphInstanceView` | 展示某个 graph instance 的 role、status、activities、attempts、artifact state |
| `LifecycleAgentView` | 展示 run 内 agent identity、role、status、lineage、current frame |
| `AgentFrameRuntimeView` | 展示 frame revision 的 procedure、capability、context、VFS、MCP、runtime refs |
| `SubjectExecutionView` | 展示业务 subject 的执行投影，例如 current agent、latest attempt、artifacts |
| `RuntimeSessionTraceView` | 展示 event stream、turns、tools、projection、debug、lineage |

## API Surface

Definition APIs can keep current route names during migration, but generated frontend types should converge on target naming:

```ts
fetchWorkflowGraphs(opts): Promise<WorkflowGraph[]>
createWorkflowGraph(input): Promise<WorkflowGraph>
updateWorkflowGraph(id, input): Promise<WorkflowGraph>
validateWorkflowGraph(input): Promise<WorkflowValidationResult>
submitHumanDecision(input): Promise<LifecycleRunView>
```

Run / subject APIs:

```ts
fetchLifecycleRun(runId): Promise<LifecycleRunView>
fetchSubjectExecution(subjectRef): Promise<SubjectExecutionView>
fetchAgentFrameRuntime(frameId): Promise<AgentFrameRuntimeView>
fetchRuntimeTrace(runtimeSessionId): Promise<RuntimeSessionTraceView>
```

## Definition Contract

Definition request/response fields:

- `entry_activity_key: string`
- `activities: ActivityDefinition[]`
- `transitions: ActivityTransition[]`
- `ActivityDefinition.executor.kind: "agent" | "function" | "human"`
- Agent executor carries `procedure_ref` / `procedure_policy` and agent policy.
- Human approval carries `form_schema_key`; completion policy carries `decision_port`.
- Transition condition supports `always` and explicit condition variants owned by backend workflow schema.

## Run Contract

`LifecycleRunView` exposes graph instances instead of a single workflow run:

```ts
type LifecycleRunView = {
  id: string
  status: LifecycleRunStatus
  workflow_graph_instances: WorkflowGraphInstanceView[]
  agents: LifecycleAgentView[]
  subject_associations: LifecycleSubjectAssociationDto[]
  runtime_trace_refs: RuntimeSessionRef[]
}
```

`WorkflowGraphInstanceView.activity_state.attempts[]` contains `graph_instance_id`、`activity_key`、`attempt`、`status`、`assignment_ref?`、`executor_run?`。

## Store Boundary

- `workflowStore` / lifecycle store indexes runs by `run_id`。
- graph instances indexed by `graph_instance_id`。
- subject execution indexed by `subject_kind + subject_id`。
- agents indexed by `agent_id` and frames by `frame_id`。
- Runtime trace store indexed by `runtime_session_id` only for debug / trace drill-down。

Lifecycle primary state is indexed by run / graph instance / subject / agent / frame.

## Mapper Boundary

- Unknown enum value -> throw with field-specific message。
- Required object field missing -> throw at mapper boundary。
- Validation endpoint returns `issues[]`; editor stores `WorkflowValidationResult` and blocks save on error severity。
- Human decision submit API error keeps current run visible and lets caller reload through store polling。
- Nullable `session_id` from legacy DTO must not become a required business key.

## Editor Model

WorkflowGraph editor selection is owned by store:

```ts
type WorkflowGraphSelection =
  | { kind: "activity"; activityKey: string }
  | { kind: "transition"; transitionId: string }
  | null
```

Store actions that mutate graph draft should preserve API isomorphism:

- activity CRUD updates `activities`
- transition CRUD updates `transitions`
- entry updates `entry_activity_key`
- graph draft index remains keyed by activity

## Related Specs

- [Backend Workflow Architecture](../backend/workflow/architecture.md)
- [WorkflowGraph Activity Backend Contract](../backend/workflow/activity-lifecycle.md)
- [Lifecycle Edge](../backend/workflow/lifecycle-edge.md)
- [Story / Task Runtime](../backend/story-task-runtime.md)
