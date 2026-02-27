# State Management

> How state is managed in this project.

---

## Overview

使用 **Zustand** 进行全局状态管理，React useState/useReducer 处理本地状态。

- **本地状态**: 组件内 useState，如 `isOpen`, `selectedTab`
- **Feature 状态**: Feature model 目录下的 Hooks，如 `useAcpSession`
- **全局状态**: `stores/` 目录下的 Zustand Store，如 `useStoryStore`
- **服务端缓存**: Store + API，如 `tasksByStoryId`

派生状态使用 `useMemo` 计算，不存储在状态中。参考 `features/acp-session/model/useAcpSession.ts` 中的 `displayItems` 聚合逻辑。

---

## State Categories

| 类型 | 存放位置 | 示例 |
|------|----------|------|
| 本地 UI 状态 | 组件内 useState | `isOpen`, `selectedTab`, `showCreate` |
| Feature 状态 | Feature model | `entries`, `isConnected` |
| 全局应用状态 | stores/ | `projects`, `backends`, `currentProjectId` |
| 服务端缓存 | Store + API | `tasksByStoryId`, `workspacesByProjectId` |

---

## When to Use Global State

将状态提升到全局 Store 的条件：

1. **跨组件共享**: 多个不相关组件需要访问同一份数据
2. **跨页面持久**: 路由切换后仍需保持的状态
3. **服务端缓存**: 从 API 获取的数据需要缓存

示例：
- `projects` + `currentProjectId` - 全局项目选择
- `workspacesByProjectId` - 按项目缓存工作空间
- `stories` - 当前项目的 Story 列表
- `backends` - 全局后端连接配置
- `tasksByStoryId` - 缓存避免重复请求

---

## Server State

服务端数据使用 Zustand Store 缓存：

```ts
// stores/storyStore.ts — 按 projectId 获取 Story
fetchStoriesByProject: async (projectId) => {
  set({ isLoading: true, error: null });
  try {
    const response = await api.get(`/stories?project_id=${projectId}`);
    const stories = response.map(mapStory);
    set({ stories, isLoading: false });
  } catch (e) {
    set({ error: (e as Error).message, isLoading: false });
  }
}

// stores/workspaceStore.ts — 按 projectId 缓存 Workspace
fetchWorkspaces: async (projectId) => {
  set({ isLoading: true, error: null });
  try {
    const workspaces = await api.get(`/projects/${projectId}/workspaces`);
    set((s) => ({
      workspacesByProjectId: { ...s.workspacesByProjectId, [projectId]: workspaces },
      isLoading: false,
    }));
  } catch (e) {
    set({ error: (e as Error).message, isLoading: false });
  }
}
```

### Store 清单

| Store | 职责 | 关键状态 |
|-------|------|----------|
| `projectStore` | Project CRUD + 选择 | `projects`, `currentProjectId` |
| `workspaceStore` | Workspace CRUD + 状态管理 | `workspacesByProjectId` |
| `storyStore` | Story/Task 数据 | `stories`, `tasksByStoryId` |
| `coordinatorStore` | 后端连接管理 | `backends`, `currentBackendId` |
| `eventStore` | SSE 事件流 | `connectionState`, `lastEventId` |

- 使用 `isLoading` 追踪加载状态
- API 响应通过 `mapStory`/`mapTask` 等函数做状态值归一化
- 错误信息存储在 `error` 字段

---

## Common Mistakes

| 错误 | 正确做法 |
|------|----------|
| 在多个 Store 存储同一份数据 | 单一 Store 存储，其他使用 selector |
| 存储可计算数据 | 使用 useMemo 计算，如 `displayItems` |
| 直接修改状态 | 始终通过 set 更新 |
| Store 过于庞大 | 按 Feature 拆分 Store |
| 忘记 reset 状态 | 提供 reset action 清理状态 |
