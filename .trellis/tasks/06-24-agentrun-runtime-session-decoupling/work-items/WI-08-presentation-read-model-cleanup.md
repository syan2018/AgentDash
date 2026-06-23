# WI-08 Presentation And Read-Model Cleanup

Status: pending

Assigned Worker: unassigned

## Tracking

- Files changed: TBD.
- Tests run: TBD.
- Blockers: None recorded.
- Handoff summary: TBD.

## Purpose

Move presentation/current-frame read models behind application query facades so API routes do not assemble anchors/current frames directly.

## Dependencies

- `WI-02`

## Scope

- `routes/sessions.rs` runtime-control view.
- `routes/lifecycle_views.rs` session trace and AgentFrame runtime view.
- Any route-local current frame resolver use that is presentation/read-model only.

## Out Of Scope

- Runtime action/current surface consumers belong to `WI-05`.

## Deliverables

- Application read-model facade for RuntimeSession trace/control-plane view.
- API routes map facade DTOs only.

## Acceptance

- API presentation routes do not import current frame resolver directly.
- Existing response contract remains stable unless explicitly updated.
