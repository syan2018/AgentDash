# Type Safety

> AgentDashboard 前端类型安全规范。

---

## 技术栈

- **TypeScript**: ~5.9.3
- **严格模式**: 已启用
- **运行时验证**: 手动映射 API 响应

---

## 类型组织

### 目录结构

```
frontend/src/
├── types/
│   └── index.ts           # 全局共享类型
├── features/
│   └── {feature}/
│       └── model/
│           └── types.ts   # Feature 私有类型
└── services/
    └── executor.ts        # API 相关类型
```

### 类型分类

| 位置 | 用途 | 示例 |
|------|------|------|
| `types/index.ts` | 跨 Feature 共享 | `Story`, `Task`, `Backend` |
| `features/X/model/types.ts` | Feature 内部 | `AcpDisplayEntry` |
| `services/*.ts` | API 相关 | `ExecutorConfig` |
| `Component.tsx` | 组件 Props | `AcpSessionListProps` |

---

## 类型定义规范

### Interface vs Type

```ts
// ✅ Interface：对象结构、可扩展
interface Story {
  id: string;
  title: string;
}

// ✅ Type：联合类型、工具类型
type StoryStatus = 'draft' | 'ready' | 'running';

// ✅ Type：复杂转换
type StoryInput = Omit<Story, 'id' | 'createdAt'>;
```

### Props 类型

```tsx
// ✅ 导出 interface
export interface AcpSessionListProps {
  sessionId: string;
  endpoint?: string;
  className?: string;
  onError?: (error: Error) => void;
}

export function AcpSessionList(props: AcpSessionListProps) {
  // ...
}
```

### Hook 返回类型

```ts
export interface UseAcpSessionResult {
  displayItems: AcpDisplayItem[];
  isConnected: boolean;
  isLoading: boolean;
  error: Error | null;
  reconnect: () => void;
}

export function useAcpSession(options: UseAcpSessionOptions): UseAcpSessionResult {
  // ...
}
```

---

## API 类型映射

后端返回 snake_case，前端使用 camelCase：

```ts
// 后端返回
interface BackendStory {
  id: string;
  title: string;
  created_at: string;  // snake_case
  task_ids: string[];  // snake_case
}

// 前端类型
interface Story {
  id: string;
  title: string;
  createdAt: string;   // camelCase
  taskIds: string[];   // camelCase
}

// 映射函数
function mapStory(raw: Record<string, unknown>): Story {
  return {
    id: String(raw.id ?? ''),
    title: String(raw.title ?? '未命名 Story'),
    createdAt: String(raw.created_at ?? new Date().toISOString()),
    taskIds: Array.isArray(raw.task_ids)
      ? raw.task_ids.map((id) => String(id))
      : [],
  };
}
```

---

## 类型守卫

```ts
// 类型守卫函数
export function isAggregatedGroup(item: AcpDisplayItem): item is AggregatedEntryGroup {
  return item.type === 'aggregated_group';
}

export function isAggregatedThinkingGroup(item: AcpDisplayItem): item is AggregatedThinkingGroup {
  return item.type === 'aggregated_thinking';
}

// 使用
if (isAggregatedGroup(item)) {
  // TypeScript 知道 item 是 AggregatedEntryGroup
  console.log(item.groupKey);
}
```

---

## 运行时安全

### API 响应处理

```ts
// ✅ 显式类型转换 + 验证
const mapTask = (raw: Record<string, unknown>): Task => {
  return {
    id: String(raw.id ?? ''),
    title: String(raw.title ?? '未命名 Task'),
    status: normalizeTaskStatus(String(raw.status ?? 'pending')),
    artifacts: Array.isArray(raw.artifacts)
      ? raw.artifacts as Task['artifacts']
      : [],
  };
};

// 使用
const response = await api.get<Record<string, unknown>[]>('/tasks');
const tasks = response.map(mapTask);
```

---

## 禁止模式

```ts
// ❌ 禁止使用 any
const data: any = await api.get('/data');

// ❌ 禁止类型断言（除非必要）
const data = await api.get('/data') as SomeType;

// ❌ 禁止非空断言
const item = items.find(x => x.id === id)!;

// ✅ 正确
const data: unknown = await api.get('/data');
const item = items.find(x => x.id === id);
if (!item) throw new Error('Not found');
```

---

## 常用类型工具

```ts
// 从数组提取元素类型
type ElementType<T> = T extends (infer U)[] ? U : never;

// 可选字段
type PartialBy<T, K extends keyof T> = Omit<T, K> & Partial<Pick<T, K>>;

// 示例：创建 Story 时 id 可选
type CreateStoryInput = PartialBy<Story, 'id' | 'createdAt'>;
```

---

## 参考类型定义

- `frontend/src/types/index.ts` - Story, Task 等核心类型
- `frontend/src/features/acp-session/model/types.ts` - ACP 相关类型
- `frontend/src/services/executor.ts` - ExecutorConfig 类型
