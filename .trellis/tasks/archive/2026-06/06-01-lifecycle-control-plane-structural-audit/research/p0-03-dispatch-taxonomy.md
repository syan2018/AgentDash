# Research: P0-03 Dispatch Taxonomy

- Query: P0-03/P0-04/P0-05/P0-06 dispatch intent taxonomy, assignment_ref, WorkflowGraphRef::ByKey, manual run, Story root/freeform bypass dispatch
- Scope: internal
- Date: 2026-06-01

## Findings

### Files Found

| Path | Description |
|------|-------------|
| `.trellis/tasks/06-01-lifecycle-control-plane-structural-audit/design.md` | Defines dispatch taxonomy, result families, and WorkflowGraphResolver boundary. |
| `.trellis/tasks/06-01-lifecycle-control-plane-structural-audit/implement.md` | Phase 3/5 gates for typed dispatch, ByKey resolution, manual run, Story root/freeform. |
| `.trellis/tasks/06-01-lifecycle-control-plane-structural-audit/structural-analysis.md` | Structural rationale for P0-03/P0-04/P0-05/P0-06 and proposed target boundaries. |
| `.trellis/spec/backend/workflow/activity-lifecycle.md` | Runtime identity and assignment contract: graph_instance_id + activity_key + attempt. |
| `.trellis/spec/backend/workflow/lifecycle-run-link.md` | LifecycleSubjectAssociation contract and SubjectRef/runtime trace query paths. |
| `.trellis/spec/backend/story-task-runtime.md` | Story/Task runtime model and SubjectRef based execution rules. |
| `crates/agentdash-domain/src/workflow/dispatch.rs` | Current single broad ExecutionIntent and ExecutionDispatchResult domain model. |
| `crates/agentdash-application/src/workflow/dispatch_service.rs` | Current LifecycleDispatchService implementation. |
| `crates/agentdash-api/src/routes/project_agents.rs` | ProjectAgent launch route builds ExecutionIntent and calls dispatch. |
| `crates/agentdash-application/src/task/service.rs` | Task start/continue service builds ExecutionIntent and calls dispatch. |
| `crates/agentdash-api/src/routes/task_execution.rs` | Task routes return route-local DTOs without assignment_ref. |
| `crates/agentdash-application/src/companion/tools.rs` | Companion subdispatch builds ExecutionIntent with gate policy and calls dispatch. |
| `crates/agentdash-application/src/routine/dispatch.rs` | Routine DispatchStrategy to ExecutionIntent mapping. |
| `crates/agentdash-application/src/routine/executor.rs` | Routine executor calls dispatch but persists only run/agent/frame refs. |
| `crates/agentdash-api/src/routes/workflows.rs` | Manual lifecycle run route bypasses LifecycleDispatchService. |
| `crates/agentdash-application/src/workflow/activity_run.rs` | ActivityLifecycleRunService creates LifecycleRun/activity_state directly. |
| `crates/agentdash-application/src/workflow/agent_executor.rs` | Scheduler/executor path creates real AgentAssignment from activity claims. |
| `crates/agentdash-api/src/routes/story_runs.rs` | Story run routes are read-only SubjectExecutionView projection. |
| `crates/agentdash-application/src/workflow/freeform.rs` | Freeform built-in graph/procedure definition seeding. |
| `crates/agentdash-application/src/reconcile/boot.rs` | Freeform boot reconcile currently only ensures/scans, no dispatch launch. |
| `crates/agentdash-api/src/session_construction.rs` | Runtime session context query maps frame/agent to Project/freeform context, not Story root. |
| `crates/agentdash-api/src/bootstrap/session_construction_provider.rs` | Runtime launch provider requires AgentFrame before launch and has task/project-agent compose branches. |

### Current Dispatch Path

Current code has already moved past the original P0-03/P0-04 symptom in the task docs: `LifecycleDispatchService` now owns both `workflow_graph_repo` and `assignment_repo` (`crates/agentdash-application/src/workflow/dispatch_service.rs:100`) and `dispatch()` returns `assignment_ref: Some(assignment.id)` (`crates/agentdash-application/src/workflow/dispatch_service.rs:219`).

The current `dispatch()` sequence is:

1. Resolve workflow graph from `intent.workflow_graph_ref` (`crates/agentdash-application/src/workflow/dispatch_service.rs:145`).
2. Resolve or create `LifecycleRun` using the resolved graph id (`crates/agentdash-application/src/workflow/dispatch_service.rs:234`, `crates/agentdash-application/src/workflow/dispatch_service.rs:521`).
3. Resolve or create `WorkflowGraphInstance` based on `RunPolicy` (`crates/agentdash-application/src/workflow/dispatch_service.rs:258`).
4. Resolve or create `LifecycleAgent` (`crates/agentdash-application/src/workflow/dispatch_service.rs:311`).
5. Create `LifecycleSubjectAssociation` when `subject_ref` exists; task is agent-scoped, other subjects run-scoped (`crates/agentdash-application/src/workflow/dispatch_service.rs:292`).
6. Create or attach runtime session (`crates/agentdash-application/src/workflow/dispatch_service.rs:466`).
7. Create initial `AgentFrame` with graph instance id and entry activity key (`crates/agentdash-application/src/workflow/dispatch_service.rs:343`).
8. Create optional lineage and gate (`crates/agentdash-application/src/workflow/dispatch_service.rs:188`, `crates/agentdash-application/src/workflow/dispatch_service.rs:201`).
9. Resolve or create entry `AgentAssignment` for the graph entry activity (`crates/agentdash-application/src/workflow/dispatch_service.rs:360`).

The entry assignment path validates that `workflow_graph.entry_activity_key` exists, tries `find_for_attempt(graph_instance.id, activity_key, 1)`, rejects active attempt ownership conflicts, otherwise creates the next attempt by scanning assignments in the graph instance (`crates/agentdash-application/src/workflow/dispatch_service.rs:368`). Tests now assert assignment creation, ByKey success, and unknown key failure (`crates/agentdash-application/src/workflow/dispatch_service.rs:1093`, `crates/agentdash-application/src/workflow/dispatch_service.rs:1155`, `crates/agentdash-application/src/workflow/dispatch_service.rs:1197`).

This closes the immediate nullable `assignment_ref` symptom for dispatch-created entry activities, but it does not yet close the taxonomy problem: the domain still exposes one broad `ExecutionIntent` with optional `subject_ref`, `parent_run_id`, `parent_agent_id`, `workflow_graph_ref`, `agent_procedure_ref`, and mixed policies (`crates/agentdash-domain/src/workflow/dispatch.rs:111`). The result is still one broad `ExecutionDispatchResult` with optional runtime, assignment, gate, subject execution, and trace refs (`crates/agentdash-domain/src/workflow/dispatch.rs:139`). `WorkflowGraphRef` still has only `ById` and `ByKey`; there is no explicit `InlineFreeform` variant (`crates/agentdash-domain/src/workflow/dispatch.rs:83`).

### Entrypoints That Use Dispatch

- ProjectAgent launch builds `WorkflowGraphRef::ByKey` from `default_lifecycle_key`, constructs broad `ExecutionIntent`, calls `LifecycleDispatchService::dispatch`, then returns run/agent/frame/runtime/assignment/subject refs (`crates/agentdash-api/src/routes/project_agents.rs:146`, `crates/agentdash-api/src/routes/project_agents.rs:156`, `crates/agentdash-api/src/routes/project_agents.rs:173`, `crates/agentdash-api/src/routes/project_agents.rs:228`).
- Task start/continue builds `ExecutionIntent(subject_ref=task)` and calls dispatch (`crates/agentdash-application/src/task/service.rs:117`, `crates/agentdash-application/src/task/service.rs:158`, `crates/agentdash-application/src/task/service.rs:273`). The application result and API DTO still omit assignment_ref (`crates/agentdash-application/src/task/execution.rs:39`, `crates/agentdash-api/src/routes/task_execution.rs:47`, `crates/agentdash-api/src/routes/task_execution.rs:95`).
- Companion subdispatch builds `ExecutionIntent(source=ParentAgent, run_policy=AppendGraph, agent_policy=SpawnChild)` with optional `GatePolicy`, then calls dispatch (`crates/agentdash-application/src/companion/tools.rs:376`, `crates/agentdash-application/src/companion/tools.rs:410`).
- Routine maps `DispatchStrategy` to broad dispatch policies and uses `SubjectRef(kind=routine_execution)` (`crates/agentdash-application/src/routine/dispatch.rs:16`, `crates/agentdash-application/src/routine/dispatch.rs:28`). The executor calls dispatch but persists only `RoutineDispatchRefs { run_id, agent_id, frame_id }`, so assignment_ref is currently dropped by this caller (`crates/agentdash-application/src/routine/executor.rs:218`, `crates/agentdash-application/src/routine/executor.rs:250`).

### Entrypoints That Bypass Dispatch

- Manual lifecycle run route `POST /lifecycle-runs` still constructs `ActivityLifecycleRunService` and calls `start_run` directly (`crates/agentdash-api/src/routes/workflows.rs:101`, `crates/agentdash-api/src/routes/workflows.rs:300`, `crates/agentdash-api/src/routes/workflows.rs:313`, `crates/agentdash-api/src/routes/workflows.rs:320`). It returns `LifecycleRun` instead of a typed dispatch result and then launches ready attempts with empty `lifecycle_key` and `root_runtime_session_id` (`crates/agentdash-api/src/routes/workflows.rs:326`).
- `ActivityLifecycleRunService::start_run` resolves the graph, generates a random graph instance id for in-run activity state, calls `LifecycleRun::new_activity`, and persists only the run (`crates/agentdash-application/src/workflow/activity_run.rs:45`). `with_assignment_repo` is a no-op (`crates/agentdash-application/src/workflow/activity_run.rs:41`). This path creates no `WorkflowGraphInstance` repository row, no subject association, no root agent/frame via dispatch.
- Story run routes only expose GET projections: `/stories/{id}/runs` and `/stories/{id}/runs/active` (`crates/agentdash-api/src/routes/story_runs.rs:29`). They read `SubjectRef::new("story", story_uuid)` and build a view from existing associations (`crates/agentdash-api/src/routes/story_runs.rs:56`, `crates/agentdash-api/src/routes/story_runs.rs:94`). No Story root launch command through dispatch was found.
- Freeform code ensures built-in definitions but does not launch a root run/agent/frame through dispatch (`crates/agentdash-application/src/workflow/freeform.rs:34`, `crates/agentdash-application/src/workflow/freeform.rs:76`). Boot reconcile constructs `FreeformLifecycleService` but currently only iterates projects and leaves `reconciled = 0` (`crates/agentdash-application/src/reconcile/boot.rs:105`, `crates/agentdash-application/src/reconcile/boot.rs:116`).
- Runtime session context query finds the frame from runtime session, then hardcodes owner type to Project and uses `FREEFORM_SESSION_LABEL` for project context planning (`crates/agentdash-api/src/session_construction.rs:15`, `crates/agentdash-api/src/session_construction.rs:48`, `crates/agentdash-api/src/session_construction.rs:64`). The application use case has a Story context query branch (`crates/agentdash-application/src/session/context_query_use_case.rs:65`), but no Story root dispatch/launch connection was found.
- Runtime launch provider refuses launch unless a runtime session already has an `AgentFrame`; it has lifecycle-node, task, project-agent, and direct-request branches, but no Story root owner branch in the provider (`crates/agentdash-api/src/bootstrap/session_construction_provider.rs:72`, `crates/agentdash-api/src/bootstrap/session_construction_provider.rs:82`, `crates/agentdash-api/src/bootstrap/session_construction_provider.rs:139`, `crates/agentdash-api/src/bootstrap/session_construction_provider.rs:144`, `crates/agentdash-api/src/bootstrap/session_construction_provider.rs:153`).

### ByKey Resolution Boundary

`WorkflowGraphRepository` already has the required persistence port: `get_by_id`, `get_by_project_and_key`, and `list_by_project` (`crates/agentdash-domain/src/workflow/repository.rs:30`). Postgres implements `get_by_project_and_key` for workflow graphs (`crates/agentdash-infrastructure/src/persistence/postgres/workflow_repository.rs:195`). Catalog upsert also uses `get_by_project_and_key`, so the repository capability already exists (`crates/agentdash-application/src/workflow/catalog.rs:52`).

Current `LifecycleDispatchService::resolve_workflow_graph` correctly resolves `ById` with project validation and `ByKey` with project validation, and defaults `None` to the persisted freeform graph key (`crates/agentdash-application/src/workflow/dispatch_service.rs:422`, `crates/agentdash-application/src/workflow/dispatch_service.rs:438`, `crates/agentdash-application/src/workflow/dispatch_service.rs:454`). However, per the task design this should become a dedicated `WorkflowGraphResolver` boundary, not a dispatch-local helper: graph identity resolution is catalog/config work, while dispatch should instantiate runtime/control-plane facts from a resolved graph.

The resolver should be placed in the application workflow layer and depend on `WorkflowGraphRepository`. It should expose a small result such as:

```rust
pub struct ResolvedWorkflowGraph {
    pub graph: WorkflowGraph,
    pub source_ref: WorkflowGraphRef,
}
```

Rules:

- `ById` loads by id and validates project/scope.
- `ByKey` loads by project/key and fails on missing.
- Freeform must be explicit, either by adding `WorkflowGraphRef::InlineFreeform` or by a separate typed `FreeformGraphRequest` that calls `FreeformLifecycleService::ensure_definition` before resolution.
- Dispatch must not generate graph definition identity.

### Minimal High-Cohesion Design

The first structural move should split intent/result shape without trying to refactor the whole lifecycle engine in the same batch.

Recommended target families from task design:

- `LifecycleRunStartIntent`: creates tracked life process and graph instance. Required result: `RunStarted { run_ref, graph_instance_ref }`, plus optional root launch if explicitly requested.
- `AgentLaunchIntent`: creates/reuses `LifecycleAgent`, `AgentFrame`, optional `RuntimeSession`. Required result: `AgentLaunched { run_ref, agent_ref, frame_ref, runtime_session_ref? }`. It should not promise assignment unless it is also a subject/activity execution.
- `SubjectExecutionIntent`: requires `SubjectRef` and a resolved graph/activity target. Required result: `SubjectExecutionScheduled` or `SubjectExecutionAssigned`, with `assignment_ref` or `pending_assignment_ref`.
- `InteractionDispatchIntent`: creates `LifecycleGate` and optional child agent. Required result: `InteractionGateOpened { gate_ref, run_ref, agent_ref?, frame_ref? }`.

Given current code, the pragmatic first implementation batch is:

1. Extract `WorkflowGraphResolver` from `LifecycleDispatchService::resolve_workflow_graph` and keep the same semantics/tests. This is low blast radius because the repo port already exists and current tests already assert ByKey success/failure.
2. Introduce typed facade methods on the dispatch service, while internally reusing the current orchestration:
   - `start_run(LifecycleRunStartIntent) -> RunStarted`
   - `launch_agent(AgentLaunchIntent) -> AgentLaunched`
   - `execute_subject(SubjectExecutionIntent) -> SubjectExecutionAssigned`
   - `dispatch_interaction(InteractionDispatchIntent) -> InteractionGateOpened`
3. Keep existing `ExecutionIntent` only as a temporary adapter for current callers, but make route/service code call typed facades as they are migrated. Avoid adding more semantics to the broad optional DTO.
4. Move ProjectAgent and Task first because they already go through dispatch and have the smallest API surface change. ProjectAgent can use `AgentLaunchIntent`; Task can use `SubjectExecutionIntent` and propagate assignment_ref into its application/API result.
5. Move manual `start_lifecycle_run` next. The route should build `LifecycleRunStartIntent` with an explicit `WorkflowGraphRef`, returning typed run/graph refs instead of bare `LifecycleRun`. If the API still launches ready attempts, it should do so after dispatch has created a persisted `WorkflowGraphInstance` and control-plane anchors.
6. Add Story root/freeform launch as a dedicated subject execution/root launch path: `subject_ref = SubjectRef(kind=story)`, graph ref from story policy or explicit freeform, run-scoped Story association, root agent/frame, and subject execution result. Do not rely on session construction to create ownership.

### Verification Commands

Run focused checks first:

```powershell
cargo test -p agentdash-application workflow::dispatch_service
cargo test -p agentdash-application routine::dispatch
cargo test -p agentdash-application workflow::activity_run
cargo check -p agentdash-domain -p agentdash-application -p agentdash-api -p agentdash-contracts
```

If contracts or route DTOs change:

```powershell
pnpm run contracts:check
```

Final gate after the implementation batch:

```powershell
pnpm run check
```

Critical behavior tests to add or update:

- ProjectAgent `WorkflowGraphRef::ByKey` missing key fails before run/graph/assignment creation.
- Task start/continue returns subject execution refs and assignment_ref/pending_assignment_ref.
- Manual lifecycle run enters typed dispatch and creates persisted graph instance/control-plane anchors.
- Story root/freeform launch creates `SubjectRef(kind=story)` association and root agent/frame.
- SubjectExecutionView can trace `SubjectRef -> LifecycleSubjectAssociation -> AgentAssignment -> ActivityAttemptState`.

### External References

- None. This research is internal-only and based on local Trellis docs/specs plus current project code.

### Related Specs

- `.trellis/spec/backend/workflow/activity-lifecycle.md:12`: activity runtime identity is `graph_instance_id + activity_key`, and assignment key includes `graph_instance_id + activity_key + attempt`.
- `.trellis/spec/backend/workflow/activity-lifecycle.md:86`: `ExecutorRunRef::AgentSession` is runtime evidence; agent/frame/attempt bridge is `AgentAssignment`.
- `.trellis/spec/backend/workflow/lifecycle-run-link.md:9`: `RuntimeSession` is a runtime trace container, not business ownership.
- `.trellis/spec/backend/workflow/lifecycle-run-link.md:99`: Subject queries should go through `LifecycleSubjectAssociation`.
- `.trellis/spec/backend/story-task-runtime.md:9`: Story is aggregate root and must not bind RuntimeSession.
- `.trellis/spec/backend/story-task-runtime.md:31`: Task execution enters via `SubjectRef(kind=Task)` and execution intent.
- `.trellis/spec/backend/story-task-runtime.md:37`: runtime state is namespaced by `WorkflowGraphInstance`.
- `.trellis/spec/backend/story-task-runtime.md:125`: Task facade names may remain, but internals submit execution intent and session route is only RuntimeTrace.
- `.trellis/tasks/06-01-lifecycle-control-plane-structural-audit/design.md:73`: dispatch taxonomy is a named boundary.
- `.trellis/tasks/06-01-lifecycle-control-plane-structural-audit/design.md:96`: WorkflowGraphResolver is a named boundary.
- `.trellis/tasks/06-01-lifecycle-control-plane-structural-audit/implement.md:40`: Phase 3 requires typed dispatch taxonomy and ByKey fix.
- `.trellis/tasks/06-01-lifecycle-control-plane-structural-audit/implement.md:73`: Phase 5 requires Story root/freeform launch through dispatch.

## Caveats / Not Found

- `python ./.trellis/scripts/task.py current --source` reported no active task. The user explicitly supplied `.trellis/tasks/06-01-lifecycle-control-plane-structural-audit/`, so this research was written there.
- Current code has partially addressed the old P0-03/P0-04 surface symptom: dispatch now resolves ByKey and creates an entry assignment. The remaining problem is structural: one broad optional intent/result still conflates run start, agent launch, subject execution, and interaction dispatch.
- No Story root/freeform launch route or service path through `ExecutionIntent`/dispatch was found. Existing Story run routes are read-only projections, and freeform service currently seeds definitions.
- Manual lifecycle run remains a clear dispatch bypass.
- The dispatch-created entry assignment is useful for a root/entry activity anchor, but typed `SubjectExecutionIntent` still needs to decide when assignment is real, pending, scheduler-owned, or not applicable.
