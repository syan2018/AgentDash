# 数据库/仓储物理设计研究

## 基本真理

1. 物理表不是领域事实的默认形态。只有当一个事实需要独立并发控制、独立生命周期、按自身维度查询、追加式历史、跨聚合引用，或需要数据库约束表达不变量时，才应该成为独立表；否则应回到父聚合 JSON 字段。当前项目规范也要求 Repository 以聚合边界定义，而不是按表结构定义：`.trellis/spec/backend/architecture.md:76`、`.trellis/spec/backend/repository-pattern.md:9`。

2. `LifecycleRun` 是 workflow 控制账本。`context`、`orchestrations`、`tasks`、`view_projection` 是它拥有的聚合字段，不应拆成顶级仓储或独立 CRUD 表；规范明确 `LifecycleRunRepository` 以一次聚合写入覆盖 context/orchestrations/tasks/projection：`.trellis/spec/backend/repository-pattern.md:30`、`.trellis/spec/backend/database-guidelines.md:31`、`.trellis/spec/backend/workflow/architecture.md:149`。

3. 下文说“父聚合 JSON”指语义上属于父聚合的结构化字段；当前项目物理规范是用 `TEXT` 承载 JSON，且列名使用业务语义而不是 `_json` 后缀：`.trellis/spec/backend/database-guidelines.md:19`、`.trellis/spec/backend/database-guidelines.md:23`。如果未来统一切换 JSONB，所有 ownership 结论不变。

4. `RuntimeSession` 是 delivery/trace，不是产品控制面的 owner。产品可见控制树来自 `LifecycleRun`、`LifecycleAgent`、`AgentFrame`、`AgentLineage`、`AgentRunLineage`、Mailbox 和 current delivery binding；session lineage/projection 只是运行时 trace 与模型上下文投影：`.trellis/spec/backend/workflow/architecture.md:31`、`.trellis/spec/backend/session/session-lineage-projection.md:5`、`.trellis/spec/backend/session/session-lineage-projection.md:35`。

5. `AgentFrame` 是 `LifecycleAgent` 的 owned revision child，不是 agent 行上的 JSON blob。它表示运行表面修订，能力/上下文/VFS/MCP surface 变化会创建新 revision；因此需要 `(agent_id, revision)` 唯一性和 current revision 查询：`crates/agentdash-domain/src/workflow/agent_frame.rs:6`、`crates/agentdash-domain/src/workflow/repository.rs:83`、`crates/agentdash-infrastructure/migrations/postgres/0001_init.sql:40`、`crates/agentdash-infrastructure/migrations/postgres/0001_init.sql:1036`。

6. `Mailbox` 是 `(run, agent)` 拥有的操作队列表，而不是父聚合 JSON。它需要 claim、priority/order、idempotency、payload cleanup、pause/resume 和并发消费，已经在仓储里使用 `FOR UPDATE SKIP LOCKED`：`crates/agentdash-domain/src/agent_run_mailbox/mod.rs:437`、`crates/agentdash-infrastructure/src/persistence/postgres/agent_run_mailbox_repository.rs:224`。

7. `session_events` 是 append-only runtime fact stream；projection/compaction/head 是 session 模型上下文投影存储。它们不属于业务 RepositorySet，而属于 `SessionPersistence` SPI：`.trellis/spec/backend/repository-pattern.md:20`、`.trellis/spec/backend/session/context-compaction-projection.md:9`、`crates/agentdash-spi/src/session_persistence.rs:791`。

## 推荐设计

### 1. 独立表、父聚合 JSON、owned child table 的边界

应保留为独立表的事实：

- `sessions`、`session_events`、`session_compactions`、`session_projection_segments`、`session_projection_heads`、`session_lineage`：运行时 trace、append-only 事件、模型投影和 trace lineage，属于 `SessionPersistence`，不是业务聚合仓储。当前 `session_events` 已收敛到 envelope-only 物理列：`crates/agentdash-infrastructure/migrations/postgres/0040_session_events_envelope_only.sql:1`、`crates/agentdash-infrastructure/src/persistence/postgres/session_repository.rs:319`。
- `agent_run_mailbox_messages`、`agent_run_mailbox_states`：`LifecycleRun/LifecycleAgent` 拥有的操作队列和 pause/backend-selection 状态，需要并发 claim 与消息排序：`crates/agentdash-infrastructure/migrations/postgres/0013_agent_run_mailbox.sql:59`、`crates/agentdash-infrastructure/migrations/postgres/0013_agent_run_mailbox.sql:214`。
- `agent_run_command_receipts`：application command idempotency/receipt store。它是应用端口，不是领域聚合仓储：`crates/agentdash-domain/src/workflow/command_receipt.rs:143`、`crates/agentdash-infrastructure/migrations/postgres/0013_agent_run_mailbox.sql:1`。
- `runtime_session_execution_anchors`：runtime session 到 run/agent/frame/orchestration/node 的 launch evidence 和 read-model index。它支撑 current delivery 选择，但不拥有 node runtime state：`.trellis/spec/backend/workflow/architecture.md:94`、`crates/agentdash-domain/src/workflow/runtime_session_anchor.rs:5`、`crates/agentdash-infrastructure/migrations/postgres/0001_init.sql:533`。
- `agent_run_lineages`：跨 run 的产品 fork provenance，edge 跨越父子 run，不能塞进单个 `LifecycleRun` JSON：`crates/agentdash-domain/src/workflow/agent_run_lineage.rs:6`、`crates/agentdash-infrastructure/migrations/postgres/0038_agent_run_lineages.sql:1`。
- 静态定义类表，如 workflow graph / agent procedure，继续作为定义聚合存在；本研究不改变它们。

应回到或保持在父聚合 JSON 的事实：

- `LifecycleRun.context`：控制面上下文，引用 agent/frame/run，不嵌完整 `AgentFrame`：`.trellis/spec/backend/workflow/architecture.md:149`。
- `LifecycleRun.orchestrations`：`OrchestrationInstance`、runtime node state、journal facts 和 status 聚合。应用层 reducer 当前就是读取 run、替换 orchestration、再写回 run：`crates/agentdash-application-workflow/src/orchestration/runtime.rs:266`。
- `LifecycleRun.tasks`：task plan item 是 run 内 durable facts；Story 只读投影，不拥有 task CRUD：`.trellis/spec/backend/repository-pattern.md:38`、`crates/agentdash-domain/src/workflow/value_objects/task_plan.rs:43`、`crates/agentdash-infrastructure/migrations/postgres/0015_lifecycle_run_tasks_story_task_cleanup.sql:1`。
- `LifecycleRun.view_projection` 和 `execution_log`：读模型快照与日志属于 run 聚合，不应反向推导 runtime state：`.trellis/spec/backend/workflow/architecture.md:156`、`crates/agentdash-infrastructure/src/persistence/postgres/workflow_repository.rs:30`。
- Mailbox row 内的 delivery variant、source metadata、payload、executor config、preview 可以继续是行内 JSON 值，因为它们只在消息生命周期内整体读写；不应拆独立表：`crates/agentdash-domain/src/agent_run_mailbox/mod.rs:178`、`crates/agentdash-domain/src/agent_run_mailbox/mod.rs:352`。

应作为父 owned child table 的事实：

- `agent_frames`：`LifecycleAgent` 的 revision child。保留 `agent_id -> lifecycle_agents(id)` ownership cascade；保留或强化 `(agent_id, revision)` 唯一索引，并补一个面向 current 查询的 `(agent_id, revision DESC, created_at DESC)` 索引：`crates/agentdash-infrastructure/migrations/postgres/0001_init.sql:1212`。
- `agent_lineages`：同一 run 内 agent control tree 的 owned child。它支撑 list children / find parent / run tree 查询，但不应作为顶级业务仓储暴露：`crates/agentdash-domain/src/workflow/agent_lineage.rs:5`、`crates/agentdash-domain/src/workflow/repository.rs:126`。
- `lifecycle_gates`：run/agent 下的 durable wait/resume child。它必须独立成表，因为需要按 agent/correlation 查询 open gate，并跨进程恢复；但写入应由 lifecycle/workflow gate port 统一管理：`crates/agentdash-domain/src/workflow/lifecycle_gate.rs:5`、`crates/agentdash-infrastructure/migrations/postgres/0001_init.sql:268`。
- `lifecycle_subject_associations`：subject 与 anchor run 的关联是跨 subject 查询入口，物理上独立，但业务上是 lifecycle association port，不是顶级领域仓储。

### 2. Mailbox

推荐物理形态：

- 保留 `agent_run_mailbox_messages` 和 `agent_run_mailbox_states`，以 `(run_id, agent_id)` 为 ownership 根；run/agent 删除时删除其 mailbox 是正确 cascade：`crates/agentdash-infrastructure/migrations/postgres/0013_agent_run_mailbox.sql:172`、`crates/agentdash-infrastructure/migrations/postgres/0013_agent_run_mailbox.sql:231`。
- `runtime_session_id` 是 delivery target，不是 owner。当前 `runtime_session_id -> sessions ON DELETE CASCADE` 会让删除 trace/session 隐式删除产品队列或状态，这是错误 ownership。应改为 `ON DELETE RESTRICT`/`NO ACTION`，或在确实需要历史保留时改成 nullable `SET NULL` 并让应用显式把消息标记为 failed/deleted：`crates/agentdash-infrastructure/migrations/postgres/0013_agent_run_mailbox.sql:190`、`crates/agentdash-infrastructure/migrations/postgres/0013_agent_run_mailbox.sql:249`。
- 保留 idempotent create 的唯一键 `(run_id, agent_id, source_dedup_key) WHERE source_dedup_key IS NOT NULL`：`crates/agentdash-infrastructure/migrations/postgres/0013_agent_run_mailbox.sql:153`。
- claim/order 索引应围绕 live queue，而不是全表宽索引。目标索引建议：`(run_id, agent_id, priority DESC, order_key ASC) WHERE status IN ('accepted','queued','ready_to_consume','blocked','paused')`，以及 `(status, claim_expires_at) WHERE status = 'consuming'`。当前已有 run/agent/order、runtime/status、claim 三类索引，可在 migration 中替换为 partial 版本：`crates/agentdash-infrastructure/migrations/postgres/0013_agent_run_mailbox.sql:156`、`crates/agentdash-infrastructure/migrations/postgres/0013_agent_run_mailbox.sql:158`、`crates/agentdash-infrastructure/migrations/postgres/0013_agent_run_mailbox.sql:160`。
- `agent_run_command_receipts` 与 mailbox message 之间只能有一个方向的关联。当前双向 nullable FK 形成 cycle，表达不出谁拥有谁：`crates/agentdash-infrastructure/migrations/postgres/0013_agent_run_mailbox.sql:199`。建议保留 `agent_run_command_receipts.mailbox_message_id NULL REFERENCES agent_run_mailbox_messages(id) ON DELETE SET NULL`，删除 `agent_run_mailbox_messages.command_receipt_id` 及对应 FK；receipt 负责 idempotency，message 是 receipt 产生的可选 effect。

仓储形态：

- `AgentRunMailboxRepository` 是 application port/operational queue port，不是顶级领域 repository。它可以保留独立接口，因为 claim/recover/pause/resume/move 是队列能力，不适合塞进 `LifecycleRunRepository`：`crates/agentdash-domain/src/agent_run_mailbox/mod.rs:437`。
- 对业务层公开时应挂在 AgentRun/Lifecycle application service 下，而不是在所有 `RepositorySet` 中平铺暴露：`crates/agentdash-application-lifecycle/src/repository_set.rs:45`、`crates/agentdash-application-agentrun/src/agent_run_repository_set.rs:40`。

### 3. AgentFrame

推荐物理形态：

- `agent_frames` 保持 `LifecycleAgent` owned child table。frame surface 大字段继续作为结构化 JSON/text 列保存；不要把完整 frame 嵌回 `lifecycle_agents`，也不要恢复 `current_frame_id`：`crates/agentdash-infrastructure/migrations/postgres/0020_drop_lifecycle_agent_current_frame.sql:1`。
- current frame 是 `AgentFrameRepository.get_current(agent_id)` 的查询语义，即最高 revision，而不是 `lifecycle_agents.current_frame_id`：`crates/agentdash-domain/src/workflow/repository.rs:89`。
- `runtime_session_execution_anchors.launch_frame_id` 是 launch evidence，创建后不应被后续 frame revision 覆盖：`crates/agentdash-domain/src/workflow/runtime_session_anchor.rs:20`。
- 不允许“单独删除 frame”成为常规业务动作。若未来做 frame retention/pruning，anchor 的 `launch_frame_id` FK 不应 cascade 删除 anchor；应改成 `RESTRICT` 或 nullable `SET NULL`，并让 retention 策略显式处理 evidence 保留。

仓储形态：

- `AgentFrameRepository` 不应作为应用层顶级 repository 暴露。frame 创建/修订应通过 `AgentRuntimeMaterializationPort`、`FrameSurfacePort` 或 `LifecycleAgent` 聚合能力完成。当前 `AgentRuntimeMaterializer` 已经更接近正确边界：`crates/agentdash-application-lifecycle/src/lifecycle/dispatch/agent_runtime_materializer.rs:24`、`crates/agentdash-application-lifecycle/src/lifecycle/dispatch/agent_runtime_materializer.rs:115`。

### 4. Lineage

推荐物理形态：

- `agent_lineages` 表示同一 `LifecycleRun` 内的 agent control tree，应作为 run owned child。当前已有 `run_id` 与 `child_agent_id` FK，但缺少 `parent_agent_id` 和 `source_frame_id` FK：`crates/agentdash-infrastructure/migrations/postgres/0001_init.sql:58`、`crates/agentdash-infrastructure/migrations/postgres/0001_init.sql:1215`、`crates/agentdash-infrastructure/migrations/postgres/0001_init.sql:1218`。
- 建议新增 `parent_agent_id REFERENCES lifecycle_agents(id) ON DELETE CASCADE`；`source_frame_id REFERENCES agent_frames(id) ON DELETE SET NULL`。lineage edge 是产品控制事实，source frame 是证据引用；证据消失不应删除 edge。
- `AgentLineageRepository.find_parent(child_agent_id)` 暗示每个 child 在一个 run 内只有一个 parent，应增加 `UNIQUE (run_id, child_agent_id)`：`crates/agentdash-domain/src/workflow/repository.rs:129`。
- `agent_run_lineages` 表示跨 run fork provenance，应保持独立表，FK 到 parent/child run 和 agent 使用 cascade 是正确的，因为 provenance edge 不能脱离产品 run/agent 存在：`crates/agentdash-infrastructure/migrations/postgres/0038_agent_run_lineages.sql:1`。
- `agent_run_lineages.parent_runtime_session_id/child_runtime_session_id` 不建议强制 FK 到 `sessions`。这些 id 是 provenance evidence；删除 runtime trace 不应删除产品 fork lineage。若要校验存在性，应在应用命令中校验，而不是 cascade ownership。
- `session_lineage` 继续只表示 runtime trace tree，不参与产品控制树：`.trellis/spec/backend/session/session-lineage-projection.md:5`、`.trellis/spec/backend/session/session-lineage-projection.md:96`。

仓储形态：

- `AgentLineageRepository` 应降级为 `LifecycleRun`/dispatch relation writer 的内部 child-table port；当前 `LifecycleRelationWriter` 比直接暴露更接近正确方向：`crates/agentdash-application-lifecycle/src/lifecycle/dispatch/lifecycle_relation_writer.rs:3`。
- `AgentRunLineageRepository` 是 fork application port，不是普通顶级 aggregate repository；它应该由 fork use case 和 AgentRun read model 使用。

### 5. Lifecycle gate / task / orchestration

推荐物理形态：

- task 不建表，继续保存在 `lifecycle_runs.tasks`。Story 删除 `tasks/task_count` 是正确收敛：`crates/agentdash-infrastructure/migrations/postgres/0015_lifecycle_run_tasks_story_task_cleanup.sql:4`。
- orchestration 不建表，继续保存在 `lifecycle_runs.orchestrations`。`OrchestrationInstance` 是 run 内执行状态，应用层通过 `LifecycleRunRepository.update` 原子写回：`crates/agentdash-domain/src/workflow/entity.rs:235`、`crates/agentdash-infrastructure/src/persistence/postgres/workflow_repository.rs:497`。
- gate 保持 `lifecycle_gates` owned child table。需要补齐 ownership FKs：`agent_id REFERENCES lifecycle_agents(id) ON DELETE CASCADE`，`frame_id REFERENCES agent_frames(id) ON DELETE SET NULL`。当前只有 `run_id -> lifecycle_runs` cascade，导致 agent/frame 引用缺少数据库保护：`crates/agentdash-infrastructure/migrations/postgres/0001_init.sql:1230`。
- gate correlation 应表达“同一个 open resume point 不重复”。建议增加 partial unique：`UNIQUE (run_id, correlation_id) WHERE status = 'open'`，如果 correlation 只在 agent 内唯一，则用 `(run_id, agent_id, correlation_id) WHERE status = 'open'`。当前只有普通 correlation index：`crates/agentdash-infrastructure/migrations/postgres/0001_init.sql:1084`。

仓储形态：

- `LifecycleGateRepository` 不应作为顶级 repository 暴露给业务任意调用。workflow/lifecycle 应依赖 `LifecycleGatePort`，只提供 `open/resolve/list_open_for_agent` 这类 use-case 能力。当前 `ApplicationWorkflowRepositorySet` 直接包含 gate repo，是需要收敛的暴露面：`crates/agentdash-application-workflow/src/repository_set.rs:9`。

### 6. Current delivery

推荐物理形态：

- `runtime_session_execution_anchors` 保留为独立 index/read-model table。它的主键是 `runtime_session_id`，记录 launch 时刻 run/agent/frame/orchestration/node/attempt：`crates/agentdash-infrastructure/migrations/postgres/0004_orchestration_runtime_convergence.sql:1`。
- `lifecycle_agents.current_delivery_*` 保留为 agent 行上的 denormalized current binding，用于 workspace command/view 快速找到当前 delivery session；它由 anchor upsert 后同步，不是 runtime node state 的来源：`crates/agentdash-infrastructure/migrations/postgres/0017_lifecycle_agent_current_delivery_binding.sql:1`、`crates/agentdash-domain/src/workflow/lifecycle_agent.rs:126`、`crates/agentdash-application-lifecycle/src/lifecycle/dispatch/agent_runtime_materializer.rs:73`。
- anchor 的 owner 是 lifecycle run/agent 控制面，不是 session trace。当前 `runtime_session_id -> sessions ON DELETE CASCADE` 会让删除 session 隐式删除 anchor，进而破坏 current delivery / projection route；应改为 `RESTRICT`/`NO ACTION`，由删除 session 的应用命令先清理或转移 product bindings：`crates/agentdash-infrastructure/migrations/postgres/0002_runtime_session_anchor_fks.sql:7`。
- `launch_frame_id -> agent_frames ON DELETE CASCADE` 也不应表达“frame 删除拥有 anchor 删除”。短期可通过禁止单独删除 frame 避免问题；长期应改成 `RESTRICT` 或 nullable `SET NULL`：`crates/agentdash-infrastructure/migrations/postgres/0002_runtime_session_anchor_fks.sql:34`。

仓储形态：

- `RuntimeSessionExecutionAnchorRepository` 应作为 `CurrentDeliveryPort`/`RuntimeDeliveryIndexPort` 暴露，不应平铺在业务 `RepositorySet`。delivery command 不应该查询 `AgentFrame` 来决定 runtime refs，规范已经明确 anchor-first：`crates/agentdash-domain/src/workflow/runtime_session_anchor.rs:5`。

### 7. Projection

推荐物理形态：

- `LifecycleRun.view_projection` 是 run 聚合的业务读模型快照，不是 session projection store，也不是 runtime state 的权威来源：`.trellis/spec/backend/workflow/architecture.md:156`。
- session projection 使用 `session_projection_heads`、`session_projection_segments`、`session_compactions`；`head` 是当前模型可见 cursor，`segments` 是 checkpoint 内容，`compaction` 是生成记录：`.trellis/spec/backend/session/context-compaction-projection.md:15`、`.trellis/spec/backend/session/context-compaction-projection.md:21`。
- `session_events` 物理存储只需要 envelope 和时序列；派生的 turn/tool/update 字段属于 transport/projection 层，不应回到 DB 列：`.trellis/spec/cross-layer/backbone-protocol.md:169`、`crates/agentdash-infrastructure/src/persistence/postgres/session_repository.rs:1781`。

仓储形态：

- 不暴露业务顶级 `ProjectionRepository`。session projection 只通过 `SessionPersistence.projection_store` 和 context projector 使用；产品 AgentRun 页面通过 run/agent/current delivery 解析到 RuntimeSession 后读取同一 Backbone/projection：`.trellis/spec/backend/session/context-compaction-projection.md:89`。

## 删除清单

### 应删除或收敛的物理结构

1. 删除 mailbox/receipt 双向 FK cycle 中的一边。推荐删除：
   - `agent_run_mailbox_messages.command_receipt_id` 列；
   - `agent_run_mailbox_messages.command_receipt_id -> agent_run_command_receipts(id)` FK；
   - 若存在仅服务该方向的索引，也一并删除。

   保留 `agent_run_command_receipts.mailbox_message_id` 作为 receipt 到 produced message 的可选引用：`crates/agentdash-infrastructure/migrations/postgres/0013_agent_run_mailbox.sql:199`。

2. 删除错误 ownership cascade，不一定删除列：
   - `agent_run_mailbox_messages.runtime_session_id -> sessions ON DELETE CASCADE` 改为 `RESTRICT`/`NO ACTION`；
   - `agent_run_mailbox_states.runtime_session_id -> sessions ON DELETE CASCADE` 改为 `RESTRICT`/`NO ACTION`；
   - `runtime_session_execution_anchors.runtime_session_id -> sessions ON DELETE CASCADE` 改为 `RESTRICT`/`NO ACTION`；
   - `runtime_session_execution_anchors.launch_frame_id -> agent_frames ON DELETE CASCADE` 改为 `RESTRICT`，或在允许 frame retention 时改为 nullable `SET NULL`。

3. 保持已删除对象不回归：
   - `activity_execution_claims`、`agent_assignments`、`lifecycle_workflow_instances` 已由 orchestration convergence 删除：`crates/agentdash-infrastructure/migrations/postgres/0004_orchestration_runtime_convergence.sql:28`。
   - `lifecycle_agents.current_frame_id` 已删除，current frame 不应作为 agent 列恢复：`crates/agentdash-infrastructure/migrations/postgres/0020_drop_lifecycle_agent_current_frame.sql:1`。
   - `lifecycle_runs.root_graph_id`、anchor 旧 assignment/graph/activity/attempt 列已删除：`crates/agentdash-infrastructure/migrations/postgres/0004_orchestration_runtime_convergence.sql:6`。
   - `stories.tasks`、`stories.task_count` 已删除，task durable facts 回到 `LifecycleRun.tasks`：`crates/agentdash-infrastructure/migrations/postgres/0015_lifecycle_run_tasks_story_task_cleanup.sql:4`。
   - `session_events` 的 flattened update/turn/tool columns 已删除，DB 不应重新保存派生字段：`crates/agentdash-infrastructure/migrations/postgres/0040_session_events_envelope_only.sql:1`。
   - old mailbox `source` 列已由 source identity 字段替代：`crates/agentdash-infrastructure/migrations/postgres/0032_agent_run_mailbox_source_identity.sql:63`。
   - `lifecycle_agents.agent_role` 已删除，source/project_agent_slug 是当前事实：`crates/agentdash-infrastructure/migrations/postgres/0014_agent_source_enum_drop_role.sql:43`。

4. 不应删除：
   - `agent_frame_transitions` 仍被 session runtime command 使用，不是当前删除目标：`crates/agentdash-infrastructure/src/persistence/session_core.rs:204`。
   - `session_projection_heads`、`session_projection_segments`、`session_compactions` 是模型上下文投影的最小物理形态，不应并回 `sessions`：`.trellis/spec/backend/session/context-compaction-projection.md:15`。
   - `agent_frames` 不应并回 `lifecycle_agents`，它是 revision child table。

### 应从顶级 RepositorySet 删除或降级的仓储暴露

1. `LifecycleAgentRepository`：降级为 lifecycle control-plane 内部 child capability；外部通过 AgentRun/Lifecycle use case 操作 agent。
2. `AgentFrameRepository`：降级为 frame materialization/surface port；外部不直接 CRUD frame。
3. `LifecycleGateRepository`：降级为 `LifecycleGatePort`，暴露 open/resolve/list-open use case。
4. `AgentLineageRepository`：降级为 run relation writer/control-tree port。
5. `RuntimeSessionExecutionAnchorRepository`：降级为 current delivery/runtime delivery index port。
6. `AgentRunCommandReceiptRepository`：作为 command idempotency application port，而不是领域 aggregate repository。
7. `AgentRunMailboxRepository`：作为 operational mailbox application port，不在通用业务 RepositorySet 平铺。
8. `AgentRunLineageRepository`：作为 fork provenance application port，不作为普通 aggregate repository。
9. `SessionProjectionStore`、`SessionCompactionStore`、`SessionLineageStore`：只留在 `SessionPersistence` SPI 内，不进入业务 RepositorySet。

当前过度暴露位置包括：`crates/agentdash-application/src/repository_set.rs:56`、`crates/agentdash-application-lifecycle/src/repository_set.rs:45`、`crates/agentdash-application-agentrun/src/agent_run_repository_set.rs:40`、`crates/agentdash-application-workflow/src/repository_set.rs:9`。

## 迁移/实施顺序

1. 先收敛代码依赖边界，不先动数据。新增或整理 application ports：`LifecycleControlPlanePort`、`FrameSurfacePort`、`LifecycleGatePort`、`RuntimeDeliveryIndexPort`、`AgentRunMailboxPort`、`AgentRunForkLineagePort`。把业务服务从平铺 child repositories 迁出，保持现有 Postgres adapter 可复用。

2. 增加兼容性约束和索引。先清理可能重复数据，再增加：
   - `agent_lineages(parent_agent_id)` FK；
   - `agent_lineages(source_frame_id)` FK；
   - `UNIQUE (run_id, child_agent_id)`；
   - `lifecycle_gates(agent_id)` FK；
   - `lifecycle_gates(frame_id)` FK；
   - open gate partial unique；
   - mailbox live queue partial indexes。

3. 替换错误 cascade。每个 FK 用“添加正确 constraint -> 删除旧 constraint”的方式迁移；预研未上线时可以直接 `ALTER TABLE DROP CONSTRAINT/ADD CONSTRAINT`，但仍应保留 migration 记录：`.trellis/spec/backend/database-guidelines.md:41`。重点替换 session 到 anchor/mailbox 的 cascade，以及 frame 到 anchor 的 cascade。

4. 打断 command receipt 与 mailbox message 的 FK cycle。先确认所有 receipt/message 关联能从 `agent_run_command_receipts.mailbox_message_id` 表达；回填缺失值；再 drop `agent_run_mailbox_messages.command_receipt_id` FK 和列；最后更新 repository mapper 与 tests。

5. 删除 RepositorySet 平铺暴露。先改 application 构造和服务入参，再删除不再需要的 public fields。不要先删除 Postgres repository 实现；它们可作为 ports 的 infra adapter 继续存在，直到进一步合并。

6. 删除已退休 DB 对象只通过新 migration 表达。不要改旧 migration，除非单独执行 baseline squash。删除对象应遵循项目规范：mainline 已不读写后，新增 `DROP COLUMN IF EXISTS` / `DROP TABLE IF EXISTS` migration：`.trellis/spec/backend/database-guidelines.md:67`。

7. 验证顺序：
   - migration readiness/guard；
   - workflow repository roundtrip；
   - orchestration reducer/runtime tests；
   - mailbox claim/recover/pause/move tests；
   - session event/projection tests；
   - fork lineage and current delivery application tests。

## 需要验证的代码事实

1. `AgentFrameRepository.get_current` 的 Postgres 实现是否确实按 `revision DESC` 取 current frame；如果当前只依赖 `(agent_id)` 或 `(agent_id, revision)` ascending index，应补 descending index。接口语义见 `crates/agentdash-domain/src/workflow/repository.rs:89`。

2. `lifecycle_gates.frame_id` 在业务上是必须证据还是可选证据。若 gate 必须随 frame 删除而删除，用 cascade；若 gate 是 durable resume point，推荐 `SET NULL` 保留 gate。领域注释倾向 durable wait/resume：`crates/agentdash-domain/src/workflow/lifecycle_gate.rs:5`。

3. 产品是否允许删除 `sessions` 行。如果允许，必须定义删除前如何处理 current delivery、anchors、mailbox messages/states；否则应通过 FK `RESTRICT` 禁止删除被产品控制面引用的 session。

4. command receipt replay 是否需要 message -> receipt 的反查。如果只是根据 command idempotency 找 receipt，再找到 produced mailbox message，则保留 receipt -> message 足够；当前 receipt trait 已包含 `attach_mailbox_message`：`crates/agentdash-domain/src/workflow/command_receipt.rs:160`。

5. `agent_frame_transitions` 是否只服务 `session_runtime_commands`。当前仍有解析和一致性检查，不能当作废表删除：`crates/agentdash-infrastructure/src/persistence/session_core.rs:170`。

6. 前端/NDJSON 是否仍消费 `session_update_type`、`turn_id`、`entry_index`、`tool_call_id` 派生字段。DB 已 envelope-only，若 wire type 仍有这些字段，应由 envelope decode/projection 派生，而不是恢复 DB 列：`crates/agentdash-spi/src/session_persistence.rs:529`。

7. `agent_run_lineages.parent_runtime_session_id/child_runtime_session_id` 是否需要强存在校验。默认建议不加 FK，以免 runtime trace 删除影响产品 provenance；若要校验，只在 fork command 执行时校验 session 存在。
