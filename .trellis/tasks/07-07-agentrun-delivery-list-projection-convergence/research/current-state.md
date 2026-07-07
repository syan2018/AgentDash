# Current State Research

## Backend Facts

- `AgentRunWorkspaceQueryService::resolve_list_item` derives list shell state from current delivery selection. The displayed list status is ultimately `AgentRunDeliveryBinding -> AgentRunExecutionState -> AgentRunWorkspaceStateModel.delivery_status`.
- `LifecycleRelationWriter::write_for_dispatch` publishes `ProjectProjectionInvalidation::agent_run_list(... AgentRunLineageChanged ...)` after child lineage creation. This explains why newly created Companion AgentRuns appear in the list.
- `AcceptedTurnLifecycleAdvanceService` publishes `agent_run_list(... AgentRunActivityChanged ...)` only for orchestration node started updates; plain Companion running status still depends on delivery binding notification paths.
- `AgentFrameLaunchCommitService::bind_current_delivery_with_anchor` currently writes `AgentRunDeliveryBinding(status=Running)` and publishes `agent_run_list(... AgentRunShellChanged ...)` directly.
- `AgentRunDeliveryStateService::mark_terminal_from_runtime_session` writes terminal binding but does not own project projection notification. `AgentRunTerminalConvergenceService` wraps it and publishes `agent_run_list(... DeliveryTerminal ...)`.

## Frontend Facts

- `agent-run-list-state-store` subscribes to project events and refreshes the list when event type is `StateChanged` or `ControlPlaneProjectionChanged` with `projection === "agent_run_list"`.
- The store ignores non-list projection invalidations, which is correct because mailbox/workspace/runtime details should not be second-guessed by the list store.
- `fetchFirstPage` uses a per-project in-flight promise. When a refresh is already running, later refresh calls await the existing promise and return. This can lose a newer invalidation if the in-flight request read an older snapshot.
- `ActiveAgentRunList` and `AgentRunShortcutList` both consume the same store, so fixing the store convergence semantics fixes both surfaces.

## Conclusion

The missing abstraction is not another frontend refresh call. The stable module should be:

```text
AgentRunDeliveryStateService:
  write delivery binding state
  publish list projection invalidation after conditional durable write succeeds

agent-run-list-state-store:
  consume project list invalidations
  serially converge first-page projection until no dirty generation remains
```

The delivery binding repository needs an explicit conditional write interface. A service-level read-before-write check rejects already stale runtimes, but it cannot protect the race where an old runtime reads itself as current, a new runtime replaces the current binding, and then the old runtime writes terminal/running state late. The write itself must assert that the current `(run_id, agent_id)` row still belongs to the same `runtime_session_id`.
