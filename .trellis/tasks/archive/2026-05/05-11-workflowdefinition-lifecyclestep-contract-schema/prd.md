# WorkflowDefinition 与 LifecycleStep contract 合并（schema 收敛）

## Goal

等 [`05-11-workflow-lifecycle-editor`](../05-11-workflow-lifecycle-editor/prd.md) 前端 Clone 语义稳定运行后，把 `WorkflowDefinition` 表折叠回 `LifecycleStepDefinition.contract`，消除"双实体"的领域冗余，让 schema 和前端 mental model 完全对齐。

## Context

- 上游任务决定：前端走 Clone 语义，每个 step 自带 contract（`workflow_key` 自动派生自 step key）
- 一旦 Clone 跑顺，`WorkflowDefinition` 在运行时的作用退化为纯 key-lookup 表，没有跨 step 共享价值
- 保留双实体的唯一代价是 schema 冗余 + 运行时多一次查表

## Preconditions (阻塞本任务开始)

- [ ] 前端任务 `05-11-workflow-lifecycle-editor` 已完成并在生产稳定运行
- [ ] 确认没有 WorkflowDefinition 跨 step 共享的存量数据（即 `workflow_key` 的引用计数 ≤ 1）
- [ ] builtin bundle 的多 workflow 形态有明确迁移方案

## Scope

- `LifecycleStepDefinition` 新增 `contract: WorkflowContract` 字段（内嵌）
- 逐步迁移运行时查找路径：`workflow_key → workflow repo` → `step.contract`
- 写一次性 migration：把 workflow table 里的 contract 落进 step 行
- 最终 drop `workflow_definitions` 表 / repository / route

## Out of Scope

- 前端 UI 改造（上游任务已完成）
- 额外能力模型改动

## Notes

- 预计跨 backend / migration / api / frontend types 的中型任务
- 会破坏现有 builtin JSON 结构（一 lifecycle 多 workflow 变成单 lifecycle 带嵌套 contract）
- 风险：Workflow run snapshot 历史数据里可能仍引用 workflow_id，需要兼容或回填

## Status

Planning（等前端任务完成后再推进）
