# Workflow Binding Panels Executable Plan

## 模块边界

本轮只读扫描 workflow frontend panels / binding editor。除 `auto_granted baseline` 事实源外，其余问题均可按前端模块快速修复，不需要架构级大拆。

## 证据

- `lifecycle-editor-shell.tsx` 同时负责加载、保存、快捷键、离开保护、画布、sidebar 路由和顶层表单。
- `SidebarRouterProps` 穿透大量 store action，并多处使用 `ReturnType<typeof useWorkflowStore.getState>` 暴露 store 内部形状。
- `CapabilityPanel.tsx` 的 `CapabilitiesEditor` 同时做 UI、MCP preset 拉取、tool catalog 拉取、probe store 调用、baseline/extra capability 归类和 directive 写操作。
- `types/workflow.ts` 的 `WELL_KNOWN_CAPABILITY_KEYS` 与 `ui/panels/shared.ts` 的 `CAP_EDITOR_WELL_KNOWN_KEYS` 重复。
- `lifecycle-editor-shell.tsx` 局部实现 target kind toggle，而 `ui/panels/shared.ts` 已有 `toggleTargetKind`。
- `types/workflow.ts` 对 generated `CapabilityConfig` 做同字段重声明，容易成为 generated DTO identity rebuild。
- `compact` prop 已标记 deprecated，但仍在 `binding-editor.tsx`、`InjectionPanel.tsx`、`HookRulesPanel.tsx`、`PortsPanel.tsx`、`CapabilityPanel.tsx` 保留。

## 可执行批次

### Batch A: 删除旧兼容与局部重复 helper

- 写入：`binding-editor.tsx`、`InjectionPanel.tsx`、`HookRulesPanel.tsx`、`PortsPanel.tsx`、`CapabilityPanel.tsx`、`lifecycle-editor-shell.tsx`。
- 内容：删除 deprecated `compact` prop 链路；统一复用 `toggleTargetKind`。
- 风险：低到中；主要是调用点类型错误。
- 验证：`pnpm --filter app-web run typecheck`。

### Batch B: 收敛 generated DTO 包装与枚举选项

- 写入：`types/workflow.ts`、`ArtifactBindingsEditor.tsx`、`PortsPanel.tsx`、相关测试。
- 内容：`WorkflowCapabilityConfig` 尽量退回 alias；alias/port option 做成 typed option helper，减少 enum cast。
- 风险：中；可能暴露旧包装类型依赖。
- 验证：`pnpm --filter app-web test -- src/features/workflow/capability-directive-ops.test.ts src/features/workflow/ui/panels/panels.test.tsx`；`pnpm --filter app-web run typecheck`。

### Batch C: 拆 LifecycleEditorShell sidebar 组合边界

- 写入：`lifecycle-editor-shell.tsx`，可新增 `ui/lifecycle-sidebar-router.tsx` 或 feature model hook。
- 内容：让 shell 只负责页面级装配，sidebar props 收敛成 `draft + selection + actions`。
- 风险：中；影响 activity/transition inspector 回调绑定。
- 验证：`pnpm --filter app-web run typecheck`。

## 架构项

`auto_granted baseline` 事实源应进入架构 backlog：前端 `AUTO_GRANTED_BASELINE` 镜像后端 `default_visibility_rules`，正确方向是后端/contract 输出 visibility baseline 或 capability projection，前端不手写镜像。
