# 用户偏好工作状态建模

## Goal

建立可扩展的用户偏好工作状态模型，让前端启动和 Project 列表加载后能够恢复用户上次使用的 Project，而不是依赖 Project 列表顺序默认跳到最后创建的 Project。

本任务的产品价值是让用户回到 Dashboard 时自然回到自己最近关注的工作空间，同时为后续更多用户级 UI / 工作区偏好提供统一承载结构。

## Requirements

- 用户偏好数据结构应以可扩展的结构块承载工作区偏好，不新增过细的单业务字段。
- Project 默认选择应优先使用用户上次使用的 Project。
- 用户显式切换 Project 后，应持久化为用户偏好的一部分。
- 当偏好指向的 Project 当前不可见、已删除或无权限时，系统应按明确规则选择可用 Project，并保持前后端状态一致。
- 实现应符合当前预研阶段约束：不做兼容性回退，不保留旧模型分支；涉及数据库结构时应包含 migration。

## Confirmed Facts

- 后端已有 `settings` 表和 `SettingsRepository`，支持 `system` / `user` / `project` scope，值为 JSON。`/settings?scope=user` 会解析到当前登录用户，适合作为用户偏好事实源。
- 现有 `agent.pi.user_preferences` 是 Pi Agent 启动上下文偏好，不适合承载 Dashboard UI 工作状态。
- 前端已有通过 user-scope settings 保存结构化 UI 状态的先例：`ui.agentrun_workspace_tab_layout.{workspaceKey}` 保存 AgentRun workspace tab layout。
- Project 列表后端按 `created_at DESC` 返回；`projectStore.fetchProjects()` 当前在没有 `currentProjectId` 时选中 `projects[0]`，所以会偏向最新创建的 Project。
- `projectStore.createProject()` 和 `cloneProject()` 当前会把新 Project 插到列表头并设为当前 Project；本任务保留“用户显式创建/克隆后进入新 Project”的交互，并把它写入用户工作状态。
- `AppContent` 在 `AuthGate` 放行后调用 `fetchProjects()`，此时 current user 已就绪，可以读取 user-scope settings。
- 本任务只新增结构化 Dashboard 工作状态，不迁移既有分散的用户级 UI settings；这样先解决 Project 默认选择痛点，同时为后续偏好收束建立目标结构。

## Acceptance Criteria

- [ ] 用户拥有多个 Project 时，刷新或重新打开前端后默认进入上次使用的 Project。
- [ ] 新建 Project 不再因为列表顺序导致自动覆盖用户正在关注的 Project。
- [ ] 用户切换 Project 后，偏好会写回后端，并在下一次加载时生效。
- [ ] 偏好结构不是单独业务列，而是面向用户工作状态的结构化模型。
- [ ] 偏好指向不可用 Project 时，前端进入一个确定的可用 Project 状态，不出现空白、反复跳转或错误选中。
- [ ] 用户显式创建或克隆 Project 后进入新 Project，并将新 Project 写入工作状态。

## Notes

- 用户明确不希望添加 `last_used_project_id` 这类过细业务字段；设计应围绕用户偏好数据结构整体演进。
- 本任务需要先检查现有 settings / preference / Project store / Project API 链路，再确定落点。
- 现有 settings schema 已满足存储需求，本任务不新增数据库 migration。
