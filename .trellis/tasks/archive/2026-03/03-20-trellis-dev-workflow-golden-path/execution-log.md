# 执行记录

## 2026-03-20

### 本轮已启动的实现

- 已确认第一条真实 workflow 采用 `Trellis Dev Workflow`，而不是继续围绕 Symphony issue loop 扩散建模。
- 已在 application 层提供 `build_trellis_dev_workflow_definition(target_kind)`。
- 已落地 `WorkflowRunService`，使黄金路径具备最小 runtime 能力：
  - 显式启动 run
  - `Start / Implement / Check / Record` phase 顺序推进
  - 对需要 session 的 phase 强制校验 `session_binding_id`
  - 写入结构化 `WorkflowRecordArtifact`
- 已接入 API：
  - 支持 bootstrap 内置 Trellis workflow
  - 支持对 target 显式启动 workflow run
  - 支持 phase activate / complete
  - 支持按 target 查询 workflow runs

### 当前还未做

- 尚未接前端 run/phase 可视化
- 尚未把 workflow run 与现有 project/story/task session 页面联通

### 下一步建议

- 先做最小 API：
  - `POST /workflows/runs`
  - `POST /workflow-runs/{id}/phases/{phase}/activate`
  - `POST /workflow-runs/{id}/phases/{phase}/complete`
- 然后让会话页最少看到：
  - 当前 workflow run
  - 当前 phase
  - phase status
  - 最近 record artifact

## 2026-03-21

### 黄金路径已跑通

- 已完成 `Trellis Dev Workflow` 的前后端闭环接线：
  - Project 详情里可注册内置 Trellis workflow
  - Project 详情里可把 Task workflow 设为默认执行流程
  - Task 抽屉里可启动 workflow run
  - Task 抽屉里可推进 phase，并显示 phase 状态与 record artifact
- 已复用 `GET /api/sessions/{id}/bindings` 自动解析 Task 对应 `SessionBindingOwner`，不再靠前端硬编码 session 关系。
- 已验证需要 session 的 `Implement` phase 会在 activate 时写入 `session_binding_id`，并在 UI 中显示“session 已挂接”。
- 已验证 `Start` phase 完成后会生成结构化 `phase_note` 记录产物。

### Playwright 实证

- 在 `2026-03-21` 使用 Playwright 打开真实前端，完成以下路径：
  - 进入已有 Project
  - 打开 `Workflow` tab
  - 注册 `Task Trellis`
  - 设为默认 Task 流程
  - 进入 Story -> TaskDrawer
  - 启动 Task workflow run
  - 激活并完成 `Start`
  - 激活 `Implement` 并确认 `session_binding_id` 成功挂接
- 验证截图：
  - `output/playwright/task-workflow-validation.png`
