# Lifecycle 控制面重构计划

## Purpose

本文把目标谓词体系拆成可以完成的工程阶段。它不是一次性大爆炸实现清单，而是按依赖顺序建立新的事实源、切换入口、迁移投影、最后删除重复字段。

核心目标：

```text
把 Agent runtime facts 从 Session / Task / Companion / Permission / HookRuntime / StepActivation 中收束到 Lifecycle -> LifecycleAgent -> AgentFrame -> RuntimeSession。
```

## Planning Assumptions

- 项目仍在预研阶段，允许 breaking schema / API / generated contract 修改。
- 数据库 migration 需要处理，但不为外部兼容保留旧 API 双轨。
- `ActivityAttemptState` 保留现名，职责定义为 Activity 的一次 executor execution record。
- `LifecycleSubjectAssociation` 只需要 run / LifecycleAgent anchor；Activity 与 ActivityAttemptState 通过 AgentAssignment 提供执行证据。
- Task 没有运行时含义。进入 runtime 的是 `SubjectRef(kind=Task, id=...)`。
- 新增抽象必须拥有事实源、不变量、查询边界、生命周期或外部依赖隔离；纯转发层应合并回 owner module。详细规则见 `abstraction-boundary-and-coupling-review.md`。
- `AgentDefinition` 只用于可复用 Agent 模板或类型定义；`AgentProcedure` 用于 Workflow Activity 绑定后的行为契约，不把二者揉成一层。

## Phase 0: 固化命名与契约边界

目标：先把名称和 DTO 边界固定，避免后续 schema 做成第二套模糊模型。

Deliverables:

- 确认目标术语：
  - `Workflow` = 当前 `ActivityLifecycleDefinition` 的目标语义，表示 executable graph config。
  - `AgentDefinition` = 可复用 Agent 类型或模板，表示静态配置面。
  - `AgentProcedure` = 当前 `WorkflowDefinition` 的目标语义，表示 single Agent Activity contract。
  - `LifecycleRun` 保留代码名，文档和 DTO 统一解释为 tracked life process。
  - `RuntimeSession` = 当前 Session event/turn/tool/resume/debug substrate。
- 在 `agentdash-contracts` 中新增目标 DTO，而不是继续让 route-local shape 漂移：
  - `SubjectRefDto`
  - `LifecycleRunRefDto`
  - `LifecycleAgentRefDto`
  - `AgentFrameRefDto`
  - `RuntimeSessionRefDto`
  - `AgentAssignmentRefDto`
  - `LifecycleRunView`
  - `LifecycleAgentDto`
  - `AgentFrameDto`
  - `AgentAssignmentDto`
  - `LifecycleSubjectAssociationDto`
  - `SubjectExecutionView`
- 更新 `terminology-notes.md` 和 `semantic-inventory.md`，让它们只记录目标词汇和原因。
- 更新 `abstraction-boundary-and-coupling-review.md`，把新增抽象的预算、owner module 和跨层依赖规则作为实现门禁。

Acceptance:

- 每个新 DTO 有明确 owner：control-plane view、agent/frame view、runtime trace view。
- 新 DTO 不把 Task、Session、ActivityAttemptState 写成 Agent 状态锚点。
- command path 只接受 stable refs 与 intent，不接受 `SubjectExecutionView`、`RuntimeTraceView`、`ProjectAgentLaunchView` 回传。
- 新 service 若只做字段转发，必须降为 private helper 或合并回 owner service。

## Phase 1: 建立 LifecycleAgent / AgentFrame / Association 持久化事实源

目标：先增加目标事实源，给后续模块迁移提供 anchor。

Schema:

- `lifecycle_agents`
  - `id`
  - `run_id`
  - `project_id`
  - `agent_kind`
  - `agent_role`
  - `project_agent_id`
  - `status`
  - `current_frame_id`
  - `created_at`
  - `updated_at`
- `agent_frames`
  - `id`
  - `agent_id`
  - `revision`
  - `procedure_id`
  - `activity_key`
  - `effective_capability_json`
  - `context_slice_json`
  - `vfs_surface_json`
  - `mcp_surface_json`
  - `runtime_session_refs_json`
  - `created_by_kind`
  - `created_by_id`
  - `created_at`
- `agent_assignments`
  - `id`
  - `run_id`
  - `agent_id`
  - `activity_key`
  - `attempt`
  - `lease_status`
  - `assigned_at`
  - `released_at`
- `lifecycle_subject_associations`
  - `id`
  - `anchor_run_id`
  - `anchor_agent_id`
  - `subject_kind`
  - `subject_id`
  - `role`
  - `metadata`
  - `created_at`
- `lifecycle_gates`
  - `id`
  - `run_id`
  - `agent_id`
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
- Keep `EffectiveCapability`, `ContextSlice`, `VfsSurface`, `McpSurface` and `RuntimeSessionRef[]` as AgentFrame-owned value objects unless they gain independent query or lifecycle needs.
- `AgentAssignment.lease_status` only records assignment ownership such as assigned/released/cancelled; executor terminal result remains in `ActivityAttemptState`.
- Add repository set entries.
- Add contract DTO mapping and generated TS exports.
- Add indexes:
  - agent by run/status
  - frame by agent/revision
  - assignment by run/activity/attempt
  - association by subject and by anchor agent
  - runtime session ref lookup from frame JSON or normalized table if query pressure is high

Backfill:

- For each `LifecycleRun.session_id`, create a root LifecycleAgent and AgentFrame with that runtime session ref.
- For each `ActivityAttemptState.executor_run.AgentSession`, create or reuse assignment to `(run_id, activity_key, attempt)` and attach runtime session evidence.
- For each `LifecycleRunLink`, copy to `LifecycleSubjectAssociation(anchor_run_id=run_id, anchor_agent_id=null, ...)`.
- For existing session lineage where both sides resolve to agents, create agent lineage metadata.

Acceptance:

- Existing lifecycle runs can be resolved to at least one root LifecycleAgent when `session_id` exists.
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
- `source`: user / routine / parent_agent / project_agent / API
- `subject_ref`: optional Story / Task / RoutineExecution / Project / External
- `workflow_key`: executable graph config
- `procedure_override`: optional AgentProcedure
- `agent_policy`: create / reuse / resume / spawn_child
- `context_policy`: inherit / slice / isolated
- `capability_policy`: baseline / grant constrained / inherited slice
- `runtime_policy`: attach existing / create runtime session / continue current

Dispatch responsibilities:

- Resolve Workflow graph and Activity entry.
- Create LifecycleRun when the intent needs a new tracking boundary.
- Select or create LifecycleAgent.
- Create SubjectAssociation at run or agent anchor.
- Build AgentFrame using existing StepActivation / session construction logic as inputs.
- Create or attach RuntimeSession as delivery substrate.
- Launch or enqueue the runtime command.
- Coordinate owner services inside one transaction boundary; frame construction, capability resolving and runtime connector delivery stay owned by their modules.

Acceptance:

- ProjectAgent open can return `run_id`, `agent_id`, `frame_id`, `runtime_session_id`.
- Task start can be expressed as `subject_ref=Task` without Task owning runtime.
- Companion dispatch can be expressed as parent LifecycleAgent spawning child LifecycleAgent + Gate.
- Routine fire can be expressed as `source=RoutineExecution` and agent reuse policy.
- Business modules do not import frame construction plans, runtime launch requests or connector event schemas.

## Phase 3: 将 StepActivation / SessionConstruction 收束为 AgentFrame

目标：把当前最像 AgentFrame 的两条路径合并成一个 frame builder。

Work items:

- Rename or wrap `StepActivationInput` into `AgentFrameActivationInput`.
- `StepActivation` output becomes a frame delta:
  - effective procedure
  - capability state
  - MCP servers
  - VFS/lifecycle/routine/canvas mounts
  - context injections
  - kickoff prompt / delivery frame
  - writable ports / lifecycle artifact mount
- `SessionConstructionPlan` becomes:
  - private `AgentFrameConstructionPlan` inside the frame builder.
  - runtime-session adapter `RuntimeLaunchRequest`, projected from `AgentFrame`.
- Pending capability transitions persist by frame/revision; session runtime commands become delivery queue.
- Hook runtime snapshot is keyed by agent/frame first, runtime session second.

Acceptance:

- A frame revision can answer: this LifecycleAgent can see which context, call which tools, mount which VFS, use which procedure, and write to which lifecycle ports.
- RuntimeSession launch can be repeated from frame data without re-solving business owner from session.
- Existing prompt launch still receives the same connector `ExecutionContext`, but it is projected from AgentFrame.
- `AgentFrameConstructionPlan` is not generated as contract, not persisted, and not imported by business modules.

## Phase 4: 切换 Lifecycle/Activity 执行链路

目标：让 workflow engine/scheduler/orchestrator stop using session as the primary association key.

Work items:

- `StartActivityLifecycleRunCommand` replaces `session_id` with dispatch/agent input.
- Scheduler creates `AgentAssignment` before writing `ExecutorRunRef`.
- `AgentActivityExecutorLauncher` returns assignment/frame/runtime refs.
- `AgentAssignment` state is limited to assignment lease state; attempt progress, terminal reason and artifacts stay in `ActivityAttemptState`.
- Terminal event handling:
  - RuntimeSession terminal -> frame lookup -> LifecycleAgent -> Assignment -> ActivityAttemptState.
  - `complete_lifecycle_node` tool uses LifecycleAgent/Assignment from frame snapshot, not `list_by_session`.
- Active workflow projection becomes `resolve_active_lifecycle_projection_for_agent` and `resolve_active_lifecycle_projection_for_runtime_session` only as a trace lookup.
- Lifecycle VFS reads node session data through AgentAssignment runtime refs.

Acceptance:

- Same LifecycleRun can contain multiple concurrent LifecycleAgents with distinct RuntimeSessions.
- A single LifecycleAgent can move through multiple ActivityAttemptStates while preserving frame history.
- `LifecycleRun.session_id` is no longer needed for terminal/advance/hook resolution.

## Phase 5: 迁移业务入口

### ProjectAgent

- Open ProjectAgent through `LifecycleDispatchService`.
- Response becomes `ProjectAgentLaunchResult`.
- `ProjectAgentSession` history becomes LifecycleAgent history with RuntimeSession refs.
- `default_lifecycle_key` remains launch policy; `default_workflow_key` is replaced by explicit Workflow/Procedure selection after naming split.

### Story

- Story page queries `SubjectExecutionView(kind=Story)`.
- Manual Story session binding becomes Story Agent association / launch history.
- Story context injection is resolved from AgentFrame context scope.

### Task

- Remove direct Task session ownership path.
- `start_task` / `continue_task` become subject execution commands:
  - `SubjectRef(kind=Task, id=task_id)`
  - optional `procedure_override` from task payload or explicit command
  - agent policy create/reuse
- `Task.lifecycle_step_key` is replaced by SubjectAssociation/Assignment projection.
- Task status/artifacts remain view projection from lifecycle facts, with source revision if stored.

### Companion / Subagent

- Companion request creates:
  - child LifecycleAgent
  - AgentFrame with inherited slice
  - Gate if wait/adoption requires parent decision
  - Agent lineage from parent LifecycleAgent
- `CompanionSessionContext` splits into:
  - lineage
  - gate correlation
  - frame contribution/inherited slice
  - runtime provenance
- `CompanionWaitRegistry` becomes durable `lifecycle_gates`.
- workflow-backed companion uses the same Workflow/Agent dispatch path as other Agent Activity launches.

### Routine

- `RoutineExecution` creates Source association.
- `SessionStrategy` becomes Agent reuse policy:
  - Fresh -> create LifecycleAgent
  - Reuse -> reuse ProjectAgent LifecycleAgent
  - PerEntity -> lookup LifecycleAgent by RoutineExecution/entity association
- `RoutineExecution.status` separates trigger dispatch state from lifecycle/agent terminal projection.

### Permission

- Grant request attaches agent/frame refs.
- Approved grants produce AgentFrame revision.
- Scope escalation creates `LifecycleSubjectAssociation(role=ControlScope)` at run/agent anchor.
- Permission UI queries frame/run/subject, not session first.

Acceptance:

- No business module creates a RuntimeSession without also creating or selecting LifecycleAgent/AgentFrame.
- Task and Companion can be traced through SubjectRef -> LifecycleAgent -> Assignment -> ActivityAttemptState.
- Routine execution can be traced through RoutineExecution subject -> run/agent -> runtime trace.

## Phase 6: Contracts and frontend migration

目标：把 UI 的事实根从 Session tree 迁到 Agent/Subject/Lifecycle views。

Contract changes:

- Move top-level lifecycle run DTOs into `agentdash-contracts`.
- Add generated TS for target view models.
- Replace:
  - `/lifecycle-runs/by-session/{session_id}` with run/agent/subject queries.
  - Task execution session responses with `SubjectExecutionView` or dispatch result.
  - ProjectAgent session open result with agent/frame/runtime refs.
  - Story session binding response with agent association/run view.

Frontend changes:

- `workflowStore`
  - replace `runsBySessionId` with normalized `runsById`, plus agent/subject indexes.
  - editor naming: graph Workflow vs AgentProcedure.
- `storyStore`
  - Task execution actions consume SubjectExecutionView.
  - Story sessions surface becomes Story agents/runs.
- `projectStore`
  - ProjectAgent open stores agent launch result.
- `SessionPage`
  - becomes RuntimeSessionTraceView route.
- New pages/components:
  - AgentFrame runtime panel
  - SubjectExecution panel for Story/Task
  - ProjectActiveAgents navigation
- Tests:
  - task drawer return asserts agent/frame result and runtime ref.
  - story context injection starts Story Agent.
  - ProjectAgent extension tests open agent workspace then inspect runtime session.

Acceptance:

- UI no longer groups primary runtime by `owner_type=story/task/project` session tree.
- Task drawer can show latest agent assignment, activity attempt status and artifacts without Task owning a session.
- RuntimeSession trace is still visible as a drill-down.
- Read views are not accepted by write commands; commands use `SubjectRef`, `ExecutionIntent` and stable refs.

## Phase 7: Remove duplicated fields and session-first APIs

目标：完成模型收束，删除让旧谓词复活的字段。

Deletions / renames:

- Drop `lifecycle_runs.session_id`.
- Remove `LifecycleRunRepository::list_by_session`; replace with agent/runtime lookup.
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

- Domain unit tests for LifecycleAgent/AgentFrame/AgentAssignment/Association invariants.
- Workflow scheduler/orchestrator tests for:
  - same-run multi-agent
  - SpawnChild agent
  - ContinueRoot agent frame transition
  - terminal event maps to assignment/attempt
- Task projection tests:
  - SubjectRef Task -> AgentAssignment -> ActivityAttemptState -> Task projection.
- Companion tests:
  - wait gate durable state
  - result resumes parent LifecycleAgent
  - workflow-backed companion creates associations.
- Permission tests:
  - grant applies to AgentFrame revision
  - scope escalation writes association.
- Routine tests:
  - Fresh/Reuse/PerEntity agent policy.

Frontend checks:

- Contract generation and `pnpm run contracts:check`.
- Store tests for normalized run/agent/subject indexes.
- Component tests for AgentFrameRuntimeView and SubjectExecutionView.
- E2E:
  - ProjectAgent open agent
  - Story agent context injection
  - Task subject execution and return
  - Companion wait/result gate

Migration checks:

- Backfill creates root agents for existing lifecycle runs.
- Existing session events remain readable through RuntimeSessionTraceView.
- Existing Story/Task views can be projected after association backfill.

## Suggested Implementation Order

1. Add target entities/repositories/contracts with backfill.
2. Build `LifecycleDispatchService` and wire ProjectAgent open through it.
3. Refactor StepActivation/SessionConstruction into AgentFrame construction.
4. Switch workflow scheduler/orchestrator to AgentAssignment and runtime ref lookup.
5. Migrate Task direct execution into SubjectRef dispatch.
6. Migrate Companion into Agent/Gate/FrameContribution.
7. Migrate Routine and Permission.
8. Update frontend view models and routes.
9. Remove session-first fields/APIs and rename Workflow/Procedure concepts.

This order keeps the most reused control-plane primitives ahead of business module migration, so Task, Companion and Routine do not each invent their own bridge.
