# WI-01 AgentRun Target And Adoption Port Ownership

Status: done

Assigned Worker: Codex

## Tracking

- Files changed:
  - `crates/agentdash-application/src/agent_run/runtime_target.rs`
  - `crates/agentdash-application/src/agent_run/mod.rs`
  - `crates/agentdash-application/src/agent_run/effective_capability.rs`
  - `crates/agentdash-application/src/agent_run/frame/surface_service.rs`
  - `crates/agentdash-application/src/agent_run/runtime_surface_update.rs`
  - `crates/agentdash-application/src/companion/tools.rs`
  - `crates/agentdash-application/src/permission/runtime_surface_update.rs`
  - `crates/agentdash-application/src/permission/service.rs`
  - `crates/agentdash-application/src/workspace_module/visibility.rs`
  - `crates/agentdash-application/src/workspace_module/tools.rs`
  - `crates/agentdash-application/src/session/hooks_service.rs`
  - `crates/agentdash-application/src/session/hub/tool_builder.rs`
  - `crates/agentdash-application/src/session/hub/tests.rs`
  - `crates/agentdash-application/src/session/launch/commit.rs`
  - `crates/agentdash-application/src/session/mod.rs`
  - `crates/agentdash-application/src/session/types.rs`
  - `.trellis/tasks/06-24-agentrun-runtime-session-decoupling/work-items/WI-01-agentrun-target-adoption-port.md`
- Commands/tests:
  - `cargo fmt -p agentdash-application`
  - `rg -n "session::AgentFrameRuntimeTarget|agentdash_application::session::AgentFrameRuntimeTarget" crates/agentdash-application/src crates/agentdash-api/src` -> no matches.
  - `cargo check -p agentdash-application` -> passed.
  - Integration rerun after WI-02 merge: `cargo test -p agentdash-application agent_run::runtime_surface` -> passed.
- Blockers: None.
- Handoff summary: `AgentFrameRuntimeTarget` and `AgentFrameHookRuntimeTarget` are now AgentRun-owned in `agent_run/runtime_target.rs` and exported from `agent_run`. Session no longer owns or re-exports these target types; SessionHub/RuntimeSession live adoption implements the AgentRun-owned `AgentRunActiveRuntimeSurfaceAdopter` port. AgentRun, companion, permission, workspace module, hook service, launch commit, and related tests now import the target from `crate::agent_run`. Test-only Arc ownership regressions from the target migration were fixed by cloning the lifecycle agent repository before injecting it into the session hub factory.

## Purpose

Move AgentFrame runtime target and live surface adoption contract to AgentRun ownership. RuntimeSession implements the adapter, but business modules and API do not see SessionHub.

## Dependencies

- `WI-00`

## Scope

- Re-own `AgentFrameRuntimeTarget` and related hook/runtime target types under AgentRun or an AgentRun-facing port module.
- Define narrow AgentRun live adoption port.
- Update SessionHub implementation to implement the AgentRun-owned port.
- Update call sites in Permission, WorkspaceModule, Canvas, Companion, Hooks and tests.

## Out Of Scope

- Do not change physical crate layout.
- Do not change adoption behavior except ownership and imports.

## Deliverables

- Production imports no longer use `session::AgentFrameRuntimeTarget`.
- SessionHub live adoption remains an internal adapter.
- Tracking doc updated with changed files and tests.

## Acceptance

- `rg -n "session::AgentFrameRuntimeTarget|agentdash_application::session::AgentFrameRuntimeTarget" crates/agentdash-application/src crates/agentdash-api/src` has no production call sites.
- `cargo check -p agentdash-application` passes.
