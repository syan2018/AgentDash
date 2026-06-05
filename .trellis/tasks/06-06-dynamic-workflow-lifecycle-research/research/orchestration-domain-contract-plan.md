# Research: Orchestration domain contract / migration plan

- Query: 复核当前 Lifecycle / WorkflowGraph / Activity runtime / ExecutorRunRef / repository / migration 事实，并为下一阶段 `orchestration-domain-contract` 给出最小 domain contract、持久化字段、repository 更新点、迁移来源边界、测试计划和风险索引。
- Scope: mixed
- Date: 2026-06-06

## Findings

### 结论摘要

下一阶段 `orchestration-domain-contract` 应只建立共同 runtime contract 与持久化承载能力，不切换静态 graph runtime 的事实源。

最小可交付闭包建议是：

- 在 domain 层新增可序列化 orchestration value objects：`LifecycleContext`、`AgentRunRef`、`AgentFrameRef`、`OrchestrationInstance`、`OrchestrationSourceRef`、`OrchestrationStatus`、`OrchestrationPlanSnapshot`、`PlanNode`、`PlanNodeKind`、`ExecutorSpec`、`ActivationRule`、`RuntimeNodeState`、`RuntimeNodeStatus`、`DispatchState`、`StateExchangeSnapshot`、`OrchestrationJournalFact`。
- 在 `LifecycleRun` aggregate 上新增字段：`context`、`orchestrations`、`view_projection`，物理列建议为 `context_json`、`orchestrations_json`、`view_projection_json`。如果本轮愿意提前铺并发控制，可再加 `orchestration_revision BIGINT NOT NULL DEFAULT 0`，但不要引入 CAS 语义半成品。
- 更新 `LifecycleRunRepository` 的 PostgreSQL row mapping、insert/update/select，证明 `LifecycleRun` 能保存和读取 0..N 个 `OrchestrationInstance`。
- 保留 `WorkflowGraphInstance.activity_state`、`ActivityExecutionClaim`、`AgentAssignment`、`RuntimeSessionExecutionAnchor` 作为现有 runtime 事实源/lease/index。第一阶段只把它们标定为后续迁移来源，不让 `orchestrations_json` 成为并行运行时事实源。

### 当前事实复核

`LifecycleRun` 当前是 run ledger，字段只有 `id`、`project_id`、`topology`、`root_graph_id`、`status`、`execution_log` 和时间戳；没有 context/orchestration/projection 字段。`LifecycleRun::new_control` 创建 `topology=WorkflowGraph`，`new_graphless` 创建普通 Agent run；`sync_graph_instance_activity_projections` 从 graph instance activity states 聚合状态；`append_execution_log` 只追加摘要日志，不是可恢复 journal。

`WorkflowGraphInstance` 当前是 run 内 graph 生效实例，字段为 `id/run_id/graph_id/role/status/activity_state`。`replace_activity_state` 强制 `ActivityLifecycleRunState.graph_instance_id == WorkflowGraphInstance.id`，并用 activity state 同步 instance status。它是现有 Activity runtime snapshot 的事实源，不应直接扩展为 dynamic orchestration 的最终事实源。

`ActivityLifecycleRunState` 当前包含 `graph_instance_id/status/attempts/outputs/inputs`。`ActivityAttemptState` 只以 `activity_key + attempt` 定位 attempt，持有 `executor_run`、时间戳和 summary。`ExecutorRunRef` 已区分 `RuntimeSession`、`FunctionRun`、`HumanDecision`，说明目标 contract 必须支持 typed executor identity，不能只建 Agent node。

`ActivityExecutorSpec` 当前覆盖 Agent / Function / Human。Function executor 已覆盖 `ApiRequest` 与 `BashExec`；scheduler 启动 function 后会生成 `ExecutorRunRef::FunctionRun` 并立即把 terminal event 交给 engine。目标 `PlanNode` / `ExecutorSpec` 应直接保留这一闭包。

`ActivityExecutionClaim` 当前是 durable claim / lease，key 含 `run_id + graph_instance_id + activity_key + attempt`，repository 用 `idempotency_key` 做 create-or-get，active attempt 有唯一索引。它应迁移为未来 `DispatchState` / `DispatchLease` 的来源，而不是第一阶段搬成 `orchestrations[].dispatch` 的事实源。

`AgentAssignment` 当前是 Activity attempt 到 LifecycleAgent/AgentFrame 的执行桥，key 含 `graph_instance_id + activity_key + attempt`。它是后续 `AgentInvocation` / `RuntimeNodeBinding` 的迁移来源；第一阶段不应把动态 node 强行伪装成旧 attempt。

`RuntimeSessionExecutionAnchor` 当前是 runtime session 到 run / launch frame / agent / assignment / activity attempt 的 launch evidence。`0002_runtime_session_anchor_fks.sql` 已为 session/run/agent/frame 加 FK。它应保留为 trace 反向索引；`orchestration_id` / `runtime_node_id` / `node_path` 可以等 runtime 真正写 node refs 时再新增，避免空字段先行。

### 建议最小 domain 类型

建议新增 `crates/agentdash-domain/src/workflow/value_objects/orchestration.rs`，再从 `value_objects.rs` 与 `workflow/mod.rs` re-export。这样符合当前可序列化 workflow value types 的组织方式，不需要把 runtime IR 塞进 `entity.rs`。

`LifecycleContext`：

| 字段 | 类型建议 | 说明 |
| --- | --- | --- |
| `main_agent_run_id` | `Option<Uuid>` | 目标名用 AgentRun；第一阶段物理上可指向当前 `lifecycle_agents.id`。 |
| `agent_runs` | `Vec<AgentRunRef>` | run 内 main/child/collaborator agent 摘要引用。 |
| `frame_refs` | `Vec<AgentFrameRef>` | frame revision 引用，不内嵌完整 capability/context/VFS/MCP surface。 |
| `permission_scope` | `Option<PermissionScopeSnapshot>` 或 `Option<Value>` | 第一阶段如果没有权限类型，先用窄 snapshot value object，避免大面积引入 permission 模块。 |
| `budget` | `Option<BudgetState>` | lifecycle 级预算/已用量/策略，先可为空。 |

`AgentRunRef` 最小字段：`agent_run_id`、`role`、`status`、`current_frame_id`、`project_agent_id`。当前代码仍叫 `LifecycleAgent`，但 contract 字段名应朝 AgentRun 语义收敛。

`AgentFrameRef` 最小字段：`frame_id`、`agent_run_id`、`revision`、`procedure_id`、`graph_instance_id`、`activity_key`。它只表达引用，完整 surface 继续由 `AgentFrame` repository 拥有。

`OrchestrationInstance`：

| 字段 | 类型建议 | 说明 |
| --- | --- | --- |
| `orchestration_id` | `Uuid` | 对现有 graph instance 迁移时可直接采用 `WorkflowGraphInstance.id`。 |
| `role` | `String` 或小 enum + constants | 第一阶段建议沿用 string role，避免过早封死 `task_execution` 等现有角色。 |
| `source_ref` | `OrchestrationSourceRef` | graph/script/run artifact 来源。 |
| `status` | `OrchestrationStatus` | `pending/running/paused/completed/failed/cancelled`。 |
| `plan_snapshot` | `OrchestrationPlanSnapshot` | graph/script 编译后的不可变 plan。 |
| `activation` | `PlanActivation` | args、cursor、limits、ready roots、budget。 |
| `node_tree` | `Vec<RuntimeNodeState>` | phase / activity / agent / function / human / barrier 等运行状态。 |
| `dispatch` | `DispatchState` | ready queue、leases、outbox 摘要。第一阶段只建类型，不接旧 scheduler 写入。 |
| `state_snapshot` | `StateExchangeSnapshot` | 变量、node outputs、artifact/cache refs。 |
| `journal_cursor` | `u64` | 已 materialize 的 journal seq；第一阶段没有独立 journal 表也可保留为 0。 |
| `created_at` / `updated_at` | `DateTime<Utc>` | 和现有 run/instance 风格一致。 |

`OrchestrationSourceRef` 建议用 tagged enum：

- `WorkflowGraph { graph_id: Uuid, graph_version: Option<i32>, graph_instance_id: Option<Uuid> }`
- `RunScriptArtifact { artifact_id: Uuid, revision: i32, source_digest: String }`
- `WorkflowScript { script_id: Uuid, version: i32 }`
- `Inline { source_digest: String }`

第一阶段只需要实际构造 `WorkflowGraph` 变体；script 变体用于 contract 编译通过和序列化稳定，不创建资产表。

`OrchestrationPlanSnapshot`：

| 字段 | 类型建议 | 说明 |
| --- | --- | --- |
| `plan_id` | `Uuid` | 单份 snapshot identity。 |
| `plan_version` | `u32` | IR schema version，从 1 开始。 |
| `source_ref` | `OrchestrationSourceRef` | 可审计来源。 |
| `nodes` | `Vec<PlanNode>` | 扁平节点列表，节点用 `node_id`/`parent_node_id`/`node_path` 表达树。 |
| `entry_node_ids` | `Vec<String>` | 支持多个 ready roots。 |
| `activation_rules` | `Vec<ActivationRule>` | condition、dependency、artifact binding、join/retry/iteration。 |
| `limits` | `OrchestrationLimits` | 并发、总 agent/effect 数、预算、timeout、max traversals。 |
| `created_at` | `DateTime<Utc>` | snapshot 创建时间。 |

`PlanNode` 最小字段：`node_id`、`node_path`、`parent_node_id`、`kind`、`label`、`executor`、`input_ports`、`output_ports`、`result_contract`、`metadata`。

`PlanNodeKind` 最小变体：`Activity`、`AgentCall`、`Function`、`LocalEffect`、`ExtensionAction`、`HumanGate`、`Phase`、`ParallelGroup`、`Pipeline`、`Barrier`、`Subworkflow`。

`ExecutorSpec` 建议用 tagged enum，并复用现有 executor value objects：

- `AgentProcedure { procedure_key, agent_reuse_policy, runtime_session_policy }`
- `Function { spec: FunctionActivityExecutorSpec }`
- `Human { spec: HumanActivityExecutorSpec }`
- `LocalEffect { capability_key, input }`
- `ExtensionAction { extension_key, action_key, input }`

`RuntimeNodeState`：

| 字段 | 类型建议 | 说明 |
| --- | --- | --- |
| `node_id` / `node_path` | `String` | 运行节点 identity；`node_path` 支持 dynamic fan-out。 |
| `kind` | `PlanNodeKind` | 与 plan node kind 对齐。 |
| `status` | `RuntimeNodeStatus` | `pending/ready/claiming/running/blocked/completed/failed/cancelled/skipped`。 |
| `attempt` | `u32` | 保留重试/重启单节点能力。 |
| `inputs` / `outputs` | `Vec<NodePortValue>` | 结构化 JSON，不走自由文本解析。 |
| `executor_run_ref` | `Option<ExecutorRunRef>` | 复用 RuntimeSession/FunctionRun/HumanDecision；若本阶段新增 effect 节点，可扩展 `EffectInvocation { effect_id, kind }`。 |
| `children` | `Vec<RuntimeNodeState>` 或 `Vec<String>` | 如果 `node_tree` 已嵌套可用前者；如果 plan flat，可用 child ids。 |
| `phase_path` | `Vec<String>` | 进度树分组。 |
| `started_at` / `completed_at` | `Option<DateTime<Utc>>` | 执行时间。 |
| `error` | `Option<RuntimeNodeError>` | 失败原因与 retryable。 |
| `trace_refs` | `Vec<RuntimeTraceRef>` | session/effect/human gate 反查引用。 |
| `cache` | `Option<NodeCacheState>` | cache key、hit/miss、source node/run。 |

`StateExchangeSnapshot` 最小字段：`variables: BTreeMap<String, Value>`、`node_outputs: BTreeMap<String, Value>`、`artifacts: Vec<StateArtifactRef>`、`cache_refs: Vec<NodeCacheRef>`。第一阶段不要把 session events、large tool traces 或完整 AgentFrame surface 放入 snapshot。

`OrchestrationJournalFact` 可以先作为 enum 定义和 serde roundtrip，不建表、不接 runtime。最小变体：`PlanActivated`、`NodeReady`、`NodeClaimed`、`NodeStarted`、`NodeCompleted`、`NodeFailed`、`NodeCancelled`、`HumanGateOpened`、`HumanGateResolved`、`SnapshotMaterialized`、`DispatchLeaseRecorded`。

### 序列化边界

- 领域字段不使用 `_json` / `_jsonb` 后缀；只有物理列名使用 `*_json`。
- Rust domain 使用 `Uuid`、`DateTime<Utc>`、typed enums/value objects；PostgreSQL 存储使用 `TEXT` JSON，符合数据库规范。
- Enums 使用 `#[serde(rename_all = "snake_case")]`；多形态 enums 使用 `#[serde(tag = "kind", rename_all = "snake_case")]`。`FunctionActivityExecutorSpec` 当前使用 `type` tag，复用时保持现有 shape。
- `LifecycleRun.context` 可以引用 current `LifecycleAgent`/`AgentFrame`，但不内嵌完整 frame surface。
- `OrchestrationPlanSnapshot` 是 audit snapshot，允许内嵌到 `orchestrations_json`；第一阶段不要拆 `orchestration_plan_snapshots` 表。
- `OrchestrationJournalFact` 是可恢复事实的类型边界；第一阶段没有 append journal 表时，不要把 `LifecycleRun.execution_log` 解释为 journal。
- `RuntimeNodeState.trace_refs` 只保存 refs，不保存 session event payload。conversation 事实仍归 `session_events`。

### Migration 字段建议

普通实现任务应新增 migration，例如 `0003_lifecycle_orchestration_contract.sql`；根据数据库规范，不能修改已提交的 `0001_init.sql` / `0002_runtime_session_anchor_fks.sql`。

最小 schema：

```sql
ALTER TABLE lifecycle_runs
    ADD COLUMN IF NOT EXISTS context_json text DEFAULT '{}'::text NOT NULL,
    ADD COLUMN IF NOT EXISTS orchestrations_json text DEFAULT '[]'::text NOT NULL,
    ADD COLUMN IF NOT EXISTS view_projection_json text;
```

可选但建议谨慎：

```sql
ALTER TABLE lifecycle_runs
    ADD COLUMN IF NOT EXISTS orchestration_revision bigint DEFAULT 0 NOT NULL;
```

只有在 repository update 同步维护该 revision 时再加，避免出现看起来可并发控制、实际没有语义的字段。

本阶段不建议新增 `lifecycle_orchestration_journal_entries` 表。理由：runtime 尚未写 journal facts，新增 append 表只会制造未使用 schema。等 common runtime 开始持久化 `OrchestrationJournalFact` 后再新增：

```text
lifecycle_run_id
orchestration_id
seq
fact_kind
fact_json
created_at
```

本阶段也不建议新增 `runtime_session_execution_anchors.orchestration_id/runtime_node_id/node_path`。理由：旧 runtime 仍按 graph instance/activity attempt 写 anchor，新增 node 字段没有生产者。等 `AgentInvocation` 从 `RuntimeNodeState` 创建 runtime session 时，再把 anchor 扩展为 `RuntimeTraceAnchor`。

### Repository 更新点

`crates/agentdash-domain/src/workflow/entity.rs`：

- `LifecycleRun` 增加 `context: LifecycleContext`、`orchestrations: Vec<OrchestrationInstance>`、`view_projection: Option<LifecycleRunViewProjection>`。
- `new_control` / `new_graphless` 初始化空 context、空 orchestrations、空 projection。
- 增加最小 aggregate 方法：`set_lifecycle_context`、`add_orchestration`、`replace_orchestration`、`orchestration_by_id`。如果使用 `orchestration_revision`，这些方法负责 bump。

`crates/agentdash-domain/src/workflow/value_objects.rs` 和 `workflow/mod.rs`：

- 新增 `mod orchestration;` 并 re-export 类型。
- 保持 `run_state.rs` 不被塞入所有新类型；`run_state.rs` 保留当前 Activity runtime state 和 `ExecutorRunRef`。

`crates/agentdash-infrastructure/src/persistence/postgres/workflow_repository.rs`：

- 更新 `RUN_COLS` / `RUN_INSERT_COLS`，纳入 `context_json`、`orchestrations_json`、`view_projection_json`，以及可选 `orchestration_revision`。
- `LifecycleRunRepository::create` bind 新字段，使用 `serde_json::to_string`。
- `LifecycleRunRepository::update` 更新新字段；如果加 revision，明确是直接写入还是递增，不要让 DB 和 domain 各自维护。
- `LifecycleRunRow` 增加新列，`TryFrom<LifecycleRunRow>` 用现有 `parse_json_column` 风格解析，错误上下文写成 `lifecycle_runs.context_json` 等。
- 增加 row parse unit test，覆盖 0..N orchestration roundtrip 和 `ExecutorRunRef` variants。

`crates/agentdash-infrastructure/migrations/`：

- 新增 `0003_*` migration。
- 跑 `pnpm run migration:guard`。
- 不需要更新 `REQUIRED_POSTGRES_TABLES`，因为本阶段只加列。如果新增 journal 表，必须同步 `crates/agentdash-infrastructure/src/migration.rs` 的 readiness table list。

`crates/agentdash-contracts/src/workflow.rs` / generated TS：

- 第一阶段不必把新 orchestration contract 暴露给前端，除非本实现明确要新增 API DTO。
- 如果暴露，建议新增 DTO/TS 类型，不要直接改变 `LifecycleRunView.workflow_graph_instances`。当前 UI 仍从 Activity projection 恢复。

### 迁移来源矩阵

| 当前表/类型 | 第一阶段定位 | 后续迁移目标 |
| --- | --- | --- |
| `lifecycle_runs` | 新 contract 的 owning aggregate 物理承载点 | `context_json` / `orchestrations_json` / `view_projection_json` 成为 common runtime aggregate state。 |
| `lifecycle_agents` / `LifecycleAgent` | `LifecycleContext.agent_runs` 的来源；目标命名是 AgentRun | 未来可重命名为 AgentRun 或派生 AgentRun index，但第一阶段不改表名。 |
| `agent_frames` / `AgentFrame` | `LifecycleContext.frame_refs` 的来源 | frame surface 继续独立存储，context 只持 refs。 |
| `workflow_graphs` / `WorkflowGraph` | `OrchestrationSourceRef::WorkflowGraph` 与未来 compiler input | 编译到 `OrchestrationPlanSnapshot`。 |
| `lifecycle_workflow_instances` / `WorkflowGraphInstance` | 迁移来源；第一阶段仍是 Activity runtime snapshot 事实源 | `OrchestrationInstance`，`orchestration_id` 可采用旧 graph instance id。 |
| `ActivityLifecycleRunState` | 迁移来源；第一阶段不双写 | `RuntimeNodeState` + `StateExchangeSnapshot`。 |
| `ActivityAttemptState` | 迁移来源；第一阶段不双写 | `RuntimeNodeState.attempt/status/executor_run_ref/timestamps`。 |
| `ActivityExecutionClaim` | 现有 scheduler lease 事实源 | `DispatchState` / `NodeDispatchLease`；只有 common scheduler 接入后迁。 |
| `AgentAssignment` | 现有 Activity attempt -> agent/frame binding | `AgentInvocation` / `RuntimeNodeBinding`。 |
| `RuntimeSessionExecutionAnchor` | runtime session 反查索引 | `RuntimeTraceAnchor` 增加 orchestration/node refs 后继续作为索引。 |
| `LifecycleRun.execution_log` | 摘要日志 | 不作为 `OrchestrationJournal` 迁移来源。 |
| `session_events` | RuntimeSession conversation 事实流 | 不迁入 orchestration journal；只通过 trace refs 关联。 |

### 第一阶段明确不迁移的事实源

- 不把 scheduler 从 `WorkflowGraphInstance.activity_state` 切到 `LifecycleRun.orchestrations[]`。
- 不让 `orchestrations_json` 和 `activity_state_json` 同时作为可推进 runtime state。
- 不把 `ActivityExecutionClaim` rows 回填为 `DispatchState.leases` 的权威数据。
- 不把 `AgentAssignment` rows 回填为 `AgentInvocation` 的权威数据。
- 不把 `RuntimeSessionExecutionAnchor` 改成必须携带 orchestration/node refs。
- 不把 `LifecycleRunView` 改为消费 `view_projection_json`。
- 不新增 script asset / compiler / runtime primitive。
- 不把 `LifecycleRun.execution_log` 当作 journal。

第一阶段的验收应是“contract 能表达目标形态且 repository 能持久化”，不是“运行时已经使用新 contract 推进”。

### 最小测试计划

Domain unit tests：

- `OrchestrationPlanSnapshot` serde roundtrip：至少包含 agent node、function node、human gate node、phase/barrier node。
- `RuntimeNodeState` serde roundtrip：覆盖 `ExecutorRunRef::RuntimeSession`、`FunctionRun`、`HumanDecision`，如果扩展 effect variant，也覆盖该 variant。
- `LifecycleRun::new_control` / `new_graphless` 默认 context/orchestrations/projection 为空且可序列化。
- `LifecycleRun` 支持 0..N `OrchestrationInstance`，并拒绝或替换重复 `orchestration_id`，规则需固定。

Repository / migration tests：

- `LifecycleRunRow` parse unit test：`context_json` / `orchestrations_json` / `view_projection_json` 能解析；坏 JSON 返回带列名的 `DomainError`。
- PostgreSQL integration test（沿用现有 optional `test_pg_pool` 风格）：create lifecycle run with 2 orchestrations -> get_by_id -> update -> get_by_id，确认 JSON roundtrip。
- `pnpm run migration:guard`。

Targeted commands：

```powershell
cargo test -p agentdash-domain orchestration
cargo test -p agentdash-infrastructure workflow_repository
pnpm run migration:guard
```

如果新增 generated DTO：

```powershell
pnpm run contracts:check
```

### 风险点

- 双事实源风险：如果第一阶段开始把 scheduler 写入 `orchestrations_json`，而旧 runtime 继续写 `activity_state_json`，后续 bug 会来自两个 snapshot 不一致。
- 过早拆表风险：journal、dispatch lease、runtime node index 都有拆表理由，但现在没有 producer/consumer。先拆会把 repository 边界固化得过早。
- Agent-only 风险：当前 function/human executor 已是一等事实；新 `PlanNode` 若只围绕 AgentRun 设计，会立刻丢失现有能力。
- 命名落差风险：目标名 AgentRun 与当前 `LifecycleAgent` 表/类型不同。第一阶段应在 value object 中使用 AgentRun 语义，但不要半途重命名表和 repository。
- 大 JSON 写冲突风险：`LifecycleRunRepository::update` 当前整行 update；未来 orchestration runtime 高频更新 node state 时需要 CAS、journal 或 lease 表。第一阶段不要假装已经解决并发，只记录 revision 或留待 runtime 阶段。
- Migration 纪律风险：项目规范要求普通任务新增 migration，不能直接改 `0001_init.sql`。
- Projection 误用风险：`view_projection_json` 只能是 read projection，不应被 command/service 当作写入输入。

## Files Found

### Task context

- `.trellis/tasks/06-06-dynamic-workflow-lifecycle-research/prd.md` - planning gate、目标和验收标准。
- `.trellis/tasks/06-06-dynamic-workflow-lifecycle-research/design.md` - 目标 Lifecycle / Orchestration 架构、核心合同和分阶段方案。
- `.trellis/tasks/06-06-dynamic-workflow-lifecycle-research/implement.md` - 下一阶段子任务拆分与 `orchestration-domain-contract` 候选文件。
- `.trellis/tasks/06-06-dynamic-workflow-lifecycle-research/target-model-sketch.md` - LifecycleContext / OrchestrationInstance 目标模型草案。
- `.trellis/tasks/06-06-dynamic-workflow-lifecycle-research/research/current-code-context.md` - 当前代码事实地图。
- `.trellis/tasks/06-06-dynamic-workflow-lifecycle-research/research/claude-workflow-behavior-coverage.md` - Dynamic Workflow 行为覆盖矩阵。

### Specs

- `.trellis/workflow.md` - research artifact 必须持久化，Phase 2 sub-agent 只读上下文后执行。
- `.trellis/spec/backend/workflow/architecture.md` - 当前 Activity runtime invariants；明确现有 `WorkflowGraphInstance.activity_state` 是当前事实源。
- `.trellis/spec/backend/workflow/activity-lifecycle.md` - Activity executor / Function executor / graph instance contract。
- `.trellis/spec/backend/session/runtime-execution-state.md` - session-scoped AgentRun command API 和 RuntimeSessionExecutionAnchor 解析边界。
- `.trellis/spec/backend/session/architecture.md` - RuntimeSession 是 trace substrate，不拥有业务归属。
- `.trellis/spec/backend/repository-pattern.md` - repository port 按 aggregate 边界定义，不混入跨聚合事务。
- `.trellis/spec/backend/database-guidelines.md` - PostgreSQL migration 历史规则、复杂值对象 TEXT JSON、普通任务新增 migration。
- `.trellis/spec/frontend/workflow-activity-lifecycle.md` - 当前前端 run view 仍以 WorkflowGraphInstance/Activity attempt projection 为主。

### Source and migration

- `crates/agentdash-domain/src/workflow/entity.rs` - `WorkflowGraph`、`ActivityExecutionClaim`、`LifecycleRun`。
- `crates/agentdash-domain/src/workflow/workflow_graph_instance.rs` - `WorkflowGraphInstance` 和 activity state 归属校验。
- `crates/agentdash-domain/src/workflow/value_objects/run_state.rs` - `ActivityLifecycleRunState`、`ActivityAttemptState`、`ExecutorRunRef`。
- `crates/agentdash-domain/src/workflow/value_objects/activity_def.rs` - Activity executor / transition / artifact binding closure。
- `crates/agentdash-domain/src/workflow/lifecycle_agent.rs` - 当前 run-scoped Agent runtime identity。
- `crates/agentdash-domain/src/workflow/agent_frame.rs` - AgentFrame revision 和 runtime surface refs。
- `crates/agentdash-domain/src/workflow/agent_assignment.rs` - Activity attempt 到 agent/frame 的 bridge。
- `crates/agentdash-domain/src/workflow/runtime_session_anchor.rs` - runtime session 反查 run/agent/frame/assignment/attempt 的 anchor。
- `crates/agentdash-domain/src/workflow/repository.rs` - 当前 workflow repository ports。
- `crates/agentdash-application/src/workflow/dispatch_service.rs` - graph / graphless run 创建与 anchor 两段写入。
- `crates/agentdash-application/src/workflow/activity_run.rs` - ActivityEvent apply 后整体替换 activity_state 并同步 run projection。
- `crates/agentdash-application/src/workflow/scheduler.rs` - claim create-or-get、executor start、claim 状态同步。
- `crates/agentdash-application/src/workflow/agent_executor.rs` - Agent / Function / Human executor start closure。
- `crates/agentdash-application/src/workflow/lifecycle_run_view_builder.rs` - 当前 view 从 graph instances/activity state/assignments 投影。
- `crates/agentdash-infrastructure/src/persistence/postgres/workflow_repository.rs` - `LifecycleRunRepository`、`ActivityExecutionClaimRepository`、row mapping。
- `crates/agentdash-infrastructure/src/persistence/postgres/lifecycle_anchor_repository.rs` - graph instance、lifecycle agent、assignment、anchor repositories。
- `crates/agentdash-infrastructure/migrations/0001_init.sql` - 当前 lifecycle/claim/assignment/anchor 表结构和索引。
- `crates/agentdash-infrastructure/migrations/0002_runtime_session_anchor_fks.sql` - anchor FK 增量 migration。
- `crates/agentdash-infrastructure/src/migration.rs` - schema readiness table list；仅新增表时需要更新。
- `crates/agentdash-contracts/src/workflow.rs` - 当前 generated workflow DTO 仍暴露 Activity state / WorkflowGraphInstance view。
- `packages/app-web/src/services/lifecycle.ts` - session command API 已走 `/sessions/{runtimeSessionId}/...`。

## Code Patterns

- `LifecycleRun` 当前字段与构造器：`crates/agentdash-domain/src/workflow/entity.rs:203`、`crates/agentdash-domain/src/workflow/entity.rs:219`、`crates/agentdash-domain/src/workflow/entity.rs:234`。
- `LifecycleRun` 当前状态投影与摘要日志：`crates/agentdash-domain/src/workflow/entity.rs:249`、`crates/agentdash-domain/src/workflow/entity.rs:260`。
- `WorkflowGraphInstance.activity_state` 字段：`crates/agentdash-domain/src/workflow/workflow_graph_instance.rs:12`。
- `replace_activity_state` 归属校验：`crates/agentdash-domain/src/workflow/workflow_graph_instance.rs:57`。
- `ActivityLifecycleRunState` 和 `ActivityAttemptState`：`crates/agentdash-domain/src/workflow/value_objects/run_state.rs:25`、`crates/agentdash-domain/src/workflow/value_objects/run_state.rs:70`。
- `ExecutorRunRef` typed executor identity：`crates/agentdash-domain/src/workflow/value_objects/run_state.rs:94`。
- `ActivityExecutorSpec` 支持 Agent/Function/Human：`crates/agentdash-domain/src/workflow/value_objects/activity_def.rs:24`。
- `FunctionActivityExecutorSpec` 支持 API request / bash：`crates/agentdash-domain/src/workflow/value_objects/activity_def.rs:93`。
- Transition condition / artifact binding：`crates/agentdash-domain/src/workflow/value_objects/activity_def.rs:185`、`crates/agentdash-domain/src/workflow/value_objects/activity_def.rs:210`、`crates/agentdash-domain/src/workflow/value_objects/activity_def.rs:233`。
- `AgentAssignment` key 包含 graph instance/activity/attempt：`crates/agentdash-domain/src/workflow/agent_assignment.rs:10`.
- `RuntimeSessionExecutionAnchor` launch evidence 字段：`crates/agentdash-domain/src/workflow/runtime_session_anchor.rs:28`。
- `LifecycleAgent` 当前 run-scoped identity：`crates/agentdash-domain/src/workflow/lifecycle_agent.rs:19`。
- `AgentFrame` 当前 frame revision surface refs：`crates/agentdash-domain/src/workflow/agent_frame.rs:10`。
- `LifecycleRunRepository` trait：`crates/agentdash-domain/src/workflow/repository.rs:87`。
- `WorkflowGraphInstanceRepository` trait：`crates/agentdash-domain/src/workflow/repository.rs:103`。
- `ActivityExecutionClaimRepository` trait：`crates/agentdash-domain/src/workflow/repository.rs:66`。
- `AgentAssignmentRepository` trait：`crates/agentdash-domain/src/workflow/repository.rs:137`。
- `RuntimeSessionExecutionAnchorRepository` trait：`crates/agentdash-domain/src/workflow/repository.rs:188`。
- `LifecycleRunRepository::create/update` 当前只写 status/execution_log：`crates/agentdash-infrastructure/src/persistence/postgres/workflow_repository.rs:522`、`crates/agentdash-infrastructure/src/persistence/postgres/workflow_repository.rs:612`。
- `LifecycleRunRow` 当前列映射：`crates/agentdash-infrastructure/src/persistence/postgres/workflow_repository.rs:750`。
- `PostgresWorkflowGraphInstanceRepository` 读写 `activity_state_json`：`crates/agentdash-infrastructure/src/persistence/postgres/lifecycle_anchor_repository.rs:42`、`crates/agentdash-infrastructure/src/persistence/postgres/lifecycle_anchor_repository.rs:75`、`crates/agentdash-infrastructure/src/persistence/postgres/lifecycle_anchor_repository.rs:144`。
- `ActivityExecutionClaimRepository::create_or_get` 使用 idempotency key：`crates/agentdash-infrastructure/src/persistence/postgres/workflow_repository.rs:425`。
- `ActivityLifecycleRunService::apply_event` 加载 context、apply event、replace state、update graph instance/run：`crates/agentdash-application/src/workflow/activity_run.rs:48`。
- `ActivityLifecycleRunService::sync_run_projection` 从所有 graph instances 聚合 run status：`crates/agentdash-application/src/workflow/activity_run.rs:176`。
- `ActivityExecutorScheduler` claim ready attempts：`crates/agentdash-application/src/workflow/scheduler.rs:95`。
- `ActivityExecutorScheduler` executor started 后写 claim 与 ActivityEvent：`crates/agentdash-application/src/workflow/scheduler.rs:224`。
- `AgentActivityExecutorLauncher` 分派 Agent/Human/Function executor：`crates/agentdash-application/src/workflow/agent_executor.rs:721`。
- Function executor 创建 `ExecutorRunRef::FunctionRun`：`crates/agentdash-application/src/workflow/agent_executor.rs:930`。
- `start_lifecycle_run` 创建 run + root graph instance + activity state：`crates/agentdash-application/src/workflow/dispatch_service.rs:318`。
- graph dispatch 两段写 anchor：`crates/agentdash-application/src/workflow/dispatch_service.rs:419`、`crates/agentdash-application/src/workflow/dispatch_service.rs:450`。
- graphless dispatch 不创建 graph instance/assignment：`crates/agentdash-application/src/workflow/dispatch_service.rs:479`。
- 当前 view builder 从 graph instances / assignments / activity state 投影：`crates/agentdash-application/src/workflow/lifecycle_run_view_builder.rs:35`、`crates/agentdash-application/src/workflow/lifecycle_run_view_builder.rs:342`。
- `lifecycle_runs` 现有 schema：`crates/agentdash-infrastructure/migrations/0001_init.sql:282`。
- `lifecycle_workflow_instances.activity_state_json`：`crates/agentdash-infrastructure/migrations/0001_init.sql:305`。
- `activity_execution_claims` schema：`crates/agentdash-infrastructure/migrations/0001_init.sql:1`。
- `agent_assignments` schema：`crates/agentdash-infrastructure/migrations/0001_init.sql:15`。
- `runtime_session_execution_anchors` schema：`crates/agentdash-infrastructure/migrations/0001_init.sql:533`。
- root graph instance unique index：`crates/agentdash-infrastructure/migrations/0001_init.sql:1098`。
- active attempt claim unique index：`crates/agentdash-infrastructure/migrations/0001_init.sql:1198`。
- `0002` anchor FK migration：`crates/agentdash-infrastructure/migrations/0002_runtime_session_anchor_fks.sql:1`。
- migration readiness table list：`crates/agentdash-infrastructure/src/migration.rs:3`。
- current generated Activity DTO / LifecycleRunView：`crates/agentdash-contracts/src/workflow.rs:411`、`crates/agentdash-contracts/src/workflow.rs:789`、`crates/agentdash-contracts/src/workflow.rs:834`。
- frontend session command route helper：`packages/app-web/src/services/lifecycle.ts:27`。

## External References

- 未联网查询；外部行为参照使用任务内副本：`.trellis/tasks/06-06-dynamic-workflow-lifecycle-research/research/claude-dynamic-workflows-official-doc-zh-cn.md`、`.trellis/tasks/06-06-dynamic-workflow-lifecycle-research/research/claude-dynamic-workflows-article-zhihu-simpread.md`。
- 本轮直接使用的外部资料归纳来自 `.trellis/tasks/06-06-dynamic-workflow-lifecycle-research/research/claude-workflow-behavior-coverage.md`。

## Related Specs

- `.trellis/spec/backend/workflow/architecture.md`
- `.trellis/spec/backend/workflow/activity-lifecycle.md`
- `.trellis/spec/backend/session/runtime-execution-state.md`
- `.trellis/spec/backend/session/architecture.md`
- `.trellis/spec/backend/repository-pattern.md`
- `.trellis/spec/backend/database-guidelines.md`
- `.trellis/spec/frontend/workflow-activity-lifecycle.md`

## Caveats / Not Found

- `task.py current --source` 返回 no active task；本文件按用户显式给出的 `.trellis/tasks/06-06-dynamic-workflow-lifecycle-research` 路径写入，没有猜测其它目录。
- 当前代码没有 `OrchestrationInstance` / `OrchestrationPlanSnapshot` / `RuntimeNodeState` 等目标类型实现。
- 当前 migration 目录已有 `0001_init.sql` 与 `0002_runtime_session_anchor_fks.sql`；下一阶段普通实现应新增 `0003_*`，不应修改历史 migration。
- 没有发现 `lifecycle_orchestration_journal_entries` 或等价 orchestration journal 表。
- 没有发现 runtime session anchor 中的 `orchestration_id` / `runtime_node_id` / `node_path` 字段。
- 没有发现 `LifecycleRunView` 消费 `view_projection_json` 的路径；当前前端 projection 仍来自 graph instances、activity state、assignments 和 anchors。
- 本研究没有运行测试或 migration guard；这是规划文档，不是实现验证结果。
