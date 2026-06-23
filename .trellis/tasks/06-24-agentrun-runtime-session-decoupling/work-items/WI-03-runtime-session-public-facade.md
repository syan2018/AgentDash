# WI-03 RuntimeSession Public Facade

Status: pending

Assigned Worker: unassigned

## Tracking

- Files changed: TBD.
- Tests run: TBD.
- Blockers: None recorded.
- Handoff summary: TBD.

## Purpose

Reduce `session` to RuntimeSession delivery/trace substrate at the public facade level.

## Dependencies

- `WI-01`

## Scope

- Tighten `session/mod.rs` exports.
- Remove re-exports of AgentRun/Lifecycle ownership types.
- Make hub/tool/surface helper modules private or crate-private where possible.
- Keep public only RuntimeSession substrate use cases: core/eventing/control/runtime delivery/launch substrate/persistence/projection/terminal/tool result where needed.

## Out Of Scope

- Do not move launch/commit write ownership in this item; `WI-07` owns that behavior.

## Deliverables

- Updated `session` public facade.
- Import fixes for downstream modules.
- Documented allowed RuntimeSession public API list.

## Acceptance

- `session/mod.rs` does not public re-export AgentRun/Lifecycle ownership types.
- `cargo check -p agentdash-application` passes.
