# 状态管理

> Zustand 5 全局状态 + React useState 本地状态。

---

## 状态分层

| 状态类型 | 存放位置 | 示例 |
|----------|----------|------|
| 本地 UI 状态 | 组件内 `useState` | `isOpen`, `selectedTab` |
| Feature 状态 | Feature `model/` hooks | `entries`, `isConnected` |
| 全局应用状态 | `stores/` | `projects`, `currentProjectId` |
| 服务端缓存 | Store + API | `tasksByStoryId`, `workspacesByProjectId` |

派生状态使用 `useMemo` 计算，不存储在状态中。

---

## Store 清单

| Store | 职责 |
|-------|------|
| `projectStore` | Project CRUD + 选择 |
| `workspaceStore` | Workspace CRUD + 状态管理 |
| `storyStore` | Story/Task 数据 |
| `coordinatorStore` | 后端连接管理 |
| `eventStore` | SSE 事件流 |
| `workflowStore` | Workflow 管理 |
| `sessionHistoryStore` | 会话历史 |
| `settingsStore` | 全局设置 |
| `currentUserStore` | 当前用户 |
| `activeSessionsStore` | 活跃会话追踪 |
| `llmProviderStore` | LLM Provider 管理 |
| `routineStore` | Routine 管理 |
| `authStore` | 认证状态 |
| `sidebarSessionsStore` | 侧边栏会话列表 |
| `workspaceTabStore` | 工作空间标签页 |

---

## 何时使用全局 Store

1. **跨组件共享**：多个不相关组件需要访问同一份数据
2. **跨页面持久**：路由切换后仍需保持的状态
3. **服务端缓存**：从 API 获取的数据需要缓存

---

## Store 规范

- 使用 `isLoading` / `error` 追踪加载和错误状态
- API 响应通过 mapper 函数做状态值归一化（不做字段名转换）
- 按 Feature 拆分 Store，避免单个 Store 过大
- 始终通过 `set` 更新状态，不直接修改

---

## 常见错误

| 错误 | 正确做法 |
|------|----------|
| 在多个 Store 存储同一份数据 | 单一 Store 存储，其他使用 selector |
| 存储可计算数据 | 使用 `useMemo` 计算 |
| 直接修改状态 | 始终通过 `set` 更新 |
| Store 过于庞大 | 按 Feature 拆分 |
| 忘记 reset 状态 | 提供 reset action |
