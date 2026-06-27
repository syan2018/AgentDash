# Canvas Interaction State

Use this reference when Canvas UI state should be visible to the Agent for inspection without automatically entering the conversation.

## API

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

## Rules

- Call `setState` for durable current UI facts such as form values, selected ids, filters, viewport mode, or active entity summaries.
- Call `clearState` when a value is no longer Agent-visible.
- Call `emit` for recent user events that help explain what just happened.
- Keep values JSON-serializable and compact. Store ids, labels, summaries, and small objects rather than full tables or binary data.
- Interaction state remains a latest snapshot. It is queryable through the Canvas workspace module and does not enter model history unless a user action submits input.
- Use `workspace_module_invoke(..., operation_key="canvas.get_interaction_state")` from the Agent side to inspect the latest snapshot.

## Component Pattern

```tsx
async function selectRow(row: { id: string; region: string; metric: string; amount: number }) {
  await window.agentdash.interaction.setState("selection", {
    kind: "dashboard_row",
    ids: [row.id],
    summary: `${row.region} / ${row.metric}`,
    value: {
      region: row.region,
      metric: row.metric,
      amount: row.amount,
    },
  });

  await window.agentdash.interaction.emit({
    kind: "row_selected",
    payload: { id: row.id },
  });
}
```

Canvas source must explicitly publish business state. The platform does not infer semantic form, selection, or filter state from arbitrary DOM nodes.
