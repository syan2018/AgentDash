# WI-01 AgentRun Target And Adoption Port Ownership

Status: pending

Assigned Worker: unassigned

## Tracking

- Files changed: TBD.
- Tests run: TBD.
- Blockers: None recorded.
- Handoff summary: TBD.

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
