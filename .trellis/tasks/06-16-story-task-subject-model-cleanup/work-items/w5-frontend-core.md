# W5 Frontend Core

## 状态

pending

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

- 待填写。

## 风险与交接

- W8 需要前端旧字段搜索结果和未覆盖风险。
