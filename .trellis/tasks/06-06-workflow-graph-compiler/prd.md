# WorkflowGraph 编译器

## 目标

在 Orchestration 领域合同落地后，规划确定性的 `WorkflowGraph -> OrchestrationPlanSnapshot` 编译器；完成计划后停下来评审，不直接进入实现。

## 需求

- 定义纯函数、确定性的 `WorkflowGraph -> OrchestrationPlanSnapshot` 编译器实施计划。
- 将 compiler 作为 `.trellis/tasks/06-06-orchestration-domain-contract` 的后续任务；在领域合同实现并 review 前不启动本任务实现。
- 在 plan IR 中保留当前 public `WorkflowGraph` 语义：
  - `entry_activity_key`
  - Agent / Function / Human Activity executor identity
  - input / output ports
  - completion policy
  - transition condition
  - artifact binding
  - join policy
  - iteration policy
  - transition `max_traversals`
- 明确当前 runtime 缺口：即使旧 Activity runtime 尚未完整执行 `join_policy`、`artifact_alias`、`max_traversals`，compiler 也必须把这些语义保留进 plan。
- 定义 pathful diagnostics，覆盖 invalid graph shape、unsupported executor policy、dangling refs、strict artifact mismatch、unbounded cycle。
- 实现前先定义 fixture 覆盖。
- 编译器保持纯粹：不读 repository、不创建 run、不启动 agent/session、不做权限授权、不 materialize scheduler/runtime state。

## 验收标准

- [ ] PRD、design、implement plan 和 context manifests 完整。
- [ ] 任务保持 `planning`，直到用户评审编译器设计。
- [ ] design 明确要保留的 source semantics 与非目标。
- [ ] implement plan 列出模块位置选择、诊断、fixtures 和验证命令。
- [ ] 任务上下文指向父任务 compiler research、当前代码事实地图和相关 specs。

## 备注

- 父任务：`.trellis/tasks/06-06-dynamic-workflow-lifecycle-research`。
- 依赖：`.trellis/tasks/06-06-orchestration-domain-contract`。
- 研究来源：`.trellis/tasks/06-06-dynamic-workflow-lifecycle-research/research/workflow-graph-compiler-plan.md`。
- 停止点：规划 artifacts 准备好后，先和用户评审，再执行 `task.py start`。
