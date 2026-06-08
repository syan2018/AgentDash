# Research: present 事件通道（Agent 请求前端展示 GUI 的现有机制）

- **Query**: session event / platform event contract；推送到前端 feed 的事件枚举；现有"打开 tab/展示 panel"事件可复用？WorkspacePanel openOrActivate；present 需新增什么事件
- **Scope**: internal
- **Date**: 2026-06-08

## Findings

### 唯一可复用的样板：`PresentCanvasTool`（present 的直接模板）

`crates/agentdash-application/src/canvas/tools.rs` L447-513。机制三步：
1. `expose_canvas_to_session(...)`（L477-483）：把 canvas mount 写入 VFS + AgentFrame 可见性 + 热更 capability（present extension 时此步换成「确认 tab 存在」即可，不一定需要）。
2. `build_canvas_presented_notification(session_id, turn_id, canvas)`（L650-677）构造一个 **`PlatformEvent::SessionMetaUpdate`** 事件：
   ```rust
   BackboneEnvelope::new(
       BackboneEvent::Platform(PlatformEvent::SessionMetaUpdate {
           key: "canvas_presented".to_string(),
           value: json!({ "canvas_id":.., "title":.., "entry_file":.. }),
       }),
       session_id, source,
   ).with_trace(TraceInfo { turn_id: Some(..), entry_index: None })
   ```
3. 通过 `session_services.eventing.inject_notification(session_id, notification)` 推送（L490-497）。`session_services` 来自 `SharedSessionToolServicesHandle`（L490），present tool 在装配时已注入 handle + session_id + turn_id（provider.rs L317-324）。

### PlatformEvent 契约

`crates/agentdash-agent-protocol/src/backbone/platform.rs` L5-42：

```rust
#[serde(tag = "kind", content = "data", rename_all = "snake_case")]
pub enum PlatformEvent {
    ExecutorSessionBound { executor_session_id },
    SourceSessionTitleUpdated {..},
    HookTrace(Box<HookTracePayload>),
    SessionMetaUpdate { key: String, value: serde_json::Value },   // ← 通用扩展点
    TerminalOutput {..}, TerminalStateChanged {..},
}
```

**没有专门的「打开 tab / 展示 panel」枚举变体**。现有所有「展示」类语义都借道 `SessionMetaUpdate { key, value }`（key 当作子类型，如 `canvas_presented` / `session_meta_updated` / `context_frame` / `system_message`）。这是 TS 自动生成（`#[derive(TS)]`），前端 `session-contracts.ts` 镜像。

> **present 新增事件的两条路线（design 选一）**：
> - **(A) 复用 `SessionMetaUpdate`**：新增一个 key，如 `workspace_module_presented`，`value` 携带 `module_id / view_key / typeId / uri / payload`。零契约改动（不动 Rust enum / 不重生成 TS），与 `canvas_presented` 完全同构。**推荐**——符合现有惯例，最小面。
> - **(B) 新增 `PlatformEvent` 变体**（如 `WorkspaceModulePresented{..}`）：需改 Rust enum + 重生成 TS + 前端 switch 加 case。面更大，收益是强类型。鉴于 canvas 都走 (A)，建议 present 也走 (A)。

### 前端接收 → openOrActivate 链路

`packages/app-web/src/pages/SessionPage.tsx` `canvas_presented` 处理（L588-600）：
```ts
case "canvas_presented": {
    const data = extractPlatformEventData(_event);
    const nextCanvasId = data?.canvas_id ?? data?.canvasId ?? data?.id;
    if (nextCanvasId) {
        setActiveCanvasId(nextCanvasId);
        void refreshSessionRuntimeContext();
        expandWorkspacePanel("canvas", `canvas://${nextCanvasId}`);   // ← openOrActivate 入口
    }
}
```

`expandWorkspacePanel(typeId, uri)` 最终调 `WorkspacePanel` 的 imperative `openTab` → `useWorkspaceTabStore.getState().openOrActivate(typeId, uri)`（WorkspacePanel.tsx L60-72）。

`useWorkspaceTabStore.openOrActivate(typeId, uri)`（`packages/app-web/src/stores/workspaceTabStore.ts` L187-196）：按 `typeId+uri` 找现有 tab，有则激活，无则 `addTab`。

事件分发的总入口：`packages/app-web/src/features/session/model/useSessionStream.ts` L319-329（`session_meta_update` 派发），以及 `systemEventVisibility.ts` L26 把 `canvas_presented` 列入可见 system event。`platformEvent.ts` L22-24 用 `data.key` 作为事件类型名。

### 各 module kind 的 present 目标（前端 tab typeId / uri）

| module kind | tab typeId | uri 形态 | 现有 registry |
|---|---|---|---|
| canvas | `"canvas"` | `canvas://{mount_id}` | WorkspacePanel.tsx L79-80；canvas-tab.tsx |
| extension webview | extension tab `type_id`（来自 manifest `workspace_tabs.type_id`） | extension uri_scheme（manifest `uri_scheme`） | `ExtensionWebviewPanel.openTab` → openOrActivate（ExtensionWebviewPanel.tsx L30-32）；`createExtensionTabDescriptors`（WorkspacePanel.tsx L52-55 动态注册到 `tabTypeRegistry`） |
| builtin panel | 内置 pinned tab typeId | — | 本轮无 |

Child 1 的 `WorkspaceModuleUiEntry`（contracts L77-85）已带 `view_key`（= extension tab `type_id` 或 canvas entry_file，见 workspace_module/mod.rs L91/166）、`renderer_kind`（"webview"/"canvas"/"panel"）、`uri_scheme`。present payload 可由 describe 出的 ui_entry 推导出 typeId+uri。

> **无前端目标时的「可操作诊断事件」（R4）**：present 找不到 ui_entry / 不可达时，应仍 `inject_notification` 一个诊断事件（如 `SessionMetaUpdate{ key:"workspace_module_present_failed", value:{ module_id, view_key, reason } }`），不静默失败。`canvas_presented` 没有失败分支可抄，但 inject_notification 机制相同。

### inject_notification 服务

`session_services.eventing` = `SessionEventingService`，方法 `inject_notification(session_id, BackboneEnvelope)`（PresentCanvasTool L493-497 调用）。事件落库后经 stream 推前端 feed。present tool 需要 `SharedSessionToolServicesHandle`（provider.rs L46/141 注入）。

## Caveats / Not Found

- 没有「open_tab / present_panel」专用 PlatformEvent；所有展示语义复用 `SessionMetaUpdate{key,value}`。建议 present 新增 key（路线 A），与 canvas 一致。
- 前端目前**只对 `canvas_presented` 有 case**；extension/builtin present 需要前端在 SessionPage switch 里新增 case（或一个通用 `workspace_module_presented` case 按 renderer_kind 分派 typeId/uri）。这是 Child 2/Child 3 的前端工作面。
- `extractPlatformEventData` / `expandWorkspacePanel` 在 SessionPage.tsx，是前端复用入口。
