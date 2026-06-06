# Common Orchestration Runtime 正式接入实施计划

## 状态

已进入实现阶段。本任务依赖 `workflow-graph-compiler` 的 plan 输出稳定；后续实现应围绕唯一 runtime path 收敛，而不是继续加固旧 Activity engine 或 `WorkflowGraphInstance` 中间身份。

当前实现进展：

- 静态 `WorkflowGraph` dispatch / lifecycle start 已先编译为 `OrchestrationPlanSnapshot`，并直接写入 `LifecycleRun.orchestrations[]`。
- 主 runtime 坐标已切到 `orchestration_id + node_path + attempt`；`RuntimeSessionExecutionAnchor` 持久化该坐标，session terminal / subject cancel / complete node 通过 anchor 回到 runtime node。
- `RepositorySet`、API bootstrap、PostgreSQL repository implementation 已停止构造和注入 `WorkflowGraphInstanceRepository`、`AgentAssignmentRepository`、`ActivityExecutionClaimRepository`。
- domain repository contract 已移除 `WorkflowGraphInstanceRepository`、`AgentAssignmentRepository`、`ActivityExecutionClaimRepository` 三个旧仓储 trait；application 模块树已删除旧 `activity_run` / `scheduler` / `agent_executor` 路径。
- lifecycle VFS mount metadata、artifact scope、session assembly 和 frame construction 已转为 `orchestration_id + node_path + attempt`。`LifecycleMountSurface` 作为 session/VFS 的窄接口，provider 从 `LifecycleRun.orchestrations[]` 定位 `OrchestrationInstance` 与 `RuntimeNodeState`，不再读取 graph instance 仓储。
- 新增 migration `0004_orchestration_runtime_convergence.sql` 将 anchor / routine dispatch schema 切到 orchestration node 坐标，并 drop 旧 activity claim / assignment / workflow instance 表。
- dispatch、subject cancel、boot task projection 测试已经改为断言 `LifecycleRun.orchestrations[]`，不再通过旧 graph instance / assignment mock 证明行为。

后续剩余面：

- `LifecycleRunView.workflow_graph_instances[]` 等 graph-compatible DTO 仍保留旧字段；短期作为 projection 兼容 view，command path 不消费这些字段。
- `workflow/lifecycle/journey` 仍保留若干旧 graph instance helper 供兼容 projection 参考；后续应在 native orchestration progress tree 落地后删去。
- `ActivityActivation` 中 phase hot-update applier 当前未被新主路径调用；后续应结合 orchestration executor adapter 决定保留为 live capability transition，还是并入统一 executor surface。
- `workflow/orchestration/runtime.rs` 已完成 reducer 第一切片：NodeStarted / NodeCompleted / NodeFailed / NodeCancelled 可更新 runtime node、trace refs、state exchange、transition successor 和 terminal idempotency；`complete_lifecycle_node` / session terminal callback 已接到 reducer。

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
2. 新增 application orchestration runtime 模块，只消费 `OrchestrationPlanSnapshot` 与 `OrchestrationInstance`，不消费 `WorkflowGraphInstance`。
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
11. 移除 scheduler / command path 对 `WorkflowGraphInstance.activity_state`、`WorkflowGraphInstanceRepository`、`AgentAssignment(graph_instance_id, activity_key, attempt)` 的事实源读取。（已完成主线删除）
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
- 实现需要继续依赖 `WorkflowGraphInstance` 或 `WorkflowGraphInstanceRepository` 作为 runtime identity / command path。
- terminal resolver 无法稳定得到 `orchestration_id` / `node_path`。
- Function/local effect 只能通过特殊旁路完成，无法进入 `RuntimeNodeState`。
- 需要新增长期兼容 fallback。
