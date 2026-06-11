# Canvas Runtime Preview Executable Plan

## 模块边界

本轮只读扫描 `canvas-panel` 前端 runtime preview / panel 链路，并只用 API canvas DTO 确认前后端语义重复。未发现明确 `legacy` / `compat` 双链路，主要问题是命名、组件职责和 DTO 事实源。

## 证据

- `CanvasSessionPanel` 被 `ProjectCanvasManager` 以 `sessionId={null}` 使用，命名不是纯 session panel。
- `CanvasSessionPanel` 同时负责 canvas/runtime snapshot 拉取、bindings 保存、preview 渲染、底部详情和编辑器装配。
- `CanvasRuntimePreview` 内联 iframe postMessage DTO、type guards、runtime invoke、VFS asset URL、extension channel 和生命周期。
- `CanvasRuntimePreview.runtime.ts` 同时包含 asset URL cache、VFS URI parser、preview document builder、sandbox boot protocol、TS transpile、import rewrite。
- runtime snapshot 已复用 generated contract，但 Canvas CRUD 仍有前端手写 `CanvasFile` / `CanvasDataBinding` / `Canvas` 与后端 API-local `CreateCanvasRequest` / `UpdateCanvasRequest` / `CanvasResponse`。
- 前端根据 `snapshot.runtime_bridge.enabled` 和 `surface.actions` 判断 action 可见性，后端 route 仍构造 `RuntimeActor` / `RuntimeContext` 进入 gateway；这是可见性二次解释，当前先作为快速修复证据保留。

## 可执行批次

### Batch A: 拆 CanvasRuntimePreview iframe bridge/protocol

- 写入：`CanvasRuntimePreview.tsx`，新增 `CanvasRuntimePreview.protocol.ts`、`CanvasRuntimePreview.bridge.ts` 或同等窄模块。
- 内容：移动 envelope 类型、type guards、postMessage result sender、runtime/asset/extension handler 组装；组件只保留 iframe 生命周期和 UI 状态。
- 风险：中；`frame_id` 与 generation 清理容易回归。
- 验证：`pnpm --filter app-web test -- CanvasRuntimePreview`；`pnpm --filter app-web run typecheck`。

### Batch B: 拆 runtime builder 内部职责

- 写入：`CanvasRuntimePreview.runtime.ts`，新增 `canvasRuntimeAssets.ts`、`canvasRuntimeModuleBuilder.ts`、`canvasRuntimeSandboxDocument.ts`。
- 内容：VFS asset URL/cache、module transpile/rewrite、sandbox HTML/boot script 分离；保留 `buildPreviewDocument()` 门面。
- 风险：中；blob URL dispose、CSS 注入、import rewrite 需要保持。
- 验证：`pnpm --filter app-web test -- CanvasRuntimePreview`；`pnpm --filter app-web run typecheck`。

### Batch C: 收窄 panel 命名语义

- 写入：`CanvasSessionPanel.tsx`、`ProjectCanvasManager.tsx`、canvas tab 调用点。
- 内容：把 `CanvasSessionPanel` 改为 `CanvasRuntimePanel` 或拆出 `CanvasRuntimePanel` + bindings 区域，让 project preview 的 `sessionId=null` 不再落在 SessionPanel 命名下。
- 风险：低到中；主要是 import/export 调整。
- 验证：`pnpm --filter app-web run typecheck`。

## 架构项

Canvas CRUD DTO 事实源收敛到 `agentdash-contracts` 应进入架构 backlog：跨 contract crate、API route/DTO、generated TS、前端 service/types，属于跨层协议事实源调整。
