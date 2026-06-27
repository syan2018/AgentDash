# WorkflowGraph / LifecycleRun Frontend Contract

前端运行观察的目标模型以 `LifecycleRunView.orchestrations[]`、`OrchestrationInstanceView`、`RuntimeNodeView`、`AgentRunView`、`AgentFrameRuntimeView` 与 `SubjectExecutionView` 为主。前端不以 RuntimeSession id、WorkflowGraph id 或单 graph id 作为 lifecycle 主索引。

## Invariants

- 前端读写 `WorkflowGraph` definition；application compiler 将静态 definition 编译为 `OrchestrationPlanSnapshot` 后进入 runtime。
- 用户层 Workflow 资产入口以 `WorkflowGraph` definition 为主；Agent Activity 关联的 `AgentProcedure` 提供单个 Activity contract，可作为编辑器配套 draft 维护。
- Run view 必须支持同一个 `LifecycleRun` 下 0..N 个 `OrchestrationInstance` 投影。
- Frontend store normalize by run、orchestration、runtime node、subject、agent、frame。
- Runtime node 的稳定坐标是 `orchestration_id + node_path + attempt`；`active_runtime_node_refs` 使用同一坐标。
- `/session/:id` 是 `RuntimeTraceView`，不是业务 runtime root。
- Read view 不能作为 command input；write command 使用 `ExecutionIntent`、`SubjectRef`、run/agent/frame refs 或 runtime node refs。
- `session_id` 只在 runtime trace refs 中出现，不能作为 lifecycle 主键。

## Target Views

| View | 用途 |
| --- | --- |
| `LifecycleRunView` | 展示 run、orchestrations、active runtime nodes、agents、subject associations、runtime trace refs 与 execution log |
| `OrchestrationInstanceView` | 展示某个 orchestration instance 的 role、status、plan digest、source ref、ready nodes 与 runtime node tree |
| `RuntimeNodeView` | 展示 runtime node 的 `node_path`、kind、status、attempt、executor run ref、时间戳与 children |
| `AgentRunView` | 展示 run 内 agent identity、role、status、lineage、current frame |
| `AgentFrameRuntimeView` | 展示 frame revision 的 procedure、capability、context、VFS、MCP，以及由 anchor read model 投影的 runtime refs |
| `SubjectExecutionView` | 展示业务 subject 的执行投影，例如 current agent、latest attempt、artifacts |
| `RuntimeSessionTraceView` | 展示 event stream、turns、tools、projection、debug、lineage |

## API Surface

Definition APIs use `WorkflowGraph` naming and generated frontend types stay aligned with runtime DTO naming:

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

AgentRun command APIs use AgentRun workspace identity as the delivery/control entrypoint:

```ts
submitAgentRunComposerInput(runId, agentId, request: AgentRunComposerSubmitRequest)
listAgentRunMailboxMessages(runId, agentId)
deleteAgentRunMailboxMessage(runId, agentId, messageId)
promoteAgentRunMailboxMessage(runId, agentId, messageId, request)
resumeAgentRunMailbox(runId, agentId, request)
```

These calls target `/agent-runs/{runId}/agents/{agentId}/...`; runtime session remains a delivery ref
inside the workspace snapshot and is not a frontend command owner.

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

`LifecycleRunView` exposes orchestration runtime projections instead of a single workflow run:

```ts
type LifecycleRunView = {
  run_ref: LifecycleRunRefDto
  project_id: string
  topology: "plain" | "workflow_graph"
  status: LifecycleRunStatus
  orchestrations: OrchestrationInstanceView[]
  active_runtime_node_refs: ActiveRuntimeNodeRefDto[]
  agents: AgentRunView[]
  subject_associations: LifecycleSubjectAssociationDto[]
  runtime_trace_refs: RuntimeSessionRef[]
  execution_log: LifecycleExecutionEntry[]
  created_at: string
  updated_at: string
  last_activity_at: string
}

type OrchestrationInstanceView = {
  orchestration_id: string
  role: string
  status: string
  plan_digest: string
  source_ref: unknown
  ready_node_ids: string[]
  nodes: RuntimeNodeView[]
  created_at: string
  updated_at: string
}

type RuntimeNodeView = {
  node_id: string
  node_path: string
  kind: string
  status: string
  attempt: number
  executor_run_ref?: ExecutorRunRef
  started_at?: string
  completed_at?: string
  children: RuntimeNodeView[]
}

type ActiveRuntimeNodeRefDto = {
  run_id: string
  orchestration_id: string
  node_path: string
  attempt: number
  status: string
}
```

`topology="plain"` runs represent ordinary Agent runtime control-plane state with `orchestrations=[]`. Activity timeline UI is entered from `topology="workflow_graph"` runs and their orchestration runtime node tree. Graph-backed provenance is read from `OrchestrationInstanceView.source_ref` and plan metadata, so the UI can display static WorkflowGraph origin without using a run-level graph field.

Runtime node lookup and human gate commands use `orchestration_id + node_path + attempt` as the durable node coordinate.

## Store Boundary

- `workflowStore` owns `WorkflowGraph` definition drafts, validation state, editor selection; Agent Activity 的 `AgentProcedure` draft 是配套 contract 编辑数据。
- `lifecycleStore` owns runtime projections: runs indexed by `run_id`。
- orchestrations indexed by `orchestration_id`。
- runtime nodes indexed by `orchestration_id + node_path + attempt`。
- subject execution indexed by `subject_kind + subject_id`。
- agents indexed by `agent_id` and frames by `frame_id`。
- Runtime trace store indexed by `runtime_session_id` only for debug / trace drill-down。

Lifecycle primary state is indexed by run / orchestration / runtime node / subject / agent / frame. Plain runs still normalize run / subject / agent / frame state even when no orchestration exists.

## Mapper Boundary

- Unknown enum value -> throw with field-specific message。
- Required object field missing -> throw at mapper boundary。
- Validation endpoint returns `issues[]`; editor stores `WorkflowValidationResult` and blocks save on error severity。
- Human decision submit API error keeps current run visible and lets caller reload through store polling。
- Runtime trace refs remain delivery/debug refs and do not become required lifecycle business keys.
- `packages/app-web/src/types/lifecycle-views.ts` is a facade over generated contracts. Ref DTOs
  shared with ProjectAgent, such as `LifecycleRunRefDto`、`AgentRunRefDto`、
  `AgentFrameRefDto`、`RuntimeSessionRefDto`、`SubjectRefDto` and
  `AgentAssignmentRefDto`, are re-exported from `project-agent-contracts`; lifecycle view DTOs
  remain re-exported from `workflow-contracts`.

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
