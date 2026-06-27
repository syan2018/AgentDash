# Canvas Submit To Agent

Use this reference when Canvas UI should let the user submit structured feedback, a decision, or follow-up context to the current AgentRun.

## API

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

## Rules

- Call `agent.submit` only from explicit user actions such as button clicks, form submits, or confirmation controls.
- Provide either `text` or canonical `input`.
- Set `include_interaction_state` when the current Canvas state is relevant to the request.
- Set `include_render_observation` when the rendered state or diagnostics should help the Agent understand the request.
- Use `delivery_intent: "queue"` for normal follow-up requests. Use `"steer"` only when the UI is explicitly steering an active run.
- Show compact success/error feedback in the Canvas UI; the backend returns the same AgentRun mailbox command response used by the workspace composer.
- If the preview reports that no live AgentRun bridge is available, keep the Canvas usable but explain that submit-to-Agent requires presenting the Canvas inside an AgentRun workspace.

`agent.submit` is the Canvas-origin user input channel for structured user feedback and follow-up context. Use `window.agentdash.invoke(...)` for RuntimeGateway actions, not for Agent input.
