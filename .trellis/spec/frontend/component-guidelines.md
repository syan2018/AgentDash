# Component Guidelines

> AgentDashboard 前端组件开发规范。

---

## 技术栈

- **框架**: React 19
- **样式**: Tailwind CSS v4
- **构建**: Vite
- **状态**: Zustand

---

## 组件结构

### 目录组织

```
frontend/src/
├── components/ui/          # 基础 UI 组件
├── components/layout/      # 布局组件
├── features/
│   └── {feature}/
│       ├── ui/             # 组件
│       ├── model/          # Hooks, types, state
│       └── index.ts        # 公开 API
└── pages/                  # 页面组件
```

### 文件模板

```tsx
/**
 * 组件功能描述
 *
 * 详细说明组件的用途和关键行为
 */

import { useState } from "react";
import type { ReactNode } from "react";

export interface MyComponentProps {
  /** 主标题 */
  title: string;
  /** 子元素 */
  children?: ReactNode;
  /** 点击回调 */
  onClick?: () => void;
}

export function MyComponent({ title, children, onClick }: MyComponentProps) {
  const [isOpen, setIsOpen] = useState(false);

  return (
    <div className="rounded-lg border p-4">
      <h2 className="text-lg font-semibold">{title}</h2>
      {children}
    </div>
  );
}

export default MyComponent;
```

---

## Props 规范

### 必选 vs 可选

```tsx
// ✅ 明确区分必选和可选
interface Props {
  sessionId: string;           // 必选：没有默认值
  className?: string;          // 可选：有默认值或可为空
  autoScroll?: boolean;        // 可选：默认 true
}

// ❌ 避免所有都可选
interface Props {
  sessionId?: string;          // 错误：核心参数不应该可选
}
```

### 默认值

```tsx
export function AcpSessionList(props: AcpSessionListProps) {
  const {
    enableAggregation = true,   // ✅ 解构时设置默认值
    className = "",
    autoScroll = true,
  } = props;
  // ...
}
```

---

## 样式规范

### Tailwind CSS 使用

```tsx
// ✅ 使用语义化类名
<div className="rounded-lg border border-border bg-card p-4 shadow-sm">

// ✅ 条件类名使用 cn 工具（需要安装 clsx + tailwind-merge）
<div className={cn("base-class", isActive && "active-class")}>

// ❌ 避免内联样式
<div style={{ padding: "16px" }}>
```

### 颜色变量

使用项目定义的颜色变量：

```tsx
// 背景
bg-card, bg-muted, bg-primary, bg-destructive

// 文字
text-foreground, text-muted-foreground, text-primary

// 边框
border-border, border-input
```

---

## Feature 组件模式

每个 feature 遵循统一结构：

```
features/acp-session/
├── ui/
│   ├── AcpSessionList.tsx    # 列表组件
│   ├── AcpSessionEntry.tsx   # 条目组件
│   └── index.ts              # 统一导出
├── model/
│   ├── types.ts              # 类型定义
│   ├── useAcpSession.ts      # 业务 Hook
│   ├── useAcpStream.ts       # 基础 Hook
│   └── index.ts
└── index.ts                  # Feature 入口
```

### Feature 入口文件

```ts
// features/acp-session/index.ts
export * from "./ui";
export * from "./model";
```

---

## 常见错误

| 错误 | 正确 |
|------|------|
| 在组件内定义新组件 | 在文件外定义或使用 useMemo |
| props 层层传递（prop drilling） | 使用 Context 或 Store |
| 组件过于庞大 | 拆分为小组件 |
| 混合适配逻辑和渲染 | 分离 model（逻辑）和 ui（渲染） |

---

## 示例：完整 Feature 组件

参考 `features/acp-session/ui/AcpSessionList.tsx`：

- 使用 TypeScript interface 定义 props
- 文档注释说明组件用途
- 从 model 导入业务逻辑
- 支持自定义渲染（renderItem）
- 提供合理的默认值
