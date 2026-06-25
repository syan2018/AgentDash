# WI-02 AgentRun Current And Resource Surface Facades

Status: done

Assigned Worker: Codex WI-02

## Tracking

- Files changed:
  - `crates/agentdash-application/src/agent_run/runtime_surface.rs`
  - `crates/agentdash-application/src/agent_run/runtime_surface_update.rs`
  - `crates/agentdash-application/src/lifecycle/surface/surface_projector.rs`
  - `crates/agentdash-application/src/runtime_gateway/mcp_access.rs`
  - `crates/agentdash-api/src/app_state.rs`
  - `crates/agentdash-api/src/session_construction.rs`
  - `crates/agentdash-api/src/routes/vfs_surfaces/resolver.rs`
- Tests run:
  - `cargo fmt -p agentdash-application -p agentdash-api` — passed.
  - `cargo test -p agentdash-application agent_run::runtime_surface` — passed, 8 tests.
  - `cargo check -p agentdash-application` — passed.
  - `cargo check -p agentdash-api` — passed. Warning: `ApiCurrentRuntimeSurface` has fields not yet read by current API consumers; those fields preserve the explicit current-surface DTO contract for upcoming route migrations.
- Blockers: None.
- Handoff summary: Current surface DTO now exposes `launch_evidence_frame_id` and `current_surface_frame_id` separately. Added `AgentRunResourceSurfaceQuery` facade backed by the current runtime surface port plus lifecycle surface projector, with runtime-session and AgentRun-address entrypoints. API resource VFS resolution now calls the application facade and only performs session existence, project permission, DTO, and error mapping; route-local AgentRun run/agent/anchor/projector assembly was removed.

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
