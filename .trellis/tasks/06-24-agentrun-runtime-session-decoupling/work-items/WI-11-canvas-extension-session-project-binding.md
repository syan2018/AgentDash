# WI-11 Canvas And Extension Session Project Binding

Status: pending

Assigned Worker: unassigned

## Tracking

- Files changed: TBD.
- Tests run: TBD.
- Blockers: None recorded.
- Handoff summary: TBD.

## Purpose

Add explicit Canvas and Extension runtime route validation that the path Project or Canvas Project matches the current runtime session surface Project before RuntimeGateway or provider invocation.

## Dependencies

- `WI-02`

## Scope

- Canvas runtime invoke validates the Canvas Project against the current runtime session surface Project.
- Canvas runtime bridge manifest validates the Canvas Project against the current runtime session surface Project.
- Extension runtime action validates the path Project against the current runtime session surface Project before installation lookup and Gateway invocation.
- Extension runtime channel validates the path Project against the current runtime session surface Project before channel provider invocation.
- Reuse the AgentRun current surface facade; do not introduce SessionHub/current-frame fallback checks.

## Out Of Scope

- Do not move generic API current-surface adapters; `WI-05` owns API/VFS/Terminal migration.
- Do not change RuntimeGateway actor/context admission semantics except through prevalidated route/application input.

## Deliverables

- Canvas mismatch rejection tests.
- Extension mismatch rejection tests.
- Route or application facade path that returns a prevalidated runtime invocation context.

## Acceptance

- Mismatched Canvas Project / runtime session Project is rejected before Gateway invocation.
- Mismatched path Project / runtime session Project is rejected before Extension provider invocation.
- Valid Canvas and Extension paths still use AgentRun current surface DTOs for backend/runtime placement.
