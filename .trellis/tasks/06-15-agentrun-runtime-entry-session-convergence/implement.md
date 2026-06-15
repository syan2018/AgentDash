# Implement Plan

## Phase 0: Research

- [x] Subagent audits session-first entries and writes `research/session-entry-audit.md`.
- [x] Main session reviews audit and updates this design if necessary.

## Phase 1: Contracts

- [x] Define or locate application-layer `AgentRunRuntimeAddress`.
- [x] Define `MessageStreamRef` / projection ref.
- [x] Define `OrchestrationNodeCoordinate` reuse or adapter.
- [x] Add conversion helpers from `RuntimeSessionExecutionAnchor` for transitional callers.

Research summary:

- Message stream / connector trace / transcript APIs can remain session-first.
- AgentRun mailbox command target convergence is the first high-value migration slice.
- AgentRun workspace query already enters through run/agent but still overuses latest runtime session for shell/status.
- Task projection/effect paths should move from session anchor node fields to orchestration node coordinate.
- `RuntimeSessionExecutionAnchorRepository::find_by_session` should become trace adapter/debug/terminal callback infrastructure, not a default business resolver.

## Phase 2: First Migration Slice

- [x] Align `AgentRunLifecycleSurfaceProjector` with AgentRun-first address.
- [x] Update AgentRun workspace query to use AgentRun-first address before optional message stream projection.
- [x] Keep connector stream and transcript APIs session-first.

## Phase 3: Runtime Entry Migration

- [x] Migrate mailbox command target from session-first to AgentRun-first.
- [x] Migrate hook control target from session-first to AgentRun-first.
- [x] Migrate task effect/runtime action targets where they currently resolve business context from session.
- [ ] Rename remaining session-first functions to make message stream intent explicit.

Mailbox slice notes:

- `AgentRunMailboxCommandTarget` now carries `AgentRunRuntimeAddress` plus optional `MessageStreamProjectionRef`.
- User/control mailbox commands have target-first entrypoints while existing session-shaped commands remain adapters.
- Scheduler has a target-first path; runtime delegate session entrypoints explicitly remain delivery adapters.
- Tests cover address-only command target construction and delegate adapter session ref semantics.

Task coordinate slice notes:

- `task::runtime_coordinate` centralizes task runtime coordinate/projection lookup.
- Task execution view and boot projection consume `run_id + agent_id + frame_id + orchestration_id + node_path + attempt`.
- Task hook terminal effects keep `runtime_session_id` as callback adapter, then validate the anchor against LifecycleRun runtime node coordinate before persisting artifact/status.
- Journey helpers now expose coordinate-first node lookup while session-id helpers remain trace-open helpers.

Hook target slice notes:

- `AgentFrameHookRuntimeTarget` carries `HookControlTarget` plus delivery runtime binding.
- `SessionHookService` resolves legacy frame/session adapters into hook target-first runtime construction and cache validation.
- `AgentFrameRuntimeTarget` remains available for capability/runtime transition adapters where only frame + delivery binding is known.

## Phase 4: Specs And Tests

- [x] Update `.trellis/spec/backend/session/architecture.md`.
- [x] Update `.trellis/spec/backend/workflow/architecture.md`.
- [x] Update `.trellis/spec/backend/session/runtime-execution-state.md`.
- [x] Add tests that non-message runtime surface APIs do not require `runtime_session_id`.
- [x] Keep tests that message stream APIs can still use `runtime_session_id`.

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
