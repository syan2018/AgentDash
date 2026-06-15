# Implement Plan

## Phase 0: Research

- [x] Subagent audits session-first entries and writes `research/session-entry-audit.md`.
- [ ] Main session reviews audit and updates this design if necessary.

## Phase 1: Contracts

- [ ] Define or locate application-layer `AgentRunRuntimeAddress`.
- [ ] Define `MessageStreamRef` / projection ref.
- [ ] Define `OrchestrationNodeCoordinate` reuse or adapter.
- [ ] Add conversion helpers from `RuntimeSessionExecutionAnchor` for transitional callers.

Research summary:

- Message stream / connector trace / transcript APIs can remain session-first.
- AgentRun mailbox command target convergence is the first high-value migration slice.
- AgentRun workspace query already enters through run/agent but still overuses latest runtime session for shell/status.
- Task projection/effect paths should move from session anchor node fields to orchestration node coordinate.
- `RuntimeSessionExecutionAnchorRepository::find_by_session` should become trace adapter/debug/terminal callback infrastructure, not a default business resolver.

## Phase 2: First Migration Slice

- [ ] Align `AgentRunLifecycleSurfaceProjector` with AgentRun-first address.
- [ ] Update AgentRun workspace query to use AgentRun-first address before optional message stream projection.
- [ ] Keep connector stream and transcript APIs session-first.

## Phase 3: Runtime Entry Migration

- [ ] Migrate mailbox command target from session-first to AgentRun-first.
- [ ] Migrate hook control target from session-first to AgentRun-first.
- [ ] Migrate task effect/runtime action targets where they currently resolve business context from session.
- [ ] Rename remaining session-first functions to make message stream intent explicit.

## Phase 4: Specs And Tests

- [ ] Update `.trellis/spec/backend/session/architecture.md`.
- [ ] Update `.trellis/spec/backend/workflow/architecture.md`.
- [ ] Update `.trellis/spec/backend/session/runtime-execution-state.md`.
- [ ] Add tests that non-message runtime surface APIs do not require `runtime_session_id`.
- [ ] Keep tests that message stream APIs can still use `runtime_session_id`.

## Validation Commands

```powershell
cargo test -p agentdash-application session::
cargo test -p agentdash-application workflow::
cargo test -p agentdash-application task::
cargo check -p agentdash-application
```

If API contracts change:

```powershell
pnpm run contracts:check
pnpm --filter app-web typecheck
```

## Risk Points

- Connector runtime still needs session identity for stream control.
- Transcript and compaction stores may be session-keyed by design.
- Existing hooks / mailbox code may use session as a convenient lookup shortcut; migrate through adapters rather than mixing ownership.
- Do not block lifecycle surface projector on complete session cleanup.

## Parallelization

- Research can run in subagent now.
- Main task projector implementation can proceed in parallel using AgentRun-first contract.
- Runtime entry migration should start after research identifies bounded slices.
