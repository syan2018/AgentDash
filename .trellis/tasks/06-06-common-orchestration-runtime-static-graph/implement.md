# Common Orchestration Runtime 正式接入实施计划

## 状态

已进入实现阶段。本任务依赖 `workflow-graph-compiler` 的 plan 输出稳定；后续实现应围绕唯一 runtime path 收敛，而不是继续加固旧 Activity engine 或 `WorkflowGraphInstance` 中间身份。

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
11. 移除 scheduler / command path 对 `WorkflowGraphInstance.activity_state`、`WorkflowGraphInstanceRepository`、`AgentAssignment(graph_instance_id, activity_key, attempt)` 的事实源读取。
12. 新增 migration 拆除或停止主线读写 `lifecycle_workflow_instances`、activity attempt claim/assignment 旧坐标；必要投影只能由 orchestration snapshot 派生。
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
