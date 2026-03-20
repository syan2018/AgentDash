# 执行记录

## 2026-03-20

### 本轮已完成

- 已在 `agentdash-domain` 建立正式 `workflow` 模块：
  - `WorkflowDefinition`
  - `WorkflowAssignment`
  - `WorkflowRun`
  - `WorkflowPhaseState`
  - `WorkflowRecordArtifact`
- 已补齐 `WorkflowDefinitionRepository / WorkflowAssignmentRepository / WorkflowRunRepository` 接口。
- 已在 `agentdash-infrastructure` 落地 `SqliteWorkflowRepository`，完成 definition / assignment / run 三类实体的持久化骨架。
- 已在 `agentdash-application` 新增 workflow application 服务：
  - `WorkflowCatalogService`
  - `WorkflowRunService`
- 已支持以下最小用例：
  - workflow definition upsert
  - project workflow assignment 写入与 default 角色归一
  - workflow run 创建
  - phase 激活
  - phase 完成
  - record artifact 追加
- 已接入 `agentdash-api` 主干：
  - workflow repo 已注入 `AppState`
  - 已暴露 workflow / assignment / run / phase API
  - 已补 response DTO 与 API 错误映射

### 当前验证

- `cargo test -p agentdash-domain workflow -- --nocapture`
- `cargo test -p agentdash-application workflow -- --nocapture`
- `cargo check -p agentdash-domain -p agentdash-infrastructure -p agentdash-application`

### 下一步建议

- 在前端补最小 workflow run 可视化。
- 在 task / story / project 详情页显示当前 workflow run 与 phase。
- 继续推进 record 阶段与 journal / archive 自动化动作的接线。

## 2026-03-21

### 本轮新增完成

- 已把 workflow 前端正式接到主干 UI：
  - `ProjectDetailDrawer` 新增 `Workflow` tab
  - `TaskDrawer` 新增 `Workflow 执行` 面板
  - `StoryPage` 已把 `project_id` 透传给 `TaskDrawer`
- 已新增前端 workflow service / store / 类型映射，前端可以直接消费：
  - workflow definition
  - project workflow assignment
  - workflow run
  - phase activate / complete
  - record artifact
- 已修复 React 19 + Zustand 下 selector fallback 返回新数组导致的无限重渲染问题。
- 已通过真实浏览器验证完成最小闭环：
  - Project 侧注册 `Trellis Dev Workflow`
  - Project 侧设为默认 Task workflow
  - Task 侧启动 workflow run
  - `Start` phase 激活并完成
  - `Implement` phase 激活并成功挂接 `session_binding_id`
  - `phase_note` 结构化记录产物已显示

### 本轮验证

- `pnpm --dir frontend exec eslint src/features/workflow/project-workflow-panel.tsx src/features/workflow/task-workflow-panel.tsx src/features/project/project-selector.tsx src/features/task/task-drawer.tsx src/stores/workflowStore.ts src/services/workflow.ts src/pages/StoryPage.tsx`
- `pnpm --dir frontend exec tsc --noEmit`
- `cargo check -p agentdash-api -p agentdash-application -p agentdash-domain -p agentdash-infrastructure`
- `cargo build --bin agentdash-server --bin agentdash-local`
- Playwright 实机验证截图：
  - `output/playwright/task-workflow-validation.png`
