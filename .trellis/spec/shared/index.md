# 前后端共享规范

> 前后端共同遵守的开发规范。

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

| 类型 | 前端规范 | 后端规范 | 示例 |
|------|---------|---------|------|
| 组件 | PascalCase `.tsx` | - | `AcpSessionList.tsx` |
| Hook | camelCase, `use` 前缀 | - | `useAcpSession.ts` |
| Store | camelCase | - | `workflowStore.ts` |
| Service | camelCase | - | `workflow.ts` |
| 工具函数 | camelCase | snake_case | `formatDate.ts` / `format_date.rs` |
| 类型定义 | `types.ts` / `index.ts` | `value_objects.rs` | - |
| 实体 | - | PascalCase struct | `Story`, `Task` |
| Repository | - | `<Entity>Repository` | `StoryRepository` |

### 变量/字段命名

| 语言 | 规范 | 示例 |
|------|------|------|
| TypeScript | camelCase（变量）、snake_case（API 字段） | `const sessionId`; `story.project_id` |
| Rust | snake_case | `session_id`, `project_repo` |
| HTTP JSON | snake_case | `{ "project_id": "...", "created_at": "..." }` |

---

## 代码注释规范

```typescript
/**
 * ACP 会话管理 Hook
 *
 * 整合流管理和条目聚合逻辑
 */
export function useAcpSession(options: UseAcpSessionOptions): UseAcpSessionResult {
  // ...
}
```

---

## 错误处理原则

1. **显式错误**：不要吞掉错误，要传递或展示
2. **用户友好**：错误信息要对用户有意义
3. **日志记录**：系统级错误需要记录日志
4. **分层错误**：每层使用自己的错误类型，在边界转换

---

## 相关规范

- [前端开发指南](../frontend/index.md)
- [后端开发指南](../backend/index.md)
- [沟通规范](../communication.md)

---

*更新：2026-03-29 — 充实命名规范，增加 HTTP JSON 字段命名约定*
