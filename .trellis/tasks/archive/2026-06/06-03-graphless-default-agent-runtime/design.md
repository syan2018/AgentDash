# Design

## Architecture

本任务把默认 Agent runtime 与显式 Activity workflow 分离：

- `LifecycleRun` 增加 `LifecycleRunTopology`，值为 `graphless` / `workflow_graph`。
- `root_graph_id` 仅在 `workflow_graph` 拓扑中存在；graphless run 允许为空。
- `WorkflowGraphInstance` 仅服务显式 workflow graph；graphless run 不创建 graph instance。
- `AgentAssignment` 仅表示 Activity attempt 到 AgentFrame 的绑定；graphless run 不创建 assignment。
- `RuntimeSessionExecutionAnchor`、`AgentFrame` 的 optional graph/activity 字段继续保留，graphless anchor 写入 `None`。

目标控制面：

```text
Graphless default
SubjectRef? -> LifecycleRun(topology=graphless)
  -> LifecycleAgent
  -> AgentFrame(graph_instance_id=None, activity_key=None)
  -> RuntimeSessionExecutionAnchor(graph_instance_id=None, assignment_id=None)
  -> RuntimeSession

Explicit workflow graph
WorkflowGraph -> WorkflowGraphInstance -> ActivityAttempt
  -> AgentAssignment
  -> LifecycleAgent / AgentFrame
  -> RuntimeSessionExecutionAnchor(assignment_id=Some)
```

## Dispatch Flow

- `WorkflowGraphRef` 在 dispatch intents 与 internal dispatch plan 中改为 `Option<WorkflowGraphRef>`。
- ProjectAgent / Story / Task / Routine / Companion default builders set graph ref to `None`.
- Explicit `default_lifecycle_key` continues to resolve via `WorkflowGraphResolver`.
- `LifecycleDispatchService::dispatch_common` branches by graph ref:
  - graphless: resolve/create run, create/reuse agent, create frame, create runtime session, write anchor, create subject association, return result with no graph instance / assignment.
  - workflow graph: existing flow remains, including graph instance activity state and entry assignment.
- `start_lifecycle_run` remains graph-only because it explicitly starts a workflow graph process.

## Data Contracts

- Domain:
  - `LifecycleRun.root_graph_id: Option<Uuid>`.
  - Add `LifecycleRunTopology` enum with `Graphless` and `WorkflowGraph` variants serialized as `graphless` / `workflow_graph`.
  - `WorkflowGraphInstance.graph_id` stays non-null; graphless run never creates workflow graph instances.
  - `SubjectExecutionDispatchResult.assignment_ref: Option<Uuid>`.
  - `InteractionGateOpenedDispatchResult.assignment_ref: Option<Uuid>`.
  - `RoutineDispatchRefs.assignment_id: Option<Uuid>`.
  - Task execution result assignment fields become optional; Task cancel graph instance field becomes optional.
- Database:
  - Update `0001_init.sql` baseline so `lifecycle_runs.root_graph_id` allows null and adds `topology text NOT NULL`.
  - Add a lifecycle run topology check constraint: graphless requires `root_graph_id IS NULL`; workflow graph requires `root_graph_id IS NOT NULL`.
  - Keep `lifecycle_workflow_instances.graph_id` non-null.
  - Keep `agent_assignments` graph fields non-null because assignments remain Activity-only.
  - Remove `project_agents.default_procedure_key` only if it exists in schema; currently only request DTO carries it.
- HTTP / TS contracts:
  - `LifecycleRunView` exposes `topology` and optional `root_graph_id`.
  - `WorkflowGraphInstanceView.graph_id` remains required if instances remain graph-only.
  - ProjectAgent create / update request removes `default_procedure_key`.
  - ProjectAgent launch and task/routine responses expose optional assignment refs.

## ProjectAgent And Procedure Semantics

- ProjectAgent keeps `default_lifecycle_key` as the only lifecycle override.
- `default_procedure_key` is removed from contracts, API route parsing, frontend payloads, stores, and UI.
- `resolve_lifecycle_key_for_project_agent` should be reduced to lifecycle-key validation only, or renamed accordingly.
- Auto-generated `auto:{procedure}` WorkflowGraph creation is deleted.
- `AgentProcedure` remains the explicit Activity executor contract used by `ActivityExecutorSpec::Agent`.

## Freeform Removal

- Remove runtime calls to `FreeformLifecycleService::ensure_definition` from Project creation and boot reconcile.
- Delete the boot reconcile freeform ownership phase.
- Delete `workflow/freeform.rs` from production modules and remove its exports; tests that need a graph fixture should define a local test helper.
- Remove default `WorkflowGraphRef::ByKey(builtin.freeform_session)` from ProjectAgent, Story, Task, Routine, and Companion default builders.
- The recently committed stopgap can be reverted as part of this task rather than preserved.

## Projection And Frontend Behavior

- Run views must not require graph instance rows.
- Frontend lifecycle stores should treat graphless runs as agent/runtime rows with no Activity timeline.
- UI should not show freeform workflow cards or graph nodes for graphless runs.
- Workflow editor and Activity lifecycle pages remain graph-only; graphless runs should not navigate there by graph id.

## Graphless Task And Routine Control

- Graphless Task continue resolves the current execution by `SubjectRef(task)` -> agent-scoped `LifecycleSubjectAssociation` -> `LifecycleAgent.current_frame_id`.
- Graphless Task cancel resolves the same target and sends runtime cancel to the latest runtime session attached to the current frame; it does not project Activity attempt status.
- Explicit Activity Task cancel keeps using assignment / graph instance / activity attempt state.
- Routine reuse targets store `run_id`, `agent_id`, `frame_id`, and optional `assignment_id`; graphless reuse ignores assignment and resumes by run / agent.

## Compatibility And Migration

- No runtime compatibility for old freeform seed data is required.
- Existing developer DB data may be manually deleted/reset.
- Because the project is pre-release, schema source of truth remains the curated baseline migration. Repository SQL, domain structs, contracts, and frontend generated types must match that baseline.

## Risks

- Many paths currently assume `assignment_ref` is present for subject execution. The implementation must update task/routine/cancel/reuse paths together.
- `LifecycleRun.root_graph_id` appears in projection and repository query helpers; all callers must handle `None`.
- Contract generation and frontend store updates are required in the same implementation to avoid cross-layer drift.
