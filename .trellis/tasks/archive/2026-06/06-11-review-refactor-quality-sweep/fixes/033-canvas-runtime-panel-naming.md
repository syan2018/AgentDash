# Fix 033: canvas runtime panel naming

## 模块

- canvas-runtime-preview

## 问题

`canvas-runtime-preview` narrow review 发现 `CanvasSessionPanel` 会被 Project 级 Canvas preview 以 `sessionId=null` 使用，命名把 runtime preview 面板误收窄为 session panel。

## 更新

- 将 `CanvasSessionPanel` 改名为 `CanvasRuntimePanel`。
- 将 props 类型同步改为 `CanvasRuntimePanelProps`。
- 更新 Project Canvas manager、canvas workspace tab 与 feature barrel export，调用点统一消费中性 runtime panel。
- 不保留旧 `CanvasSessionPanel` 兼容导出。

## 涉及文件

- `packages/app-web/src/features/canvas-panel/CanvasRuntimePanel.tsx`
- `packages/app-web/src/features/canvas-panel/ProjectCanvasManager.tsx`
- `packages/app-web/src/features/canvas-panel/index.ts`
- `packages/app-web/src/features/workspace-panel/tab-types/canvas-tab.tsx`

## 验证

- `pnpm --filter app-web run typecheck`
- `pnpm --filter app-web test -- CanvasRuntimePreview`
- `git diff --check`

## Commit

- `e2ac5d35`：`refactor(canvas): 收窄运行时面板命名`
