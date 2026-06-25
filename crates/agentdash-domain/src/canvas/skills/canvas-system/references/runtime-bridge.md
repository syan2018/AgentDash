# Canvas Runtime Bridge Reference

Use this reference when writing Canvas source that calls AgentDashboard session runtime actions.

## Runtime SDK

Canvas preview iframe exposes one SDK object:

```ts
window.agentdash.invoke(actionKey: string, input?: unknown): Promise<RuntimeInvocationResult>
window.agentdash.assets.url(uri: string): Promise<string>
window.agentdash.assets.revoke(url: string): void
window.agentdash.interaction.setState(key: string, value: unknown): Promise<CanvasInteractionSnapshot>
window.agentdash.interaction.clearState(key: string): Promise<CanvasInteractionSnapshot>
window.agentdash.interaction.emit(event: CanvasInteractionEvent): Promise<CanvasInteractionSnapshot>
window.agentdash.interaction.getState(): CanvasInteractionState
window.agentdash.agent.submit(input: CanvasAgentSubmitInput): Promise<AgentRunMessageCommandResponse>
```

The `invoke` API is for Session Runtime Actions. The `assets` API is for Canvas browser resource loading. The `interaction` API exposes latest Agent-visible browser state for diagnostics. The `agent.submit` API sends an explicit Canvas user action to the current AgentRun mailbox as canonical user input.

`invoke` rules:

- Call it only from explicit user actions such as button clicks, form submits, or refresh controls.
- Do not call it during module load, React render, or automatic polling.
- Pass only the action key and provider input. The platform supplies actor, session context, policy, trace, and routing.
- Do not send tokens, backend ids, MCP transports, relay commands, absolute paths, or arbitrary HTTP requests.
- Do not build a runtime action discovery flow inside the canvas. Use action keys and input contracts provided by the platform, user request, or task context.

These `invoke` rules do not prohibit loading VFS image assets during component effects. Use `window.agentdash.assets.url(uri)` for image rendering instead of routing images through runtime actions.

`invoke` is not an Agent input channel. Use `window.agentdash.agent.submit(...)` when a Canvas button, form, or selection action should ask the current Agent to continue.

## Result Shape

`invoke` resolves with:

```ts
interface RuntimeInvocationResult {
  action_key: string;
  trace: {
    trace_id: string;
    invocation_id: string;
    parent_trace_id?: string | null;
    created_at: string;
  };
  output: {
    output: unknown;
    metadata: Record<string, unknown>;
  };
}
```

Provider-specific data is under `result.output.output`.

`invoke` rejects when the action is unavailable, denied by capability policy, given invalid input, or fails in the provider. Show a compact error state in the canvas UI.

## VFS Image Assets

Use this API to render image binary files from mounts visible in the current session runtime surface. This is the standard route for VFS images in Canvas source:

```ts
const imageUrl = await window.agentdash.assets.url("main://docs/diagram.png");
```

The URI shape is:

```text
<vfs_mount_id>://<mount_relative_path>
```

Examples:

```ts
await window.agentdash.assets.url("main://docs/diagram.png");
await window.agentdash.assets.url("skill-assets://skills/demo/assets/logo.png");
await window.agentdash.assets.url("docs-media://assets/doc-1/source-123.png");
```

Behavior:

- Resolves only against the current Canvas/session VFS surface.
- Returns a browser URL suitable for `<img src={imageUrl}>`.
- Rejects non-image MIME types.
- Rejects invalid mount URIs, unavailable mounts, missing sessions, and provider read failures.
- Does not expose VFS `surface_ref`, backend ids, auth headers, signed provider URLs, or local paths to Canvas code.

Cleanup:

```ts
const imageUrl = await window.agentdash.assets.url(uri);
// later, when no longer needed:
window.agentdash.assets.revoke(imageUrl);
```

The preview runtime also revokes generated URLs when the Canvas reloads or unmounts.

Do not use:

```tsx
<img src="main://docs/diagram.png" />
```

Browsers cannot load VFS mount URIs directly. Always resolve them through `window.agentdash.assets.url(uri)` first.

## Interaction State

Use `window.agentdash.interaction` when Canvas UI state should be visible to the Agent for inspection without automatically entering the conversation.

```ts
await window.agentdash.interaction.setState("selection", {
  kind: "table_row",
  ids: ["row-17"],
  summary: "Q2 East region revenue",
});

await window.agentdash.interaction.emit({
  kind: "row_selected",
  payload: { id: "row-17" },
});
```

Rules:

- Call `setState` for durable current UI facts such as form values, selected ids, filters, viewport mode, or active entity summaries.
- Call `clearState` when a value is no longer Agent-visible.
- Call `emit` for recent user events that help explain what just happened.
- Keep values JSON-serializable and compact. Store ids, labels, summaries, and small objects rather than full tables or binary data.
- Interaction state remains a latest snapshot. It is queryable through the Canvas workspace module and does not enter model history unless a user action submits input.
- Use `workspace_module_invoke(..., operation_key="canvas.get_interaction_state")` from the Agent side to inspect the latest snapshot.

## Submit To Agent

Use `window.agentdash.agent.submit(...)` only from explicit user actions such as button clicks or form submits.

```ts
await window.agentdash.agent.submit({
  text: "Analyze the selected row and suggest the next action.",
  include_interaction_state: true,
  include_render_observation: true,
  delivery_intent: "queue",
});
```

Input shape:

```ts
type CanvasAgentSubmitInput = {
  text?: string;
  input?: UserInput[];
  include_interaction_state?: boolean;
  include_render_observation?: boolean;
  delivery_intent?: "queue" | "steer";
  client_command_id?: string;
};
```

Rules:

- Provide either `text` or canonical `input`.
- Set `include_interaction_state` when the current Canvas state is relevant to the request.
- Set `include_render_observation` when the rendered state or diagnostics should help the Agent understand the request.
- Use `delivery_intent: "queue"` for normal follow-up requests. Use `"steer"` only when the UI is explicitly steering an active run.
- Show compact success/error feedback in the Canvas UI; the backend returns the same AgentRun mailbox command response used by the workspace composer.
- If the preview reports that no live AgentRun bridge is available, keep the Canvas usable but explain that submit-to-Agent requires presenting the Canvas inside an AgentRun workspace.

## Runtime Actions

### `mcp.call_tool`

Use this when the platform or task context gives you a specific MCP runtime tool to call.

Input:

```ts
{
  runtime_name: string;
  arguments?: Record<string, unknown> | null;
}
```

Alternative target form, only when explicitly provided:

```ts
{
  server_name: string;
  tool_name: string;
  arguments?: Record<string, unknown> | null;
}
```

Notes:

- Prefer `runtime_name`.
- Do not invent `runtime_name`, `server_name`, or `tool_name`.
- `arguments` must be a JSON object or `null`.
- The returned payload is the MCP tool result serialized inside `RuntimeInvocationResult.output.output`.

Example:

```tsx
const invocation = await window.agentdash.invoke("mcp.call_tool", {
  runtime_name: "provided_runtime_tool_name",
  arguments: { limit: 10 },
});

const toolResult = invocation.output.output;
```

### `mcp.list_tools`

This action exists for platform-controlled tool surface inspection. Do not build ordinary Canvas UX around tool discovery unless the platform or task explicitly asks for that diagnostic/admin behavior.

Input:

```ts
{
  server_names?: string[];
}
```

Output:

```ts
{
  tools: Array<{
    runtime_name: string;
    server_name: string;
    tool_name: string;
    uses_relay: boolean;
    description: string;
    parameters_schema: unknown;
  }>;
}
```

## Agent-Side Canvas Interfaces

These are Agent tools, not browser runtime APIs:

- `workspace_module_create(kind="canvas")`: create or attach a Canvas and return the `canvas:{canvas_mount_id}` descriptor. The create result exposes the current session's Canvas VFS mount and skill path so the Agent can immediately edit files such as `cvs-demo://src/main.tsx`.
- `workspace_module_list`: inspect project workspace modules, including existing Canvas modules named `canvas:{canvas_mount_id}`.
- `workspace_module_describe(module_id="canvas:{canvas_mount_id}")`: inspect the Canvas module UI entries and operation schemas before invoking or presenting it.
- `workspace_module_invoke(module_id="canvas:{canvas_mount_id}", operation_key="canvas.bind_data", input={...})`: map a VFS `source_uri` to `bindings/<alias>.<ext>` using the operation schema returned by describe; the extension follows explicit `content_type` or is inferred from `source_uri`.
- `workspace_module_invoke(module_id="canvas:{canvas_mount_id}", operation_key="canvas.inspect_render_state", input={})`: read the latest render observation reported by the Canvas iframe, including ready/error status, viewport, DOM summary, and diagnostics.
- `workspace_module_invoke(module_id="canvas:{canvas_mount_id}", operation_key="canvas.get_interaction_state", input={})`: read the latest interaction snapshot explicitly exposed by Canvas source.
- `workspace_module_present(module_id="canvas:{canvas_mount_id}", view_key="preview")`: expose the Canvas runtime surface to the current session and open its `presentation_uri`.
- VFS tools: edit files through `{canvas_mount_id}://...`; canvas mounts support read/write/list/search, not exec.

URI boundaries:

- `canvas://{canvas_mount_id}` is the presentation URI for the WorkspacePanel Canvas tab.
- `{canvas_mount_id}://...` is the Agent-editable VFS URI for Canvas files.
- Backend ids and absolute paths are not browser runtime API inputs.
