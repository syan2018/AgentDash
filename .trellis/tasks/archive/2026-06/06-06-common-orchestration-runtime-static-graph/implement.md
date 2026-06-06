# Common Orchestration Runtime 正式接入实施计划

## 状态

已进入实现阶段。本任务依赖 `workflow-graph-compiler` 的 plan 输出稳定；后续实现应围绕唯一 runtime path 收敛：`LifecycleRun.orchestrations[] -> OrchestrationInstance -> RuntimeNodeState`。

当前实现进展：

- 静态 `WorkflowGraph` dispatch / lifecycle start 已先编译为 `OrchestrationPlanSnapshot`，并直接写入 `LifecycleRun.orchestrations[]`。
- 主 runtime 坐标已切到 `orchestration_id + node_path + attempt`；`RuntimeSessionExecutionAnchor` 持久化该坐标，session terminal / subject cancel / complete node 通过 anchor 回到 runtime node。
- `RepositorySet`、API bootstrap、PostgreSQL repository implementation 已停止构造和注入 Activity-specific runtime repository。
- domain repository contract 已移除 Activity-specific runtime repository trait；application 模块树已删除旧 `activity_run` / `scheduler` / `agent_executor` 路径。
- lifecycle VFS mount metadata、artifact scope、session assembly 和 frame construction 已转为 `orchestration_id + node_path + attempt`。`LifecycleMountSurface` 作为 session/VFS 的窄接口，provider 从 `LifecycleRun.orchestrations[]` 定位 `OrchestrationInstance` 与 `RuntimeNodeState`，不再读取 graph instance 仓储。
- 新增 migration `0004_orchestration_runtime_convergence.sql` 将 anchor / routine dispatch schema 切到 orchestration node 坐标，并 drop 旧 activity claim / assignment / workflow instance 表。
- dispatch、subject cancel、boot task projection 测试已经改为断言 `LifecycleRun.orchestrations[]`，不再通过旧 graph instance / assignment mock 证明行为。

当前已收口面：

- `LifecycleRunView` 公开 `orchestrations[]`、`RuntimeNodeView` 与 `active_runtime_node_refs[]`；API / frontend / VFS / hook projection 从 orchestration snapshot 读取。
- `workflow/lifecycle/journey` 的 progress helper 已改为 runtime node helper。
- `workflow/orchestration/runtime.rs` 已完成 reducer 第一切片：NodeStarted / NodeCompleted / NodeFailed / NodeCancelled 可更新 runtime node、trace refs、state exchange、transition successor 和 terminal idempotency；`complete_lifecycle_node` / session terminal callback 已接到 reducer。
- `workflow/orchestration/executor_launcher.rs` 已接入 semantic executor launcher：drain `LifecycleRun.orchestrations[].dispatch.ready_node_ids`，按 `PlanNodeKind` 启动 AgentCall / Function / LocalEffect / HumanGate，并只通过 runtime event 写回 node state。
- Function API 与 BashExec/local effect 通过 `FunctionRunner` SPI 执行；同步完成也保持 `NodeStarted -> NodeCompleted/NodeFailed` 的事件顺序。
- HumanGate 会创建 `LifecycleGate(gate_kind=orchestration_human_gate)` 并记录 `ExecutorRunRef::HumanDecision`；`POST /lifecycle-runs/{id}/orchestration-human-decisions` 负责提交 decision、完成 node 并继续 drain 后继节点。
- `ActivityActivation` 中 phase hot-update applier 当前未被新主路径调用；后续应结合 orchestration executor adapter 决定保留为 live capability transition，还是并入统一 executor surface。

当前明确边界：

- AgentCall 当前正式支持 `CreateActivityAgent + CreateNew`；`ContinueCurrentAgent + DeliverToCurrentTrace` 需要 connector delivery surface，执行器将其 materialize 为 `Blocked` runtime node。
- `ExecutorSpec::Function(ApiRequest/BashExec)` 是当前 Function / LocalEffect 执行面；`ExecutorSpec::LocalEffect(capability_key, input)` 先记录 started，再以带 orchestration node coordinate 的 failed node 表达未接入 capability executor。
- attempt policy 当前支持单次 attempt；多 attempt、unbounded attempt 与非 latest artifact alias 在副作用前阻塞。
- `max_traversals` 由 reducer 在 successor activation 时阻塞，orchestration status 聚合为 Paused，LifecycleRun status 聚合为 Blocked。

## 最终收口批次

本批次执行旧运行态结构硬删除：

- 删除 domain 中 Activity-specific runtime entity / value object / module export。
- 删除 application 旧 engine 与 graph-instance helper，projection、journey、mount、task/story projection 统一使用 `RuntimeNodeStatus` / `RuntimeNodeState`。
- 删除 API / contracts / frontend 中 graph instance、active activity、assignment DTO 与 route surface；contracts 重新生成。
- PostgreSQL anchor repository 不再读写 graph/activity binding columns；当前 schema 由 `0004_orchestration_runtime_convergence.sql` drop 旧列和旧表。
- 规格文档同步为 common orchestration runtime 当前合同。

## 当前实现切片

本轮先做 `runtime reducer + terminal bridge`，不同时展开 executor launcher：

1. 在 `workflow/orchestration/runtime.rs` 实现 node event reducer。
   - `NodeStarted` 写 node status、started_at、executor_run_ref、trace refs。
   - `NodeCompleted` 写 outputs、completed_at，并 materialize state exchange。
   - `NodeFailed` / `NodeCancelled` 写 terminal status、error/reason。
   - terminal event 必须幂等，重复完成不能重复激活后继。
2. `NodeCompleted` 从 `state_exchange_rules` 复制完成节点 outputs 到 successor inputs / `StateExchangeSnapshot.node_outputs`。
3. reducer 根据 `ActivationRule::Transition` 激活 successor ready nodes，更新 `dispatch.ready_node_ids`；暂未完整支持的 condition / join / max_traversals 必须保守阻断或显式报错，不静默当成成功。
4. `LifecycleOrchestrator` 的 terminal materialization 改为调用 reducer。`complete_lifecycle_node` 若能稳定读取 lifecycle scoped artifacts，则传入 output port values；否则本切片先保留 reducer 参数能力，下一切片补 port output collection。
5. 本切片不做 AgentCall / Function / LocalEffect / HumanGate 启动器，不新增 lease/outbox 表，不改 frontend/API。

本切片已完成。当前 reducer 支持简单 transition、condition、All/Any/First/NOfM join、state exchange materialization、duplicate terminal idempotency；`max_traversals` 暂以 blocking diagnostic 将目标 node 标记为 Blocked，后续结合 attempt/traversal policy 一并实现。

## 当前实现切片：AgentCall started materialization

本轮只处理 graph-backed dispatch 的 entry AgentCall 已启动事实，不同时展开完整 scheduler：

1. `dispatch_common` 在创建 runtime session、AgentFrame、RuntimeSessionExecutionAnchor 后，必须向 reducer 提交 `NodeStarted`。
2. `NodeStarted.executor_run_ref` 使用 `ExecutorRunRef::RuntimeSession { session_id }`，由 reducer 同步写入 `RuntimeNodeState.executor_run_ref` 和 `trace_refs`。
3. 更新后的 `LifecycleRun` 必须持久化，返回给上层的 `DispatchFacts.run` 也应是 node 已 `Running` 的版本。
4. `start_lifecycle_run` 只初始化 orchestration，不创建 runtime session，因此 entry node 仍保持 `Ready`。
5. 本切片不做 Function / LocalEffect / HumanGate launcher，不新增 lease/outbox 表，不改 frontend/API。

## 当前实现切片：Semantic executor launcher

本轮完成 common runtime 的副作用启动器：

1. `OrchestrationExecutorLauncher::drain_ready_nodes(run_id)` 读取 `LifecycleRun.orchestrations[].dispatch.ready_node_ids`，一次只处理当前 ready queue 的第一个 runtime node，并在每次事件写回后重新加载 aggregate。
2. `AgentCall` 读取 semantic `ExecutorSpec::AgentProcedure`：
   - `CreateActivityAgent + CreateNew` 创建 run-scoped `LifecycleAgent`、`AgentFrame`、`RuntimeSession` 与 `RuntimeSessionExecutionAnchor`，随后提交 `NodeStarted(ExecutorRunRef::RuntimeSession)`。
   - 需要继续当前 trace 的策略先写 `NodeBlocked`，原因是该策略需要真实 connector delivery surface 才能表示“已投递”。
3. `Function` 与 compiler 产生的 BashExec `LocalEffect` 通过 `FunctionRunner` SPI 执行，输出映射到 declared output ports；HTTP 非 2xx、bash 非 0 exit、template/transport/process 错误都通过 `NodeFailed(RuntimeNodeError)` 写回。
4. `HumanGate` 创建 lifecycle gate 并提交 `NodeStarted(ExecutorRunRef::HumanDecision)`；decision route 完成 gate、提交 `NodeCompleted`，再 drain 后继 ready nodes。
5. `NodeBlocked` 是 reducer 正式事件：runtime node 进入 `Blocked`，orchestration 聚合为 `Paused`，LifecycleRun 聚合为 `Blocked`。
6. 本批次不新增 durable lease/outbox 表；当前 drain 在单进程应用服务内串行执行，业务事实源保持 `LifecycleRun.orchestrations[]`。

## 上下文顺序

实现代理必须读取：

1. 本任务 `prd.md`、`design.md`、`implement.md`。
2. `.trellis/tasks/06-06-orchestration-domain-contract` 最终实现 diff。
3. `.trellis/tasks/06-06-workflow-graph-compiler` artifacts 和最终 compiler 实现 diff。
4. 父任务 research：
   - `research/claude-workflow-behavior-coverage.md`
   - `research/workflow-graph-compiler-plan.md`
   - `research/common-runtime-convergence-plan.md`
   - `research/current-code-context.md`
5. `implement.jsonl` 中列出的 specs。

## 建议实施步骤

1. 修正 domain/application 合同，使 runtime 坐标以 `orchestration_id + node_path + attempt` 为准；definition provenance 只进入 plan metadata 或可选审计字段。
2. 新增 application orchestration runtime 模块，只消费 `OrchestrationPlanSnapshot` 与 `OrchestrationInstance`。
3. 定义 `OrchestrationEvent`，覆盖当前 ActivityEvent 等价事件和 plan activation。
4. 实现 entry ready node materialization。
5. 实现 node event -> `RuntimeNodeState` / `StateExchangeSnapshot` 纯状态推进。
6. 接入 semantic executor launcher：
   - `AgentCall`
   - `Function`
   - `LocalEffect`
   - `HumanGate`
7. 接入 scheduler claim/outbox；若沿用现有 claim 表模式，必须改成 `orchestration_id + node_path + attempt` lease，并明确它只是 operational lease。
8. 升级 runtime trace anchor / resolver 到 orchestration node 坐标。
9. 改造 session terminal callback 和 `complete_lifecycle_node` 走 runtime node terminal event。
10. 从 orchestration snapshot 生成 graph-compatible `LifecycleRunView`。
11. 移除 scheduler / command path 对 Activity-specific runtime snapshot 和 repository 的事实源读取。（已完成主线删除）
12. 新增 migration 拆除或停止主线读写 `lifecycle_workflow_instances`、activity attempt claim/assignment 旧坐标；必要投影只能由 orchestration snapshot 派生。（已完成 schema 收束）
13. 更新 specs 与 migration。

## 验证命令

按实际 touched package 缩小范围，预期至少：

```powershell
cargo test -p agentdash-application orchestration
cargo test -p agentdash-application workflow
cargo test -p agentdash-domain orchestration
cargo test -p agentdash-infrastructure workflow_repository
pnpm run migration:guard
git diff --check
```

如触及 contracts / frontend projection：

```powershell
pnpm run contracts:check
pnpm --filter app-web test -- lifecycle
```

如完成端到端 runtime 切换，应运行：

```powershell
pnpm dev
```

## 停止条件

- compiler 输出不足以表达 runtime 所需节点、rules 或 state exchange。
- 实现需要引入第二套 runtime identity / command path。
- terminal resolver 无法稳定得到 `orchestration_id` / `node_path`。
- Function/local effect 只能通过特殊旁路完成，无法进入 `RuntimeNodeState`。
- 需要新增长期兼容 fallback。
