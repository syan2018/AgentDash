# Research: WI-10 Lifecycle storage gates subjects current state

- Query: 从当前代码事实评估 Lifecycle control-plane 的 context、view projection、gates、subjects、agent lineage 存储形态是否过度拆表或错误组合
- Scope: internal
- Date: 2026-07-04

## Findings

### Files found

- `AGENTS.md`: 项目协作约束；本次只读取代码并只写 research 文件。
- `.trellis/workflow.md`: research 必须持久化到 task `research/`，Phase 2 research sub-agent 只产出研究文档。
- `.trellis/tasks/07-03-agentrun-lifecycle-repository-deletion-driven-convergence/work-items/WI-10-lifecycle-storage-gates-subjects.md`: WI-10 范围定义。
- `.trellis/tasks/07-03-agentrun-lifecycle-repository-deletion-driven-convergence/design.md`: D-016/D-017 分类规则和候选对象。
- `.trellis/tasks/07-03-agentrun-lifecycle-repository-deletion-driven-convergence/decisions.md`: D-002/D-014/D-016/D-017 以及 Q-001/Q-003 当前规划结论。
- `.trellis/tasks/07-03-agentrun-lifecycle-repository-deletion-driven-convergence/inventory.md`: WI-00 执行前 inventory。
- `.trellis/spec/backend/repository-pattern.md`: 当前实际 repository 规格文件。
- `.trellis/spec/backend/database-guidelines.md`: 当前实际 database 规格文件。
- `.trellis/spec/backend/workflow/architecture.md`: Lifecycle/orchestration/gate/subject 相关合同。
- `crates/agentdash-domain/src/workflow/entity.rs`: `LifecycleRun` 当前 aggregate 字段。
- `crates/agentdash-domain/src/workflow/repository.rs`: LifecycleRun/Gate/Subject/Lineage repository trait。
- `crates/agentdash-domain/src/workflow/lifecycle_gate.rs`: durable gate entity。
- `crates/agentdash-domain/src/workflow/lifecycle_subject_association.rs`: subject association entity。
- `crates/agentdash-domain/src/workflow/agent_lineage.rs`: same-run agent control tree entity。
- `crates/agentdash-domain/src/workflow/agent_run_lineage.rs`: cross-run fork lineage entity。
- `crates/agentdash-domain/src/workflow/runtime_session_anchor.rs`: runtime trace launch evidence。
- `crates/agentdash-domain/src/workflow/agent_run_delivery_binding.rs`: AgentRun current delivery binding state。
- `crates/agentdash-infrastructure/src/persistence/postgres/workflow_repository.rs`: `LifecycleRun` row mapping。
- `crates/agentdash-infrastructure/src/persistence/postgres/lifecycle_anchor_repository.rs`: subject/gate/agent lineage PostgreSQL repositories。
- `crates/agentdash-infrastructure/src/persistence/postgres/agent_run_lineage_repository.rs`: product fork lineage and fork materialization transaction。
- `crates/agentdash-infrastructure/src/persistence/postgres/agent_run_delivery_binding_repository.rs`: current delivery binding table adapter。
- `crates/agentdash-infrastructure/migrations/0001_init.sql`: baseline lifecycle tables/indexes/FKs。
- `crates/agentdash-infrastructure/migrations/0003_lifecycle_orchestration_contract.sql`: historical `context` / `view_projection` add.
- `crates/agentdash-infrastructure/migrations/0041_drop_lifecycle_run_context_view_projection.sql`: current drop migration for `context` / `view_projection`.
- `crates/agentdash-infrastructure/migrations/0044_agent_run_delivery_bindings.sql`: current delivery binding table and anchor FK hardening。
- Application consumers: `agentdash-application-lifecycle`, `agentdash-application-workflow`, `agentdash-application-agentrun`, `agentdash-application/src/task`, `agentdash-application/src/companion`, `agentdash-api/src/routes/lifecycle_agents.rs`。

### `LifecycleRun.context` and removed fields

Current code no longer has `LifecycleContext` as a domain type. `LifecycleRun` currently contains `id/project_id/created_by_user_id/topology/orchestrations/tasks/status/execution_log/timestamps` only; there is no `context`, `permission_scope`, `budget`, `main_agent_run_id`, `agent_runs`, or `frame_refs` field in the aggregate (`crates/agentdash-domain/src/workflow/entity.rs:156`, `crates/agentdash-domain/src/workflow/entity.rs:163`, `crates/agentdash-domain/src/workflow/entity.rs:169`, `crates/agentdash-domain/src/workflow/entity.rs:173`). The PostgreSQL `RUN_COLS` and insert/update bindings also omit `context` and `view_projection`, writing only `orchestrations/tasks/status/execution_log` plus identity/timestamps (`crates/agentdash-infrastructure/src/persistence/postgres/workflow_repository.rs:32`, `crates/agentdash-infrastructure/src/persistence/postgres/workflow_repository.rs:420`, `crates/agentdash-infrastructure/src/persistence/postgres/workflow_repository.rs:431`, `crates/agentdash-infrastructure/src/persistence/postgres/workflow_repository.rs:495`, `crates/agentdash-infrastructure/src/persistence/postgres/workflow_repository.rs:667`).

The only remaining `context`/`view_projection` lifecycle-run hits are migration history: `0003` added both columns (`crates/agentdash-infrastructure/migrations/0003_lifecycle_orchestration_contract.sql:1`, `crates/agentdash-infrastructure/migrations/0003_lifecycle_orchestration_contract.sql:4`) and `0041` drops both (`crates/agentdash-infrastructure/migrations/0041_drop_lifecycle_run_context_view_projection.sql:1`, `crates/agentdash-infrastructure/migrations/0041_drop_lifecycle_run_context_view_projection.sql:3`). `rg` hits for `AgentConversationLifecycleContext` are conversation snapshot DTOs, not `LifecycleRun.context` (`crates/agentdash-contracts/src/runtime/workflow.rs:1211`, `crates/agentdash-api/src/routes/lifecycle_agents.rs:1904`).

The deleted context subfields are rebuildable from real sources:

- `agent_runs` / `main_agent_run_id`: agents come from `LifecycleAgentRepository::list_by_run` (`crates/agentdash-domain/src/workflow/repository.rs:76`, `crates/agentdash-infrastructure/src/persistence/postgres/lifecycle_anchor_repository.rs:115`) and API root selection uses `AgentLineage` forest, not embedded context (`crates/agentdash-api/src/routes/lifecycle_agents.rs:250`, `crates/agentdash-api/src/routes/lifecycle_agents.rs:262`).
- `frame_refs`: current frame is resolved from `AgentRunDeliveryBinding` or `AgentFrameRepository::get_current`, not lifecycle context (`crates/agentdash-application-agentrun/src/agent_run/workspace/query.rs:350`, `crates/agentdash-application-agentrun/src/agent_run/workspace/query.rs:407`, `crates/agentdash-application-agentrun/src/agent_run/workspace/query.rs:415`). AgentFrame is a revision child row (`crates/agentdash-domain/src/workflow/agent_frame.rs:6`, `crates/agentdash-infrastructure/src/persistence/postgres/lifecycle_anchor_repository.rs:235`).
- `permission_scope` / `budget`: no current production field or consumer under `LifecycleRun`; remaining `budget` hits are workflow script limits/session compaction, not lifecycle context (`crates/agentdash-domain/src/workflow/value_objects/orchestration.rs:295`, `crates/agentdash-application-workflow/src/orchestration/script_compiler.rs:1074`).

Conclusion: current code has already executed the correct storage decision for `LifecycleRun.context`: delete. If permission/budget later becomes business state, it should be reintroduced as explicit control-plane state with a named writer/consumer, not as generic context.

### `LifecycleRun.view_projection`

Current code no longer has `LifecycleRun.view_projection`. `RUN_COLS`, `LifecycleRunRow`, and row mapping do not include it (`crates/agentdash-infrastructure/src/persistence/postgres/workflow_repository.rs:32`, `crates/agentdash-infrastructure/src/persistence/postgres/workflow_repository.rs:635`, `crates/agentdash-infrastructure/src/persistence/postgres/workflow_repository.rs:650`). The drop migration is present (`crates/agentdash-infrastructure/migrations/0041_drop_lifecycle_run_context_view_projection.sql:2`, `crates/agentdash-infrastructure/migrations/0041_drop_lifecycle_run_context_view_projection.sql:3`).

The current read model is rebuilt from facts:

- `build_lifecycle_run_view_with_preloaded` loads agents, subject associations, runtime trace refs, and delivery bindings, then assembles the view (`crates/agentdash-application-lifecycle/src/lifecycle/run_view_builder.rs:75`, `crates/agentdash-application-lifecycle/src/lifecycle/run_view_builder.rs:88`, `crates/agentdash-application-lifecycle/src/lifecycle/run_view_builder.rs:90`, `crates/agentdash-application-lifecycle/src/lifecycle/run_view_builder.rs:96`).
- Runtime trace refs are collected from execution anchors (`crates/agentdash-application-lifecycle/src/lifecycle/run_view_builder.rs:200`, `crates/agentdash-application-lifecycle/src/lifecycle/run_view_builder.rs:205`).
- Subject execution views start from subject associations, not `view_projection` (`crates/agentdash-application-lifecycle/src/lifecycle/run_view_builder.rs:109`, `crates/agentdash-application-lifecycle/src/lifecycle/run_view_builder.rs:114`).

Conclusion: `view_projection` is not current state. If a cache is needed later, it should be an explicitly rebuildable projection outside the aggregate write model.

### Gates

`LifecycleGate` is a durable wait/review/resume point with run/agent/frame coordinates, `gate_kind`, `correlation_id`, `status`, payload, and resolution metadata (`crates/agentdash-domain/src/workflow/lifecycle_gate.rs:5`, `crates/agentdash-domain/src/workflow/lifecycle_gate.rs:10`, `crates/agentdash-domain/src/workflow/lifecycle_gate.rs:18`, `crates/agentdash-domain/src/workflow/lifecycle_gate.rs:53`). Its repository exposes `create/get/list_open_for_agent/update` (`crates/agentdash-domain/src/workflow/repository.rs:119`, `crates/agentdash-domain/src/workflow/repository.rs:122`). PostgreSQL stores it in `lifecycle_gates` with open-by-agent, correlation, and run indexes (`crates/agentdash-infrastructure/migrations/0001_init.sql:268`, `crates/agentdash-infrastructure/migrations/0001_init.sql:1080`, `crates/agentdash-infrastructure/migrations/0001_init.sql:1084`) and repository code performs single-row create/get/update plus `WHERE agent_id=$1 AND status='open'` query (`crates/agentdash-infrastructure/src/persistence/postgres/lifecycle_anchor_repository.rs:523`, `crates/agentdash-infrastructure/src/persistence/postgres/lifecycle_anchor_repository.rs:557`, `crates/agentdash-infrastructure/src/persistence/postgres/lifecycle_anchor_repository.rs:579`).

Production consumers prove this is not just parent JSON:

- Workflow companion and HumanGate open gates and persist them (`crates/agentdash-application-workflow/src/gate/resolver.rs:68`, `crates/agentdash-application-workflow/src/gate/resolver.rs:78`, `crates/agentdash-application-workflow/src/gate/resolver.rs:96`, `crates/agentdash-application-workflow/src/gate/resolver.rs:121`).
- Gate resolve checks the current row is still open before update (`crates/agentdash-application-workflow/src/gate/resolver.rs:131`, `crates/agentdash-application-workflow/src/gate/resolver.rs:142`, `crates/agentdash-application-workflow/src/gate/resolver.rs:442`, `crates/agentdash-application-workflow/src/gate/resolver.rs:450`).
- HumanGate node start stores a `NodeStarted` executor ref using gate id (`crates/agentdash-application-workflow/src/orchestration/human_gate_launcher.rs:61`, `crates/agentdash-application-workflow/src/orchestration/human_gate_launcher.rs:83`).
- AgentRun workspace and wait activity list open gates by agent (`crates/agentdash-application-agentrun/src/agent_run/workspace/query.rs:188`, `crates/agentdash-application/src/wait_activity/service.rs:257`).
- Companion response paths list open gates and correlate by request id (`crates/agentdash-application/src/companion/gate_control.rs:554`, `crates/agentdash-application/src/companion/gate_control.rs:559`; `crates/agentdash-application/src/companion/tools.rs:1811`, `crates/agentdash-application/src/companion/tools.rs:1817`).

Conclusion: gates should remain a Lifecycle-owned child table. They should not be JSONB on `LifecycleRun` because they need open-by-agent lookup and independent row status updates. They also should not be treated as independent aggregate roots; the parent owner remains Lifecycle control-plane.

### Subject association

`LifecycleSubjectAssociation` links a `SubjectRef` to whole-run or agent-scoped anchors (`crates/agentdash-domain/src/workflow/lifecycle_subject_association.rs:5`, `crates/agentdash-domain/src/workflow/lifecycle_subject_association.rs:10`, `crates/agentdash-domain/src/workflow/lifecycle_subject_association.rs:39`, `crates/agentdash-domain/src/workflow/lifecycle_subject_association.rs:60`). The repository has both subject -> anchor and anchor -> subject query directions (`crates/agentdash-domain/src/workflow/repository.rs:104`, `crates/agentdash-domain/src/workflow/repository.rs:106`, `crates/agentdash-domain/src/workflow/repository.rs:110`). PostgreSQL has a dedicated table plus subject and anchor indexes (`crates/agentdash-infrastructure/migrations/0001_init.sql:294`, `crates/agentdash-infrastructure/migrations/0001_init.sql:1090`, `crates/agentdash-infrastructure/migrations/0001_init.sql:1094`), and adapter queries directly by `(subject_kind, subject_id)` or `(anchor_run_id, anchor_agent_id)` (`crates/agentdash-infrastructure/src/persistence/postgres/lifecycle_anchor_repository.rs:402`, `crates/agentdash-infrastructure/src/persistence/postgres/lifecycle_anchor_repository.rs:420`).

Production consumer evidence for high-value reverse lookup:

- Lifecycle run view lists run/agent associations for projection (`crates/agentdash-application-lifecycle/src/lifecycle/run_view_builder.rs:75`, `crates/agentdash-application-lifecycle/src/lifecycle/run_view_builder.rs:83`).
- Subject execution view starts from `list_by_subject`, then loads associated runs (`crates/agentdash-application-lifecycle/src/lifecycle/run_view_builder.rs:109`, `crates/agentdash-application-lifecycle/src/lifecycle/run_view_builder.rs:114`, `crates/agentdash-application-lifecycle/src/lifecycle/run_view_builder.rs:118`).
- Runtime trace context resolves anchor -> associations, falling back from agent to run scope (`crates/agentdash-application-lifecycle/src/lifecycle/session_run_context_resolver.rs:37`, `crates/agentdash-application-lifecycle/src/lifecycle/session_run_context_resolver.rs:69`, `crates/agentdash-application-lifecycle/src/lifecycle/session_run_context_resolver.rs:75`).
- Subject cancel/control uses `list_by_subject` to find current delivery (`crates/agentdash-application-lifecycle/src/lifecycle/subject_execution_control.rs:148`, `crates/agentdash-application-lifecycle/src/lifecycle/subject_execution_control.rs:152`, `crates/agentdash-application-lifecycle/src/lifecycle/subject_execution_control.rs:164`).
- Task lookup and story task projection use `list_by_subject` to avoid scanning all runs first (`crates/agentdash-application/src/task/plan.rs:264`, `crates/agentdash-application/src/task/plan.rs:271`, `crates/agentdash-application/src/task/plan.rs:316`, `crates/agentdash-application/src/task/plan.rs:323`).
- Task context builder follows task association to active workflow projection (`crates/agentdash-application/src/task/context_builder.rs:229`, `crates/agentdash-application/src/task/context_builder.rs:239`, `crates/agentdash-application/src/task/context_builder.rs:241`).
- Permission escalation creates run-scoped control association when a grant is used (`crates/agentdash-application/src/permission/escalation.rs:50`, `crates/agentdash-application/src/permission/escalation.rs:76`, `crates/agentdash-application/src/permission/escalation.rs:84`).

Conclusion: subject association needs a physical indexed relationship table. It is not merely `LifecycleRun` internal JSON because the subject -> run/agent direction is a first-class query path. Minimal shape is one row per association with `anchor_run_id`, optional `anchor_agent_id`, `subject_kind`, `subject_id`, `role`, metadata, plus indexes for `(subject_kind, subject_id, created_at desc)` and `(anchor_run_id, anchor_agent_id, created_at)`.

### Same-run agent lineage

`AgentLineage` is explicitly the same-run control tree; comments distinguish it from RuntimeSession trace lineage (`crates/agentdash-domain/src/workflow/agent_lineage.rs:5`, `crates/agentdash-domain/src/workflow/agent_lineage.rs:7`). The repository supports child, parent, and full-run tree reads (`crates/agentdash-domain/src/workflow/repository.rs:127`, `crates/agentdash-domain/src/workflow/repository.rs:129`, `crates/agentdash-domain/src/workflow/repository.rs:133`). PostgreSQL stores rows in `agent_lineages` and indexes child, parent, and run (`crates/agentdash-infrastructure/migrations/0001_init.sql:58`, `crates/agentdash-infrastructure/migrations/0001_init.sql:1038`, `crates/agentdash-infrastructure/migrations/0001_init.sql:1042`). Adapter queries match those access patterns (`crates/agentdash-infrastructure/src/persistence/postgres/lifecycle_anchor_repository.rs:673`, `crates/agentdash-infrastructure/src/persistence/postgres/lifecycle_anchor_repository.rs:687`, `crates/agentdash-infrastructure/src/persistence/postgres/lifecycle_anchor_repository.rs:700`).

Production consumers:

- Dispatch relation writer creates a lineage row when a plan has `parent_agent_id` (`crates/agentdash-application-lifecycle/src/lifecycle/dispatch/lifecycle_relation_writer.rs:33`, `crates/agentdash-application-lifecycle/src/lifecycle/dispatch/lifecycle_relation_writer.rs:40`, `crates/agentdash-application-lifecycle/src/lifecycle/dispatch/lifecycle_relation_writer.rs:49`).
- AgentRun list loads all run lineages and builds a forest; roots are agents not appearing as children (`crates/agentdash-api/src/routes/lifecycle_agents.rs:250`, `crates/agentdash-api/src/routes/lifecycle_agents.rs:258`, `crates/agentdash-api/src/routes/lifecycle_agents.rs:1676`, `crates/agentdash-api/src/routes/lifecycle_agents.rs:1688`).
- AgentRun detail resolves one-hop parent/children from same-run lineage (`crates/agentdash-api/src/routes/lifecycle_agents.rs:562`, `crates/agentdash-api/src/routes/lifecycle_agents.rs:572`, `crates/agentdash-api/src/routes/lifecycle_agents.rs:580`, `crates/agentdash-api/src/routes/lifecycle_agents.rs:617`).
- Run view uses same-run lineage to filter whole-run history agents (`crates/agentdash-application-lifecycle/src/lifecycle/run_view_builder.rs:255`, `crates/agentdash-application-lifecycle/src/lifecycle/run_view_builder.rs:274`, `crates/agentdash-application-lifecycle/src/lifecycle/run_view_builder.rs:285`).
- Companion parent lookup depends on `find_parent` (`crates/agentdash-application/src/companion/gate_control.rs:545`, `crates/agentdash-application/src/companion/gate_control.rs:699`).

Conclusion: same-run `AgentLineage` is a Lifecycle child fact/table, not a projection. It should not be merged into `AgentRunLineage` product fork provenance. The API DTO name is still confusing: `AgentRunLineageRef` is documented as a control-tree reference whose `relation_kind` comes from `AgentLineage` (`crates/agentdash-contracts/src/runtime/workflow.rs:1653`, `crates/agentdash-contracts/src/runtime/workflow.rs:1655`).

### Cross-run product fork lineage

`AgentRunLineage` is a separate cross-run fork provenance model (`crates/agentdash-domain/src/workflow/agent_run_lineage.rs:6`, `crates/agentdash-domain/src/workflow/agent_run_lineage.rs:12`). Its repository queries by child, parent, or either run id (`crates/agentdash-domain/src/workflow/repository.rs:137`, `crates/agentdash-infrastructure/src/persistence/postgres/agent_run_lineage_repository.rs:44`, `crates/agentdash-infrastructure/src/persistence/postgres/agent_run_lineage_repository.rs:62`, `crates/agentdash-infrastructure/src/persistence/postgres/agent_run_lineage_repository.rs:81`). Fork command replay expects canonical lineage to exist for a child AgentRun (`crates/agentdash-application-agentrun/src/agent_run/fork.rs:492`, `crates/agentdash-application-agentrun/src/agent_run/fork.rs:495`).

Current schema still requires parent/child runtime session ids and indexes those runtime ids (`crates/agentdash-infrastructure/migrations/0038_agent_run_lineages.sql:10`, `crates/agentdash-infrastructure/migrations/0038_agent_run_lineages.sql:26`, `crates/agentdash-infrastructure/migrations/0038_agent_run_lineages.sql:30`). Current repository API has no query by runtime session id; runtime id hits are field storage, inserts, logging, tests, and fork/companion command payloads rather than index-backed repository entrypoints.

Conclusion: product fork lineage is not the same as same-run agent lineage. It is an AgentRun fork record / cross-run relationship table. Runtime-session-id columns and runtime indexes are overdesign candidates for WI-08/WI-12, but WI-10 should not collapse this table into Lifecycle same-run lineage.

### Orchestrations, tasks, execution log

Current `LifecycleRun` still owns `orchestrations`, `tasks`, and `execution_log` as embedded aggregate fields (`crates/agentdash-domain/src/workflow/entity.rs:163`, `crates/agentdash-domain/src/workflow/entity.rs:165`, `crates/agentdash-domain/src/workflow/entity.rs:168`). Repository create/update roundtrips these columns as JSON text (`crates/agentdash-infrastructure/src/persistence/postgres/workflow_repository.rs:428`, `crates/agentdash-infrastructure/src/persistence/postgres/workflow_repository.rs:431`, `crates/agentdash-infrastructure/src/persistence/postgres/workflow_repository.rs:500`, `crates/agentdash-infrastructure/src/persistence/postgres/workflow_repository.rs:503`).

Observed access pattern is parent-loaded mutation:

- Orchestration runtime applies events by locating an `orchestration_id` inside loaded `run.orchestrations` and returning an updated run (`crates/agentdash-application-workflow/src/orchestration/runtime.rs:266`, `crates/agentdash-application-workflow/src/orchestration/runtime.rs:272`, `crates/agentdash-application-workflow/src/orchestration/runtime.rs:279`).
- Ready-node lookup scans loaded run orchestrations and `dispatch.ready_node_ids` (`crates/agentdash-application-workflow/src/orchestration/ready_node.rs:67`, `crates/agentdash-application-workflow/src/orchestration/ready_node.rs:77`).
- Task commands load run, mutate `run.tasks`, and update run (`crates/agentdash-application/src/task/plan.rs:90`, `crates/agentdash-application/src/task/plan.rs:95`, `crates/agentdash-domain/src/workflow/entity.rs:256`, `crates/agentdash-domain/src/workflow/entity.rs:371`).
- Execution log flush groups by run, loads run, appends entries, and updates run (`crates/agentdash-application-lifecycle/src/lifecycle/execution_log.rs:41`, `crates/agentdash-application-lifecycle/src/lifecycle/execution_log.rs:61`, `crates/agentdash-application-lifecycle/src/lifecycle/execution_log.rs:65`).

Conclusion: no current evidence justifies splitting orchestrations/tasks/execution_log as part of WI-10. They are parent-owned JSON/text state today. Execution log has future append-concurrency/pagination risk, but not enough current query value for an independent table in this slice.

### Overdesign candidates

- Already resolved: `LifecycleRun.context` and `LifecycleRun.view_projection` were independent aggregate fields with no current production decision consumers; current code removed them.
- Repository exposure remains over-broad: `RepositorySet`, `AgentRunRepositorySet`, `application-lifecycle::RepositorySet`, and `WorkflowRepositorySet` still expose `LifecycleSubjectAssociationRepository`, `LifecycleGateRepository`, and/or `AgentLineageRepository` as flat dependencies (`crates/agentdash-application/src/repository_set.rs:84`, `crates/agentdash-application/src/repository_set.rs:87`, `crates/agentdash-application-agentrun/src/agent_run_repository_set.rs:68`, `crates/agentdash-application-agentrun/src/agent_run_repository_set.rs:73`, `crates/agentdash-application-lifecycle/src/repository_set.rs:72`, `crates/agentdash-application-lifecycle/src/repository_set.rs:77`, `crates/agentdash-application-workflow/src/repository_set.rs:9`, `crates/agentdash-application-workflow/src/repository_set.rs:12`). The physical tables are justified; the overdesign is top-level repository semantics leaking into broad service containers.
- `agent_run_lineages` runtime-session indexes are overdesign candidates because no current repository method queries by runtime session id (`crates/agentdash-infrastructure/migrations/0038_agent_run_lineages.sql:26`, `crates/agentdash-infrastructure/src/persistence/postgres/agent_run_lineage_repository.rs:44`, `crates/agentdash-infrastructure/src/persistence/postgres/agent_run_lineage_repository.rs:81`).

## Storage classification

| Item | Classification | Recommendation and reason |
| --- | --- | --- |
| `LifecycleRun.context.main_agent_run_id` | delete | Already absent from domain/repository; main/root AgentRun is derived from `LifecycleAgent.list_by_run` plus `AgentLineage` root forest. |
| `LifecycleRun.context.agent_runs` | delete | Already absent; duplicates `lifecycle_agents` and run-scoped lineage facts. |
| `LifecycleRun.context.frame_refs` | delete | Already absent; frame/current resolution comes from `AgentRunDeliveryBinding`, `RuntimeSessionExecutionAnchor`, and `AgentFrameRepository`. |
| `LifecycleRun.context.permission_scope` | delete | No current production consumer. Reintroduce only as explicit control-plane state if it participates in permission decisions. |
| `LifecycleRun.context.budget` | delete | No current LifecycleRun governance consumer. Current budget hits are workflow-script/session concepts, not LifecycleRun context. |
| `LifecycleRun.view_projection` | projection / delete | Already deleted from aggregate. Read model is rebuildable from run, agents, subject associations, anchors, delivery bindings, and orchestrations. |
| `LifecycleRun.orchestrations` | JSONB on parent | Current storage is JSON text on `lifecycle_runs`. Keep parent-owned embedded state until there is node-level DB claim/scan/lease evidence. |
| `LifecycleRun.tasks` | JSONB on parent | Current task commands mutate loaded run; subject association supplies reverse lookup. No task table needed now. |
| `LifecycleRun.execution_log` | JSONB on parent | Keep embedded for current usage. Split later only for append conflict safety, pagination, or immutable audit identity. |
| `LifecycleGate` / `lifecycle_gates` | child table | Keep Lifecycle-owned child table. Needs durable open rows, status updates, open-by-agent lookup, and correlation matching. Add a narrower agent+correlation open query/index if scaling. |
| `LifecycleSubjectAssociation` | independent table | Keep indexed relationship table because subject -> run/agent is a first-class query direction. It is not an independent aggregate, but it is independent physical storage. |
| Same-run `AgentLineage` / `agent_lineages` | child table | Keep Lifecycle child table for same-run control tree. It is a durable child fact, not a projection and not product fork lineage. |
| Cross-run `AgentRunLineage` / `agent_run_lineages` | independent table | Keep as product fork record / cross-run relationship. Runtime session id fields/indexes remain WI-08/WI-12 cleanup candidates. |
| Flat RepositorySet exposure for gate/subject/lineage | delete | Delete from broad service dependencies over time; replace with narrow lifecycle/gate/subject/control-tree ports. Physical tables remain. |

## Executable slices

### Slice 1: LifecycleRun deleted fields stabilization

- Allowed write scope: `crates/agentdash-domain/src/workflow/entity.rs`, `crates/agentdash-infrastructure/src/persistence/postgres/workflow_repository.rs`, `crates/agentdash-infrastructure/src/persistence/postgres/agent_run_lineage_repository.rs`, lifecycle repository tests, generated compile fixes only.
- Migration need: none if `0041_drop_lifecycle_run_context_view_projection.sql` remains in chain; otherwise add a forward migration that drops `context` and `view_projection`.
- Validation commands: `rg "LifecycleContext|permission_scope|main_agent_run_id|frame_refs|view_projection" crates/agentdash-domain crates/agentdash-infrastructure crates/agentdash-application* crates/agentdash-api`; `cargo check -p agentdash-domain -p agentdash-infrastructure -p agentdash-application-agentrun`; `pnpm run migration:guard` if migration files change.
- Parallel safety: safe with gate/subject/lineage hardening; conflicts with WI-08 only if both edit fork materialization or `agent_run_lineage_repository.rs`.

### Slice 2: LifecycleGate child table hardening

- Allowed write scope: `crates/agentdash-domain/src/workflow/repository.rs`, `crates/agentdash-infrastructure/src/persistence/postgres/lifecycle_anchor_repository.rs`, gate consumers under `agentdash-application-workflow/src/gate`, companion gate lookup call sites, new migration under `crates/agentdash-infrastructure/migrations/`.
- Migration need: likely yes. Add agent/frame FKs if still missing, plus a partial/composite index for open correlation lookup such as `(agent_id, correlation_id) WHERE status='open'` or `(run_id, agent_id, correlation_id) WHERE status='open'`.
- Validation commands: `cargo test -p agentdash-application-workflow gate`; `cargo test -p agentdash-application companion::gate_control`; `cargo check -p agentdash-infrastructure -p agentdash-application-workflow -p agentdash-application`; `pnpm run migration:guard`.
- Parallel safety: not safe in parallel with subject/lineage DB hardening if both edit `lifecycle_anchor_repository.rs` or the same migration batch; otherwise independent from LifecycleRun deleted fields.

### Slice 3: Subject association index/query consolidation

- Allowed write scope: `crates/agentdash-domain/src/workflow/repository.rs`, `crates/agentdash-infrastructure/src/persistence/postgres/lifecycle_anchor_repository.rs`, `agentdash-application-lifecycle/src/lifecycle/run_view_builder.rs`, task/routine callers only if adding batch APIs, new migration.
- Migration need: yes if adding composite `(anchor_run_id, anchor_agent_id, created_at)` and `(subject_kind, subject_id, created_at DESC)` indexes. No table deletion.
- Validation commands: `cargo test -p agentdash-application-lifecycle run_view_builder subject_execution_control`; `cargo test -p agentdash-application task`; `cargo check -p agentdash-infrastructure -p agentdash-application-lifecycle -p agentdash-application`; `pnpm run migration:guard`.
- Parallel safety: conflicts with gate/lineage hardening in `lifecycle_anchor_repository.rs`; safe with WI-08 fork lineage work.

### Slice 4: Same-run lineage constraints and naming isolation

- Allowed write scope: `crates/agentdash-domain/src/workflow/repository.rs`, `crates/agentdash-infrastructure/src/persistence/postgres/lifecycle_anchor_repository.rs`, `crates/agentdash-api/src/routes/lifecycle_agents.rs`, contract comments or DTO rename only if coordinated, new migration.
- Migration need: likely yes. Add `parent_agent_id` FK, optional `source_frame_id` FK, and `UNIQUE (run_id, child_agent_id)` after duplicate audit. Do not merge with `agent_run_lineages`.
- Validation commands: `cargo test -p agentdash-api lifecycle_agents`; `cargo test -p agentdash-application-lifecycle run_view_builder`; `cargo test -p agentdash-application companion::gate_control`; `pnpm run contracts:check` if DTO naming changes; `pnpm run migration:guard` if migration files change.
- Parallel safety: coordinate with WI-08 if renaming DTOs or product fork lineage terms; schema constraints can run separately from LifecycleRun deleted fields.

### Slice 5: RepositorySet exposure cleanup for lifecycle child facts

- Allowed write scope: application composition/deps files under `crates/agentdash-application*/src/*repository_set.rs`, lifecycle dispatch facade/service deps, workflow gate/subject/control-tree adapter constructors, API bootstrap wiring.
- Migration need: none.
- Validation commands: `cargo check -p agentdash-application -p agentdash-application-lifecycle -p agentdash-application-agentrun -p agentdash-application-workflow -p agentdash-api`.
- Parallel safety: broad compile-surface change; avoid parallel with slices 2-4 because they touch the same repository traits and constructors.

## Risks / open questions

- `task.py current --source` returned `Current task: (none)`; this research used the user-provided task path and output path explicitly.
- User-requested spec paths `.trellis/spec/backend/repository/architecture.md` and `.trellis/spec/backend/database/architecture.md` do not exist. Current matching specs are `.trellis/spec/backend/repository-pattern.md` and `.trellis/spec/backend/database-guidelines.md`.
- Gate table has valid child-table usage, but correlation lookups currently list all open gates for an agent and filter in memory in companion paths. If open-gate count grows, add a repository method/index instead of moving gate into JSON.
- Subject association table is justified by reverse lookup, but current run-view building can call `list_by_anchor` once per agent. A batch anchor query may be a performance slice, not a storage-shape reason to delete the table.
- `agent_lineages` lacks visible unique-parent and parent-agent FK constraints in baseline migration. Adding constraints requires duplicate audit and may reveal existing inconsistent rows.
- `agent_run_lineages` runtime session id fields/indexes are still product fork cleanup risk. WI-10 should only avoid conflating them with same-run `AgentLineage`; WI-08 should decide canonical fork record shape.

## Related specs

- `.trellis/spec/backend/repository-pattern.md`: repository port should reflect aggregate boundary; cross-aggregate consistency uses command ports.
- `.trellis/spec/backend/database-guidelines.md`: migration chain is schema source; retired columns are dropped by forward migration.
- `.trellis/spec/backend/workflow/architecture.md`: current workflow control-plane vocabulary for LifecycleRun, RuntimeSessionExecutionAnchor, LifecycleSubjectAssociation, AgentLineage, and gates.
- `.trellis/spec/backend/session/agentrun-mailbox.md`: waiting projection reads open LifecycleGate rows while mailbox carries wake/result envelopes.

## External references

- None. This research used current repository files only.

## Caveats / Not Found

- No production `LifecycleRun.context` writer/consumer exists in current code; only historical migration add/drop and unrelated conversation DTOs remain.
- No production `LifecycleRun.view_projection` writer/consumer exists in current code; current views are rebuilt.
- No current repository query by `agent_run_lineages.parent_runtime_session_id` or `child_runtime_session_id` was found; only field storage/insertion/logging and command payload usage were found.
- No evidence was found that `orchestrations`, `tasks`, or `execution_log` require independent DB scan/claim/pagination in the current implementation.
