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

## Affected Areas

- `crates/agentdash-contracts/src/workflow.rs`
- `crates/agentdash-api/src/routes/lifecycle_views.rs`
- `packages/app-web/src/services/lifecycle.ts`
- `packages/app-web/src/stores/lifecycleStore.ts`
- `packages/app-web/src/features/workspace-panel/model/useSessionRuntimeState.ts`
- `packages/app-web/src/pages/SessionPage.tsx`

## Validation

- Frontend hook test mocks backend by session id.
- Store test confirms cache write only after backend response.
- Backend route test covers auth and missing frame.
