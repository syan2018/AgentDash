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

## 流式 Hook 规范（SSE + Fetch NDJSON）

参考 `useAcpStream` + `streamTransport` 实现（fetch 优先，sse 回退）：

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
  // 1. 默认使用 FetchNdjsonTransport（支持 header + 自定义重连）
  // 2. NDJSON 首次连接失败时自动回退 EventSourceTransport
  // 3. 统一通过 transport 生命周期更新 isConnected/isLoading
  // 4. 清理逻辑统一走 streamRegistry（组件卸载 + HMR dispose）
}
```

### 流式 Hook 必备功能

- [ ] 连接状态追踪 (`isConnected`)
- [ ] 提供 transport 抽象，业务层不直接依赖 `EventSource`
- [ ] NDJSON transport 支持 `x-stream-since-id` 续传
- [ ] SSE transport 支持 `Last-Event-ID` 续传
- [ ] NDJSON 首次失败自动降级到 SSE
- [ ] 消息缓冲与批量刷新（避免频繁重渲染）
- [ ] 错误处理（区分连接错误和解析错误）
- [ ] 清理函数（useEffect 返回时关闭 transport）
- [ ] HMR dispose 时统一关闭所有流连接（防止连接累积）
- [ ] 手动重连方法（重新创建 EventSource 实例）

### 环境变量约定

- `VITE_API_ORIGIN`（可选）：
  - 未设置：前端请求走相对路径（通常通过 Vite proxy 到后端）
  - 设置后：请求直接拼接为 `${VITE_API_ORIGIN}/api/...`，用于绕过 dev proxy

### NDJSON Envelope 契约（前端消费）

- `connected`：
  - 字段：`last_event_id: number`
- `notification`：
  - 字段：`id: number`, `notification: SessionNotification`
- `heartbeat`：
  - 字段：`timestamp: number`

前端必须：
- 对未知 `type` 安全忽略（不抛异常中断流）
- 仅当 `type=notification` 且 payload 满足 `SessionNotification` 结构时入队

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

## ACP 事件归并契约（useAcpStream reducer）

`useAcpStream.applyNotification` 是 ACP 事件的唯一归并入口。以下是各事件类型的处理契约：

### 事件处理矩阵

| SessionUpdate 类型 | 归并策略 | 主流绘制 | 备注 |
|---|---|---|---|
| `tool_call` | **upsert**：按 `toolCallId` 反向查找，存在则覆盖，否则新建 | 是 | 对齐 Zed `upsert_tool_call` |
| `tool_call_update` | **merge**：合并到已有 entry；找不到锚点则创建孤立条目 | 是 | 孤立 update 不丢弃 |
| `agent_message_chunk` | **合并相邻**：同类型 + 同 turnId + 文本类型 → `mergeStreamChunk` | 是 | |
| `user_message_chunk` | 同上 | 是 | |
| `agent_thought_chunk` | 同上 | 是 | |
| `plan` | 直接添加新条目 | 是 | |
| `session_info_update` | 直接添加新条目 | **否**（静默） | 数据保留在 entries 中，UI 不绘制 |
| `usage_update` | 直接添加新条目 + 实时更新 `tokenUsage` state | **否**（header 圆环） | 用量通过 header 小圆环展示 |
| 其他 | 直接添加新条目 | 否 | |

### isPendingApproval 终态保护

```
status === "completed" | "failed" | "canceled" | "rejected"
  → isPendingApproval = false（覆盖）
status === "pending"
  → isPendingApproval = true
status === "in_progress"
  → isPendingApproval = false
其他
  → 保留已有值
```

### ToolCallStatus 扩展

SDK v0.14 定义 `"pending" | "in_progress" | "completed" | "failed"`。
后端可能发送 Zed 扩展状态 `"canceled" | "rejected"`。
前端使用 `ExtendedToolCallStatus` 类型兼容。

### 错误模式：tool_call_update 覆盖 approval 状态

```ts
// 错误：直接用 incoming status 设置 isPendingApproval
isPendingApproval: update.status === "pending"

// 正确：终态覆盖 + 非终态保留
if (isTerminalToolCallStatus(incomingStatus)) {
  nextPendingApproval = false;
} else if (incomingStatus === "pending") {
  nextPendingApproval = true;
} else if (incomingStatus === "in_progress") {
  nextPendingApproval = false;
}
```

---

## 参考实现

- `features/acp-session/model/useAcpStream.ts` - ACP 流管理 + 事件归并 reducer
- `features/acp-session/model/useAcpSession.ts` - 聚合逻辑 + tokenUsage 暴露
- `features/acp-session/model/streamTransport.ts` - NDJSON/SSE 双通道传输
- `stores/storyStore.ts` - Zustand Store
