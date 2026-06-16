# W0 Spec Gate

## 状态

pending

## 目标

确保长期 spec、任务文档和 subagent manifest 都指向同一个目标模型：Story 是 subject / context aggregate，Task 是 `LifecycleRun.tasks` 内的计划项事实，runtime execution truth 来自 `SubjectExecutionView` / Lifecycle projection。

## 输入

- `prd.md`
- `design.md`
- `implement.md`
- `implement.jsonl`
- `check.jsonl`
- `.trellis/spec/backend/story-task-runtime.md`
- `.trellis/spec/backend/repository-pattern.md`
- `.trellis/spec/backend/database-guidelines.md`
- `.trellis/spec/frontend/type-safety.md`
- `.trellis/spec/frontend/state-management.md`
- `.trellis/spec/cross-layer/frontend-backend-contracts.md`

## 范围

- 更新会误导实现的长期 spec。
- 确认 `LifecycleRun.tasks` 是 Task durable facts 唯一来源。
- 确认 Story 只消费 Task projection。
- 确认 Task plan DTO 不包含 execution status、artifacts 或 `dispatch_preference`。
- 确认 manifest 覆盖 backend / frontend / cross-layer / research 上下文。

## 范围边界

- 该节点只解决实现前的知识一致性，原因是后续 domain、migration 和 contract 都需要从同一目标模型出发。
- 业务代码、migration 和 TypeScript contract 放到后续节点处理，原因是这些变更需要依赖 W0 固化后的 spec 和 manifest。

## 验收

- 长期 spec 与 `design.md` 没有事实源冲突。
- `implement.jsonl` 和 `check.jsonl` 包含后续实现与检查所需上下文。
- 后续 W1 可以直接以 `LifecycleRun.tasks` 为目标实现。

## 产出记录

- 待填写。

## 风险与交接

- 待填写。
