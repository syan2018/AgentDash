# 研究：Common Orchestration Runtime 与仓储收敛计划

- 查询：为后续 `common-orchestration-runtime-static-graph` 规划 common runtime 与 repository convergence，复核当前 engine / scheduler / activity_run / agent_executor / lifecycle_run_view_builder / persistence 事实，并明确从 `WorkflowGraphInstance.activity_state` 迁移到 `OrchestrationInstance` runtime 的阶段、状态归属、风险和测试。
- 范围：源码、spec 与任务文档。
- 日期：2026-06-06

## 结论

### 已复核文件

| 路径 | 说明 |
| --- | --- |
| `.trellis/workflow.md` | Trellis 阶段与 research 落盘要求；当前请求属于 Phase 1.2 research 子代理工作。 |
| `.trellis/tasks/06-06-dynamic-workflow-lifecycle-research/prd.md` | 任务目标与 planning gate；要求本任务只保存 research / design / plan，不进入代码实现。 |
| `.trellis/tasks/06-06-dynamic-workflow-lifecycle-research/design.md` | 目标架构入口：静态 graph 与动态 script 都编译到 `OrchestrationPlanSnapshot`，由 common runtime 执行。 |
| `.trellis/tasks/06-06-dynamic-workflow-lifecycle-research/implement.md` | 后续子任务拆解；`common-orchestration-runtime-static-graph` 依赖 domain contract 与 graph compiler。 |
| `.trellis/tasks/06-06-dynamic-workflow-lifecycle-research/target-model-sketch.md` | 目标概念模型：`LifecycleRun.orchestrations[]`、`OrchestrationInstance`、journal / snapshot / trace anchor 的边界。 |
| `.trellis/tasks/06-06-dynamic-workflow-lifecycle-research/research/current-code-context.md` | 当前代码事实地图；本次源码复核的起点。 |
| `.trellis/tasks/06-06-dynamic-workflow-lifecycle-research/research/claude-workflow-behavior-coverage.md` | 行为压力测试矩阵；要求 runtime 支持 plan IR、journal/snapshot、typed execution、limits、progress projection。 |
| `.trellis/spec/backend/workflow/architecture.md` | 当前 workflow spec：仍声明 `WorkflowGraphInstance.activity_state` 是 Activity runtime 权威状态源。 |
| `.trellis/spec/backend/workflow/activity-lifecycle.md` | 当前 Activity lifecycle contract：scheduler claim、terminal callback、function immediate terminal event。 |
| `.trellis/spec/backend/session/runtime-execution-state.md` | session runtime command 与 terminal effect outbox 边界。 |
| `.trellis/spec/backend/session/architecture.md` | RuntimeSession 是 delivery / trace substrate，不拥有 Lifecycle progress 或 Agent effective surface。 |
| `.trellis/spec/backend/repository-pattern.md` | Repository port 应对应聚合边界；跨聚合一致性用显式 command port。 |
| `.trellis/spec/backend/database-guidelines.md` | 复杂值对象以 JSON 文本入库；普通任务新增 migration，不改历史 migration。 |
| `.trellis/spec/frontend/workflow-activity-lifecycle.md` | 前端运行观察当前以 `LifecycleRunView` / `WorkflowGraphInstanceView` / `ActivityStateView` 为主。 |
| `crates/agentdash-domain/src/workflow/entity.rs` | `WorkflowGraph`、`LifecycleRun`、`ActivityExecutionClaim` 当前领域实体。 |
| `crates/agentdash-domain/src/workflow/value_objects/activity_def.rs` | Activity definition/executor/transition/condition/artifact binding 当前闭包。 |
| `crates/agentdash-domain/src/workflow/value_objects/run_state.rs` | `ActivityLifecycleRunState`、`ActivityAttemptState`、`ExecutorRunRef`、claim status 当前结构。 |
| `crates/agentdash-domain/src/workflow/workflow_graph_instance.rs` | `WorkflowGraphInstance.activity_state` 持有并替换 Activity snapshot。 |
| `crates/agentdash-domain/src/workflow/repository.rs` | 当前 repository traits 把 run、graph instance、claim、assignment、anchor 拆成并列 port。 |
| `crates/agentdash-application/src/workflow/engine.rs` | `LifecycleEngine` 以 `ActivityEvent` 推进 in-memory activity state。 |
| `crates/agentdash-application/src/workflow/scheduler.rs` | Scheduler 扫描 Ready attempt，创建 durable claim，启动 executor，并把 start/immediate events 应用回 state。 |
| `crates/agentdash-application/src/workflow/activity_run.rs` | Activity service 加载 definition/run/graph instance/state，应用事件后整体替换 `activity_state`。 |
| `crates/agentdash-application/src/workflow/agent_executor.rs` | Agent / human / function executor 启动路径；function 已是一等 `FunctionRun`。 |
| `crates/agentdash-application/src/workflow/orchestrator.rs` | session terminal callback 与 `complete_lifecycle_node` 到 ActivityEvent 的桥。 |
| `crates/agentdash-application/src/workflow/session_association.rs` | runtime session 通过 anchor -> assignment -> run 反查 Activity attempt。 |
| `crates/agentdash-application/src/workflow/subject_execution_control.rs` | cancel 当前同时写 ActivityCancelled、abandon claim、release assignment、runtime cancel delivery。 |
| `crates/agentdash-application/src/workflow/lifecycle_run_view_builder.rs` | `LifecycleRunView` 当前从 graph instances、activity_state、assignments、anchors 投影。 |
| `crates/agentdash-infrastructure/src/persistence/postgres/workflow_repository.rs` | `lifecycle_runs` 与 `activity_execution_claims` PostgreSQL repository。 |
| `crates/agentdash-infrastructure/src/persistence/postgres/lifecycle_anchor_repository.rs` | `lifecycle_workflow_instances`、`agent_assignments`、`runtime_session_execution_anchors` repository。 |
| `crates/agentdash-infrastructure/migrations/0001_init.sql` | 当前运行态表结构 baseline。 |
| `crates/agentdash-infrastructure/migrations/0002_runtime_session_anchor_fks.sql` | runtime session anchor 外键补充 migration。 |
| `packages/app-web/src/stores/lifecycleStore.ts` | 前端 lifecycle store 当前按 run、graph instance、agent、frame、runtime trace 归一化。 |
| `crates/agentdash-contracts/src/workflow.rs` | 当前 TS/Rust contract：`LifecycleRunView` 暴露 `workflow_graph_instances` 与 `active_activity_refs`。 |

### 当前 Runtime 事实

当前静态定义不是普通 DAG。`WorkflowGraph` 已包含 `entry_activity_key`、`activities`、`transitions`（`crates/agentdash-domain/src/workflow/entity.rs:73`），`ActivityDefinition` 已有 executor、input/output ports、completion、iteration、join policy（`crates/agentdash-domain/src/workflow/value_objects/activity_def.rs:7`），executor 覆盖 Agent / Function / Human（`crates/agentdash-domain/src/workflow/value_objects/activity_def.rs:26`），function 又覆盖 API request / bash exec（`crates/agentdash-domain/src/workflow/value_objects/activity_def.rs:95`），transition 覆盖 condition、artifact binding、max traversal（`crates/agentdash-domain/src/workflow/value_objects/activity_def.rs:186`）。因此 common runtime IR 的静态 graph 子集至少要表达 executor identity、completion、attempt、condition、artifact exchange、join/iteration。

当前 `LifecycleRun` 只是控制 ledger：字段只有 topology、root graph、status、execution_log、时间戳（`crates/agentdash-domain/src/workflow/entity.rs:203`），`sync_graph_instance_activity_projections` 从各 graph instance 的 activity state 聚合 run status（`crates/agentdash-domain/src/workflow/entity.rs:249`）。真正的运行节点状态在 `WorkflowGraphInstance.activity_state`（`crates/agentdash-domain/src/workflow/workflow_graph_instance.rs:12`），`replace_activity_state` 整体替换 snapshot 并用 state status 推导 graph instance status（`crates/agentdash-domain/src/workflow/workflow_graph_instance.rs:57`）。

当前 `ActivityLifecycleRunState` 是 Activity runtime 主要状态容器，保存 attempts、outputs、inputs（`crates/agentdash-domain/src/workflow/value_objects/run_state.rs:70`）。`ActivityAttemptState` 保存 activity_key、attempt、status、executor_run、started/completed、summary（`crates/agentdash-domain/src/workflow/value_objects/run_state.rs:25`）。`ExecutorRunRef` 已区分 `RuntimeSession`、`FunctionRun`、`HumanDecision`（`crates/agentdash-domain/src/workflow/value_objects/run_state.rs:94`）。

`LifecycleEngine` 是纯状态推进器：`initialize` materialize entry ready 与其他 pending attempts（`crates/agentdash-application/src/workflow/engine.rs:116`），`apply_event` 处理 claim/start/completed/failed/cancelled/human decision（`crates/agentdash-application/src/workflow/engine.rs:164`），completion 后写 outputs 并推进后继（`crates/agentdash-application/src/workflow/engine.rs:275`），后继根据 transitions 与 conditions 激活（`crates/agentdash-application/src/workflow/engine.rs:357`）。它没有 append-only journal；事件只在调用栈中存在，最终写回 snapshot。

`ActivityLifecycleRunService` 每次推进都会加载 graph instance、run、definition、activity_state（`crates/agentdash-application/src/workflow/activity_run.rs:104`），调用 engine 后整体替换 `activity_state`（`crates/agentdash-application/src/workflow/activity_run.rs:48`），再同步 run projection 并更新 graph instance/run（`crates/agentdash-application/src/workflow/activity_run.rs:57`）。这说明当前 service 是 “snapshot rewrite + projection sync”，不是 durable journal runtime。

`ActivityExecutorScheduler` 扫描 Ready attempts（`crates/agentdash-application/src/workflow/scheduler.rs:101`），用 `ActivityExecutionClaim::new` 生成 idempotency key（`crates/agentdash-domain/src/workflow/entity.rs:107`），`create_or_get` durable claim 后把 attempt 标记为 Claiming（`crates/agentdash-application/src/workflow/scheduler.rs:117`），executor 启动后写 claim.running + `ExecutorStarted`（`crates/agentdash-application/src/workflow/scheduler.rs:224`）。Function executor 返回 `immediate_events` 并在同一 scheduler pass 内应用（`crates/agentdash-application/src/workflow/scheduler.rs:191`）。

Agent executor 已经提供可复用的执行身份链。Agent activity 创建 assignment、agent、frame（`crates/agentdash-application/src/workflow/agent_executor.rs:381`），创建 runtime session（`crates/agentdash-application/src/workflow/agent_executor.rs:434`），启动 workflow prompt 后返回 `ExecutorRunRef::RuntimeSession` 与 assignment（`crates/agentdash-application/src/workflow/agent_executor.rs:770`）。ContinueRoot 复用当前/root runtime session 并禁止多个 running ContinueRoot 并行（`crates/agentdash-application/src/workflow/agent_executor.rs:838`）。Human executor 返回 `ExecutorRunRef::HumanDecision`（`crates/agentdash-application/src/workflow/agent_executor.rs:753`）。Function executor 生成 `ExecutorRunRef::FunctionRun` 并把 API/bash outcome 映射为 ActivityCompleted/ActivityFailed（`crates/agentdash-application/src/workflow/agent_executor.rs:930`）。

Function side effect 边界在 SPI/infrastructure。`FunctionRunner` 只暴露 `run_api_request` 与 `run_bash`（`crates/agentdash-spi/src/platform/function_runner.rs:36`），application 拥有 event shaping 和 success/failure policy。默认实现执行 reqwest HTTP 与 tokio process（`crates/agentdash-infrastructure/src/function_runner.rs:25`）。这意味着 common runtime 不能把非 Agent effect 藏进 AgentRun；必须保留 `FunctionRun` 或泛化 `EffectTraceRef`。

Terminal callback 现在通过 session terminal effect outbox 调用 workflow orchestrator。terminal side effect 会被 enqueue 为 `SessionTerminalCallback`（`crates/agentdash-application/src/session/terminal_effects.rs:158`），replay 时调用 callback（`crates/agentdash-application/src/session/terminal_effects.rs:339`）。`LifecycleOrchestrator` 通过 `resolve_activity_session_association` 反查 Activity attempt，再 `apply_event` + launch successors（`crates/agentdash-application/src/workflow/orchestrator.rs:134`）。`complete_lifecycle_node` 工具也走同一个 orchestrator（`crates/agentdash-application/src/workflow/tools/advance_node.rs:76`）。

当前 runtime session 反查是 Activity-specific。`RuntimeSessionExecutionAnchor` 保存 run、launch_frame、agent、assignment、graph_instance、activity_key、attempt（`crates/agentdash-domain/src/workflow/runtime_session_anchor.rs:28`）。`session_association` 要求 anchor 回到 `AgentAssignment`，再返回 graph_instance_id/activity_key/attempt（`crates/agentdash-application/src/workflow/session_association.rs:178`、`crates/agentdash-application/src/workflow/session_association.rs:360`）。这条路径要在 common runtime 中泛化为 `runtime_session_id -> lifecycle_run_id / orchestration_id / node_path / agent_run_id / frame_id`。

`LifecycleRunView` 当前从 graph instances 和 activity state 投影：builder 先加载 agents/assignments/graph instances/anchors（`crates/agentdash-application/src/workflow/lifecycle_run_view_builder.rs:35`），再把 `WorkflowGraphInstance.activity_state` 映射成 `WorkflowGraphInstanceView.activities`（`crates/agentdash-application/src/workflow/lifecycle_run_view_builder.rs:342`）并从 activity states 生成 `active_activity_refs`（`crates/agentdash-application/src/workflow/lifecycle_run_view_builder.rs:363`）。contract 也只暴露 `workflow_graph_instances`、`active_activity_refs`、agents、subject_associations、runtime_trace_refs（`crates/agentdash-contracts/src/workflow.rs:834`）。前端 store 按这些 view 归一化（`packages/app-web/src/stores/lifecycleStore.ts:31`）。

当前 persistence 已经分散运行态：`lifecycle_runs` 只含 status/execution_log（`crates/agentdash-infrastructure/migrations/0001_init.sql:282`），`lifecycle_workflow_instances` 含 `activity_state_json`（`crates/agentdash-infrastructure/migrations/0001_init.sql:305`），`activity_execution_claims` 是 durable claim/lease（`crates/agentdash-infrastructure/migrations/0001_init.sql:1`），`agent_assignments` 是 attempt -> agent/frame 绑定（`crates/agentdash-infrastructure/migrations/0001_init.sql:15`），`runtime_session_execution_anchors` 是 session 反查索引（`crates/agentdash-infrastructure/migrations/0001_init.sql:533`）。Postgres repository 也按这些表和 traits 拆开（`crates/agentdash-domain/src/workflow/repository.rs:87`、`crates/agentdash-domain/src/workflow/repository.rs:103`、`crates/agentdash-domain/src/workflow/repository.rs:137`、`crates/agentdash-domain/src/workflow/repository.rs:188`）。

### 收敛后的状态归属

| State / fact | Target placement | Reason |
| --- | --- | --- |
| Lifecycle subject/project/status/main agent/current agent summaries | `LifecycleRun.context` in aggregate snapshot | 生命周期上下文属于 owning run，读取 Lifecycle progress 时总是需要；不应从 runtime session 推断。 |
| AgentRun refs / AgentFrame refs / current frame ids | `LifecycleRun.context.agent_runs[]` with frame refs; full frame surface remains frame store | Agent execution identity 属于 Lifecycle；frame surface 可能较大且已有独立 revision store。 |
| Permission scope / budget summary | `LifecycleRun.context` plus per-orchestration counters | 权限和预算跨 orchestration 生效，但执行消耗要能按 instance/node 归集。 |
| `OrchestrationInstance` identity/status/source_ref/role | `LifecycleRun.orchestrations[]` | Orchestration 是 Lifecycle 内部状态容器，不是平级 run。 |
| Immutable compiled plan for this run | `orchestrations[].plan_snapshot` initially; split only if plan becomes cold/large or shared cache | 审计复现优先；静态 graph 和 script 都需要相同 runtime input。 |
| Plan activation args/cursor/limits/ready roots | `orchestrations[].activation` | 属于 plan 在本 instance 内的 materialized runtime state。 |
| Runtime node tree/status/attempts/inputs/outputs/executor refs/children/phase_path/error/trace refs | `orchestrations[].state_exchange_snapshot.node_tree` or `orchestrations[].node_tree` | 这是替代 `ActivityLifecycleRunState.attempts` 的核心事实源。 |
| Artifact exchange materialization / variable snapshot / output summaries | `StateExchangeSnapshot` inside orchestration snapshot | 当前 `inputs` / `outputs` 应升级为通用 state exchange，供 resume、projection、function template context 使用。 |
| Ready queue summary / dispatch watermarks | `orchestrations[].dispatch` summary | 只保存可恢复调度摘要；具体 worker claim 不应变成业务事实。 |
| Node dispatch claim / worker lease / expires_at / idempotency | Lease/outbox store, or embedded `dispatch.leases` only while single-row locking suffices | Lease 是并发控制，不是业务状态；如果多 worker claim 需要独立表，表也应只存 lease。 |
| Append-only runtime facts | `lifecycle_orchestration_journal_entries(lifecycle_run_id, orchestration_id, seq, event_kind, event)` when resume/replay/incremental subscription is needed | journal 支持 resume、audit、cache 和进度增量；不应塞入 `session_events`，因为坐标不是 conversation turn。 |
| Current journal cursor / materialized seq | `orchestrations[].journal_cursor` and `LifecycleRun.seq/version` | snapshot 必须知道已 materialize 到哪一条 journal fact。 |
| RuntimeSession -> node/agent/frame reverse lookup | `runtime_trace_anchors` / evolved `runtime_session_execution_anchors` as trace index | 这是反查热路径和 evidence，不是 runtime state 第二事实源。 |
| Function/API/bash/local effect call result refs | Node state + journal fact; split `runtime_effect_traces` only when external call id / stream / retention 需要 | FunctionRun 是一等 execution identity，但默认不需要独立 aggregate。 |
| LifecycleRunView / active refs / graph-compatible DTO | `lifecycle_runs.view_projection` or rebuilt projection; never command input | UI 需要稳定 read model，但 projection 不能成为第二事实源。 |
| Historical `WorkflowGraphInstance.activity_state_json` | Migration source and temporary compatibility projection only | 旧 snapshot 绑定静态 graph attempt，不适合作为 common runtime truth。 |
| Historical `ActivityExecutionClaim` | Lease adapter / migration source, not node truth | Claim 只表达调度占用；node status 归 snapshot/journal。 |
| Historical `AgentAssignment` | AgentInvocation projection / trace binding source, not owning runtime state | 目标 runtime node 绑定 agent/frame，assignment 可降级为投影或被 trace anchor 替代。 |
| `LifecycleRun.execution_log` | Summary projection or human-readable audit summary | 不够表达恢复事实，不应作为 orchestration journal。 |

### 从 `WorkflowGraphInstance.activity_state` 迁移的阶段计划

#### Phase A：静态 graph runtime 合同预备

前置条件：`OrchestrationPlanSnapshot` / `RuntimeNodeState` / `OrchestrationInstance` domain contract 和 `WorkflowGraph -> OrchestrationPlanSnapshot` compiler 必须先存在。若前置条件不满足，不要直接编辑旧 engine 来启动 common runtime。

工作内容：

- Add a small `OrchestrationRuntime` application module that consumes `OrchestrationPlanSnapshot`, not `WorkflowGraph`.
- Define `OrchestrationEvent` equivalents for current Activity events: `NodeClaimAccepted`, `NodeStartFailed`, `NodeStarted`, `NodeCompleted`, `NodeFailed`, `NodeCancelled`, `HumanDecisionSubmitted`, plus `PlanActivated`.
- Implement pure materialization from event -> snapshot first, mirroring `LifecycleEngine` behavior but using `RuntimeNodeState`.
- Keep graph compiler fixtures as runtime fixtures; every old Activity behavior must enter through plan nodes.

退出标准：

- A root static graph can initialize `OrchestrationInstance(role=root)` with entry ready node.
- Activity-specific `ActivityLifecycleRunState` is no longer needed inside the new runtime core.

#### Phase B：短期迁移观测才允许 dual-write

Because the project is not live, avoid long compatibility layers. During one implementation slice, short-lived dual-write is acceptable only to prove parity:

- Runtime writes authoritative state to `LifecycleRun.orchestrations[]`.
- A graph-compatible projection is generated from `RuntimeNodeState` into existing `WorkflowGraphInstanceView` / `ActiveActivityRefDto` for UI.
- If `lifecycle_workflow_instances.activity_state_json` must still be populated for old builder/tests during this phase, mark it as generated projection and remove writes in the same or next child task.

Do not allow new scheduler/terminal paths to read both old and new state with fallback logic. That would create two facts.

#### Phase C：Scheduler 收敛

- Replace `ActivityExecutorScheduler` input from `ActivityLifecycleRunState` to `OrchestrationInstance` snapshot.
- Claim ready `RuntimeNodeState` by `orchestration_id + node_path + attempt`, not `graph_instance_id + activity_key + attempt`.
- Keep a lease/outbox boundary equivalent to `ActivityExecutionClaim`, but make it operational: `NodeDispatchLease` can reuse table patterns from `activity_execution_claims` while node truth stays in snapshot/journal.
- Map executor start result back to `RuntimeNodeState.executor_run_ref` and journal facts.
- Preserve retryable start failure semantics: current engine returns retryable start failure to Ready, non-retryable to Failed (`crates/agentdash-application/src/workflow/engine.rs:178`).

#### Phase D：Executor 收敛

- Introduce executor launch input shaped by `PlanNode.executor`, then adapt existing `AgentActivityExecutorLauncher` logic.
- Agent nodes should still create/reuse Lifecycle agent/frame/runtime session through existing port logic, but write `AgentInvocation` under runtime node and `RuntimeTraceAnchor` with `orchestration_id/node_path`.
- Function nodes should still execute through `FunctionRunner`, return `FunctionRun` / effect refs, and produce terminal node events immediately.
- Human nodes should become `human_gate` node states; existing `HumanDecision` ref can be retained as executor ref.

#### Phase E：Terminal / command resolver 收敛

- Replace `resolve_activity_session_association` with a runtime-node resolver:

```text
runtime_session_id
  -> RuntimeTraceAnchor
  -> lifecycle_run_id / orchestration_id / node_path / agent_run_id / frame_id
  -> RuntimeNodeState terminal event
```

- `complete_lifecycle_node` should complete the current runtime node, not an Activity attempt. For static graph projection it can still return activity-compatible active refs.
- Session terminal callback must remain outbox-driven: session terminal fact is durable before workflow callback, and callback failure must not roll back session terminal.
- Terminal event application must be idempotent by journal seq or node terminal status; duplicate tool completion + later session terminal should not advance successors twice.

#### Phase F：View / repository 收敛

- Make `LifecycleRunView` builder read from `LifecycleRun.orchestrations[]` / `view_projection`.
- Generate current `workflow_graph_instances` and `active_activity_refs` from root static graph orchestration until frontend has native orchestration tree UI.
- Demote `WorkflowGraphInstanceRepository` to definition-instance projection/migration source, then remove `activity_state_json` from write path.
- Demote `AgentAssignmentRepository` to active agent invocation projection or replace terminal resolver with trace anchor fields.
- Keep `runtime_session_execution_anchors` or renamed `runtime_trace_anchors` as a narrow index, adding `orchestration_id` and `node_path`.
- Add migrations as new files per database spec; do not edit `0001_init.sql` in a normal implementation task.

### 风险点

#### UI Projection

The existing contract requires `LifecycleRunView.workflow_graph_instances[]` and `active_activity_refs[]` (`crates/agentdash-contracts/src/workflow.rs:834`), and frontend store ingests graph instances from the run view (`packages/app-web/src/stores/lifecycleStore.ts:129`). If runtime switches to orchestration snapshot without a graph-compatible projection, current UI loses active node display and graph instance indexing.

缓解策略：

- First release should project static graph orchestration into the old view fields.
- Projection must be read-only; commands should use runtime node refs or session commands, not `ActivityAttemptView`.
- Introduce native orchestration progress fields only after the old projection is stable.

#### Terminal Callback

Current callback path assumes runtime session maps to `AgentAssignment` and Activity attempt (`crates/agentdash-application/src/workflow/session_association.rs:178`). It also runs from terminal effect outbox after terminal event persistence (`crates/agentdash-application/src/session/terminal_effects.rs:158`). New runtime must preserve the outbox boundary but replace the resolver.

风险：

- Duplicate advancement if `complete_lifecycle_node` completes a node and later session terminal callback also completes it.
- Lost terminal advancement if trace anchor lacks node_path.
- Callback failure creating dead-letter effect while orchestration state remains running.

缓解策略：

- Add idempotent terminal fact application keyed by `(lifecycle_run_id, orchestration_id, node_path, attempt, terminal_source)`.
- Anchor must be written before runtime session launch is accepted.
- Dead-letter callback should be visible in projection and retryable without replaying session terminal fact.

#### Function Executor / Local Effects

Function executor currently returns `FunctionRun` and immediate terminal event (`crates/agentdash-application/src/workflow/agent_executor.rs:910`), and `FunctionRunner` owns raw API/bash side effects (`crates/agentdash-spi/src/platform/function_runner.rs:36`). New runtime must not treat function node as an AgentRun-less oddity.

风险：

- Immediate function completion bypasses journal or node start state.
- Bash/API permission and workspace root are not modeled in plan/runtime.
- Large stdout/stderr/API body bloats `LifecycleRun.orchestrations[]`.

缓解策略：

- Always append `NodeStarted` before `EffectCompleted/EffectFailed`, even for synchronous functions.
- Model function/local effect as typed `ExecutorSpec` with capability key and audit refs.
- Store large effect payloads as refs when they exceed a small threshold; node state keeps summary/result ref.

#### Cancel / Pause / Retry

Cancel currently spans ActivityEvent, claim update, assignment release, and runtime delivery command (`crates/agentdash-application/src/workflow/subject_execution_control.rs:83`、`crates/agentdash-application/src/workflow/subject_execution_control.rs:249`). Pause/resume does not exist as orchestration command in current code; `LifecycleGateService` is durable wait/resume for gates, not whole workflow pause (`crates/agentdash-application/src/workflow/lifecycle_gate_service.rs:1`). Retry exists only as scheduler start failure semantics and attempt policy, not as user-facing node retry.

风险：

- Partial cancel leaves node cancelled but runtime session still running, or runtime cancelled but node still running.
- Pause that only stops scheduler does not decide what happens to active AgentRun/function nodes.
- Retry may reuse stale outputs/cache or violate `max_attempts`.

缓解策略：

- Write control commands as orchestration journal facts: `PauseRequested`, `ResumeRequested`, `CancelRequested`, `RetryRequested`.
- Scheduler must refuse new claims when instance status is paused/cancelling.
- Active node cancel should produce per-node cancellation intents and runtime delivery commands.
- Retry should create a new node attempt, invalidate or explicitly reuse cache based on plan policy, and keep old trace refs.

### 最小测试计划

1. Domain snapshot roundtrip
   - `LifecycleRun` persists 0, 1, and multiple `OrchestrationInstance` entries.
   - `OrchestrationInstance` roundtrips plan snapshot, activation, node tree, dispatch summary, cache refs, journal cursor.

2. Static graph runtime unit tests
   - Entry activity materializes as Ready runtime node.
   - Agent node claim -> started writes executor ref and trace anchor payload.
   - Function node claim -> started -> completed/failed happens in one pass but still records both node start and terminal facts.
   - Human node becomes waiting/blocked gate and completes from decision event.
   - Condition false does not activate successor.
   - Artifact binding copies structured output to successor input snapshot.
   - Retryable start failure returns node attempt to Ready; non-retryable marks Failed.
   - Duplicate terminal event is idempotent and does not launch successors twice.

3. Repository integration tests
   - `LifecycleRunRepository` saves/loads `context`, `orchestrations`, `view_projection`, `seq/version`.
   - Journal append/list by `(lifecycle_run_id, orchestration_id, seq)` preserves order.
   - Lease create/update cannot change node status without runtime event application.
   - Runtime trace anchor lookup by session returns lifecycle/orchestration/node/agent/frame refs.
   - Migration guard passes after adding new migration.

4. Application e2e with fake launchers
   - Static graph with two agent activities advances through common runtime.
   - Static graph with function activity completes immediately and activates successor.
   - `complete_lifecycle_node` completes current runtime node and returns active projection.
   - Session terminal callback advances a node through new resolver.
   - Cancel subject execution writes orchestration cancel state and produces runtime cancel delivery.

5. Projection / frontend contract tests
   - `LifecycleRunView` generated from orchestration snapshot still fills `workflow_graph_instances` and `active_activity_refs`.
   - `lifecycleStore.ingestLifecycleRun` continues to normalize run/graph/agent/frame data.
   - When native orchestration progress fields are added, mapper rejects missing required fields and unknown enum values at boundary.

### 实现代理源码 / Spec 复核索引

实现前应重新打开：

- `.trellis/tasks/06-06-dynamic-workflow-lifecycle-research/design.md`
- `.trellis/tasks/06-06-dynamic-workflow-lifecycle-research/implement.md`
- `.trellis/tasks/06-06-dynamic-workflow-lifecycle-research/target-model-sketch.md`
- `.trellis/tasks/06-06-dynamic-workflow-lifecycle-research/research/workflow-graph-compiler-plan.md`
- `.trellis/tasks/06-06-dynamic-workflow-lifecycle-research/research/orchestration-domain-contract-plan.md` if present in the task context for the implementation run
- `.trellis/spec/backend/workflow/architecture.md`
- `.trellis/spec/backend/workflow/activity-lifecycle.md`
- `.trellis/spec/backend/session/runtime-execution-state.md`
- `.trellis/spec/backend/session/architecture.md`
- `.trellis/spec/backend/repository-pattern.md`
- `.trellis/spec/backend/database-guidelines.md`
- `.trellis/spec/frontend/workflow-activity-lifecycle.md`
- `crates/agentdash-domain/src/workflow/entity.rs`
- `crates/agentdash-domain/src/workflow/value_objects/activity_def.rs`
- `crates/agentdash-domain/src/workflow/value_objects/run_state.rs`
- `crates/agentdash-domain/src/workflow/workflow_graph_instance.rs`
- `crates/agentdash-domain/src/workflow/repository.rs`
- `crates/agentdash-application/src/workflow/engine.rs`
- `crates/agentdash-application/src/workflow/scheduler.rs`
- `crates/agentdash-application/src/workflow/activity_run.rs`
- `crates/agentdash-application/src/workflow/agent_executor.rs`
- `crates/agentdash-application/src/workflow/orchestrator.rs`
- `crates/agentdash-application/src/workflow/session_association.rs`
- `crates/agentdash-application/src/workflow/subject_execution_control.rs`
- `crates/agentdash-application/src/workflow/lifecycle_run_view_builder.rs`
- `crates/agentdash-infrastructure/src/persistence/postgres/workflow_repository.rs`
- `crates/agentdash-infrastructure/src/persistence/postgres/lifecycle_anchor_repository.rs`
- `crates/agentdash-infrastructure/migrations/`
- `crates/agentdash-contracts/src/workflow.rs`
- `packages/app-web/src/stores/lifecycleStore.ts`
- `packages/app-web/src/services/lifecycle.ts`

### 相关 Specs

- `.trellis/spec/backend/workflow/architecture.md`
- `.trellis/spec/backend/workflow/activity-lifecycle.md`
- `.trellis/spec/backend/workflow/lifecycle-edge.md`
- `.trellis/spec/backend/workflow/lifecycle-run-link.md`
- `.trellis/spec/backend/session/architecture.md`
- `.trellis/spec/backend/session/runtime-execution-state.md`
- `.trellis/spec/backend/repository-pattern.md`
- `.trellis/spec/backend/database-guidelines.md`
- `.trellis/spec/frontend/workflow-activity-lifecycle.md`

### 外部资料

- `.trellis/tasks/06-06-dynamic-workflow-lifecycle-research/research/claude-dynamic-workflows-official-doc-zh-cn.md` — 用户贴入的 Claude Code Dynamic Workflows 官方文档中文页文本副本。
- `.trellis/tasks/06-06-dynamic-workflow-lifecycle-research/research/claude-dynamic-workflows-article-zhihu-simpread.md` — 用户贴入的中文调研文章副本。
- `.trellis/tasks/06-06-dynamic-workflow-lifecycle-research/research/claude-workflow-behavior-coverage.md` — 基于上述两份资料抽象出的行为覆盖矩阵。

本研究未执行实时联网复核，刻意以任务内保存的资料副本作为行为基准。

## 注意事项 / 未发现

- `python ./.trellis/scripts/task.py current --source` returned no active task in this Codex session, but the user prompt explicitly provided the task path and output path; this file was written only under that requested task's `research/` directory.
- `OrchestrationInstance` / `OrchestrationPlanSnapshot` / `RuntimeNodeState` / `StateExchangeSnapshot` 已在 `orchestration-domain-contract` 任务中落入代码；后续 common runtime 实现前必须重新读取最终代码合同，并先处理 compiler 计划提出的 plan digest 修正。
- Current specs still state `WorkflowGraphInstance.activity_state` is authoritative Activity runtime state. That is a current-state contract and must be updated during implementation; it is not compatible with the target common runtime truth model.
- Pause/resume/retry are not yet first-class orchestration commands in current workflow code. Existing durable gate wait/resume and scheduler retryable start failure are related mechanisms but not whole-instance pause/resume or user-facing node retry.
- This research did not run tests and did not inspect generated TypeScript output beyond source contracts/store files.
