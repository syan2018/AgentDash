# Component Guidelines

## Extension Interaction Component V1

第三方 Interaction component 使用版本化 descriptor + isolated iframe + scoped MessageChannel：

```text
ComponentDescriptorV1 {
  component_ref,
  artifact_digest,
  entry,
  props_schema,
  events,
  state_projection_schema,
  slots,
  sizing,
  resource_policy,
  csp
}
```

- instance 固定 exact artifact digest；安装升级只影响新 definition/new instance，既有 renderer 不静默切换。
- component 只接收 props、只读 state projection 和 instance-scoped host capability；typed event 经 schema validation 后交给 definition binding。
- canonical mutation 只能进入版本化 platform command（V1 通用为 `state_patch_v1`）；外部行为进入 canonical Operation/OperationScript。component/Extension 不提供 reducer code。
- iframe 使用专属 MessagePort、CSP、origin/resource allowlist、payload/频率限制与 lease；不得共享全局 `window.agentdash` authority，也不得把 browser 提交的 Project/backend/session/capability 当作可信事实。
- resize/focus/theme/a11y/presentation 属于 renderer protocol；component unmount/lease expiry 不关闭 InteractionInstance。

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
