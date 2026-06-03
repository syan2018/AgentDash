# Frontend Session Runtime Frame Query Design

## Backend Endpoint

推荐新增：

```text
GET /sessions/{runtime_session_id}/frame-runtime
```

返回：

```rust
pub struct RuntimeSessionFrameRuntimeView {
    pub runtime_session_ref: RuntimeSessionRefDto,
    pub frame: Option<AgentFrameRuntimeView>,
    pub activity_anchor: Option<RuntimeSessionActivityAnchorDto>,
}
```

`RuntimeSessionTraceView.frame_ref` 保留 trace drill-down 价值；前端 runtime state 不应先查 trace 再查 frame runtime，否则 session-first 间接链路仍留在 UI。

## Frontend Flow

```text
useSessionRuntimeState(sessionId)
  -> fetchSessionFrameRuntime(sessionId)
  -> setFrame(frame)
  -> return frame runtime projection
```

`lifecycleStore.frames` 只做缓存，不用于 session-to-frame 选择。

## SessionPage Prompt Entry

`SessionPage` 可以继续作为 runtime trace 页面存在，但 prompt 发送必须通过 resolved runtime view 回到 Agent / Lifecycle 控制面。

目标流程：

```text
SessionPage(sessionId)
  -> useSessionRuntimeState(sessionId)
  -> RuntimeSessionFrameRuntimeView
  -> resolve prompt dispatch target:
       activity attempt / assignment
       lifecycle run + agent/frame
       subject run context
  -> SessionChatView.customSend(...)
  -> lifecycle / subject / agent dispatch service
  -> refresh frame runtime + hook runtime + trace feed
```

`SessionChatView` 已有 `customSend`，因此 SessionPage 不需要让 chat view 直接访问 session prompt API。SessionPage 应提供一个 `customSend` 实现，把 prompt 和 executor config 交给 Agent / Lifecycle anchored service。

推荐 dispatch target 优先级：

1. Activity session：使用 runtime anchor 的 assignment / attempt / run identity 继续当前 Activity。
2. Subject-bound run：使用 run context 中的 subject kind/id 走 subject continue API，例如 task continue。
3. Agent surface：使用 agent/frame anchored continue API。
4. Missing anchor：显示不可发送状态，提示当前 trace 没有关联可继续的 Agent / Lifecycle execution。

新增或扩展服务时，API 名称可以由实现阶段按现有 route 风格决定，但返回与输入都应围绕 Agent / Lifecycle identity，而不是只接收 `session_id + prompt`。

## Affected Areas

- `crates/agentdash-contracts/src/workflow.rs`
- `crates/agentdash-api/src/routes/lifecycle_views.rs`
- `packages/app-web/src/services/lifecycle.ts`
- `packages/app-web/src/stores/lifecycleStore.ts`
- `packages/app-web/src/features/workspace-panel/model/useSessionRuntimeState.ts`
- `packages/app-web/src/pages/SessionPage.tsx`
- `packages/app-web/src/features/session/ui/SessionChatView.tsx`
- `packages/app-web/src/features/session/ui/SessionChatViewTypes.ts`
- `packages/app-web/src/services/story.ts`
- `packages/app-web/src/services/project.ts`

## Validation

- Frontend hook test mocks backend by session id.
- Store test confirms cache write only after backend response.
- Backend route test covers auth and missing frame.
- SessionPage test covers custom prompt send with resolved Agent / Lifecycle target.
- SessionPage test covers missing anchor disabled/error state.
