# 状态管理

> AgentDashboard 前端状态管理规范。

---

## 概览

使用 **Zustand 5** 进行全局状态管理，React `useState`/`useReducer` 处理本地状态。

| 状态类型 | 存放位置 | 示例 |
|----------|----------|------|
| 本地 UI 状态 | 组件内 `useState` | `isOpen`, `selectedTab`, `showCreate` |
| Feature 状态 | Feature `model/` hooks | `entries`, `isConnected` |
| 全局应用状态 | `stores/` | `projects`, `currentProjectId` |
| 服务端缓存 | Store + API | `tasksByStoryId`, `workspacesByProjectId` |

派生状态使用 `useMemo` 计算，不存储在状态中。

---

## Store 清单

| Store | 职责 | 关键状态 |
|-------|------|----------|
| `projectStore` | Project CRUD + 选择 | `projects`, `currentProjectId` |
| `workspaceStore` | Workspace CRUD + 状态管理 | `workspacesByProjectId` |
| `storyStore` | Story/Task 数据 | `stories`, `tasksByStoryId` |
| `coordinatorStore` | 后端连接管理 | `backends`, `currentBackendId` |
| `eventStore` | SSE 事件流 | `connectionState`, `lastEventId` |
| `workflowStore` | Workflow 管理 | workflow 定义、lifecycle、binding |
| `sessionHistoryStore` | 会话历史 | 会话列表、加载状态 |
| `settingsStore` | 全局设置 | LLM API keys、系统配置 |
| `currentUserStore` | 当前用户 | 用户信息 |
| `activeSessionsStore` | 活跃会话追踪 | 运行中的 session 列表 |

---

## 何时使用全局 Store

将状态提升到全局 Store 的条件：

1. **跨组件共享**: 多个不相关组件需要访问同一份数据
2. **跨页面持久**: 路由切换后仍需保持的状态
3. **服务端缓存**: 从 API 获取的数据需要缓存

---

## Store 模式示例

```ts
// stores/storyStore.ts — 按 projectId 获取 Story
import { create } from 'zustand';

interface StoryState {
  stories: Story[];
  isLoading: boolean;
  error: string | null;
  fetchStoriesByProject: (projectId: string) => Promise<void>;
}

export const useStoryStore = create<StoryState>((set) => ({
  stories: [],
  isLoading: false,
  error: null,

  fetchStoriesByProject: async (projectId) => {
    set({ isLoading: true, error: null });
    try {
      const response = await api.get(`/stories?project_id=${projectId}`);
      const stories = response.map(mapStory);
      set({ stories, isLoading: false });
    } catch (e) {
      set({ error: (e as Error).message, isLoading: false });
    }
  },
}));
```

```ts
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

---

## Store 规范

- 使用 `isLoading` 追踪加载状态
- API 响应通过 mapper 函数做状态值归一化（不做字段名转换）
- 错误信息存储在 `error` 字段
- 按 Feature 拆分 Store，避免单个 Store 过大

---

## 常见错误

| 错误 | 正确做法 |
|------|----------|
| 在多个 Store 存储同一份数据 | 单一 Store 存储，其他使用 selector |
| 存储可计算数据 | 使用 `useMemo` 计算 |
| 直接修改状态 | 始终通过 `set` 更新 |
| Store 过于庞大 | 按 Feature 拆分 Store |
| 忘记 reset 状态 | 提供 reset action 清理状态 |

---

*更新：2026-03-29 — 对齐实际 10 个 Store 清单，示例改用 projectId 驱动*
