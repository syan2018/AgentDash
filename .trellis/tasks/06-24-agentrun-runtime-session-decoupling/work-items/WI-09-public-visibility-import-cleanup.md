# WI-09 Public Visibility And Import Cleanup

Status: pending

Assigned Worker: unassigned

## Tracking

- Files changed: TBD.
- Tests run: TBD.
- Blockers: None recorded.
- Handoff summary: TBD.

## Purpose

Remove broad exports and old import paths after all consumer migrations complete.

## Dependencies

- `WI-03`
- `WI-04`
- `WI-05`
- `WI-06`
- `WI-07`
- `WI-08`

## Scope

- Tighten `agentdash-application/src/lib.rs` exports.
- Tighten `session/mod.rs`.
- Tighten `agent_run/frame/mod.rs`.
- Tighten `vfs/mod.rs`.
- Remove old compatibility shims or make them private.
- Run import hotspot checks and document remaining intentional exceptions.

## Deliverables

- Import graph suitable for future physical crate split.
- Updated `work-items/WI-09...` with before/after import summary.

## Acceptance

- Production forbidden imports are gone or documented as internal adapters.
- `cargo check -p agentdash-application`
- `cargo check -p agentdash-api`
- `cargo check -p agentdash-local`
- `cargo check -p agentdash-mcp`
