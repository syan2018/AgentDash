# Lifecycle 控制面硬切清场执行计划

## Phase 1: Target Contracts And Baseline

Decision anchors:

- [语义盘点：RuntimeSession / Lifecycle Association / Projection](../06-01-lifecycle-control-plane-concept-alignment/semantic-inventory.md)
- [重构计划 Phase 0 / Phase 1 / Phase 7](../06-01-lifecycle-control-plane-concept-alignment/refactor-plan.md)
- [模块差距：Persistence / API / contracts 与 Proposed Target View Models](../06-01-lifecycle-control-plane-concept-alignment/module-gap-analysis.md)
- [边界评估：Generated Contracts Are Boundary Objects](../06-01-lifecycle-control-plane-concept-alignment/abstraction-boundary-and-coupling-review.md)

- Add target DTOs to `agentdash-contracts` and export generated TS:
  - stable refs
  - lifecycle run / agent / frame / subject / runtime trace views
  - `ProjectAgentLaunchResult`
- Delete public old contract exports:
  - `ProjectAgentSession`
  - `OpenProjectAgentSessionResult`
  - run-link DTOs
  - session-first workflow run shapes
- Rename public workflow routes and frontend service paths:
  - `/agent-procedures`
  - `/workflow-graphs`
- Rewrite clean baseline to target schema.
- Remove `lifecycle_gates` duplicate schema and align migrations with repository expectations.
- Remove readiness requirement for `lifecycle_run_links`.

Validation:

```powershell
pnpm run contracts:check
rg -n "ProjectAgentSession|OpenProjectAgentSessionResult|LifecycleRunLinkDto|RunLinksResponse" crates packages/app-web/src
rg -n "workflow_definitions|lifecycle_definitions|session_bindings|lifecycle_run_links" crates/agentdash-infrastructure/migrations/0001_init.sql
```

## Phase 2: Dispatch Runtime Ownership

Decision anchors:

- [语义盘点：Dispatch 与 RuntimeSession](../06-01-lifecycle-control-plane-concept-alignment/semantic-inventory.md)
- [重构计划 Phase 2 / Phase 3](../06-01-lifecycle-control-plane-concept-alignment/refactor-plan.md)
- [边界评估：One Stable Ingress / One Effective Runtime Surface](../06-01-lifecycle-control-plane-concept-alignment/abstraction-boundary-and-coupling-review.md)
- [模块差距：Session construction / launch 与 Agent executor / step activation](../06-01-lifecycle-control-plane-concept-alignment/module-gap-analysis.md)

- Extend `LifecycleDispatchService` dependencies so it can create RuntimeSession through the existing runtime/session owner without route-level ownership.
- Implement `RuntimePolicy::CreateRuntimeSession`:
  - create RuntimeSession from `AgentFrame`
  - persist runtime ref into `AgentFrame.runtime_session_refs_json`
  - return `runtime_session_ref`
- Delete ProjectAgent route direct session creation and replace with `/launch`.
- Convert Task / Companion / Routine / manual run to depend on dispatch-created runtime refs.
- Delete production `SessionConstructionPlan -> RuntimeLaunchRequest` path.
- Ensure runtime launch from frame includes execution profile, capability, context, VFS, MCP, procedure, and runtime refs.

Validation:

```powershell
rg -n "SessionConstructionPlan|runtime_launch_request_from_construction_plan|build_session_construction_for_launch" crates/agentdash-api/src crates/agentdash-application/src
rg -n "RuntimePolicy::CreateRuntimeSession => None|create_session\\(\"\"\\)" crates
cargo test -p agentdash-application dispatch
```

## Phase 3: Assignment Hard Guard

Decision anchors:

- [语义盘点：ActivityAttemptState 与 LifecycleActor / AgentStateAnchor](../06-01-lifecycle-control-plane-concept-alignment/semantic-inventory.md)
- [重构计划 Phase 4](../06-01-lifecycle-control-plane-concept-alignment/refactor-plan.md)
- [边界评估：Assignment State 与 ActivityAttemptState Is Evidence](../06-01-lifecycle-control-plane-concept-alignment/abstraction-boundary-and-coupling-review.md)
- [模块差距：Activity attempt evidence 与 Workflow engine/scheduler](../06-01-lifecycle-control-plane-concept-alignment/module-gap-analysis.md)

- Remove `Uuid::nil()` placeholders from lifecycle workflow paths.
- Make scheduler/orchestrator create real `AgentAssignment` before RuntimeSession launch for Agent Activity attempts.
- Make `ExecutionDispatchResult.assignment_ref` required for Agent Activity execution.
- Fix `ActivityExecutionClaimRepository::find_running_by_executor_session` to query tagged `ExecutorRunRef` JSON.
- Update terminal / advance / hook resolution to prefer:
  - RuntimeSession -> AgentFrame -> LifecycleAgent -> AgentAssignment -> ActivityAttemptState
- Keep ActivityAttemptState as evidence only.

Validation:

```powershell
rg -n "Uuid::nil\\(\\)" crates/agentdash-application/src/workflow
rg -n "assignment_ref: None" crates
cargo test -p agentdash-infrastructure find_running_by_executor_session
cargo test -p agentdash-application workflow
```

## Phase 4: Subject Association Cutover

Decision anchors:

- [语义盘点：Story / Task / Lifecycle Association](../06-01-lifecycle-control-plane-concept-alignment/semantic-inventory.md)
- [重构计划 Phase 5 / Phase 7](../06-01-lifecycle-control-plane-concept-alignment/refactor-plan.md)
- [边界评估：Business Modules Only Speak SubjectRef And Dispatch Result](../06-01-lifecycle-control-plane-concept-alignment/abstraction-boundary-and-coupling-review.md)
- [模块差距：Task execution projection 与 Runtime ownership](../06-01-lifecycle-control-plane-concept-alignment/module-gap-analysis.md)

- Replace all `LifecycleRunLinkRepository` application/API usage with `LifecycleSubjectAssociationRepository`.
- Remove `/lifecycle-runs/{id}/links` routes and run-link DTOs.
- Rewrite Story runs / active run queries using subject execution or lifecycle run view.
- Make Task dispatch create agent-scoped subject association after agent creation.
- Rewrite `TaskExecutionView` / `SubjectExecutionView` to use agent association and assignment, not run link or active-agent guessing.
- Remove old `lifecycle_run_links` repository and migration readiness after forward migration/drop is complete.

Validation:

```powershell
rg -n "LifecycleRunLink|LifecycleRunLinkRepository|lifecycle_run_link_repo|lifecycle_run_links" crates packages/app-web/src
cargo test -p agentdash-application task
cargo test -p agentdash-api story
```

## Phase 5: Frontend Hard Cut

Decision anchors:

- [重构计划 Phase 6](../06-01-lifecycle-control-plane-concept-alignment/refactor-plan.md)
- [模块差距：Frontend 与 Proposed Target View Models](../06-01-lifecycle-control-plane-concept-alignment/module-gap-analysis.md)
- [边界评估：One Stable Egress / Projection Can Depend On Many Facts, Commands Cannot](../06-01-lifecycle-control-plane-concept-alignment/abstraction-boundary-and-coupling-review.md)
- [语义盘点：Task、Story、Projection、RuntimeSession](../06-01-lifecycle-control-plane-concept-alignment/semantic-inventory.md)

- Replace hand-written `lifecycle-views.ts` with generated target contract imports.
- Wire `useLifecycleStore` as the main runtime store:
  - ingest ProjectAgent launch result
  - ingest Task start/continue result
  - fetch subject execution and frame runtime views
- Update Agent tab to lifecycle runs / agents only; remove old session list props and `ActiveSessionList` alias.
- Update Task drawer:
  - remove `TaskSessionPayload`
  - remove `/tasks/{id}/session`
  - use returned refs and `SubjectExecutionView`
- Update Story runtime panel:
  - remove `createStorySession / unbindStorySession`
  - use Story subject execution / launch entrypoint
- Add or connect `/run/:id`, `/subject/:kind/:id`, `/agent/:id`.
- Reduce `/session/:id` to RuntimeSession trace detail and trace drill-down from frame/runtime refs.
- Remove `lifecycle_step_key`, `agent_session`, `by-session`, `binding_kind`, `binding_kinds` frontend code.

Validation:

```powershell
pnpm run frontend:check
rg -n "TaskSessionPayload|lifecycle_step_key|agent_session|fetchWorkflowRunsBySession|by-session|binding_kind|binding_kinds|ActiveSessionList" packages/app-web/src
```

## Phase 6: Tests And E2E Rewrite

Decision anchors:

- [重构计划 Phase 8](../06-01-lifecycle-control-plane-concept-alignment/refactor-plan.md)
- [边界评估：Refactor Health Checks](../06-01-lifecycle-control-plane-concept-alignment/abstraction-boundary-and-coupling-review.md)
- [模块差距：Priority Risk Areas](../06-01-lifecycle-control-plane-concept-alignment/module-gap-analysis.md)
- [语义盘点：核心判断](../06-01-lifecycle-control-plane-concept-alignment/semantic-inventory.md)

- Update backend tests for:
  - RuntimeSession creation and frame ref persistence
  - runtime trace lookup
  - real assignment guard
  - tagged executor run query
  - agent-scoped SubjectExecutionView
  - LifecycleGate repository/schema
- Update frontend tests for:
  - Agent tab lifecycle props
  - Task drawer subject execution
  - Story subject execution panel
  - Session trace-only route
  - runtime ref kind `runtime_session`
- Rewrite E2E tests away from session-first expectations:
  - ProjectAgent launch -> agent/frame visible -> trace drill-down
  - Story / Task subject execution projection
  - Companion gate resolve
  - Routine dispatch projection
  - Permission frame revision

Validation:

```powershell
pnpm run contracts:check
pnpm run frontend:check
pnpm run frontend:test
cargo test --workspace
pnpm run e2e:test:critical
```

## Final Cleanup Gate

Run all cleanup checks from `prd.md`. If any old symbol remains, classify it as:

- allowed runtime trace substrate, or
- test fixture explicitly named legacy, or
- blocker requiring deletion before task completion.

No old public API / DTO / frontend route / clean baseline schema may remain under the blocker class.
