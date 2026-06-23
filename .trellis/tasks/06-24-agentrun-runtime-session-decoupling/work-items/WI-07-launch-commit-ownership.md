# WI-07 Launch And Commit Ownership Split

Status: pending

Assigned Worker: unassigned

## Tracking

- Files changed: TBD.
- Tests run: TBD.
- Blockers: None recorded.
- Handoff summary: TBD.

## Purpose

Split RuntimeSession connector delivery commit from AgentRun/Lifecycle control-plane writes.

## Dependencies

- `WI-01`
- `WI-03`

## Scope

- Move AgentFrame revision writes out of `session/launch/commit.rs`.
- Move LifecycleAgent current delivery binding out of session commit.
- Move bootstrap status decision out of `session/launch/orchestrator.rs`.
- Keep RuntimeSession accepted turn, trace commit, connector attach and stream processing in session.

## Deliverables

- AgentRun/Lifecycle launch/commit adapter owns durable control-plane writes.
- RuntimeSession launch pipeline consumes final envelope and records delivery facts.
- Launch/commit tests updated.

## Acceptance

- Session launch code no longer directly owns AgentFrame/Lifecycle write decisions.
- Existing session launch behavior remains intact.
- `cargo check -p agentdash-application` passes.
