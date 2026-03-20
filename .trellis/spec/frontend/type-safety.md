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

> **设计决策（v0.2）**：前端类型直接使用 **snake_case** 与后端对齐，不做 camelCase 转换。
> 原因：减少映射层复杂度，避免序列化/反序列化不一致。

```ts
// 前端类型直接使用 snake_case（与后端 Rust 实体一致）
interface Story {
  id: string;
  project_id: string;     // snake_case，与后端一致
  backend_id: string;
  title: string;
  status: StoryStatus;
  context: StoryContext;   // 结构化上下文
  created_at: string;
  updated_at: string;
}

// 映射函数仅做状态值归一化，不做字段名转换
const mapStory = (raw: Record<string, unknown>): Story => {
  return {
    id: String(raw.id ?? ''),
    project_id: String(raw.project_id ?? ''),
    backend_id: String(raw.backend_id ?? ''),
    title: String(raw.title ?? '未命名 Story'),
    status: normalizeStoryStatus(String(raw.status ?? 'draft')),
    context: parseStoryContext(raw.context),
    created_at: String(raw.created_at ?? new Date().toISOString()),
    updated_at: String(raw.updated_at ?? new Date().toISOString()),
  };
};
```

### 业务 API 映射边界

- 前端业务类型以 `frontend/src/types/index.ts` 为准，字段名统一 `snake_case`
- store/service mapper 只负责：
  - `unknown -> typed object`
  - 状态值归一化
  - null / array / number 等基础运行时校验
- store/service mapper 不负责：
  - 同时兼容 `camelCase` + `snake_case`
  - 猜测后端字段别名
  - 用双字段读取掩盖后端 DTO 契约错误

```ts
// ✅ 正确：只接受规范字段
const mapSessionBinding = (raw: Record<string, unknown>): SessionBinding => ({
  id: String(raw.id ?? ''),
  session_id: String(raw.session_id ?? ''),
  owner_type: String(raw.owner_type ?? 'story') as SessionBinding['owner_type'],
  owner_id: String(raw.owner_id ?? ''),
  created_at: String(raw.created_at ?? new Date().toISOString()),
});

// ❌ 错误：把后端契约漂移长期固化在前端
const mapSessionBinding = (raw: Record<string, unknown>): SessionBinding => ({
  session_id: String(raw.sessionId ?? raw.session_id ?? ''),
  owner_type: String(raw.ownerType ?? raw.owner_type ?? 'story') as SessionBinding['owner_type'],
});
```

当 mapper 开始出现 `fooBar ?? foo_bar` 时，应回到后端 DTO 修复，而不是继续扩写前端兼容层。

### 状态值归一化

后端可能返回旧版状态名，前端映射函数负责归一化：

```ts
const normalizeStoryStatus = (value: string): StoryStatus => {
  switch (value) {
    case 'created': case 'draft': return 'draft';
    case 'context_ready': case 'decomposed': case 'ready': return 'ready';
    case 'executing': return 'running';
    case 'awaiting_verification': return 'review';
    // ...
    default: return 'draft';
  }
};
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

- `frontend/src/types/index.ts` - Project, Workspace, Story, Task 等核心类型
- `frontend/src/features/acp-session/model/types.ts` - ACP 相关类型
- `frontend/src/services/executor.ts` - ExecutorConfig 类型

---

## 架构演进记录

### v0.2 — snake_case 直接映射

**变更**：前端类型从 camelCase 切换到 snake_case，与后端 Rust 实体直接对齐。

**影响范围**：
- `types/index.ts` 中所有字段名使用 snake_case
- 组件中访问字段如 `story.created_at`（非 `story.createdAt`）
- Store 映射函数不再做字段名转换

**原因**：
1. 减少映射层代码量和出错概率
2. 新增 Project/Workspace 等多个实体后，camelCase 转换维护成本过高
3. TypeScript 类型与 API JSON 响应直接匹配，调试更直观
