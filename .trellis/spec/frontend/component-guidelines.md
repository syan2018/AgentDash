# Component Guidelines

> 前端组件开发规范。

---

## 组件规范

- 组件 props 使用 TypeScript interface 定义，明确必选/可选
- 可选 props 在解构时设置默认值
- 导出方式：`export function MyComponent`（具名导出优先）
- 禁止在组件内定义新组件（提取到文件外或用 `useMemo`）
- model 层（逻辑）和 ui 层（渲染）分离

---

## 样式规范

使用 Tailwind CSS v4，禁止内联样式。条件类名使用 `cn`（clsx + tailwind-merge）。

### 项目颜色变量

```
背景：bg-card, bg-muted, bg-primary, bg-destructive
文字：text-foreground, text-muted-foreground, text-primary
边框：border-border, border-input
```

---

## 常见错误

| 错误 | 正确 |
|------|------|
| 在组件内定义新组件 | 在文件外定义或使用 useMemo |
| props 层层传递 | 使用 Context 或 Store |
| 组件过于庞大 | 拆分为小组件 |
| 混合适配逻辑和渲染 | 分离 model 和 ui |
