# Canvas Runtime Actions

Use this reference when Canvas source calls AgentDashboard session runtime actions through `window.agentdash.invoke(...)`.

## Rules

- Call `invoke` only from explicit user actions such as button clicks, form submits, or refresh controls.
- Do not call it during module load, React render, or automatic polling.
- Pass only the action key and provider input. The platform supplies actor, session context, policy, trace, and routing.
- Do not send tokens, backend ids, MCP transports, relay commands, absolute paths, or arbitrary HTTP requests.
- Do not build a runtime action discovery flow inside ordinary Canvas UX. Use action keys and input contracts provided by the platform, user request, or task context.

These rules do not prohibit loading VFS image assets during component effects. Use `window.agentdash.assets.url(uri)` for image rendering instead of routing images through runtime actions.

## Result Shape

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

`invoke` rejects when the action is unavailable, denied by capability policy, given invalid input, or fails in the provider. Show a compact error state in the Canvas UI.

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
