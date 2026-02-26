# Shared Guidelines

> 前后端共享的开发规范。

---

## 语言要求

> **必须使用中文**

- 所有与用户的交流必须使用中文
- 所有文档更新必须使用中文
- 代码注释必须使用中文
- 提交信息必须使用中文

---

## 命名规范

### 文件命名

| 类型 | 规范 | 示例 |
|------|------|------|
| 组件 | PascalCase | `AcpSessionList.tsx` |
| Hook | camelCase, use前缀 | `useAcpSession.ts` |
| Store | camelCase, use前缀 | `useStoryStore.ts` |
| 工具函数 | camelCase | `formatDate.ts` |
| 类型定义 | camelCase | `types.ts` |

### 变量命名

```typescript
// ✅ 正确
const sessionId: string;
const isConnected: boolean;
const displayItems: AcpDisplayItem[];

// ❌ 错误
const session_id: string;  // 不使用 snake_case
const connected: boolean;  // 布尔值不使用 is/has 前缀
```

---

## 代码注释规范

```typescript
/**
 * ACP 会话管理 Hook
 *
 * 整合流管理和条目聚合逻辑
 */
export function useAcpSession(options: UseAcpSessionOptions): UseAcpSessionResult {
  // 实现...
}

// 单行注释用于简单说明
const shouldScrollRef = useRef(true); // 控制是否自动滚动
```

---

## 错误处理原则

1. **显式错误**：不要吞掉错误，要传递或展示
2. **用户友好**：错误信息要对用户有意义
3. **日志记录**：系统级错误需要记录日志

---

## 相关规范

- [Frontend Guidelines](../frontend/index.md)
- [Backend Guidelines](../backend/index.md)
