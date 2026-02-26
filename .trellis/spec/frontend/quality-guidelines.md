# Quality Guidelines

> Code quality standards for frontend development.

---

## Overview

- **Linting**: ESLint + typescript-eslint
- **构建**: TypeScript 严格模式
- **检查命令**: `npm run lint` (frontend), `cargo check` (backend)

代码提交前必须通过 lint 检查。

---

## Forbidden Patterns

| 禁止模式 | 原因 | 替代方案 |
|----------|------|----------|
| `any` 类型 | 失去类型安全 | 使用 `unknown` 并做类型守卫 |
| `!` 非空断言 | 运行时可能出错 | 先做 null 检查 |
| 内联样式 `style={{}}` | 难以维护 | 使用 Tailwind 类名 |
| 直接修改 props | 违反 React 原则 | 使用回调或状态提升 |

---

## Required Patterns

- 组件 props 使用 TypeScript interface 定义
- Hook 返回类型显式声明
- API 响应使用映射函数转换（snake_case → camelCase）
- 错误边界处理网络请求错误

---

## Testing Requirements

当前以手动测试为主。关键功能需要：

1. 基础功能测试（输入、点击、提交）
2. 错误场景测试（网络断开、服务端错误）
3. 边界情况测试（空输入、超长内容）

---

## Code Review Checklist

- [ ] 类型定义完整，无 `any`
- [ ] 错误处理完善
- [ ] Props 有文档注释
- [ ] 无 console.log 调试代码
- [ ] 组件拆分合理
