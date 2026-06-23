# WI-02 AgentRun Current And Resource Surface Facades

Status: pending

Assigned Worker: unassigned

## Tracking

- Files changed: TBD.
- Tests run: TBD.
- Blockers: None recorded.
- Handoff summary: TBD.

## Purpose

Stabilize the AgentRun current surface and resource surface facades so all consumers use one closed surface closure.

## Dependencies

- `WI-00`

## Scope

- Stabilize `AgentRunRuntimeSurfaceQueryPort` and DTO naming.
- Ensure DTOs preserve both launch frame id and current surface frame id.
- Add `AgentRunResourceSurfaceQuery` or equivalent facade around `AgentRunLifecycleSurfaceProjector`.
- Move API-side resource VFS projector assembly into application facade.

## Out Of Scope

- Do not migrate every API route in this item; `WI-05` owns route migration.

## Deliverables

- Current surface DTO with unambiguous frame fields.
- Resource surface facade returning VFS/resource projection from runtime session or AgentRun address.
- Tests for current surface query and resource surface projection.

## Acceptance

- `cargo test -p agentdash-application agent_run::runtime_surface` passes.
- API consumers can call a facade instead of assembling projector/repositories.
