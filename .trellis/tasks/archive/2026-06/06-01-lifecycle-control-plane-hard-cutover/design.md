# Lifecycle 控制面硬切清场设计

## Design Principles

- `LifecycleRun` 追踪 execution life process，不承载 RuntimeSession ownership。
- `LifecycleAgent` 是 run 内 Agent runtime identity。
- `AgentFrame` 是 capability / context / VFS / MCP / procedure / runtime refs 的唯一权威事实源。
- `RuntimeSession` 只承载 turn / tool / event / resume / debug trace。
- `AgentAssignment` 是 `LifecycleAgent + AgentFrame` 到 `ActivityAttemptState` 的执行证据桥。
- `LifecycleSubjectAssociation` 是业务 subject 与 run / agent anchor 的唯一关联事实源。
- Projection 可以聚合多事实源，command path 只接受 `ExecutionIntent` 和 stable refs。

## Target Runtime Flow

Agent runtime launch 必须按以下顺序执行：

```text
ExecutionIntent
  -> resolve/create LifecycleRun
  -> resolve/create WorkflowGraphInstance when graph-backed
  -> resolve/create LifecycleAgent
  -> build and persist AgentFrame
  -> create AgentAssignment when launching an Agent Activity
  -> create RuntimeSession from AgentFrame
  -> persist RuntimeSessionRef into AgentFrame revision
  -> write ActivityAttemptState.executor_run = RuntimeSessionRef
  -> return ExecutionDispatchResult with refs
```

Freeform ProjectAgent / manual run 可没有 activity assignment，但必须返回 `run_ref / agent_ref / frame_ref / runtime_session_ref`，并不得写 fake assignment。

Agent Activity execution 必须返回 `assignment_ref`；缺少真实 assignment 是 dispatch error。

## Runtime Creation Ownership

`LifecycleDispatchService` owns orchestration:

- Resolves run / agent / subject / graph / assignment.
- Calls a runtime launch owner with an `AgentFrame`.
- Receives `RuntimeSessionRef`.
- Writes a new frame revision or updates the frame revision with `runtime_session_refs_json`.
- Returns `ExecutionDispatchResult`.

Runtime adapter owns connector-specific delivery only:

- It may construct internal `RuntimeLaunchRequest`.
- It must not resolve Story / Task / Project owner.
- It must not accept `SessionConstructionPlan` as production input.

## Contract Boundary

Add generated contract DTOs under `agentdash-contracts` and remove frontend hand-written target view types:

- Stable refs: run, agent, frame, runtime session, assignment.
- Views: lifecycle run, lifecycle agent, agent frame runtime, subject execution, runtime session trace.
- Launch result: ProjectAgent launch.

Write commands must use stable input:

- `ExecutionIntent`
- `SubjectRefDto`
- launch options / policy refs

Read views must not be accepted as command input.

## API Boundary

Target routes:

```text
POST /projects/{project_id}/agents/{agent_key}/launch
GET  /lifecycle-runs/{run_id}/view
GET  /subjects/{kind}/{id}/execution
GET  /agent-frames/{frame_id}/runtime
GET  /runtime-sessions/{runtime_session_id}/trace
GET/POST/PATCH/DELETE /agent-procedures
GET/POST/PATCH/DELETE /workflow-graphs
```

Remove old routes:

```text
/projects/{id}/agents/{key}/session
/lifecycle-runs/by-session/{session_id}
/lifecycle-runs/{id}/links
/tasks/{id}/session
/workflow-definitions
/activity-lifecycle-definitions
```

## Projection Design

`SubjectExecutionView` construction:

```text
SubjectRef(kind, id)
  -> LifecycleSubjectAssociation by subject
  -> prefer agent-scoped association
  -> LifecycleAgent
  -> current AgentFrame
  -> AgentAssignment(s)
  -> ActivityAttemptState(s)
  -> artifacts / status projection
```

Task status / artifacts may remain as projection cache only if source refs are persisted or the cache can be rebuilt from lifecycle facts.

`RuntimeSessionTraceView` construction:

```text
RuntimeSessionRef
  -> RuntimeSession events / turns / tools / resume state
  -> optional frame ref for navigation metadata only
```

Runtime trace view must not resolve business ownership directly.

## Migration Design

Clean baseline must represent the target schema directly:

- `agent_procedures`, not `workflow_definitions`.
- `workflow_graphs`, not `lifecycle_definitions`.
- `lifecycle_subject_associations`, not `lifecycle_run_links`.
- `lifecycle_runs` without `session_id`, `binding_kind`, or `binding_id`.
- `lifecycle_gates` using the domain/repository schema: `run_id`, optional `agent_id`, optional `frame_id`, `correlation_id`, `status`, `payload_json`, `resolved_by`.
- No `session_bindings`.

Forward migration may perform one-time data movement for developer DBs, but final readiness and clean baseline must not require old tables.

## Failure Modes

- Dispatch fails if runtime creation succeeds but frame refs cannot be persisted.
- Dispatch fails if Agent Activity launch lacks real `agent_id`, `frame_id`, or `assignment_id`.
- RuntimeSession terminal callback fails closed if it cannot resolve frame -> agent -> assignment.
- Frontend fails compile if old session-first contracts are referenced.
- Contracts check fails if target DTOs are not generated.

## Compatibility Position

This task intentionally does not provide compatibility aliases. Any caller still using old routes, old DTO names, run links, or session-first navigation must be migrated or deleted in this task.
