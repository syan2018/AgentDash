# Workflow phase 推进与 completion 闭环

## Goal

把 `manual`、`session_ended`、`checklist_passed` 三种 completion mode 做成统一的运行态推进闭环，使 Hook Runtime、Task 状态、WorkflowRunService 和前端展示保持一致语义。

## Background

当前系统已经具备：

- workflow phase metadata 暴露
- `BeforeStop` 对 completion_mode 的第一版消费
- implement/check phase 的基础 gate

但还缺少完整闭环：

- phase completion evidence 没有统一结构
- hook 观察到的 signal 没有正式写回 workflow 推进器
- route/gateway 层仍保留部分 phase reconcile 逻辑

## Scope

- 定义 completion evidence 结构
- 统一不同 completion mode 的运行态信号
- 梳理 Hook Runtime 与 WorkflowRunService 的协作接口
- 收敛现有的 route/gateway phase reconcile 逻辑

## Requirements

- `BeforeStop` 不只是继续/放行，还要给出 completion judgment
- `AfterTool` / `AfterTurn` 可以产生 phase completion signal
- task status / session state / checklist evidence 需要统一解读
- 前端可读到当前 phase 为何允许完成或为何被阻止

## Acceptance Criteria

- [ ] 明确 completion evidence 结构
- [ ] 明确三种 completion mode 的信号来源与判定规则
- [ ] 收敛当前分散的 phase reconcile 逻辑
- [ ] 给出 Hook Runtime -> WorkflowRunService 的闭环方案
- [ ] 给出可测试的典型场景矩阵

## References

- [execution_hooks.rs](crates/agentdash-api/src/execution_hooks.rs)
- [routes/workflows.rs](crates/agentdash-api/src/routes/workflows.rs)
- [workflow/run.rs](crates/agentdash-application/src/workflow/run.rs)
- [trellis_dev_task.json](crates/agentdash-application/src/workflow/builtins/trellis_dev_task.json)
