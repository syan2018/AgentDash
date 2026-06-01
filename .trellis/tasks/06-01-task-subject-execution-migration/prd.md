# Task Subject Execution 迁移

## 目标

把 Task execution 从 Task-owned runtime state 迁到 `SubjectRef(kind=Task)` dispatch，让 Task 只保存业务 spec / authoring preference，运行状态来自 projection。

## 依赖

- 父任务：`06-01-session-lifecycle-control-plane-refactor`
- 依赖：`06-01-lifecycle-dispatch-service`
- 依赖：`06-01-session-lifecycle-target-anchors-schema`
- 依赖：`06-01-workflow-agent-assignment-migration`

## 蓝图阶段

- 推进：`target-state-blueprint.md` B5 Business Subject Migration。
- 退出贡献：Task 是 business spec/view subject；execution state 从 SubjectRef、association、agent assignment、activity attempt、artifacts 投影得到。

## 重构模式

- 采用父任务 `target-state-blueprint.md` 中的 breaking-mode 约束。
- 删除 task session ownership 与 `lifecycle_step_key` 使用，不做 compatibility wrapper。
- Task UI/API 可以在 SubjectExecutionView 和前端迁移落地前暂时不完整。

## 需求

- Task start / continue 输入改为 `SubjectRef(kind=Task, id=task_id)`。
- Task execution 默认优先在同一 `LifecycleRun` 内追加或复用 `WorkflowGraphInstance` / `LifecycleAgent`；只有独立生命周期边界成立时才创建 linked run。
- `Task.lifecycle_step_key` 迁为 `SubjectExecutionView.task_projection` 或删除。
- `Task.status`、`Task.artifacts` 改为 LifecycleRun / ActivityAttempt / AgentAssignment 派生 projection。
- `Task.agent_binding` 改为 dispatch policy / procedure override / authoring preference，不作为 runtime owner。

## 交付物

- Task start / continue 的 `SubjectRef(kind=Task)` dispatch。
- `SubjectExecutionView.task_projection` 或等价 projection。
- Task status/artifact/current agent source refs。
- Task model / API / frontend type 中 runtime owner 字段的删除或降级。

## 不承担

- 不默认为 Task 子图创建 child `LifecycleRun`。
- 不让 Task entity 保存 Activity 位置 truth。
- 不负责整体 frontend route 迁移。

## 验收标准

- [ ] Task 数据模型不再保存 Activity 位置 truth。
- [ ] Task 页面仍能展示状态、产物、当前 agent，但这些字段带有 source run / agent / activity / attempt refs。
- [ ] Task continue 不通过 session id 查找 run。
- [ ] Task execution 可以表达 same-run WorkflowGraphInstance 路径，不把 Task 子图误建成 child LifecycleRun。
- [ ] 相关 API / frontend type 不再把 Task session 当作主模型。
