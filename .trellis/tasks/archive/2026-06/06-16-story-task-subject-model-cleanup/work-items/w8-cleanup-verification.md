# W8 Cleanup + Verification

## 状态

done

## 依赖

- W5 done
- W6 done
- W7 done

## 目标

完成旧模型清理、总体验证、spec finish 和提交前风险记录。

## 输入

- W5 / W6 / W7 的产出和交接。
- `implement.md` 验证命令。
- `.trellis/spec/backend/story-task-runtime.md`
- `.trellis/spec/backend/repository-pattern.md`
- `.trellis/spec/frontend/type-safety.md`
- `.trellis/spec/frontend/state-management.md`
- `.trellis/spec/cross-layer/frontend-backend-contracts.md`

## 范围

- 搜索并清理旧 TaskStatus、旧 `/tasks/{id}` 心智、`dispatch_preference`、`task.artifacts`、Story Task durable CRUD。
- 确认 Task API 第一版只以 Run / AgentRun workspace 为作用域。
- 确认 Story 页面只消费 Story Task projection。
- 确认 runtime artifacts、latest runtime node、linked runs 只由 SubjectExecutionView / Lifecycle projection 承担。
- 更新 focused tests / E2E 断言方向。
- 更新长期 spec，只记录新边界为什么成立。
- 跑总体验证命令并记录未覆盖风险。

## 验收

- `cargo check --workspace` 按风险面执行并记录结果。
- `pnpm run migration:guard` 通过。
- `pnpm run contracts:check` 通过。
- `pnpm run frontend:check` 通过。
- LifecycleRun aggregate、repository、Story projection、SubjectExecutionView、MCP、frontend focused tests 覆盖关键路径。
- repo 搜索旧字段只剩 migration / 历史 research / 明确允许的文档语境。

## 产出记录

- 删除旧 `agentdash_domain::task` 模块、Story aggregate 内 `tasks/task_count` 字段、Story task mutation helpers 和 Story task DTO。
- 删除旧 application `task::artifact`、`task::config`、`task::gateway` 实现，以及 `task_session_terminal` preset / owner-default rule / AppState handler 注册。
- `TaskExecutionView.task_status` 改为 `TaskPlanStatus`，通过 `LifecycleRun.tasks` 定位计划项；runtime execution 状态仍由 lifecycle projection 提供。
- Story contract / generated TS / frontend Story board/list/card 不再消费持久 `task_count`；Story 详情继续消费 Story Task projection。
- Relay MCP Story 输出不再暴露 `task_count`；workflow MCP preset 示例改为当前内置 preset。
- Hook effect 文档和 terminal effect 测试改用中性 `record:note` 示例，避免把 Task 状态写回作为通用默认路径。
- 更新 `.trellis/spec/backend/hooks/hook-script-engine.md` 的 builtin preset baseline。

## 验证记录

- `pnpm run contracts:check`
- `cargo check --workspace`
- `pnpm run frontend:check`
- `pnpm run migration:guard`
- 旧 surface 搜索：代码主线无 `agentdash_domain::task`、`TaskDispatchPreference`、`TaskArtifactAdded`、`task_session_terminal`、`task:set_status`、`tasksByStoryId` 命中；剩余命中为 schema migration 的 `task_count`、workflow negative serialization test 的 `dispatch_preference`、Story projection contract 复用当前 `TaskResponse` 类型。

## 风险与交接

- `0001_init.sql` 仍包含历史 `task_count` 列，后续由 `0015_lifecycle_run_tasks_story_task_cleanup.sql` 在迁移序列中删除；migration guard 已通过。
- `.trellis/tasks/.../research` 与部分 work-item 输入说明保留旧字段名称作为规划证据，代码事实源以 W8 验证记录为准。
