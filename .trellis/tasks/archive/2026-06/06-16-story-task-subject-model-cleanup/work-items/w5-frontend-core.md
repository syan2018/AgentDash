# W5 Frontend Core

## 状态

done

## 依赖

- W4 done

## 目标

将前端 Task 体验拆成 Run-scoped Task plan、Story Task projection 和 SubjectExecution runtime projection 三条清晰路径。

## 输入

- W3 generated TS。
- W4 backend command / read model。
- `.trellis/spec/frontend/architecture.md`
- `.trellis/spec/frontend/type-safety.md`
- `.trellis/spec/frontend/state-management.md`
- `packages/app-web/src/services/story.ts`
- `packages/app-web/src/services/lifecycle.ts`
- `packages/app-web/src/stores/storyStore.ts`
- `packages/app-web/src/stores/lifecycleStore.ts`
- `packages/app-web/src/pages/StoryPage.tsx`
- `packages/app-web/src/pages/AgentRunWorkspacePage.tsx`
- `packages/app-web/src/features/task/task-drawer.tsx`
- `packages/app-web/src/features/task/task-subject-execution-panel.tsx`
- `packages/app-web/src/components/ui/status-badge.tsx`

## 范围

- AgentRun workspace 增加 Task plan 面板，成为 Task 创建、推进、归档和 assignment 的第一入口。
- Story 页面只展示 Task projection 和来源关系。
- `storyStore.tasksByStoryId` 改为 Story projection cache，或迁出到 run-scoped Task plan store。
- `lifecycleStore` 保持 SubjectExecution / runtime artifacts / linked runs 唯一执行投影缓存。
- TaskDrawer 聚焦计划项编辑、状态推进、assignment 和 linked runs。
- Task status UI 切换为 `open / active / review / blocked / done / dropped`。

## 范围边界

- 前端消费 generated DTO，原因是跨层 wire shape 的事实源在 Rust contract 和生成文件。
- 旧状态、旧字段和 artifacts 双来源通过类型错误直接收敛，原因是项目未上线且目标是一次性回到正确模型。
- runtime artifacts 只来自 `SubjectExecutionView` / lifecycle projection，原因是 Task plan facts 与 execution projection 是不同事实源。

## 验收

- 用户可以在 AgentRun workspace 创建、推进、归档 Task。
- Story 页面能展示由 Story-bound run / linked run 推导的 Task projection。
- TaskDrawer 不读取 `task.artifacts`。
- `pnpm run frontend:check` 通过。
- focused tests 覆盖 TaskStatusBadge 新状态、Story projection-only、TaskDrawer execution projection 来源。

## 产出记录

- 新增 `packages/app-web/src/services/taskPlan.ts` 与 `packages/app-web/src/stores/taskPlanStore.ts`，前端通过 run / agent-run scoped API 执行 Task plan list/create/update/status/archive。
- `AgentRunWorkspacePage` 增加 `TaskPlanPanel`，支持当前 AgentRun 内计划项创建、状态推进、归档和基础 assignment 字段录入。
- `storyStore.tasksByStoryId` 已替换为 `storyTaskProjectionByStoryId`；`StoryPage` 改为消费 `/stories/{id}/task-projection`，展示 projection 来源关系，不再提供 Story 页面直接创建 Task。
- `TaskDrawer` 改为计划字段、状态、owner/assigned/source task、context/story ref 与 linked runs；runtime artifacts/latest node 只通过 `TaskSubjectExecutionPanel` 的 `SubjectExecutionView` 展示。
- `TaskStatusBadge` 切换为 `open / active / review / blocked / done / dropped`，新增 focused test 覆盖新状态集合。
- 删除旧 Story Task 创建面板与 dispatch preference UI/helper，前端类型入口不再从 generated Task contract 暴露旧 `dispatch_preference` / `artifacts` wrapper。
- 验证已通过 `pnpm run frontend:check` 和 `pnpm --filter app-web test src/components/ui/status-badge.test.tsx`。

## 风险与交接

- 已运行旧 surface 搜索：`rg -n 'TaskDispatchPreference|dispatch_preference|task\.artifacts|tasksByStoryId|task\.description|task\.story_id|CreateTaskPanel|dispatch-preference|awaiting_verification|TaskStatusBadge status="(pending|assigned|running|failed|cancelled|completed)"' packages/app-web/src`，无命中。
- 已运行 `rg -n 'TaskStatus|dispatch_preference|task\.artifacts|tasksByStoryId' packages/app-web/src`；仅剩新 TaskStatus 类型/组件使用，无旧字段命中。
- 当前 Task assignment UI 第一版是手填 `assigned_agent_id` / `owner_agent_id`；W6/W7 后续若提供 companion/fanout selector，可接入同一 run-scoped update command。
- Story projection 写后刷新依赖显式重新拉取；若后续增加 Task plan event，需要事件 payload 带 Story projection key 或后端提供 projection invalidation 事件。
- 未改 MCP/capability 与 workflow fanout；对应节点仍需完成后再由 W8 做全链路旧 surface 总清理。
