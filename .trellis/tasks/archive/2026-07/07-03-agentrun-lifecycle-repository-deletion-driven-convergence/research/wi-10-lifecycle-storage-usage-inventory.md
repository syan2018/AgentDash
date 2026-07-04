# Research: WI-10 Lifecycle Storage Usage Inventory

- Query: WI-10 Lifecycle storage 中 context/view_projection/gates/subjects/agent_lineage/orchestration/tasks/execution_log 的当前使用点、事实归属和可删除/保留风险
- Scope: internal
- Date: 2026-07-04

## Findings

### Files Found

- `.trellis/tasks/07-03-agentrun-lifecycle-repository-deletion-driven-convergence/prd.md`: 本轮删除驱动目标、RuntimeSession 内部化、Lifecycle/AgentRun/AgentFrame/Fork 总归属。
- `.trellis/tasks/07-03-agentrun-lifecycle-repository-deletion-driven-convergence/design.md`: D-016/D-017 分类规则、candidate evaluation、projection/read model 策略。
- `.trellis/tasks/07-03-agentrun-lifecycle-repository-deletion-driven-convergence/decisions.md`: D-002/D-014/D-016/D-017 以及 Q-001/Q-002/Q-003 等 Accepted 决策。
- `.trellis/tasks/07-03-agentrun-lifecycle-repository-deletion-driven-convergence/inventory.md`: WI-00 对 context、gates、subjects、lineage、orchestrations/tasks/execution_log 的执行前结论。
- `.trellis/tasks/07-03-agentrun-lifecycle-repository-deletion-driven-convergence/work-items/WI-10-lifecycle-storage-gates-subjects.md`: WI-10 范围、验收和验证关键词。
- `.trellis/spec/backend/architecture.md`: 后端整洁架构、跨聚合一致性和 repository 边界规范。
- `.trellis/spec/backend/repository-pattern.md`: repository trait 按 aggregate 边界、单聚合整体持久化、跨聚合 command port 规范。
- `.trellis/spec/backend/database-guidelines.md`: migration、JSON text column、删除退役列和 migration guard 规范。
- `.trellis/spec/backend/workflow/architecture.md`: 现行 workflow contract 仍描述 `LifecycleRun.context/orchestrations/view_projection` 为 owning aggregate 字段。
- `.trellis/spec/backend/workflow/lifecycle-run-link.md`: `LifecycleSubjectAssociation`、`RuntimeSessionExecutionAnchor` 和 `AgentLineage` 控制树规范。
- `crates/agentdash-domain/src/workflow/value_objects/orchestration.rs`: `LifecycleContext` 与 `OrchestrationInstance` value types。
- `crates/agentdash-domain/src/workflow/entity.rs`: `LifecycleRun` aggregate 字段、orchestration/task/execution_log aggregate 方法。
- `crates/agentdash-domain/src/workflow/repository.rs`: LifecycleRun/Gate/SubjectAssociation/AgentLineage/AgentRunLineage repository traits。
- `crates/agentdash-domain/src/workflow/lifecycle_gate.rs`: `LifecycleGate` durable wait/review/resume entity。
- `crates/agentdash-domain/src/workflow/lifecycle_subject_association.rs`: `LifecycleSubjectAssociation` domain entity。
- `crates/agentdash-domain/src/workflow/agent_lineage.rs`: same-run agent control tree entity。
- `crates/agentdash-domain/src/workflow/agent_run_lineage.rs`: cross-run product fork lineage entity。
- `crates/agentdash-infrastructure/src/persistence/postgres/workflow_repository.rs`: `LifecycleRunRepository` PostgreSQL create/update/select/row mapping。
- `crates/agentdash-infrastructure/src/persistence/postgres/lifecycle_anchor_repository.rs`: LifecycleAgent/Frame/Gate/SubjectAssociation/AgentLineage/ExecutionAnchor PostgreSQL repository implementations。
- `crates/agentdash-infrastructure/src/persistence/postgres/agent_run_lineage_repository.rs`: `agent_run_lineages` repository and fork materialization transaction。
- `crates/agentdash-infrastructure/migrations/0001_init.sql`: baseline tables and indexes for gates, subjects, agent_lineages, lifecycle_runs。
- `crates/agentdash-infrastructure/migrations/0003_lifecycle_orchestration_contract.sql`: adds lifecycle context/orchestrations/view_projection。
- `crates/agentdash-infrastructure/migrations/0015_lifecycle_run_tasks_story_task_cleanup.sql`: moves durable task facts onto `lifecycle_runs.tasks`。
- `crates/agentdash-infrastructure/migrations/0038_agent_run_lineages.sql`: product fork lineage table, still requiring runtime session ids.
- `crates/agentdash-application-workflow/src/gate/resolver.rs`: gate open/resolve state transitions.
- `crates/agentdash-application-workflow/src/orchestration/human_gate_launcher.rs`: workflow HumanGate opens gate and emits `NodeStarted`.
- `crates/agentdash-application-workflow/src/orchestration/runtime.rs`: reducer updates embedded orchestration state inside a `LifecycleRun`.
- `crates/agentdash-application-workflow/src/orchestration/ready_node.rs`: ready node lookup scans loaded `LifecycleRun.orchestrations`.
- `crates/agentdash-application-lifecycle/src/lifecycle/run_view_builder.rs`: lifecycle/subject read model derives views from run, agents, subjects, lineage, runtime anchors.
- `crates/agentdash-application-lifecycle/src/lifecycle/session_run_context_resolver.rs`: runtime trace to subject context uses anchor then subject associations.
- `crates/agentdash-application-lifecycle/src/lifecycle/subject_execution_control.rs`: subject cancel/control uses subject reverse lookup.
- `crates/agentdash-application-lifecycle/src/lifecycle/dispatch/lifecycle_relation_writer.rs`: dispatch writes same-run `AgentLineage` and gate facts.
- `crates/agentdash-application-lifecycle/src/lifecycle/execution_log.rs`: flushes pending hook execution log entries into `LifecycleRun.execution_log`.
- `crates/agentdash-application-agentrun/src/agent_run/workspace/query.rs`: AgentRun workspace reads open gates and subject associations.
- `crates/agentdash-application/src/task/plan.rs`: task facts are read/written through loaded `LifecycleRun.tasks`.
- `crates/agentdash-application/src/task/context_builder.rs`: task context and active workflow projection use subject association reverse lookup.
- `crates/agentdash-application/src/wait_activity/service.rs`: wait activity reads open lifecycle gates per agent.
- `crates/agentdash-application/src/companion/gate_control.rs`: companion result/parent request uses `AgentLineage.find_parent` and open gate lookup.
- `crates/agentdash-api/src/routes/lifecycle_agents.rs`: API list/detail builds AgentRun control tree from same-run `AgentLineage`.
- `crates/agentdash-contracts/src/runtime/workflow.rs`: DTO exposes subject associations and same-run lineage parent/children refs.

### LifecycleContext / `lifecycle_runs.context`

当前用途：

- `LifecycleContext` 当前包含 `main_agent_run_id`、`agent_runs`、`frame_refs`、`permission_scope`、`budget`，但代码使用点主要是默认值、repository roundtrip、测试构造和 fork materialization 复制。
- 当前生产路径未发现 `permission_scope` / `budget` / `frame_refs` / `main_agent_run_id` 的业务 consumer。`rg` 在目标 crate 中只命中类型定义、roundtrip 测试、fork clone 和非 LifecycleContext 的 session compaction budget 字段。
- AgentRun/frame/subject 查询事实已经通过 `lifecycle_agents`、`agent_frames`、`lifecycle_subject_associations`、read model builder 和 runtime anchor 进入，不需要从 context 反查。

file:line 证据：

- `crates/agentdash-domain/src/workflow/value_objects/orchestration.rs:15` 定义 `LifecycleContext`，字段位于 `main_agent_run_id` 到 `budget`。
- `crates/agentdash-domain/src/workflow/entity.rs:165` 将 `context` 放在 `LifecycleRun` aggregate 上，构造函数默认空 context 见 `crates/agentdash-domain/src/workflow/entity.rs:195` 和 `crates/agentdash-domain/src/workflow/entity.rs:218`。
- `crates/agentdash-domain/src/workflow/entity.rs:230` 只有 `set_lifecycle_context` 直接设置 context 并 touch activity。
- `crates/agentdash-infrastructure/src/persistence/postgres/workflow_repository.rs:32` 的 `RUN_COLS` 包含 `context`；create/update 只是 serde roundtrip，见 `crates/agentdash-infrastructure/src/persistence/postgres/workflow_repository.rs:428` 和 `crates/agentdash-infrastructure/src/persistence/postgres/workflow_repository.rs:502`。
- `crates/agentdash-infrastructure/src/persistence/postgres/workflow_repository.rs:664` 从 `lifecycle_runs.context` parse JSON。
- `crates/agentdash-infrastructure/src/persistence/postgres/workflow_repository.rs:1063` 到 `crates/agentdash-infrastructure/src/persistence/postgres/workflow_repository.rs:1088` 是 context roundtrip 测试构造和断言。
- `crates/agentdash-infrastructure/src/persistence/postgres/agent_run_lineage_repository.rs:116` 在 fork materialization 中 `child_run.context = input.parent_run.context.clone()`。
- `crates/agentdash-application-agentrun/src/test_support/workflow_repositories.rs:204` 测试 fork materialization 同样 clone parent context。
- `crates/agentdash-application-agentrun/src/agent_run/workspace/query.rs:233` 到 `crates/agentdash-application-agentrun/src/agent_run/workspace/query.rs:251` workspace snapshot 从 frame/runtime/subject/gate 组合 conversation，不读 `LifecycleContext`。
- `crates/agentdash-application-lifecycle/src/lifecycle/session_run_context_resolver.rs:69` 到 `crates/agentdash-application-lifecycle/src/lifecycle/session_run_context_resolver.rs:87` runtime trace 到业务 context 通过 subject association 构造，不读 `LifecycleRun.context`。

D-016/D-017 分类建议：

- D-016：`LifecycleContext` 当前不是 independent fact source，也不满足 child table 或 runtime trace store 资格；它是重复 embedded snapshot / stale read projection 候选。
- D-017：没有独立锁、scan、claim、分页、恢复需求；`permission_scope/budget` 若未来成为不可丢失治理状态，应以明确 control-plane state/binding 重建，而不是保留泛化 context。

实现风险：

- `.trellis/spec/backend/workflow/architecture.md` 仍把 `LifecycleRun.context` 描述为 orchestration contract owning aggregate 字段；WI-10 实现删除前应同步通过 spec update 记录新事实归属。
- `PostgresAgentRunForkMaterialization` 当前复制 parent context；删除 context 时需要同时处理 fork materialization 插入 `lifecycle_runs` 的列集，避免 WI-08 前后互相踩。
- repository roundtrip 测试显式断言 context；实现时测试需要改成验证删除后的 run roundtrip 字段。

建议的最小实现切片：

- 在 WI-10 代码切片中从 `LifecycleRun`、`PostgresWorkflowRepository`、test support fork materialization 中删除 context 读写和 clone。
- 在 WI-12 migration 切片中 `DROP COLUMN IF EXISTS context`。
- 若仍需 `permission_scope/budget`，另开明确 state/binding，不在本切片保留兼容字段。

### `view_projection`

当前用途：

- `view_projection` 是 `LifecycleRun` 上的 optional JSON 字段，当前主要被 repository roundtrip 和 fork materialization 复制使用。
- 真实 read model 通过 `run_view_builder` 从 `LifecycleRun.orchestrations`、agents、subject associations、runtime trace refs、execution_log 等事实重建。
- 未发现应用逻辑从 `view_projection` 反向推导 runtime state 或业务决策。

file:line 证据：

- `crates/agentdash-domain/src/workflow/entity.rs:171` 到 `crates/agentdash-domain/src/workflow/entity.rs:172` 定义 `view_projection: Option<Value>`。
- `crates/agentdash-infrastructure/migrations/0003_lifecycle_orchestration_contract.sql:4` 添加 `view_projection text`。
- `crates/agentdash-infrastructure/src/persistence/postgres/workflow_repository.rs:431` create 绑定 `view_projection`，`crates/agentdash-infrastructure/src/persistence/postgres/workflow_repository.rs:505` update 绑定 `view_projection`，`crates/agentdash-infrastructure/src/persistence/postgres/workflow_repository.rs:673` 到 `crates/agentdash-infrastructure/src/persistence/postgres/workflow_repository.rs:677` 读取 parse。
- `crates/agentdash-infrastructure/src/persistence/postgres/workflow_repository.rs:1075`、`crates/agentdash-infrastructure/src/persistence/postgres/workflow_repository.rs:1090`、`crates/agentdash-infrastructure/src/persistence/postgres/workflow_repository.rs:1102`、`crates/agentdash-infrastructure/src/persistence/postgres/workflow_repository.rs:1115` 是 roundtrip 测试写入和断言。
- `crates/agentdash-infrastructure/src/persistence/postgres/agent_run_lineage_repository.rs:117` fork materialization clone parent `view_projection`。
- `crates/agentdash-application-lifecycle/src/lifecycle/run_view_builder.rs:88` 到 `crates/agentdash-application-lifecycle/src/lifecycle/run_view_builder.rs:102` assemble view 从 run/orchestrations/subject associations/runtime refs 派生，不读 `view_projection`。
- `crates/agentdash-application-lifecycle/src/lifecycle/run_view_builder.rs:451` 到 `crates/agentdash-application-lifecycle/src/lifecycle/run_view_builder.rs:528` 将 `run.orchestrations` 和 runtime nodes 转成 view。

D-016/D-017 分类建议：

- D-016：可重建 read projection/cache，不是 independent fact source。
- D-017：没有独立 scan/claim/lock/pagination，也没有不可丢失业务决策状态。建议删除 aggregate 字段或移出为明确可重建 projection，但当前代码证据支持删除。

实现风险：

- 删除字段需要同时清理 `RUN_COLS`、insert/update SQL、row mapping、fork materialization helper、roundtrip 测试。
- 如果并行 WI-09 正在整理 frontend/API projection，需避免把 `view_projection` 误当过渡 read model 重新接入。

建议的最小实现切片：

- WI-10 删除 domain/repository/test 中 `view_projection` 字段和 clone。
- WI-12 添加 `DROP COLUMN IF EXISTS view_projection`。
- read model 继续使用 `run_view_builder` 派生，不新增 replacement cache。

### `LifecycleGate` / `lifecycle_gates`

当前用途：

- `LifecycleGate` 是 durable wait/review/resume 点，包含 run/agent/frame、gate kind、correlation、status、payload、resolved metadata。
- gates 被 workflow HumanGate、companion request/response、wait activity、workspace snapshot 多处按 open gate 查询或按 gate id resolve。
- `lifecycle_gates` 已有 `agent_id,status` partial index、`correlation_id` index、`run_id` index；这满足 D-017 中 child table 的局部更新和 open-by-agent 查询要求。

file:line 证据：

- `crates/agentdash-domain/src/workflow/lifecycle_gate.rs:5` 到 `crates/agentdash-domain/src/workflow/lifecycle_gate.rs:8` 注释说明 durable wait/review/resume 和 correlation_id。
- `crates/agentdash-domain/src/workflow/lifecycle_gate.rs:30` 到 `crates/agentdash-domain/src/workflow/lifecycle_gate.rs:51` 创建 open gate；`crates/agentdash-domain/src/workflow/lifecycle_gate.rs:53` 到 `crates/agentdash-domain/src/workflow/lifecycle_gate.rs:61` resolve/is_open。
- `crates/agentdash-domain/src/workflow/repository.rs:118` 到 `crates/agentdash-domain/src/workflow/repository.rs:122` repository trait 暴露 create/get/list_open_for_agent/update。
- `crates/agentdash-infrastructure/migrations/0001_init.sql:268` 到 `crates/agentdash-infrastructure/migrations/0001_init.sql:280` 定义 `lifecycle_gates` 表。
- `crates/agentdash-infrastructure/migrations/0001_init.sql:1080`、`crates/agentdash-infrastructure/migrations/0001_init.sql:1082`、`crates/agentdash-infrastructure/migrations/0001_init.sql:1084` 定义 open-by-agent/correlation/run indexes。
- `crates/agentdash-infrastructure/src/persistence/postgres/lifecycle_anchor_repository.rs:748` 到 `crates/agentdash-infrastructure/src/persistence/postgres/lifecycle_anchor_repository.rs:825` 实现 create/get/list_open_for_agent/update。
- `crates/agentdash-application-workflow/src/gate/resolver.rs:61` 到 `crates/agentdash-application-workflow/src/gate/resolver.rs:93` open companion gate 并持久化。
- `crates/agentdash-application-workflow/src/gate/resolver.rs:96` 到 `crates/agentdash-application-workflow/src/gate/resolver.rs:128` open workflow HumanGate，payload 携带 orchestration/node/attempt。
- `crates/agentdash-application-workflow/src/gate/resolver.rs:131` 到 `crates/agentdash-application-workflow/src/gate/resolver.rs:151` resolve workflow HumanGate。
- `crates/agentdash-application-workflow/src/gate/resolver.rs:442` 到 `crates/agentdash-application-workflow/src/gate/resolver.rs:456` resolve 前通过 `load_open_gate` 强制 gate 仍 open。
- `crates/agentdash-application-workflow/src/orchestration/human_gate_launcher.rs:61` 到 `crates/agentdash-application-workflow/src/orchestration/human_gate_launcher.rs:90` open HumanGate 后返回 `NodeStarted` event，executor ref 为 HumanDecision gate id。
- `crates/agentdash-application-agentrun/src/agent_run/workspace/query.rs:188` 到 `crates/agentdash-application-agentrun/src/agent_run/workspace/query.rs:196` workspace conversation snapshot 读取 agent open gates 为 waiting items。
- `crates/agentdash-application/src/wait_activity/service.rs:257` 到 `crates/agentdash-application/src/wait_activity/service.rs:280` wait activity 按 agent 查询 open gates。
- `crates/agentdash-application/src/companion/tools.rs:1797` 到 `crates/agentdash-application/src/companion/tools.rs:1803` companion tools 读取 child agent open gate correlation。
- `crates/agentdash-application/src/companion/gate_control.rs:548` 到 `crates/agentdash-application/src/companion/gate_control.rs:557` child result 按 child agent open gates + correlation_id 查找 request。

D-016/D-017 分类建议：

- D-016：Lifecycle-owned parent child table port。它不是同级 aggregate，但也不应合回 `LifecycleRun` JSONB。
- D-017：保留物理表。原因是 open-by-agent 查询、status 局部更新、correlation resume/polling、跨 transport gate resolve 都需要索引和独立行更新。

实现风险：

- 现 trait 没有 `find_open_by_agent_and_correlation`，部分调用先 `list_open_for_agent` 再内存按 correlation 过滤；如果 open gate 数量增长，可能需要补一个窄 query port。
- status 仍是 string，缺少强 enum/check 约束；收敛时不要把这个问题和删除表混在一起。
- gate resolver 同时返回 delivery/notification intents；改表形态会影响 companion/workflow adapter。

建议的最小实现切片：

- WI-10 保留 `lifecycle_gates` 物理表和 repository trait，文档/命名层标注为 Lifecycle child table port。
- 如做轻量优化，只新增 agent+correlation 的查询方法和索引，不迁入 JSONB。
- WI-12 只做需要的约束/index migration，不删除 gate 表。

### `LifecycleSubjectAssociation` / `lifecycle_subject_associations`

当前用途：

- `LifecycleSubjectAssociation` 表达 `SubjectRef` 到 whole run 或 agent 的关联，当前同时承担 subject reverse lookup、run/agent anchor 展示、frame construction context、task/routine/permission/control scope 等入口。
- 代码存在两个主查询方向：`list_by_subject` 用于 subject -> run/agent，`list_by_anchor` 用于 run/agent -> subject context 和 DTO projection。
- 物理表已有 subject、anchor_run、anchor_agent 索引，符合 indexed relationship table 资格。

file:line 证据：

- `crates/agentdash-domain/src/workflow/lifecycle_subject_association.rs:5` 到 `crates/agentdash-domain/src/workflow/lifecycle_subject_association.rs:8` 注释定义 SubjectRef 到 whole run 或 LifecycleAgent 的关系。
- `crates/agentdash-domain/src/workflow/lifecycle_subject_association.rs:39` 到 `crates/agentdash-domain/src/workflow/lifecycle_subject_association.rs:77` 提供 run-scoped 和 agent-scoped constructors。
- `crates/agentdash-domain/src/workflow/repository.rs:103` 到 `crates/agentdash-domain/src/workflow/repository.rs:114` repository trait 提供 create/list_by_subject/list_by_anchor/delete。
- `crates/agentdash-infrastructure/migrations/0001_init.sql:294` 到 `crates/agentdash-infrastructure/migrations/0001_init.sql:303` 定义 `lifecycle_subject_associations` 表。
- `crates/agentdash-infrastructure/migrations/0001_init.sql:1090`、`crates/agentdash-infrastructure/migrations/0001_init.sql:1092`、`crates/agentdash-infrastructure/migrations/0001_init.sql:1094` 定义 anchor/subject indexes。
- `crates/agentdash-infrastructure/src/persistence/postgres/lifecycle_anchor_repository.rs:610` 到 `crates/agentdash-infrastructure/src/persistence/postgres/lifecycle_anchor_repository.rs:693` 实现 create/list_by_subject/list_by_anchor/delete。
- `crates/agentdash-application-lifecycle/src/lifecycle/run_view_builder.rs:75` 到 `crates/agentdash-application-lifecycle/src/lifecycle/run_view_builder.rs:85` run view 读取 run-scoped 和 agent-scoped associations。
- `crates/agentdash-application-lifecycle/src/lifecycle/run_view_builder.rs:110` 到 `crates/agentdash-application-lifecycle/src/lifecycle/run_view_builder.rs:119` SubjectExecutionView 从 subject reverse lookup 开始。
- `crates/agentdash-application-lifecycle/src/lifecycle/session_run_context_resolver.rs:69` 到 `crates/agentdash-application-lifecycle/src/lifecycle/session_run_context_resolver.rs:87` runtime trace context 先 agent anchor，空时回退 whole-run anchor。
- `crates/agentdash-application-lifecycle/src/lifecycle/subject_execution_control.rs:145` 到 `crates/agentdash-application-lifecycle/src/lifecycle/subject_execution_control.rs:175` subject cancel/control 使用 `list_by_subject` 定位 run/agent/current delivery。
- `crates/agentdash-application/src/task/plan.rs:264` 到 `crates/agentdash-application/src/task/plan.rs:313` task lookup 通过 `SubjectRef(kind=task)` 反查 run ids。
- `crates/agentdash-application/src/task/plan.rs:316` 到 `crates/agentdash-application/src/task/plan.rs:345` story task projection 通过 story subject 反查关联 runs。
- `crates/agentdash-application/src/task/context_builder.rs:229` 到 `crates/agentdash-application/src/task/context_builder.rs:244` task active workflow projection 从 task subject associations 开始。
- `crates/agentdash-application/src/frame_construction/composer_project_agent.rs:242` 到 `crates/agentdash-application/src/frame_construction/composer_project_agent.rs:256` frame construction 通过 agent anchor association 找 subject context。
- `crates/agentdash-mcp/src/servers/story.rs:251` 到 `crates/agentdash-mcp/src/servers/story.rs:265` MCP story server 验证 run 是否 story-bound。
- `crates/agentdash-contracts/src/runtime/workflow.rs:1411` 到 `crates/agentdash-contracts/src/runtime/workflow.rs:1425` DTO 直接暴露 subject associations。

D-016/D-017 分类建议：

- D-016：indexed relationship table / Lifecycle association port。它是关系事实，不是可重建 projection，也不应塞进 AgentRun workspace projection cache。
- D-017：保留物理表。subject reverse lookup、anchor lookup、role/metadata projection、跨 task/routine/story/control scope 的反查需要索引。

实现风险：

- 当前 migration 只有 `idx_lsa_anchor_run` 和 partial `idx_lsa_anchor_agent`，而 `list_by_anchor(run_id, Some(agent_id))` 使用 `anchor_run_id=$1 AND anchor_agent_id=$2`；若数据量增长，建议添加 composite index `(anchor_run_id, anchor_agent_id)`。
- `.trellis/spec/backend/workflow/lifecycle-run-link.md` 中 trait 还列了 `list_by_subject_and_role` / `delete_by_run`，当前代码 trait 没有；WI-10 不必补全全部 spec API，除非实现切片需要。
- association 不应承载 runtime node anchor；runtime node 证据继续来自 `RuntimeSessionExecutionAnchor` 和 orchestration state。

建议的最小实现切片：

- 保留表和 trait，按 D-016 命名为 Lifecycle association capability，而不是顶级 aggregate。
- 可在 WI-12 加 composite anchor index；不做 JSONB 合并。
- 如果删除 `LifecycleRun.context`，将所有 subject/context 读取固定走 associations，避免重新引入 context snapshot。

### `AgentLineage` / `agent_lineages`

当前用途：

- `AgentLineage` 表达同一 run 内 agent spawn/delegation/companion control tree。domain 注释明确 UI 控制树用 `AgentLineage`，RuntimeSessionLineage 只保留 trace/debug。
- 创建路径由 Lifecycle dispatch relation writer 根据 parent agent 写入。
- 消费路径包括 AgentRun 列表 root 收束、detail parent/children、SubjectExecution whole-run agent 过滤、companion parent lookup。
- 这与 product fork lineage 不同，不能和 `agent_run_lineages` 或 RuntimeSession lineage 混用。

file:line 证据：

- `crates/agentdash-domain/src/workflow/agent_lineage.rs:5` 到 `crates/agentdash-domain/src/workflow/agent_lineage.rs:8` 注释定义 same-run control tree。
- `crates/agentdash-domain/src/workflow/repository.rs:126` 到 `crates/agentdash-domain/src/workflow/repository.rs:132` trait 提供 create/list_children/find_parent/list_by_run。
- `crates/agentdash-infrastructure/migrations/0001_init.sql:58` 到 `crates/agentdash-infrastructure/migrations/0001_init.sql:67` 定义 `agent_lineages` 表。
- `crates/agentdash-infrastructure/migrations/0001_init.sql:1038`、`crates/agentdash-infrastructure/migrations/0001_init.sql:1040`、`crates/agentdash-infrastructure/migrations/0001_init.sql:1042` 定义 child/parent/run indexes。
- `crates/agentdash-infrastructure/src/persistence/postgres/lifecycle_anchor_repository.rs:880` 到 `crates/agentdash-infrastructure/src/persistence/postgres/lifecycle_anchor_repository.rs:947` 实现 create/list_children/find_parent/list_by_run。
- `crates/agentdash-application-lifecycle/src/lifecycle/dispatch/lifecycle_relation_writer.rs:33` 到 `crates/agentdash-application-lifecycle/src/lifecycle/dispatch/lifecycle_relation_writer.rs:50` dispatch 有 parent_agent_id 时创建 `AgentLineage`。
- `crates/agentdash-api/src/routes/lifecycle_agents.rs:244` 到 `crates/agentdash-api/src/routes/lifecycle_agents.rs:267` AgentRun 列表一次 `list_by_run` 构建 control tree forest，只输出 root agents。
- `crates/agentdash-api/src/routes/lifecycle_agents.rs:557` 到 `crates/agentdash-api/src/routes/lifecycle_agents.rs:635` detail 使用同一 run lineage 构造一跳 parent/children refs。
- `crates/agentdash-api/src/routes/lifecycle_agents.rs:1630` 到 `crates/agentdash-api/src/routes/lifecycle_agents.rs:1645` `build_lineage_forest` 从 lineage edges 构建 parent -> children 与 child set。
- `crates/agentdash-api/src/routes/lifecycle_agents.rs:2282` 到 `crates/agentdash-api/src/routes/lifecycle_agents.rs:2313` descendant count 带 visited 防环和深度上限。
- `crates/agentdash-application-lifecycle/src/lifecycle/run_view_builder.rs:260` 到 `crates/agentdash-application-lifecycle/src/lifecycle/run_view_builder.rs:295` whole-run subject history 用 lineage 排除 child agents。
- `crates/agentdash-application/src/companion/gate_control.rs:693` 到 `crates/agentdash-application/src/companion/gate_control.rs:727` companion parent request 通过 child frame -> `AgentLineage.find_parent` 定位 parent agent/current delivery。
- `crates/agentdash-contracts/src/runtime/workflow.rs:1652` 到 `crates/agentdash-contracts/src/runtime/workflow.rs:1667` DTO 注释说明 `AgentRunLineageRef` 是 AgentRun lineage 控制树一跳引用，relation_kind 来自 `AgentLineage`。

D-016/D-017 分类建议：

- D-016：Lifecycle/AgentRun control surface 的 parent-owned child table，语义为 same-run agent control tree。不是 independent product fork source。
- D-017：保留物理表。parent/child/run 三类查询、root 收束、companion parent lookup、任意深度 DFS 都需要 indexed rows。

实现风险：

- DTO 名 `AgentRunLineageRef` 容易和 product fork `AgentRunLineage` 混淆，但当前注释已说明它引用 same-run `AgentLineage`。
- 数据库层缺少显式 acyclic/unique-parent 约束，应用层通过 parent lookup 和 DFS visited 规避；若后续要强约束，需要 migration 和写入冲突处理。
- 如果 WI-08 重命名 product fork record，同时改 DTO 名可能与 WI-10 并行冲突。

建议的最小实现切片：

- WI-10 保留 `agent_lineages` 表，标注为 same-run control tree child table。
- 不把它合并进 `LifecycleRun.context` 或 `orchestrations` JSON。
- 命名清理可以只改文档/局部 comments；跨 DTO rename 等 WI-08 lineage 语义稳定后做。

### Product Fork Boundary: `AgentRunLineage` / `agent_run_lineages`

当前用途：

- `AgentRunLineage` 是跨 run fork lineage，当前仍包含 `parent_runtime_session_id` 和 `child_runtime_session_id` 必填字段。
- fork materialization 在创建 child LifecycleRun 时复制 parent context/view_projection，并写入 product fork lineage 表；fork replay 还会从 command receipt `result_json` 还原 lineage。
- 这属于 WI-08 的 product fork canonical record 范围，但 WI-10 删除 `context/view_projection` 会触达 fork materialization 插入列。

file:line 证据：

- `crates/agentdash-domain/src/workflow/agent_run_lineage.rs:6` 到 `crates/agentdash-domain/src/workflow/agent_run_lineage.rs:10` 注释说明它链接 forked child AgentRun 到 parent AgentRun/runtime trace boundary。
- `crates/agentdash-domain/src/workflow/agent_run_lineage.rs:23` 到 `crates/agentdash-domain/src/workflow/agent_run_lineage.rs:24` runtime session ids 是必填字段。
- `crates/agentdash-infrastructure/migrations/0038_agent_run_lineages.sql:1` 到 `crates/agentdash-infrastructure/migrations/0038_agent_run_lineages.sql:18` 定义 product fork lineage 表并要求 parent/child runtime session ids `NOT NULL`。
- `crates/agentdash-infrastructure/migrations/0038_agent_run_lineages.sql:20` 到 `crates/agentdash-infrastructure/migrations/0038_agent_run_lineages.sql:30` 定义 parent/child runtime indexes。
- `crates/agentdash-infrastructure/src/persistence/postgres/agent_run_lineage_repository.rs:38` 到 `crates/agentdash-infrastructure/src/persistence/postgres/agent_run_lineage_repository.rs:93` product lineage repo 支持 create/find_parent/list_children/list_by_run。
- `crates/agentdash-infrastructure/src/persistence/postgres/agent_run_lineage_repository.rs:114` 到 `crates/agentdash-infrastructure/src/persistence/postgres/agent_run_lineage_repository.rs:118` fork materialization clone parent context/view_projection。
- `crates/agentdash-application-agentrun/src/agent_run/fork.rs:36` 到 `crates/agentdash-application-agentrun/src/agent_run/fork.rs:45` AgentRun fork repos 注入 `AgentRunLineageRepository` 和 fork materialization port。
- `crates/agentdash-application-agentrun/src/agent_run/fork.rs:795` 到 `crates/agentdash-application-agentrun/src/agent_run/fork.rs:835` 从 receipt result_json 还原 `AgentRunLineage`。

D-016/D-017 分类建议：

- D-016：AgentRun child lineage / fork record，待 WI-08 收束为 canonical product fork fact；不要归入 Lifecycle same-run `AgentLineage`。
- D-017：是否保留物理表由 product fork tree 查询、审计和 replay 需求决定。当前 evidence 支持保留 product fork record，但 runtime session id 必填和 runtime indexes 是 WI-08 删除/降级风险点。

实现风险：

- WI-10 删除 context/view_projection 时必须更新 fork materialization 的 lifecycle_runs insert 列，否则即使不处理 fork schema 也会编译或测试失败。
- product fork canonical semantics 未稳定前，不建议 WI-10 改 `agent_run_lineages` 表结构或 DTO 命名。

建议的最小实现切片：

- WI-10 仅移除 fork materialization 中对 deleted LifecycleRun fields 的 clone/insert。
- `agent_run_lineages` 表、runtime session id 必填、receipt result_json lineage cache 留给 WI-08/WI-12。

### `LifecycleRun.orchestrations`

当前用途：

- `orchestrations` 是 `LifecycleRun` 内部 0..N `OrchestrationInstance`，保存 plan snapshot、runtime node state、dispatch ready queue、state exchange、journal cursor。
- reducer 在加载的 `LifecycleRun` 内找到 matching `orchestration_id` 并整体更新 run。
- ready-node lookup 当前扫描 loaded `run.orchestrations` 和 `dispatch.ready_node_ids`，没有独立 DB claim/scan API。
- run view、subject execution、VFS/journey surface 也从 loaded `LifecycleRun.orchestrations` 派生。

file:line 证据：

- `crates/agentdash-domain/src/workflow/value_objects/orchestration.rs:48` 到 `crates/agentdash-domain/src/workflow/value_objects/orchestration.rs:66` 定义 `OrchestrationInstance`，包含 `node_tree`、`dispatch`、`state_snapshot`、`journal_cursor`。
- `crates/agentdash-domain/src/workflow/entity.rs:167` 到 `crates/agentdash-domain/src/workflow/entity.rs:168` `LifecycleRun` 持有 `orchestrations`。
- `crates/agentdash-domain/src/workflow/entity.rs:235` 到 `crates/agentdash-domain/src/workflow/entity.rs:264` aggregate 提供 add/replace/find orchestration 方法，并刷新 run status。
- `crates/agentdash-infrastructure/migrations/0003_lifecycle_orchestration_contract.sql:3` 添加 `orchestrations text DEFAULT '[]'`。
- `crates/agentdash-infrastructure/src/persistence/postgres/workflow_repository.rs:429`、`crates/agentdash-infrastructure/src/persistence/postgres/workflow_repository.rs:503` 以整体 JSON 写入 `orchestrations`。
- `crates/agentdash-infrastructure/src/persistence/postgres/workflow_repository.rs:665` 到 `crates/agentdash-infrastructure/src/persistence/postgres/workflow_repository.rs:668` 以 JSON parse `orchestrations`。
- `crates/agentdash-application-workflow/src/orchestration/runtime.rs:266` 到 `crates/agentdash-application-workflow/src/orchestration/runtime.rs:283` reducer 在 run 内 `iter_mut` 找 orchestration 后更新整个 run。
- `crates/agentdash-application-workflow/src/orchestration/ready_node.rs:67` 到 `crates/agentdash-application-workflow/src/orchestration/ready_node.rs:83` next ready node 在 loaded run 的 orchestrations 中扫描。
- `crates/agentdash-application-lifecycle/src/lifecycle/run_view_builder.rs:451` 到 `crates/agentdash-application-lifecycle/src/lifecycle/run_view_builder.rs:528` read model 从 embedded orchestrations 派生。

D-016/D-017 分类建议：

- D-016：parent-owned child fact under `LifecycleRun` aggregate。
- D-017：当前保留 embedded JSON/text。未发现独立 scan/claim/pagination repository；ready queue 是 run 内 dispatch state。如果未来 multi-worker scheduler 需要跨 run 节点 claim/lease/恢复扫描，再拆 child table。

实现风险：

- 整体 JSON 更新在高并发 node advancement 下有 lost update 风险；目前没有 evidence 显示独立 DB claim 已存在。
- `.trellis/spec/backend/workflow/architecture.md` 仍要求 `LifecycleRun.orchestrations` owning aggregate 字段，此项与当前 evidence 一致。

建议的最小实现切片：

- WI-10 不拆 `orchestrations`。
- 删除 context/view_projection 时保持 `orchestrations` repository roundtrip 和 reducer tests。
- 若发现 scheduler 并发 claim 需求，另开 work item 拆 RuntimeNodeState child table，不在 WI-10 中做。

### `LifecycleRun.tasks`

当前用途：

- `tasks` 是 run-scoped durable Task plan facts，已从 Story schema 清理到 `LifecycleRun.tasks`。
- task create/update/archive/status/reorder 都加载 run、修改 aggregate tasks、整体 update run。
- subject associations 提供 story/task reverse lookup，tasks 本身没有独立 table scan/claim/pagination。

file:line 证据：

- `crates/agentdash-domain/src/workflow/entity.rs:169` 到 `crates/agentdash-domain/src/workflow/entity.rs:170` `LifecycleRun` 持有 tasks。
- `crates/agentdash-domain/src/workflow/entity.rs:271` 到 `crates/agentdash-domain/src/workflow/entity.rs:416` aggregate 提供 create/update/archive/status/reorder task 方法。
- `crates/agentdash-infrastructure/migrations/0015_lifecycle_run_tasks_story_task_cleanup.sql:1` 到 `crates/agentdash-infrastructure/migrations/0015_lifecycle_run_tasks_story_task_cleanup.sql:6` 添加 `lifecycle_runs.tasks` 并删除 Story tasks/task_count。
- `crates/agentdash-infrastructure/src/persistence/postgres/workflow_repository.rs:430`、`crates/agentdash-infrastructure/src/persistence/postgres/workflow_repository.rs:504` 整体 JSON 写入 tasks。
- `crates/agentdash-infrastructure/src/persistence/postgres/workflow_repository.rs:669` 到 `crates/agentdash-infrastructure/src/persistence/postgres/workflow_repository.rs:672` parse `lifecycle_runs.tasks`。
- `crates/agentdash-application/src/task/plan.rs:90` 到 `crates/agentdash-application/src/task/plan.rs:101` list tasks 加载 run 并过滤 `run.tasks`。
- `crates/agentdash-application/src/task/plan.rs:104` 到 `crates/agentdash-application/src/task/plan.rs:236` create/update/archive/status/reorder 均是 load run -> mutate aggregate -> repository update。
- `crates/agentdash-application/src/task/workspace.rs:72` 到 `crates/agentdash-application/src/task/workspace.rs:108` workspace task read 先调用 list_run_tasks，再内存按 status/task_id 过滤。
- `crates/agentdash-application/src/task/plan.rs:264` 到 `crates/agentdash-application/src/task/plan.rs:345` task/story reverse lookup 走 subject associations，再批量加载 run ids。

D-016/D-017 分类建议：

- D-016：parent-owned child fact under `LifecycleRun`。
- D-017：当前 embedded JSON/text 合理。没有 claim/lock/恢复扫描；排序和局部变更当前通过 aggregate 方法处理。若未来 task 需要跨 run 大规模分页/独立 assignment queue，再评估 child table。

实现风险：

- `find_project_task_plan_item` fallback 仍可能 list project runs 扫描 tasks；subject association reverse lookup 已是优化入口，但数据量大时仍需评估 task index/table。
- WI-10 删除 context/view_projection 不应误触 tasks 字段或 story cleanup migration。

建议的最小实现切片：

- 保持 `LifecycleRun.tasks` embedded。
- 只更新 repository column list 时保留 tasks parse/write。
- 如果做 migration，确认 `0015` 引入的 tasks 不被错误 drop。

### `LifecycleRun.execution_log`

当前用途：

- `execution_log` 是 run-scoped append-ish control-plane log，hook pending entries grouped by run id 后加载 run、append entries、update run。
- run view 和 VFS/journey surface 读取整个 log，没有独立 append table、分页或 replay cursor。

file:line 证据：

- `crates/agentdash-domain/src/workflow/entity.rs:174` 到 `crates/agentdash-domain/src/workflow/entity.rs:175` `LifecycleRun` 持有 `execution_log`。
- `crates/agentdash-domain/src/workflow/entity.rs:419` 到 `crates/agentdash-domain/src/workflow/entity.rs:424` `append_execution_log` 扩展 Vec 并 touch activity。
- `crates/agentdash-infrastructure/migrations/0001_init.sql:282` 到 `crates/agentdash-infrastructure/migrations/0001_init.sql:292` baseline `lifecycle_runs` 包含 `execution_log text DEFAULT '[]'`。
- `crates/agentdash-infrastructure/src/persistence/postgres/workflow_repository.rs:433`、`crates/agentdash-infrastructure/src/persistence/postgres/workflow_repository.rs:507` 整体 JSON 写入 execution_log。
- `crates/agentdash-infrastructure/src/persistence/postgres/workflow_repository.rs:679` parse `lifecycle_runs.execution_log`。
- `crates/agentdash-application-lifecycle/src/lifecycle/execution_log.rs:41` 到 `crates/agentdash-application-lifecycle/src/lifecycle/execution_log.rs:70` grouped by run id 后 `repo.get_by_id`、`run.append_execution_log`、`repo.update`。
- `crates/agentdash-application/src/repository_set.rs:398` 到 `crates/agentdash-application/src/repository_set.rs:410` hook projection adapter 调用 `flush_execution_log_entries`。
- `crates/agentdash-application-lifecycle/src/lifecycle/run_view_builder.rs:439` 到 `crates/agentdash-application-lifecycle/src/lifecycle/run_view_builder.rs:444` run view 映射 execution_log。

D-016/D-017 分类建议：

- D-016：parent-owned append-ish child fact under `LifecycleRun`。
- D-017：当前保留 embedded。没有独立 append-only event table、分页、恢复扫描或外部按 log id 查询需求。若 log 增长、审计、并发 append 或分页成为需求，再拆 event table。

实现风险：

- 并发 hook log flush 会整体更新 run，有 lost append 风险。
- log 增长会放大 `LifecycleRun` row payload 和 read model成本。
- 当前不是 WI-10 P0 删除目标，过早拆表会扩大 migration 范围。

建议的最小实现切片：

- WI-10 不拆 `execution_log`。
- 删除 context/view_projection 时保持 log column 和 tests。
- 记录后续可选拆分条件：需要 append conflict safety、分页或审计不可变 event identity。

## Related Specs

- `.trellis/spec/backend/architecture.md`: repository/application/domain/infrastructure dependency invariants and cross-aggregate command port rule.
- `.trellis/spec/backend/repository-pattern.md`: repository reflects aggregate boundary; single aggregate repository may write owned fields atomically; cross-aggregate consistency must use explicit command port.
- `.trellis/spec/backend/database-guidelines.md`: migration is schema source of truth; ordinary schema changes add migrations; retired columns should be dropped with migration.
- `.trellis/spec/backend/workflow/architecture.md`: current workflow contract still names `context/orchestrations/view_projection` as owning aggregate fields. WI-10 decisions intentionally supersede context/view_projection after code evidence.
- `.trellis/spec/backend/workflow/lifecycle-run-link.md`: `LifecycleSubjectAssociation` query paths, runtime-session-to-subject control-plane lookup, and AgentLineage control tree contract.

## External References

- No external references used. This inventory is based on current repository facts only.

## 可并行实现建议

- 可与 WI-02 并行：删除 `LifecycleRun.context` / `view_projection` 的 domain/repository/test 调整可以并行于 RuntimeSession query-port/table rename，只要不同时编辑同一 migration 文件；实际 schema drop 应排入 WI-12 migration 批次。
- 可与 WI-04 并行：`lifecycle_gates` 保留、`lifecycle_subject_associations` 保留、`agent_lineages` 保留的分类落地主要是 Lifecycle storage 层，不应触碰 mailbox owner correction。若 WI-04 改 companion gate delivery path，应避免同时重命名 gate resolver DTO/commands。
- 可与 WI-09 并行：`view_projection` 删除可以和 frontend/API projection cleanup 并行，但双方必须以 `run_view_builder` / AgentRun workspace query 作为 read model 来源，不能让 WI-09 临时消费 `lifecycle_runs.view_projection`。
- 需要等 WI-12 migration：任何 `DROP COLUMN lifecycle_runs.context/view_projection`、新增 subject composite index、约束调整都必须通过 migration 批次执行和验证。
- 需要等 WI-08 或 lineage/fork 语义稳定：`agent_run_lineages` runtime session id 必填、fork receipt `result_json` lineage cache、product fork record 命名/DTO 重塑。WI-10 只能处理因删除 context/view_projection 造成的 fork materialization column cleanup。
- 不建议并行大 rename：`AgentRunLineageRef` DTO 当前实际引用 same-run `AgentLineage` 控制树。若和 WI-08 同时改名，容易与 product fork lineage 重塑冲突。

## Caveats / Not Found

- `task.py current --source` 返回当前任务为空；本研究采用用户明确提供的 task path 和 output path，没有猜测 active task。
- 未发现 `permission_scope` / `budget` 在 `LifecycleContext` 上的生产 consumer；命中项主要是 type fields、roundtrip tests、workflow script limits budget、session compaction budget_scope 等非 LifecycleContext 用法。
- 未发现 `view_projection` 的业务 consumer；命中项主要是 repository roundtrip、fork materialization clone 和测试。
- 未发现 `orchestrations`、`tasks`、`execution_log` 的独立 DB scan/claim/pagination repository。当前证据均为加载 `LifecycleRun` 后内存处理或整体 update。
- 未运行 `git diff --name-only`：trellis-research agent 的范围规则禁止任何 git operation。已仅写入本 research 文件。
