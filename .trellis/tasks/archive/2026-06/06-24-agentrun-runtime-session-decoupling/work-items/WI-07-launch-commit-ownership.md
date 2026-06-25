# WI-07 Launch And Commit Ownership Split

Status: done

Assigned Worker: codex-agent

## Tracking

- Files changed:
  - `crates/agentdash-application/src/agent_run/frame/launch_commit.rs`
  - `crates/agentdash-application/src/agent_run/frame/mod.rs`
  - `crates/agentdash-application/src/agent_run/frame/surface_service.rs`
  - `crates/agentdash-application/src/agent_run/mod.rs`
  - `crates/agentdash-application/src/session/hooks_service.rs`
  - `crates/agentdash-application/src/session/launch/commit.rs`
  - `crates/agentdash-application/src/session/launch/deps.rs`
  - `crates/agentdash-application/src/session/launch/orchestrator.rs`
- Tests run:
  - `cargo check -p agentdash-application`
  - `cargo test -p agentdash-application launch_commit`
  - `cargo test -p agentdash-application runtime_command_apply_commit_failure_marks_failed_and_returns_error`
  - `cargo test -p agentdash-application frame::surface_service`
  - `rg -n "AgentFrameBuilder|resolve_current_frame_from_delivery_trace_ref|bind_current_delivery_from_anchor|DeliveryBindingStatus|AgentFrameRuntimeTarget" crates/agentdash-application/src/session/launch -g "*.rs"` returned no matches.
  - `git diff --check` passed.
- Blockers: 无。
- Handoff summary: AgentRun/Lifecycle accepted launch commit adapter now owns accepted AgentFrame revision persistence, LifecycleAgent current delivery binding, hook runtime target sync timing, and bootstrap status query/marking. RuntimeSession launch commit keeps accepted delivery events, meta updates, runtime command applied commit, title derivation, and stream ingestion, then consumes the adapter outcome for control-plane side effects.

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
