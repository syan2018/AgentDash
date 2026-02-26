# Hook Guidelines

> AgentDashboard 前端 Hook 开发规范。

---

## Hook 分类

### 1. 基础 Hooks（复用逻辑）

位于 `src/hooks/`，提供通用功能：

```ts
// hooks/use-theme.ts
export function useTheme() {
  // 主题切换逻辑
}
```

### 2. Feature Hooks（业务逻辑）

位于 `features/{feature}/model/`，封装特定业务：

```ts
// features/acp-session/model/useAcpSession.ts
export function useAcpSession(options: UseAcpSessionOptions): UseAcpSessionResult {
  // ACP 会话管理逻辑
}
```

---

## 命名规范

| 类型 | 命名 | 示例 |
|------|------|------|
| 基础 Hook | use + 功能 | `useTheme`, `useLocalStorage` |
| 业务 Hook | use + 领域 | `useAcpSession`, `useStoryStore` |
| 流 Hook | use + Stream | `useAcpStream` |

---

## Hook 结构模板

```ts
/**
 * Hook 功能描述
 *
 * 详细说明用途、参数和返回值
 */

import { useState, useEffect, useCallback } from "react";

export interface UseXOptions {
  /** 描述 */
  key: string;
}

export interface UseXResult {
  /** 描述 */
  data: T;
  /** 是否加载中 */
  isLoading: boolean;
  /** 错误信息 */
  error: Error | null;
  /** 重新获取 */
  refetch: () => void;
}

export function useX(options: UseXOptions): UseXResult {
  // 1. 状态定义
  const [data, setData] = useState<T | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<Error | null>(null);

  // 2. 回调函数使用 useCallback
  const fetchData = useCallback(async () => {
    setIsLoading(true);
    try {
      const result = await api.fetch(options.key);
      setData(result);
    } catch (e) {
      setError(e as Error);
    } finally {
      setIsLoading(false);
    }
  }, [options.key]);

  // 3. 副作用使用 useEffect
  useEffect(() => {
    fetchData();
  }, [fetchData]);

  // 4. 返回结果
  return {
    data,
    isLoading,
    error,
    refetch: fetchData,
  };
}
```

---

## SSE Hook 规范

参考 `useAcpStream` 实现（使用 EventSource）：

```ts
export interface UseStreamOptions {
  sessionId: string;
  endpoint?: string;
  onEntry?: (entry: Entry) => void;
  onError?: (error: Error) => void;
}

export interface UseStreamResult {
  entries: Entry[];
  isConnected: boolean;
  isLoading: boolean;
  error: Error | null;
  reconnect: () => void;
  close: () => void;
}

export function useAcpStream(options: UseStreamOptions): UseStreamResult {
  // 1. 使用 EventSource 建立 SSE 连接
  // 2. 消息聚合（chunk 合并）
  // 3. EventSource 自动重连（通过 Last-Event-ID）
  // 4. 清理逻辑（组件卸载时关闭 EventSource）
}
```

### SSE Hook 必备功能

- [ ] 连接状态追踪 (`isConnected`)
- [ ] 使用 `EventSource` API（浏览器原生 SSE 客户端）
- [ ] 支持 `Last-Event-ID` 实现断线续传
- [ ] 消息缓冲与批量刷新（避免频繁重渲染）
- [ ] 错误处理（区分连接错误和解析错误）
- [ ] 清理函数（useEffect 返回时关闭 EventSource）
- [ ] 手动重连方法（重新创建 EventSource 实例）

---

## Store Hook 规范

使用 Zustand 创建 Store：

```ts
// stores/storyStore.ts
import { create } from 'zustand';

interface StoryState {
  // 状态
  stories: Story[];
  isLoading: boolean;

  // 动作
  fetchStories: (backendId: string) => Promise<void>;
  createStory: (backendId: string, title: string) => Promise<void>;
}

export const useStoryStore = create<StoryState>((set) => ({
  stories: [],
  isLoading: false,

  fetchStories: async (backendId) => {
    set({ isLoading: true });
    try {
      const response = await api.get(`/stories?backend_id=${backendId}`);
      set({ stories: response, isLoading: false });
    } catch (e) {
      set({ error: (e as Error).message, isLoading: false });
    }
  },
  // ...
}));
```

---

## 常见错误

| 错误 | 正确 |
|------|------|
| 忘记 useCallback 导致无限重渲染 | 依赖变化的回调使用 useCallback |
| useEffect 缺少依赖项 | 完整填写依赖数组 |
| Hook 条件调用 | 始终在组件顶层调用 Hook |
| 返回过多状态 | 使用对象返回，方便解构 |

---

## 参考实现

- `features/acp-session/model/useAcpStream.ts` - SSE 流管理（使用 EventSource）
- `features/acp-session/model/useAcpSession.ts` - 业务逻辑封装
- `stores/storyStore.ts` - Zustand Store
