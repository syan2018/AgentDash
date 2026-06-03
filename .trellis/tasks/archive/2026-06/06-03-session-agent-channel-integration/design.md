# Session 与 Agent 会话信道整合设计

## Architecture Boundary

Session 页是用户面向的消息流壳，发送命令的业务入口是 LifecycleAgent。运行时 trace id 只用于定位当前壳对应的 delivery RuntimeSession；服务端必须沿 `runtime_session_id -> RuntimeSessionExecutionAnchor / AgentFrame -> LifecycleAgent -> LifecycleRun` 解析出控制面事实，再派发用户消息。

本设计延续当前控制面不变量：

- `LifecycleRun` 是业务执行控制账本。
- `LifecycleAgent` 是 run-scoped 执行身份。
- `AgentFrame` 是 Agent runtime surface revision。
- `RuntimeSession` 是 connector delivery / trace evidence。
- 用户看见 Session，但 command 不以 Session 为业务 root。

## Target Data Flow

```text
SessionPage input
  -> frontend Agent session send service
  -> POST Agent/Lifecycle message command
  -> resolve delivery runtime session to AgentFrame/LifecycleAgent
  -> validate frame references delivery RuntimeSession
  -> build Agent user-message command with prompt blocks
  -> LifecycleAgent unified dispatch path
  -> delivery RuntimeSession receives connector prompt
  -> existing Session stream/feed renders user message and agent response
```

The key shift is where ownership is resolved. Frontend may start from `/session/:runtime_session_id`, but backend command handling must resolve and execute through Agent/Frame semantics before touching runtime delivery.

## Backend Contract

Introduce a narrow Agent session command endpoint owned by the lifecycle/agent control surface rather than the generic runtime trace surface.

Recommended shape:

```http
POST /lifecycle-agents/by-runtime-session/{runtime_session_id}/messages
```

Request:

```json
{
  "prompt_blocks": [{ "kind": "text", "text": "..." }],
  "executor_config": null
}
```

Response:

```json
{
  "runtime_session_id": "...",
  "run_ref": { "run_id": "..." },
  "agent_ref": { "run_id": "...", "agent_id": "..." },
  "frame_ref": { "agent_id": "...", "frame_id": "...", "revision": 1 }
}
```

Route responsibilities:

- authenticate and authorize through resolved project permission;
- resolve runtime session to AgentFrame and LifecycleAgent;
- validate that resolved frame still references the requested delivery RuntimeSession;
- delegate to the unified Agent/Lifecycle send use case;
- return refs needed for frontend store refresh.

Application responsibilities:

- keep `session_runtime.start_prompt` behind the Agent use case, as runtime delivery implementation detail;
- construct the command from AgentFrame runtime surface and user intent;
- preserve Session stream events as the rendering path after dispatch.

## Cleanup Contract

The implementation must remove or seal the old command affordances so they cannot be reconnected by accident:

- `SessionChatView` should not own a default prompt transport. It may render input only when a parent supplies an Agent dispatcher or should render a disabled/not-ready state with a reason.
- No frontend service should expose `sendSessionPrompt(sessionId, prompt)` or similar session-first naming.
- No API route should expose ordinary user prompt submission as `/sessions/{id}/prompt`.
- Existing `SessionRuntimeService::start_prompt` stays as internal runtime delivery plumbing, not as route-level business API. New route handlers must not call it until after Agent/Frame resolution.
- Specs and tests must describe Session prompt continuation as LifecycleAgent command dispatch, not RuntimeSession command dispatch.

## Frontend Contract

`SessionPage` receives a runtime session id from route. It should:

- load `useSessionRuntimeState` to obtain `hookRuntime.snapshot.run_context` and frame facts;
- provide `SessionChatView.customSend`;
- call a typed service function that names the control-plane action, not a runtime trace action;
- refresh lifecycle/frame/session state after send;
- show a disabled/not-ready input state when no AgentFrame/LifecycleAgent can be resolved.

`SessionChatView` remains a reusable message UI. It should not infer business ownership. Its contract becomes: parent-owned dispatcher required for user prompt submission.

## Validation Strategy

Automated validation:

- frontend type-check;
- frontend tests for `SessionPage` passing Agent dispatcher and for unresolved runtime traces being visibly non-sendable;
- backend route/use-case test resolving runtime session to AgentFrame/LifecycleAgent and invoking the Agent send path;
- regression search/test proving `/sessions/{id}/prompt` and `sendSessionPrompt` are absent.

Manual validation:

- run `pnpm dev`;
- open browser;
- create/open a Project Agent session;
- send message 1 and wait for visible Agent response;
- send message 2 and wait for visible Agent response;
- verify Session page continues streaming and does not show the Runtime trace forbidden message.

## Risks

- The current codebase contains session-first helpers that look convenient but bypass control-plane ownership. The implementation should prefer new explicit Agent/Lifecycle command naming even if existing runtime helpers are nearby.
- Some current read models may not expose all refs needed by the frontend. Backend resolution by runtime session keeps frontend payload small and prevents frontend from reconstructing ownership from partial state.
