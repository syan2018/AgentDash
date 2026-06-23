# WI-00 Baseline Import And Contract Inventory

Status: pending

Assigned Worker: unassigned

## Tracking

- Files changed: TBD.
- Tests run: TBD.
- Blockers: None recorded.
- Handoff summary: TBD.

## Purpose

Capture the pre-change dependency graph so later cleanup can prove real convergence rather than local refactors.

## Dependencies

- None.

## Scope

- Run import hotspot searches for `session`, `agent_run`, `lifecycle`, `runtime_gateway`, `vfs`, and API current-surface paths.
- Record existing exceptions and test-only imports separately from production imports.
- Confirm current public exports from `session/mod.rs`, `agent_run/mod.rs`, `agent_run/frame/mod.rs`, `vfs/mod.rs`, and `agentdash-application/src/lib.rs`.

## Deliverables

- `work-items/WI-00-baseline-import-contract-inventory.md` updated with baseline command output summary.
- Baseline forbidden import list used by `WI-09` and `WI-10`.

## Suggested Commands

```powershell
rg -n "crate::session::|agentdash_application::session::" crates/agentdash-application/src/agent_run crates/agentdash-application/src/lifecycle crates/agentdash-api/src
rg -n "AgentFrameRuntimeTarget|AgentFrameBuilder|AgentFrameSurfaceExt|resolve_current_frame_from_delivery_trace_ref" crates/agentdash-application/src crates/agentdash-api/src
rg -n "^(pub mod|pub use|pub\\(crate\\) use)" crates/agentdash-application/src/session/mod.rs crates/agentdash-application/src/agent_run/mod.rs crates/agentdash-application/src/agent_run/frame/mod.rs crates/agentdash-application/src/vfs/mod.rs crates/agentdash-application/src/lib.rs
```

## Acceptance

- Baseline distinguishes production and test-only imports.
- All later work items can reference the same forbidden import list.
