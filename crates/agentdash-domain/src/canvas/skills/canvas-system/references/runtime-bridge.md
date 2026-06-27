# Canvas Runtime Bridge Reference

Use this reference as the routing guide for Canvas source that needs the AgentDashboard runtime SDK.

## Runtime SDK Map

Canvas preview iframe exposes one SDK object:

```ts
window.agentdash.invoke(actionKey: string, input?: unknown): Promise<RuntimeInvocationResult>
window.agentdash.assets.url(uri: string): Promise<string>
window.agentdash.assets.revoke(url: string): void
window.agentdash.interaction.setState(key: string, value: unknown): Promise<{ ok: true }>
window.agentdash.interaction.clearState(key: string): Promise<{ ok: true }>
window.agentdash.interaction.emit(event: CanvasInteractionEvent): Promise<{ ok: true }>
window.agentdash.interaction.getState(): unknown
window.agentdash.agent.submit(input: CanvasAgentSubmitInput): Promise<AgentRunMessageCommandResponse>
```

## Choose By Use Case

| Situation | Recommended action | Reference |
| --- | --- | --- |
| A Canvas button needs to call a runtime tool, MCP action, refresh command, or backend-hosted capability and render the returned data in the Canvas. | Call `window.agentdash.invoke(...)` from the user action handler; keep action keys provided by the platform/task context. | `runtime-actions.md` |
| Canvas source needs to show an image that lives in VFS, skill assets, generated docs media, or another session mount. | Resolve a browser URL with `window.agentdash.assets.url(uri)` and revoke it when the component no longer needs it. | `vfs-assets.md` |
| The user changes filters, selects a row/card/node, fills a form, or performs a UI action the Agent may need to inspect later. | Publish compact semantic state with `window.agentdash.interaction.setState(...)` and recent events with `emit(...)`; do not rely on DOM inference. | `interaction-state.md` |
| Canvas UI should let the user submit structured feedback, a decision, or follow-up context from the current surface. | Call `window.agentdash.agent.submit(...)` from the user action and include interaction/render facts only when relevant. | `agent-submit.md` |
| The Agent needs to create/copy/attach a Canvas, bind data, diagnose render state, inspect interaction state, or present a preview. | Use workspace module operations and describe output as the source of truth for available operations. | `agent-side-interfaces.md` |
| You are unsure whether the action is a runtime tool call or a user submission. | Use `invoke` for tool/action results displayed in Canvas; use `agent.submit` when the user's feedback, decision, or request should become the AgentRun's next mailbox input. | `runtime-actions.md`, `agent-submit.md` |

## Boundary Table

| Canvas API | Use For | Fact Produced |
| --- | --- | --- |
| `agentdash.invoke` | RuntimeGateway actions such as MCP calls | Runtime invocation result |
| `agentdash.assets.url` | Browser rendering of VFS image assets | Revocable browser image URL |
| `agentdash.interaction.*` | Agent-visible latest Canvas UI state | Interaction snapshot |
| `agentdash.agent.submit` | Structured user feedback or follow-up context from Canvas | AgentRun mailbox user input |

`agentdash.invoke` is not an Agent input channel. Use `agentdash.agent.submit(...)` when Canvas UI collects user feedback, decisions, or follow-up context that should enter the AgentRun mailbox.

The iframe does not receive tokens, backend ids, session ids, auth headers, relay commands, signed provider URLs, or local paths. The parent page and backend resolve runtime/session/project details from the current AgentRun Canvas bridge.
