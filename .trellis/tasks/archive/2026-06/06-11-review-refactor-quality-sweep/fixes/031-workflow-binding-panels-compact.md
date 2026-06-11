# Fix 031: workflow binding panels compact cleanup

## 模块

- workflow-binding-panels

## 问题

`workflow-binding-panels` narrow review 发现多个 panel 仍保留 deprecated `compact` prop 链路；`lifecycle-editor-shell.tsx` 也有一份局部 target kind toggle，与 `ui/panels/shared.ts` 中已有 helper 重复。

## 更新

- 删除 `BindingEditor`、`InjectionPanel`、`CapabilityPanel`、`HookRulesPanel`、`PortsPanel` 中的 `compact` prop 定义和空消费。
- 清理 activity inspector 对这些 panel 的 `compact` 传递。
- 在 lifecycle editor header 中复用 `toggleTargetKind`，保持至少保留一个 target kind 的语义集中到共享 helper。

## 涉及文件

- `packages/app-web/src/features/workflow/binding-editor.tsx`
- `packages/app-web/src/features/workflow/lifecycle-editor-shell.tsx`
- `packages/app-web/src/features/workflow/ui/activity-inspector-sections.tsx`
- `packages/app-web/src/features/workflow/ui/panels/CapabilityPanel.tsx`
- `packages/app-web/src/features/workflow/ui/panels/HookRulesPanel.tsx`
- `packages/app-web/src/features/workflow/ui/panels/InjectionPanel.tsx`
- `packages/app-web/src/features/workflow/ui/panels/PortsPanel.tsx`

## 验证

- `pnpm --filter app-web run typecheck`
- `pnpm --filter app-web test -- src/features/workflow/ui/panels/panels.test.tsx`
- `git diff --check`

## Commit

- `b0df9ce4`：`refactor(workflow): 清理绑定面板旧 compact 链路`
