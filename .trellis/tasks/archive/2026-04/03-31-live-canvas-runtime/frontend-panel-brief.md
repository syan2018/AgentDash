# Live Canvas Canvas Panel（SessionPage）简报

## 背景

- 请参阅 [PRD](.trellis/tasks/03-31-live-canvas-runtime/prd.md) 中“Session 页布局”与“系统事件”两个章节，它把 `canvas_presented` 事件、SessionPage 的上下文面板、`SessionChatView` 的 `onSystemEvent` 参数都已经列出。
- [执行计划](.trellis/tasks/03-31-live-canvas-runtime/execution-plan.md) 明确把 `session-page-canvas-panel` 作为独立工作包，并强调 Canvas Panel 要用现成 `SessionPage` 作为主战场。
- 当前 SessionPage 均使用 `SessionChatView`（`frontend/src/pages/SessionPage.tsx`、`story-session-panel.tsx`、`task-agent-session-panel.tsx`）在中间区域展示会话流；`SessionPage.tsx` 通过 `onSystemEvent` 回调收到 `SystemEvent` 并仅使用 `eventType`，而 `SessionChatView.tsx` 已把整个 `SessionUpdate` 透出。
- `AcpSystemEventGuard.ts` 与 `AcpSystemEventCard.tsx` 已有 hooks 处理 pipeline（visible events、hook decision 过滤等），canvas 事件可复用这套 guard。

## 为什么首版只在独立 `SessionPage` 落地

1. `SessionPage` 当前已经是唯一会全面渲染 `SessionChatView`、context panel 与系统事件的页面，改造它不会波及 story/task 的 panel 布局。
2. `story-session-panel.tsx` / `task-agent-session-panel.tsx` 只复用 `SessionChatView`，没有自己的左侧结构，要在这些场景增加 Canvas Panel 要么重写布局，要么强行把 Panel 作为 `SessionChatView` 的子节点，改动面更大。
3. `SessionPage` 控制了 `handleSystemEvent`、hook runtime refresh、owner binding bar 等高层逻辑，可在此集中处理 `canvas_presented` 事件与面板状态，而不会在多个组件间复制 state。
4. 先把 Canvas Panel 做成“独立页面级抽屉/侧栏”，等主链路稳定后再考虑将视图下沉到 Story/Task 面板。

避免先碰的部分：暂时不改 `StorySessionPanel` / `TaskAgentSessionPanel` 的结构、不要触碰 `SessionChatView` 的渲染流程（保持输入框/流区原地），不马上替换 context panel 布局。

## 组件拆分建议、状态流、事件流、接口调用

- **组件拆分**：
  - `CanvasSessionPanel`：独立组件，负责展示 iframe / loading / error / refresh 按钮。
  - `CanvasPanelContainer`（SessionPage）控制面板位置（右侧抽屉 vs 侧栏），渲染 `CanvasSessionPanel` 或提示信息。
  - `CanvasEventBridge`（可选）：封装 `postMessage` 与 runtime snapshot 的通信逻辑，供 Panel 复用。

- **状态流（SessionPage）**：
  - `activeCanvasId`：当前系统事件指定的 canvas。
  - `isCanvasPanelOpen`：面板是否呈现。
  - `canvasSnapshotState`：`Idle | Loading | Ready | Error`，携带 snapshot payload / error message。
  - `canvasEventMetadata`：`canvas_id`、`mount_id`、`title`、`suggestedTab` 等 UI hint（`canvas_presented` 事件里).
  - `canvasDataBindings`：若需要直接展示 data alias，可在 Panel 状态里缓存.

- **事件流**：
  - `SessionChatView` 收到 `SessionUpdate` 的 `canvas_presented`（由后端 `AgentDashEventV1` 注入）时，调用 `onSystemEvent(eventType, update)`。
  - `SessionPage.handleSystemEvent` 通过 `extractAgentDashMetaFromUpdate(update)` 取出 `meta.event.data`；确认 `event.type === "canvas_presented"` 后设置 `activeCanvasId`、`canvasEventMetadata` 并打开面板。
  - Panel 打开后调用 runtime snapshot API，若成功切换 `canvasSnapshotState` 并触发 iframe 重新加载，错误则进入 `Error` 态。
  - 关闭时清理 state，并可触发 `canvas_presented` 的 `refresh_requested` 工具（P2）继续刷新。

- **接口调用时机**：
  - `canvas_presented` 事件通知到 `canvasEventMetadata` 后立即发起 `GET /api/canvases/{id}/runtime-snapshot?session_id={sessionId}`。
  - Snapshot 返回后 `CanvasSessionPanel` 初始化 iframe bootstrap HTML（import map + entry）。
  - 若需要重新注入数据，面板可调用 `GET /api/canvases/{id}/bindings` 或仅通过 `canvasSnapshot` 里的 binding paths。

## 最小 UI 落地 + 后续增强

- **最小 UI**：在 `SessionPage` 的主内容区外包一层 `div`（`flex flex-row h-full`），左边保留现有 `SessionChatView`，右边保留固定宽度或抽屉，渲染 `CanvasSessionPanel`。Panel 默认显示 loading spinner，成功后替换为 `iframe`（`srcdoc`），错误则显示卡片。初版只需渲染 `iframe` + 标题 + 关闭按钮，并在 iframe 上方显示 data alias / snapshot meta。
- **后续增强**：增加“Canvas Tab”切换、数据绑定列表、交互事件回传按钮、/canvas logs view。可在 Panel 内嵌 `CanvasInspector` 组件，展示 binding alias 与目前 snapshot hash。

- **增强建议**：当 iframe 报错或 canvas 重置时，发出 `canvas_refresh_requested` 事件给后端工具（P2），并在 Panel 底部显示“刷新”按钮。后端再回复 `canvas_presented` 或 `canvas_data_injected` 事件，Panel 会自动 reload。

## 潜在阻力

1. `SessionPage.tsx` 目前是纵向堆栈（header + optional context panel + `SessionChatView`），要变成双栏需要改 `className` 与 `overflow` 逻辑，还要协调 `ContextPanel` 的 show/hide。
2. `SessionChatView` 内部 `div` 有 `flex-1 overflow-y-auto`，放在 Row 里需要确保 height / scroll 不受影响（可能需要让 ChatView 限制 max width）。
3. `handleSystemEvent` 目前只写 `switch (eventType)`，并不把 `SessionUpdate` 传上去；目标方案会把 update 提供给 SessionPage。
4. `SessionPage` 目前在 `streamPrefixContent`、`headerSlot`、`inputPrefix` 上已有多处插槽，新增 Canvas Panel 需要保证这些 slot 还能正常渲染，不要互相覆盖。

为应对阻力，建议先实现右侧抽屉（绝对定位覆盖部分宽度），这样不用马上改 `SessionChatView` 的 flex 布局，后续再把抽屉切成正式侧栏。
