# 用户偏好工作状态建模设计

## Architecture

用户工作状态落在现有 `settings` API 的 user scope，不新增 Project 表字段或新的偏好表。新 setting 只服务 Dashboard / workspace UI 状态，和 `agent.pi.user_preferences` 保持边界清晰。

建议 key：

```text
ui.workspace_state
```

建议值：

```json
{
  "schema_version": 1,
  "navigation": {
    "current_project_id": "project-id"
  },
  "recent": {
    "project_ids": ["project-id"]
  }
}
```

## Data Flow

前端启动时，`AppContent` 继续在身份就绪后拉取 Project 列表。`projectStore.fetchProjects()` 同步读取 user-scope `ui.workspace_state`，在 Project 列表中查找 `navigation.current_project_id`：

- 命中可访问 Project：设为当前 Project。
- 未命中但当前 `currentProjectId` 仍在列表中：保持当前 Project。
- 均不可用：选择列表第一个可用 Project，并把工作状态修正到该 Project。
- 列表为空：当前 Project 置空，不写入 Project id。

用户显式选择 Project、创建 Project、克隆 Project、删除当前 Project 后，`projectStore` 通过同一 service 写回 `ui.workspace_state`。写回时保留 `schema_version`，更新 `navigation.current_project_id`，并维护去重后的 `recent.project_ids`。

## Boundaries

- 后端不新增专用 DTO；复用 `settingsApi.list/update` 和 generated `settings-contracts.ts`。
- 前端新增一个小 service/model 处理 `ui.workspace_state` 的读写、解析、默认值和 recent 列表收敛。
- `projectStore` 只消费该 service，不直接拼 settings key 或解析 JSON。
- 不迁移 `ui.agentrun_workspace_tab_layout.*`，因为它是 AgentRun workspace 粒度 layout 状态，和 Project 导航偏好变化节奏不同。

## Migration

现有 `settings` 表已经支持 user-scope JSON value，本任务不需要数据库 migration。旧用户第一次加载时没有 `ui.workspace_state`，前端会按可访问 Project 列表选择确定项目并写入新结构。

## Tradeoffs

把 Project 导航偏好放进结构化 setting，而不是独立 key 或列，可以让后续增加 Dashboard 默认 tab、最近 Project、上次打开的 assets 类目等偏好时继续落在同一块用户工作状态里。代价是当前仍保留既有分散的 UI setting key，但本任务变更面小、可快速验证。
