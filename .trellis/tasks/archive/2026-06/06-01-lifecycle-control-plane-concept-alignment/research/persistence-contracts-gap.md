# Research: persistence-contracts-gap

- Query: 持久化、数据库、contracts、API 边界扫描；定位 lifecycle/session/workflow/agent/task runtime facts 的字段分布、重复表达和目标归属。
- Scope: internal
- Date: 2026-06-01

## Findings

### Context Loaded

- `.trellis/workflow.md`: Trellis research 必须持久化到任务 `research/`，研究阶段不改代码。
- `.trellis/spec/backend/database-guidelines.md`: PostgreSQL schema 事实源是 `crates/agentdash-infrastructure/migrations/`，预研阶段历史 migration 与 forward migration 应共同收敛到目标 schema。
- `.trellis/spec/backend/repository-pattern.md`: session runtime persistence 走 `agentdash-spi::session_persistence` store 边界，不进普通 aggregate `RepositorySet` 聚合语义。
- `.trellis/spec/backend/session/architecture.md`: `ExecutionContext` 是 connector-facing projection；runtime map、active turn、connector live session 是三件事。
- `.trellis/spec/backend/session/runtime-execution-state.md`: terminal event / outbox / runtime command store 仍是 session runtime store 边界。
- `.trellis/spec/backend/session/execution-context-frames.md`: `ExecutionSessionFrame` / `ExecutionTurnFrame` 是 per-turn projection，不应回写成 application 事实源。
- `.trellis/spec/backend/workflow/architecture.md`: Activity lifecycle 是 workflow 运行主模型，durable advancement 只能通过 ActivityEvent 进入 LifecycleEngine。
- `.trellis/spec/backend/workflow/activity-lifecycle.md`: `ActivityAttemptState` 是 activity 执行证据，Function executor 也必须产出 terminal event。
- `.trellis/spec/backend/workflow/lifecycle-run-link.md`: `LifecycleRunLink` 是 run-subject association，目标替代 session binding。
- `.trellis/spec/cross-layer/frontend-backend-contracts.md`: `agentdash-contracts -> generated TS -> frontend service` 是 contract 主链路。
- `.trellis/spec/frontend/workflow-activity-lifecycle.md`: 前端应只读 `activity_state.attempts/outputs/inputs`，不读 step state。

### Files Found

| File | Description |
| --- | --- |
| `crates/agentdash-infrastructure/src/migration.rs` | PostgreSQL migration runner 与 schema readiness table list。 |
| `crates/agentdash-infrastructure/migrations/0001_init.sql` | 干净库基线，仍含旧 `tasks`、`session_bindings`、step lifecycle columns。 |
| `crates/agentdash-infrastructure/migrations/0020_stories_tasks_jsonb.sql` | Task 合入 `stories.tasks` JSONB 的迁移。 |
| `crates/agentdash-infrastructure/migrations/0021_workflow_binding_kind_no_task.sql` | 将 workflow/lifecycle binding 中的 `task` 收敛为 `story`。 |
| `crates/agentdash-infrastructure/migrations/0022_drop_task_runtime_fields.sql` | 删除旧 `tasks.executor_session_id` / `execution_mode`。 |
| `crates/agentdash-infrastructure/migrations/0024_workflow_binding_kinds.sql` | `binding_kind` 迁移为 `binding_kinds`。 |
| `crates/agentdash-infrastructure/migrations/0034_session_terminal_effect_outbox.sql` | session terminal effect durable outbox。 |
| `crates/agentdash-infrastructure/migrations/0035_session_runtime_commands.sql` | session-scoped runtime/capability transition records。 |
| `crates/agentdash-infrastructure/migrations/0044_project_agents.sql` | ProjectAgent project instance 表。 |
| `crates/agentdash-infrastructure/migrations/0047_activity_lifecycle_definition.sql` | Activity lifecycle definition columns。 |
| `crates/agentdash-infrastructure/migrations/0048_activity_execution_claims.sql` | durable activity execution claim 表。 |
| `crates/agentdash-infrastructure/migrations/0049_lifecycle_run_activity_state.sql` | `lifecycle_runs.activity_state`。 |
| `crates/agentdash-infrastructure/migrations/0057_backend_execution_leases.sql` | backend execution lease 与 session/turn 绑定。 |
| `crates/agentdash-infrastructure/migrations/0059_session_compaction_projection_store.sql` | session model-context compaction/projection/head store。 |
| `crates/agentdash-infrastructure/migrations/0060_session_lineage.sql` | session-to-session lineage/fork/spawn relation。 |
| `crates/agentdash-infrastructure/migrations/0068_drop_step_lifecycle_columns.sql` | 删除 step lifecycle columns。 |
| `crates/agentdash-infrastructure/migrations/0070_lifecycle_run_links.sql` | run-subject link 表及 session binding backfill。 |
| `crates/agentdash-infrastructure/migrations/0071_drop_session_bindings.sql` | 添加 `sessions.project_id` 并删除 `session_bindings`。 |
| `crates/agentdash-infrastructure/migrations/0072_permission_grants.sql` | permission grants，当前同时持有 `run_id` 与 `session_id`。 |
| `crates/agentdash-infrastructure/src/persistence/postgres/session_repository.rs` | `SessionMeta`、events、runtime commands、terminal effects、projection、lineage 的 Postgres store。 |
| `crates/agentdash-infrastructure/src/persistence/session_core.rs` | session rows / projection rows / runtime command / lineage row mapping。 |
| `crates/agentdash-infrastructure/src/persistence/postgres/workflow_repository.rs` | workflow/lifecycle definition、activity claim、lifecycle run repository。 |
| `crates/agentdash-infrastructure/src/persistence/postgres/run_link_repository.rs` | `lifecycle_run_links` repository。 |
| `crates/agentdash-infrastructure/src/persistence/postgres/story_repository.rs` | Story aggregate 持久化，Task 已写入 `stories.tasks` JSONB。 |
| `crates/agentdash-infrastructure/src/persistence/postgres/agent_repository.rs` | ProjectAgent persistence。 |
| `crates/agentdash-infrastructure/src/persistence/postgres/routine_repository.rs` | Routine / RoutineExecution persistence。 |
| `crates/agentdash-infrastructure/src/persistence/postgres/permission_grant_repository.rs` | permission grants repository。 |
| `crates/agentdash-infrastructure/src/persistence/postgres/backend_execution_lease_repository.rs` | backend lease repository。 |
| `crates/agentdash-contracts/src/session.rs` | generated session DTO source。 |
| `crates/agentdash-contracts/src/workflow.rs` | generated workflow/activity/lifecycle/run-link DTO source。 |
| `crates/agentdash-contracts/src/project_agent.rs` | generated ProjectAgent DTO source。 |
| `crates/agentdash-contracts/src/core.rs` | generated TaskResponse / StoryResponse source。 |
| `crates/agentdash-contracts/src/generate_ts.rs` | generated TS file boundary and check mode。 |
| `crates/agentdash-api/src/routes/acp_sessions.rs` | session CRUD, freeform lifecycle ensure, context/projection/lineage/prompt/cancel API。 |
| `crates/agentdash-api/src/routes/workflows.rs` | workflow/lifecycle APIs, still exposes domain `LifecycleRun` directly。 |
| `crates/agentdash-api/src/routes/story_runs.rs` | run-oriented Story runs API。 |
| `crates/agentdash-api/src/routes/story_sessions.rs` | story session compatibility-style surface backed by lifecycle run links。 |
| `crates/agentdash-api/src/routes/project_agents.rs` | ProjectAgent session open and default lifecycle launch。 |
| `crates/agentdash-api/src/routes/task_execution.rs` | Task direct execution API still returns session-oriented payload。 |
| `packages/app-web/src/generated/session-contracts.ts` | generated session DTOs consumed by frontend。 |
| `packages/app-web/src/generated/workflow-contracts.ts` | generated workflow/lifecycle DTOs consumed by frontend。 |
| `packages/app-web/src/generated/project-agent-contracts.ts` | generated ProjectAgent DTOs consumed by frontend。 |
| `packages/app-web/src/generated/core-contracts.ts` | generated TaskResponse with `lifecycle_step_key`。 |

### Current Field Distribution

#### Database / Persistence

- `sessions` is still the densest runtime fact table: `last_execution_status`, `last_turn_id`, `last_terminal_message`, `executor_config_json`, `executor_session_id`, `companion_context_json`, `visible_canvas_mount_ids_json` were baseline columns in `0001_init.sql:105`; `project_id` is later added by `0071_drop_session_bindings.sql:6`. `session_core::map_meta_row` maps these into `SessionMeta` including `project_id`, `executor_session_id`, companion context, visible canvas mounts and `bootstrap_state` at `crates/agentdash-infrastructure/src/persistence/session_core.rs:46`, `:55`, `:65`, `:67`, `:75`, `:81`.
- `session_events` is the runtime event log keyed by `session_id,event_seq` from `0001_init.sql:120`. API/session generated contracts expose the same stream as `SessionEventResponse.session_id/event_seq/.../notification` at `crates/agentdash-contracts/src/session.rs:19` and `packages/app-web/src/generated/session-contracts.ts:31`.
- `session_terminal_effects` is session+turn anchored outbox: `session_id`, `turn_id`, `terminal_event_seq`, `effect_type`, `payload_json`, `status` at `0034_session_terminal_effect_outbox.sql:1-8`. It represents durable terminal side effects, but has no run/actor/attempt anchor.
- `session_runtime_commands` is session-scoped runtime transition storage: `session_id`, `transition_id`, `phase_node`, `status`, `payload_json` at `0035_session_runtime_commands.sql:1-7`; repository maps `session_runtime_commands.status` at `crates/agentdash-infrastructure/src/persistence/session_core.rs:184`.
- `session_compactions`, `session_projection_segments`, `session_projection_heads` are session-scoped context projection stores at `0059_session_compaction_projection_store.sql:1`, `:39`, `:65`; `lifecycle_item_id` is stored inside session compactions at `0059_session_compaction_projection_store.sql:7`, but the owning frame is still `session_id`.
- `session_lineage` links `child_session_id -> parent_session_id` with `relation_kind` at `0060_session_lineage.sql:1-4`; generated session contracts expose the same parent/child session language at `packages/app-web/src/generated/session-contracts.ts:37`, `:39`, `:41`, `:45`.
- `lifecycle_runs` baseline originally had `binding_kind`, `binding_id`, `current_step_key`, `step_states` at `0001_init.sql:266`; `0008_lifecycle_run_session_id.sql:4` adds `session_id NOT NULL DEFAULT ''` and drops old binding columns at `0008_lifecycle_run_session_id.sql:8-9`; `0049_lifecycle_run_activity_state.sql:1` adds `activity_state`; `0068_drop_step_lifecycle_columns.sql:3-6` drops old step definition/run columns. Repository still persists `RUN_COLS = id,project_id,lifecycle_id,session_id,status,execution_log,activity_state,...` at `crates/agentdash-infrastructure/src/persistence/postgres/workflow_repository.rs:38`.
- `PostgresWorkflowRepository::list_by_session` still exists and queries `lifecycle_runs.session_id` at `crates/agentdash-infrastructure/src/persistence/postgres/workflow_repository.rs:631-635`.
- `activity_execution_claims` persists `run_id`, `activity_key`, `attempt`, `executor_kind`, `status`, `idempotency_key`, `executor_run_ref` at `0048_activity_execution_claims.sql:1-10`; repository serializes `executor_run_ref` at `crates/agentdash-infrastructure/src/persistence/postgres/workflow_repository.rs:449-460` and updates it at `:500-505`.
- `lifecycle_run_links` persists only run-level association: `run_id`, `subject_kind`, `subject_id`, `role`, `metadata`, `created_at` at `0070_lifecycle_run_links.sql:3-11`; repository exposes `LINK_COLS` at `crates/agentdash-infrastructure/src/persistence/postgres/run_link_repository.rs:11` and supports `list_by_run`, `list_by_subject`, `list_by_subject_and_role` at `:42`, `:55`, `:73`.
- `permission_grants` currently anchors both to `run_id` and `session_id`, plus source turn/tool, requested paths, grant scope and scope escalation intent at `0072_permission_grants.sql:4-15`; repository has both `list_active_by_session` and `list_active_by_run` at `crates/agentdash-infrastructure/src/persistence/postgres/permission_grant_repository.rs:116` and `:132`.
- `backend_execution_leases` is substrate-level placement keyed by `session_id,turn_id`, with backend/workspace/root_ref/selection/state at `0057_backend_execution_leases.sql:1-24`; repository maps `session_id` and `turn_id` at `crates/agentdash-infrastructure/src/persistence/postgres/backend_execution_lease_repository.rs:238-241`.
- `stories.tasks` JSONB is the active Task aggregate storage: migration adds it at `0020_stories_tasks_jsonb.sql:22-23`, then writes old task fields into each JSON object including `status`, `agent_binding`, `artifacts` at `0020_stories_tasks_jsonb.sql:74-83`; `PostgresStoryRepository` reads and writes `stories.tasks` at `crates/agentdash-infrastructure/src/persistence/postgres/story_repository.rs:37`, `:73-75`, `:402-419`.
- Old standalone `tasks` table remains in `0001_init.sql:78-93` and is only marked deprecated by `0020_stories_tasks_jsonb.sql:112`; `migration.rs` readiness does not require `tasks`, so code already treats it as non-authoritative.
- `project_agents` persists agent project instances with `config`, `default_lifecycle_key`, `is_default_for_story`, `is_default_for_task`, `knowledge_enabled` at `0044_project_agents.sql:19-35`; repository maps the same fields at `crates/agentdash-infrastructure/src/persistence/postgres/agent_repository.rs:23-37` and `:72-95`.
- `routine_executions` persists `resolved_prompt`, `session_id`, `status`, `entity_key`; repository inserts/updates it at `crates/agentdash-infrastructure/src/persistence/postgres/routine_repository.rs:248-257` and `:288-295`.
- Schema readiness currently omits `lifecycle_run_links` and `permission_grants`: `REQUIRED_POSTGRES_TABLES` includes `lifecycle_definitions`, `lifecycle_runs`, `sessions`, `workflow_definitions` etc. at `crates/agentdash-infrastructure/src/migration.rs:3-48`, but not the two newer tables used by repository bootstrap at `crates/agentdash-api/src/bootstrap/repositories.rs:104-106`.

#### Contracts / API / Generated

- `agentdash-contracts::workflow` still exposes `EffectiveSessionContract.active_step_key` at `crates/agentdash-contracts/src/workflow.rs:205`, `LifecycleExecutionEntry.step_key` at `:517`, and generated enum values `step_activated` / `step_completed` at `packages/app-web/src/generated/workflow-contracts.ts:76`.
- `ActivityAttemptState` is generated with `executor_run?: ExecutorRunRef` at `crates/agentdash-contracts/src/workflow.rs:410-416` and `packages/app-web/src/generated/workflow-contracts.ts:6`; `ExecutorRunRef.AgentSession { session_id }` is generated at `crates/agentdash-contracts/src/workflow.rs:476-477` and `packages/app-web/src/generated/workflow-contracts.ts:58`.
- `StoryRunOverviewDto` exposes `session_id?: string` and run links at `crates/agentdash-contracts/src/workflow.rs:596-606` / `packages/app-web/src/generated/workflow-contracts.ts:90`.
- `LifecycleRunLinkDto` / `AttachRunLinkRequest` expose only `run_id`, `subject_kind`, `subject_id`, `role`, `metadata` at `crates/agentdash-contracts/src/workflow.rs:582-630` / `packages/app-web/src/generated/workflow-contracts.ts:42`, `:78`, `:86`.
- `ProjectAgent` generated contracts expose `default_lifecycle_key`, `is_default_for_story`, `is_default_for_task`; create/update requests also accept `default_workflow_key`, but the persisted response does not store it at `crates/agentdash-contracts/src/project_agent.rs:73-130` and `packages/app-web/src/generated/project-agent-contracts.ts:6-20`.
- ProjectAgent session open returns `session_id` and `binding_id`, and `ProjectAgentSession` is also `binding_id + session_id` at `crates/agentdash-contracts/src/project_agent.rs:37-39`, `:65-69` / `packages/app-web/src/generated/project-agent-contracts.ts:8`, `:14`.
- `TaskResponse` still carries `lifecycle_step_key`, `status`, `agent_binding`, `artifacts` at `crates/agentdash-contracts/src/core.rs:1046-1074` / `packages/app-web/src/generated/core-contracts.ts:92`.
- `StartWorkflowRunRequest` is route-local, not generated, and requires `session_id` at `crates/agentdash-api/src/dto/workflow.rs:18-23`. `/workflows` run endpoints return domain `LifecycleRun` directly, not contract DTO, at `crates/agentdash-api/src/routes/workflows.rs:337`, `:392`, `:410`, `:436`.
- `/lifecycle-runs/by-session/{session_id}` is still a public session-first API at `crates/agentdash-api/src/routes/workflows.rs:405-428`.
- `story_sessions` exposes `SessionBindingResponse`-shaped objects (`owner_type`, `owner_id`, `label`, `binding_id=session_id`) even though `session_bindings` was dropped; it now builds them from `LifecycleRunLink` and `LifecycleRun.session_id` at `crates/agentdash-api/src/routes/story_sessions.rs:32-72`, `:129-180`.
- ProjectAgent session open creates a Session first, then starts default lifecycle/freeform lifecycle against that session at `crates/agentdash-api/src/routes/project_agents.rs:167-208`; `auto_start_lifecycle_run` passes `session_id` into `StartActivityLifecycleRunCommand` at `crates/agentdash-api/src/routes/project_agents.rs:624-675`.
- Task execution API remains session-first: `StartTaskResponse.session_id`, `ContinueTaskResponse.session_id`, `TaskSessionResponse.session_id/agent_binding/runtime_surface` at `crates/agentdash-api/src/dto/task_execution.rs:16-55`; route returns the same fields at `crates/agentdash-api/src/routes/task_execution.rs:48-50`, `:93-94`, `:142-168`.
- Frontend workflow types narrow `WorkflowRun.session_id` to required string at `packages/app-web/src/types/workflow.ts:292`; service mapper also requires it at `packages/app-web/src/services/workflow.ts:509`.
- Frontend workflow service maps `LifecycleExecutionEntry.step_key` at `packages/app-web/src/services/workflow.ts:405-412` and maps `ExecutorRunRef.agent_session.session_id` at `:421`.
- Frontend uses attempt `executor_run.session_id` for lifecycle session navigation at `packages/app-web/src/features/workflow/lifecycle-session-view.tsx:59-60` and checks attempt session ids on SessionPage at `packages/app-web/src/pages/SessionPage.tsx:419-421`.
- Frontend Story/Task surfaces still map `Task.lifecycle_step_key` at `packages/app-web/src/services/story.ts:208` and display it at `packages/app-web/src/pages/StoryPage.tsx:156-158`.
- Frontend ProjectAgent service hand-maps `OpenProjectAgentSessionResult.session_id/binding_id` and `ProjectAgentSession.session_id` at `packages/app-web/src/services/project.ts:61-95`.

### Field-Level Gaps

| Current field / surface | Current semantics | Problem | Target owner | Migration / deletion / rename recommendation |
| --- | --- | --- | --- | --- |
| `lifecycle_runs.session_id` | Root/current runtime session shortcut for a run; used for `list_by_session` and launch context. | Duplicates `ActivityAttemptState.executor_run.AgentSession.session_id`; makes RuntimeSession look like Lifecycle/ownership anchor. Cannot model multiple actors or multiple runtime sessions under one run. | `ActorFrame.runtime_session_refs`; optionally `Actor.root_runtime_session_ref`. | Backfill a root Actor for each non-null run session; move run-to-session queries to Actor/RuntimeSession ref index; drop `lifecycle_runs.session_id`; remove `/lifecycle-runs/by-session/{session_id}` or change to runtime-session ref lookup. |
| `ActivityAttemptState.executor_run.AgentSession.session_id` / `activity_execution_claims.executor_run_ref` | Attempt-level executor evidence points directly to session. | It records evidence but also acts as the only Agent identity/ref. Same Actor across attempts has no anchor; Task projection cannot answer "which Actor handled this SubjectRef" without session guessing. | Keep `ActivityAttemptState` as evidence; add `ActorAssignment` linking Actor -> ActivityAttemptState; ActorFrame owns runtime session refs. | Introduce `actor_assignments` or `activity_attempt_actor_id`; convert `executor_run.AgentSession` to `actor_assignment_ref` or `runtime_session_ref` nested under assignment. Keep attempt status/summary/artifacts. |
| `sessions.project_id` | Project ownership shortcut after dropping SessionBinding. Used for session permission/listing. | RuntimeSession still carries business ownership. It is useful as an index but should not be authoritative for control scope. | `LifecycleSubjectAssociation(role=control_scope)` / `ActorFrame.context_scope`; RuntimeSession may retain denormalized `project_id` for listing only. | Backfill control-scope associations for project sessions. API permission should resolve via Actor/Lifecycle association first. If retained, rename to `project_index_id` / document as denormalized index, not source of truth. |
| `sessions.executor_config_json` | Session launch executor config snapshot. | Config/procedure/capability facts belong to the effective ActorFrame, not unstructured SessionMeta. | `ActorFrame.execution_profile` / `ActorProcedure` ref. | Backfill from SessionMeta into root ActorFrame; subsequent launches should rebuild ActorFrame from procedure/capability sources. |
| `sessions.executor_session_id` | Connector-native session/resume id, derived from `ExecutorSessionBound` platform event. | This is RuntimeSession substrate, but currently also affects session prompt lifecycle and "follow-up" behavior. | RuntimeSession connector resume state, referenced by ActorFrame runtime refs. | Keep as RuntimeSession trace field if needed, but expose through ActorFrame/runtime ref for lifecycle control. |
| `sessions.companion_context_json` | Companion context snapshot on SessionMeta. | Companion/context source is an ActorFrame context fact; it should not be hidden inside one RuntimeSession row. | `ActorFrame.context_projection` and lifecycle/actor subject associations. | Move durable companion context to ActorFrame/context frame projection. RuntimeSession may retain event history only. |
| `sessions.visible_canvas_mount_ids_json` | Session-visible canvas/VFS runtime surface. | VFS/capability surface is part of effective ActorFrame. | `ActorFrame.vfs_surface` / capability dimension state. | Backfill into ActorFrame revision; remove from SessionMeta or keep as per-runtime UI cache only. |
| `sessions.last_execution_status`, `last_turn_id`, `last_terminal_message`, `bootstrap_state` | Session runtime summary and bootstrap lifecycle. | Useful runtime trace summary, but it is often consumed as Agent state. Actor running/waiting/completed should come from Actor/ActivityAttempt transitions. | RuntimeSession summary plus Actor state projection. | Keep for RuntimeSession list/status; add Actor status projection and update UI to use Actor/Lifecycle status when viewing lifecycle control plane. |
| `session_runtime_commands.session_id`, `phase_node`, `payload_json` | Requested/applied/failed capability/context runtime transitions for one session. | Capability/context transition facts are exactly ActorFrame evolution facts; `phase_node` still encodes workflow phase language, not ActorFrame revision. | `ActorFrameTransition` / `ActorFrameEvent`. | Create actor-frame transition store with actor/frame id, source runtime command id, status, payload. Backfill requested/applied commands by resolving session -> actor. Drop or rename `phase_node` to `activity_key` / `source_activity_key` only if it remains needed as provenance. |
| `session_compactions.*`, `session_projection_segments.*`, `session_projection_heads.*` | Model-visible context projection and compaction checkpoint scoped by session. | Context projection should be frame-owned so next turn, inspector and launch consume the same ActorFrame closure; session-only projection cannot represent shared Actor context or same-run multiple actors. | `ActorFrameContextProjection`; RuntimeSession event-range provenance. | Add `actor_id` / `frame_id` to projection stores or create parallel actor projection tables. Backfill current heads by root runtime session. Keep source event seq as RuntimeSession provenance. |
| `session_lineage.child_session_id/parent_session_id/relation_kind` | Session fork/companion/spawn lineage. | Lineage between agents/lifecycles is not always session lineage. It duplicates intended `LifecycleRunLink(SpawnedBy)` and cannot attach to Actor. | `LifecycleSubjectAssociation` / `ActorLineage`, with RuntimeSession lineage as trace provenance. | Backfill associations from session lineage where parent/child sessions resolve to runs/actors. Future spawn should write lifecycle/actor lineage first, runtime session lineage second if needed for trace UI. |
| `session_terminal_effects.session_id/turn_id/terminal_event_seq` | Durable side-effect outbox after session terminal event. | Business effects often target ActivityAttempt or Lifecycle state, but outbox cannot identify run/actor/attempt except via session lookup. | RuntimeSession terminal event plus ActivityAttempt terminal evidence / Actor effect outbox. | Add `run_id`, `actor_id`, `activity_key`, `attempt` nullable during migration, then require source refs for lifecycle effects. Session-only effects remain runtime effects. |
| `backend_execution_leases.session_id/turn_id` | Backend placement lease for one session turn. | Correct as runtime substrate, but ActorFrame should be able to point at current backend placement without inferring through session. | RuntimeSession turn lease, projected into ActorFrame runtime placement. | Keep table mostly intact; add actor/runtime-session ref indexing only if control plane needs actor-level active placement queries. |
| `permission_grants.run_id + session_id + grant_scope` | Grant lifecycle with run and session anchors; supports active-by-session and active-by-run queries. | Duplicates control scope; `GrantScope::WorkflowStep` naming is step-era; session-based query gives permission meaning to RuntimeSession. | `ActorFrame` / `LifecycleSubjectAssociation(role=control_scope)`; source runtime session/turn/tool as provenance. | Add `actor_id` or `frame_id`, rename scope values to `turn/session/activity/actor_frame` depending target. Deprecate `list_active_by_session`; keep `source_runtime_session_id` for provenance if needed. |
| `lifecycle_run_links.run_id, subject_kind, subject_id, role` | Whole-run subject association for Story/Task/Project/RoutineExecution/LifecycleRun/External. | No actor anchor; cannot express same-run `SubjectRef(kind=Task)` being handled by a specific Actor. ProjectAgent is not a subject kind. | `LifecycleSubjectAssociation(anchor_run_id, anchor_actor_id?, subject_ref, role, metadata)`. | Add nullable `anchor_actor_id`; consider rename table after code switch. Add `project_agent` subject kind only if ProjectAgent needs first-class subject/source links. Do not add ActivityAttempt as association anchor; use Actor assignment for evidence. |
| `workflow_definitions.binding_kinds` / `lifecycle_definitions.binding_kinds` | Catalog/filter and launch applicability (`project`, `story`). | Mixes catalog filter, launch scope, subject requirements and capability contract. `task` has already been collapsed into `story`, but Task SubjectRef still needs runtime dispatch policy. | Workflow catalog metadata; launch/subject requirements belong to Lifecycle/Procedure/Association policy. | Split into explicit fields before renaming: `catalog_target_kinds`, `launch_subject_requirements`, `control_scope_policy` or equivalent. Do not reintroduce `task` as workflow binding kind; use SubjectRef policy. |
| `WorkflowDefinition` table/type vs `ActivityLifecycleDefinition` table/type | `WorkflowDefinition` is single Agent Activity contract; `ActivityLifecycleDefinition` is executable graph. | Names are inverted relative to target: Workflow should be graph config, single Agent behavior should be Actor/ActivityProcedure. | `Workflow` for graph; `ActorProcedure` / `ActivityProcedure` for single-agent contract. | Plan table/type/API rename as a breaking contract update: `lifecycle_definitions` -> workflow graph target; `workflow_definitions` -> actor/activity procedures. |
| `EffectiveSessionContract.active_step_key` | Session contract says current active step. | Step terminology survived Activity migration. | `ActorFrame.active_activity_key` or `ActivityProcedure` context. | Rename to `active_activity_key`; regenerate TS; update frontend mappers. |
| `LifecycleExecutionEntry.step_key` and `LifecycleExecutionEventKind::Step*` | Execution log event refers to step key and step activation/completion. | Step-era language conflicts with Activity-only runtime and target `ActivityAttemptState`. | Lifecycle event log with `activity_key` / event kind `activity_*`. | Rename JSON fields/enum values and migrate stored `execution_log` JSON. |
| `ProjectAgent.default_lifecycle_key` | Agent launch default lifecycle. | Reasonable launch policy, but currently opening a ProjectAgent creates a session first and maybe starts a lifecycle. | Actor launch policy / default Lifecycle template. | Keep concept but change open flow to create Lifecycle/Actor first, RuntimeSession second. Response should return `lifecycle_run_id`, `actor_id`, `runtime_session_id`. |
| `ProjectAgent default_workflow_key` request field | API convenience to auto-create one-activity lifecycle from a single workflow. | Keeps old "workflow as single agent contract" naming alive and is not persisted as its own field. | ActorProcedure selection or explicit Workflow graph config. | Remove request field in target contract. UI should select explicit Workflow graph or ActorProcedure according to renamed model. |
| `ProjectAgent.is_default_for_task` | Task default Agent flag. | Gives Task a runtime dispatch meaning. Target says Task is data/view/payload; runtime uses SubjectRef. | SubjectRef dispatch policy under Story/Project. | Replace with `default_for_subject_kinds` / project dispatch policy if still needed; do not make Task entity own runtime defaults. |
| `OpenProjectAgentSessionResult.session_id/binding_id` and `ProjectAgentSession` | Product UI surface for ProjectAgent sessions. | `binding_id` is just `session_id`; Actor/lifecycle run is hidden. | ProjectAgent Actor instance/session view. | Replace with `actor_id`, `lifecycle_run_id`, `runtime_session_id`; remove `binding_id` unless a real association id is returned. |
| `RoutineExecution.session_id/status` | Routine execution records prompt dispatch and resulting session id. | Status is dispatch/session status, not Agent terminal truth; no run/source association. | `LifecycleSubjectAssociation(subject=RoutineExecution, role=source)` plus Actor/Run status. | Add `run_id`; create Source link when routine dispatches. Keep `session_id` only as source runtime trace until Actor model lands, then rename to `runtime_session_id` or remove from primary response. |
| `TaskResponse.lifecycle_step_key` | Task points at old lifecycle step/activity key. | Task acquires runtime location. Target says Task is data/view/payload; runtime uses `SubjectRef(kind=Task)`. | SubjectRef + Actor association + ActivityAttempt evidence. | Remove from TaskResponse/spec. If UI needs placement, provide TaskExecutionProjection DTO derived from subject association and actor assignment. |
| `Task.status` / `Task.artifacts` in `stories.tasks` | Read-only projection per domain comments, persisted inside Story aggregate. | Projection facts live in task data row; duplicates ActivityAttempt/artifact state. | Task view projection generated from ActivityAttemptState and lifecycle artifacts. | Split `TaskSpec` from `TaskProjection` in contracts/storage. If persisted for view cache, make it clearly projection cache with source revision, not task runtime truth. |
| `Task.agent_binding` | Task-specific agent config/context. | Could be Activity payload/procedure override, but as Task field it suggests Task owns execution rules. | SubjectRef payload / ActorProcedure override / dispatch policy. | Move execution-facing fields out of Task spec or rename to payload/request metadata. |
| Task execution `StartTaskResponse.session_id`, `TaskSessionResponse.session_id/runtime_surface` | Direct task execution launches/returns session surface. | Bypasses Lifecycle -> Actor -> ActorFrame and perpetuates Task runtime. | Subject execution response: `subject_ref`, `actor_id`, `lifecycle_run_id`, optional `runtime_session_id`. | Replace task execution API with subject dispatch API; build task panel from TaskExecutionProjection. |
| Story session `SessionBindingResponse.owner_type/owner_id/label` | Old SessionBinding-shaped response backed by lifecycle links. | API name/shape claims session binding exists after it was dropped. | Story run / actor / runtime session view. | Replace `/stories/{id}/sessions` payload with run/actor/session projection; remove owner_type/owner_id duplication. |
| `WorkflowRun.session_id` in frontend local type | Required frontend field for raw domain run. | Backend run `session_id` is optional after migration 0070; local type is stricter than actual schema. | Actor/runtime session refs. | Contractualize LifecycleRun DTO in `agentdash-contracts`; frontend consumes generated type. Replace required `session_id` with actor/session refs. |

### DB Migration Order

1. Schema readiness and baseline cleanup:
   - Add `lifecycle_run_links` and `permission_grants` to `REQUIRED_POSTGRES_TABLES`; they are required by repository bootstrap but missing from readiness at `migration.rs:3-48`.
   - For pre-production target schema, update baseline `0001_init.sql` alongside forward migrations: remove old standalone `tasks` and `session_bindings`, remove step columns from lifecycle baseline, and keep the forward migrations for existing dev DBs.

2. Introduce target anchors before deleting any current fields:
   - Create `lifecycle_actors`, `actor_frames`, `actor_runtime_sessions` or equivalent tables.
   - Extend/replace `lifecycle_run_links` with actor anchor: `anchor_run_id`, nullable `anchor_actor_id`, `subject_kind`, `subject_id`, `role`, `metadata`, `created_at`.
   - Add actor/frame refs to permission grants, terminal effects and context/runtime transition stores where those facts need lifecycle control-plane ownership.

3. Backfill current data:
   - For each `lifecycle_runs.session_id`, create one root Actor + ActorFrame + runtime session ref.
   - For each `ActivityAttemptState.executor_run.AgentSession` and `activity_execution_claims.executor_run_ref`, create/reuse Actor assignment to `(run_id, activity_key, attempt)` and link the runtime session as evidence.
   - For `lifecycle_run_links`, keep whole-run links; Task links remain run-level unless an Actor can be inferred from attempt/session evidence.
   - For `permission_grants`, resolve `run_id + session_id` to ActorFrame where possible; keep turn/tool ids as runtime provenance.
   - For `session_runtime_commands` and context projection heads, backfill to ActorFrame transitions/projection from session -> actor mapping.
   - For `session_lineage`, create lifecycle/actor lineage associations when both sessions resolve to actors/runs.
   - For RoutineExecution, create Source links to run once `run_id` is available.

4. Switch repositories and APIs in one breaking boundary:
   - LifecycleRun repository stops writing `session_id`; session-first queries route through actor/runtime-session refs.
   - Activity attempt writes Actor assignment and runtime session evidence.
   - Permission and context projection APIs query ActorFrame/control-scope first, not session first.
   - ProjectAgent open and Task execution dispatch create Lifecycle/Actor first, then attach RuntimeSession.

5. Drop or rename old fields:
   - Drop `lifecycle_runs.session_id`.
   - Drop or repurpose `activity_execution_claims.executor_run_ref` after Actor assignment carries runtime evidence.
   - Drop old standalone `tasks` table from clean baseline and forward migration.
   - Rename `LifecycleExecutionEntry.step_key` -> `activity_key`, `StepActivated/StepCompleted` -> `ActivityActivated/ActivityCompleted`.
   - Remove `EffectiveSessionContract.active_step_key`.
   - Remove `ProjectAgent.default_workflow_key` request field and replace Task-default flags with subject dispatch policy.
   - Remove story/session `SessionBindingResponse` surfaces once replacement DTOs exist.

### Contract Update Boundary

- Move route-local lifecycle run DTOs into `agentdash-contracts` before changing wire fields. Today `/workflows` returns domain `LifecycleRun` directly at `crates/agentdash-api/src/routes/workflows.rs:337`, `:392`, `:410`, `:436`, while frontend hand-maps it in `packages/app-web/src/services/workflow.ts:497-518`.
- `workflow-contracts.ts` must change for:
  - `ActivityAttemptState.executor_run` / `ExecutorRunRef.AgentSession`.
  - `StoryRunOverviewDto.session_id`.
  - `LifecycleRunLinkDto` / `AttachRunLinkRequest` actor anchor fields.
  - `EffectiveSessionContract.active_step_key`.
  - `LifecycleExecutionEntry.step_key` and `LifecycleExecutionEventKind` enum values.
- `session-contracts.ts` can remain RuntimeSession-oriented for event stream/projection/lineage, but ActorFrame APIs should not be added as more session fields. Add new actor/lifecycle generated contracts instead of expanding SessionMeta ownership.
- `project-agent-contracts.ts` must change for ProjectAgent session open:
  - `OpenProjectAgentSessionResult.session_id/binding_id` -> `actor_id/lifecycle_run_id/runtime_session_id`.
  - `ProjectAgentSession` -> ProjectAgent actor/runtime-session overview.
  - remove `default_workflow_key` from create/update if single-agent contract is renamed to ActorProcedure.
- `core-contracts.ts` must change for Task:
  - remove `lifecycle_step_key`.
  - split Task spec from Task projection if `status/artifacts/agent_binding` are no longer Task truth.
- Task execution, story session, project session and permission grant DTOs are partly route-local today. Before target changes, decide whether they enter `agentdash-contracts` so frontend drift is detected by `pnpm run contracts:check`.
- Generated boundary command is `cargo run -p agentdash-contracts --bin generate_contracts_ts`; project check script includes `pnpm run contracts:check` at `package.json:42-43`.

### Frontend Generated / Consumer Impact

- `packages/app-web/src/generated/workflow-contracts.ts` has the main breaking shape: `ExecutorRunRef` session ref, StoryRunOverview session ref, run link no actor anchor, step event names.
- `packages/app-web/src/services/workflow.ts` must stop requiring `run.session_id` (`:509`) and stop interpreting `executor_run.agent_session.session_id` as the activity navigation target (`:421`).
- `packages/app-web/src/features/workflow/lifecycle-session-view.tsx` and `packages/app-web/src/pages/SessionPage.tsx` currently navigate/inspect via attempt session id (`lifecycle-session-view.tsx:59-60`, `SessionPage.tsx:419-421`); these should pivot to Actor/assignment/runtime session refs.
- `packages/app-web/src/generated/project-agent-contracts.ts` and `packages/app-web/src/services/project.ts` are directly affected by replacing ProjectAgent session open/result fields (`project.ts:61-95`).
- `packages/app-web/src/generated/core-contracts.ts` / `packages/app-web/src/services/story.ts` / `StoryPage.tsx` are affected by removing `Task.lifecycle_step_key` (`core-contracts.ts:92`, `story.ts:208`, `StoryPage.tsx:156-158`).
- `packages/app-web/src/services/story.ts` and `features/task/task-agent-session-panel.tsx` currently expect task session responses with `session_id`, `agent_binding`, `runtime_surface`; these should consume a SubjectExecutionProjection / ActorFrame projection instead.
- `packages/app-web/src/services/permission.ts` currently sends `session_id` query params; permission UI/API should switch to run/actor/frame refs after grant migration.

### Code Patterns

- Session fact projection is event-derived and merged into `sessions` rows: `projection_from_envelope` maps `TurnStarted`, `TurnCompleted`, `Error`, `ExecutorSessionBound` and `turn_terminal` meta updates to `last_execution_status`, `last_terminal_message`, `executor_session_id` at `crates/agentdash-infrastructure/src/persistence/session_core.rs:667-733`.
- `save_session_meta` deliberately merges stale meta so event-derived execution status wins over old saves at `crates/agentdash-infrastructure/src/persistence/postgres/session_repository.rs:244-267`; this reinforces that SessionMeta fields are runtime projection, not lifecycle control-plane truth.
- `LifecycleRun::new_activity` still accepts `session_id: Option<String>` and stores it at `crates/agentdash-domain/src/workflow/entity.rs:227-247`; `bind_runtime_session` mutates it at `:255-256`.
- Domain `Task` explicitly states it does not hold internal session id or executor resume id at `crates/agentdash-domain/src/task/entity.rs:12`, but still has `lifecycle_step_key`, `status`, `agent_binding`, `artifacts` at `:34-42`.
- `LifecycleRunLink` docs state it replaces `LifecycleRun.session_id -> SessionBinding -> Story` reverse lookup at `crates/agentdash-domain/src/workflow/run_link.rs:7-10`, but the table/repository still cannot anchor a subject to Actor.
- ProjectAgent open flow creates/updates a session, marks owner bootstrap pending, then starts default/freeform lifecycle at `crates/agentdash-api/src/routes/project_agents.rs:167-208`; target flow should invert this to Lifecycle/Actor first.
- Story session API creates a session, ensures freeform lifecycle run, then attaches Story link by session lookup at `crates/agentdash-api/src/routes/story_sessions.rs:140-169` and `:227-256`; target should attach Story subject during Lifecycle/Actor dispatch, not by reverse session lookup.

### Related Specs

- `.trellis/spec/backend/database-guidelines.md`
- `.trellis/spec/backend/repository-pattern.md`
- `.trellis/spec/backend/session/architecture.md`
- `.trellis/spec/backend/session/runtime-execution-state.md`
- `.trellis/spec/backend/session/execution-context-frames.md`
- `.trellis/spec/backend/workflow/architecture.md`
- `.trellis/spec/backend/workflow/activity-lifecycle.md`
- `.trellis/spec/backend/workflow/lifecycle-run-link.md`
- `.trellis/spec/cross-layer/frontend-backend-contracts.md`
- `.trellis/spec/frontend/workflow-activity-lifecycle.md`

### External References

- None. This slice used only local source, migrations, generated contracts and project specs.

## Caveats / Not Found

- `python ./.trellis/scripts/task.py current --source` returned no active task; this file was written to the explicit task path supplied in the prompt.
- No `Actor`, `ActorFrame`, `RuntimeSession` target entities/tables/contracts were found in the scanned persistence/API/contract code; all target ownership recommendations are gap mapping, not existing implementation.
- `LifecycleRunLink` exists only as run-level association; no actor-level subject association table or DTO exists.
- `/workflows` lifecycle run APIs still return domain `LifecycleRun` directly; generated workflow contracts cover sub-shapes but not the top-level run DTO.
- This was a research-only pass. No migrations, code, generated TS, specs or tests were modified.
