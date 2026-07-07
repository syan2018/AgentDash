# Implementation Plan

## Ordered Steps

1. Backend delivery state convergence
   - Extend `AgentRunDeliveryStateService` dependencies to include `LifecycleAgentRepository` and optional `ProjectProjectionNotificationPort`.
   - Add `AgentRunDeliveryBindingRepository::upsert_if_current_runtime_session` and require concrete repositories to implement it explicitly.
   - Add running transition method for accepted turns that writes current `AgentRunDeliveryBinding(status=Running)` and publishes `agent_run_list` invalidation.
   - Move terminal transition invalidation into the same service path.
   - Update `launch_commit` and `terminal_convergence` to call the centralized service rather than each owning list invalidation.

2. Backend tests
   - Add/adjust `delivery_state.rs` tests for running invalidation, terminal invalidation, stale runtime no-op, and read/write race loss.
   - Add repository test coverage for conditional current-runtime writes.
   - Update launch/terminal tests affected by the constructor/dependency change.

3. Frontend list convergence
   - Replace first-page in-flight short-circuit semantics with per-project dirty/applied generation tracking.
   - Ensure `refreshProject` / `invalidateProject` schedule serial first-page refreshes and preserve existing loading states.
   - Keep `loadMore` behavior unchanged unless required by type interactions.

4. Frontend tests
   - Add an in-flight invalidation regression test in `agent-run-list-state-store.test.ts`.
   - Keep existing tests for project-scoped list invalidation and ignored non-list projections.

5. Spec update
   - Update `.trellis/spec/backend/session/agentrun-mailbox.md` or cross-layer contract docs to record that delivery binding status writes publish AgentRun list projection invalidation.
   - Keep the wording focused on why the contract exists.

## Validation Commands

```powershell
cargo fmt
cargo test -p agentdash-application-agentrun delivery_state
cargo test -p agentdash-application-agentrun
pnpm --filter app-web test -- src/features/agent/agent-run-list-state-store.test.ts
pnpm run frontend:check
```

Run a narrower frontend lint/test first if iteration speed matters, then run the listed checks before commit.

## Risk Points

- `AgentRunDeliveryStateService` constructor changes can ripple through tests and terminal convergence setup.
- The list store generation loop must avoid recursive unbounded refresh when a fetch fails; errors should still clear in-flight state while preserving dirty state for the next explicit invalidation.
- Existing unrelated dirty task directory must not be included in commits.

## Commit Shape

One coherent commit is expected:

```text
fix(agent-run): 收敛 delivery 状态列表投影刷新
```

Include task artifacts, code, tests, and spec updates that belong to this convergence task.
