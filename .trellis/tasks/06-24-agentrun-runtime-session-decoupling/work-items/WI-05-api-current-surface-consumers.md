# WI-05 API, VFS And Terminal Current Surface Consumers

Status: pending

Assigned Worker: unassigned

## Tracking

- Files changed: TBD.
- Tests run: TBD.
- Blockers: None recorded.
- Handoff summary: TBD.

## Purpose

Migrate API current-surface, VFS and Terminal consumers to AgentRun facades so routes stop assembling current/resource surface from session construction helpers or route-local anchor selection.

## Dependencies

- `WI-02`

## Scope

- Rename or move `agentdash-api/src/session_construction.rs` to an AgentRun runtime surface adapter.
- Terminal launch target derivation consumes application runtime placement facade.
- VFS `SessionRuntime` / `AgentRun` sources consume AgentRun resource surface facade.

## Out Of Scope

- `routes/sessions.rs` and `routes/lifecycle_views.rs` presentation read-model cleanup belongs to `WI-08`.
- Canvas/Extension Project/session binding guards belong to `WI-11`.

## Deliverables

- Route code performs auth/DTO/error mapping only.
- Terminal/VFS current surface tests updated.

## Acceptance

- `cargo check -p agentdash-api` passes.
- VFS AgentRun latest-anchor selection is no longer route-owned.
