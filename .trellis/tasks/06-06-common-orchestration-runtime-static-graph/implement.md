# Common Orchestration Runtime Static Graph 实施计划

## 状态

暂不启动实现。本任务依赖 `workflow-graph-compiler` 的 plan 输出稳定。当前只作为顺序推进的第三个子任务，防止后续直接改旧 Activity engine。

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

1. 新增 application orchestration runtime 模块，只消费 `OrchestrationPlanSnapshot`。
2. 定义 `OrchestrationEvent`，覆盖当前 ActivityEvent 等价事件和 plan activation。
3. 实现 entry ready node materialization。
4. 实现 node event -> `RuntimeNodeState` / `StateExchangeSnapshot` 纯状态推进。
5. 接入 semantic executor launcher：
   - `AgentCall`
   - `Function`
   - `LocalEffect`
   - `HumanGate`
6. 接入 scheduler claim/outbox；若沿用现有 claim 表模式，明确它只是 lease。
7. 升级 runtime trace anchor / resolver 到 orchestration node 坐标。
8. 改造 session terminal callback 和 `complete_lifecycle_node` 走 runtime node terminal event。
9. 从 orchestration snapshot 生成 graph-compatible `LifecycleRunView`。
10. 移除 scheduler / command path 对 `WorkflowGraphInstance.activity_state` 的事实源读取。
11. 更新 specs 与 migration。

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
- 实现需要继续依赖 `WorkflowGraphInstance.activity_state` 作为推进事实源。
- terminal resolver 无法稳定得到 `orchestration_id` / `node_path`。
- Function/local effect 只能通过特殊旁路完成，无法进入 `RuntimeNodeState`。
- 需要新增长期兼容 fallback。
