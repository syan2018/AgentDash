# 设计

## Boundary

`SessionMeta` 是 RuntimeSession trace metadata，不是 workspace shell。

保留在 RuntimeSession trace meta：

- `runtime_session_id`
- `last_event_seq`
- `executor_session_id`
- trace title / title provenance
- terminal trace summary
- trace timestamps

迁移到 AgentRun Workspace shell/list projection：

- user-visible workspace title
- title edit target
- delivery status shown in sidebar/workspace header
- last visible turn id
- last workspace activity timestamp
- command action availability

## DTO Shape

API contract child should introduce:

```text
RuntimeSessionTraceMeta {
  runtime_session_ref
  event_seq
  executor_session_id?
  trace_title?
  trace_title_source?
  terminal_summary?
  updated_at
}

AgentRunWorkspaceShell {
  display_title
  title_source
  delivery_status
  last_turn_id?
  last_activity_at
}
```

`AgentRunWorkspaceView` includes `shell` and optional `delivery_trace_meta`. RuntimeSession trace endpoints use `RuntimeSessionTraceMeta`.

## Data Sources

`AgentRunWorkspaceShell.display_title` should be derived from AgentRun-facing facts: ProjectAgent display name, subject association metadata, and optional user workspace title. Provider/source session title remains trace metadata unless explicitly promoted by a workspace title policy.

`delivery_status` should be derived from active turn/command receipt/delivery projection. `SessionMeta.last_delivery_status` can be read when building trace metadata or recovering interrupted trace state, but it should not be the workspace authority once command receipt and AgentRun command projection exist.

`executor_session_id` remains in trace meta because connector follow-up and repository rehydrate need provider runtime continuity.

## Frontend Projection

The sidebar/list component should become AgentRun-oriented. It should navigate to `/agent-runs/:runId/:agentId` and use workspace/list shell data. RuntimeSession trace links should be explicit trace links.

`SessionMetaUpdate` platform events remain renderable feed events. Workspace title changes should either arrive through AgentRun workspace refresh or a renamed AgentRun shell event after the API contract child defines it.

## Spec Updates

Final integration should update:

- `.trellis/spec/backend/session/runtime-execution-state.md`
- `.trellis/spec/frontend/state-management.md`
- `.trellis/spec/frontend/architecture.md`
- `.trellis/spec/cross-layer/frontend-backend-contracts.md`

These specs should state why RuntimeSession meta remains trace/delivery metadata and why workspace shell belongs to AgentRun projection.
