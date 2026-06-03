# Research: Workflow/lifecycle schema

- Query: 正式评估 Workflow/lifecycle 分区中 agent_procedures、workflow_graphs、lifecycle_runs、lifecycle_workflow_instances、lifecycle_agents、agent_frames、agent_assignments、lifecycle_subject_associations、lifecycle_gates、agent_lineages、runtime_session_execution_anchors、activity_execution_claims、routine_executions、routines 的表/字段语义正确性。
- Scope: internal
- Date: 2026-06-03

## Findings

### Scope Inputs

任务要求当前 PostgreSQL baseline 不是旧 migration 链 dump 的兼容结果，而是表达当前领域事实的 schema；审计必须区分 business fact、runtime fact、projection/cache、outbox/audit、historical residue，并结合 repository、domain、application、API、frontend contract 与 spec 证据。任务 PRD 明确允许在预研期删除历史包袱，不为旧库提供兼容迁移路径（`.trellis/tasks/06-03-database-semantic-baseline-audit/prd.md`）。设计文档要求每个候选项给出建议动作与风险（`.trellis/tasks/06-03-database-semantic-baseline-audit/design.md`），实施计划把本分区定义为 Workflow lifecycle slice（`.trellis/tasks/06-03-database-semantic-baseline-audit/implement.md`）。

### Related Specs

- `.trellis/spec/backend/workflow/architecture.md`: `WorkflowGraph` 是 graph definition 主模型，`LifecycleRun` 是 tracked life process / control ledger；Activity/attempt identity 必须包含 `graph_instance_id`；Agent Activity execution identity 使用 `AgentAssignment(run_id, graph_instance_id, activity_key, attempt, agent_id, frame_id)`。
- `.trellis/spec/backend/workflow/activity-lifecycle.md`: `WorkflowDefinition` 的目标语义是 `AgentProcedure`，不是 graph config；`WorkflowGraphInstance` 是 run 内 graph instance；RuntimeSession 只保留 runtime evidence；AgentAssignment 桥接 activity attempt 与 Agent/Frame。
- `.trellis/spec/backend/workflow/lifecycle-run-link.md`: `LifecycleSubjectAssociation` 表达 SubjectRef 到 whole run 或 LifecycleAgent 的关系；RuntimeSession 降级为 runtime trace container，不承载 business ownership。
- `.trellis/spec/backend/workflow/lifecycle-edge.md`: `active_node_keys` 用于 runtime advancement 判断，但当前目标模型里活跃 Activity 应来自 graph instance activity state。
- `.trellis/spec/backend/session/execution-context-frames.md`: `ExecutionContext` 是 connector-facing projection；MCP、VFS、context、capability 等是 launch/turn 投影，不应写回为 session 架构事实源。
- `.trellis/spec/backend/session/runtime-execution-state.md`: runtime state 边界只回答 turn claim/active/cancel/terminal cleanup；connector projection、tool/context hot update 不应成为业务事实源。
- `.trellis/spec/backend/session/session-lineage-projection.md`: session lineage 是 runtime trace/debug projection；business visibility 由 LifecycleSubjectAssociation 和 AgentLineage 投影。
- `.trellis/spec/frontend/workflow-activity-lifecycle.md`: 前端 target view 以 run / graph instance / subject / agent / frame 为主索引，RuntimeSession 只作为 trace drill-down；`LifecycleRunView` 包含 graph instances、agents、subject associations、runtime trace refs、execution log。

### Files Found

- `crates/agentdash-infrastructure/migrations/0001_init.sql`: 当前 PostgreSQL baseline，含本次审计的表定义、索引、FK 与 dump 风格约束。
- `crates/agentdash-infrastructure/src/persistence/postgres/workflow_repository.rs`: agent_procedures、workflow_graphs、lifecycle_runs、activity_execution_claims repository SQL。
- `crates/agentdash-infrastructure/src/persistence/postgres/lifecycle_anchor_repository.rs`: lifecycle_workflow_instances、lifecycle_agents、agent_frames、agent_assignments、subject associations、gates、lineages、runtime_session_execution_anchors repository SQL。
- `crates/agentdash-infrastructure/src/persistence/postgres/routine_repository.rs`: routines 与 routine_executions repository SQL。
- `crates/agentdash-domain/src/workflow/entity.rs`: AgentProcedure、WorkflowGraph、ActivityExecutionClaim、LifecycleRun domain shape。
- `crates/agentdash-domain/src/workflow/workflow_graph_instance.rs`: WorkflowGraphInstance domain shape。
- `crates/agentdash-domain/src/workflow/lifecycle_agent.rs`: LifecycleAgent domain shape。
- `crates/agentdash-domain/src/workflow/agent_frame.rs`: AgentFrame revision snapshot、runtime_session_refs 与 canvas refs domain shape。
- `crates/agentdash-domain/src/workflow/agent_assignment.rs`: AgentAssignment execution bridge。
- `crates/agentdash-domain/src/workflow/lifecycle_subject_association.rs`: subject-to-run/agent association。
- `crates/agentdash-domain/src/workflow/lifecycle_gate.rs`: durable gate/wait point。
- `crates/agentdash-domain/src/workflow/agent_lineage.rs`: agent control tree edge。
- `crates/agentdash-domain/src/workflow/runtime_session_anchor.rs`: RuntimeSession -> control-plane launch evidence anchor。
- `crates/agentdash-domain/src/routine/entity.rs`: Routine、RoutineExecution、RoutineDispatchRefs domain shape。
- `crates/agentdash-application/src/workflow/dispatch_service.rs`: execution intent dispatch creates run、graph instance、agent、frame、assignment、gate、lineage、runtime session anchor。
- `crates/agentdash-application/src/workflow/activity_run.rs`, `scheduler.rs`, `orchestrator.rs`: Activity lifecycle state advancement, durable claims, terminal callback and ready attempt launch.
- `crates/agentdash-application/src/workflow/execution_log.rs`: hook pending execution log flushing and activity artifact helpers.
- `crates/agentdash-application/src/workflow/frame_builder.rs`, `frame_surface.rs`, `runtime_launch.rs`: AgentFrame surface construction and runtime session refs.
- `crates/agentdash-application/src/workflow/lifecycle_run_view_builder.rs`: target read model assembly with runtime trace refs and execution log.
- `crates/agentdash-application/src/routine/executor.rs`, `dispatch.rs`, `reuse_resolver.rs`: routine trigger execution, dispatch intent and reuse validation.
- `crates/agentdash-api/src/routes/workflows.rs`, `dto/workflow.rs`: definition APIs and lifecycle run start/human decision routes.
- `crates/agentdash-api/src/routes/lifecycle_views.rs`: target lifecycle run/agent/frame/runtime trace view routes.
- `crates/agentdash-api/src/routes/routines.rs`, `dto/routine.rs`: routine CRUD, webhook fire, execution history API.
- `packages/app-web/src/generated/workflow-contracts.ts`, `packages/app-web/src/services/lifecycle.ts`, `packages/app-web/src/features/routine/execution-history-panel.tsx`: frontend generated/view usage of lifecycle and routine dispatch refs.

### Code Patterns

- Definition tables already align with target domain: `AgentProcedure` carries project/key/name/source/version/contract/installed source metadata (`crates/agentdash-domain/src/workflow/entity.rs:15`), while `WorkflowGraph` carries entry activity, activities and transitions (`crates/agentdash-domain/src/workflow/entity.rs:72`). Repository SQL maps the same columns (`crates/agentdash-infrastructure/src/persistence/postgres/workflow_repository.rs:25`, `crates/agentdash-infrastructure/src/persistence/postgres/workflow_repository.rs:37`).
- LifecycleRun currently mixes ledger fields and read-model fields: domain comments mark `active_node_keys` as read-model-only derived from `WorkflowGraphInstance.activity_state` (`crates/agentdash-domain/src/workflow/entity.rs:203`), and `sync_graph_instance_activity_projections` recomputes it from graph instance states (`crates/agentdash-domain/src/workflow/entity.rs:249`). Repository persists it in `lifecycle_runs` (`crates/agentdash-infrastructure/src/persistence/postgres/workflow_repository.rs:37`, `crates/agentdash-infrastructure/src/persistence/postgres/workflow_repository.rs:614`).
- `record_artifacts` is present in migration (`crates/agentdash-infrastructure/migrations/0001_init.sql:397`) and insert columns (`crates/agentdash-infrastructure/src/persistence/postgres/workflow_repository.rs:38`), but is absent from `RUN_COLS`, `LifecycleRun`, update SQL, DTO and frontend contracts. This is high-confidence historical residue.
- `execution_log` is a hook/runtime audit timeline flushed from `PendingExecutionLogEntry` into `LifecycleRun.execution_log` (`crates/agentdash-spi/src/hooks/mod.rs:775`, `crates/agentdash-application/src/workflow/execution_log.rs:43`) and exposed in `LifecycleRunView` (`crates/agentdash-contracts/src/workflow.rs:741`). It is useful but currently stored as a mutable JSON array on `lifecycle_runs`.
- Activity claim identity is correct in domain: `ActivityExecutionClaim::new` builds idempotency key from `run_id:graph_instance_id:activity_key:attempt` (`crates/agentdash-domain/src/workflow/entity.rs:124`), and migration has a partial unique active-attempt index on `(run_id, graph_instance_id, activity_key, attempt)` (`crates/agentdash-infrastructure/migrations/0001_init.sql:2254`). The column default zero UUID in baseline is dump/backfill residue (`crates/agentdash-infrastructure/migrations/0001_init.sql:21`).
- `AgentFrame` is intentionally a revision snapshot for effective capability/context/VFS/MCP/execution profile/runtime refs (`crates/agentdash-domain/src/workflow/agent_frame.rs:27`), but runtime refs are explicitly trace/delivery refs, not subject association (`crates/agentdash-domain/src/workflow/agent_frame.rs:25`). `runtime_session_execution_anchors` was added to replace JSON contains lookup from frame refs (`crates/agentdash-domain/src/workflow/runtime_session_anchor.rs:8`).
- `runtime_session_execution_anchors` migration uses mixed SQL types: `runtime_session_id text`, but `run_id`, `launch_frame_id`, `agent_id`, `assignment_id`, `graph_instance_id` are `uuid` (`crates/agentdash-infrastructure/migrations/0001_init.sql:724`). Repository binds all UUIDs as strings (`crates/agentdash-infrastructure/src/persistence/postgres/lifecycle_anchor_repository.rs:1173`), while other lifecycle tables store IDs as text. This is the clearest type normalization defect.
- Routine dispatch refs are domain facts for the trigger execution audit: `RoutineExecution` has `dispatch_refs` (`crates/agentdash-domain/src/routine/entity.rs:120`), `mark_dispatched` writes them (`crates/agentdash-domain/src/routine/entity.rs:151`), the executor records them after dispatch (`crates/agentdash-application/src/routine/executor.rs:262`), and reuse resolver validates run/agent/frame/assignment consistency before reuse (`crates/agentdash-application/src/routine/reuse_resolver.rs:125`). They should be retained.

### Table-by-table Audit

#### agent_procedures

Definition: `CREATE TABLE public.agent_procedures` at `0001_init.sql:104`. Domain: `AgentProcedure` at `entity.rs:15`. Repository: `WF_COLS` at `workflow_repository.rs:25`.

- Field classification:
  - Business/definition fact: `id`, `project_id`, `key`, `name`, `description`, `source`, `version`, `contract`, `created_at`, `updated_at`.
  - Business/definition provenance: `library_asset_id`, `source_ref`, `source_version`, `source_digest`, `installed_at`.
  - Historical residue: dump-style `CONSTRAINT *_not_null` names and `project_id DEFAULT '00000000-...'`.
- Recommendation: keep table. Normalize constraints/defaults in baseline: remove zero UUID default on `project_id`, prefer plain `NOT NULL`, keep unique `(project_id, key)` (`0001_init.sql:2247`). Optional future name alignment: table name is already target semantic; API route still uses `/agent-procedures`, which is acceptable.
- Priority: P2 keep-but-normalize.

#### workflow_graphs

Definition: `CREATE TABLE public.workflow_graphs` at `0001_init.sql:1048`; unique `(project_id, key)` at `0001_init.sql:1855`. Domain: `WorkflowGraph` at `entity.rs:72`.

- Field classification:
  - Business/definition fact: `id`, `project_id`, `key`, `name`, `description`, `source`, `version`, `entry_activity_key`, `activities`, `transitions`, `created_at`, `updated_at`.
  - Business/definition provenance: `library_asset_id`, `source_ref`, `source_version`, `source_digest`, `installed_at`.
  - Historical residue: dump/backfill style constraint names if present, and possible `text` JSON columns rather than `jsonb` for `activities`/`transitions`.
- Recommendation: keep. Consider `activities`/`transitions` as `jsonb NOT NULL` in new baseline if the project wants queryability and JSON validation consistency, but repository currently serializes text and would need code changes.
- Priority: P2 keep-but-normalize; JSONB is optional P3 code-touching cleanup.

#### lifecycle_runs

Definition: `CREATE TABLE public.lifecycle_runs` at `0001_init.sql:392`. Domain: `LifecycleRun` at `entity.rs:194`. Repository columns at `workflow_repository.rs:37`.

- Field classification:
  - Runtime/control ledger fact: `id`, `project_id`, `root_graph_id`, `status`, `created_at`, `updated_at`, `last_activity_at`.
  - Projection/cache: `active_node_keys` is derived from `WorkflowGraphInstance.activity_state` (`entity.rs:203`, `entity.rs:249`).
  - Outbox/audit/runtime trace: `execution_log` records hook/runtime lifecycle execution entries (`execution_log.rs:43`, `agentdash-spi/src/hooks/mod.rs:775`).
  - Historical residue: `record_artifacts`; dump-style `root_graph_id` constraint named `lifecycle_runs_lifecycle_id_not_null`.
- Recommendation:
  - Remove `record_artifacts` directly with repository insert cleanup. It is not read into domain and is only inserted as a placeholder via `RUN_INSERT_COLS`.
  - Keep `active_node_keys` only as a read model if current API/tools still need cheap active display; better target is to remove from `lifecycle_runs` after updating API/tool callers to derive active attempts from `lifecycle_workflow_instances.activity_state_json`.
  - Move `execution_log` out to append-only `lifecycle_execution_events` or equivalent if the next schema slice can touch code. The current JSON array is system/runtime behavior mixed into the ledger row; it is still needed by `LifecycleRunView`, VFS lifecycle provider, hook snapshot and journey surfaces, so not directly removable.
  - Rename constraint/defaults in baseline: `root_graph_id` not `lifecycle_id`.
- Priority: P0 remove `record_artifacts`; P1 migrate `execution_log`; P1/P2 remove or demote `active_node_keys` from persisted source-of-truth.

#### lifecycle_workflow_instances

Definition: `CREATE TABLE public.lifecycle_workflow_instances` at `0001_init.sql:426`. Domain: `WorkflowGraphInstance` at `workflow_graph_instance.rs:12`.

- Field classification:
  - Runtime fact: `id`, `run_id`, `graph_id`, `role`, `status`, `activity_state_json`, `created_at`, `updated_at`.
  - Projection/cache: `status` can be derived from `activity_state_json.status` but is useful for indexed list/read surfaces.
- Recommendation: keep. This table is the correct location for activity state; `LifecycleRun.active_node_keys` should derive from this table rather than compete with it. Add/verify uniqueness for `(run_id, role)` when `role='root'`, matching spec; current baseline only has PK and FK, so root uniqueness is missing from the visible migration excerpt.
- Priority: P1 keep but add constraints/indexes.

#### lifecycle_agents

Definition: `CREATE TABLE public.lifecycle_agents` at `0001_init.sql:354`. Domain: `LifecycleAgent` at `lifecycle_agent.rs:19`.

- Field classification:
  - Runtime/control fact: `id`, `run_id`, `project_id`, `agent_kind`, `agent_role`, `project_agent_id`, `status`, `current_frame_id`, `bootstrap_status`, `created_at`, `updated_at`.
  - Projection/cache: `current_frame_id` is a current pointer to latest effective frame; can be derived from max frame revision but is useful.
- Recommendation: keep. Add FK for `current_frame_id` to `agent_frames(id)` if cyclic creation/update order is acceptable; currently only `agent_frames.agent_id` has FK (`0001_init.sql:2294`). Status/bootstrap values should get CHECK constraints or typed enums in baseline.
- Priority: P2 keep-but-constrain.

#### agent_frames

Definition: `CREATE TABLE public.agent_frames` at `0001_init.sql:64`. Domain: `AgentFrame` at `agent_frame.rs:27`.

- Field classification:
  - Runtime snapshot fact: `id`, `agent_id`, `revision`, `procedure_id`, `graph_instance_id`, `activity_key`, `effective_capability_json`, `context_slice_json`, `vfs_surface_json`, `mcp_surface_json`, `execution_profile_json`, `created_by_kind`, `created_by_id`, `created_at`.
  - Runtime trace/provenance refs: `runtime_session_refs_json`.
  - Mutable projection/UI runtime: `visible_canvas_mount_ids_json`.
  - Potential historical residue / needs split: keeping runtime refs and visible canvas refs in the same immutable-ish revision table blurs revision snapshot and live trace/update state.
- Recommendation:
  - Keep capability/context/VFS/MCP/execution profile in one frame table for now. The domain and frame builder treat a frame as an effective runtime surface revision; this is semantically coherent.
  - Keep `runtime_session_refs_json` during transition because many view paths still collect runtime trace refs from frames, but it should be downgraded to projection or replaced by normalized `runtime_session_execution_anchors` for lookup. The domain already says anchor replaces JSON contains lookup (`runtime_session_anchor.rs:8`), yet repository still has `find_by_runtime_session` using JSON containment (`lifecycle_anchor_repository.rs:558`).
  - Move `visible_canvas_mount_ids_json` out or mark it as mutable frame projection. Domain comment says it is runtime-appended and not copied with revision (`agent_frame.rs:51`), which conflicts with the table’s revision-snapshot meaning.
  - Add uniqueness `(agent_id, revision)` and FKs for `procedure_id` and `graph_instance_id`; current visible migration only shows FK to `lifecycle_agents`.
- Priority: P1 split/normalize runtime_session refs and visible canvas refs; P2 add uniqueness/FKs/checks.

#### agent_assignments

Definition: `CREATE TABLE public.agent_assignments` at `0001_init.sql:29`. Domain: `AgentAssignment` at `agent_assignment.rs:10`.

- Field classification:
  - Runtime execution bridge fact: `id`, `run_id`, `graph_instance_id`, `activity_key`, `attempt`, `agent_id`, `frame_id`, `lease_status`, `assigned_at`, `released_at`.
- Recommendation: keep. This is the target bridge required by workflow specs. Add FK to `lifecycle_workflow_instances(id)` for `graph_instance_id`, add uniqueness for `(graph_instance_id, activity_key, attempt)` if only one assignment per attempt is allowed, and add CHECK for `lease_status`. Current FKs cover run/agent/frame only (`0001_init.sql:2262`, `0001_init.sql:2270`, `0001_init.sql:2278`).
- Priority: P1 keep but add graph-instance FK and uniqueness.

#### lifecycle_subject_associations

Definition: `CREATE TABLE public.lifecycle_subject_associations` at `0001_init.sql:410`. Domain: `LifecycleSubjectAssociation` at `lifecycle_subject_association.rs:10`.

- Field classification:
  - Business/control association fact: `id`, `anchor_run_id`, `anchor_agent_id`, `subject_kind`, `subject_id`, `role`, `metadata_json`, `created_at`.
- Recommendation: keep. This is the target owner/subject/control association layer and should not be folded into sessions. Add FK for `anchor_agent_id` to `lifecycle_agents(id)` and enforce agent belongs to run through repository/service or composite FK if schema supports it. Add indexes matching spec on anchor and subject; visible migration excerpt shows FK to run only (`0001_init.sql:2350`).
- Priority: P1 keep but constrain/index.

#### lifecycle_gates

Definition: `CREATE TABLE public.lifecycle_gates` at `0001_init.sql:373`. Domain: `LifecycleGate` at `lifecycle_gate.rs:10`.

- Field classification:
  - Runtime/control fact: `id`, `run_id`, `agent_id`, `frame_id`, `gate_kind`, `correlation_id`, `status`, `payload_json`, `resolved_by`, `created_at`, `resolved_at`.
  - Audit/control fact: `resolved_by`, `resolved_at`.
- Recommendation: keep. Add FKs for `agent_id` and `frame_id`; add unique/correlation guard for open gates if duplicate resume points are invalid; add CHECK for status. Existing migration has run FK and agent/status index (`0001_init.sql:1862`, `0001_init.sql:2342`) but no visible FK to agent/frame.
- Priority: P2 keep-but-constrain.

#### agent_lineages

Definition: `CREATE TABLE public.agent_lineages` at `0001_init.sql:88`. Domain: `AgentLineage` at `agent_lineage.rs:9`.

- Field classification:
  - Business/control tree fact: `id`, `run_id`, `parent_agent_id`, `child_agent_id`, `relation_kind`, `source_frame_id`, `metadata_json`, `created_at`.
- Recommendation: keep. It is distinct from session_lineage per spec. Add FK for `parent_agent_id` and `source_frame_id`, add unique child edge if each child has one primary parent, and CHECK for relation kind. Current visible migration shows child/run FKs only (`0001_init.sql:2302`, `0001_init.sql:2310`).
- Priority: P2 keep-but-constrain.

#### runtime_session_execution_anchors

Definition: `CREATE TABLE public.runtime_session_execution_anchors` at `0001_init.sql:724`. Domain: `RuntimeSessionExecutionAnchor` at `runtime_session_anchor.rs:13`. Repository upsert at `lifecycle_anchor_repository.rs:1157`.

- Field classification:
  - Runtime launch evidence fact: `runtime_session_id`, `run_id`, `launch_frame_id`, `agent_id`, `assignment_id`, `graph_instance_id`, `activity_key`, `attempt`, `created_by_kind`, `created_at`, `updated_at`.
  - Trace adapter: primary key by `runtime_session_id` allows RuntimeSession -> control-plane reverse lookup.
  - Historical/schema inconsistency: UUID vs text type mixing in a schema where peer tables use text IDs.
- Recommendation: keep. It replaces slower and semantically weaker JSON contains lookup from `agent_frames.runtime_session_refs_json`. Highest-priority cleanup is type normalization: make all FK-ish ID columns `text` to match existing tables, or migrate the whole schema to native UUID consistently. Given current baseline broadly stores UUIDs as text and repositories bind strings, this table should use text now. Add FKs to lifecycle_runs, agent_frames, lifecycle_agents, agent_assignments, lifecycle_workflow_instances after type normalization. Keep `runtime_session_id` as text because SessionMeta id is string in session persistence.
- Priority: P0 normalize UUID/text mismatch; P1 add FKs/indexes.

#### activity_execution_claims

Definition: `CREATE TABLE public.activity_execution_claims` at `0001_init.sql:10`. Domain: `ActivityExecutionClaim` at `entity.rs:91`. Repository SQL at `workflow_repository.rs:39`.

- Field classification:
  - Runtime durable claim fact: `claim_id`, `run_id`, `graph_instance_id`, `activity_key`, `attempt`, `executor_kind`, `status`, `idempotency_key`, `executor_run_ref`, `created_at`, `updated_at`.
  - Runtime evidence/projection: `executor_run_ref` stores started executor trace ref; okay as launch evidence.
  - Historical residue: `graph_instance_id DEFAULT '00000000-...'`.
- Recommendation: keep. Remove zero UUID default; `graph_instance_id` is mandatory target identity, not a backfill fallback. Add FK to `lifecycle_workflow_instances(id)` and CHECK for status/executor_kind. Keep unique idempotency key and partial unique active-attempt index.
- Priority: P0 remove default; P1 add graph-instance FK.

#### routines

Definition: `CREATE TABLE public.routines` at `0001_init.sql:682`. Domain: `Routine` at `routine/entity.rs:10`. Repository at `routine_repository.rs:73`.

- Field classification:
  - Business/config fact: `id`, `project_id`, `name`, `prompt_template`, `project_agent_id`, `trigger_config`, `dispatch_strategy`, `enabled`, `created_at`, `updated_at`.
  - Runtime/projection cache: `last_fired_at`.
  - Historical residue: constraint names `routines_agent_id_not_null`, `routines_session_strategy_not_null` no longer match field names.
- Recommendation: keep. Routine is a first-class project trigger config. Add FK to `projects` and `project_agents` if not present elsewhere; add JSONB for trigger/dispatch if queryability matters. Rename constraints and consider an index for scheduled trigger lookup; repository uses `trigger_config::jsonb @> ...` (`routine_repository.rs:125`), so storing as `jsonb` would be cleaner.
- Priority: P2 keep-but-normalize; JSONB is P2/P3 code-touching.

#### routine_executions

Definition: `CREATE TABLE public.routine_executions` at `0001_init.sql:660`. Domain: `RoutineExecution` at `routine/entity.rs:109`. Repository mapping at `routine_repository.rs:270`.

- Field classification:
  - Runtime/audit fact: `id`, `routine_id`, `trigger_source`, `trigger_payload`, `resolved_prompt`, `status`, `started_at`, `completed_at`, `error`, `entity_key`.
  - Runtime/control-plane anchor fact: `dispatch_run_id`, `dispatch_agent_id`, `dispatch_frame_id`, `dispatch_assignment_id`.
- Recommendation: keep dispatch refs. They are not stale duplication: RoutineExecution is the durable trigger audit row, and dispatch refs let routine history navigate to the created/reused run and allow `DispatchStrategy::Reuse` / `PerEntity` validation. API currently exposes run/agent/frame but hides assignment (`dto/routine.rs:66`), while reuse resolver validates assignment internally (`routine/reuse_resolver.rs:218`).
- Cleanup: add FKs to routines/lifecycle_runs/lifecycle_agents/agent_frames/agent_assignments after text/UUID consistency; add CHECK for status; consider making dispatch refs all-or-none through a CHECK. Keep `entity_key` for per-entity affinity.
- Priority: P1 keep but constrain; do not remove routine dispatch refs.

### Cross-cutting Recommendations

1. P0 direct baseline cleanup: remove `lifecycle_runs.record_artifacts`; remove `activity_execution_claims.graph_instance_id` zero UUID default; normalize `runtime_session_execution_anchors` UUID columns to text or all referenced tables to UUID, with current evidence favoring text.
2. P1 code-touching schema convergence: migrate `lifecycle_runs.execution_log` to append-only execution events, then project into `LifecycleRunView`; migrate/remove `lifecycle_runs.active_node_keys` as persisted state by deriving from `lifecycle_workflow_instances.activity_state_json`.
3. P1 normalize AgentFrame trace refs: keep effective capability/context/VFS/MCP/execution profile in the frame snapshot; move `runtime_session_refs_json` lookup responsibility to `runtime_session_execution_anchors`; split or explicitly mark `visible_canvas_mount_ids_json` as mutable projection outside immutable revision semantics.
4. P1 add missing relational constraints for graph_instance/agent/frame/assignment anchors after ID-type normalization. The current baseline has many text IDs with limited FKs; this is acceptable during development but weak for the “semantic baseline” goal.
5. P2 rewrite dump-style constraint names/defaults on this slice: `*_not_null` constraint names, `lifecycle_runs_lifecycle_id_not_null`, `routines_session_strategy_not_null`, and zero UUID defaults should not survive the new baseline.

### Highest-priority Suggestions

- Direct removal: `lifecycle_runs.record_artifacts` is the safest immediate deletion candidate. It is present in migration and insert SQL, but absent from domain reads, updates, DTOs and frontend contracts.
- Needs code changes before removal: `lifecycle_runs.execution_log` and `active_node_keys` are useful but misplaced/mixed. `execution_log` should become append-only audit/runtime event storage; `active_node_keys` should be a read projection from `lifecycle_workflow_instances.activity_state_json`.
- Position/type defect: `runtime_session_execution_anchors` should stay, but its UUID/text mix should be fixed before more code depends on it.
- Looks odd but should stay: routine dispatch refs on `routine_executions`; AgentFrame capability/context/VFS/MCP snapshot fields; AgentAssignment; LifecycleSubjectAssociation; AgentLineage.

## Caveats / Not Found

- I did not modify code, specs, migrations, or task report; only this research file was written.
- Some table definitions in `0001_init.sql` store JSON as `text` while repositories cast to JSONB in queries. This research flags semantic cleanup candidates, but it does not include a full mechanical text-vs-jsonb migration plan for every affected column.
- I did not run build/typecheck because this is a read-only research slice and no code changes were made.
- Active task lookup via `python ./.trellis/scripts/task.py current --source` returned no active task in this shell; the user-provided task path and explicit writable research file were used.
