# Canvas Runtime Bridge Reference

Use this reference when writing Canvas source that calls AgentDashboard session runtime actions.

## Runtime SDK

Canvas preview iframe exposes one SDK object:

```ts
window.agentdash.invoke(actionKey: string, input?: unknown): Promise<RuntimeInvocationResult>
window.agentdash.assets.url(uri: string): Promise<string>
window.agentdash.assets.revoke(url: string): void
```

The `invoke` API is for Session Runtime Actions. The `assets` API is for Canvas browser resource loading.

`invoke` rules:

- Call it only from explicit user actions such as button clicks, form submits, or refresh controls.
- Do not call it during module load, React render, or automatic polling.
- Pass only the action key and provider input. The platform supplies actor, session context, policy, trace, and routing.
- Do not send tokens, backend ids, MCP transports, relay commands, absolute paths, or arbitrary HTTP requests.
- Do not build a runtime action discovery flow inside the canvas. Use action keys and input contracts provided by the platform, user request, or task context.

These `invoke` rules do not prohibit loading VFS image assets during component effects. Use `window.agentdash.assets.url(uri)` for image rendering instead of routing images through runtime actions.

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

Use this API to render image binary files from mounts visible in the current session runtime surface:

```ts
const imageUrl = await window.agentdash.assets.url("main://docs/diagram.png");
```

The URI shape is:

```text
<mount_id>://<mount_relative_path>
```

Examples:

```ts
await window.agentdash.assets.url("main://docs/diagram.png");
await window.agentdash.assets.url("skill-assets://skills/demo/assets/logo.png");
await window.agentdash.assets.url("ld-km://assets/doc-id/source-id.png");
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

- `canvases_list`: inspect project canvases and mount ids.
- `canvas_start`: attach/create a canvas and return `canvas_id`, `mount_id`, `entry_file`, and `skill_path`.
- `bind_canvas_data`: map a VFS `source_uri` to `bindings/<alias>.json`.
- `present_canvas`: show the canvas to the user.
- VFS tools: edit files through `<mount_id>://...`; canvas mounts support read/write/list/search, not exec.
