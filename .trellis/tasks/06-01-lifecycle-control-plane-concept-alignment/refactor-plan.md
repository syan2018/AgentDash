# Lifecycle 控制面重构计划

## Purpose

本文把目标谓词体系拆成可以完成的工程阶段。它不是一次性大爆炸实现清单，而是按依赖顺序建立新的事实源、切换入口、迁移投影、最后删除重复字段。

核心目标：

```text
把 Agent runtime facts 从 Session / Task / Companion / Permission / HookRuntime / StepActivation 中收束到 Lifecycle -> Actor -> ActorFrame -> RuntimeSession。
```

## Planning Assumptions

- 项目仍在预研阶段，允许 breaking schema / API / generated contract 修改。
- 数据库 migration 需要处理，但不为外部兼容保留旧 API 双轨。
- `ActivityAttemptState` 保留现名，职责定义为 Activity 的一次 executor execution record。
- `LifecycleSubjectAssociation` 只需要 run / Actor anchor；Activity 与 ActivityAttemptState 通过 ActorAssignment 提供执行证据。
- Task 没有运行时含义。进入 runtime 的是 `SubjectRef(kind=Task, id=...)`。

## Phase 0: 固化命名与契约边界

目标：先把名称和 DTO 边界固定，避免后续 schema 做成第二套模糊模型。

Deliverables:

- 确认目标术语：
  - `Workflow` = 当前 `ActivityLifecycleDefinition` 的目标语义，表示 executable graph config。
  - `ActivityProcedure` 或 `ActorProcedure` = 当前 `WorkflowDefinition` 的目标语义，表示 single Agent Activity contract。
  - `LifecycleRun` 是否保留代码名；若保留，文档和 DTO 统一解释为 tracked life process。
  - `RuntimeSession` = 当前 Session event/turn/tool/resume/debug substrate。
- 在 `agentdash-contracts` 中新增目标 DTO，而不是继续让 route-local shape 漂移：
  - `LifecycleRunView`
  - `LifecycleActorDto`
  - `ActorFrameDto`
  - `ActorAssignmentDto`
  - `LifecycleSubjectAssociationDto`
  - `SubjectRefDto`
  - `SubjectExecutionView`
- 更新 `terminology-notes.md` 和 `semantic-inventory.md`，让它们只记录目标词汇和原因。

Acceptance:

- 每个新 DTO 有明确 owner：control-plane view、actor/frame view、runtime trace view。
- 新 DTO 不把 Task、Session、ActivityAttemptState 写成 Agent 状态锚点。

## Phase 1: 建立 Actor / ActorFrame / Association 持久化事实源

目标：先增加目标事实源，给后续模块迁移提供 anchor。

Schema:

- `lifecycle_actors`
  - `id`
  - `run_id`
  - `project_id`
  - `actor_kind`
  - `actor_role`
  - `project_agent_id`
  - `status`
  - `current_frame_id`
  - `created_at`
  - `updated_at`
- `actor_frames`
  - `id`
  - `actor_id`
  - `revision`
  - `procedure_id`
  - `activity_key`
  - `capability_state_json`
  - `context_projection_ref`
  - `vfs_surface_json`
  - `mcp_servers_json`
  - `runtime_refs_json`
  - `created_by_kind`
  - `created_by_id`
  - `created_at`
- `actor_assignments`
  - `id`
  - `run_id`
  - `actor_id`
  - `activity_key`
  - `attempt`
  - `status`
  - `assigned_at`
  - `released_at`
- `lifecycle_subject_associations`
  - `id`
  - `anchor_run_id`
  - `anchor_actor_id`
  - `subject_kind`
  - `subject_id`
  - `role`
  - `metadata`
  - `created_at`
- `lifecycle_gates`
  - `id`
  - `run_id`
  - `actor_id`
  - `frame_id`
  - `gate_kind`
  - `correlation_id`
  - `status`
  - `payload`
  - `resolved_by`
  - `created_at`
  - `resolved_at`

Domain / repository:

- Add domain entities and repositories under workflow/lifecycle module boundaries.
- Add repository set entries.
- Add contract DTO mapping and generated TS exports.
- Add indexes:
  - actor by run/status
  - frame by actor/revision
  - assignment by run/activity/attempt
  - association by subject and by anchor actor
  - runtime session ref lookup from frame JSON or normalized table if query pressure is high

Backfill:

- For each `LifecycleRun.session_id`, create a root Actor and ActorFrame with that runtime session ref.
- For each `ActivityAttemptState.executor_run.AgentSession`, create or reuse assignment to `(run_id, activity_key, attempt)` and attach runtime session evidence.
- For each `LifecycleRunLink`, copy to `LifecycleSubjectAssociation(anchor_run_id=run_id, anchor_actor_id=null, ...)`.
- For existing session lineage where both sides resolve to actors, create actor lineage metadata.

Acceptance:

- Existing lifecycle runs can be resolved to at least one root Actor when `session_id` exists.
- Existing subject links can be queried through the new association repository.
- No production code path depends on new tables yet; this phase only makes target facts available.

## Phase 2: 引入统一 Lifecycle Dispatch Service

目标：把 ProjectAgent、Story session、Task direct execution、Companion、Routine 的入口统一成一个 execution intent。

New application service:

```text
LifecycleDispatchService
  input: ExecutionIntent
  output: ExecutionDispatchResult
```

`ExecutionIntent` contains:

- `project_id`
- `source`: user / routine / parent_actor / project_agent / API
- `subject_ref`: optional Story / Task / RoutineExecution / Project / External
- `workflow_key`: executable graph config
- `procedure_override`: optional ActivityProcedure / ActorProcedure
- `actor_policy`: create / reuse / resume / spawn_child
- `context_policy`: inherit / slice / isolated
- `capability_policy`: baseline / grant constrained / inherited slice
- `runtime_policy`: attach existing / create runtime session / continue current

Dispatch responsibilities:

- Resolve Workflow graph and Activity entry.
- Create LifecycleRun when the intent needs a new tracking boundary.
- Select or create Actor.
- Create SubjectAssociation at run or actor anchor.
- Build ActorFrame using existing StepActivation / session construction logic as inputs.
- Create or attach RuntimeSession as delivery substrate.
- Launch or enqueue the runtime command.

Acceptance:

- ProjectAgent open can return `run_id`, `actor_id`, `frame_id`, `runtime_session_id`.
- Task start can be expressed as `subject_ref=Task` without Task owning runtime.
- Companion dispatch can be expressed as parent Actor spawning child Actor + Gate.
- Routine fire can be expressed as `source=RoutineExecution` and actor reuse policy.

## Phase 3: 将 StepActivation / SessionConstruction 收束为 ActorFrame

目标：把当前最像 ActorFrame 的两条路径合并成一个 frame builder。

Work items:

- Rename or wrap `StepActivationInput` into `ActorFrameActivationInput`.
- `StepActivation` output becomes a frame delta:
  - effective procedure
  - capability state
  - MCP servers
  - VFS/lifecycle/routine/canvas mounts
  - context injections
  - kickoff prompt / delivery frame
  - writable ports / lifecycle artifact mount
- `SessionConstructionPlan` becomes:
  - `ActorFrameConstructionPlan`
  - `RuntimeSessionLaunchPlan`
- Pending capability transitions persist by frame/revision; session runtime commands become delivery queue.
- Hook runtime snapshot is keyed by actor/frame first, runtime session second.

Acceptance:

- A frame revision can answer: this Actor can see which context, call which tools, mount which VFS, use which procedure, and write to which lifecycle ports.
- RuntimeSession launch can be repeated from frame data without re-solving business owner from session.
- Existing prompt launch still receives the same connector `ExecutionContext`, but it is projected from ActorFrame.

## Phase 4: 切换 Lifecycle/Activity 执行链路

目标：让 workflow engine/scheduler/orchestrator stop using session as the primary association key.

Work items:

- `StartActivityLifecycleRunCommand` replaces `session_id` with dispatch/actor input.
- Scheduler creates `ActorAssignment` before writing `ExecutorRunRef`.
- `AgentActivityExecutorLauncher` returns assignment/frame/runtime refs.
- Terminal event handling:
  - RuntimeSession terminal -> frame lookup -> Actor -> Assignment -> ActivityAttemptState.
  - `complete_lifecycle_node` tool uses Actor/Assignment from frame snapshot, not `list_by_session`.
- Active workflow projection becomes `resolve_active_lifecycle_projection_for_actor` and `resolve_active_lifecycle_projection_for_runtime_session` only as a trace lookup.
- Lifecycle VFS reads node session data through ActorAssignment runtime refs.

Acceptance:

- Same LifecycleRun can contain multiple concurrent Actors with distinct RuntimeSessions.
- A single Actor can move through multiple ActivityAttemptStates while preserving frame history.
- `LifecycleRun.session_id` is no longer needed for terminal/advance/hook resolution.

## Phase 5: 迁移业务入口

### ProjectAgent

- Open ProjectAgent through `LifecycleDispatchService`.
- Response becomes `ProjectAgentActorLaunchResult`.
- `ProjectAgentSession` history becomes Actor history with RuntimeSession refs.
- `default_lifecycle_key` remains launch policy; `default_workflow_key` is replaced by explicit Workflow/Procedure selection after naming split.

### Story

- Story page queries `SubjectExecutionView(kind=Story)`.
- Manual Story session binding becomes Story Actor association / launch history.
- Story context injection is resolved from ActorFrame context scope.

### Task

- Remove direct Task session ownership path.
- `start_task` / `continue_task` become subject execution commands:
  - `SubjectRef(kind=Task, id=task_id)`
  - optional `procedure_override` from task payload or explicit command
  - actor policy create/reuse
- `Task.lifecycle_step_key` is replaced by SubjectAssociation/Assignment projection.
- Task status/artifacts remain view projection from lifecycle facts, with source revision if stored.

### Companion / Subagent

- Companion request creates:
  - child Actor
  - ActorFrame with inherited slice
  - Gate if wait/adoption requires parent decision
  - Actor lineage from parent Actor
- `CompanionSessionContext` splits into:
  - lineage
  - gate correlation
  - frame contribution/inherited slice
  - runtime provenance
- `CompanionWaitRegistry` becomes durable `lifecycle_gates`.
- workflow-backed companion uses the same Workflow/Actor dispatch path as other Agent Activity launches.

### Routine

- `RoutineExecution` creates Source association.
- `SessionStrategy` becomes Actor reuse policy:
  - Fresh -> create Actor
  - Reuse -> reuse ProjectAgent Actor
  - PerEntity -> lookup Actor by RoutineExecution/entity association
- `RoutineExecution.status` separates trigger dispatch state from lifecycle/actor terminal projection.

### Permission

- Grant request attaches actor/frame refs.
- Approved grants produce ActorFrame revision.
- Scope escalation creates `LifecycleSubjectAssociation(role=ControlScope)` at run/actor anchor.
- Permission UI queries frame/run/subject, not session first.

Acceptance:

- No business module creates a RuntimeSession without also creating or selecting Actor/Frame.
- Task and Companion can be traced through SubjectRef -> Actor -> Assignment -> ActivityAttemptState.
- Routine execution can be traced through RoutineExecution subject -> run/actor -> runtime trace.

## Phase 6: Contracts and frontend migration

目标：把 UI 的事实根从 Session tree 迁到 Actor/Subject/Lifecycle views。

Contract changes:

- Move top-level lifecycle run DTOs into `agentdash-contracts`.
- Add generated TS for target view models.
- Replace:
  - `/lifecycle-runs/by-session/{session_id}` with run/actor/subject queries.
  - Task execution session responses with `SubjectExecutionView` or dispatch result.
  - ProjectAgent session open result with actor/frame/runtime refs.
  - Story session binding response with actor association/run view.

Frontend changes:

- `workflowStore`
  - replace `runsBySessionId` with normalized `runsById`, plus actor/subject indexes.
  - editor naming: graph Workflow vs ActivityProcedure.
- `storyStore`
  - Task execution actions consume SubjectExecutionView.
  - Story sessions surface becomes Story actors/runs.
- `projectStore`
  - ProjectAgent open stores actor launch result.
- `SessionPage`
  - becomes RuntimeSessionTraceView route.
- New pages/components:
  - ActorFrame runtime panel
  - SubjectExecution panel for Story/Task
  - ProjectActiveActors navigation
- Tests:
  - task drawer return asserts actor/frame result and runtime ref.
  - story context injection starts Story Actor.
  - ProjectAgent extension tests open actor workspace then inspect runtime session.

Acceptance:

- UI no longer groups primary runtime by `owner_type=story/task/project` session tree.
- Task drawer can show latest actor assignment, activity attempt status and artifacts without Task owning a session.
- RuntimeSession trace is still visible as a drill-down.

## Phase 7: Remove duplicated fields and session-first APIs

目标：完成模型收束，删除让旧谓词复活的字段。

Deletions / renames:

- Drop `lifecycle_runs.session_id`.
- Remove `LifecycleRunRepository::list_by_session`; replace with actor/runtime lookup.
- Rename execution log fields:
  - `step_key` -> `activity_key`
  - `StepActivated/StepCompleted` -> Activity vocabulary.
- Remove `Task.lifecycle_step_key` from domain/contract/UI.
- Split or mark `Task.status/artifacts` as projection cache with source revision.
- Remove route-local `SessionBindingResponse` shapes for Story sessions.
- Replace `ProjectAgentSession.binding_id=session_id`.
- Replace `GrantScope::WorkflowStep` naming with activity/frame scope vocabulary.
- Remove `default_workflow_key` request field after Workflow/Procedure split.

Database:

- Update clean baseline migration to target schema.
- Add forward migrations for current developer DBs.
- Add `lifecycle_run_links` and `permission_grants` to readiness if they remain before final rename.
- Drop old standalone `tasks` table from clean baseline after Task JSONB projection strategy is settled.

Acceptance:

- `rg "list_by_session"` has no lifecycle control-plane callers.
- `rg "lifecycle_step_key"` only appears in migrations/backfill notes until removed.
- generated TS has no top-level `WorkflowRun.session_id`.
- Task execution tests no longer navigate from Task directly to `/session/{id}`.

## Phase 8: Verification Plan

Backend checks:

- Domain unit tests for Actor/Frame/Assignment/Association invariants.
- Workflow scheduler/orchestrator tests for:
  - same-run multi-actor
  - SpawnChild actor
  - ContinueRoot actor frame transition
  - terminal event maps to assignment/attempt
- Task projection tests:
  - SubjectRef Task -> ActorAssignment -> ActivityAttemptState -> Task projection.
- Companion tests:
  - wait gate durable state
  - result resumes parent Actor
  - workflow-backed companion creates associations.
- Permission tests:
  - grant applies to ActorFrame revision
  - scope escalation writes association.
- Routine tests:
  - Fresh/Reuse/PerEntity actor policy.

Frontend checks:

- Contract generation and `pnpm run contracts:check`.
- Store tests for normalized run/actor/subject indexes.
- Component tests for ActorFrameRuntimeView and SubjectExecutionView.
- E2E:
  - ProjectAgent open actor
  - Story actor context injection
  - Task subject execution and return
  - Companion wait/result gate

Migration checks:

- Backfill creates root actors for existing lifecycle runs.
- Existing session events remain readable through RuntimeSessionTraceView.
- Existing Story/Task views can be projected after association backfill.

## Suggested Implementation Order

1. Add target entities/repositories/contracts with backfill.
2. Build `LifecycleDispatchService` and wire ProjectAgent open through it.
3. Refactor StepActivation/SessionConstruction into ActorFrame construction.
4. Switch workflow scheduler/orchestrator to ActorAssignment and runtime ref lookup.
5. Migrate Task direct execution into SubjectRef dispatch.
6. Migrate Companion into Actor/Gate/FrameContribution.
7. Migrate Routine and Permission.
8. Update frontend view models and routes.
9. Remove session-first fields/APIs and rename Workflow/Procedure concepts.

This order keeps the most reused control-plane primitives ahead of business module migration, so Task, Companion and Routine do not each invent their own bridge.
