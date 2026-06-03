# WorkflowGraph / LifecycleRun Frontend Contract

前端运行观察的目标模型以 `LifecycleRunView`、`LifecycleAgentView`、`AgentFrameRuntimeView` 与 `SubjectExecutionView` 为主；`WorkflowGraphInstanceView -> ActivityState / ActivityAttemptState` 只在 `topology="workflow_graph"` 的显式 Activity runtime 中出现。前端不以 RuntimeSession id 或单 graph id 作为 lifecycle 主索引。

## Invariants

- 前端读写 `WorkflowGraph` definition；当前 `ActivityLifecycleDefinition` 是迁移来源。
- 用户层 Workflow 资产入口以 `WorkflowGraph` definition 为主；Agent Activity 关联的 `AgentProcedure` 提供单个 Activity contract，可作为编辑器配套 draft 维护。
- Run view 必须支持同一个 `LifecycleRun` 下 0..N 个 `WorkflowGraphInstance`。
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
| `AgentFrameRuntimeView` | 展示 frame revision 的 procedure、capability、context、VFS、MCP，以及由 anchor read model 投影的 runtime refs |
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
  topology: "graphless" | "workflow_graph"
  root_graph_id?: string | null
  status: LifecycleRunStatus
  workflow_graph_instances: WorkflowGraphInstanceView[]
  active_activity_refs: ActiveActivityRef[]
  agents: LifecycleAgentView[]
  subject_associations: LifecycleSubjectAssociationDto[]
  runtime_trace_refs: RuntimeSessionRef[]
}
```

`topology="graphless"` runs represent ordinary Agent runtime control-plane state and may have `root_graph_id=null` with `workflow_graph_instances=[]`. Activity timeline UI is entered from `topology="workflow_graph"` runs and their graph instances.

`WorkflowGraphInstanceView.activity_state.attempts[]` contains `graph_instance_id`、`activity_key`、`attempt`、`status`、`assignment_ref?`、`executor_run?`。

## Store Boundary

- `workflowStore` owns `WorkflowGraph` definition drafts, validation state, editor selection; Agent Activity 的 `AgentProcedure` draft 是配套 contract 编辑数据。
- `lifecycleStore` owns runtime projections: runs indexed by `run_id`。
- graph instances indexed by `graph_instance_id`。
- subject execution indexed by `subject_kind + subject_id`。
- agents indexed by `agent_id` and frames by `frame_id`。
- Runtime trace store indexed by `runtime_session_id` only for debug / trace drill-down。

Lifecycle primary state is indexed by run / graph instance / subject / agent / frame. Graphless runs still normalize run / subject / agent / frame state even when no graph instance exists.

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
